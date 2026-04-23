use crate::core::{runtime_config, runtime_registry};
use crate::error::AppError;
use crate::models::runtime_config::{RuntimeConfigSchema, RuntimeConfigValues};
use crate::state::AppState;
use crate::storage::repositories::{RuntimeConfigOverrideRepository, RuntimeVersionRepository};
use crate::utils::windows::open_file_in_default_app;
use rusqlite::Connection;
use std::collections::HashMap;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn get_runtime_config_schema(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeConfigSchema, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    runtime_config::schema_for_runtime(&runtime, &state.workspace_dir)
}

#[tauri::command]
pub fn get_runtime_config_values(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeConfigValues, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    runtime_config::values_for_runtime(&connection, &runtime, &state.workspace_dir)
}

#[tauri::command]
pub fn update_runtime_config(
    runtime_id: String,
    patch: HashMap<String, String>,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeConfigValues, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    let normalized = runtime_config::validate_patch(&runtime, &patch)?;

    RuntimeConfigOverrideRepository::upsert_many(&connection, &runtime_id, &normalized)?;
    runtime_registry::materialize_runtime_config_file(&connection, &state.workspace_dir, &runtime)?;

    runtime_config::values_for_runtime(&connection, &runtime, &state.workspace_dir)
}

#[tauri::command]
pub fn open_runtime_config_file(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    runtime_config::ensure_runtime_config_supported(&runtime)?;
    let config_path = runtime_registry::materialize_runtime_config_file(
        &connection,
        &state.workspace_dir,
        &runtime,
    )?;
    open_file_in_default_app(&config_path)?;
    Ok(true)
}
