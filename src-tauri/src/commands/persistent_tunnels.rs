use crate::core::persistent_tunnels;
use crate::core::service_manager;
use crate::error::AppError;
use crate::models::persistent_tunnel::{
    ApplyProjectPersistentHostnameInput, ApplyProjectPersistentHostnameResult,
    CreatePersistentNamedTunnelInput, DeleteProjectPersistentHostnameResult,
    PersistentTunnelHealthCheck, PersistentTunnelHealthReport, PersistentTunnelManagedSetup,
    PersistentTunnelNamedTunnelSummary, PersistentTunnelProvider, PersistentTunnelSetupStatus,
    PersistentTunnelStatus, ProjectPersistentHostname, ProjectPersistentTunnelState,
    SelectPersistentNamedTunnelInput, UpdatePersistentTunnelSetupInput,
    UpsertProjectPersistentHostnameInput,
};
use crate::models::project::{Project, ServerType};
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::{AppState, ManagedServiceProcess};
use crate::storage::repositories::{
    PersistentTunnelSetupRepository, ProjectPersistentHostnameRepository, ProjectRepository,
    now_iso,
};
use crate::utils::paths::{
    managed_config_output_path, managed_persistent_tunnel_auth_cert_path,
    managed_persistent_tunnel_config_path, managed_persistent_tunnel_credentials_dir,
    managed_persistent_tunnel_credentials_path, managed_persistent_tunnel_log_path,
};
use crate::utils::process::{
    configure_background_command, find_process_ids_by_commandline, kill_process_tree,
};
use crate::utils::windows::open_url_in_default_browser;
use reqwest::Url;
use reqwest::blocking::Client;
use rfd::FileDialog;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tauri::Manager;
use uuid::Uuid;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn project_server_service_name(server_type: &ServerType) -> ServiceName {
    match server_type {
        ServerType::Apache => ServiceName::Apache,
        ServerType::Nginx => ServiceName::Nginx,
        ServerType::Frankenphp => ServiceName::Frankenphp,
    }
}

fn refresh_project_server_aliases(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<(), AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let service = project_server_service_name(&project.server_type);
    service_manager::sync_managed_configs_for_service(connection, state, &service)?;
    let current = service_manager::get_service_status(connection, state, service.clone())?;
    if matches!(current.status, ServiceStatus::Running) {
        let _ = service_manager::restart_service(connection, state, service)?;
    }

    Ok(())
}

fn persistent_tunnel_process_key() -> &'static str {
    "persistent-tunnel:shared"
}

fn persistent_tunnel_process_name() -> &'static str {
    "cloudflared.exe"
}

fn project_origin_url(project: &Project) -> String {
    let scheme = if project.ssl_enabled { "https" } else { "http" };
    let port = if project.ssl_enabled { 443 } else { 80 };
    format!("{scheme}://127.0.0.1:{port}")
}

fn project_display_url(project: &Project) -> String {
    let scheme = if project.ssl_enabled { "https" } else { "http" };
    format!("{scheme}://{}", project.domain.trim().to_ascii_lowercase())
}

fn persistent_public_url(hostname: &str) -> String {
    format!("https://{}", hostname.trim().to_ascii_lowercase())
}

fn persistent_tunnel_name_matches(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[derive(Debug, Clone)]
struct PersistentTunnelRoute {
    project_id: String,
    hostname: String,
    project_domain: String,
    origin_url: String,
    local_url: String,
    ssl_enabled: bool,
}

fn persistent_tunnel_run_args(
    runtime: &persistent_tunnels::PersistentTunnelRuntime,
    config_path: &Path,
    log_path: &Path,
) -> Vec<String> {
    vec![
        "tunnel".to_string(),
        "--config".to_string(),
        config_path.to_string_lossy().to_string(),
        "--origincert".to_string(),
        runtime.auth_cert_path.to_string_lossy().to_string(),
        "--logfile".to_string(),
        log_path.to_string_lossy().to_string(),
        "--no-autoupdate".to_string(),
        "run".to_string(),
        runtime.tunnel_id.clone(),
    ]
}

fn mutex_error() -> AppError {
    AppError::new_validation(
        "PERSISTENT_TUNNEL_STATE_LOCK_FAILED",
        "DevNest could not access the in-memory persistent tunnel state cache.",
    )
}

fn normalize_yaml_value(value: &str) -> String {
    value.replace('\'', "''")
}

fn normalize_yaml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn configured_persistent_tunnel_hostnames(state: &AppState) -> Vec<String> {
    let config_path = managed_persistent_tunnel_config_path(&state.workspace_dir);
    let Ok(content) = fs::read_to_string(config_path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("- hostname:")
                .map(str::trim)
                .or_else(|| trimmed.strip_prefix("hostname:").map(str::trim))
        })
        .map(|value| {
            value
                .trim_matches('\'')
                .trim_matches('"')
                .trim()
                .to_ascii_lowercase()
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn detected_shared_persistent_tunnel_pids(state: &AppState) -> Result<Vec<u32>, AppError> {
    let config_path = managed_persistent_tunnel_config_path(&state.workspace_dir);
    if !config_path.exists() {
        return Ok(Vec::new());
    }

    find_process_ids_by_commandline(
        persistent_tunnel_process_name(),
        &config_path.to_string_lossy(),
    )
}

fn active_persistent_tunnel_project_ids(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<String>, AppError> {
    let mut ids = tracked_active_persistent_tunnel_project_ids(state)?;

    if !detected_shared_persistent_tunnel_pids(state)?.is_empty() {
        let configured_hostnames = configured_persistent_tunnel_hostnames(state);
        if !configured_hostnames.is_empty() {
            for project in ProjectRepository::list(connection)? {
                let Some(hostname) =
                    ProjectPersistentHostnameRepository::get_by_project(connection, &project.id)?
                else {
                    continue;
                };

                if configured_hostnames
                    .iter()
                    .any(|configured| configured == &hostname.hostname)
                {
                    ids.push(project.id);
                }
            }
        }
    }

    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn tracked_active_persistent_tunnel_project_ids(state: &AppState) -> Result<Vec<String>, AppError> {
    let tunnels = state
        .project_persistent_tunnels
        .lock()
        .map_err(|_| mutex_error())?;
    let mut ids = tunnels
        .iter()
        .filter(|(_, tunnel)| !matches!(tunnel.status, PersistentTunnelStatus::Stopped))
        .map(|(project_id, _)| project_id.clone())
        .collect::<Vec<_>>();

    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub(crate) fn reset_persistent_tunnels_for_origin_service_stop(
    connection: &Connection,
    state: &AppState,
    service: &ServiceName,
) -> Result<(), AppError> {
    let server_type = match service {
        ServiceName::Apache => ServerType::Apache,
        ServiceName::Nginx => ServerType::Nginx,
        ServiceName::Frankenphp => ServerType::Frankenphp,
        ServiceName::Mysql | ServiceName::Mailpit | ServiceName::Redis => return Ok(()),
    };
    let affected_projects = ProjectRepository::list(connection)?
        .into_iter()
        .filter(|project| project.server_type.as_str() == server_type.as_str())
        .collect::<Vec<_>>();

    if affected_projects.is_empty() {
        return Ok(());
    }

    let mut removed_active = false;
    for project in &affected_projects {
        let previous = remove_project_persistent_tunnel_state(state, &project.id)?;
        removed_active |= previous
            .as_ref()
            .map(|tunnel| !matches!(tunnel.status, PersistentTunnelStatus::Stopped))
            .unwrap_or(false);
    }

    let process_running = !detected_shared_persistent_tunnel_pids(state)?.is_empty();
    let configured_hostnames = if process_running {
        configured_persistent_tunnel_hostnames(state)
    } else {
        Vec::new()
    };
    let mut configured_route_active = false;
    if process_running && !configured_hostnames.is_empty() {
        for project in &affected_projects {
            let Some(hostname) =
                ProjectPersistentHostnameRepository::get_by_project(connection, &project.id)?
            else {
                continue;
            };

            configured_route_active |= configured_hostnames
                .iter()
                .any(|configured| configured.eq_ignore_ascii_case(&hostname.hostname));
        }
    }

    if !removed_active && !configured_route_active {
        return Ok(());
    }

    let remaining_ids = tracked_active_persistent_tunnel_project_ids(state)?;
    if remaining_ids.is_empty() {
        stop_shared_persistent_tunnel_process(state)?;
    } else {
        let _ = restart_shared_persistent_tunnel_for_projects(connection, state, &remaining_ids)?;
    }

    Ok(())
}

pub(crate) fn reset_project_persistent_tunnel_after_profile_change(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<(), AppError> {
    let previous = remove_project_persistent_tunnel_state(state, project_id)?;
    let was_active = previous
        .as_ref()
        .map(|tunnel| !matches!(tunnel.status, PersistentTunnelStatus::Stopped))
        .unwrap_or(false);
    let process_running = !detected_shared_persistent_tunnel_pids(state)?.is_empty();
    let configured_hostnames = if process_running {
        configured_persistent_tunnel_hostnames(state)
    } else {
        Vec::new()
    };
    let configured_route_active = if process_running && !configured_hostnames.is_empty() {
        ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?
            .map(|hostname| {
                configured_hostnames
                    .iter()
                    .any(|configured| configured.eq_ignore_ascii_case(&hostname.hostname))
            })
            .unwrap_or(false)
    } else {
        false
    };

    if !was_active && !configured_route_active {
        return Ok(());
    }

    let remaining_ids = tracked_active_persistent_tunnel_project_ids(state)?;
    if remaining_ids.is_empty() {
        stop_shared_persistent_tunnel_process(state)?;
    } else {
        let _ = restart_shared_persistent_tunnel_for_projects(connection, state, &remaining_ids)?;
    }

    Ok(())
}

fn build_persistent_tunnel_route(
    connection: &Connection,
    project_id: &str,
) -> Result<PersistentTunnelRoute, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let hostname = ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?
        .map(|item| item.hostname)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_HOSTNAME_NOT_ASSIGNED",
                "Reserve a stable public hostname for this project before starting a persistent tunnel.",
            )
        })?;

    Ok(PersistentTunnelRoute {
        project_id: project.id.clone(),
        hostname,
        project_domain: project.domain.trim().to_ascii_lowercase(),
        origin_url: project_origin_url(&project),
        local_url: project_display_url(&project),
        ssl_enabled: project.ssl_enabled,
    })
}

fn shared_persistent_tunnel_config_text(
    runtime: &persistent_tunnels::PersistentTunnelRuntime,
    routes: &[PersistentTunnelRoute],
) -> String {
    let mut content = format!(
        "tunnel: '{}'\ncredentials-file: '{}'\ningress:\n",
        normalize_yaml_value(&runtime.tunnel_id),
        normalize_yaml_value(&normalize_yaml_path(&runtime.credentials_path))
    );

    for route in routes {
        content.push_str(&format!(
            "  - hostname: '{}'\n    service: '{}'\n",
            normalize_yaml_value(&route.hostname),
            normalize_yaml_value(&route.origin_url)
        ));
        if route.ssl_enabled {
            content.push_str("    originRequest:\n");
            content.push_str("      noTLSVerify: true\n");
            content.push_str(&format!(
                "      originServerName: '{}'\n",
                normalize_yaml_value(&route.project_domain)
            ));
        }
    }

    content.push_str("  - service: http_status:404\n");
    content
}

fn write_shared_persistent_tunnel_config(
    state: &AppState,
    runtime: &persistent_tunnels::PersistentTunnelRuntime,
    routes: &[PersistentTunnelRoute],
) -> Result<PathBuf, AppError> {
    let config_path = managed_persistent_tunnel_config_path(&state.workspace_dir);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_CONFIG_FAILED",
                "DevNest could not prepare the shared persistent tunnel config folder.",
                error.to_string(),
            )
        })?;
    }

    fs::write(
        &config_path,
        shared_persistent_tunnel_config_text(runtime, routes),
    )
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_CONFIG_FAILED",
            "DevNest could not write the shared persistent tunnel config file.",
            error.to_string(),
        )
    })?;

    Ok(config_path)
}

fn stop_shared_persistent_tunnel_process(state: &AppState) -> Result<(), AppError> {
    let tracked = state
        .managed_processes
        .lock()
        .map_err(|_| mutex_error())?
        .remove(persistent_tunnel_process_key());

    if let Some(mut process) = tracked {
        match process.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                kill_process_tree(process.pid)?;
                let _ = process.child.wait();
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "PERSISTENT_TUNNEL_STOP_FAILED",
                    "DevNest could not inspect the shared persistent tunnel process before stopping it.",
                    error.to_string(),
                ));
            }
        }
    }

    for pid in detected_shared_persistent_tunnel_pids(state)? {
        kill_process_tree(pid)?;
    }

    Ok(())
}

fn load_project_persistent_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    let tunnels = state
        .project_persistent_tunnels
        .lock()
        .map_err(|_| mutex_error())?;

    Ok(tunnels.get(project_id).cloned())
}

fn store_project_persistent_tunnel_state(
    state: &AppState,
    tunnel: Option<ProjectPersistentTunnelState>,
) -> Result<(), AppError> {
    let mut tunnels = state
        .project_persistent_tunnels
        .lock()
        .map_err(|_| mutex_error())?;

    if let Some(next) = tunnel {
        tunnels.insert(next.project_id.clone(), next);
    }

    Ok(())
}

fn remove_project_persistent_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    let mut tunnels = state
        .project_persistent_tunnels
        .lock()
        .map_err(|_| mutex_error())?;

    Ok(tunnels.remove(project_id))
}

fn sync_project_persistent_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    let Some(mut current) = load_project_persistent_tunnel_state(state, project_id)? else {
        return Ok(None);
    };

    let mut processes = state.managed_processes.lock().map_err(|_| mutex_error())?;
    let mut remove_process = false;
    let next_status = if let Some(process) = processes.get_mut(persistent_tunnel_process_key()) {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                remove_process = true;
                current.details = Some(format!(
                    "The shared persistent tunnel process exited unexpectedly with exit code {:?}.",
                    status.code()
                ));
                PersistentTunnelStatus::Error
            }
            Ok(None) => {
                current.details = Some(format!(
                    "Persistent tunnel is active for {} through the shared named tunnel and routes through {}.",
                    current.public_url, current.local_url
                ));
                PersistentTunnelStatus::Running
            }
            Err(error) => {
                remove_process = true;
                current.details = Some(error.to_string());
                PersistentTunnelStatus::Error
            }
        }
    } else if detected_shared_persistent_tunnel_pids(state)?.is_empty() {
        current.details = Some(
            "Persistent tunnel metadata still exists, but the process is no longer running."
                .to_string(),
        );
        PersistentTunnelStatus::Stopped
    } else {
        current.details = Some(format!(
            "Persistent tunnel is active for {} through the shared named tunnel and routes through {}.",
            current.public_url, current.local_url
        ));
        PersistentTunnelStatus::Running
    };

    if remove_process {
        processes.remove(persistent_tunnel_process_key());
    }

    drop(processes);
    current.status = next_status;
    current.updated_at = now_iso()?;
    store_project_persistent_tunnel_state(state, Some(current.clone()))?;
    Ok(Some(current))
}

fn inferred_project_persistent_tunnel_state(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let Some(hostname) =
        ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?
    else {
        return Ok(None);
    };
    let setup =
        PersistentTunnelSetupRepository::get(connection, &PersistentTunnelProvider::Cloudflared)?;
    let setup_status = persistent_tunnels::persistent_tunnel_setup_status(connection).ok();
    let configured_hostnames = configured_persistent_tunnel_hostnames(state);
    let process_running = !detected_shared_persistent_tunnel_pids(state)?.is_empty();
    let route_active = configured_hostnames
        .iter()
        .any(|configured| configured == &hostname.hostname);
    let status = if process_running && route_active {
        PersistentTunnelStatus::Running
    } else {
        PersistentTunnelStatus::Stopped
    };
    let tunnel = ProjectPersistentTunnelState {
        project_id: project.id.clone(),
        provider: PersistentTunnelProvider::Cloudflared,
        status: status.clone(),
        hostname: hostname.hostname.clone(),
        local_url: project_display_url(&project),
        public_url: persistent_public_url(&hostname.hostname),
        log_path: managed_persistent_tunnel_log_path(&state.workspace_dir)
            .to_string_lossy()
            .to_string(),
        binary_path: setup_status.and_then(|status| status.binary_path),
        tunnel_id: setup.as_ref().and_then(|item| item.tunnel_id.clone()),
        credentials_path: setup
            .as_ref()
            .and_then(|item| item.credentials_path.clone()),
        updated_at: now_iso()?,
        details: Some(if matches!(status, PersistentTunnelStatus::Running) {
            format!(
                "DevNest detected an active shared persistent tunnel for {} from a previous app session.",
                persistent_public_url(&hostname.hostname)
            )
        } else if route_active {
            "Persistent tunnel config still includes this hostname, but the shared process is not running.".to_string()
        } else {
            "The persistent public tunnel is stopped.".to_string()
        }),
    };

    store_project_persistent_tunnel_state(state, Some(tunnel.clone()))?;
    Ok(Some(tunnel))
}

fn project_persistent_tunnel_state(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    if let Some(tunnel) = sync_project_persistent_tunnel_state(state, project_id)? {
        return Ok(Some(tunnel));
    }

    inferred_project_persistent_tunnel_state(connection, state, project_id)
}

fn active_persistent_tunnel_exists(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<bool, AppError> {
    Ok(matches!(
        project_persistent_tunnel_state(connection, state, project_id)?.map(|item| item.status),
        Some(PersistentTunnelStatus::Starting | PersistentTunnelStatus::Running)
    ))
}

fn ensure_project_origin_service(
    connection: &Connection,
    state: &AppState,
    project: &Project,
) -> Result<(), AppError> {
    let service = project_server_service_name(&project.server_type);
    let service_state = service_manager::get_service_status(connection, state, service.clone())?;
    if !matches!(service_state.status, ServiceStatus::Running) {
        let _ = service_manager::start_service(connection, state, service)?;
    }

    Ok(())
}

fn health_status_rank(status: &PersistentTunnelStatus) -> u8 {
    match status {
        PersistentTunnelStatus::Error => 3,
        PersistentTunnelStatus::Starting => 2,
        PersistentTunnelStatus::Stopped => 1,
        PersistentTunnelStatus::Running => 0,
    }
}

fn worst_health_status(checks: &[PersistentTunnelHealthCheck]) -> PersistentTunnelStatus {
    checks
        .iter()
        .map(|item| item.status.clone())
        .max_by_key(health_status_rank)
        .unwrap_or(PersistentTunnelStatus::Stopped)
}

fn dns_resolves(hostname: &str) -> bool {
    format!("{hostname}:443").to_socket_addrs().is_ok()
}

fn parse_tunnel_id_from_text(value: &str) -> Option<String> {
    value
        .split(|character: char| !(character.is_ascii_hexdigit() || character == '-'))
        .find_map(|token| {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                return None;
            }

            Uuid::parse_str(trimmed).ok().map(|uuid| uuid.to_string())
        })
}

fn parse_tunnel_id_from_credentials_document(path: &Path) -> Result<String, AppError> {
    let content = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_CREDENTIALS_INVALID",
            "DevNest could not read the selected named tunnel credentials file.",
            error.to_string(),
        )
    })?;

    let document: Value = serde_json::from_str(&content).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_CREDENTIALS_INVALID",
            "DevNest could not parse the selected named tunnel credentials JSON.",
            error.to_string(),
        )
    })?;

    let tunnel_id = ["TunnelID", "tunnelID", "tunnel_id", "id"]
        .into_iter()
        .find_map(|key| document.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .and_then(parse_tunnel_id_from_text)
        })
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_CREDENTIALS_INVALID",
                "DevNest could not determine which tunnel this credentials file belongs to.",
            )
        })?;

    Ok(tunnel_id)
}

fn merge_setup(
    connection: &Connection,
    auth_cert_path: Option<&str>,
    credentials_path: Option<&str>,
    tunnel_id: Option<&str>,
    tunnel_name: Option<&str>,
    default_hostname_zone: Option<&str>,
) -> Result<PersistentTunnelManagedSetup, AppError> {
    let current =
        PersistentTunnelSetupRepository::get(connection, &PersistentTunnelProvider::Cloudflared)?;

    let next_auth_cert_path = auth_cert_path.map(ToOwned::to_owned).or_else(|| {
        current
            .as_ref()
            .and_then(|item| item.auth_cert_path.clone())
    });
    let next_credentials_path = credentials_path.map(ToOwned::to_owned).or_else(|| {
        current
            .as_ref()
            .and_then(|item| item.credentials_path.clone())
    });
    let next_tunnel_id = tunnel_id
        .map(ToOwned::to_owned)
        .or_else(|| current.as_ref().and_then(|item| item.tunnel_id.clone()));
    let next_tunnel_name = tunnel_name
        .map(ToOwned::to_owned)
        .or_else(|| current.as_ref().and_then(|item| item.tunnel_name.clone()));
    let next_default_zone = default_hostname_zone.map(ToOwned::to_owned).or_else(|| {
        current
            .as_ref()
            .and_then(|item| item.default_hostname_zone.clone())
    });

    PersistentTunnelSetupRepository::upsert(
        connection,
        &PersistentTunnelProvider::Cloudflared,
        next_auth_cert_path.as_deref(),
        next_credentials_path.as_deref(),
        next_tunnel_id.as_deref(),
        next_tunnel_name.as_deref(),
        next_default_zone.as_deref(),
    )
}

fn discover_zone_if_missing(connection: &Connection, auth_cert_path: &Path) -> Option<String> {
    let current =
        PersistentTunnelSetupRepository::get(connection, &PersistentTunnelProvider::Cloudflared)
            .ok()
            .flatten();
    if let Some(zone) = current
        .as_ref()
        .and_then(|item| item.default_hostname_zone.as_ref())
    {
        return Some(zone.clone());
    }

    persistent_tunnels::discover_default_zone_from_auth_cert(auth_cert_path)
        .ok()
        .flatten()
}

fn overwrite_setup(
    connection: &Connection,
    setup: Option<PersistentTunnelManagedSetup>,
) -> Result<(), AppError> {
    if let Some(next) = setup {
        PersistentTunnelSetupRepository::upsert(
            connection,
            &PersistentTunnelProvider::Cloudflared,
            next.auth_cert_path.as_deref(),
            next.credentials_path.as_deref(),
            next.tunnel_id.as_deref(),
            next.tunnel_name.as_deref(),
            next.default_hostname_zone.as_deref(),
        )?;
    } else {
        let _ = PersistentTunnelSetupRepository::delete(
            connection,
            &PersistentTunnelProvider::Cloudflared,
        )?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path, code: &str, message: &str) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|error| AppError::with_details(code, message, error.to_string()))
}

fn copy_file_into_place(source_path: &Path, target_path: &Path) -> Result<PathBuf, AppError> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_SETUP_FAILED",
                "DevNest could not prepare the managed persistent tunnel folder.",
                error.to_string(),
            )
        })?;
    }

    if source_path.exists() && target_path.exists() {
        let same_path = fs::canonicalize(source_path)
            .ok()
            .zip(fs::canonicalize(target_path).ok())
            .map(|(source, target)| source == target)
            .unwrap_or_else(|| {
                source_path
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&target_path.to_string_lossy())
            });
        if same_path {
            return Ok(target_path.to_path_buf());
        }
    }

    fs::copy(source_path, target_path).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_SETUP_FAILED",
            "DevNest could not copy the selected persistent tunnel file into app-managed storage.",
            error.to_string(),
        )
    })?;

    Ok(target_path.to_path_buf())
}

fn copy_auth_cert_into_managed_storage(
    state: &AppState,
    source_path: &Path,
) -> Result<PathBuf, AppError> {
    copy_file_into_place(
        source_path,
        &managed_persistent_tunnel_auth_cert_path(&state.workspace_dir),
    )
}

fn copy_credentials_into_managed_storage(
    state: &AppState,
    source_path: &Path,
    tunnel_id: &str,
) -> Result<PathBuf, AppError> {
    copy_file_into_place(
        source_path,
        &managed_persistent_tunnel_credentials_path(&state.workspace_dir, tunnel_id),
    )
}

fn run_cloudflared_capture(
    binary_path: &Path,
    args: &[String],
    current_dir: Option<&Path>,
    error_code: &str,
    error_message: &str,
) -> Result<Output, AppError> {
    let mut command = Command::new(binary_path);
    command.args(args).stdin(Stdio::null());
    configure_background_command(&mut command);
    if let Some(path) = current_dir {
        command.current_dir(path);
    }

    command
        .output()
        .map_err(|error| AppError::with_details(error_code, error_message, error.to_string()))
}

fn cloudflared_arg_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn lower_text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase()
}

fn dns_route_requires_overwrite(details: &str) -> bool {
    let normalized = details.to_ascii_lowercase();
    (normalized.contains("code: 1003")
        || normalized.contains("already exists")
        || normalized.contains("already configured"))
        && (normalized.contains("a, aaaa, or cname record")
            || normalized.contains("record with that host already exists")
            || normalized.contains("exists and points")
            || normalized.contains("already configured"))
}

fn cloudflared_list_runtime(connection: &Connection) -> Option<(PathBuf, PathBuf)> {
    let setup_status = persistent_tunnels::persistent_tunnel_setup_status(connection).ok()?;
    let binary_path = setup_status.binary_path.map(PathBuf::from)?;
    let auth_cert_path = setup_status.auth_cert_path.map(PathBuf::from)?;
    if binary_path.exists() && auth_cert_path.exists() {
        Some((binary_path, auth_cert_path))
    } else {
        None
    }
}

fn parse_tunnel_name_from_list_entry(entry: &Value) -> Option<String> {
    ["name", "Name"]
        .into_iter()
        .find_map(|key| entry.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_tunnel_id_from_list_entry(entry: &Value) -> Option<String> {
    ["id", "ID", "tunnelID", "TunnelID"]
        .into_iter()
        .find_map(|key| entry.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| parse_tunnel_id_from_text(&entry.to_string()))
}

fn discover_named_tunnels(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<PersistentTunnelNamedTunnelSummary>, AppError> {
    let current_setup =
        PersistentTunnelSetupRepository::get(connection, &PersistentTunnelProvider::Cloudflared)?;
    let selected_tunnel_id = current_setup
        .as_ref()
        .and_then(|item| item.tunnel_id.clone());
    let mut items = BTreeMap::<String, PersistentTunnelNamedTunnelSummary>::new();

    let credentials_roots = [Some(managed_persistent_tunnel_credentials_dir(
        &state.workspace_dir,
    ))];
    for root in credentials_roots.into_iter().flatten() {
        if !root.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }

                if let Ok(tunnel_id) = parse_tunnel_id_from_credentials_document(&path) {
                    let item = items.entry(tunnel_id.clone()).or_insert(
                        PersistentTunnelNamedTunnelSummary {
                            tunnel_id: tunnel_id.clone(),
                            tunnel_name: tunnel_id.clone(),
                            credentials_path: Some(path.to_string_lossy().to_string()),
                            selected: selected_tunnel_id.as_deref() == Some(tunnel_id.as_str()),
                        },
                    );
                    if item.credentials_path.is_none() {
                        item.credentials_path = Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    if let Some((binary_path, auth_cert_path)) = cloudflared_list_runtime(connection) {
        let args = vec![
            "tunnel".to_string(),
            "--origincert".to_string(),
            auth_cert_path.to_string_lossy().to_string(),
            "list".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];
        let output = run_cloudflared_capture(
            &binary_path,
            &args,
            None,
            "PERSISTENT_TUNNEL_LIST_FAILED",
            "DevNest could not ask cloudflared for the available named tunnels.",
        )?;

        if output.status.success() {
            let json_text = String::from_utf8_lossy(&output.stdout);
            if let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&json_text) {
                for entry in entries {
                    let Some(tunnel_id) = parse_tunnel_id_from_list_entry(&entry) else {
                        continue;
                    };
                    let tunnel_name = parse_tunnel_name_from_list_entry(&entry)
                        .unwrap_or_else(|| tunnel_id.clone());
                    let item = items.entry(tunnel_id.clone()).or_insert(
                        PersistentTunnelNamedTunnelSummary {
                            tunnel_id: tunnel_id.clone(),
                            tunnel_name: tunnel_name.clone(),
                            credentials_path: None,
                            selected: selected_tunnel_id.as_deref() == Some(tunnel_id.as_str()),
                        },
                    );
                    item.tunnel_name = tunnel_name;
                    if item.selected {
                        item.selected = true;
                    }
                }
            }
        }
    }

    Ok(items.into_values().collect())
}

fn slugify_public_host_segment(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_dash = false;

    for character in value.chars() {
        let next = if character.is_ascii_alphanumeric() {
            Some(character.to_ascii_lowercase())
        } else {
            None
        };

        if let Some(character) = next {
            output.push(character);
            last_was_dash = false;
            continue;
        }

        if !last_was_dash && !output.is_empty() {
            output.push('-');
            last_was_dash = true;
        }
    }

    output.trim_matches('-').to_string()
}

fn suggested_public_hostname(project: &Project, zone: &str) -> Result<String, AppError> {
    let left_side = project
        .domain
        .trim()
        .trim_end_matches(".test")
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = slugify_public_host_segment(if left_side.is_empty() {
        &project.name
    } else {
        &left_side
    });

    if slug.is_empty() {
        return Err(AppError::new_validation(
            "PERSISTENT_HOSTNAME_SUGGESTION_FAILED",
            "DevNest could not generate a stable public hostname suggestion for this project.",
        ));
    }

    Ok(format!(
        "{slug}.{}",
        zone.trim().trim_matches('.').to_ascii_lowercase()
    ))
}

fn resolve_named_tunnel_runtime_with_default_zone(
    connection: &Connection,
) -> Result<persistent_tunnels::PersistentTunnelRuntime, AppError> {
    let runtime = persistent_tunnels::resolve_named_tunnel_runtime(connection)?;
    if runtime.default_hostname_zone.is_some() {
        return Ok(runtime);
    }

    if let Some(discovered_zone) =
        persistent_tunnels::discover_default_zone_from_auth_cert(&runtime.auth_cert_path)
            .ok()
            .flatten()
    {
        merge_setup(connection, None, None, None, None, Some(&discovered_zone))?;
    }

    persistent_tunnels::resolve_named_tunnel_runtime(connection)
}

fn resolve_project_persistent_hostname(
    project: &Project,
    hostname_input: Option<&str>,
    default_zone: Option<&str>,
) -> Result<String, AppError> {
    let trimmed = hostname_input.unwrap_or_default().trim();
    if trimmed.is_empty() {
        let zone = default_zone.ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_HOSTNAME_ZONE_MISSING",
                "Set a default public zone in Settings before leaving the stable hostname blank.",
            )
        })?;
        return suggested_public_hostname(project, zone);
    }

    let normalized = trimmed.trim_end_matches('.').to_ascii_lowercase();
    if normalized.contains('.') {
        return Ok(normalized);
    }

    let zone = default_zone.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_HOSTNAME_ZONE_MISSING",
            "Set a default public zone in Settings before using a bare subdomain here.",
        )
    })?;

    Ok(format!(
        "{}.{}",
        normalized,
        zone.trim().trim_matches('.').to_ascii_lowercase()
    ))
}

fn cloudflare_api_client() -> Result<Client, AppError> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not prepare the Cloudflare DNS cleanup client.",
                error.to_string(),
            )
        })
}

fn cloudflare_zone_candidates(hostname: &str, zone_hint: Option<&str>) -> Vec<String> {
    let normalized = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels = normalized.split('.').collect::<Vec<_>>();
    let mut candidates = Vec::new();

    if let Some(zone) = zone_hint
        .map(str::trim)
        .map(|value| value.trim_matches('.').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        let matches_zone = normalized == zone || normalized.ends_with(&format!(".{zone}"));
        if matches_zone {
            candidates.push(zone);
        }
    }

    if labels.len() == 2 && !candidates.iter().any(|item| item == &normalized) {
        candidates.push(normalized.clone());
    }

    for index in 1..labels.len().saturating_sub(1) {
        let candidate = labels[index..].join(".");
        if candidate.split('.').count() < 2 {
            continue;
        }

        if !candidates.iter().any(|item| item == &candidate) {
            candidates.push(candidate);
        }
    }

    candidates
}

fn cloudflare_lookup_zone_id(
    client: &Client,
    api_token: &str,
    zone_name: &str,
) -> Result<Option<String>, AppError> {
    let mut url = Url::parse("https://api.cloudflare.com/client/v4/zones").map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not build the Cloudflare zone lookup URL.",
            error.to_string(),
        )
    })?;
    url.query_pairs_mut()
        .append_pair("name", zone_name)
        .append_pair("status", "active")
        .append_pair("per_page", "1");

    let response = client
        .get(url)
        .bearer_auth(api_token)
        .send()
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not query Cloudflare for the matching DNS zone.",
                error.to_string(),
            )
        })?;
    let status = response.status();
    let body_text = response.text().map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not read the Cloudflare zone lookup response.",
            error.to_string(),
        )
    })?;
    let body: Value = serde_json::from_str(&body_text).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not parse the Cloudflare zone lookup response.",
            error.to_string(),
        )
    })?;

    if !status.is_success() || body.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not look up the Cloudflare DNS zone for this hostname.",
            body_text,
        ));
    }

    Ok(body
        .get("result")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

fn delete_named_tunnel_dns_route(
    auth_cert_path: &Path,
    hostname: &str,
    tunnel_id: Option<&str>,
    zone_hint: Option<&str>,
) -> Result<bool, AppError> {
    let normalized_hostname = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    let metadata = persistent_tunnels::parse_cloudflared_auth_cert_metadata(auth_cert_path)?;
    let client = cloudflare_api_client()?;
    let zone_id = if let Some(zone_id) = metadata.zone_id.clone() {
        zone_id
    } else {
        let mut resolved_zone_id = None;
        for zone_name in cloudflare_zone_candidates(&normalized_hostname, zone_hint) {
            if let Some(zone_id) =
                cloudflare_lookup_zone_id(&client, &metadata.api_token, &zone_name)?
            {
                resolved_zone_id = Some(zone_id);
                break;
            }
        }

        resolved_zone_id.ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not determine which Cloudflare zone owns this hostname.",
            )
        })?
    };

    let mut list_url = Url::parse(&format!(
        "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records"
    ))
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not build the Cloudflare DNS record lookup URL.",
            error.to_string(),
        )
    })?;
    list_url
        .query_pairs_mut()
        .append_pair("name", &normalized_hostname)
        .append_pair("per_page", "100");

    let list_response = client
        .get(list_url)
        .bearer_auth(&metadata.api_token)
        .send()
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not query Cloudflare for the current DNS record.",
                error.to_string(),
            )
        })?;
    let list_status = list_response.status();
    let list_body_text = list_response.text().map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not read the Cloudflare DNS record lookup response.",
            error.to_string(),
        )
    })?;
    let list_body: Value = serde_json::from_str(&list_body_text).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not parse the Cloudflare DNS record lookup response.",
            error.to_string(),
        )
    })?;

    if !list_status.is_success() || list_body.get("success").and_then(Value::as_bool) == Some(false)
    {
        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
            "DevNest could not inspect the current Cloudflare DNS record for this hostname.",
            list_body_text,
        ));
    }

    let expected_target = tunnel_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{}.cfargotunnel.com", value.to_ascii_lowercase()));
    let mut deleted_any = false;
    let results = list_body
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for record in results {
        let record_id = record
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let record_name = record
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(|value| value.trim_end_matches('.').to_ascii_lowercase());
        let record_content = record
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
            .unwrap_or_default();

        let Some(record_id) = record_id else {
            continue;
        };
        if record_name.as_deref() != Some(normalized_hostname.as_str()) {
            continue;
        }

        let matches_tunnel_record = if let Some(expected_target) = expected_target.as_deref() {
            record_content == expected_target
        } else {
            record_content.ends_with(".cfargotunnel.com")
        };
        if !matches_tunnel_record {
            continue;
        }

        let delete_url =
            format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{record_id}");
        let delete_response = client
            .delete(delete_url)
            .bearer_auth(&metadata.api_token)
            .send()
            .map_err(|error| {
                AppError::with_details(
                    "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                    "DevNest could not remove the Cloudflare DNS record for this hostname.",
                    error.to_string(),
                )
            })?;
        let delete_status = delete_response.status();
        let delete_body_text = delete_response.text().map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not read the Cloudflare DNS delete response.",
                error.to_string(),
            )
        })?;
        let delete_body: Value = serde_json::from_str(&delete_body_text).map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not parse the Cloudflare DNS delete response.",
                error.to_string(),
            )
        })?;

        if !delete_status.is_success()
            || delete_body.get("success").and_then(Value::as_bool) == Some(false)
        {
            return Err(AppError::with_details(
                "PERSISTENT_TUNNEL_DNS_DELETE_FAILED",
                "DevNest could not remove the Cloudflare DNS record for this hostname.",
                delete_body_text,
            ));
        }

        deleted_any = true;
    }

    Ok(deleted_any)
}

fn ensure_named_tunnel_dns_route(
    runtime: &persistent_tunnels::PersistentTunnelRuntime,
    hostname: &str,
    current_dir: &Path,
) -> Result<(), AppError> {
    let args = vec![
        "tunnel".to_string(),
        "--origincert".to_string(),
        runtime.auth_cert_path.to_string_lossy().to_string(),
        "route".to_string(),
        "dns".to_string(),
        runtime.tunnel_id.clone(),
        hostname.to_string(),
    ];
    let output = run_cloudflared_capture(
        &runtime.binary_path,
        &args,
        Some(current_dir),
        "PERSISTENT_TUNNEL_DNS_ROUTE_FAILED",
        "DevNest could not configure the DNS route for this persistent hostname.",
    )?;

    if output.status.success() {
        return Ok(());
    }

    let combined = lower_text(&output);
    if dns_route_requires_overwrite(&combined) {
        let overwrite_args = vec![
            "tunnel".to_string(),
            "--origincert".to_string(),
            runtime.auth_cert_path.to_string_lossy().to_string(),
            "route".to_string(),
            "dns".to_string(),
            "--overwrite-dns".to_string(),
            runtime.tunnel_id.clone(),
            hostname.to_string(),
        ];
        let overwrite_output = run_cloudflared_capture(
            &runtime.binary_path,
            &overwrite_args,
            Some(current_dir),
            "PERSISTENT_TUNNEL_DNS_ROUTE_FAILED",
            "DevNest could not update the DNS route for this persistent hostname.",
        )?;

        if overwrite_output.status.success() {
            return Ok(());
        }

        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_DNS_ROUTE_FAILED",
            "DevNest could not update the DNS route for this persistent hostname.",
            lower_text(&overwrite_output).trim().to_string(),
        ));
    }

    Err(AppError::with_details(
        "PERSISTENT_TUNNEL_DNS_ROUTE_FAILED",
        "DevNest could not configure the DNS route for this persistent hostname.",
        combined.trim().to_string(),
    ))
}

fn restart_shared_persistent_tunnel_for_projects(
    connection: &Connection,
    state: &AppState,
    project_ids: &[String],
) -> Result<
    (
        persistent_tunnels::PersistentTunnelRuntime,
        Vec<PersistentTunnelRoute>,
    ),
    AppError,
> {
    stop_shared_persistent_tunnel_process(state)?;

    let runtime = persistent_tunnels::resolve_named_tunnel_runtime(connection)?;
    let ordered_ids = project_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut routes = Vec::new();

    for project_id in ordered_ids.iter() {
        let project = ProjectRepository::get(connection, project_id)?;
        ensure_project_origin_service(connection, state, &project)?;
        routes.push(build_persistent_tunnel_route(connection, project_id)?);
    }

    let config_path = write_shared_persistent_tunnel_config(state, &runtime, &routes)?;
    let log_path = managed_persistent_tunnel_log_path(&state.workspace_dir);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_START_FAILED",
                "DevNest could not prepare the shared persistent tunnel log folder.",
                error.to_string(),
            )
        })?;
    }
    fs::write(&log_path, "").map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_START_FAILED",
            "DevNest could not reset the shared persistent tunnel log file.",
            error.to_string(),
        )
    })?;

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut command = Command::new(&runtime.binary_path);
    command
        .args(persistent_tunnel_run_args(
            &runtime,
            &config_path,
            &log_path,
        ))
        .current_dir(&state.workspace_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    configure_background_command(&mut command);

    let child = command.spawn().map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_START_FAILED",
            "DevNest could not start the shared persistent domain tunnel.",
            error.to_string(),
        )
    })?;

    let pid = child.id();
    state
        .managed_processes
        .lock()
        .map_err(|_| mutex_error())?
        .insert(
            persistent_tunnel_process_key().to_string(),
            ManagedServiceProcess {
                pid,
                child,
                log_path: log_path.clone(),
            },
        );

    for route in &routes {
        store_project_persistent_tunnel_state(
            state,
            Some(ProjectPersistentTunnelState {
                project_id: route.project_id.clone(),
                provider: PersistentTunnelProvider::Cloudflared,
                status: PersistentTunnelStatus::Starting,
                hostname: route.hostname.clone(),
                local_url: route.local_url.clone(),
                public_url: persistent_public_url(&route.hostname),
                log_path: log_path.to_string_lossy().to_string(),
                binary_path: Some(runtime.binary_path.to_string_lossy().to_string()),
                tunnel_id: Some(runtime.tunnel_id.clone()),
                credentials_path: Some(runtime.credentials_path.to_string_lossy().to_string()),
                updated_at: now_iso()?,
                details: Some(format!(
                    "Shared persistent tunnel is starting for {} through {}.",
                    persistent_public_url(&route.hostname),
                    route.local_url
                )),
            }),
        )?;
    }

    Ok((runtime, routes))
}

pub(crate) fn start_project_persistent_tunnel_internal(
    project_id: &str,
    state: &AppState,
    auto_assign_hostname: bool,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let connection = connection_from_state(state)?;
    if let Some(existing) = project_persistent_tunnel_state(&connection, state, project_id)? {
        if matches!(
            existing.status,
            PersistentTunnelStatus::Starting | PersistentTunnelStatus::Running
        ) {
            return Ok(existing);
        }
    }

    let project = ProjectRepository::get(&connection, project_id)?;
    let runtime = resolve_named_tunnel_runtime_with_default_zone(&connection)?;
    let hostname_record = ProjectPersistentHostnameRepository::get_by_project(&connection, project_id)?
        .or_else(|| {
            if !auto_assign_hostname {
                return None;
            }

            runtime
                .default_hostname_zone
                .as_deref()
                .and_then(|zone| suggested_public_hostname(&project, zone).ok())
                .and_then(|hostname| {
                    ProjectPersistentHostnameRepository::upsert(
                        &connection,
                        project_id,
                        &PersistentTunnelProvider::Cloudflared,
                        &hostname,
                    )
                    .ok()
                })
        })
        .ok_or_else(|| {
            AppError::new_validation(
                if auto_assign_hostname {
                    "PERSISTENT_HOSTNAME_ZONE_MISSING"
                } else {
                    "PERSISTENT_HOSTNAME_NOT_ASSIGNED"
                },
                if auto_assign_hostname {
                    "Set a default public zone in Settings or save a custom hostname before publishing this project."
                } else {
                    "Reserve a stable public hostname for this project before starting a persistent tunnel."
                },
            )
        })?;

    refresh_project_server_aliases(&connection, state, project_id)?;
    ensure_named_tunnel_dns_route(
        &runtime,
        &hostname_record.hostname,
        Path::new(&project.path),
    )?;
    let mut active_ids = tracked_active_persistent_tunnel_project_ids(state)?;
    if !active_ids.iter().any(|value| value == project_id) {
        active_ids.push(project_id.to_string());
    }
    active_ids.sort();
    active_ids.dedup();

    let _ = restart_shared_persistent_tunnel_for_projects(&connection, state, &active_ids)?;

    project_persistent_tunnel_state(&connection, state, project_id)?.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_START_FAILED",
            "Persistent tunnel started, but DevNest could not confirm the current state.",
        )
    })
}

fn apply_project_persistent_hostname_internal(
    project_id: &str,
    hostname_input: Option<&str>,
    state: &AppState,
) -> Result<ApplyProjectPersistentHostnameResult, AppError> {
    let connection = connection_from_state(state)?;
    let project = ProjectRepository::get(&connection, project_id)?;
    let previous_hostname =
        ProjectPersistentHostnameRepository::get_by_project(&connection, project_id)?;
    let runtime = resolve_named_tunnel_runtime_with_default_zone(&connection)?;
    let resolved_hostname = resolve_project_persistent_hostname(
        &project,
        hostname_input,
        runtime.default_hostname_zone.as_deref(),
    )?;
    let hostname = ProjectPersistentHostnameRepository::upsert(
        &connection,
        project_id,
        &PersistentTunnelProvider::Cloudflared,
        &resolved_hostname,
    )?;

    refresh_project_server_aliases(&connection, state, project_id)?;
    ensure_named_tunnel_dns_route(&runtime, &hostname.hostname, Path::new(&project.path))?;

    let mut active_ids = tracked_active_persistent_tunnel_project_ids(state)?;
    if !active_ids.iter().any(|value| value == project_id) {
        active_ids.push(project_id.to_string());
    }
    active_ids.sort();
    active_ids.dedup();

    let _ = restart_shared_persistent_tunnel_for_projects(&connection, state, &active_ids)?;

    if let Some(previous_hostname) = previous_hostname
        .as_ref()
        .map(|item| item.hostname.as_str())
        .filter(|value| !value.eq_ignore_ascii_case(&hostname.hostname))
    {
        let _ = delete_named_tunnel_dns_route(
            &runtime.auth_cert_path,
            previous_hostname,
            Some(&runtime.tunnel_id),
            runtime.default_hostname_zone.as_deref(),
        )?;
    }

    let tunnel = project_persistent_tunnel_state(&connection, state, project_id)?.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_START_FAILED",
            "Persistent tunnel applied the hostname, but DevNest could not confirm the current state.",
        )
    })?;

    Ok(ApplyProjectPersistentHostnameResult { hostname, tunnel })
}

fn delete_project_persistent_hostname_internal(
    project_id: &str,
    state: &AppState,
) -> Result<DeleteProjectPersistentHostnameResult, AppError> {
    let connection = connection_from_state(state)?;
    let hostname = ProjectPersistentHostnameRepository::get_by_project(&connection, project_id)?
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_HOSTNAME_NOT_ASSIGNED",
                "This project does not have a stable public hostname yet.",
            )
        })?;
    let setup =
        PersistentTunnelSetupRepository::get(&connection, &PersistentTunnelProvider::Cloudflared)?;
    let auth_cert_path = persistent_tunnels::persistent_tunnel_setup_status(&connection)?
        .auth_cert_path
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_AUTH_MISSING",
                "Connect Cloudflare or import the cloudflared cert before deleting this hostname from DNS.",
            )
        })?;

    let _ = stop_project_persistent_tunnel_internal(project_id, state)?;
    let _ = delete_named_tunnel_dns_route(
        &auth_cert_path,
        &hostname.hostname,
        setup.as_ref().and_then(|item| item.tunnel_id.as_deref()),
        setup
            .as_ref()
            .and_then(|item| item.default_hostname_zone.as_deref()),
    )?;
    let deleted = ProjectPersistentHostnameRepository::delete_by_project(&connection, project_id)?;
    if deleted {
        refresh_project_server_aliases(&connection, state, project_id)?;
    }

    Ok(DeleteProjectPersistentHostnameResult {
        hostname: hostname.hostname,
    })
}

#[tauri::command]
pub fn get_persistent_tunnel_setup_status(
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn connect_persistent_tunnel_provider(
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let binary_path = persistent_tunnels::persistent_tunnel_setup_status(&connection)?
        .binary_path
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_BINARY_MISSING",
                "Install cloudflared before connecting Cloudflare.",
            )
        })?;

    let args = cloudflared_arg_strings(&["tunnel", "login"]);
    let output = run_cloudflared_capture(
        &binary_path,
        &args,
        Some(&state.workspace_dir),
        "PERSISTENT_TUNNEL_AUTH_FAILED",
        "DevNest could not start the Cloudflare login flow.",
    )?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_AUTH_FAILED",
            "Cloudflare login did not finish successfully.",
            lower_text(&output),
        ));
    }

    let default_cert = persistent_tunnels::default_cloudflared_cert_path().ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_AUTH_FAILED",
            "Cloudflare login finished, but DevNest could not find the cloudflared cert.pem file afterward.",
        )
    })?;
    let managed_cert_path = copy_auth_cert_into_managed_storage(&state, &default_cert)?;
    let default_zone = discover_zone_if_missing(&connection, &managed_cert_path);
    merge_setup(
        &connection,
        Some(&managed_cert_path.to_string_lossy()),
        None,
        None,
        None,
        default_zone.as_deref(),
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn import_persistent_tunnel_auth_cert(
    state: tauri::State<'_, AppState>,
) -> Result<Option<PersistentTunnelSetupStatus>, AppError> {
    let source_path = match FileDialog::new()
        .add_filter("cloudflared Auth Cert", &["pem"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };

    let managed_cert_path = copy_auth_cert_into_managed_storage(&state, &source_path)?;
    let connection = connection_from_state(&state)?;
    let default_zone = discover_zone_if_missing(&connection, &managed_cert_path);
    merge_setup(
        &connection,
        Some(&managed_cert_path.to_string_lossy()),
        None,
        None,
        None,
        default_zone.as_deref(),
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection).map(Some)
}

#[tauri::command]
pub fn create_persistent_named_tunnel(
    input: CreatePersistentNamedTunnelInput,
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let setup =
        PersistentTunnelSetupRepository::get(&connection, &PersistentTunnelProvider::Cloudflared)?;
    let auth_cert_path = setup
        .as_ref()
        .and_then(|item| item.auth_cert_path.as_ref())
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_AUTH_MISSING",
                "Connect Cloudflare or import the cloudflared cert before creating a named tunnel.",
            )
        })?;
    let binary_path = persistent_tunnels::persistent_tunnel_setup_status(&connection)?
        .binary_path
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_BINARY_MISSING",
                "Install cloudflared before creating a named tunnel.",
            )
        })?;

    let name = input.name.trim();
    if name.len() < 2 {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_NAME_INVALID",
            "Named tunnel name must contain at least 2 characters.",
        ));
    }
    if discover_named_tunnels(&connection, &state)?
        .iter()
        .any(|item| persistent_tunnel_name_matches(&item.tunnel_name, name))
    {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_NAME_EXISTS",
            "A named tunnel with this name already exists. Use Tunnel instead of creating a duplicate.",
        ));
    }

    let args = vec![
        "tunnel".to_string(),
        "--origincert".to_string(),
        auth_cert_path.to_string_lossy().to_string(),
        "create".to_string(),
        name.to_string(),
    ];
    let output = run_cloudflared_capture(
        &binary_path,
        &args,
        Some(&state.workspace_dir),
        "PERSISTENT_TUNNEL_CREATE_FAILED",
        "DevNest could not create the named tunnel.",
    )?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_CREATE_FAILED",
            "DevNest could not create the named tunnel.",
            lower_text(&output),
        ));
    }

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let tunnel_id = parse_tunnel_id_from_text(&combined).ok_or_else(|| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_CREATE_FAILED",
            "Named tunnel was created, but DevNest could not determine the new tunnel ID.",
            combined.trim().to_string(),
        )
    })?;
    let credentials_source = persistent_tunnels::credentials_path_next_to_auth_cert(
        &auth_cert_path,
        &tunnel_id,
    )
    .or_else(|| persistent_tunnels::default_cloudflared_credentials_path(&tunnel_id))
    .ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_CREDENTIALS_MISSING",
            "cloudflared created the named tunnel, but DevNest could not find the credentials JSON afterward.",
        )
    })?;
    let managed_credentials_path =
        copy_credentials_into_managed_storage(&state, &credentials_source, &tunnel_id)?;
    merge_setup(
        &connection,
        Some(&auth_cert_path.to_string_lossy()),
        Some(&managed_credentials_path.to_string_lossy()),
        Some(&tunnel_id),
        Some(name),
        None,
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn import_persistent_tunnel_credentials(
    state: tauri::State<'_, AppState>,
) -> Result<Option<PersistentTunnelSetupStatus>, AppError> {
    let source_path = match FileDialog::new()
        .add_filter("cloudflared Credentials", &["json"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };

    let tunnel_id = parse_tunnel_id_from_credentials_document(&source_path)?;
    let managed_credentials_path =
        copy_credentials_into_managed_storage(&state, &source_path, &tunnel_id)?;
    let connection = connection_from_state(&state)?;
    merge_setup(
        &connection,
        None,
        Some(&managed_credentials_path.to_string_lossy()),
        Some(&tunnel_id),
        None,
        None,
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection).map(Some)
}

#[tauri::command]
pub fn list_available_persistent_named_tunnels(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PersistentTunnelNamedTunnelSummary>, AppError> {
    let connection = connection_from_state(&state)?;
    discover_named_tunnels(&connection, &state)
}

#[tauri::command]
pub fn select_persistent_named_tunnel(
    input: SelectPersistentNamedTunnelInput,
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let named_tunnel = discover_named_tunnels(&connection, &state)?
        .into_iter()
        .find(|item| item.tunnel_id == input.tunnel_id)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_NOT_FOUND",
                "DevNest could not find that named tunnel in the current Cloudflare account or managed credentials store.",
            )
        })?;
    let credentials_path = named_tunnel
        .credentials_path
        .clone()
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_CREDENTIALS_MISSING",
                "DevNest found the named tunnel, but no credentials JSON is managed for it yet. Import that tunnel's credentials file before using it.",
            )
        })?;
    let managed_credentials_path = copy_credentials_into_managed_storage(
        &state,
        Path::new(&credentials_path),
        &named_tunnel.tunnel_id,
    )?;

    merge_setup(
        &connection,
        None,
        Some(&managed_credentials_path.to_string_lossy()),
        Some(&named_tunnel.tunnel_id),
        Some(&named_tunnel.tunnel_name),
        None,
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn delete_persistent_named_tunnel(
    tunnel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let setup =
        PersistentTunnelSetupRepository::get(&connection, &PersistentTunnelProvider::Cloudflared)?;
    let named_tunnel = discover_named_tunnels(&connection, &state)?
        .into_iter()
        .find(|item| item.tunnel_id == tunnel_id)
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_NOT_FOUND",
                "DevNest could not find that named tunnel in the current Cloudflare account or managed credentials store.",
            )
        })?;
    let active_project_ids = active_persistent_tunnel_project_ids(&connection, &state)?;
    let deleting_selected = setup
        .as_ref()
        .and_then(|item| item.tunnel_id.as_ref())
        .map(|current| current == &named_tunnel.tunnel_id)
        .unwrap_or(false);

    if deleting_selected && !active_project_ids.is_empty() {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_IN_USE",
            "Stop or delete the hostname for all projects using the selected shared tunnel before deleting it.",
        ));
    }

    let (binary_path, auth_cert_path) = cloudflared_list_runtime(&connection).ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_AUTH_MISSING",
            "Connect Cloudflare or import the cloudflared cert before deleting a named tunnel.",
        )
    })?;
    let args = vec![
        "tunnel".to_string(),
        "--origincert".to_string(),
        auth_cert_path.to_string_lossy().to_string(),
        "delete".to_string(),
        named_tunnel.tunnel_id.clone(),
    ];
    let output = run_cloudflared_capture(
        &binary_path,
        &args,
        Some(&state.workspace_dir),
        "PERSISTENT_TUNNEL_DELETE_FAILED",
        "DevNest could not delete the named tunnel.",
    )?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PERSISTENT_TUNNEL_DELETE_FAILED",
            "DevNest could not delete the named tunnel.",
            lower_text(&output),
        ));
    }

    if deleting_selected {
        stop_shared_persistent_tunnel_process(&state)?;
        let preserved_setup = setup.map(|current| PersistentTunnelManagedSetup {
            provider: current.provider,
            auth_cert_path: current.auth_cert_path,
            credentials_path: None,
            tunnel_id: None,
            tunnel_name: None,
            default_hostname_zone: current.default_hostname_zone,
            created_at: current.created_at,
            updated_at: now_iso().unwrap_or_default(),
        });
        overwrite_setup(&connection, preserved_setup)?;
    }

    if let Some(credentials_path) = named_tunnel.credentials_path.as_deref() {
        let managed_credentials_dir =
            managed_persistent_tunnel_credentials_dir(&state.workspace_dir);
        let credentials_path = PathBuf::from(credentials_path);
        if credentials_path.starts_with(&managed_credentials_dir) {
            remove_file_if_exists(
                &credentials_path,
                "PERSISTENT_TUNNEL_DELETE_FAILED",
                "DevNest deleted the named tunnel, but could not remove the managed credentials file.",
            )?;
        }
    }

    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn disconnect_persistent_tunnel_provider(
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    if !active_persistent_tunnel_project_ids(&connection, &state)?.is_empty() {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_IN_USE",
            "Stop or delete the hostname for all projects using the shared persistent tunnel before disconnecting Cloudflare from DevNest.",
        ));
    }

    let current_setup =
        PersistentTunnelSetupRepository::get(&connection, &PersistentTunnelProvider::Cloudflared)?;
    stop_shared_persistent_tunnel_process(&state)?;
    {
        let mut tunnels = state
            .project_persistent_tunnels
            .lock()
            .map_err(|_| mutex_error())?;
        tunnels.clear();
    }
    if let Some(credentials_path) = current_setup
        .as_ref()
        .and_then(|setup| setup.credentials_path.as_ref())
    {
        let managed_credentials_dir =
            managed_persistent_tunnel_credentials_dir(&state.workspace_dir);
        let credentials_path = PathBuf::from(credentials_path);
        if credentials_path.starts_with(&managed_credentials_dir) {
            remove_file_if_exists(
                &credentials_path,
                "PERSISTENT_TUNNEL_DISCONNECT_FAILED",
                "DevNest disconnected Cloudflare, but could not remove the managed named tunnel credentials.",
            )?;
        }
    }
    overwrite_setup(&connection, None)?;
    remove_file_if_exists(
        &managed_persistent_tunnel_auth_cert_path(&state.workspace_dir),
        "PERSISTENT_TUNNEL_DISCONNECT_FAILED",
        "DevNest disconnected Cloudflare, but could not remove the managed auth cert.",
    )?;
    remove_file_if_exists(
        &managed_persistent_tunnel_config_path(&state.workspace_dir),
        "PERSISTENT_TUNNEL_DISCONNECT_FAILED",
        "DevNest disconnected Cloudflare, but could not remove the managed shared tunnel config.",
    )?;
    remove_file_if_exists(
        &managed_persistent_tunnel_log_path(&state.workspace_dir),
        "PERSISTENT_TUNNEL_DISCONNECT_FAILED",
        "DevNest disconnected Cloudflare, but could not remove the managed tunnel log.",
    )?;

    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn update_persistent_tunnel_setup(
    input: UpdatePersistentTunnelSetupInput,
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let current =
        PersistentTunnelSetupRepository::get(&connection, &PersistentTunnelProvider::Cloudflared)?;

    let setup = PersistentTunnelManagedSetup {
        provider: PersistentTunnelProvider::Cloudflared,
        auth_cert_path: current
            .as_ref()
            .and_then(|item| item.auth_cert_path.clone()),
        credentials_path: current
            .as_ref()
            .and_then(|item| item.credentials_path.clone()),
        tunnel_id: current.as_ref().and_then(|item| item.tunnel_id.clone()),
        tunnel_name: current.as_ref().and_then(|item| item.tunnel_name.clone()),
        default_hostname_zone: input.default_hostname_zone.clone(),
        created_at: current
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now_iso().unwrap_or_default()),
        updated_at: now_iso()?,
    };
    overwrite_setup(&connection, Some(setup))?;
    persistent_tunnels::persistent_tunnel_setup_status(&connection)
}

#[tauri::command]
pub fn get_project_persistent_hostname(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectPersistentHostname>, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectPersistentHostnameRepository::get_by_project(&connection, &project_id)
}

#[tauri::command]
pub async fn apply_project_persistent_hostname(
    input: ApplyProjectPersistentHostnameInput,
    app: tauri::AppHandle,
) -> Result<ApplyProjectPersistentHostnameResult, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        apply_project_persistent_hostname_internal(
            &input.project_id,
            input.hostname.as_deref(),
            &state,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent hostname apply did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub fn upsert_project_persistent_hostname(
    input: UpsertProjectPersistentHostnameInput,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPersistentHostname, AppError> {
    let connection = connection_from_state(&state)?;
    if active_persistent_tunnel_exists(&connection, &state, &input.project_id)? {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_IN_USE",
            "Stop the persistent tunnel before changing the reserved hostname for this project.",
        ));
    }
    let hostname = ProjectPersistentHostnameRepository::upsert(
        &connection,
        &input.project_id,
        &PersistentTunnelProvider::Cloudflared,
        &input.hostname,
    )?;
    refresh_project_server_aliases(&connection, &state, &input.project_id)?;
    Ok(hostname)
}

#[tauri::command]
pub async fn delete_project_persistent_hostname(
    project_id: String,
    app: tauri::AppHandle,
) -> Result<DeleteProjectPersistentHostnameResult, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        delete_project_persistent_hostname_internal(&project_id, &state)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent hostname delete did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub fn remove_project_persistent_hostname(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    delete_project_persistent_hostname_internal(&project_id, &state).map(|_| true)
}

#[tauri::command]
pub fn get_project_persistent_tunnel_state(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectPersistentTunnelState>, AppError> {
    let connection = connection_from_state(&state)?;
    project_persistent_tunnel_state(&connection, &state, &project_id)
}

#[tauri::command]
pub async fn publish_project_persistent_tunnel(
    project_id: String,
    app: tauri::AppHandle,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        start_project_persistent_tunnel_internal(&project_id, &state, true)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent project publish did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn start_project_persistent_tunnel(
    project_id: String,
    app: tauri::AppHandle,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        start_project_persistent_tunnel_internal(&project_id, &state, false)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent tunnel start did not finish cleanly.",
            error.to_string(),
        )
    })?
}

pub(crate) fn stop_project_persistent_tunnel_internal(
    project_id: &str,
    state: &AppState,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let previous = remove_project_persistent_tunnel_state(state, project_id)?;
    let connection = connection_from_state(&state)?;
    let mut remaining_ids = tracked_active_persistent_tunnel_project_ids(state)?;
    remaining_ids.retain(|value| value != project_id);
    if remaining_ids.is_empty() {
        stop_shared_persistent_tunnel_process(state)?;
    } else {
        let _ = restart_shared_persistent_tunnel_for_projects(&connection, state, &remaining_ids)?;
    }

    if let Some(mut existing) = previous {
        existing.status = PersistentTunnelStatus::Stopped;
        existing.updated_at = now_iso()?;
        existing.details = Some("The persistent public tunnel is stopped.".to_string());
        return Ok(existing);
    }

    let project = ProjectRepository::get(&connection, project_id)?;
    let hostname = ProjectPersistentHostnameRepository::get_by_project(&connection, project_id)?
        .map(|item| item.hostname)
        .unwrap_or_else(|| "not-configured".to_string());

    Ok(ProjectPersistentTunnelState {
        project_id: project_id.to_string(),
        provider: PersistentTunnelProvider::Cloudflared,
        status: PersistentTunnelStatus::Stopped,
        hostname: hostname.clone(),
        local_url: project_display_url(&project),
        public_url: persistent_public_url(&hostname),
        log_path: managed_persistent_tunnel_log_path(&state.workspace_dir)
            .to_string_lossy()
            .to_string(),
        binary_path: None,
        tunnel_id: None,
        credentials_path: None,
        updated_at: now_iso()?,
        details: Some("The persistent public tunnel is stopped.".to_string()),
    })
}

#[tauri::command]
pub async fn stop_project_persistent_tunnel(
    project_id: String,
    app: tauri::AppHandle,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        stop_project_persistent_tunnel_internal(&project_id, &state)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent tunnel stop did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn unpublish_project_persistent_tunnel(
    project_id: String,
    app: tauri::AppHandle,
) -> Result<ProjectPersistentTunnelState, AppError> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let previous = remove_project_persistent_tunnel_state(&state, &project_id)?;
        let connection = connection_from_state(&state)?;
        let project = ProjectRepository::get(&connection, &project_id)?;
        let hostname_record =
            ProjectPersistentHostnameRepository::get_by_project(&connection, &project_id)?;
        let hostname = hostname_record
            .as_ref()
            .map(|item| item.hostname.clone())
            .or_else(|| previous.as_ref().map(|item| item.hostname.clone()))
            .ok_or_else(|| {
                AppError::new_validation(
                    "PERSISTENT_TUNNEL_NOT_PUBLISHED",
                    "This project does not have a published persistent hostname to remove yet.",
                )
            })?;

        let mut remaining_ids = tracked_active_persistent_tunnel_project_ids(&state)?;
        remaining_ids.retain(|value| value != &project_id);
        if remaining_ids.is_empty() {
            stop_shared_persistent_tunnel_process(&state)?;
        } else {
            let _ =
                restart_shared_persistent_tunnel_for_projects(&connection, &state, &remaining_ids)?;
        }

        Ok(ProjectPersistentTunnelState {
            project_id,
            provider: PersistentTunnelProvider::Cloudflared,
            status: PersistentTunnelStatus::Stopped,
            hostname: hostname.clone(),
            local_url: project_display_url(&project),
            public_url: persistent_public_url(&hostname),
            log_path: managed_persistent_tunnel_log_path(&state.workspace_dir)
                .to_string_lossy()
                .to_string(),
            binary_path: None,
            tunnel_id: None,
            credentials_path: None,
            updated_at: now_iso()?,
            details: Some(
                "Project was unpublished. DevNest removed its hostname from the shared named tunnel ingress and kept the reserved hostname for later reuse."
                    .to_string(),
            ),
        })
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_JOIN_FAILED",
            "Persistent tunnel unpublish did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub fn open_project_persistent_tunnel_url(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let Some(tunnel) = project_persistent_tunnel_state(&connection, &state, &project_id)? else {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_NOT_RUNNING",
            "Start or publish the persistent tunnel first so DevNest has a stable public URL to open.",
        ));
    };

    if !matches!(tunnel.status, PersistentTunnelStatus::Running) {
        return Err(AppError::new_validation(
            "PERSISTENT_TUNNEL_URL_NOT_READY",
            "Persistent tunnel is not running yet.",
        ));
    }

    open_url_in_default_browser(&tunnel.public_url)?;
    Ok(true)
}

#[tauri::command]
pub fn inspect_project_persistent_tunnel_health(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PersistentTunnelHealthReport, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let hostname = ProjectPersistentHostnameRepository::get_by_project(&connection, &project_id)?
        .map(|item| item.hostname);
    let persistent_tunnel = project_persistent_tunnel_state(&connection, &state, &project_id)?;
    let setup = persistent_tunnels::persistent_tunnel_setup_status(&connection)?;
    let server_service = project_server_service_name(&project.server_type);
    let service_state =
        service_manager::get_service_status(&connection, &state, server_service.clone())?;
    let config_path =
        managed_config_output_path(&state.workspace_dir, &project.server_type, &project.domain);
    let config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let mut checks = Vec::new();

    checks.push(PersistentTunnelHealthCheck {
        code: "setup".to_string(),
        label: "Named Tunnel Setup".to_string(),
        status: if setup.ready {
            PersistentTunnelStatus::Running
        } else {
            PersistentTunnelStatus::Error
        },
        message: if setup.ready {
            "cloudflared, credentials, tunnel identity are ready.".to_string()
        } else {
            setup.guidance.unwrap_or(setup.details)
        },
    });

    checks.push(PersistentTunnelHealthCheck {
        code: "hostname".to_string(),
        label: "Reserved Hostname".to_string(),
        status: if let Some(current_hostname) = hostname.as_deref() {
            if !current_hostname.is_empty() {
                PersistentTunnelStatus::Running
            } else {
                PersistentTunnelStatus::Error
            }
        } else if setup.default_hostname_zone.is_some() {
            PersistentTunnelStatus::Starting
        } else {
            PersistentTunnelStatus::Error
        },
        message: hostname
            .as_ref()
            .map(|value| format!("{value} is reserved for this project."))
            .unwrap_or_else(|| {
                if let Some(zone) = setup.default_hostname_zone {
                    format!(
                        "No explicit hostname is reserved yet, but DevNest can auto-generate one under {zone} when you publish."
                    )
                } else {
                    "No stable public hostname is reserved for this project yet.".to_string()
                }
            }),
    });

    let alias_synced = hostname
        .as_ref()
        .map(|value| config_text.contains(value))
        .unwrap_or(false);
    checks.push(PersistentTunnelHealthCheck {
        code: "config".to_string(),
        label: "Managed Config Alias".to_string(),
        status: if alias_synced {
            PersistentTunnelStatus::Running
        } else {
            PersistentTunnelStatus::Error
        },
        message: if alias_synced {
            "Managed Apache, Nginx, or FrankenPHP config includes the stable hostname.".to_string()
        } else {
            "Managed config does not currently include the reserved hostname alias.".to_string()
        },
    });

    checks.push(PersistentTunnelHealthCheck {
        code: "origin".to_string(),
        label: "Origin Service".to_string(),
        status: if matches!(service_state.status, ServiceStatus::Running) {
            PersistentTunnelStatus::Running
        } else if matches!(service_state.status, ServiceStatus::Error) {
            PersistentTunnelStatus::Error
        } else {
            PersistentTunnelStatus::Stopped
        },
        message: if matches!(service_state.status, ServiceStatus::Running) {
            format!(
                "{} is running and can serve the project origin.",
                server_service.display_name()
            )
        } else {
            format!(
                "{} is not running yet, so the persistent tunnel origin cannot respond.",
                server_service.display_name()
            )
        },
    });

    checks.push(PersistentTunnelHealthCheck {
        code: "process".to_string(),
        label: "Persistent Tunnel Process".to_string(),
        status: persistent_tunnel
            .as_ref()
            .map(|item| item.status.clone())
            .unwrap_or(PersistentTunnelStatus::Stopped),
        message: persistent_tunnel
            .as_ref()
            .and_then(|item| item.details.clone())
            .unwrap_or_else(|| "Persistent tunnel process is not running.".to_string()),
    });

    let dns_status = if let Some(current_hostname) = hostname.as_deref() {
        if dns_resolves(current_hostname) {
            PersistentTunnelStatus::Running
        } else {
            PersistentTunnelStatus::Starting
        }
    } else {
        PersistentTunnelStatus::Stopped
    };
    checks.push(PersistentTunnelHealthCheck {
        code: "dns".to_string(),
        label: "Public DNS".to_string(),
        status: dns_status.clone(),
        message: if let Some(current_hostname) = hostname.as_deref() {
            if matches!(dns_status, PersistentTunnelStatus::Running) {
                format!("{current_hostname} resolves from the current machine.")
            } else {
                format!(
                    "{current_hostname} does not resolve yet from the current machine. DNS propagation or hostname routing may still be pending."
                )
            }
        } else {
            "DNS cannot be checked until a stable hostname is reserved.".to_string()
        },
    });

    let overall_status = worst_health_status(&checks);

    Ok(PersistentTunnelHealthReport {
        project_id,
        hostname,
        overall_status,
        checks,
        updated_at: now_iso()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        PersistentTunnelRoute, copy_file_into_place, dns_route_requires_overwrite,
        persistent_tunnel_run_args, shared_persistent_tunnel_config_text,
    };
    use crate::core::persistent_tunnels::PersistentTunnelRuntime;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn builds_shared_persistent_tunnel_config_for_multiple_projects() {
        let runtime = PersistentTunnelRuntime {
            binary_path: PathBuf::from(r"C:\cloudflared.exe"),
            auth_cert_path: PathBuf::from(r"C:\cert.pem"),
            credentials_path: PathBuf::from(r"C:\tunnel.json"),
            tunnel_id: "demo-tunnel".to_string(),
            default_hostname_zone: Some("a2zone.space".to_string()),
        };
        let config = shared_persistent_tunnel_config_text(
            &runtime,
            &[
                PersistentTunnelRoute {
                    project_id: "project-1".to_string(),
                    hostname: "vietruyen.a2zone.space".to_string(),
                    project_domain: "vietruyen.test".to_string(),
                    origin_url: "https://127.0.0.1:443".to_string(),
                    local_url: "https://vietruyen.test".to_string(),
                    ssl_enabled: true,
                },
                PersistentTunnelRoute {
                    project_id: "project-2".to_string(),
                    hostname: "datlichhoc.a2zone.space".to_string(),
                    project_domain: "datlichhoc.test".to_string(),
                    origin_url: "http://127.0.0.1:80".to_string(),
                    local_url: "http://datlichhoc.test".to_string(),
                    ssl_enabled: false,
                },
            ],
        );

        assert!(config.contains("hostname: 'vietruyen.a2zone.space'"));
        assert!(config.contains("hostname: 'datlichhoc.a2zone.space'"));
        assert!(config.contains("originServerName: 'vietruyen.test'"));
        assert!(config.contains("service: 'http://127.0.0.1:80'"));
        assert!(config.contains("service: http_status:404"));
    }

    #[test]
    fn runs_shared_persistent_tunnel_with_config_file() {
        let runtime = PersistentTunnelRuntime {
            binary_path: PathBuf::from(r"C:\cloudflared.exe"),
            auth_cert_path: PathBuf::from(r"C:\cert.pem"),
            credentials_path: PathBuf::from(r"C:\tunnel.json"),
            tunnel_id: "demo-tunnel".to_string(),
            default_hostname_zone: Some("a2zone.space".to_string()),
        };
        let args = persistent_tunnel_run_args(
            &runtime,
            &PathBuf::from(r"C:\config.yml"),
            &PathBuf::from(r"C:\persistent.log"),
        );

        assert!(
            args.windows(2)
                .any(|window| window == ["--config", r"C:\config.yml"])
        );
        assert_eq!(args.last().map(String::as_str), Some("demo-tunnel"));
    }

    #[test]
    fn reuses_managed_credentials_file_without_copying_onto_itself() {
        let temp_dir =
            std::env::temp_dir().join(format!("devnest-persistent-tunnel-copy-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let credentials_path = temp_dir.join("demo.json");
        fs::write(&credentials_path, br#"{"TunnelID":"demo"}"#)
            .expect("credentials file should exist");

        let copied = copy_file_into_place(&credentials_path, &credentials_path)
            .expect("same-file copy should be treated as reuse");

        assert_eq!(copied, credentials_path);
        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn detects_dns_route_conflicts_that_need_overwrite() {
        assert!(dns_route_requires_overwrite(
            "Failed to add route: code: 1003, reason: Failed to create record demo.example.com with err An A, AAAA, or CNAME record with that host already exists."
        ));
        assert!(dns_route_requires_overwrite(
            "hostname already configured and exists and points to another tunnel"
        ));
        assert!(!dns_route_requires_overwrite(
            "permission denied while calling cloudflare api"
        ));
    }
}
