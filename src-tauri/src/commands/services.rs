use crate::commands::persistent_tunnels;
use crate::core::{frankenphp_octane_manager, service_manager};
use crate::error::AppError;
use crate::models::service::ServiceName;
use crate::models::service::ServiceState;
use crate::state::AppState;
use crate::storage::repositories::ServiceRepository;
use crate::utils::windows::open_url_in_default_browser;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn get_all_service_status(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ServiceState>, AppError> {
    let connection = connection_from_state(&state)?;
    service_manager::get_all_service_status(&connection, &state)
}

#[tauri::command]
pub fn get_service_status(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceState, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    service_manager::get_service_status(&connection, &state, service.name)
}

#[tauri::command]
pub fn start_service(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceState, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    service_manager::start_service(&connection, &state, service.name)
}

#[tauri::command]
pub fn stop_service(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceState, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    let service_name = service.name;
    let stopped = service_manager::stop_service(&connection, &state, service_name.clone())?;
    if matches!(service_name, ServiceName::Frankenphp) {
        frankenphp_octane_manager::mark_stale_for_frankenphp_stop(&connection, &state)?;
    }
    persistent_tunnels::reset_persistent_tunnels_for_origin_service_stop(
        &connection,
        &state,
        &service_name,
    )?;
    Ok(stopped)
}

#[tauri::command]
pub fn restart_service(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceState, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;
    service_manager::restart_service(&connection, &state, service.name)
}

#[tauri::command]
pub fn open_service_dashboard(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, &name)?;

    match service.name {
        ServiceName::Mailpit => {
            if service.status != crate::models::service::ServiceStatus::Running {
                return Err(AppError::new_validation(
                    "SERVICE_DASHBOARD_UNAVAILABLE",
                    "Start Mailpit before opening its inbox dashboard.",
                ));
            }
            let port = service.port.unwrap_or(8025);
            open_url_in_default_browser(&format!("http://127.0.0.1:{port}"))?;
            Ok(true)
        }
        _ => Err(AppError::new_validation(
            "SERVICE_DASHBOARD_UNAVAILABLE",
            "This service does not expose a built-in browser dashboard.",
        )),
    }
}
