use crate::core::log_reader::ProjectWorkerLogPayload;
use crate::core::worker_manager;
use crate::error::AppError;
use crate::models::worker::{CreateProjectWorkerInput, ProjectWorker, UpdateProjectWorkerPatch};
use crate::state::AppState;
use crate::storage::project_workers::ProjectWorkerRepository;
use crate::storage::repositories::ProjectRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(serde::Serialize)]
pub struct DeleteProjectWorkerResult {
    pub success: bool,
}

fn worker_log_path(state: &AppState, project_id: &str, worker_id: &str) -> String {
    state
        .workspace_dir
        .join("runtime-logs")
        .join("workers")
        .join(project_id)
        .join(format!("{worker_id}.log"))
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
pub fn list_project_workers(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectWorker>, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::list_project_workers(&connection, &state, &project_id)
}

#[tauri::command]
pub fn list_all_workers(state: tauri::State<'_, AppState>) -> Result<Vec<ProjectWorker>, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::list_all_workers(&connection, &state)
}

#[tauri::command]
pub fn create_project_worker(
    input: CreateProjectWorkerInput,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &input.project_id)?;
    let worker_id = uuid::Uuid::new_v4().to_string();
    ProjectWorkerRepository::create(
        &connection,
        &worker_id,
        input,
        &project.path,
        &worker_log_path(&state, &project.id, &worker_id),
    )
}

#[tauri::command]
pub fn update_project_worker(
    worker_id: String,
    patch: UpdateProjectWorkerPatch,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    let current = ProjectWorkerRepository::get(&connection, &worker_id)?;
    let project = ProjectRepository::get(&connection, &current.project_id)?;
    let should_stop_before_update = patch.command_line.is_some()
        || patch.working_directory.is_some()
        || patch.preset_type.is_some();

    if should_stop_before_update {
        let status = worker_manager::get_project_worker_status(&connection, &state, &worker_id)?;
        if matches!(
            status.status,
            crate::models::worker::ProjectWorkerStatus::Running
        ) {
            let _ = worker_manager::stop_project_worker(&connection, &state, &worker_id)?;
        }
    }

    ProjectWorkerRepository::update(&connection, &worker_id, patch, &project.path)
}

#[tauri::command]
pub fn delete_project_worker(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteProjectWorkerResult, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::delete_project_worker(&connection, &state, &worker_id)?;
    Ok(DeleteProjectWorkerResult { success: true })
}

#[tauri::command]
pub fn get_project_worker_status(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::get_project_worker_status(&connection, &state, &worker_id)
}

#[tauri::command]
pub fn start_project_worker(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::start_project_worker(&connection, &state, &worker_id)
}

#[tauri::command]
pub fn stop_project_worker(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::stop_project_worker(&connection, &state, &worker_id)
}

#[tauri::command]
pub fn restart_project_worker(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorker, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::restart_project_worker(&connection, &state, &worker_id)
}

#[tauri::command]
pub fn read_project_worker_logs(
    worker_id: String,
    lines: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectWorkerLogPayload, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::read_project_worker_logs(&connection, &state, &worker_id, lines.unwrap_or(200))
}

#[tauri::command]
pub fn clear_project_worker_logs(
    worker_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    worker_manager::clear_project_worker_logs(&connection, &state, &worker_id)?;
    Ok(true)
}
