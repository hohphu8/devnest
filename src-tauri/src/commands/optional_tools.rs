use crate::core::config_generator;
use crate::core::hosts_editor;
use crate::core::optional_tools;
use crate::core::service_manager;
use crate::error::AppError;
use crate::models::optional_tool::{
    OptionalToolHealthStatus, OptionalToolInstallStage, OptionalToolInstallTask,
    OptionalToolInventoryItem, OptionalToolPackage, OptionalToolType, OptionalToolVersion,
};
use crate::models::service::ServiceName;
use crate::state::AppState;
use crate::storage::repositories::{OptionalToolVersionRepository, ServiceRepository, now_iso};
use crate::utils::paths::downloaded_optional_tool_type_dir;
use crate::utils::process::configure_background_command;
use crate::utils::windows::{
    apply_hosts_file_with_elevation, hosts_file_path, open_folder_in_explorer,
    remove_hosts_file_with_elevation,
};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;
use uuid::Uuid;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn current_optional_tool_install_task(
    state: &AppState,
) -> Result<Option<OptionalToolInstallTask>, AppError> {
    let task = state.optional_tool_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "OPTIONAL_TOOL_INSTALL_STATE_LOCK_FAILED",
            "DevNest could not read the current optional tool install state.",
        )
    })?;

    Ok(task.clone())
}

fn update_optional_tool_install_task(
    state: &AppState,
    task: Option<OptionalToolInstallTask>,
) -> Result<(), AppError> {
    let mut current = state.optional_tool_install_task.lock().map_err(|_| {
        AppError::new_validation(
            "OPTIONAL_TOOL_INSTALL_STATE_LOCK_FAILED",
            "DevNest could not update the current optional tool install state.",
        )
    })?;

    *current = task;
    Ok(())
}

fn install_task_snapshot(
    package: &OptionalToolPackage,
    stage: OptionalToolInstallStage,
    message: impl Into<String>,
    error_code: Option<String>,
) -> Result<OptionalToolInstallTask, AppError> {
    Ok(OptionalToolInstallTask {
        package_id: package.id.clone(),
        display_name: package.display_name.clone(),
        tool_type: package.tool_type.clone(),
        version: package.version.clone(),
        stage,
        message: message.into(),
        updated_at: now_iso()?,
        error_code,
    })
}

fn to_inventory_item(tool: OptionalToolVersion) -> OptionalToolInventoryItem {
    let path = PathBuf::from(&tool.path);
    let status = if path.exists() && path.is_file() {
        OptionalToolHealthStatus::Available
    } else {
        OptionalToolHealthStatus::Missing
    };

    OptionalToolInventoryItem {
        id: tool.id,
        tool_type: tool.tool_type,
        version: tool.version,
        path: tool.path,
        is_active: tool.is_active,
        status,
        created_at: tool.created_at,
        updated_at: tool.updated_at,
        details: None,
    }
}

fn parse_first_version_token(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .filter(|token| !token.contains('\\') && !token.contains('/'))
        .flat_map(|token| {
            token.split(|character: char| {
                !(character.is_ascii_alphanumeric()
                    || character == '.'
                    || character == '-'
                    || character == '_')
            })
        })
        .map(|token| token.trim_start_matches(['v', 'V']))
        .find(|token| {
            token.chars().any(|character| character.is_ascii_digit())
                && token.contains('.')
                && !token.to_ascii_lowercase().ends_with(".exe")
        })
        .map(|token| token.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_first_version_token;

    #[test]
    fn parses_redis_windows_version_output() {
        let output = "Redis server v=8.6.2 sha=00000000:0 malloc=libc bits=64";

        assert_eq!(parse_first_version_token(output), Some("8.6.2".to_string()));
    }

    #[test]
    fn parses_restic_version_output() {
        let output = "restic 0.18.1 compiled with go1.25.1 on windows/amd64";

        assert_eq!(
            parse_first_version_token(output),
            Some("0.18.1".to_string())
        );
    }
}

fn verify_optional_tool_binary(
    package: &OptionalToolPackage,
    entry_path: &Path,
) -> Result<String, AppError> {
    let args = match package.tool_type {
        OptionalToolType::Mailpit => vec!["version"],
        OptionalToolType::Cloudflared => vec!["--version"],
        OptionalToolType::Redis => vec!["--version"],
        OptionalToolType::Restic => vec!["version"],
        OptionalToolType::Phpmyadmin => {
            if !entry_path.exists() || !entry_path.is_file() {
                return Err(AppError::new_validation(
                    "OPTIONAL_TOOL_VERIFY_FAILED",
                    "DevNest could not find phpMyAdmin's index.php after install.",
                ));
            }

            return Ok(package.version.clone());
        }
    };

    let mut command = Command::new(entry_path);
    command.args(args);
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_VERIFY_FAILED",
            format!(
                "DevNest could not verify the {} binary after install.",
                package.tool_type.display_name()
            ),
            error.to_string(),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    parse_first_version_token(&combined).ok_or_else(|| {
        AppError::with_details(
            "OPTIONAL_TOOL_VERIFY_FAILED",
            format!(
                "DevNest installed {} but could not read its version output.",
                package.tool_type.display_name()
            ),
            combined.trim().to_string(),
        )
    })
}

fn apply_managed_hosts_entry(domain: &str, target_ip: &str) -> Result<(), AppError> {
    let hosts_path = hosts_file_path();
    match hosts_editor::apply_hosts_entry(&hosts_path, domain, target_ip) {
        Ok(_) => Ok(()),
        Err(error) if error.code == "HOSTS_PERMISSION_DENIED" => {
            apply_hosts_file_with_elevation(&hosts_path, domain, target_ip)
        }
        Err(error) => Err(error),
    }
}

fn remove_managed_hosts_entry(domain: &str) -> Result<(), AppError> {
    let hosts_path = hosts_file_path();
    match hosts_editor::remove_hosts_entry(&hosts_path, domain) {
        Ok(_) => Ok(()),
        Err(error) if error.code == "HOSTS_PERMISSION_DENIED" => {
            remove_hosts_file_with_elevation(&hosts_path, domain)
        }
        Err(error) => Err(error),
    }
}

fn preferred_php_version_for_optional_web_tool(
    connection: &Connection,
) -> Result<String, AppError> {
    Ok(
        crate::storage::repositories::RuntimeVersionRepository::find_active_by_type(
            connection,
            &crate::models::runtime::RuntimeType::Php,
        )?
        .map(|runtime| runtime.version)
        .or_else(|| {
            crate::storage::repositories::RuntimeVersionRepository::list_by_type(
                connection,
                &crate::models::runtime::RuntimeType::Php,
            )
            .ok()
            .and_then(|runtimes| {
                runtimes
                    .into_iter()
                    .max_by_key(|runtime| (runtime.is_active, runtime.updated_at.clone()))
                    .map(|runtime| runtime.version)
            })
        })
        .unwrap_or_else(|| "8.2".to_string()),
    )
}

fn managed_phpmyadmin_config_text(sample_text: &str) -> String {
    let base = sample_text
        .trim_end()
        .trim_end_matches("?>")
        .trim_end()
        .to_string();
    let fallback = r#"<?php
$i = 0;
$i++;
$cfg['Servers'][$i]['auth_type'] = 'cookie';"#
        .to_string();
    let content = if base.is_empty() { fallback } else { base };

    format!(
        "{content}\n\n// DevNest managed local config\n$cfg['blowfish_secret'] = '{}';\n$cfg['Servers'][$i]['host'] = '127.0.0.1';\n$cfg['Servers'][$i]['connect_type'] = 'tcp';\n$cfg['Servers'][$i]['AllowNoPassword'] = true;\n",
        Uuid::new_v4().simple()
    )
}

fn ensure_phpmyadmin_config_file(install_root: &Path) -> Result<(), AppError> {
    let config_path = install_root.join("config.inc.php");
    let sample_path = install_root.join("config.sample.inc.php");
    let sample_text = if sample_path.exists() {
        fs::read_to_string(&sample_path).map_err(|error| {
            AppError::with_details(
                "OPTIONAL_TOOL_INSTALL_FAILED",
                "DevNest could not read phpMyAdmin's sample config file.",
                error.to_string(),
            )
        })?
    } else {
        String::new()
    };

    fs::write(&config_path, managed_phpmyadmin_config_text(&sample_text)).map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_INSTALL_FAILED",
            "DevNest could not write phpMyAdmin's managed config.inc.php file.",
            error.to_string(),
        )
    })
}

fn finalize_phpmyadmin_install(
    connection: &Connection,
    state: &AppState,
    install_entry_path: &Path,
) -> Result<(), AppError> {
    let install_root = install_entry_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "OPTIONAL_TOOL_INSTALL_FAILED",
            "phpMyAdmin install root could not be resolved after extraction.",
        )
    })?;
    let php_version = preferred_php_version_for_optional_web_tool(connection)?;
    ensure_phpmyadmin_config_file(install_root)?;

    for server_type in [
        crate::models::project::ServerType::Apache,
        crate::models::project::ServerType::Nginx,
        crate::models::project::ServerType::Frankenphp,
    ] {
        config_generator::generate_phpmyadmin_config(
            &state.workspace_dir,
            install_root,
            &server_type,
            &php_version,
        )?;
    }

    apply_managed_hosts_entry(config_generator::PHPMYADMIN_DOMAIN, "127.0.0.1")?;

    for service_name in [
        ServiceName::Apache,
        ServiceName::Nginx,
        ServiceName::Frankenphp,
    ] {
        let service = ServiceRepository::get(connection, service_name.as_str())?;
        if matches!(
            service.status,
            crate::models::service::ServiceStatus::Running
        ) {
            service_manager::restart_service(connection, state, service_name)?;
        }
    }

    Ok(())
}

fn finalize_phpmyadmin_removal(connection: &Connection, state: &AppState) -> Result<(), AppError> {
    remove_managed_hosts_entry(config_generator::PHPMYADMIN_DOMAIN)?;

    for server_type in [
        crate::models::project::ServerType::Apache,
        crate::models::project::ServerType::Nginx,
        crate::models::project::ServerType::Frankenphp,
    ] {
        config_generator::remove_managed_config(
            &state.workspace_dir,
            &server_type,
            config_generator::PHPMYADMIN_DOMAIN,
        )?;
    }

    for service_name in [
        ServiceName::Apache,
        ServiceName::Nginx,
        ServiceName::Frankenphp,
    ] {
        let service = ServiceRepository::get(connection, service_name.as_str())?;
        if matches!(
            service.status,
            crate::models::service::ServiceStatus::Running
        ) {
            service_manager::restart_service(connection, state, service_name)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn list_optional_tool_inventory(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<OptionalToolInventoryItem>, AppError> {
    let connection = connection_from_state(&state)?;
    OptionalToolVersionRepository::repair_invalid_versions(&connection)?;
    let tools = OptionalToolVersionRepository::list(&connection)?;
    Ok(tools.into_iter().map(to_inventory_item).collect())
}

#[tauri::command]
pub fn list_optional_tool_packages(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<OptionalToolPackage>, AppError> {
    optional_tools::list_optional_tool_packages(&state.resources_dir)
}

#[tauri::command]
pub fn get_optional_tool_install_task(
    state: tauri::State<'_, AppState>,
) -> Result<Option<OptionalToolInstallTask>, AppError> {
    current_optional_tool_install_task(&state)
}

#[tauri::command]
pub async fn install_optional_tool_package(
    package_id: String,
    app: tauri::AppHandle,
) -> Result<OptionalToolInventoryItem, AppError> {
    let state = app.state::<AppState>();
    let packages = optional_tools::list_optional_tool_packages(&state.resources_dir)?;
    let package = packages
        .into_iter()
        .find(|item| item.id == package_id)
        .ok_or_else(|| {
            AppError::new_validation(
                "OPTIONAL_TOOL_PACKAGE_NOT_FOUND",
                "Optional tool package was not found in the current manifest.",
            )
        })?;

    update_optional_tool_install_task(
        &state,
        Some(install_task_snapshot(
            &package,
            OptionalToolInstallStage::Queued,
            format!("Queued {} for download.", package.display_name),
            None,
        )?),
    )?;

    let workspace_dir = state.workspace_dir.clone();
    let db_path = state.db_path.clone();
    let package_for_install = package.clone();
    let app_handle = app.clone();

    let install_result = tauri::async_runtime::spawn_blocking(move || {
        let app_state = app_handle.state::<AppState>();

        update_optional_tool_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                OptionalToolInstallStage::Downloading,
                format!("Downloading {}...", package_for_install.display_name),
                None,
            )?),
        )?;
        let archive_path =
            optional_tools::download_optional_tool_archive(&package_for_install, &workspace_dir)?;

        update_optional_tool_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                OptionalToolInstallStage::Verifying,
                format!("Verifying {}...", package_for_install.display_name),
                None,
            )?),
        )?;
        optional_tools::verify_archive_checksum(
            &archive_path,
            package_for_install.checksum_sha256.as_deref(),
        )?;

        update_optional_tool_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                OptionalToolInstallStage::Extracting,
                format!("Extracting {}...", package_for_install.display_name),
                None,
            )?),
        )?;
        let extracted_root = optional_tools::extract_optional_tool_package(
            &package_for_install,
            &archive_path,
            &workspace_dir,
        )?;
        let entry_binary_path =
            optional_tools::resolve_package_entry_path(&package_for_install, &extracted_root)?;
        let detected_version =
            verify_optional_tool_binary(&package_for_install, &entry_binary_path)?;

        update_optional_tool_install_task(
            &app_state,
            Some(install_task_snapshot(
                &package_for_install,
                OptionalToolInstallStage::Registering,
                format!(
                    "Registering {} in DevNest...",
                    package_for_install.display_name
                ),
                None,
            )?),
        )?;

        let connection = Connection::open(&db_path)?;
        OptionalToolVersionRepository::clear_active_for_type(
            &connection,
            &package_for_install.tool_type,
        )?;
        let tool = OptionalToolVersionRepository::upsert(
            &connection,
            &package_for_install.tool_type,
            &detected_version,
            &entry_binary_path.to_string_lossy(),
            true,
        )?;

        let mut inventory_item = to_inventory_item(tool);
        inventory_item.details = Some(format!(
            "{} installed into {}.",
            package_for_install.display_name,
            extracted_root.to_string_lossy()
        ));
        Ok::<OptionalToolInventoryItem, AppError>(inventory_item)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_INSTALL_JOIN_FAILED",
            "Optional tool installation did not finish cleanly.",
            error.to_string(),
        )
    })?;

    match install_result {
        Ok(mut tool) => {
            if matches!(package.tool_type, OptionalToolType::Phpmyadmin) {
                let connection = connection_from_state(&state)?;
                if let Err(error) =
                    finalize_phpmyadmin_install(&connection, &state, Path::new(&tool.path))
                {
                    update_optional_tool_install_task(
                        &state,
                        Some(install_task_snapshot(
                            &package,
                            OptionalToolInstallStage::Failed,
                            error.message.clone(),
                            Some(error.code.clone()),
                        )?),
                    )?;
                    return Err(error);
                }
                tool.details = Some(
                    "phpMyAdmin is installed and mapped to https://phpmyadmin.test with HTTP fallback for Apache, Nginx, and FrankenPHP."
                        .to_string(),
                );
            }
            update_optional_tool_install_task(
                &state,
                Some(install_task_snapshot(
                    &package,
                    OptionalToolInstallStage::Completed,
                    format!("{} installed successfully.", package.display_name),
                    None,
                )?),
            )?;
            Ok(tool)
        }
        Err(error) => {
            update_optional_tool_install_task(
                &state,
                Some(install_task_snapshot(
                    &package,
                    OptionalToolInstallStage::Failed,
                    error.message.clone(),
                    Some(error.code.clone()),
                )?),
            )?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn remove_optional_tool(
    tool_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let tool = OptionalToolVersionRepository::get_by_id(&connection, &tool_id)?;

    if matches!(tool.tool_type, OptionalToolType::Mailpit) {
        let service = ServiceRepository::get(&connection, ServiceName::Mailpit.as_str())?;
        if matches!(
            service.status,
            crate::models::service::ServiceStatus::Running
        ) {
            return Err(AppError::new_validation(
                "OPTIONAL_TOOL_IN_USE",
                "Mailpit is running right now. Stop the Mailpit service before uninstalling it.",
            ));
        }
    }

    if matches!(tool.tool_type, OptionalToolType::Cloudflared) {
        let tunnels = state.project_tunnels.lock().map_err(|_| {
            AppError::new_validation(
                "OPTIONAL_TOOL_STATE_LOCK_FAILED",
                "DevNest could not inspect active project tunnels before uninstalling cloudflared.",
            )
        })?;
        if tunnels
            .values()
            .any(|tunnel| !matches!(tunnel.status, crate::models::tunnel::TunnelStatus::Stopped))
        {
            return Err(AppError::new_validation(
                "OPTIONAL_TOOL_IN_USE",
                "A project tunnel is still active. Stop all active tunnels before uninstalling cloudflared.",
            ));
        }
    }

    let tool_path = PathBuf::from(&tool.path);
    let tool_type_dir = downloaded_optional_tool_type_dir(&state.workspace_dir, &tool.tool_type);
    if tool_path.starts_with(&tool_type_dir) {
        let install_root = tool_path
            .ancestors()
            .find(|ancestor| ancestor.parent() == Some(tool_type_dir.as_path()))
            .map(PathBuf::from);
        if let Some(install_root) = install_root {
            if install_root.exists() {
                fs::remove_dir_all(install_root)?;
            }
        }
    }

    let deleted = OptionalToolVersionRepository::delete_by_id(&connection, &tool_id)?;
    if deleted && matches!(tool.tool_type, OptionalToolType::Phpmyadmin) {
        finalize_phpmyadmin_removal(&connection, &state)?;
    }

    Ok(deleted)
}

#[tauri::command]
pub fn reveal_optional_tool_path(
    tool_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let tool = OptionalToolVersionRepository::get_by_id(&connection, &tool_id)?;
    let tool_path = PathBuf::from(&tool.path);
    let folder = if tool_path.exists() && tool_path.is_dir() {
        tool_path
    } else {
        tool_path.parent().map(PathBuf::from).ok_or_else(|| {
            AppError::new_validation(
                "OPTIONAL_TOOL_PATH_OPEN_FAILED",
                "DevNest could not determine which optional tool folder to open.",
            )
        })?
    };

    if !folder.exists() || !folder.is_dir() {
        return Err(AppError::new_validation(
            "OPTIONAL_TOOL_PATH_OPEN_FAILED",
            "The optional tool folder does not exist anymore.",
        ));
    }

    open_folder_in_explorer(&folder)?;
    Ok(true)
}
