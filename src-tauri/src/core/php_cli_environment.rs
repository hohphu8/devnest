use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::runtime::RuntimeType;
use crate::storage::repositories::RuntimeVersionRepository;
use crate::utils::paths::managed_cli_shims_dir;
use crate::utils::windows::{
    broadcast_environment_change, ensure_system_path_dir_with_elevation,
    read_user_environment_variable, write_user_environment_variable,
};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

const PHP_CLI_SHIM_FILE_NAME: &str = "php.cmd";

fn normalize_path_entry(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn path_entries(value: &str) -> Vec<String> {
    value
        .split(';')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn build_user_path_with_preferred_dir(current_path: &str, preferred_dir: &Path) -> String {
    let preferred_dir = preferred_dir.to_string_lossy().to_string();
    let normalized_preferred = normalize_path_entry(&preferred_dir);
    let mut entries = vec![preferred_dir];

    entries.extend(
        path_entries(current_path)
            .into_iter()
            .filter(|entry| normalize_path_entry(entry) != normalized_preferred),
    );

    entries.join(";")
}

fn php_cli_shim_contents(php_binary: &Path, config_path: &Path) -> String {
    let escaped_binary_path = php_binary.to_string_lossy().replace('%', "%%");
    let escaped_config_path = config_path.to_string_lossy().replace('%', "%%");
    let escaped_config_dir = config_path
        .parent()
        .unwrap_or(config_path)
        .to_string_lossy()
        .replace('%', "%%");

    format!(
        "@echo off\r\nset \"PHPRC={escaped_config_dir}\"\r\nset \"PHP_INI_SCAN_DIR=\"\r\n\"{escaped_binary_path}\" -c \"{escaped_config_path}\" %*\r\n"
    )
}

pub fn sync_active_php_cli_environment(
    connection: &Connection,
    workspace_dir: &Path,
) -> Result<Option<String>, AppError> {
    let active_php = RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Php)?;
    let shim_dir = managed_cli_shims_dir(workspace_dir);
    let shim_path = shim_dir.join(PHP_CLI_SHIM_FILE_NAME);

    let Some(runtime) = active_php else {
        if shim_path.exists() {
            fs::remove_file(&shim_path).map_err(|error| {
                AppError::with_details(
                    "PHP_CLI_ENV_SYNC_FAILED",
                    "DevNest could not remove the managed PHP CLI shim after the active PHP runtime changed.",
                    error.to_string(),
                )
            })?;
        }

        return Ok(None);
    };

    let php_binary = PathBuf::from(&runtime.path);
    if !php_binary.exists() || !php_binary.is_file() {
        return Err(AppError::with_details(
            "PHP_CLI_ENV_SYNC_FAILED",
            "The selected active PHP runtime binary is missing, so DevNest could not update the shell `php` command.",
            runtime.path,
        ));
    }
    let config_path =
        runtime_registry::build_managed_php_config(connection, workspace_dir, &runtime.version)?;

    fs::create_dir_all(&shim_dir).map_err(|error| {
        AppError::with_details(
            "PHP_CLI_ENV_SYNC_FAILED",
            "DevNest could not create the managed PHP CLI shim directory.",
            error.to_string(),
        )
    })?;
    fs::write(&shim_path, php_cli_shim_contents(&php_binary, &config_path)).map_err(|error| {
        AppError::with_details(
            "PHP_CLI_ENV_SYNC_FAILED",
            "DevNest could not write the managed PHP CLI shim.",
            error.to_string(),
        )
    })?;

    let current_user_path = read_user_environment_variable("Path")?.unwrap_or_default();
    let next_user_path = build_user_path_with_preferred_dir(&current_user_path, &shim_dir);
    if current_user_path != next_user_path {
        write_user_environment_variable("Path", Some(&next_user_path))?;
    }
    let system_path_warning = match ensure_system_path_dir_with_elevation(&shim_dir) {
        Ok(_) => None,
        Err(error) if error.code == "SYSTEM_ENV_PERMISSION_DENIED" => Some(
            "Administrator permission was denied, so DevNest could not add its PHP CLI shim to the system PATH yet. Existing machine-level PHP entries may still win until you approve that one-time PATH update.".to_string(),
        ),
        Err(error) => return Err(error),
    };
    broadcast_environment_change()?;

    Ok(system_path_warning)
}

#[cfg(test)]
mod tests {
    use super::{build_user_path_with_preferred_dir, php_cli_shim_contents};
    use std::path::Path;

    #[test]
    fn prepends_shim_dir_without_duplicate_entries() {
        let preferred = Path::new(r"C:\Users\demo\AppData\Roaming\DevNest\cli\bin");
        let merged = build_user_path_with_preferred_dir(
            r"C:\Windows\System32;C:\Users\demo\AppData\Roaming\DevNest\cli\bin\;C:\php",
            preferred,
        );

        assert_eq!(
            merged,
            r"C:\Users\demo\AppData\Roaming\DevNest\cli\bin;C:\Windows\System32;C:\php"
        );
    }

    #[test]
    fn adds_preferred_dir_when_user_path_is_empty() {
        let preferred = Path::new(r"C:\Users\demo\AppData\Roaming\DevNest\cli\bin");
        let merged = build_user_path_with_preferred_dir("", preferred);

        assert_eq!(merged, r"C:\Users\demo\AppData\Roaming\DevNest\cli\bin");
    }

    #[test]
    fn renders_php_cmd_shim_that_forwards_arguments_with_managed_config() {
        let shim = php_cli_shim_contents(
            Path::new(r"D:\php\8.4\php.exe"),
            Path::new(r"C:\Users\demo\AppData\Roaming\DevNest\service-state\php\8.4\php.ini"),
        );
        assert_eq!(
            shim,
            "@echo off\r\nset \"PHPRC=C:\\Users\\demo\\AppData\\Roaming\\DevNest\\service-state\\php\\8.4\"\r\nset \"PHP_INI_SCAN_DIR=\"\r\n\"D:\\php\\8.4\\php.exe\" -c \"C:\\Users\\demo\\AppData\\Roaming\\DevNest\\service-state\\php\\8.4\\php.ini\" %*\r\n"
        );
    }
}
