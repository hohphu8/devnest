use crate::core::php_cli_environment;
use crate::core::runtime_packages;
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::project::ServerType;
use crate::models::runtime::{
    RuntimeHealthStatus, RuntimeInstallStage, RuntimeInstallTask, RuntimeInventoryItem,
    RuntimePackage, RuntimeSource, RuntimeVersion,
};
use crate::state::AppState;
use crate::storage::repositories::{
    ProjectRepository, RuntimeSuppressionRepository, RuntimeVersionRepository, now_iso,
};
use crate::tray;
use crate::utils::files::copy_dir_recursive;
use crate::utils::paths::{
    bundled_runtime_root, downloaded_runtime_root, downloaded_runtime_type_dir,
    managed_runtime_root, managed_runtime_type_dir,
};
use crate::utils::windows::reveal_in_explorer;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tauri::Manager;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn current_runtime_install_task(state: &AppState) -> Result<Option<RuntimeInstallTask>, AppError> {
    let task = state.runtime_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "RUNTIME_INSTALL_STATE_LOCK_FAILED",
            "DevNest could not read the current runtime install state.",
        )
    })?;

    Ok(task.clone())
}

fn update_runtime_install_task(
    state: &AppState,
    task: Option<RuntimeInstallTask>,
) -> Result<(), AppError> {
    let mut current = state.runtime_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "RUNTIME_INSTALL_STATE_LOCK_FAILED",
            "DevNest could not update the current runtime install state.",
        )
    })?;

    *current = task;
    Ok(())
}

fn install_task_snapshot(
    package: &RuntimePackage,
    stage: RuntimeInstallStage,
    message: impl Into<String>,
    error_code: Option<String>,
) -> Result<RuntimeInstallTask, AppError> {
    Ok(RuntimeInstallTask {
        package_id: package.id.clone(),
        display_name: package.display_name.clone(),
        runtime_type: package.runtime_type.clone(),
        version: package.version.clone(),
        stage,
        message: message.into(),
        updated_at: now_iso()?,
        error_code,
    })
}

fn to_inventory_item(
    runtime: RuntimeVersion,
    workspace_dir: &Path,
    resources_dir: &Path,
) -> RuntimeInventoryItem {
    let runtime_path = Path::new(&runtime.path);
    let path_exists = runtime_path.exists() && runtime_path.is_file();
    let source = runtime_source_from_path(runtime_path, workspace_dir, resources_dir);
    let details = if path_exists {
        None
    } else {
        Some(format!(
            "Runtime binary was not found at {}. Re-import or verify the runtime path.",
            runtime.path
        ))
    };

    RuntimeInventoryItem {
        id: runtime.id,
        runtime_type: runtime.runtime_type,
        version: runtime.version,
        path: runtime.path,
        is_active: runtime.is_active,
        source,
        status: if path_exists {
            RuntimeHealthStatus::Available
        } else {
            RuntimeHealthStatus::Missing
        },
        created_at: runtime.created_at,
        updated_at: runtime.updated_at,
        details,
    }
}

fn runtime_source_from_path(
    runtime_path: &Path,
    workspace_dir: &Path,
    resources_dir: &Path,
) -> RuntimeSource {
    if runtime_path.starts_with(downloaded_runtime_root(workspace_dir)) {
        RuntimeSource::Downloaded
    } else if runtime_path.starts_with(managed_runtime_root(workspace_dir)) {
        RuntimeSource::Imported
    } else if runtime_path.starts_with(bundled_runtime_root(resources_dir)) {
        RuntimeSource::Bundled
    } else {
        RuntimeSource::External
    }
}

fn sync_php_cli_if_needed(
    connection: &Connection,
    workspace_dir: &Path,
    runtime_type: &crate::models::runtime::RuntimeType,
    should_set_active: bool,
) -> Result<Option<String>, AppError> {
    if should_set_active && matches!(runtime_type, crate::models::runtime::RuntimeType::Php) {
        return php_cli_environment::sync_active_php_cli_environment(connection, workspace_dir);
    }

    Ok(None)
}

fn apply_sync_warning(
    mut runtime: RuntimeInventoryItem,
    sync_warning: Option<String>,
) -> RuntimeInventoryItem {
    if let Some(sync_warning) = sync_warning {
        runtime.details = match runtime.details {
            Some(existing) if !existing.trim().is_empty() => {
                Some(format!("{existing} {sync_warning}"))
            }
            _ => Some(sync_warning),
        };
    }

    runtime
}

fn managed_runtime_container_path(
    runtime: &RuntimeVersion,
    workspace_dir: &Path,
    resources_dir: &Path,
) -> Option<PathBuf> {
    let runtime_path = Path::new(&runtime.path);

    if runtime_path.starts_with(downloaded_runtime_root(workspace_dir)) {
        return Some(
            downloaded_runtime_type_dir(workspace_dir, &runtime.runtime_type)
                .join(version_folder_name(&runtime.version)),
        );
    }

    if runtime_path.starts_with(managed_runtime_root(workspace_dir)) {
        return Some(
            managed_runtime_type_dir(workspace_dir, &runtime.runtime_type)
                .join(version_folder_name(&runtime.version)),
        );
    }

    if runtime_path.starts_with(bundled_runtime_root(resources_dir)) {
        return None;
    }

    None
}

pub(crate) fn list_runtime_inventory_snapshot(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<RuntimeInventoryItem>, AppError> {
    runtime_registry::sync_runtime_versions(
        connection,
        &state.workspace_dir,
        &state.resources_dir,
    )?;
    let runtimes = RuntimeVersionRepository::list(connection)?;

    Ok(runtimes
        .into_iter()
        .map(|runtime| to_inventory_item(runtime, &state.workspace_dir, &state.resources_dir))
        .collect())
}

pub(crate) fn set_active_runtime_internal(
    connection: &Connection,
    state: &AppState,
    runtime_id: &str,
) -> Result<RuntimeInventoryItem, AppError> {
    let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;

    RuntimeVersionRepository::clear_active_for_type(connection, &runtime.runtime_type)?;

    let active_runtime = RuntimeVersionRepository::upsert(
        connection,
        &runtime.runtime_type,
        &runtime.version,
        &runtime.path,
        true,
    )?;
    let sync_warning = sync_php_cli_if_needed(
        connection,
        &state.workspace_dir,
        &runtime.runtime_type,
        true,
    )?;

    Ok(apply_sync_warning(
        to_inventory_item(active_runtime, &state.workspace_dir, &state.resources_dir),
        sync_warning,
    ))
}

#[tauri::command]
pub fn list_runtime_inventory(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RuntimeInventoryItem>, AppError> {
    let connection = connection_from_state(&state)?;
    list_runtime_inventory_snapshot(&connection, &state)
}

#[tauri::command]
pub fn list_runtime_packages(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RuntimePackage>, AppError> {
    runtime_packages::list_runtime_packages(&state.resources_dir)
}

#[tauri::command]
pub fn get_runtime_install_task(
    state: tauri::State<'_, AppState>,
) -> Result<Option<RuntimeInstallTask>, AppError> {
    current_runtime_install_task(&state)
}

fn parse_runtime_type(value: &str) -> Result<crate::models::runtime::RuntimeType, AppError> {
    crate::models::runtime::RuntimeType::from_str(value).map_err(|_| {
        AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "Runtime type must be one of php, apache, nginx, or mysql.",
        )
    })
}

#[tauri::command]
pub fn verify_runtime_path(
    runtime_type: String,
    path: String,
) -> Result<RuntimeInventoryItem, AppError> {
    let runtime_type = parse_runtime_type(&runtime_type)?;
    let normalized_path = path.trim().to_string();
    let version =
        runtime_registry::verify_runtime_binary(&runtime_type, Path::new(&normalized_path))?;
    let timestamp = now_iso()?;

    Ok(RuntimeInventoryItem {
        id: format!("{}-{version}", runtime_type.as_str()),
        runtime_type,
        version,
        path: normalized_path,
        is_active: false,
        source: RuntimeSource::External,
        status: RuntimeHealthStatus::Available,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        details: Some("Runtime binary verified successfully.".to_string()),
    })
}

fn runtime_home_from_binary(
    runtime_type: &crate::models::runtime::RuntimeType,
    path: &Path,
) -> Result<PathBuf, AppError> {
    let binary_dir = path.parent().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_RUNTIME_SOURCE",
            "The selected runtime binary must live inside a runtime folder.",
        )
    })?;

    let runtime_home = match runtime_type {
        crate::models::runtime::RuntimeType::Php | crate::models::runtime::RuntimeType::Nginx => {
            binary_dir.to_path_buf()
        }
        crate::models::runtime::RuntimeType::Apache
        | crate::models::runtime::RuntimeType::Mysql => {
            if binary_dir
                .file_name()
                .map(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
                .unwrap_or(false)
            {
                binary_dir
                    .parent()
                    .ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_RUNTIME_SOURCE",
                            "The selected runtime binary must live inside a runtime folder.",
                        )
                    })?
                    .to_path_buf()
            } else {
                binary_dir.to_path_buf()
            }
        }
    };

    if !runtime_home.exists() || !runtime_home.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_SOURCE",
            "The selected runtime folder does not exist or is not a directory.",
        ));
    }

    Ok(runtime_home)
}

fn version_folder_name(version: &str) -> String {
    version
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn dependency_guard_message(
    runtime: &RuntimeVersion,
    dependent_projects: &[String],
) -> Result<(), AppError> {
    if dependent_projects.is_empty() {
        return Ok(());
    }

    let preview = dependent_projects
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if dependent_projects.len() > 3 {
        format!(" and {} more", dependent_projects.len() - 3)
    } else {
        String::new()
    };

    let message = match runtime.runtime_type {
        crate::models::runtime::RuntimeType::Php => format!(
            "PHP {} is still referenced by tracked projects. Move those projects to another PHP version before removing this runtime.",
            runtime.version
        ),
        crate::models::runtime::RuntimeType::Apache | crate::models::runtime::RuntimeType::Nginx => format!(
            "The active {} runtime is still needed by tracked projects. Set another {} runtime active before removing this one.",
            runtime.runtime_type.as_str(),
            runtime.runtime_type.as_str()
        ),
        crate::models::runtime::RuntimeType::Mysql => {
            "The active MySQL runtime is still needed by tracked database projects. Set another MySQL runtime active before removing this one.".to_string()
        }
    };

    Err(AppError::with_details(
        "RUNTIME_IN_USE",
        message,
        format!("Dependent projects: {preview}{suffix}"),
    ))
}

fn guard_runtime_removal(
    connection: &Connection,
    runtime: &RuntimeVersion,
) -> Result<(), AppError> {
    let projects = ProjectRepository::list(connection)?;

    let dependent_projects = match runtime.runtime_type {
        crate::models::runtime::RuntimeType::Php => projects
            .into_iter()
            .filter(|project| project.php_version == runtime.version)
            .map(|project| format!("{} ({})", project.name, project.domain))
            .collect::<Vec<_>>(),
        crate::models::runtime::RuntimeType::Apache
        | crate::models::runtime::RuntimeType::Nginx => {
            if !runtime.is_active {
                Vec::new()
            } else {
                let server_type = match runtime.runtime_type {
                    crate::models::runtime::RuntimeType::Apache => ServerType::Apache,
                    crate::models::runtime::RuntimeType::Nginx => ServerType::Nginx,
                    _ => unreachable!(),
                };

                projects
                    .into_iter()
                    .filter(|project| {
                        matches!(
                            (&project.server_type, &server_type),
                            (ServerType::Apache, ServerType::Apache)
                                | (ServerType::Nginx, ServerType::Nginx)
                        )
                    })
                    .map(|project| format!("{} ({})", project.name, project.domain))
                    .collect::<Vec<_>>()
            }
        }
        crate::models::runtime::RuntimeType::Mysql => {
            if !runtime.is_active {
                Vec::new()
            } else {
                projects
                    .into_iter()
                    .filter(|project| {
                        project.database_name.is_some() || project.database_port.is_some()
                    })
                    .map(|project| format!("{} ({})", project.name, project.domain))
                    .collect::<Vec<_>>()
            }
        }
    };

    dependency_guard_message(runtime, &dependent_projects)
}

#[tauri::command]
pub fn link_runtime_path(
    runtime_type: String,
    path: String,
    set_active: Option<bool>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInventoryItem, AppError> {
    let runtime_type = parse_runtime_type(&runtime_type)?;
    let normalized_path = path.trim().to_string();
    let version =
        runtime_registry::verify_runtime_binary(&runtime_type, Path::new(&normalized_path))?;
    let connection = connection_from_state(&state)?;
    let should_set_active = set_active.unwrap_or(true);

    if should_set_active {
        RuntimeVersionRepository::clear_active_for_type(&connection, &runtime_type)?;
    }

    RuntimeSuppressionRepository::remove(&connection, &runtime_type, Path::new(&normalized_path))?;

    let runtime = RuntimeVersionRepository::upsert(
        &connection,
        &runtime_type,
        &version,
        &normalized_path,
        should_set_active,
    )?;
    let sync_warning = sync_php_cli_if_needed(
        &connection,
        &state.workspace_dir,
        &runtime_type,
        should_set_active,
    )?;

    let inventory_item = apply_sync_warning(
        to_inventory_item(runtime, &state.workspace_dir, &state.resources_dir),
        sync_warning,
    );
    let _ = tray::refresh(&app);

    Ok(inventory_item)
}

#[tauri::command]
pub fn import_runtime_path(
    runtime_type: String,
    path: String,
    set_active: Option<bool>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInventoryItem, AppError> {
    let runtime_type = parse_runtime_type(&runtime_type)?;
    let normalized_path = path.trim().to_string();
    let source_binary = Path::new(&normalized_path);
    let version = runtime_registry::verify_runtime_binary(&runtime_type, source_binary)?;
    let runtime_home = runtime_home_from_binary(&runtime_type, source_binary)?;

    if source_binary.starts_with(managed_runtime_root(&state.workspace_dir)) {
        return Err(AppError::new_validation(
            "RUNTIME_ALREADY_MANAGED",
            "This runtime is already inside the DevNest managed runtimes folder. Link it directly instead of importing again.",
        ));
    }

    if source_binary.starts_with(bundled_runtime_root(&state.resources_dir)) {
        return Err(AppError::new_validation(
            "RUNTIME_ALREADY_BUNDLED",
            "This runtime is already bundled with DevNest. Link it directly instead of importing another copy.",
        ));
    }

    let binary_relative_path = source_binary.strip_prefix(&runtime_home).map_err(|_| {
        AppError::new_validation(
            "INVALID_RUNTIME_SOURCE",
            "The selected runtime binary must stay inside its runtime folder.",
        )
    })?;

    let destination_root = managed_runtime_type_dir(&state.workspace_dir, &runtime_type)
        .join(version_folder_name(&version));

    if destination_root.exists() {
        fs::remove_dir_all(&destination_root)?;
    }

    copy_dir_recursive(&runtime_home, &destination_root)?;
    let imported_binary_path = destination_root.join(binary_relative_path);

    if !imported_binary_path.exists() || !imported_binary_path.is_file() {
        return Err(AppError::new_validation(
            "RUNTIME_IMPORT_FAILED",
            "DevNest copied the runtime folder but could not find the managed runtime binary afterward.",
        ));
    }

    let connection = connection_from_state(&state)?;
    let should_set_active = set_active.unwrap_or(true);

    if should_set_active {
        RuntimeVersionRepository::clear_active_for_type(&connection, &runtime_type)?;
    }

    let runtime = RuntimeVersionRepository::upsert(
        &connection,
        &runtime_type,
        &version,
        &imported_binary_path.to_string_lossy(),
        should_set_active,
    )?;
    let sync_warning = sync_php_cli_if_needed(
        &connection,
        &state.workspace_dir,
        &runtime_type,
        should_set_active,
    )?;

    let mut inventory_item = to_inventory_item(runtime, &state.workspace_dir, &state.resources_dir);
    inventory_item.details = Some(format!(
        "Runtime copied into the managed DevNest runtime root at {}.",
        destination_root.to_string_lossy()
    ));

    let inventory_item = apply_sync_warning(inventory_item, sync_warning);
    let _ = tray::refresh(&app);

    Ok(inventory_item)
}

#[tauri::command]
pub async fn install_runtime_package(
    package_id: String,
    set_active: Option<bool>,
    app: tauri::AppHandle,
) -> Result<RuntimeInventoryItem, AppError> {
    let state = app.state::<AppState>();
    let packages = runtime_packages::list_runtime_packages(&state.resources_dir)?;
    let package = packages
        .into_iter()
        .find(|item| item.id == package_id)
        .ok_or_else(|| {
            AppError::new_validation(
                "RUNTIME_PACKAGE_NOT_FOUND",
                "Runtime package was not found in the current manifest.",
            )
        })?;

    update_runtime_install_task(
        &state,
        Some(install_task_snapshot(
            &package,
            RuntimeInstallStage::Queued,
            format!("Queued {} for download.", package.display_name),
            None,
        )?),
    )?;

    let workspace_dir = state.workspace_dir.clone();
    let resources_dir = state.resources_dir.clone();
    let db_path = state.db_path.clone();
    let package_for_install = package.clone();
    let should_set_active = set_active.unwrap_or(true);
    let app_handle = app.clone();

    let install_result = tauri::async_runtime::spawn_blocking(move || {
        let app_state = app_handle.state::<AppState>();

        update_runtime_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                RuntimeInstallStage::Downloading,
                format!("Downloading {}...", package_for_install.display_name),
                None,
            )?),
        )?;
        let archive_path =
            runtime_packages::download_runtime_archive(&package_for_install, &workspace_dir)?;

        update_runtime_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                RuntimeInstallStage::Verifying,
                format!("Verifying {} checksum...", package_for_install.display_name),
                None,
            )?),
        )?;
        runtime_packages::verify_archive_checksum(
            &archive_path,
            &package_for_install.checksum_sha256,
        )?;

        update_runtime_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                RuntimeInstallStage::Extracting,
                format!("Extracting {}...", package_for_install.display_name),
                None,
            )?),
        )?;
        let extracted_root = runtime_packages::extract_runtime_archive(
            &package_for_install,
            &archive_path,
            &workspace_dir,
        )?;
        let entry_binary_path =
            runtime_packages::resolve_package_entry_path(&package_for_install, &extracted_root)?;
        let detected_version = runtime_registry::verify_runtime_binary(
            &package_for_install.runtime_type,
            &entry_binary_path,
        )?;

        if detected_version != package_for_install.version {
            return Err(AppError::with_details(
                "RUNTIME_PACKAGE_VERSION_MISMATCH",
                "Installed runtime binary version did not match the package manifest.",
                format!(
                    "manifest={}, detected={detected_version}",
                    package_for_install.version
                ),
            ));
        }

        update_runtime_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                RuntimeInstallStage::Registering,
                format!(
                    "Registering {} in DevNest...",
                    package_for_install.display_name
                ),
                None,
            )?),
        )?;

        let connection = Connection::open(&db_path)?;

        if should_set_active {
            RuntimeVersionRepository::clear_active_for_type(
                &connection,
                &package_for_install.runtime_type,
            )?;
        }

        RuntimeSuppressionRepository::remove(
            &connection,
            &package_for_install.runtime_type,
            &entry_binary_path,
        )?;

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &package_for_install.runtime_type,
            &package_for_install.version,
            &entry_binary_path.to_string_lossy(),
            should_set_active,
        )?;
        let sync_warning = sync_php_cli_if_needed(
            &connection,
            &workspace_dir,
            &package_for_install.runtime_type,
            should_set_active,
        )?;

        let mut inventory_item = to_inventory_item(runtime, &workspace_dir, &resources_dir);
        inventory_item.details = Some(format!(
            "Runtime package installed from {} into {}.",
            package_for_install.display_name,
            extracted_root.to_string_lossy()
        ));

        Ok::<RuntimeInventoryItem, AppError>(apply_sync_warning(inventory_item, sync_warning))
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "RUNTIME_INSTALL_JOIN_FAILED",
            "Runtime installation did not finish cleanly.",
            error.to_string(),
        )
    })?;

    match install_result {
        Ok(runtime) => {
            update_runtime_install_task(
                &state,
                Some(install_task_snapshot(
                    &package,
                    RuntimeInstallStage::Completed,
                    format!("{} installed successfully.", package.display_name),
                    None,
                )?),
            )?;
            let _ = tray::refresh(&app);
            Ok(runtime)
        }
        Err(error) => {
            update_runtime_install_task(
                &state,
                Some(install_task_snapshot(
                    &package,
                    RuntimeInstallStage::Failed,
                    error.message.clone(),
                    Some(error.code.clone()),
                )?),
            )?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn set_active_runtime(
    runtime_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInventoryItem, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = set_active_runtime_internal(&connection, &state, &runtime_id)?;
    let _ = tray::refresh(&app);
    Ok(runtime)
}

#[tauri::command]
pub fn remove_runtime_reference(
    runtime_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    let runtime_path = PathBuf::from(&runtime.path);
    let runtime_source =
        runtime_source_from_path(&runtime_path, &state.workspace_dir, &state.resources_dir);

    guard_runtime_removal(&connection, &runtime)?;

    if matches!(runtime_source, RuntimeSource::External) {
        RuntimeSuppressionRepository::suppress(&connection, &runtime.runtime_type, &runtime_path)?;
    }

    if let Some(managed_root) =
        managed_runtime_container_path(&runtime, &state.workspace_dir, &state.resources_dir)
    {
        if managed_root.exists() {
            fs::remove_dir_all(&managed_root)?;
        }
    }

    let deleted = RuntimeVersionRepository::delete_by_id(&connection, &runtime.id)?;
    if deleted
        && runtime.is_active
        && matches!(
            runtime.runtime_type,
            crate::models::runtime::RuntimeType::Php
        )
    {
        let _ = php_cli_environment::sync_active_php_cli_environment(
            &connection,
            &state.workspace_dir,
        )?;
    }

    if deleted {
        let _ = tray::refresh(&app);
    }

    Ok(deleted)
}

#[tauri::command]
pub fn reveal_runtime_path(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;
    let runtime_home = runtime_home_from_binary(&runtime.runtime_type, Path::new(&runtime.path))?;
    reveal_in_explorer(&runtime_home)?;
    Ok(true)
}
