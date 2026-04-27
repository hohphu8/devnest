use crate::commands::hosts;
use crate::commands::persistent_tunnels::{
    start_project_persistent_tunnel_internal, stop_project_persistent_tunnel_internal,
};
use crate::commands::tunnels::{start_project_tunnel_internal, stop_project_tunnel_internal};
use crate::core::{config_generator, reliability, runtime_registry, service_manager};
use crate::error::AppError;
use crate::models::project::{FrameworkType, FrankenphpMode, UpdateProjectPatch};
use crate::models::reliability::{
    ActionPreflightReport, ReliabilityAction, ReliabilityInspectorSnapshot, ReliabilityLayer,
    ReliabilityTransferResult, RepairExecutionResult, RepairWorkflow, RepairWorkflowInfo,
};
use crate::models::runtime::RuntimeType;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::{
    ProjectPersistentHostnameRepository, ProjectRepository, RuntimeVersionRepository, now_iso,
};
use rusqlite::Connection;
use std::path::PathBuf;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn server_service_for_project(project: &crate::models::project::Project) -> ServiceName {
    match project.server_type {
        crate::models::project::ServerType::Apache => ServiceName::Apache,
        crate::models::project::ServerType::Nginx => ServiceName::Nginx,
        crate::models::project::ServerType::Frankenphp => ServiceName::Frankenphp,
    }
}

fn project_server_runtime_type(project: &crate::models::project::Project) -> RuntimeType {
    match project.server_type {
        crate::models::project::ServerType::Apache => RuntimeType::Apache,
        crate::models::project::ServerType::Nginx => RuntimeType::Nginx,
        crate::models::project::ServerType::Frankenphp => RuntimeType::Frankenphp,
    }
}

fn runtime_available(path: &str) -> bool {
    let candidate = PathBuf::from(path);
    candidate.exists() && candidate.is_file()
}

fn first_available_runtime(
    connection: &Connection,
    runtime_type: &RuntimeType,
    preferred_version_prefix: Option<&str>,
) -> Result<Option<crate::models::runtime::RuntimeVersion>, AppError> {
    let mut runtimes = RuntimeVersionRepository::list_by_type(connection, runtime_type)?;
    runtimes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    if let Some(prefix) = preferred_version_prefix {
        if let Some(runtime) = runtimes
            .iter()
            .find(|runtime| runtime.version.starts_with(prefix) && runtime_available(&runtime.path))
        {
            return Ok(Some(runtime.clone()));
        }
    }

    Ok(runtimes
        .into_iter()
        .find(|runtime| runtime_available(&runtime.path)))
}

fn activate_runtime(
    connection: &Connection,
    runtime: &crate::models::runtime::RuntimeVersion,
) -> Result<(), AppError> {
    RuntimeVersionRepository::clear_active_for_type(connection, &runtime.runtime_type)?;
    let _ = RuntimeVersionRepository::upsert(
        connection,
        &runtime.runtime_type,
        &runtime.version,
        &runtime.path,
        true,
    )?;
    Ok(())
}

fn repair_project_workflow(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<RepairExecutionResult, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    if matches!(project.framework, FrameworkType::Laravel)
        && project.document_root.trim() != "public"
    {
        let _ = ProjectRepository::update(
            connection,
            project_id,
            UpdateProjectPatch {
                name: None,
                domain: None,
                server_type: None,
                php_version: None,
                framework: None,
                document_root: Some("public".to_string()),
                ssl_enabled: None,
                database_name: None,
                database_port: None,
                status: None,
                frankenphp_mode: None,
            },
        )?;
    }

    let refreshed = ProjectRepository::get(connection, project_id)?;
    let aliases = ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?
        .map(|item| vec![item.hostname])
        .unwrap_or_default();
    let worker_port = if !matches!(refreshed.frankenphp_mode, FrankenphpMode::Classic) {
        Some(
            FrankenphpOctaneWorkerRepository::get_or_create_for_mode(
                connection,
                &state.workspace_dir,
                &refreshed.id,
                refreshed.frankenphp_mode.clone(),
            )?
            .worker_port,
        )
    } else {
        None
    };
    let _ = config_generator::generate_config_with_aliases_and_frankenphp_worker_port(
        &refreshed,
        &state.workspace_dir,
        &aliases,
        worker_port,
    )?;
    let _ = hosts::apply_hosts_entry(refreshed.domain.clone(), None)?;

    Ok(RepairExecutionResult {
        workflow: RepairWorkflow::Project,
        success: true,
        message:
            "DevNest refreshed the project profile, rebuilt managed config, and reapplied the local domain entry."
                .to_string(),
        touched_layers: vec![
            ReliabilityLayer::Project,
            ReliabilityLayer::Config,
            ReliabilityLayer::Dns,
        ],
        generated_at: now_iso()?,
    })
}

fn repair_runtime_links_workflow(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<RepairExecutionResult, AppError> {
    runtime_registry::sync_runtime_versions(
        connection,
        &state.workspace_dir,
        &state.resources_dir,
    )?;

    let project = ProjectRepository::get(connection, project_id)?;
    let mut changes = Vec::new();

    let server_type = project_server_runtime_type(&project);
    let current_server = RuntimeVersionRepository::find_active_by_type(connection, &server_type)?;
    if current_server
        .as_ref()
        .map(|runtime| !runtime_available(&runtime.path))
        .unwrap_or(true)
    {
        if let Some(runtime) = first_available_runtime(connection, &server_type, None)? {
            activate_runtime(connection, &runtime)?;
            changes.push(format!(
                "Activated {} {}.",
                runtime.runtime_type.as_str(),
                runtime.version
            ));
        }
    }

    let matching_php =
        first_available_runtime(connection, &RuntimeType::Php, Some(&project.php_version))?;
    let current_php = RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Php)?;
    if current_php
        .as_ref()
        .map(|runtime| !runtime_available(&runtime.path))
        .unwrap_or(true)
    {
        if let Some(runtime) = matching_php {
            activate_runtime(connection, &runtime)?;
            changes.push(format!("Activated PHP {}.", runtime.version));
        }
    }

    if project.database_name.is_some() || project.database_port.is_some() {
        let current_mysql =
            RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Mysql)?;
        if current_mysql
            .as_ref()
            .map(|runtime| !runtime_available(&runtime.path))
            .unwrap_or(true)
        {
            if let Some(runtime) = first_available_runtime(connection, &RuntimeType::Mysql, None)? {
                activate_runtime(connection, &runtime)?;
                changes.push(format!("Activated MySQL {}.", runtime.version));
            }
        }
    }

    Ok(RepairExecutionResult {
        workflow: RepairWorkflow::RuntimeLinks,
        success: true,
        message: if changes.is_empty() {
            "Runtime inventory was rescanned. DevNest did not need to change any active runtime links."
                .to_string()
        } else {
            format!(
                "Runtime inventory was rescanned and DevNest repaired the active links: {}",
                changes.join(" ")
            )
        },
        touched_layers: vec![ReliabilityLayer::Runtime],
        generated_at: now_iso()?,
    })
}

fn repair_tunnel_workflow(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<RepairExecutionResult, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let service_name = server_service_for_project(&project);
    let service = service_manager::get_service_status(connection, state, service_name.clone())?;

    if matches!(service.status, ServiceStatus::Running) {
        let _ = service_manager::restart_service(connection, state, service_name)?;
    } else {
        let _ = service_manager::start_service(connection, state, service_name)?;
    }

    let persistent_hostname =
        ProjectPersistentHostnameRepository::get_by_project(connection, project_id)?;
    if persistent_hostname.is_some() {
        let _ = stop_project_persistent_tunnel_internal(project_id, state)?;
        let _ = start_project_persistent_tunnel_internal(project_id, state, false)?;
        return Ok(RepairExecutionResult {
            workflow: RepairWorkflow::Tunnel,
            success: true,
            message:
                "DevNest refreshed the persistent tunnel route, regenerated server aliases, and restarted the local origin path."
                    .to_string(),
            touched_layers: vec![
                ReliabilityLayer::Tunnel,
                ReliabilityLayer::Config,
                ReliabilityLayer::Service,
            ],
            generated_at: now_iso()?,
        });
    }

    let quick_tunnel = {
        let tunnels = state.project_tunnels.lock().map_err(|_| {
            AppError::new_validation(
                "TUNNEL_STATE_LOCK_FAILED",
                "DevNest could not inspect the quick tunnel cache.",
            )
        })?;
        tunnels.get(project_id).cloned()
    };
    if quick_tunnel.is_some() {
        let _ = stop_project_tunnel_internal(project_id, state)?;
        let _ = start_project_tunnel_internal(project_id, state)?;
        return Ok(RepairExecutionResult {
            workflow: RepairWorkflow::Tunnel,
            success: true,
            message:
                "DevNest restarted the optional project tunnel and refreshed the local origin binding."
                    .to_string(),
            touched_layers: vec![ReliabilityLayer::Tunnel, ReliabilityLayer::Service],
            generated_at: now_iso()?,
        });
    }

    Ok(RepairExecutionResult {
        workflow: RepairWorkflow::Tunnel,
        success: true,
        message:
            "DevNest refreshed the local origin service and managed aliases. No active tunnel process needed a restart."
                .to_string(),
        touched_layers: vec![ReliabilityLayer::Config, ReliabilityLayer::Service],
        generated_at: now_iso()?,
    })
}

#[tauri::command]
pub fn list_repair_workflows() -> Result<Vec<RepairWorkflowInfo>, AppError> {
    Ok(reliability::available_repair_workflows())
}

#[tauri::command]
pub fn run_action_preflight(
    action: ReliabilityAction,
    project_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ActionPreflightReport, AppError> {
    let connection = connection_from_state(&state)?;
    reliability::run_preflight(&connection, &state, action, project_id.as_deref())
}

#[tauri::command]
pub fn inspect_reliability_state(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ReliabilityInspectorSnapshot, AppError> {
    let connection = connection_from_state(&state)?;
    reliability::inspect_project_state(&connection, &state, &project_id)
}

#[tauri::command]
pub fn export_diagnostics_bundle(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    let connection = connection_from_state(&state)?;
    reliability::export_diagnostics_bundle(&connection, &state, &project_id)
}

#[tauri::command]
pub fn backup_app_metadata(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    reliability::backup_app_metadata(&state)
}

#[tauri::command]
pub fn restore_app_metadata(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ReliabilityTransferResult>, AppError> {
    reliability::restore_app_metadata(&state)
}

#[tauri::command]
pub fn run_repair_workflow(
    project_id: String,
    workflow: RepairWorkflow,
    state: tauri::State<'_, AppState>,
) -> Result<RepairExecutionResult, AppError> {
    let connection = connection_from_state(&state)?;
    match workflow {
        RepairWorkflow::Project => repair_project_workflow(&connection, &state, &project_id),
        RepairWorkflow::Tunnel => repair_tunnel_workflow(&connection, &state, &project_id),
        RepairWorkflow::RuntimeLinks => {
            repair_runtime_links_workflow(&connection, &state, &project_id)
        }
    }
}
