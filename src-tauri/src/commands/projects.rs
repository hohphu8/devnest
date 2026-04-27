use crate::commands::{database, persistent_tunnels};
use crate::core::frankenphp_octane_manager;
use crate::core::scheduled_task_manager;
use crate::core::worker_manager;
use crate::core::{config_generator, project_scanner};
use crate::error::AppError;
use crate::models::project::{
    CreateProjectInput, FrankenphpMode, Project, ServerType, UpdateProjectPatch,
};
use crate::models::scan::ScanResult;
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::ProjectRepository;
use crate::utils::windows::{open_folder_in_explorer, open_in_vscode, open_terminal_at_path};
use rfd::FileDialog;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn remove_project_managed_configs(workspace_dir: &Path, domain: &str) -> Result<(), AppError> {
    for server_type in [
        ServerType::Apache,
        ServerType::Nginx,
        ServerType::Frankenphp,
    ] {
        config_generator::remove_managed_config(workspace_dir, &server_type, domain)?;
    }

    Ok(())
}

#[derive(serde::Serialize)]
pub struct DeleteProjectResult {
    pub success: bool,
}

#[derive(serde::Serialize)]
pub struct ProjectQuickActionResult {
    pub success: bool,
}

#[tauri::command]
pub fn list_projects(state: tauri::State<'_, AppState>) -> Result<Vec<Project>, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectRepository::list(&connection)
}

#[tauri::command]
pub fn get_project(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectRepository::get(&connection, &project_id)
}

#[tauri::command]
pub fn create_project(
    input: CreateProjectInput,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectRepository::create(&connection, input)
}

#[tauri::command]
pub fn update_project(
    project_id: String,
    patch: UpdateProjectPatch,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let connection = connection_from_state(&state)?;
    let current = ProjectRepository::get(&connection, &project_id)?;
    let persistent_route_context_changed = patch
        .domain
        .as_ref()
        .map(|domain| domain.trim().eq_ignore_ascii_case(current.domain.trim()))
        .map(|same| !same)
        .unwrap_or(false)
        || patch
            .server_type
            .as_ref()
            .map(|server_type| server_type.as_str() != current.server_type.as_str())
            .unwrap_or(false)
        || patch
            .document_root
            .as_ref()
            .map(|document_root| document_root != &current.document_root)
            .unwrap_or(false)
        || patch
            .ssl_enabled
            .map(|ssl_enabled| ssl_enabled != current.ssl_enabled)
            .unwrap_or(false);
    let next_database_name = patch
        .database_name
        .as_ref()
        .map(|value| value.as_ref().map(|name| name.trim().to_string()))
        .unwrap_or_else(|| current.database_name.clone());

    if current.database_name.is_some() && next_database_name != current.database_name {
        let note = format!("before relinking project {}", current.name);
        if let Err(error) = database::take_project_linked_database_snapshot_if_enabled(
            &connection,
            &state,
            &current,
            &note,
        ) {
            eprintln!(
                "DevNest linked-project snapshot failed before updating {}: {}",
                current.name, error
            );
        }
    }

    let updated = ProjectRepository::update(&connection, &project_id, patch)?;
    if !matches!(current.frankenphp_mode, FrankenphpMode::Classic)
        && current.frankenphp_mode.as_str() != updated.frankenphp_mode.as_str()
    {
        let _ = frankenphp_octane_manager::stop(&connection, &state, &project_id);
    }

    remove_project_managed_configs(&state.workspace_dir, &current.domain)?;
    if updated.domain != current.domain {
        remove_project_managed_configs(&state.workspace_dir, &updated.domain)?;
    }
    let worker_port = if !matches!(updated.frankenphp_mode, FrankenphpMode::Classic) {
        Some(
            FrankenphpOctaneWorkerRepository::get_or_create_for_mode(
                &connection,
                &state.workspace_dir,
                &updated.id,
                updated.frankenphp_mode.clone(),
            )?
            .worker_port,
        )
    } else {
        None
    };
    config_generator::generate_config_with_aliases_and_frankenphp_worker_port(
        &updated,
        &state.workspace_dir,
        &[],
        worker_port,
    )?;
    if persistent_route_context_changed {
        persistent_tunnels::reset_project_persistent_tunnel_after_profile_change(
            &connection,
            &state,
            &updated.id,
        )?;
    }

    Ok(updated)
}

#[tauri::command]
pub fn delete_project(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteProjectResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let note = format!("before deleting project {}", project.name);
    if let Err(error) = database::take_project_linked_database_snapshot_if_enabled(
        &connection,
        &state,
        &project,
        &note,
    ) {
        eprintln!(
            "DevNest linked-project snapshot failed before deleting {}: {}",
            project.name, error
        );
    }
    worker_manager::delete_workers_for_project(&connection, &state, &project_id)?;
    let _ = frankenphp_octane_manager::stop(&connection, &state, &project_id);
    scheduled_task_manager::delete_tasks_for_project(&connection, &state, &project_id)?;
    ProjectRepository::delete(&connection, &project_id)?;
    remove_project_managed_configs(&state.workspace_dir, &project.domain)?;

    Ok(DeleteProjectResult { success: true })
}

fn project_path_from_id(connection: &Connection, project_id: &str) -> Result<PathBuf, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    Ok(Path::new(&project.path).to_path_buf())
}

#[tauri::command]
pub fn open_project_folder(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectQuickActionResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project_path = project_path_from_id(&connection, &project_id)?;
    open_folder_in_explorer(&project_path)?;

    Ok(ProjectQuickActionResult { success: true })
}

#[tauri::command]
pub fn open_project_terminal(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectQuickActionResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project_path = project_path_from_id(&connection, &project_id)?;
    open_terminal_at_path(&project_path)?;

    Ok(ProjectQuickActionResult { success: true })
}

#[tauri::command]
pub fn open_project_vscode(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectQuickActionResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project_path = project_path_from_id(&connection, &project_id)?;
    open_in_vscode(&project_path)?;

    Ok(ProjectQuickActionResult { success: true })
}

#[tauri::command]
pub async fn scan_project(path: String) -> Result<ScanResult, AppError> {
    let project_path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || project_scanner::scan_project(&project_path))
        .await
        .map_err(|error| {
            AppError::with_details(
                "PROJECT_SCAN_FAILED",
                "DevNest could not finish scanning the selected project folder.",
                error.to_string(),
            )
        })?
}

#[tauri::command]
pub fn pick_project_folder() -> Result<Option<String>, AppError> {
    Ok(FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string()))
}
