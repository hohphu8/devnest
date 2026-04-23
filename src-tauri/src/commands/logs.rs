use crate::core::log_reader::ServiceLogPayload;
use crate::core::service_manager;
use crate::error::AppError;
use crate::state::AppState;
use crate::storage::repositories::ServiceRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub async fn read_service_logs(
    name: String,
    lines: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceLogPayload, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    let log_path = service_manager::resolve_service_log_path(&state, &service.name)?;
    let service_name = service.name.as_str().to_string();
    let line_count = lines.unwrap_or(200);

    tauri::async_runtime::spawn_blocking(move || {
        crate::core::log_reader::read_tail_payload(&log_path, &service_name, line_count)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not finish reading the service log file.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub fn clear_service_logs(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    service_manager::clear_service_logs(&state, service.name)?;
    Ok(true)
}
