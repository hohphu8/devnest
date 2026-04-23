use crate::core::php_extension_packages;
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::runtime::{
    PhpExtensionInstallResult, PhpExtensionPackage, PhpExtensionState, PhpFunctionState,
    RuntimeType,
};
use crate::state::AppState;
use crate::storage::repositories::{
    PhpExtensionOverrideRepository, PhpFunctionOverrideRepository, RuntimeVersionRepository,
};
use rfd::FileDialog;
use rusqlite::Connection;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tauri::Manager;
use zip::ZipArchive;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn php_runtime_home(binary_path: &Path) -> Result<PathBuf, AppError> {
    binary_path.parent().map(Path::to_path_buf).ok_or_else(|| {
        AppError::new_validation(
            "INVALID_PHP_RUNTIME",
            "PHP runtime binary must live inside its runtime directory.",
        )
    })
}

fn available_php_extensions(binary_path: &Path) -> Result<Vec<String>, AppError> {
    let runtime_home = php_runtime_home(binary_path)?;
    Ok(runtime_registry::available_php_extensions(&runtime_home))
}

fn php_version_family(version: &str) -> Result<String, AppError> {
    let mut parts = version.trim().split('.');
    let major = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::new_validation(
                "INVALID_PHP_VERSION",
                "PHP runtime version must include a major version component.",
            )
        })?;
    let minor = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::new_validation(
                "INVALID_PHP_VERSION",
                "PHP runtime version must include a minor version component.",
            )
        })?;

    Ok(format!("{major}.{minor}"))
}

fn normalize_extension_name(file_name: &str) -> Option<String> {
    let normalized = file_name.trim().to_ascii_lowercase();
    normalized
        .strip_prefix("php_")
        .and_then(|value| value.strip_suffix(".dll"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_extension_identifier(extension_name: &str) -> Option<String> {
    let normalized = extension_name.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }

    Some(normalized)
}

fn php_extension_pick_path() -> Result<Option<PathBuf>, AppError> {
    Ok(FileDialog::new()
        .add_filter("PHP Extension", &["dll", "zip"])
        .pick_file())
}

fn install_extension_dll(source_path: &Path, ext_dir: &Path) -> Result<String, AppError> {
    let file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            AppError::new_validation(
                "PHP_EXTENSION_INSTALL_FAILED",
                "The selected PHP extension file name is invalid.",
            )
        })?;
    let extension_name = normalize_extension_name(file_name).ok_or_else(|| {
        AppError::new_validation(
            "PHP_EXTENSION_INSTALL_FAILED",
            "PHP extension DLLs must follow the `php_<name>.dll` naming pattern.",
        )
    })?;

    fs::create_dir_all(ext_dir).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "DevNest could not create the PHP ext directory.",
            error.to_string(),
        )
    })?;
    fs::copy(source_path, ext_dir.join(file_name)).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "DevNest could not copy the selected PHP extension DLL into the runtime.",
            error.to_string(),
        )
    })?;

    Ok(extension_name)
}

fn install_extension_archive(source_path: &Path, ext_dir: &Path) -> Result<Vec<String>, AppError> {
    let archive = File::open(source_path).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "DevNest could not open the selected PHP extension archive.",
            error.to_string(),
        )
    })?;
    let mut zip = ZipArchive::new(archive).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "The selected PHP extension archive is not a valid zip file.",
            error.to_string(),
        )
    })?;

    fs::create_dir_all(ext_dir).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "DevNest could not create the PHP ext directory.",
            error.to_string(),
        )
    })?;

    let mut installed = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| {
            AppError::with_details(
                "PHP_EXTENSION_INSTALL_FAILED",
                "DevNest could not read a file inside the PHP extension archive.",
                error.to_string(),
            )
        })?;
        if entry.is_dir() {
            continue;
        }

        let Some(file_name) = Path::new(entry.name())
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let Some(extension_name) = normalize_extension_name(&file_name) else {
            continue;
        };

        let mut output = File::create(ext_dir.join(&file_name)).map_err(|error| {
            AppError::with_details(
                "PHP_EXTENSION_INSTALL_FAILED",
                "DevNest could not write a PHP extension DLL into the runtime.",
                error.to_string(),
            )
        })?;
        std::io::copy(&mut entry, &mut output).map_err(|error| {
            AppError::with_details(
                "PHP_EXTENSION_INSTALL_FAILED",
                "DevNest could not extract a PHP extension DLL into the runtime.",
                error.to_string(),
            )
        })?;
        installed.push(extension_name);
    }

    installed.sort();
    installed.dedup();
    if installed.is_empty() {
        return Err(AppError::new_validation(
            "PHP_EXTENSION_INSTALL_FAILED",
            "The selected archive did not contain any `php_<name>.dll` extension files.",
        ));
    }

    Ok(installed)
}

#[tauri::command]
pub fn list_php_extensions(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PhpExtensionState>, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP extensions can only be managed for PHP runtimes.",
        ));
    }

    PhpExtensionOverrideRepository::list_for_runtime(
        &connection,
        &runtime.id,
        &runtime.version,
        &available_php_extensions(Path::new(&runtime.path))?,
    )
}

#[tauri::command]
pub fn set_php_extension_enabled(
    runtime_id: String,
    extension_name: String,
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<PhpExtensionState, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP extensions can only be managed for PHP runtimes.",
        ));
    }

    let normalized_extension = extension_name.trim().to_ascii_lowercase();
    let available = available_php_extensions(Path::new(&runtime.path))?;
    if !available.iter().any(|item| item == &normalized_extension) {
        return Err(AppError::new_validation(
            "PHP_EXTENSION_NOT_AVAILABLE",
            format!(
                "PHP {} does not expose the `{}` extension DLL in its ext directory.",
                runtime.version, normalized_extension
            ),
        ));
    }

    PhpExtensionOverrideRepository::set_enabled(
        &connection,
        &runtime.id,
        &normalized_extension,
        enabled,
    )?;

    PhpExtensionOverrideRepository::list_for_runtime(
        &connection,
        &runtime.id,
        &runtime.version,
        &available,
    )?
    .into_iter()
    .find(|item| item.extension_name == normalized_extension)
    .ok_or_else(|| {
        AppError::new_validation(
            "PHP_EXTENSION_NOT_FOUND",
            "Updated PHP extension state could not be reloaded.",
        )
    })
}

#[tauri::command]
pub fn install_php_extension(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<PhpExtensionInstallResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP extensions can only be installed for PHP runtimes.",
        ));
    }

    let Some(source_path) = php_extension_pick_path()? else {
        return Ok(None);
    };

    let runtime_home = php_runtime_home(Path::new(&runtime.path))?;
    let ext_dir = runtime_home.join("ext");
    let source_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let installed_extensions = if source_name.ends_with(".zip") {
        install_extension_archive(&source_path, &ext_dir)?
    } else {
        vec![install_extension_dll(&source_path, &ext_dir)?]
    };

    Ok(Some(PhpExtensionInstallResult {
        runtime_id: runtime.id,
        runtime_version: runtime.version,
        installed_extensions,
        source_path: source_path.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub fn list_php_extension_packages(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PhpExtensionPackage>, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP extension packages can only be listed for PHP runtimes.",
        ));
    }

    php_extension_packages::list_php_extension_packages(
        &state.resources_dir,
        &php_version_family(&runtime.version)?,
    )
}

#[tauri::command]
pub async fn install_php_extension_package(
    runtime_id: String,
    package_id: String,
    app: tauri::AppHandle,
) -> Result<PhpExtensionInstallResult, AppError> {
    let state = app.state::<AppState>();
    let workspace_dir = state.workspace_dir.clone();
    let resources_dir = state.resources_dir.clone();
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let connection = Connection::open(&db_path)?;
        let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

        if !matches!(runtime.runtime_type, RuntimeType::Php) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP extension packages can only be installed for PHP runtimes.",
            ));
        }

        let package = php_extension_packages::list_php_extension_packages(
            &resources_dir,
            &php_version_family(&runtime.version)?,
        )?
        .into_iter()
        .find(|item| item.id == package_id)
        .ok_or_else(|| {
            AppError::new_validation(
                "PHP_EXTENSION_PACKAGE_NOT_FOUND",
                "PHP extension package was not found in the current manifest for this runtime family.",
            )
        })?;

        let archive_path =
            php_extension_packages::download_php_extension_archive(&package, &workspace_dir)?;
        php_extension_packages::verify_archive_checksum(
            &archive_path,
            package.checksum_sha256.as_deref(),
        )?;

        let runtime_home = php_runtime_home(Path::new(&runtime.path))?;
        let ext_dir = runtime_home.join("ext");
        let installed_extensions = php_extension_packages::install_php_extension_package(
            &package,
            &archive_path,
            &ext_dir,
        )?;

        Ok::<PhpExtensionInstallResult, AppError>(PhpExtensionInstallResult {
            runtime_id: runtime.id,
            runtime_version: runtime.version,
            installed_extensions,
            source_path: archive_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_JOIN_FAILED",
            "PHP extension installation did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub fn remove_php_extension(
    runtime_id: String,
    extension_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP extensions can only be removed for PHP runtimes.",
        ));
    }

    let normalized_extension =
        normalize_extension_identifier(&extension_name).ok_or_else(|| {
            AppError::new_validation(
                "INVALID_PHP_EXTENSION",
                "The selected PHP extension name is not valid.",
            )
        })?;
    let runtime_home = php_runtime_home(Path::new(&runtime.path))?;
    let extension_path = runtime_home
        .join("ext")
        .join(format!("php_{normalized_extension}.dll"));

    if !extension_path.exists() {
        return Err(AppError::new_validation(
            "PHP_EXTENSION_NOT_AVAILABLE",
            format!(
                "PHP {} does not expose the `{}` extension DLL in its ext directory.",
                runtime.version, normalized_extension
            ),
        ));
    }

    fs::remove_file(&extension_path).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_REMOVE_FAILED",
            "DevNest could not remove the selected PHP extension DLL from the runtime.",
            error.to_string(),
        )
    })?;
    PhpExtensionOverrideRepository::delete(&connection, &runtime.id, &normalized_extension)?;

    Ok(true)
}

#[tauri::command]
pub fn list_php_functions(
    runtime_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PhpFunctionState>, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP functions can only be managed for PHP runtimes.",
        ));
    }

    PhpFunctionOverrideRepository::list_for_runtime(
        &connection,
        &runtime.id,
        &runtime.version,
        &runtime_registry::managed_php_functions(),
    )
}

#[tauri::command]
pub fn set_php_function_enabled(
    runtime_id: String,
    function_name: String,
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<PhpFunctionState, AppError> {
    let connection = connection_from_state(&state)?;
    let runtime = RuntimeVersionRepository::get_by_id(&connection, &runtime_id)?;

    if !matches!(runtime.runtime_type, RuntimeType::Php) {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_TYPE",
            "PHP functions can only be managed for PHP runtimes.",
        ));
    }

    let normalized_function = function_name.trim().to_ascii_lowercase();
    let managed_functions = runtime_registry::managed_php_functions();
    if !managed_functions
        .iter()
        .any(|item| item == &normalized_function)
    {
        return Err(AppError::new_validation(
            "PHP_FUNCTION_NOT_MANAGED",
            format!(
                "`{}` is outside the managed PHP function list.",
                normalized_function
            ),
        ));
    }

    PhpFunctionOverrideRepository::set_enabled(
        &connection,
        &runtime.id,
        &normalized_function,
        enabled,
    )?;

    PhpFunctionOverrideRepository::list_for_runtime(
        &connection,
        &runtime.id,
        &runtime.version,
        &managed_functions,
    )?
    .into_iter()
    .find(|item| item.function_name == normalized_function)
    .ok_or_else(|| {
        AppError::new_validation(
            "PHP_FUNCTION_NOT_FOUND",
            "Updated PHP function state could not be reloaded.",
        )
    })
}
