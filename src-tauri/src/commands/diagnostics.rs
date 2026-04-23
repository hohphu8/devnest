use crate::core::{diagnostics, local_ssl};
use crate::error::AppError;
use crate::models::diagnostics::DiagnosticItem;
use crate::models::project::{Project, UpdateProjectPatch};
use crate::state::AppState;
use crate::storage::repositories::ProjectRepository;
use crate::utils::windows::trust_certificate_for_current_user;
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn run_diagnostics(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DiagnosticItem>, AppError> {
    let connection = connection_from_state(&state)?;
    diagnostics::run_diagnostics(&connection, &state, &project_id)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFixResult {
    pub success: bool,
    pub code: String,
    pub message: String,
    pub project: Option<Project>,
}

#[tauri::command]
pub fn apply_diagnostic_fix(
    project_id: String,
    code: String,
    state: tauri::State<'_, AppState>,
) -> Result<DiagnosticFixResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;

    match code.as_str() {
        "LARAVEL_DOCUMENT_ROOT_MISMATCH" => {
            let updated = ProjectRepository::update(
                &connection,
                &project_id,
                UpdateProjectPatch {
                    name: None,
                    domain: None,
                    server_type: None,
                    php_version: None,
                    framework: None,
                    document_root: Some("public".to_string()),
                    ssl_enabled: None,
                    database_name: None,
                    database_port: None,
                    status: None,
                },
            )?;

            Ok(DiagnosticFixResult {
                success: true,
                code,
                message: "Document root was updated to `public`.".to_string(),
                project: Some(updated),
            })
        }
        "SSL_AUTHORITY_MISSING" | "SSL_TRUST_MISSING" => {
            let authority = local_ssl::ensure_ssl_authority(&state.workspace_dir)?;
            trust_certificate_for_current_user(&authority.cert_path)?;

            Ok(DiagnosticFixResult {
                success: true,
                code,
                message: "DevNest Local CA is now trusted for the current user.".to_string(),
                project: Some(project),
            })
        }
        "SSL_CERTIFICATE_MISSING" => {
            if !project.ssl_enabled {
                return Err(AppError::new_validation(
                    "SSL_NOT_ENABLED",
                    "Enable SSL in the project profile before regenerating a certificate.",
                ));
            }

            local_ssl::regenerate_ssl_material(&state.workspace_dir, &project.domain)?;

            Ok(DiagnosticFixResult {
                success: true,
                code,
                message: "Project SSL certificate was regenerated.".to_string(),
                project: Some(project),
            })
        }
        _ => Err(AppError::new_validation(
            "DIAGNOSTIC_FIX_UNSUPPORTED",
            "This diagnostic does not support an automatic fix yet.",
        )),
    }
}
