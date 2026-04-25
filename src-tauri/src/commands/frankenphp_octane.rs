use crate::core::log_reader::ProjectWorkerLogPayload;
use crate::core::{frankenphp_octane_manager, service_manager};
use crate::error::AppError;
use crate::models::frankenphp_octane::{
    FrankenphpOctanePreflight, FrankenphpOctaneWorkerSettings,
    UpdateFrankenphpOctaneWorkerSettingsInput,
};
use crate::models::project::FrankenphpMode;
use crate::models::service::ServiceName;
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::ProjectRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn get_project_frankenphp_worker_settings(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::get_settings(&connection, &state, &project_id)
}

#[tauri::command]
pub fn update_project_frankenphp_worker_settings(
    project_id: String,
    input: UpdateFrankenphpOctaneWorkerSettingsInput,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    let current = frankenphp_octane_manager::get_status(&connection, &state, &project_id)?;
    if matches!(
        current.status,
        crate::models::frankenphp_octane::FrankenphpOctaneWorkerStatus::Running
    ) {
        let _ = frankenphp_octane_manager::stop(&connection, &state, &project_id)?;
    }

    let updated = FrankenphpOctaneWorkerRepository::update(
        &connection,
        &state.workspace_dir,
        &project_id,
        input,
    )?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    if matches!(project.frankenphp_mode, FrankenphpMode::Octane) {
        service_manager::sync_managed_configs_for_service(
            &connection,
            &state,
            &ServiceName::Frankenphp,
        )?;
    }

    Ok(updated)
}

#[tauri::command]
pub fn get_project_frankenphp_octane_preflight(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctanePreflight, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::preflight(&connection, &state, &project_id)
}

#[tauri::command]
pub fn get_project_frankenphp_worker_status(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::get_status(&connection, &state, &project_id)
}

#[tauri::command]
pub fn start_project_frankenphp_worker(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::start(&connection, &state, &project_id)
}

#[tauri::command]
pub fn stop_project_frankenphp_worker(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::stop(&connection, &state, &project_id)
}

#[tauri::command]
pub fn restart_project_frankenphp_worker(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::restart(&connection, &state, &project_id)
}

#[tauri::command]
pub fn reload_project_frankenphp_worker(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::reload(&connection, &state, &project_id)
}

#[tauri::command]
pub fn read_project_frankenphp_worker_logs(
    project_id: String,
    lines: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorkerLogPayload, AppError> {
    let connection = connection_from_state(&state)?;
    frankenphp_octane_manager::read_logs(&connection, &state, &project_id, lines.unwrap_or(200))
}
