use crate::core::project_env_inspector;
use crate::error::AppError;
use crate::models::project_env_var::{
    CreateProjectEnvVarInput, ProjectEnvInspection, ProjectEnvVar, UpdateProjectEnvVarInput,
};
use crate::state::AppState;
use crate::storage::repositories::{ProjectEnvVarRepository, ProjectRepository};
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(serde::Serialize)]
pub struct DeleteProjectEnvVarResult {
    pub success: bool,
}

#[tauri::command]
pub fn list_project_env_vars(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectEnvVar>, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectEnvVarRepository::list_by_project(&connection, &project_id)
}

#[tauri::command]
pub fn create_project_env_var(
    input: CreateProjectEnvVarInput,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectEnvVar, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectEnvVarRepository::create(&connection, input)
}

#[tauri::command]
pub fn update_project_env_var(
    input: UpdateProjectEnvVarInput,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectEnvVar, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectEnvVarRepository::update(&connection, input)
}

#[tauri::command]
pub fn delete_project_env_var(
    project_id: String,
    env_var_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteProjectEnvVarResult, AppError> {
    let connection = connection_from_state(&state)?;
    ProjectEnvVarRepository::delete(&connection, &project_id, &env_var_id)?;

    Ok(DeleteProjectEnvVarResult { success: true })
}

#[tauri::command]
pub fn inspect_project_env(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectEnvInspection, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let tracked_vars = ProjectEnvVarRepository::list_by_project(&connection, &project_id)?;

    project_env_inspector::inspect_project_env(&project, tracked_vars)
}
