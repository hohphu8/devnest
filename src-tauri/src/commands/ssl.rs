use crate::core::local_ssl;
use crate::error::AppError;
use crate::state::AppState;
use crate::storage::repositories::ProjectRepository;
use crate::utils::windows::{
    is_certificate_trusted_for_current_user, open_url_in_default_browser,
    remove_certificate_from_current_user_store, trust_certificate_for_current_user,
};
use rusqlite::Connection;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSslResult {
    pub success: bool,
    pub domain: String,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSslAuthorityResult {
    pub success: bool,
    pub cert_path: String,
    pub trusted: bool,
}

#[tauri::command]
pub fn trust_local_ssl_authority(
    state: tauri::State<'_, AppState>,
) -> Result<LocalSslAuthorityResult, AppError> {
    let authority = local_ssl::ensure_ssl_authority(&state.workspace_dir)?;
    trust_certificate_for_current_user(&authority.cert_path)?;

    Ok(LocalSslAuthorityResult {
        success: true,
        cert_path: authority.cert_path.to_string_lossy().to_string(),
        trusted: true,
    })
}

#[tauri::command]
pub fn get_local_ssl_authority_status(
    state: tauri::State<'_, AppState>,
) -> Result<LocalSslAuthorityResult, AppError> {
    let authority = local_ssl::ensure_ssl_authority(&state.workspace_dir)?;
    let trusted = is_certificate_trusted_for_current_user(&authority.cert_path)?;

    Ok(LocalSslAuthorityResult {
        success: true,
        cert_path: authority.cert_path.to_string_lossy().to_string(),
        trusted,
    })
}

#[tauri::command]
pub fn untrust_local_ssl_authority(
    state: tauri::State<'_, AppState>,
) -> Result<LocalSslAuthorityResult, AppError> {
    let authority = local_ssl::ensure_ssl_authority(&state.workspace_dir)?;
    let _removed = remove_certificate_from_current_user_store(&authority.cert_path)?;

    Ok(LocalSslAuthorityResult {
        success: true,
        cert_path: authority.cert_path.to_string_lossy().to_string(),
        trusted: false,
    })
}

#[tauri::command]
pub fn regenerate_project_ssl_certificate(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectSslResult, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    if !project.ssl_enabled {
        return Err(AppError::new_validation(
            "SSL_NOT_ENABLED",
            "Enable SSL in the project profile before regenerating a local certificate.",
        ));
    }

    let material = local_ssl::regenerate_ssl_material(&state.workspace_dir, &project.domain)?;

    Ok(ProjectSslResult {
        success: true,
        domain: project.domain,
        cert_path: material.cert_path.to_string_lossy().to_string(),
        key_path: material.key_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn open_project_site(
    project_id: String,
    prefer_https: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let use_https = prefer_https.unwrap_or(false) && project.ssl_enabled;
    let url = if use_https {
        format!("https://{}", project.domain)
    } else {
        format!("http://{}", project.domain)
    };

    open_url_in_default_browser(&url)?;
    Ok(true)
}
