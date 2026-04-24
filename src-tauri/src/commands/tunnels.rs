use crate::core::runtime_registry;
use crate::core::service_manager;
use crate::error::AppError;
use crate::models::optional_tool::OptionalToolType;
use crate::models::project::{Project, ServerType};
use crate::models::service::{ServiceName, ServiceStatus};
use crate::models::tunnel::{ProjectTunnelState, TunnelProvider, TunnelStatus};
use crate::state::{AppState, ManagedServiceProcess};
use crate::storage::repositories::{ProjectRepository, now_iso};
use crate::utils::process::{configure_background_command, kill_process_tree};
use crate::utils::windows::open_url_in_default_browser;
use rusqlite::Connection;
use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn tunnel_process_key(project_id: &str) -> String {
    format!("tunnel:{project_id}")
}

fn project_tunnel_log_path(workspace_dir: &Path, project_id: &str) -> PathBuf {
    workspace_dir
        .join("logs")
        .join("tunnels")
        .join(format!("{project_id}.log"))
}

fn project_origin_url(_domain: &str, ssl_enabled: bool) -> String {
    let scheme = if ssl_enabled { "https" } else { "http" };
    let port = if ssl_enabled { 443 } else { 80 };
    format!("{scheme}://127.0.0.1:{port}")
}

fn project_display_url(domain: &str, ssl_enabled: bool) -> String {
    let scheme = if ssl_enabled { "https" } else { "http" };
    format!("{scheme}://{}", domain.trim().to_lowercase())
}

fn tunnel_origin_args(domain: &str, ssl_enabled: bool) -> Vec<String> {
    let mut args = Vec::new();

    if ssl_enabled {
        args.push("--no-tls-verify".to_string());
        args.push("--origin-server-name".to_string());
        args.push(domain.trim().to_ascii_lowercase());
    }

    args
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

fn project_server_service_name(project: &Project) -> ServiceName {
    match project.server_type {
        ServerType::Apache => ServiceName::Apache,
        ServerType::Nginx => ServiceName::Nginx,
        ServerType::Frankenphp => ServiceName::Frankenphp,
    }
}

fn ensure_project_origin_service(
    connection: &Connection,
    state: &AppState,
    project: &Project,
) -> Result<(), AppError> {
    let service = project_server_service_name(project);
    let service_state = service_manager::get_service_status(connection, state, service.clone())?;
    if !matches!(service_state.status, ServiceStatus::Running) {
        let _ = service_manager::start_service(connection, state, service)?;
    }

    Ok(())
}

fn sync_public_host_alias(
    connection: &Connection,
    state: &AppState,
    project: &Project,
    public_url: &str,
) -> Result<(), AppError> {
    let Some(public_host) = public_tunnel_host(public_url) else {
        return Ok(());
    };

    if public_host == project.domain.to_ascii_lowercase() {
        return Ok(());
    }

    let _ =
        service_manager::restart_service(connection, state, project_server_service_name(project))?;
    Ok(())
}

fn maybe_sync_public_host_alias(
    connection: &Connection,
    state: &AppState,
    project: &Project,
    tunnel: &mut ProjectTunnelState,
) -> Result<(), AppError> {
    if tunnel.public_host_alias_synced {
        return Ok(());
    }

    let Some(public_url) = tunnel.public_url.clone() else {
        return Ok(());
    };

    sync_public_host_alias(connection, state, project, &public_url)?;
    tunnel.public_host_alias_synced = true;
    tunnel.updated_at = now_iso()?;
    tunnel.details = Some(format!(
        "Public tunnel is active for {} through the optional cloudflared integration.",
        tunnel.local_url
    ));
    store_project_tunnel_state(state, Some(tunnel.clone()))?;

    Ok(())
}

fn tunnel_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(value) = env::var("DEVNEST_TUNNEL_BIN") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed));
        }
    }

    candidates.push(PathBuf::from(
        r"C:\Program Files\cloudflared\cloudflared.exe",
    ));
    candidates.push(PathBuf::from(r"C:\cloudflared\cloudflared.exe"));
    candidates.push(PathBuf::from("cloudflared"));

    candidates
}

fn resolve_tunnel_binary() -> PathBuf {
    tunnel_binary_candidates()
        .into_iter()
        .find(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("cloudflared"))
}

fn resolve_managed_tunnel_binary(connection: &Connection) -> Result<Option<PathBuf>, AppError> {
    runtime_registry::resolve_optional_tool_path_from_registry(
        connection,
        &OptionalToolType::Cloudflared,
    )
}

fn tunnel_binary_label(binary_path: &Path) -> String {
    binary_path.to_string_lossy().to_string()
}

fn extract_public_tunnel_url(log_content: &str) -> Option<String> {
    for token in log_content.split_whitespace() {
        let trimmed = token.trim_matches(|character: char| {
            matches!(
                character,
                '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        });

        let normalized = trimmed.to_ascii_lowercase();
        if normalized.starts_with("https://")
            && (normalized.contains(".trycloudflare.com")
                || normalized.starts_with("https://trycloudflare.com"))
        {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn read_public_tunnel_url(log_path: &Path) -> Option<String> {
    fs::read_to_string(log_path)
        .ok()
        .and_then(|content| extract_public_tunnel_url(&content))
}

fn store_project_tunnel_state(
    state: &AppState,
    tunnel: Option<ProjectTunnelState>,
) -> Result<(), AppError> {
    let mut tunnels = state.project_tunnels.lock().map_err(|_| {
        AppError::new_validation(
            "TUNNEL_STATE_LOCK_FAILED",
            "DevNest could not update the current tunnel state.",
        )
    })?;

    if let Some(next) = tunnel {
        tunnels.insert(next.project_id.clone(), next);
    }

    Ok(())
}

fn remove_project_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectTunnelState>, AppError> {
    let mut tunnels = state.project_tunnels.lock().map_err(|_| {
        AppError::new_validation(
            "TUNNEL_STATE_LOCK_FAILED",
            "DevNest could not update the current tunnel state.",
        )
    })?;

    Ok(tunnels.remove(project_id))
}

fn load_project_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectTunnelState>, AppError> {
    let tunnels = state.project_tunnels.lock().map_err(|_| {
        AppError::new_validation(
            "TUNNEL_STATE_LOCK_FAILED",
            "DevNest could not read the current tunnel state.",
        )
    })?;

    Ok(tunnels.get(project_id).cloned())
}

fn sync_project_tunnel_state(
    state: &AppState,
    project_id: &str,
) -> Result<Option<ProjectTunnelState>, AppError> {
    let Some(mut current) = load_project_tunnel_state(state, project_id)? else {
        return Ok(None);
    };

    let key = tunnel_process_key(project_id);
    let mut processes = state.managed_processes.lock().map_err(|_| {
        AppError::new_validation(
            "TUNNEL_STATE_LOCK_FAILED",
            "DevNest could not inspect the managed tunnel process state.",
        )
    })?;

    let mut remove_process = false;
    let next_status = if let Some(process) = processes.get_mut(&key) {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                remove_process = true;
                current.details = Some(format!(
                    "The tunnel process exited before DevNest could keep it alive (exit code {:?}).",
                    status.code()
                ));
                TunnelStatus::Error
            }
            Ok(None) => {
                current.public_url = read_public_tunnel_url(Path::new(&current.log_path));
                if current.public_url.is_some() {
                    current.details = Some(format!(
                        "Public tunnel is active for {} through the optional cloudflared integration.",
                        current.local_url
                    ));
                    TunnelStatus::Running
                } else {
                    current.details = Some(format!(
                        "Tunnel is starting for {}. DevNest is waiting for cloudflared to publish the public URL.",
                        current.local_url
                    ));
                    TunnelStatus::Starting
                }
            }
            Err(error) => {
                remove_process = true;
                current.details = Some(error.to_string());
                TunnelStatus::Error
            }
        }
    } else {
        current.public_url = read_public_tunnel_url(Path::new(&current.log_path));
        if current.public_url.is_some() {
            current.details = Some(
                "Tunnel metadata still exists, but the process is no longer running.".to_string(),
            );
        }
        TunnelStatus::Stopped
    };

    if remove_process {
        processes.remove(&key);
    }

    drop(processes);

    current.status = next_status;
    current.updated_at = now_iso()?;
    store_project_tunnel_state(state, Some(current.clone()))?;
    Ok(Some(current))
}

#[tauri::command]
pub fn get_project_tunnel_state(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectTunnelState>, AppError> {
    let mut tunnel = sync_project_tunnel_state(&state, &project_id)?;
    if let Some(current) = tunnel.as_mut() {
        if current.public_url.is_some() && !current.public_host_alias_synced {
            let connection = connection_from_state(&state)?;
            let project = ProjectRepository::get(&connection, &project_id)?;
            maybe_sync_public_host_alias(&connection, &state, &project, current)?;
        }
    }

    Ok(tunnel)
}

pub(crate) fn start_project_tunnel_internal(
    project_id: &str,
    state: &AppState,
) -> Result<ProjectTunnelState, AppError> {
    if let Some(existing) = sync_project_tunnel_state(&state, &project_id)? {
        if matches!(
            existing.status,
            TunnelStatus::Starting | TunnelStatus::Running
        ) {
            return Ok(existing);
        }
    }

    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, project_id)?;
    ensure_project_origin_service(&connection, &state, &project)?;
    let origin_url = project_origin_url(&project.domain, project.ssl_enabled);
    let local_url = project_display_url(&project.domain, project.ssl_enabled);
    let binary_path =
        resolve_managed_tunnel_binary(&connection)?.unwrap_or_else(resolve_tunnel_binary);
    let log_path = project_tunnel_log_path(&state.workspace_dir, project_id);

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "TUNNEL_START_FAILED",
                "DevNest could not prepare the managed tunnel log folder.",
                error.to_string(),
            )
        })?;
    }
    fs::write(&log_path, "").map_err(|error| {
        AppError::with_details(
            "TUNNEL_START_FAILED",
            "DevNest could not reset the managed tunnel log file.",
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

    let mut command = Command::new(&binary_path);
    command
        .args([
            "tunnel",
            "--url",
            &origin_url,
            "--logfile",
            &log_path.to_string_lossy(),
            "--no-autoupdate",
        ])
        .args(tunnel_origin_args(&project.domain, project.ssl_enabled))
        .current_dir(Path::new(&project.path))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    configure_background_command(&mut command);

    let child = command.spawn().map_err(|error| {
        let human_message = if error.kind() == std::io::ErrorKind::NotFound {
            "cloudflared was not found. Install it first or set DEVNEST_TUNNEL_BIN."
        } else {
            "DevNest could not start the optional project tunnel."
        };

        AppError::with_details("TUNNEL_START_FAILED", human_message, error.to_string())
    })?;

    let pid = child.id();
    state
        .managed_processes
        .lock()
        .map_err(|_| {
            AppError::new_validation(
                "TUNNEL_STATE_LOCK_FAILED",
                "DevNest could not track the new tunnel process.",
            )
        })?
        .insert(
            tunnel_process_key(project_id),
            ManagedServiceProcess {
                pid,
                child,
                log_path: log_path.clone(),
            },
        );

    let initial_state = ProjectTunnelState {
        project_id: project_id.to_string(),
        provider: TunnelProvider::Cloudflared,
        status: TunnelStatus::Starting,
        local_url: local_url.clone(),
        public_url: None,
        public_host_alias_synced: false,
        log_path: log_path.to_string_lossy().to_string(),
        binary_path: Some(tunnel_binary_label(&binary_path)),
        updated_at: now_iso()?,
        details: Some(format!(
            "cloudflared started for {}. Internal origin is {} while DevNest waits for the public tunnel URL.",
            local_url, origin_url
        )),
    };
    store_project_tunnel_state(state, Some(initial_state))?;

    let mut tunnel = None;
    for _ in 0..12 {
        thread::sleep(Duration::from_millis(700));
        let next = sync_project_tunnel_state(state, project_id)?.ok_or_else(|| {
            AppError::new_validation(
                "TUNNEL_START_FAILED",
                "Tunnel process started, but DevNest could not read back the tunnel state.",
            )
        })?;

        let should_stop_waiting = next.public_url.is_some()
            || matches!(next.status, TunnelStatus::Error | TunnelStatus::Stopped);
        tunnel = Some(next);
        if should_stop_waiting {
            break;
        }
    }

    let mut tunnel = tunnel
        .or_else(|| sync_project_tunnel_state(state, project_id).ok().flatten())
        .ok_or_else(|| {
            AppError::new_validation(
                "TUNNEL_START_FAILED",
                "Tunnel process started, but DevNest could not confirm the current tunnel state.",
            )
        })?;

    if tunnel.public_url.is_some() {
        maybe_sync_public_host_alias(&connection, &state, &project, &mut tunnel)?;
    }

    sync_project_tunnel_state(state, project_id)?.ok_or_else(|| {
        AppError::new_validation(
            "TUNNEL_START_FAILED",
            "Tunnel started, but DevNest could not confirm the final public host state.",
        )
    })
}

pub(crate) fn stop_project_tunnel_internal(
    project_id: &str,
    state: &AppState,
) -> Result<ProjectTunnelState, AppError> {
    let key = tunnel_process_key(project_id);
    let tracked = state
        .managed_processes
        .lock()
        .map_err(|_| {
            AppError::new_validation(
                "TUNNEL_STATE_LOCK_FAILED",
                "DevNest could not update the tunnel process state.",
            )
        })?
        .remove(&key);

    if let Some(mut process) = tracked {
        match process.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                kill_process_tree(process.pid)?;
                let _ = process.child.wait();
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "TUNNEL_STOP_FAILED",
                    "DevNest could not inspect the tunnel process before stopping it.",
                    error.to_string(),
                ));
            }
        }
    }

    let previous = remove_project_tunnel_state(state, project_id)?;
    let tunnel = if let Some(mut existing) = previous {
        let should_refresh_server_aliases = existing.public_url.is_some();
        existing.status = TunnelStatus::Stopped;
        existing.public_url = None;
        existing.public_host_alias_synced = false;
        existing.updated_at = now_iso()?;
        existing.details = Some("The optional public tunnel is stopped.".to_string());

        if should_refresh_server_aliases {
            let connection = connection_from_state(&state)?;
            let project = ProjectRepository::get(&connection, &existing.project_id)?;
            let service = project_server_service_name(&project);
            let service_state =
                service_manager::get_service_status(&connection, &state, service.clone())?;
            if matches!(service_state.status, ServiceStatus::Running) {
                let _ = service_manager::restart_service(&connection, &state, service)?;
            }
        }

        existing
    } else {
        let connection = connection_from_state(&state)?;
        let project = ProjectRepository::get(&connection, project_id)?;
        ProjectTunnelState {
            project_id: project_id.to_string(),
            provider: TunnelProvider::Cloudflared,
            status: TunnelStatus::Stopped,
            local_url: project_display_url(&project.domain, project.ssl_enabled),
            public_url: None,
            public_host_alias_synced: false,
            log_path: project_tunnel_log_path(&state.workspace_dir, &project.id)
                .to_string_lossy()
                .to_string(),
            binary_path: None,
            updated_at: now_iso()?,
            details: Some("The optional public tunnel is stopped.".to_string()),
        }
    };

    Ok(tunnel)
}

#[tauri::command]
pub fn start_project_tunnel(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectTunnelState, AppError> {
    start_project_tunnel_internal(&project_id, &state)
}

#[tauri::command]
pub fn stop_project_tunnel(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectTunnelState, AppError> {
    stop_project_tunnel_internal(&project_id, &state)
}

#[tauri::command]
pub fn open_project_tunnel_url(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let Some(tunnel) = sync_project_tunnel_state(&state, &project_id)? else {
        return Err(AppError::new_validation(
            "TUNNEL_NOT_RUNNING",
            "Start the optional tunnel first so DevNest has a public URL to open.",
        ));
    };

    let public_url = tunnel.public_url.ok_or_else(|| {
        AppError::new_validation(
            "TUNNEL_URL_NOT_READY",
            "Tunnel is starting, but the public URL is not ready yet.",
        )
    })?;

    open_url_in_default_browser(&public_url)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{extract_public_tunnel_url, project_origin_url, tunnel_origin_args};

    #[test]
    fn extracts_public_url_from_cloudflared_log_line() {
        let content =
            "INFO Quick Tunnel available at https://fancy-demo.trycloudflare.com metrics=ok";
        assert_eq!(
            extract_public_tunnel_url(content).as_deref(),
            Some("https://fancy-demo.trycloudflare.com")
        );
    }

    #[test]
    fn ignores_cloudflare_terms_link_and_prefers_trycloudflare_url() {
        let content = "INFO By using this service you accept https://www.cloudflare.com/website-terms/ and your quick tunnel is https://bright-lake.trycloudflare.com";
        assert_eq!(
            extract_public_tunnel_url(content).as_deref(),
            Some("https://bright-lake.trycloudflare.com")
        );
    }

    #[test]
    fn builds_https_origin_url_for_ssl_projects() {
        assert_eq!(
            project_origin_url("datlichhoc.test", true),
            "https://127.0.0.1:443"
        );
        assert_eq!(
            project_origin_url("datlichhoc.test", false),
            "http://127.0.0.1:80"
        );
    }

    #[test]
    fn adds_origin_flags_for_ssl_quick_tunnels() {
        assert_eq!(
            tunnel_origin_args("datlichhoc.test", false),
            Vec::<String>::new()
        );
        assert_eq!(
            tunnel_origin_args("datlichhoc.test", true),
            vec!["--no-tls-verify", "--origin-server-name", "datlichhoc.test"]
        );
    }
}
