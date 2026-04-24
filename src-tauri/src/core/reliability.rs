use crate::core::{config_generator, diagnostics, persistent_tunnels, ports, service_manager};
use crate::error::AppError;
use crate::models::project::{Project, ServerType};
use crate::models::reliability::{
    ActionPreflightReport, InspectorConfigSnapshot, InspectorRuntimeBinding,
    InspectorRuntimeSnapshot, ReliabilityAction, ReliabilityCheck, ReliabilityInspectorSnapshot,
    ReliabilityLayer, ReliabilityStatus, ReliabilityTransferResult, RepairWorkflow,
    RepairWorkflowInfo,
};
use crate::models::runtime::RuntimeType;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::models::tunnel::ProjectTunnelState;
use crate::state::AppState;
use crate::storage::repositories::{
    ProjectPersistentHostnameRepository, ProjectRepository, RuntimeVersionRepository, now_iso,
};
use crate::utils::paths::{managed_config_root, managed_persistent_tunnel_root, managed_ssl_root};
use crate::utils::windows::hosts_file_path;
use base64::Engine;
use rfd::FileDialog;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupFileEntry {
    relative_path: String,
    content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppMetadataBackupDocument {
    format_version: u32,
    exported_at: String,
    source: String,
    db_file_name: String,
    db_content_base64: String,
    workspace_files: Vec<BackupFileEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsBundleDocument {
    format_version: u32,
    exported_at: String,
    source: String,
    inspector: ReliabilityInspectorSnapshot,
    provision_preflight: ActionPreflightReport,
    publish_preflight: ActionPreflightReport,
    start_preflight: ActionPreflightReport,
    repair_workflows: Vec<RepairWorkflowInfo>,
    service_logs: Vec<DiagnosticsBundleLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsBundleLogEntry {
    service: String,
    content: String,
}

fn project_server_service_name(project: &Project) -> ServiceName {
    match project.server_type {
        ServerType::Apache => ServiceName::Apache,
        ServerType::Nginx => ServiceName::Nginx,
        ServerType::Frankenphp => ServiceName::Frankenphp,
    }
}

fn project_server_runtime_type(project: &Project) -> RuntimeType {
    match project.server_type {
        ServerType::Apache => RuntimeType::Apache,
        ServerType::Nginx => RuntimeType::Nginx,
        ServerType::Frankenphp => RuntimeType::Frankenphp,
    }
}

fn push_check(
    checks: &mut Vec<ReliabilityCheck>,
    code: &str,
    layer: ReliabilityLayer,
    status: ReliabilityStatus,
    blocking: bool,
    title: &str,
    message: String,
    suggestion: Option<String>,
) {
    checks.push(ReliabilityCheck {
        code: code.to_string(),
        layer,
        status,
        blocking,
        title: title.to_string(),
        message,
        suggestion,
    });
}

fn preflight_summary(
    action: &ReliabilityAction,
    checks: &[ReliabilityCheck],
    ready: bool,
) -> String {
    if ready {
        return match action {
            ReliabilityAction::ProvisionProject => {
                "Provisioning can continue. Config, runtime, and local domain checks passed."
                    .to_string()
            }
            ReliabilityAction::PublishPersistentDomain => {
                "Publish can continue. Persistent tunnel prerequisites look ready.".to_string()
            }
            ReliabilityAction::StartProjectRuntime => {
                "Runtime start can continue. DevNest found the required binaries and ports."
                    .to_string()
            }
            ReliabilityAction::RestoreAppMetadata => {
                "Metadata restore can continue. No managed processes are blocking the restore."
                    .to_string()
            }
        };
    }

    let blocking_count = checks.iter().filter(|check| check.blocking).count();
    if blocking_count == 0 {
        "Action can continue, but DevNest found warnings worth reviewing first.".to_string()
    } else {
        format!(
            "Action is blocked until {} reliability issue{} {} resolved.",
            blocking_count,
            if blocking_count == 1 { "" } else { "s" },
            if blocking_count == 1 { "is" } else { "are" }
        )
    }
}

fn matching_runtime_binding(
    connection: &Connection,
    runtime_type: &RuntimeType,
    preferred_version: Option<&str>,
) -> Result<InspectorRuntimeBinding, AppError> {
    let runtimes = RuntimeVersionRepository::list_by_type(connection, runtime_type)?;
    let selected = if let Some(version) = preferred_version {
        runtimes
            .iter()
            .find(|runtime| runtime.version.trim().starts_with(version.trim()))
            .cloned()
            .or_else(|| runtimes.iter().find(|runtime| runtime.is_active).cloned())
    } else {
        runtimes.iter().find(|runtime| runtime.is_active).cloned()
    };

    Ok(if let Some(runtime) = selected {
        let runtime_path = PathBuf::from(&runtime.path);
        let available = runtime_path.exists() && runtime_path.is_file();
        InspectorRuntimeBinding {
            kind: runtime.runtime_type.as_str().to_string(),
            version: Some(runtime.version),
            path: Some(runtime.path.clone()),
            active: runtime.is_active,
            available,
            details: if available {
                None
            } else {
                Some("Tracked runtime path is missing from disk.".to_string())
            },
        }
    } else {
        InspectorRuntimeBinding {
            kind: runtime_type.as_str().to_string(),
            version: None,
            path: None,
            active: false,
            available: false,
            details: Some("No tracked runtime is linked yet.".to_string()),
        }
    })
}

fn hosts_entry_present(domain: &str) -> bool {
    let hosts_path = hosts_file_path();
    fs::read_to_string(hosts_path)
        .ok()
        .map(|content| {
            content
                .to_ascii_lowercase()
                .contains(&domain.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

fn current_managed_process_count(state: &AppState) -> Result<usize, AppError> {
    let mut processes = state.managed_processes.lock().map_err(|_| {
        AppError::new_validation(
            "WORKSPACE_STATE_LOCK_FAILED",
            "DevNest could not inspect the managed process list.",
        )
    })?;
    let mut stale_keys = Vec::new();

    for (key, process) in processes.iter_mut() {
        match process.child.try_wait() {
            Ok(Some(_)) => stale_keys.push(key.clone()),
            Ok(None) => {}
            Err(error) => {
                return Err(AppError::with_details(
                    "WORKSPACE_STATE_INSPECT_FAILED",
                    "DevNest could not inspect a managed process before running restore preflight.",
                    error.to_string(),
                ));
            }
        }
    }

    for key in stale_keys {
        processes.remove(&key);
    }

    Ok(processes.len())
}

fn collect_workspace_files(
    root: &Path,
    workspace_dir: &Path,
    files: &mut Vec<BackupFileEntry>,
) -> Result<(), AppError> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        if path.is_dir() {
            collect_workspace_files(&path, workspace_dir, files)?;
            continue;
        }

        let relative = path.strip_prefix(workspace_dir).map_err(|error| {
            AppError::with_details(
                "WORKSPACE_BACKUP_FAILED",
                "DevNest could not normalize a workspace file for backup.",
                error.to_string(),
            )
        })?;
        let bytes = fs::read(&path).map_err(|error| {
            AppError::with_details(
                "WORKSPACE_BACKUP_FAILED",
                "DevNest could not read a workspace file for backup.",
                error.to_string(),
            )
        })?;
        files.push(BackupFileEntry {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }

    Ok(())
}

fn backup_document(state: &AppState) -> Result<AppMetadataBackupDocument, AppError> {
    let db_bytes = fs::read(&state.db_path).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_BACKUP_FAILED",
            "DevNest could not read the metadata database for backup.",
            error.to_string(),
        )
    })?;
    let mut workspace_files = Vec::new();
    for root in [
        managed_config_root(&state.workspace_dir),
        managed_ssl_root(&state.workspace_dir),
        managed_persistent_tunnel_root(&state.workspace_dir),
    ] {
        collect_workspace_files(&root, &state.workspace_dir, &mut workspace_files)?;
    }

    Ok(AppMetadataBackupDocument {
        format_version: 1,
        exported_at: now_iso()?,
        source: "DevNest".to_string(),
        db_file_name: state
            .db_path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "devnest.sqlite3".to_string()),
        db_content_base64: base64::engine::general_purpose::STANDARD.encode(db_bytes),
        workspace_files,
    })
}

fn parse_backup_document(path: &Path) -> Result<AppMetadataBackupDocument, AppError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_RESTORE_FAILED",
            "DevNest could not read the selected app metadata backup file.",
            error.to_string(),
        )
    })?;
    let document: AppMetadataBackupDocument = serde_json::from_str(&raw).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_RESTORE_FAILED",
            "The selected app metadata backup file is not valid JSON.",
            error.to_string(),
        )
    })?;

    if document.format_version != 1 {
        return Err(AppError::new_validation(
            "APP_METADATA_BACKUP_UNSUPPORTED",
            "This DevNest metadata backup version is not supported by the current build.",
        ));
    }

    Ok(document)
}

fn validated_restore_path(workspace_dir: &Path, relative_path: &str) -> Result<PathBuf, AppError> {
    let normalized = relative_path.replace('\\', "/");
    if normalized.contains("..") || normalized.starts_with('/') {
        return Err(AppError::new_validation(
            "APP_METADATA_RESTORE_FAILED",
            "Backup contains an invalid relative workspace path.",
        ));
    }

    let path = workspace_dir.join(&normalized);
    let starts_in_workspace = path.starts_with(workspace_dir);
    if !starts_in_workspace {
        return Err(AppError::new_validation(
            "APP_METADATA_RESTORE_FAILED",
            "Backup contains a file outside the DevNest workspace.",
        ));
    }

    Ok(path)
}

pub fn available_repair_workflows() -> Vec<RepairWorkflowInfo> {
    vec![
        RepairWorkflowInfo {
            workflow: RepairWorkflow::Project,
            title: "Repair Project".to_string(),
            summary:
                "Rebuild managed config, reapply the local domain, and correct the common Laravel document-root drift."
                    .to_string(),
            touches: vec![
                "project profile".to_string(),
                "managed config".to_string(),
                "hosts file".to_string(),
            ],
        },
        RepairWorkflowInfo {
            workflow: RepairWorkflow::Tunnel,
            title: "Repair Tunnel".to_string(),
            summary:
                "Refresh project tunnel bindings and restart the related local origin path when a public route has drifted."
                    .to_string(),
            touches: vec![
                "persistent hostname alias".to_string(),
                "optional or persistent tunnel state".to_string(),
                "web service".to_string(),
            ],
        },
        RepairWorkflowInfo {
            workflow: RepairWorkflow::RuntimeLinks,
            title: "Repair Runtime Links".to_string(),
            summary:
                "Rescan tracked runtimes and switch broken active runtime references to the best available replacement."
                    .to_string(),
            touches: vec![
                "runtime inventory".to_string(),
                "active runtime selection".to_string(),
                "project compatibility hints".to_string(),
            ],
        },
    ]
}

pub fn run_preflight(
    connection: &Connection,
    state: &AppState,
    action: ReliabilityAction,
    project_id: Option<&str>,
) -> Result<ActionPreflightReport, AppError> {
    let generated_at = now_iso()?;
    let mut checks = Vec::new();

    match action {
        ReliabilityAction::RestoreAppMetadata => {
            let managed_processes = current_managed_process_count(state)?;
            if managed_processes > 0 {
                push_check(
                    &mut checks,
                    "MANAGED_PROCESSES_RUNNING",
                    ReliabilityLayer::Workspace,
                    ReliabilityStatus::Error,
                    true,
                    "Managed services are still running",
                    format!(
                        "Stop the {} managed service or tunnel process{} before restoring app metadata.",
                        managed_processes,
                        if managed_processes == 1 { "" } else { "es" }
                    ),
                    Some(
                        "Use the Services and Reliability pages to stop runtimes, quick tunnels, and persistent tunnels first."
                            .to_string(),
                    ),
                );
            } else {
                push_check(
                    &mut checks,
                    "WORKSPACE_IDLE",
                    ReliabilityLayer::Workspace,
                    ReliabilityStatus::Ok,
                    false,
                    "Workspace is idle",
                    "No managed services or tunnels are currently running.".to_string(),
                    None,
                );
            }
        }
        ReliabilityAction::ProvisionProject
        | ReliabilityAction::PublishPersistentDomain
        | ReliabilityAction::StartProjectRuntime => {
            let project_id = project_id.ok_or_else(|| {
                AppError::new_validation(
                    "PROJECT_NOT_FOUND",
                    "A project must be selected for this reliability action.",
                )
            })?;
            let project = ProjectRepository::get(connection, project_id)?;
            let preview = config_generator::preview_config(&project, &state.workspace_dir);
            match preview {
                Ok(rendered) => push_check(
                    &mut checks,
                    "CONFIG_RENDER_READY",
                    ReliabilityLayer::Config,
                    ReliabilityStatus::Ok,
                    false,
                    "Managed config can render",
                    format!(
                        "DevNest can render the managed {} config at {}.",
                        rendered.server_type.as_str(),
                        rendered.output_path.to_string_lossy()
                    ),
                    None,
                ),
                Err(error) => push_check(
                    &mut checks,
                    "CONFIG_RENDER_FAILED",
                    ReliabilityLayer::Config,
                    ReliabilityStatus::Error,
                    true,
                    "Managed config cannot render",
                    error.message,
                    Some(
                        "Repair the project profile or document root before retrying provisioning."
                            .to_string(),
                    ),
                ),
            }

            let server_runtime =
                matching_runtime_binding(connection, &project_server_runtime_type(&project), None)?;
            if !server_runtime.available {
                push_check(
                    &mut checks,
                    "SERVER_RUNTIME_MISSING",
                    ReliabilityLayer::Runtime,
                    ReliabilityStatus::Error,
                    true,
                    "Selected web server runtime is missing",
                    format!(
                        "DevNest cannot find a working {} runtime for this project.",
                        project.server_type.as_str()
                    ),
                    Some(
                        "Open Settings or run Repair Runtime Links to re-link the server runtime."
                            .to_string(),
                    ),
                );
            } else {
                push_check(
                    &mut checks,
                    "SERVER_RUNTIME_READY",
                    ReliabilityLayer::Runtime,
                    ReliabilityStatus::Ok,
                    false,
                    "Selected web server runtime is available",
                    format!(
                        "{} runtime {} is available at {}.",
                        project.server_type.as_str(),
                        server_runtime
                            .version
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                        server_runtime
                            .path
                            .clone()
                            .unwrap_or_else(|| "unknown path".to_string())
                    ),
                    None,
                );
            }

            let php_runtime = matching_runtime_binding(
                connection,
                &RuntimeType::Php,
                Some(&project.php_version),
            )?;
            if !php_runtime.available {
                push_check(
                    &mut checks,
                    "PHP_RUNTIME_MISSING",
                    ReliabilityLayer::Runtime,
                    ReliabilityStatus::Error,
                    true,
                    "Project PHP runtime is missing",
                    format!(
                        "DevNest cannot find a working PHP {} runtime for this project.",
                        project.php_version
                    ),
                    Some(
                        "Link or import the matching PHP runtime, or run Repair Runtime Links."
                            .to_string(),
                    ),
                );
            } else {
                push_check(
                    &mut checks,
                    "PHP_RUNTIME_READY",
                    ReliabilityLayer::Runtime,
                    ReliabilityStatus::Ok,
                    false,
                    "Project PHP runtime is available",
                    format!(
                        "PHP {} resolves to {}.",
                        project.php_version,
                        php_runtime
                            .path
                            .clone()
                            .unwrap_or_else(|| "unknown path".to_string())
                    ),
                    None,
                );
            }

            let service = service_manager::get_service_status(
                connection,
                state,
                project_server_service_name(&project),
            )?;
            if !matches!(service.status, ServiceStatus::Running) {
                let port = service.port.or_else(|| {
                    project_server_service_name(&project)
                        .default_port()
                        .map(i64::from)
                });
                if let Some(port) = port.and_then(|value| u16::try_from(value).ok()) {
                    let port_check = ports::check_port(port)?;
                    if !port_check.available {
                        push_check(
                            &mut checks,
                            "SERVICE_PORT_CONFLICT",
                            ReliabilityLayer::Service,
                            ReliabilityStatus::Error,
                            true,
                            "Required service port is already in use",
                            format!(
                                "Port {} is already owned by {}.",
                                port,
                                port_check
                                    .process_name
                                    .as_deref()
                                    .unwrap_or("another process")
                            ),
                            Some("Free the conflicting port or change the runtime port before retrying.".to_string()),
                        );
                    } else {
                        push_check(
                            &mut checks,
                            "SERVICE_PORT_READY",
                            ReliabilityLayer::Service,
                            ReliabilityStatus::Ok,
                            false,
                            "Required service port is free",
                            format!(
                                "Port {} is available for {}.",
                                port,
                                project.server_type.as_str()
                            ),
                            None,
                        );
                    }
                }
            }

            if matches!(action, ReliabilityAction::ProvisionProject) {
                let local_domain_present = hosts_entry_present(&project.domain);
                push_check(
                    &mut checks,
                    "HOSTS_ENTRY_STATE",
                    ReliabilityLayer::Dns,
                    if local_domain_present {
                        ReliabilityStatus::Ok
                    } else {
                        ReliabilityStatus::Warning
                    },
                    false,
                    "Local domain hosts state",
                    if local_domain_present {
                        format!("{} already exists in the local hosts file.", project.domain)
                    } else {
                        format!(
                            "{} is not in the hosts file yet. DevNest can add it during provisioning.",
                            project.domain
                        )
                    },
                    if local_domain_present {
                        None
                    } else {
                        Some("Provisioning will update the hosts file and may trigger Windows elevation.".to_string())
                    },
                );
            }

            if matches!(action, ReliabilityAction::PublishPersistentDomain) {
                let setup = persistent_tunnels::persistent_tunnel_setup_status(connection)?;
                if !setup.ready {
                    push_check(
                        &mut checks,
                        "PERSISTENT_SETUP_NOT_READY",
                        ReliabilityLayer::Tunnel,
                        ReliabilityStatus::Error,
                        true,
                        "Persistent tunnel setup is incomplete",
                        setup.details,
                        setup.guidance,
                    );
                } else {
                    push_check(
                        &mut checks,
                        "PERSISTENT_SETUP_READY",
                        ReliabilityLayer::Tunnel,
                        ReliabilityStatus::Ok,
                        false,
                        "Persistent tunnel setup is ready",
                        "cloudflared, auth, credentials, and tunnel selection are ready."
                            .to_string(),
                        None,
                    );
                }

                let persistent_hostname =
                    ProjectPersistentHostnameRepository::get_by_project(connection, &project.id)?;
                if persistent_hostname.is_none()
                    && setup
                        .default_hostname_zone
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                {
                    push_check(
                        &mut checks,
                        "PERSISTENT_HOSTNAME_MISSING",
                        ReliabilityLayer::Tunnel,
                        ReliabilityStatus::Error,
                        true,
                        "No stable public hostname is available",
                        "This project has no reserved hostname and Settings has no default public zone for auto-generation.".to_string(),
                        Some("Save a project hostname or set a default public zone in Settings first.".to_string()),
                    );
                } else {
                    push_check(
                        &mut checks,
                        "PERSISTENT_HOSTNAME_READY",
                        ReliabilityLayer::Tunnel,
                        ReliabilityStatus::Ok,
                        false,
                        "Stable public hostname is available",
                        persistent_hostname
                            .as_ref()
                            .map(|item| format!("{} is reserved for this project.", item.hostname))
                            .unwrap_or_else(|| {
                                format!(
                                    "DevNest can auto-generate a hostname under {}.",
                                    setup.default_hostname_zone.unwrap_or_default()
                                )
                            }),
                        None,
                    );
                }
            }
        }
    }

    let ready = !checks.iter().any(|check| check.blocking);

    Ok(ActionPreflightReport {
        action: action.clone(),
        project_id: project_id.map(ToOwned::to_owned),
        ready,
        summary: preflight_summary(&action, &checks, ready),
        checks,
        generated_at,
    })
}

pub fn inspect_project_state(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<ReliabilityInspectorSnapshot, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let diagnostics = diagnostics::run_diagnostics(connection, state, project_id)?;
    let services = service_manager::get_all_service_status(connection, state)?;
    let preview = config_generator::preview_config(&project, &state.workspace_dir);
    let persistent_hostname =
        ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?;
    let persistent_tunnel = {
        let tunnels = state.project_persistent_tunnels.lock().map_err(|_| {
            AppError::new_validation(
                "TUNNEL_STATE_LOCK_FAILED",
                "DevNest could not inspect the persistent tunnel cache.",
            )
        })?;
        tunnels.get(project_id).cloned()
    };
    let quick_tunnel: Option<ProjectTunnelState> = {
        let tunnels = state.project_tunnels.lock().map_err(|_| {
            AppError::new_validation(
                "TUNNEL_STATE_LOCK_FAILED",
                "DevNest could not inspect the quick tunnel cache.",
            )
        })?;
        tunnels.get(project_id).cloned()
    };

    let persistent_health = None;

    let server_runtime =
        matching_runtime_binding(connection, &project_server_runtime_type(&project), None)?;
    let php_runtime =
        matching_runtime_binding(connection, &RuntimeType::Php, Some(&project.php_version))?;
    let mysql_runtime = if project.database_name.is_some() || project.database_port.is_some() {
        Some(matching_runtime_binding(
            connection,
            &RuntimeType::Mysql,
            None,
        )?)
    } else {
        None
    };

    let mut runtime_issues = Vec::new();
    if !server_runtime.available {
        runtime_issues.push(format!(
            "Selected {} runtime is missing from disk.",
            project.server_type.as_str()
        ));
    }
    if !php_runtime.available {
        runtime_issues.push(format!(
            "PHP {} runtime is missing from disk.",
            project.php_version
        ));
    }
    if let Some(mysql_runtime) = mysql_runtime.as_ref() {
        if !mysql_runtime.available {
            runtime_issues.push("Linked MySQL runtime is missing from disk.".to_string());
        }
    }

    let config = match preview {
        Ok(rendered) => InspectorConfigSnapshot {
            server_type: rendered.server_type,
            output_path: rendered.output_path.to_string_lossy().to_string(),
            preview: Some(rendered.config_text.clone()),
            local_domain_alias_present: hosts_entry_present(&project.domain),
            persistent_alias_present: persistent_hostname
                .as_ref()
                .map(|item| rendered.config_text.contains(&item.hostname))
                .unwrap_or(false),
        },
        Err(_) => InspectorConfigSnapshot {
            server_type: project.server_type.clone(),
            output_path: String::new(),
            preview: None,
            local_domain_alias_present: hosts_entry_present(&project.domain),
            persistent_alias_present: false,
        },
    };

    Ok(ReliabilityInspectorSnapshot {
        project,
        diagnostics,
        services,
        config,
        runtime: InspectorRuntimeSnapshot {
            server: server_runtime,
            php: php_runtime,
            mysql: mysql_runtime,
            issues: runtime_issues,
        },
        quick_tunnel,
        persistent_hostname,
        persistent_tunnel,
        persistent_health,
        generated_at: now_iso()?,
    })
}

pub fn export_diagnostics_bundle(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    let inspector = inspect_project_state(connection, state, project_id)?;
    let repair_workflows = available_repair_workflows();
    let provision_preflight = run_preflight(
        connection,
        state,
        ReliabilityAction::ProvisionProject,
        Some(project_id),
    )?;
    let publish_preflight = run_preflight(
        connection,
        state,
        ReliabilityAction::PublishPersistentDomain,
        Some(project_id),
    )?;
    let start_preflight = run_preflight(
        connection,
        state,
        ReliabilityAction::StartProjectRuntime,
        Some(project_id),
    )?;

    let mut service_logs = Vec::new();
    for service in [
        ServiceName::Apache,
        ServiceName::Nginx,
        ServiceName::Mysql,
        ServiceName::Mailpit,
        ServiceName::Redis,
    ] {
        let payload = service_manager::read_service_logs(state, service.clone(), 120)?;
        if payload.content.trim().is_empty() {
            continue;
        }
        service_logs.push(DiagnosticsBundleLogEntry {
            service: payload.name,
            content: payload.content,
        });
    }

    let document = DiagnosticsBundleDocument {
        format_version: 1,
        exported_at: now_iso()?,
        source: "DevNest".to_string(),
        inspector,
        provision_preflight,
        publish_preflight,
        start_preflight,
        repair_workflows,
        service_logs,
    };

    let target_path = match FileDialog::new()
        .add_filter("DevNest Diagnostics Bundle", &["json"])
        .set_file_name("devnest-diagnostics-bundle.json")
        .save_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };
    let payload = serde_json::to_string_pretty(&document).map_err(|error| {
        AppError::with_details(
            "DIAGNOSTICS_BUNDLE_EXPORT_FAILED",
            "DevNest could not serialize the diagnostics bundle.",
            error.to_string(),
        )
    })?;
    fs::write(&target_path, payload).map_err(|error| {
        AppError::with_details(
            "DIAGNOSTICS_BUNDLE_EXPORT_FAILED",
            "DevNest could not write the diagnostics bundle file.",
            error.to_string(),
        )
    })?;

    Ok(Some(ReliabilityTransferResult {
        success: true,
        path: target_path.to_string_lossy().to_string(),
    }))
}

pub fn backup_app_metadata(
    state: &AppState,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    let target_path = match FileDialog::new()
        .add_filter("DevNest App Metadata Backup", &["json"])
        .set_file_name("devnest-app-metadata-backup.json")
        .save_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };
    let document = backup_document(state)?;
    let payload = serde_json::to_string_pretty(&document).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_BACKUP_FAILED",
            "DevNest could not serialize the app metadata backup.",
            error.to_string(),
        )
    })?;
    fs::write(&target_path, payload).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_BACKUP_FAILED",
            "DevNest could not write the app metadata backup file.",
            error.to_string(),
        )
    })?;

    Ok(Some(ReliabilityTransferResult {
        success: true,
        path: target_path.to_string_lossy().to_string(),
    }))
}

pub fn restore_app_metadata(
    state: &AppState,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    let source_path = match FileDialog::new()
        .add_filter("DevNest App Metadata Backup", &["json"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };

    if current_managed_process_count(state)? > 0 {
        return Err(AppError::new_validation(
            "APP_METADATA_RESTORE_BLOCKED",
            "Stop managed services and tunnels before restoring DevNest app metadata.",
        ));
    }

    let document = parse_backup_document(&source_path)?;
    let db_bytes = base64::engine::general_purpose::STANDARD
        .decode(document.db_content_base64)
        .map_err(|error| {
            AppError::with_details(
                "APP_METADATA_RESTORE_FAILED",
                "DevNest could not decode the backed up metadata database.",
                error.to_string(),
            )
        })?;

    for root in [
        managed_config_root(&state.workspace_dir),
        managed_ssl_root(&state.workspace_dir),
        managed_persistent_tunnel_root(&state.workspace_dir),
    ] {
        if root.exists() {
            fs::remove_dir_all(&root).map_err(|error| {
                AppError::with_details(
                    "APP_METADATA_RESTORE_FAILED",
                    "DevNest could not clear the existing managed metadata folders before restore.",
                    error.to_string(),
                )
            })?;
        }
    }

    for file in document.workspace_files {
        let target_path = validated_restore_path(&state.workspace_dir, &file.relative_path)?;
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::from)?;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.content_base64)
            .map_err(|error| {
                AppError::with_details(
                    "APP_METADATA_RESTORE_FAILED",
                    "DevNest could not decode a backed up workspace file.",
                    error.to_string(),
                )
            })?;
        fs::write(&target_path, bytes).map_err(|error| {
            AppError::with_details(
                "APP_METADATA_RESTORE_FAILED",
                "DevNest could not restore a workspace file from backup.",
                error.to_string(),
            )
        })?;
    }

    fs::write(&state.db_path, db_bytes).map_err(|error| {
        AppError::with_details(
            "APP_METADATA_RESTORE_FAILED",
            "DevNest could not restore the metadata database.",
            error.to_string(),
        )
    })?;

    Ok(Some(ReliabilityTransferResult {
        success: true,
        path: source_path.to_string_lossy().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::{available_repair_workflows, inspect_project_state, run_preflight};
    use crate::models::project::{CreateProjectInput, FrameworkType, ServerType};
    use crate::models::reliability::ReliabilityAction;
    use crate::state::AppState;
    use crate::storage::db::init_database;
    use crate::storage::repositories::ProjectRepository;
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use uuid::Uuid;

    fn setup_state() -> (std::path::PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("devnest-reliability-{}", Uuid::new_v4()));
        let workspace_dir = root.join("workspace");
        let db_path = workspace_dir.join("devnest.sqlite3");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        init_database(&db_path).expect("database should initialize");
        let connection = Connection::open(&db_path).expect("db should open");
        let state = AppState {
            db_path,
            workspace_dir,
            resources_dir: root.join("resources"),
            started_at: "2026-04-19T00:00:00Z".to_string(),
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

        (root, state, connection)
    }

    fn create_project_root() -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("devnest-reliability-project-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("public")).expect("public root should exist");
        root
    }

    #[test]
    fn exposes_phase_11_repair_workflows() {
        let workflows = available_repair_workflows();
        assert_eq!(workflows.len(), 3);
        assert!(
            workflows
                .iter()
                .any(|item| item.workflow == crate::models::reliability::RepairWorkflow::Project)
        );
        assert!(
            workflows
                .iter()
                .any(|item| item.workflow == crate::models::reliability::RepairWorkflow::Tunnel)
        );
        assert!(
            workflows
                .iter()
                .any(|item| item.workflow
                    == crate::models::reliability::RepairWorkflow::RuntimeLinks)
        );
    }

    #[test]
    fn provision_preflight_blocks_when_required_runtimes_are_missing() {
        let (root, state, connection) = setup_state();
        let project_root = create_project_root();
        let project = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                name: "Reliability Project".to_string(),
                path: project_root.to_string_lossy().to_string(),
                domain: "reliability.test".to_string(),
                server_type: ServerType::Nginx,
                php_version: "8.4".to_string(),
                framework: FrameworkType::Laravel,
                document_root: "public".to_string(),
                ssl_enabled: false,
                database_name: None,
                database_port: None,
            },
        )
        .expect("project should create");

        let report = run_preflight(
            &connection,
            &state,
            ReliabilityAction::ProvisionProject,
            Some(&project.id),
        )
        .expect("preflight should run");

        assert!(!report.ready);
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.code == "SERVER_RUNTIME_MISSING")
        );
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.code == "PHP_RUNTIME_MISSING")
        );

        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn inspector_surfaces_runtime_issues_when_links_are_missing() {
        let (root, state, connection) = setup_state();
        let project_root = create_project_root();
        let project = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                name: "Inspector Project".to_string(),
                path: project_root.to_string_lossy().to_string(),
                domain: "inspector.test".to_string(),
                server_type: ServerType::Apache,
                php_version: "8.3".to_string(),
                framework: FrameworkType::Laravel,
                document_root: "public".to_string(),
                ssl_enabled: false,
                database_name: Some("inspector_db".to_string()),
                database_port: Some(3306),
            },
        )
        .expect("project should create");

        let snapshot =
            inspect_project_state(&connection, &state, &project.id).expect("inspector should load");

        assert!(!snapshot.runtime.issues.is_empty());
        assert!(
            snapshot
                .runtime
                .issues
                .iter()
                .any(|issue| issue.contains("runtime is missing"))
        );

        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn restore_preflight_ignores_exited_managed_processes() {
        let (root, state, connection) = setup_state();
        let child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("test child should spawn");
        let pid = child.id();

        state
            .managed_processes
            .lock()
            .expect("managed process lock should succeed")
            .insert(
                "stale-test".to_string(),
                crate::state::ManagedServiceProcess {
                    pid,
                    child,
                    log_path: state
                        .workspace_dir
                        .join("runtime-logs")
                        .join("stale-test.log"),
                },
            );

        thread::sleep(Duration::from_millis(150));

        let report = run_preflight(
            &connection,
            &state,
            ReliabilityAction::RestoreAppMetadata,
            None,
        )
        .expect("restore preflight should run");

        assert!(report.ready);
        assert_eq!(
            state
                .managed_processes
                .lock()
                .expect("managed process lock should succeed")
                .len(),
            0
        );

        fs::remove_dir_all(root).ok();
    }
}
