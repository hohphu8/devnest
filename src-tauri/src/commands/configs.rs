use crate::core::config_generator;
use crate::error::AppError;
use crate::models::project::{FrankenphpMode, ServerType};
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::ProjectRepository;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVhostConfigResult {
    pub server_type: ServerType,
    pub config_text: String,
    pub output_path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateVhostConfigResult {
    pub success: bool,
    pub output_path: String,
}

#[tauri::command]
pub fn preview_vhost_config(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PreviewVhostConfigResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let worker_port = if !matches!(project.frankenphp_mode, FrankenphpMode::Classic) {
        Some(
            FrankenphpOctaneWorkerRepository::get_or_create_for_mode(
                &connection,
                &state.workspace_dir,
                &project.id,
                project.frankenphp_mode.clone(),
            )?
            .worker_port,
        )
    } else {
        None
    };
    let rendered = config_generator::preview_config_with_frankenphp_worker_port(
        &project,
        &state.workspace_dir,
        worker_port,
    )?;

    Ok(PreviewVhostConfigResult {
        server_type: rendered.server_type,
        config_text: rendered.config_text,
        output_path: rendered.output_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn generate_vhost_config(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<GenerateVhostConfigResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let worker_port = if !matches!(project.frankenphp_mode, FrankenphpMode::Classic) {
        Some(
            FrankenphpOctaneWorkerRepository::get_or_create_for_mode(
                &connection,
                &state.workspace_dir,
                &project.id,
                project.frankenphp_mode.clone(),
            )?
            .worker_port,
        )
    } else {
        None
    };
    let rendered = config_generator::generate_config_with_aliases_and_frankenphp_worker_port(
        &project,
        &state.workspace_dir,
        &[],
        worker_port,
    )?;

    Ok(GenerateVhostConfigResult {
        success: true,
        output_path: rendered.output_path.to_string_lossy().to_string(),
    })
}
