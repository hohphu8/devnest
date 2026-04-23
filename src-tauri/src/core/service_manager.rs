use crate::core::config_generator;
use crate::core::log_reader;
use crate::core::ports;
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::project::ServerType;
use crate::models::runtime::RuntimeType;
use crate::models::service::{ServiceName, ServiceState, ServiceStatus};
use crate::models::tunnel::TunnelStatus;
use crate::state::{AppState, ManagedServiceProcess};
use crate::storage::repositories::{
    OptionalToolVersionRepository, ProjectPersistentHostnameRepository, ProjectRepository,
    RuntimeVersionRepository, ServiceRepository,
};
use crate::utils::process::{configure_background_command, is_process_running, kill_process_tree};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

struct SyncResult {
    running_pid: Option<u32>,
    exited: bool,
    exit_success: bool,
    exit_message: Option<String>,
}

fn mutex_error() -> AppError {
    AppError::new_validation(
        "SERVICE_STATE_LOCK_FAILED",
        "Could not access the in-memory service state cache.",
    )
}

fn status_message(service: &ServiceName, exit_status: &ExitStatus) -> Option<String> {
    if exit_status.success() {
        return None;
    }

    let detail = exit_status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "an unknown exit status".to_string());

    Some(format!(
        "{} stopped unexpectedly with {}.",
        service.display_name(),
        detail
    ))
}

fn sync_tracked_process(state: &AppState, service: &ServiceName) -> Result<SyncResult, AppError> {
    let mut processes = state.managed_processes.lock().map_err(|_| mutex_error())?;
    let key = service.as_str().to_string();
    let mut running_pid = None;
    let mut exited = false;
    let mut exit_success = false;
    let mut exit_message = None;

    if let Some(process) = processes.get_mut(&key) {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                exited = true;
                exit_success = status.success();
                exit_message = status_message(service, &status);
            }
            Ok(None) => {
                running_pid = Some(process.pid);
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "SERVICE_STATUS_FAILED",
                    format!(
                        "Could not inspect the {} process state.",
                        service.display_name()
                    ),
                    error.to_string(),
                ));
            }
        }
    }

    if exited {
        processes.remove(&key);
    }

    Ok(SyncResult {
        running_pid,
        exited,
        exit_success,
        exit_message,
    })
}

fn save_running_state(
    connection: &Connection,
    service: &ServiceName,
    pid: u32,
    port: Option<u16>,
) -> Result<ServiceState, AppError> {
    ServiceRepository::save_state(
        connection,
        service,
        &ServiceStatus::Running,
        Some(i64::from(pid)),
        port.map(i64::from),
        None,
    )
}

fn save_stopped_state(
    connection: &Connection,
    service: &ServiceName,
    port: Option<u16>,
) -> Result<ServiceState, AppError> {
    ServiceRepository::save_state(
        connection,
        service,
        &ServiceStatus::Stopped,
        None,
        port.map(i64::from),
        None,
    )
}

fn save_error_state(
    connection: &Connection,
    service: &ServiceName,
    port: Option<u16>,
    message: &str,
) -> Result<ServiceState, AppError> {
    ServiceRepository::save_state(
        connection,
        service,
        &ServiceStatus::Error,
        None,
        port.map(i64::from),
        Some(message),
    )
}

fn php_process_key(version: &str) -> String {
    format!("php-{version}")
}

fn is_php_fastcgi_process_name(process_name: Option<&str>) -> bool {
    process_name
        .map(|name| matches!(name.trim().to_ascii_lowercase().as_str(), "php-cgi" | "php"))
        .unwrap_or(false)
}

fn required_php_versions_for_service(
    connection: &Connection,
    service: &ServiceName,
) -> Result<Vec<String>, AppError> {
    let server_type = match service {
        ServiceName::Apache => Some(ServerType::Apache),
        ServiceName::Nginx => Some(ServerType::Nginx),
        ServiceName::Mysql | ServiceName::Mailpit | ServiceName::Redis => None,
    };

    let Some(server_type) = server_type else {
        return Ok(Vec::new());
    };

    let projects = ProjectRepository::list(connection)?;
    let mut versions = BTreeSet::new();

    for project in projects {
        let matches_server = matches!(
            (&project.server_type, &server_type),
            (ServerType::Apache, ServerType::Apache) | (ServerType::Nginx, ServerType::Nginx)
        );
        if matches_server {
            versions.insert(project.php_version);
        }
    }

    if OptionalToolVersionRepository::find_active_by_type(
        connection,
        &crate::models::optional_tool::OptionalToolType::Phpmyadmin,
    )?
    .is_some()
    {
        versions.insert(preferred_php_version_for_optional_web_tool(connection)?);
    }

    Ok(versions.into_iter().collect())
}

fn public_tunnel_host(public_url: &str) -> Option<String> {
    public_url
        .trim()
        .strip_prefix("https://")
        .or_else(|| public_url.trim().strip_prefix("http://"))
        .map(|value| {
            value
                .split('/')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        })
        .filter(|value| !value.is_empty())
}

fn public_host_aliases_for_project(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Vec<String>, AppError> {
    let project_tunnels = state.project_tunnels.lock().map_err(|_| mutex_error())?;
    let mut aliases = project_tunnels
        .get(project_id)
        .filter(|tunnel| {
            matches!(
                tunnel.status,
                TunnelStatus::Starting | TunnelStatus::Running
            )
        })
        .and_then(|tunnel| tunnel.public_url.as_deref())
        .and_then(public_tunnel_host)
        .map(|host| vec![host])
        .unwrap_or_default();

    drop(project_tunnels);

    if let Some(persistent_hostname) =
        ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?
    {
        if !aliases
            .iter()
            .any(|alias| alias == &persistent_hostname.hostname)
        {
            aliases.push(persistent_hostname.hostname);
        }
    }

    Ok(aliases)
}

pub(crate) fn sync_managed_configs_for_service(
    connection: &Connection,
    state: &AppState,
    service: &ServiceName,
) -> Result<(), AppError> {
    let server_type = match service {
        ServiceName::Apache => Some(ServerType::Apache),
        ServiceName::Nginx => Some(ServerType::Nginx),
        ServiceName::Mysql | ServiceName::Mailpit | ServiceName::Redis => None,
    };

    let Some(server_type) = server_type else {
        return Ok(());
    };

    for project in ProjectRepository::list(connection)? {
        let matches_server = matches!(
            (&project.server_type, &server_type),
            (ServerType::Apache, ServerType::Apache) | (ServerType::Nginx, ServerType::Nginx)
        );
        if matches_server {
            let aliases = public_host_aliases_for_project(connection, state, &project.id)?;
            config_generator::generate_config_with_aliases(
                &project,
                &state.workspace_dir,
                &aliases,
            )?;
        }
    }

    sync_phpmyadmin_config_for_service(connection, state, &server_type)?;

    Ok(())
}

fn preferred_php_version_for_optional_web_tool(
    connection: &Connection,
) -> Result<String, AppError> {
    if let Some(runtime) =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Php)?
    {
        return Ok(runtime.version);
    }

    Ok(
        RuntimeVersionRepository::list_by_type(connection, &RuntimeType::Php)?
            .into_iter()
            .max_by_key(|runtime| (runtime.is_active, runtime.updated_at.clone()))
            .map(|runtime| runtime.version)
            .unwrap_or_else(|| "8.2".to_string()),
    )
}

fn sync_phpmyadmin_config_for_service(
    connection: &Connection,
    state: &AppState,
    server_type: &ServerType,
) -> Result<(), AppError> {
    let phpmyadmin = OptionalToolVersionRepository::find_active_by_type(
        connection,
        &crate::models::optional_tool::OptionalToolType::Phpmyadmin,
    )?;
    let Some(tool) = phpmyadmin else {
        config_generator::remove_managed_config(
            &state.workspace_dir,
            server_type,
            config_generator::PHPMYADMIN_DOMAIN,
        )?;
        return Ok(());
    };

    let entry_path = std::path::PathBuf::from(tool.path);
    let Some(install_root) = entry_path.parent() else {
        config_generator::remove_managed_config(
            &state.workspace_dir,
            server_type,
            config_generator::PHPMYADMIN_DOMAIN,
        )?;
        return Ok(());
    };
    if !install_root.exists() || !install_root.is_dir() {
        config_generator::remove_managed_config(
            &state.workspace_dir,
            server_type,
            config_generator::PHPMYADMIN_DOMAIN,
        )?;
        return Ok(());
    }

    let php_version = preferred_php_version_for_optional_web_tool(connection)?;
    config_generator::generate_phpmyadmin_config(
        &state.workspace_dir,
        install_root,
        server_type,
        &php_version,
    )?;
    Ok(())
}

fn start_php_fastcgi_process(
    state: &AppState,
    version: &str,
    runtime: runtime_registry::RuntimeCommand,
) -> Result<(), AppError> {
    if let Some(port) = runtime.port {
        let mut port_check = ports::check_port(port)?;
        if !port_check.available && is_php_fastcgi_process_name(port_check.process_name.as_deref())
        {
            if let Some(pid) = port_check.pid {
                kill_process_tree(pid)?;
                thread::sleep(Duration::from_millis(250));
                port_check = ports::check_port(port)?;
            }
        }

        if !port_check.available {
            return Err(AppError::with_details(
                "PHP_FASTCGI_PORT_IN_USE",
                format!(
                    "PHP {version} could not start because FastCGI port {port} is already in use."
                ),
                format!(
                    "pid={:?}, processName={:?}",
                    port_check.pid, port_check.process_name
                ),
            ));
        }
    }

    if let Some(parent) = runtime.log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "PHP_FASTCGI_START_FAILED",
                format!("Could not create the PHP {version} log directory."),
                error.to_string(),
            )
        })?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&runtime.log_path)
        .map_err(|error| {
            AppError::with_details(
                "PHP_FASTCGI_START_FAILED",
                format!("Could not open the PHP {version} log file."),
                error.to_string(),
            )
        })?;
    let stderr = stdout.try_clone().map_err(|error| {
        AppError::with_details(
            "PHP_FASTCGI_START_FAILED",
            format!("Could not duplicate the PHP {version} log stream."),
            error.to_string(),
        )
    })?;

    let mut command = Command::new(&runtime.binary_path);
    command.args(&runtime.args);
    configure_background_command(&mut command);
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    command.stdin(Stdio::null());
    if let Some(current_dir) = &runtime.working_dir {
        command.current_dir(current_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        AppError::with_details(
            "PHP_FASTCGI_START_FAILED",
            format!("Could not start PHP {version} FastCGI."),
            error.to_string(),
        )
    })?;

    thread::sleep(Duration::from_millis(250));
    match child.try_wait() {
        Ok(Some(status)) => {
            let log_tail = log_reader::read_tail(&runtime.log_path, 20)?;
            return Err(AppError::with_details(
                "PHP_FASTCGI_START_FAILED",
                format!("PHP {version} FastCGI exited immediately with {status}."),
                if log_tail.is_empty() {
                    "No log output was captured.".to_string()
                } else {
                    log_tail
                },
            ));
        }
        Ok(None) => {}
        Err(error) => {
            return Err(AppError::with_details(
                "PHP_FASTCGI_START_FAILED",
                format!("Could not confirm that PHP {version} FastCGI is running."),
                error.to_string(),
            ));
        }
    }

    let key = php_process_key(version);
    state
        .managed_processes
        .lock()
        .map_err(|_| mutex_error())?
        .insert(
            key,
            ManagedServiceProcess {
                pid: child.id(),
                child,
                log_path: runtime.log_path,
            },
        );

    Ok(())
}

fn ensure_php_fastcgi_processes(
    connection: &Connection,
    state: &AppState,
    service: &ServiceName,
) -> Result<(), AppError> {
    for version in required_php_versions_for_service(connection, service)? {
        let key = php_process_key(&version);
        let is_running = {
            let mut processes = state.managed_processes.lock().map_err(|_| mutex_error())?;
            let status = if let Some(process) = processes.get_mut(&key) {
                match process.child.try_wait() {
                    Ok(Some(_)) => Some(false),
                    Ok(None) => Some(true),
                    Err(_) => Some(false),
                }
            } else {
                None
            };

            if matches!(status, Some(false)) {
                processes.remove(&key);
            }

            status.unwrap_or(false)
        };

        if is_running {
            continue;
        }

        let runtime = runtime_registry::resolve_php_fastcgi_runtime(
            connection,
            &state.workspace_dir,
            &version,
        )?;
        start_php_fastcgi_process(state, &version, runtime)?;
    }

    Ok(())
}

fn stop_php_fastcgi_processes(state: &AppState) -> Result<(), AppError> {
    let keys = {
        let processes = state.managed_processes.lock().map_err(|_| mutex_error())?;
        processes
            .keys()
            .filter(|key| key.starts_with("php-"))
            .cloned()
            .collect::<Vec<_>>()
    };

    if keys.is_empty() {
        return Ok(());
    }

    let mut processes = state.managed_processes.lock().map_err(|_| mutex_error())?;
    for key in keys {
        if let Some(mut process) = processes.remove(&key) {
            match process.child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    process.child.kill().map_err(|error| {
                        AppError::with_details(
                            "PHP_FASTCGI_STOP_FAILED",
                            "Could not stop a managed PHP FastCGI process.",
                            error.to_string(),
                        )
                    })?;
                    let _ = process.child.wait();
                }
                Err(error) => {
                    return Err(AppError::with_details(
                        "PHP_FASTCGI_STOP_FAILED",
                        "Could not inspect a managed PHP FastCGI process before stopping it.",
                        error.to_string(),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn guard_service_ports(service: &ServiceName, primary_port: Option<u16>) -> Result<(), AppError> {
    if let Some(port) = primary_port {
        let port_check = ports::check_port(port)?;
        if !port_check.available {
            return Err(AppError::with_details(
                "PORT_IN_USE",
                format!(
                    "Port {} is already in use by {}. Stop the conflicting process before starting {}.",
                    port,
                    port_check
                        .process_name
                        .as_deref()
                        .unwrap_or("another process"),
                    service.display_name()
                ),
                format!(
                    "pid={:?}, processName={:?}",
                    port_check.pid, port_check.process_name
                ),
            ));
        }
    }

    if matches!(service, ServiceName::Mailpit) {
        let smtp_port = runtime_registry::mailpit_smtp_port()?;
        let port_check = ports::check_port(smtp_port)?;
        if !port_check.available {
            return Err(AppError::with_details(
                "PORT_IN_USE",
                format!(
                    "Mailpit SMTP port {} is already in use by {}. Stop the conflicting process before starting Mailpit.",
                    smtp_port,
                    port_check
                        .process_name
                        .as_deref()
                        .unwrap_or("another process"),
                ),
                format!(
                    "pid={:?}, processName={:?}",
                    port_check.pid, port_check.process_name
                ),
            ));
        }
    }

    Ok(())
}

pub fn get_service_status(
    connection: &Connection,
    state: &AppState,
    service: ServiceName,
) -> Result<ServiceState, AppError> {
    let current = ServiceRepository::get(connection, service.as_str())?;
    let expected_port = current
        .port
        .or_else(|| service.default_port().map(i64::from));

    let sync_result = sync_tracked_process(state, &service)?;
    if let Some(pid) = sync_result.running_pid {
        return save_running_state(
            connection,
            &service,
            pid,
            expected_port.map(|value| value as u16),
        );
    }

    if sync_result.exited {
        if sync_result.exit_success {
            return save_stopped_state(
                connection,
                &service,
                expected_port.map(|value| value as u16),
            );
        }

        return save_error_state(
            connection,
            &service,
            expected_port.map(|value| value as u16),
            sync_result
                .exit_message
                .as_deref()
                .unwrap_or("The service stopped unexpectedly."),
        );
    }

    if let Some(pid) = current.pid {
        if is_process_running(pid as u32)? {
            return save_running_state(
                connection,
                &service,
                pid as u32,
                expected_port.map(|value| value as u16),
            );
        }

        return save_stopped_state(
            connection,
            &service,
            expected_port.map(|value| value as u16),
        );
    }

    if current.port.is_none() {
        return ServiceRepository::save_state(
            connection,
            &service,
            &current.status,
            None,
            expected_port,
            current.last_error.as_deref(),
        );
    }

    Ok(current)
}

pub fn get_all_service_status(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<ServiceState>, AppError> {
    let services = ServiceRepository::list(connection)?;
    let mut synchronized = Vec::with_capacity(services.len());

    for service in services {
        synchronized.push(get_service_status(connection, state, service.name)?);
    }

    Ok(synchronized)
}

pub fn start_service(
    connection: &Connection,
    state: &AppState,
    service: ServiceName,
) -> Result<ServiceState, AppError> {
    let current = get_service_status(connection, state, service.clone())?;
    if matches!(current.status, ServiceStatus::Running) && current.pid.is_some() {
        return Ok(current);
    }

    let runtime = runtime_registry::resolve_service_runtime(
        connection,
        &state.workspace_dir,
        service.clone(),
    )?;
    guard_service_ports(&service, runtime.port)?;

    if matches!(service, ServiceName::Apache | ServiceName::Nginx) {
        sync_managed_configs_for_service(connection, state, &service)?;
        ensure_php_fastcgi_processes(connection, state, &service)?;
    }

    if let Some(parent) = runtime.log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "SERVICE_START_FAILED",
                format!(
                    "Could not create the log directory for {}.",
                    service.display_name()
                ),
                error.to_string(),
            )
        })?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&runtime.log_path)
        .map_err(|error| {
            AppError::with_details(
                "SERVICE_START_FAILED",
                format!(
                    "Could not open the log file for {}.",
                    service.display_name()
                ),
                error.to_string(),
            )
        })?;
    let stderr = stdout.try_clone().map_err(|error| {
        AppError::with_details(
            "SERVICE_START_FAILED",
            format!(
                "Could not duplicate the log stream for {}.",
                service.display_name()
            ),
            error.to_string(),
        )
    })?;

    let mut command = Command::new(&runtime.binary_path);
    command.args(&runtime.args);
    configure_background_command(&mut command);
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    command.stdin(Stdio::null());
    if let Some(current_dir) = &runtime.working_dir {
        command.current_dir(current_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        if matches!(service, ServiceName::Apache | ServiceName::Nginx) {
            let _ = stop_php_fastcgi_processes(state);
        }

        AppError::with_details(
            "SERVICE_START_FAILED",
            format!("Could not start {}.", service.display_name()),
            error.to_string(),
        )
    })?;

    thread::sleep(Duration::from_millis(250));
    match child.try_wait() {
        Ok(Some(status)) => {
            let log_tail = log_reader::read_tail(&runtime.log_path, 20)?;
            let error_message = status_message(&service, &status).unwrap_or_else(|| {
                format!(
                    "{} exited immediately after launch.",
                    service.display_name()
                )
            });
            let _ = save_error_state(connection, &service, runtime.port, &error_message);
            if matches!(service, ServiceName::Apache | ServiceName::Nginx) {
                let _ = stop_php_fastcgi_processes(state);
            }

            return Err(AppError::with_details(
                "SERVICE_START_FAILED",
                error_message,
                if log_tail.is_empty() {
                    "No log output was captured.".to_string()
                } else {
                    log_tail
                },
            ));
        }
        Ok(None) => {}
        Err(error) => {
            let _ = save_error_state(
                connection,
                &service,
                runtime.port,
                "The service started but its process state could not be verified.",
            );
            if matches!(service, ServiceName::Apache | ServiceName::Nginx) {
                let _ = stop_php_fastcgi_processes(state);
            }

            return Err(AppError::with_details(
                "SERVICE_START_FAILED",
                format!(
                    "Could not confirm that {} is running.",
                    service.display_name()
                ),
                error.to_string(),
            ));
        }
    }

    let pid = child.id();
    state
        .managed_processes
        .lock()
        .map_err(|_| mutex_error())?
        .insert(
            service.as_str().to_string(),
            ManagedServiceProcess {
                pid,
                child,
                log_path: runtime.log_path,
            },
        );

    save_running_state(connection, &service, pid, runtime.port)
}

pub fn stop_service(
    connection: &Connection,
    state: &AppState,
    service: ServiceName,
) -> Result<ServiceState, AppError> {
    let current = get_service_status(connection, state, service.clone())?;
    let expected_port = current
        .port
        .or_else(|| service.default_port().map(i64::from))
        .map(|value| value as u16);

    let tracked = state
        .managed_processes
        .lock()
        .map_err(|_| mutex_error())?
        .remove(service.as_str());

    if let Some(mut process) = tracked {
        match process.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                process.child.kill().map_err(|error| {
                    AppError::with_details(
                        "SERVICE_STOP_FAILED",
                        format!("Could not stop {}.", service.display_name()),
                        error.to_string(),
                    )
                })?;
                let _ = process.child.wait();
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "SERVICE_STOP_FAILED",
                    format!(
                        "Could not inspect the {} process before stopping it.",
                        service.display_name()
                    ),
                    error.to_string(),
                ));
            }
        }

        return save_stopped_state(connection, &service, expected_port);
    }

    if let Some(pid) = current.pid {
        if is_process_running(pid as u32)? {
            kill_process_tree(pid as u32)?;
        }
    }

    if matches!(service, ServiceName::Apache | ServiceName::Nginx) {
        stop_php_fastcgi_processes(state)?;
    }

    save_stopped_state(connection, &service, expected_port)
}

pub fn restart_service(
    connection: &Connection,
    state: &AppState,
    service: ServiceName,
) -> Result<ServiceState, AppError> {
    let _ = stop_service(connection, state, service.clone())?;
    start_service(connection, state, service)
}

pub fn read_service_logs(
    state: &AppState,
    service: ServiceName,
    lines: usize,
) -> Result<log_reader::ServiceLogPayload, AppError> {
    let log_path = resolve_service_log_path(state, &service)?;
    log_reader::read_tail_payload(&log_path, service.as_str(), lines)
}

pub fn clear_service_logs(state: &AppState, service: ServiceName) -> Result<(), AppError> {
    let log_path = resolve_service_log_path(state, &service)?;

    log_reader::clear(&log_path)
}

pub fn resolve_service_log_path(
    state: &AppState,
    service: &ServiceName,
) -> Result<std::path::PathBuf, AppError> {
    Ok(
        if let Some(process) = state
            .managed_processes
            .lock()
            .map_err(|_| mutex_error())?
            .get(service.as_str())
        {
            process.log_path.clone()
        } else {
            runtime_registry::service_log_path(&state.workspace_dir, service)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        clear_service_logs, get_service_status, is_php_fastcgi_process_name,
        public_host_aliases_for_project, read_service_logs, start_service, stop_service,
        sync_managed_configs_for_service,
    };
    use crate::models::persistent_tunnel::PersistentTunnelProvider;
    use crate::models::project::{CreateProjectInput, FrameworkType, ServerType};
    use crate::models::service::{ServiceName, ServiceStatus};
    use crate::models::tunnel::{ProjectTunnelState, TunnelProvider, TunnelStatus};
    use crate::state::AppState;
    use crate::storage::db::init_database;
    use crate::storage::repositories::{ProjectPersistentHostnameRepository, ProjectRepository};
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
    use uuid::Uuid;

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn acquire_env_lock() -> MutexGuard<'static, ()> {
        env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // Service manager tests serialize environment mutations with acquire_env_lock.
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // Service manager tests serialize environment mutations with acquire_env_lock.
        unsafe { std::env::remove_var(key) }
    }

    fn setup_state() -> (std::path::PathBuf, std::path::PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("devnest-service-manager-{}", Uuid::new_v4()));
        let workspace_dir = root.join("workspace");
        let db_path = workspace_dir.join("devnest.sqlite3");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        init_database(&db_path).expect("database should initialize");
        let connection = Connection::open(&db_path).expect("db should open");
        let state = AppState {
            db_path,
            workspace_dir,
            resources_dir: root.join("resources"),
            started_at: "2026-04-17T00:00:00Z".to_string(),
            allow_exit: Mutex::new(false),
            managed_processes: Mutex::new(HashMap::new()),
            managed_worker_processes: Mutex::new(HashMap::new()),
            managed_scheduled_task_runs: Arc::new(Mutex::new(HashMap::new())),
            scheduled_task_scheduler_shutdown: Arc::new(AtomicBool::new(false)),
            runtime_install_task: Mutex::new(None),
            optional_tool_install_task: Mutex::new(None),
            project_tunnels: Mutex::new(HashMap::new()),
            project_persistent_tunnels: Mutex::new(HashMap::new()),
            project_mobile_previews: Mutex::new(HashMap::new()),
        };

        (root, state.workspace_dir.clone(), state, connection)
    }

    fn make_project_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("devnest-service-project-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("public")).expect("project root should exist");
        root
    }

    #[test]
    fn starts_stops_and_reads_logs_for_configured_service() {
        let _guard = acquire_env_lock();
        let (root, workspace_dir, state, connection) = setup_state();
        let script_path = root.join("apache-loop.ps1");
        let log_path = workspace_dir.join("runtime-logs").join("apache-test.log");

        fs::write(
            &script_path,
            "[Console]::Out.WriteLine('apache boot'); [Console]::Out.Flush(); while ($true) { [Console]::Out.WriteLine('apache heartbeat'); [Console]::Out.Flush(); Start-Sleep -Milliseconds 200 }",
        )
        .expect("script should be written");

        set_env_var("DEVNEST_RUNTIME_APACHE_BIN", "powershell");
        set_env_var(
            "DEVNEST_RUNTIME_APACHE_ARGS",
            format!(
                "-NoProfile -ExecutionPolicy Bypass -File {}",
                script_path.to_string_lossy()
            ),
        );
        set_env_var("DEVNEST_RUNTIME_APACHE_PORT", "18080");
        set_env_var("DEVNEST_LOG_APACHE", &log_path);

        let started =
            start_service(&connection, &state, ServiceName::Apache).expect("service should start");
        assert_eq!(started.status, ServiceStatus::Running);
        assert!(started.pid.is_some());

        let stopped =
            stop_service(&connection, &state, ServiceName::Apache).expect("service should stop");
        assert_eq!(stopped.status, ServiceStatus::Stopped);

        let logs = read_service_logs(&state, ServiceName::Apache, 20)
            .expect("logs should remain readable after stop");
        assert_eq!(logs.name, "apache");

        remove_env_var("DEVNEST_RUNTIME_APACHE_BIN");
        remove_env_var("DEVNEST_RUNTIME_APACHE_ARGS");
        remove_env_var("DEVNEST_RUNTIME_APACHE_PORT");
        remove_env_var("DEVNEST_LOG_APACHE");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn clears_logs_for_configured_service() {
        let _guard = acquire_env_lock();
        let (root, workspace_dir, state, connection) = setup_state();
        let log_path = workspace_dir.join("runtime-logs").join("apache-clear.log");

        set_env_var("DEVNEST_LOG_APACHE", &log_path);
        fs::create_dir_all(log_path.parent().expect("log parent should exist"))
            .expect("log parent should be created");
        fs::write(&log_path, "line one\nline two\n").expect("log file should be written");

        clear_service_logs(&state, ServiceName::Apache).expect("logs should clear");

        let logs = read_service_logs(&state, ServiceName::Apache, 20)
            .expect("logs should remain readable");
        assert!(logs.content.is_empty());

        remove_env_var("DEVNEST_LOG_APACHE");
        drop(connection);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn returns_stopped_state_when_service_is_not_running() {
        let (_root, _workspace_dir, state, connection) = setup_state();
        let status = get_service_status(&connection, &state, ServiceName::Mysql)
            .expect("status should load");

        assert_eq!(status.status, ServiceStatus::Stopped);
    }

    #[test]
    fn recognizes_php_fastcgi_process_names() {
        assert!(is_php_fastcgi_process_name(Some("php-cgi")));
        assert!(is_php_fastcgi_process_name(Some("PHP")));
        assert!(!is_php_fastcgi_process_name(Some("httpd")));
        assert!(!is_php_fastcgi_process_name(None));
    }

    #[test]
    fn syncs_tunnel_public_host_aliases_into_managed_server_configs() {
        let (_root, workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let project = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                name: "Alias Project".to_string(),
                path: project_root.to_string_lossy().to_string(),
                domain: "vietruyen.test".to_string(),
                server_type: ServerType::Apache,
                php_version: "8.4".to_string(),
                framework: FrameworkType::Laravel,
                document_root: "public".to_string(),
                ssl_enabled: true,
                database_name: None,
                database_port: None,
            },
        )
        .expect("project should create");

        state
            .project_tunnels
            .lock()
            .expect("tunnel state should lock")
            .insert(
                project.id.clone(),
                ProjectTunnelState {
                    project_id: project.id.clone(),
                    provider: TunnelProvider::Cloudflared,
                    status: TunnelStatus::Running,
                    local_url: "https://vietruyen.test".to_string(),
                    public_url: Some("https://violet-river.trycloudflare.com".to_string()),
                    public_host_alias_synced: false,
                    log_path: String::new(),
                    binary_path: None,
                    updated_at: "2026-04-18T00:00:00Z".to_string(),
                    details: None,
                },
            );

        sync_managed_configs_for_service(&connection, &state, &ServiceName::Apache)
            .expect("config sync should succeed");

        let config_path = workspace_dir
            .join("managed-configs")
            .join("apache")
            .join("sites")
            .join("vietruyen.test.conf");
        let config_text = fs::read_to_string(config_path).expect("config should exist");

        assert!(config_text.contains("ServerAlias violet-river.trycloudflare.com"));

        fs::remove_dir_all(project_root).ok();
    }

    #[test]
    fn includes_persistent_public_hostname_aliases_in_managed_server_configs() {
        let (_root, workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let project = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                name: "Persistent Alias Project".to_string(),
                path: project_root.to_string_lossy().to_string(),
                domain: "datlichhoc.test".to_string(),
                server_type: ServerType::Nginx,
                php_version: "8.4".to_string(),
                framework: FrameworkType::Laravel,
                document_root: "public".to_string(),
                ssl_enabled: true,
                database_name: None,
                database_port: None,
            },
        )
        .expect("project should create");

        ProjectPersistentHostnameRepository::upsert(
            &connection,
            &project.id,
            &PersistentTunnelProvider::Cloudflared,
            "preview.devnest.example.com",
        )
        .expect("persistent hostname should save");

        let aliases = public_host_aliases_for_project(&connection, &state, &project.id)
            .expect("persistent aliases should load");
        assert!(aliases.contains(&"preview.devnest.example.com".to_string()));

        sync_managed_configs_for_service(&connection, &state, &ServiceName::Nginx)
            .expect("config sync should succeed");

        let config_path = workspace_dir
            .join("managed-configs")
            .join("nginx")
            .join("sites")
            .join("datlichhoc.test.conf");
        let config_text = fs::read_to_string(config_path).expect("config should exist");

        assert!(config_text.contains("server_name datlichhoc.test preview.devnest.example.com;"));

        fs::remove_dir_all(project_root).ok();
    }
}
