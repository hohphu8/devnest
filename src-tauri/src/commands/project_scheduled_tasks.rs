use crate::core::log_reader::ProjectScheduledTaskRunLogPayload;
use crate::core::scheduled_task_manager;
use crate::error::AppError;
use crate::models::scheduled_task::{
    CreateProjectScheduledTaskInput, ProjectScheduledTask, ProjectScheduledTaskRun,
    UpdateProjectScheduledTaskPatch,
};
use crate::state::AppState;
use crate::storage::project_scheduled_tasks::ProjectScheduledTaskRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(serde::Serialize)]
pub struct DeleteProjectScheduledTaskResult {
    pub success: bool,
}

#[tauri::command]
pub fn list_project_scheduled_tasks(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectScheduledTask>, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::list_project_scheduled_tasks(&connection, &state, &project_id)
}

#[tauri::command]
pub fn list_all_scheduled_tasks(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectScheduledTask>, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::list_all_scheduled_tasks(&connection, &state)
}

#[tauri::command]
pub fn create_project_scheduled_task(
    input: CreateProjectScheduledTaskInput,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let created = ProjectScheduledTaskRepository::create(&connection, &task_id, input)?;

    if created.enabled {
        return scheduled_task_manager::enable_project_scheduled_task(
            &connection,
            &state,
            &created.id,
        );
    }

    scheduled_task_manager::get_project_scheduled_task_status(&connection, &state, &created.id)
}

#[tauri::command]
pub fn update_project_scheduled_task(
    task_id: String,
    patch: UpdateProjectScheduledTaskPatch,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    let should_stop_before_update = patch.task_type.is_some()
        || patch.command_line.is_some()
        || patch.working_directory.is_some()
        || patch.url.is_some()
        || patch.schedule_mode.is_some()
        || patch.simple_schedule_kind.is_some()
        || patch.schedule_expression.is_some()
        || patch.interval_seconds.is_some()
        || patch.daily_time.is_some()
        || patch.weekly_day.is_some();

    if should_stop_before_update
        && matches!(
            scheduled_task_manager::get_project_scheduled_task_status(
                &connection,
                &state,
                &task_id
            )?
            .status,
            crate::models::scheduled_task::ProjectScheduledTaskStatus::Running
        )
    {
        let _ =
            scheduled_task_manager::disable_project_scheduled_task(&connection, &state, &task_id)?;
    }

    let updated = ProjectScheduledTaskRepository::update(&connection, &task_id, patch)?;
    if updated.enabled {
        return scheduled_task_manager::enable_project_scheduled_task(
            &connection,
            &state,
            &updated.id,
        );
    }

    scheduled_task_manager::disable_project_scheduled_task(&connection, &state, &updated.id)
}

#[tauri::command]
pub fn delete_project_scheduled_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteProjectScheduledTaskResult, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::delete_project_scheduled_task(&connection, &state, &task_id)?;
    Ok(DeleteProjectScheduledTaskResult { success: true })
}

#[tauri::command]
pub fn get_project_scheduled_task_status(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::get_project_scheduled_task_status(&connection, &state, &task_id)
}

#[tauri::command]
pub fn enable_project_scheduled_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::enable_project_scheduled_task(&connection, &state, &task_id)
}

#[tauri::command]
pub fn disable_project_scheduled_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::disable_project_scheduled_task(&connection, &state, &task_id)
}

#[tauri::command]
pub fn run_project_scheduled_task_now(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::run_project_scheduled_task_now(&connection, &state, &task_id)
}

#[tauri::command]
pub fn list_project_scheduled_task_runs(
    task_id: String,
    limit: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectScheduledTaskRun>, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::list_project_scheduled_task_runs(
        &connection,
        &task_id,
        limit.unwrap_or(25),
    )
}

#[tauri::command]
pub fn read_project_scheduled_task_run_logs(
    run_id: String,
    lines: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTaskRunLogPayload, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::read_project_scheduled_task_run_logs(
        &connection,
        &run_id,
        lines.unwrap_or(200),
    )
}

#[tauri::command]
pub fn clear_project_scheduled_task_logs(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::clear_project_scheduled_task_logs(&connection, &state, &task_id)?;
    Ok(true)
}

#[tauri::command]
pub fn clear_project_scheduled_task_history(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectScheduledTask, AppError> {
    let connection = connection_from_state(&state)?;
    scheduled_task_manager::clear_project_scheduled_task_history(&connection, &state, &task_id)
}
