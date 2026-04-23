use crate::core::mobile_preview;
use crate::error::AppError;
use crate::state::AppState;
use crate::storage::repositories::ProjectRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn get_project_mobile_preview_state(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<crate::models::mobile_preview::ProjectMobilePreviewState>, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    mobile_preview::get_preview_state(&state, &project)
}

#[tauri::command]
pub fn start_project_mobile_preview(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::models::mobile_preview::ProjectMobilePreviewState, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    mobile_preview::start_preview(&connection, &state, &project)
}

#[tauri::command]
pub fn stop_project_mobile_preview(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::models::mobile_preview::ProjectMobilePreviewState, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    mobile_preview::stop_preview(&state, &project)
}
