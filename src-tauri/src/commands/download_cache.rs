use crate::core::download_cache;
use crate::error::AppError;
use crate::models::optional_tool::OptionalToolInstallStage;
use crate::models::runtime::RuntimeInstallStage;
use crate::state::AppState;

fn install_task_is_active(
    runtime_stage: Option<&RuntimeInstallStage>,
    optional_tool_stage: Option<&OptionalToolInstallStage>,
) -> bool {
    runtime_stage
        .map(|stage| {
            matches!(
                stage,
                RuntimeInstallStage::Queued
                    | RuntimeInstallStage::Downloading
                    | RuntimeInstallStage::Verifying
                    | RuntimeInstallStage::Extracting
                    | RuntimeInstallStage::Registering
            )
        })
        .unwrap_or(false)
        || optional_tool_stage
            .map(|stage| {
                matches!(
                    stage,
                    OptionalToolInstallStage::Queued
                        | OptionalToolInstallStage::Downloading
                        | OptionalToolInstallStage::Verifying
                        | OptionalToolInstallStage::Extracting
                        | OptionalToolInstallStage::Registering
                )
            })
            .unwrap_or(false)
}

fn guard_no_active_download_install(state: &AppState) -> Result<(), AppError> {
    let runtime_task = state.runtime_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "DOWNLOAD_CACHE_STATE_LOCK_FAILED",
            "DevNest could not inspect the current runtime install state before clearing downloads.",
        )
    })?;
    let optional_tool_task = state.optional_tool_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "DOWNLOAD_CACHE_STATE_LOCK_FAILED",
            "DevNest could not inspect the current optional tool install state before clearing downloads.",
        )
    })?;

    if install_task_is_active(
        runtime_task.as_ref().map(|task| &task.stage),
        optional_tool_task.as_ref().map(|task| &task.stage),
    ) {
        return Err(AppError::new_validation(
            "DOWNLOAD_CACHE_CLEAR_BLOCKED",
            "A download or install task is still running. Wait for it to finish before clearing download cache.",
        ));
    }

    Ok(())
}

#[tauri::command]
pub fn get_download_cache_summary(
    state: tauri::State<'_, AppState>,
) -> Result<download_cache::DownloadCacheSummary, AppError> {
    download_cache::summarize_download_cache(&state.workspace_dir)
}

#[tauri::command]
pub fn clear_download_cache(
    state: tauri::State<'_, AppState>,
) -> Result<download_cache::ClearDownloadCacheResult, AppError> {
    guard_no_active_download_install(&state)?;
    download_cache::clear_download_cache(&state.workspace_dir)
}
