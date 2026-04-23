use crate::error::AppError;
use crate::state::{AppState, BootState};
use reqwest::Url;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::{Config as UpdaterConfig, Error as UpdaterError, UpdaterExt};

const DEFAULT_RELEASE_CHANNEL: &str = "stable";
const DEFAULT_UPDATE_ENDPOINT: &str =
    "https://github.com/hohphu8/devnest/releases/latest/download/stable.json";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReleaseInfo {
    pub app_name: String,
    pub current_version: String,
    pub release_channel: String,
    pub update_endpoint: Option<String>,
    pub updater_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckResult {
    pub status: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_channel: String,
    pub checked_at: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub update_endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInstallResult {
    pub status: String,
    pub target_version: String,
}

fn release_channel() -> String {
    option_env!("DEVNEST_RELEASE_CHANNEL")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_RELEASE_CHANNEL)
        .to_string()
}

fn update_endpoint_fallback() -> Option<String> {
    option_env!("DEVNEST_UPDATE_ENDPOINT")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(DEFAULT_UPDATE_ENDPOINT.to_string()))
}

fn updater_public_key_fallback() -> Option<String> {
    option_env!("DEVNEST_UPDATER_PUBLIC_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn updater_runtime_config(app: &AppHandle) -> Option<UpdaterConfig> {
    app.config()
        .plugins
        .0
        .get("updater")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn configured_update_endpoint(app: &AppHandle) -> Option<String> {
    updater_runtime_config(app)
        .and_then(|config| config.endpoints.first().map(|url| url.to_string()))
        .or_else(update_endpoint_fallback)
}

fn configured_updater_public_key(app: &AppHandle) -> Option<String> {
    updater_runtime_config(app)
        .map(|config| config.pubkey)
        .filter(|value| !value.trim().is_empty())
        .or_else(updater_public_key_fallback)
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn format_pub_date(date: &time::OffsetDateTime) -> Option<String> {
    date.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

fn configured_update_url(app: &AppHandle) -> Result<Url, AppError> {
    let endpoint = configured_update_endpoint(app).ok_or_else(|| {
        AppError::new_validation(
            "APP_UPDATE_NOT_CONFIGURED",
            "DevNest is not configured with an update metadata endpoint.",
        )
    })?;

    Url::parse(&endpoint).map_err(|error| {
        AppError::with_details(
            "APP_UPDATE_ENDPOINT_INVALID",
            "DevNest is configured with an invalid update metadata URL.",
            error.to_string(),
        )
    })
}

fn require_updater_public_key(app: &AppHandle) -> Result<String, AppError> {
    configured_updater_public_key(app).ok_or_else(|| {
        AppError::new_validation(
            "APP_UPDATE_NOT_CONFIGURED",
            "DevNest cannot check for updates because this build does not include an updater public key yet.",
        )
    })
}

fn map_updater_error(error: UpdaterError) -> AppError {
    match error {
        UpdaterError::EmptyEndpoints => AppError::new_validation(
            "APP_UPDATE_ENDPOINT_MISSING",
            "DevNest does not have an update metadata endpoint configured.",
        ),
        UpdaterError::ReleaseNotFound => AppError::with_details(
            "APP_UPDATE_METADATA_INVALID",
            "DevNest could not read a valid update manifest from the metadata endpoint.",
            error.to_string(),
        ),
        UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_) => {
            AppError::with_details(
                "APP_UPDATE_TARGET_UNSUPPORTED",
                "The update manifest does not include a Windows package for this DevNest build.",
                error.to_string(),
            )
        }
        UpdaterError::Reqwest(_)
        | UpdaterError::Network(_)
        | UpdaterError::Http(_)
        | UpdaterError::InvalidHeaderName(_)
        | UpdaterError::InvalidHeaderValue(_) => AppError::with_details(
            "APP_UPDATE_NETWORK_ERROR",
            "DevNest could not reach the update server. Check your connection and try again.",
            error.to_string(),
        ),
        UpdaterError::Minisign(_) | UpdaterError::Base64(_) | UpdaterError::SignatureUtf8(_) => {
            AppError::with_details(
                "APP_UPDATE_SIGNATURE_INVALID",
                "DevNest rejected the downloaded update because its signature could not be verified.",
                error.to_string(),
            )
        }
        UpdaterError::PackageInstallFailed
        | UpdaterError::AuthenticationFailed
        | UpdaterError::InvalidUpdaterFormat
        | UpdaterError::FailedToDetermineExtractPath
        | UpdaterError::TempDirNotFound
        | UpdaterError::BinaryNotFoundInArchive => AppError::with_details(
            "APP_UPDATE_INSTALL_FAILED",
            "DevNest downloaded the update but Windows could not prepare or install it cleanly.",
            error.to_string(),
        ),
        UpdaterError::InsecureTransportProtocol => AppError::with_details(
            "APP_UPDATE_ENDPOINT_INSECURE",
            "DevNest refuses to use a non-HTTPS update metadata endpoint in release builds.",
            error.to_string(),
        ),
        UpdaterError::Io(_) => AppError::with_details(
            "APP_UPDATE_IO_ERROR",
            "DevNest hit a filesystem error while preparing the update.",
            error.to_string(),
        ),
        _ => AppError::with_details(
            "APP_UPDATE_FAILED",
            "DevNest could not complete the update flow.",
            error.to_string(),
        ),
    }
}

#[tauri::command]
pub fn ping() -> Result<String, AppError> {
    Ok("pong:tauri".to_string())
}

#[tauri::command]
pub fn get_boot_state(state: tauri::State<'_, AppState>) -> Result<BootState, AppError> {
    Ok(BootState {
        app_name: "DevNest".to_string(),
        environment: "tauri".to_string(),
        db_path: state.db_path.display().to_string(),
        started_at: state.started_at.clone(),
    })
}

#[tauri::command]
pub fn get_app_release_info(app: AppHandle) -> Result<AppReleaseInfo, AppError> {
    let update_endpoint = configured_update_endpoint(&app);
    let updater_public_key = configured_updater_public_key(&app);

    Ok(AppReleaseInfo {
        app_name: app.package_info().name.clone(),
        current_version: app.package_info().version.to_string(),
        release_channel: release_channel(),
        update_endpoint,
        updater_configured: updater_public_key.is_some(),
    })
}

#[tauri::command]
pub async fn check_for_app_update(app: AppHandle) -> Result<AppUpdateCheckResult, AppError> {
    let endpoint = configured_update_url(&app)?;
    let _public_key = require_updater_public_key(&app)?;
    let checked_at = now_rfc3339();
    let current_version = app.package_info().version.to_string();
    let channel = release_channel();

    let update = app
        .updater()
        .map_err(map_updater_error)?
        .check()
        .await
        .map_err(map_updater_error)?;

    if let Some(update) = update {
        return Ok(AppUpdateCheckResult {
            status: "updateAvailable".to_string(),
            current_version,
            latest_version: Some(update.version.clone()),
            release_channel: channel,
            checked_at,
            notes: update.body.clone(),
            pub_date: update.date.as_ref().and_then(format_pub_date),
            update_endpoint: Some(endpoint.to_string()),
        });
    }

    Ok(AppUpdateCheckResult {
        status: "upToDate".to_string(),
        current_version,
        latest_version: None,
        release_channel: channel,
        checked_at,
        notes: None,
        pub_date: None,
        update_endpoint: Some(endpoint.to_string()),
    })
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<AppUpdateInstallResult, AppError> {
    let _endpoint = configured_update_url(&app)?;
    let _public_key = require_updater_public_key(&app)?;

    let update = app
        .updater()
        .map_err(map_updater_error)?
        .check()
        .await
        .map_err(map_updater_error)?
        .ok_or_else(|| {
            AppError::new_validation(
                "APP_UPDATE_NOT_AVAILABLE",
                "DevNest did not find a newer release to install.",
            )
        })?;

    let target_version = update.version.clone();
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(map_updater_error)?;

    Ok(AppUpdateInstallResult {
        status: "restartRequired".to_string(),
        target_version,
    })
}
