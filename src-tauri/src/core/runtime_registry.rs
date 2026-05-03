use crate::core::runtime_config::{
    load_apache_runtime_config, load_nginx_runtime_config, load_php_runtime_config,
};
use crate::error::AppError;
use crate::models::optional_tool::OptionalToolType;
use crate::models::project::ServerType;
use crate::models::runtime::RuntimeType;
use crate::models::service::ServiceName;
use crate::storage::repositories::{
    OptionalToolVersionRepository, PhpExtensionOverrideRepository, PhpFunctionOverrideRepository,
    RuntimeSuppressionRepository, RuntimeVersionRepository,
};
use crate::utils::paths::{
    bundled_runtime_type_dir, managed_php_state_dir, managed_server_config_dir,
    managed_service_state_dir, normalize_for_config,
};
use crate::utils::process::{configure_background_command, split_command_args};
use rusqlite::Connection;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SUPPORTED_PHP_FAMILIES: &[&str] = &["7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5"];

#[derive(Debug, Clone)]
pub struct RuntimeCommand {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub port: Option<u16>,
    pub log_path: PathBuf,
}

fn runtime_logs_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("runtime-logs")
}

fn service_log_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_LOG_APACHE",
        ServiceName::Nginx => "DEVNEST_LOG_NGINX",
        ServiceName::Frankenphp => "DEVNEST_LOG_FRANKENPHP",
        ServiceName::Mysql => "DEVNEST_LOG_MYSQL",
        ServiceName::Mailpit => "DEVNEST_LOG_MAILPIT",
        ServiceName::Redis => "DEVNEST_LOG_REDIS",
    }
}

fn service_bin_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_RUNTIME_APACHE_BIN",
        ServiceName::Nginx => "DEVNEST_RUNTIME_NGINX_BIN",
        ServiceName::Frankenphp => "DEVNEST_RUNTIME_FRANKENPHP_BIN",
        ServiceName::Mysql => "DEVNEST_RUNTIME_MYSQL_BIN",
        ServiceName::Mailpit => "DEVNEST_RUNTIME_MAILPIT_BIN",
        ServiceName::Redis => "DEVNEST_RUNTIME_REDIS_BIN",
    }
}

fn service_args_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_RUNTIME_APACHE_ARGS",
        ServiceName::Nginx => "DEVNEST_RUNTIME_NGINX_ARGS",
        ServiceName::Frankenphp => "DEVNEST_RUNTIME_FRANKENPHP_ARGS",
        ServiceName::Mysql => "DEVNEST_RUNTIME_MYSQL_ARGS",
        ServiceName::Mailpit => "DEVNEST_RUNTIME_MAILPIT_ARGS",
        ServiceName::Redis => "DEVNEST_RUNTIME_REDIS_ARGS",
    }
}

fn service_cwd_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_RUNTIME_APACHE_CWD",
        ServiceName::Nginx => "DEVNEST_RUNTIME_NGINX_CWD",
        ServiceName::Frankenphp => "DEVNEST_RUNTIME_FRANKENPHP_CWD",
        ServiceName::Mysql => "DEVNEST_RUNTIME_MYSQL_CWD",
        ServiceName::Mailpit => "DEVNEST_RUNTIME_MAILPIT_CWD",
        ServiceName::Redis => "DEVNEST_RUNTIME_REDIS_CWD",
    }
}

fn service_port_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_RUNTIME_APACHE_PORT",
        ServiceName::Nginx => "DEVNEST_RUNTIME_NGINX_PORT",
        ServiceName::Frankenphp => "DEVNEST_RUNTIME_FRANKENPHP_PORT",
        ServiceName::Mysql => "DEVNEST_RUNTIME_MYSQL_PORT",
        ServiceName::Mailpit => "DEVNEST_RUNTIME_MAILPIT_PORT",
        ServiceName::Redis => "DEVNEST_RUNTIME_REDIS_PORT",
    }
}

fn service_version_env_key(service: &ServiceName) -> &'static str {
    match service {
        ServiceName::Apache => "DEVNEST_RUNTIME_APACHE_VERSION",
        ServiceName::Nginx => "DEVNEST_RUNTIME_NGINX_VERSION",
        ServiceName::Frankenphp => "DEVNEST_RUNTIME_FRANKENPHP_VERSION",
        ServiceName::Mysql => "DEVNEST_RUNTIME_MYSQL_VERSION",
        ServiceName::Mailpit => "DEVNEST_RUNTIME_MAILPIT_VERSION",
        ServiceName::Redis => "DEVNEST_RUNTIME_REDIS_VERSION",
    }
}

fn mailpit_smtp_port_env_key() -> &'static str {
    "DEVNEST_RUNTIME_MAILPIT_SMTP_PORT"
}

fn php_bin_env_key(version: &str) -> &'static str {
    match runtime_version_family(version).as_str() {
        "7.4" => "DEVNEST_RUNTIME_PHP_74_BIN",
        "8.0" => "DEVNEST_RUNTIME_PHP_80_BIN",
        "8.1" => "DEVNEST_RUNTIME_PHP_81_BIN",
        "8.2" => "DEVNEST_RUNTIME_PHP_82_BIN",
        "8.3" => "DEVNEST_RUNTIME_PHP_83_BIN",
        "8.4" => "DEVNEST_RUNTIME_PHP_84_BIN",
        "8.5" => "DEVNEST_RUNTIME_PHP_85_BIN",
        _ => "DEVNEST_RUNTIME_PHP_83_BIN",
    }
}

fn path_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

fn resolve_env_binary(value: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(value.trim());
    if path_exists(&candidate) {
        return Some(candidate);
    }

    if !value.contains('\\') && !value.contains('/') {
        return find_in_path(value.trim());
    }

    None
}

fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| path_exists(candidate))
        .cloned()
}

fn find_in_path(file_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let mut executable_names = vec![file_name.to_string()];
    if !file_name.contains('.') {
        executable_names.extend(
            [".exe", ".cmd", ".bat"]
                .into_iter()
                .map(|suffix| format!("{file_name}{suffix}")),
        );
    }

    env::split_paths(&path_var)
        .flat_map(|entry| {
            executable_names
                .iter()
                .map(move |name| entry.join(name))
                .collect::<Vec<_>>()
        })
        .find(|candidate| path_exists(candidate))
}

fn service_candidates(service: &ServiceName) -> Vec<PathBuf> {
    match service {
        ServiceName::Apache => vec![
            PathBuf::from(r"C:\Apache24\bin\httpd.exe"),
            PathBuf::from(r"C:\xampp\apache\bin\httpd.exe"),
            PathBuf::from(r"C:\laragon\bin\apache\httpd.exe"),
            PathBuf::from(r"C:\Program Files\Apache24\bin\httpd.exe"),
        ],
        ServiceName::Nginx => vec![
            PathBuf::from(r"C:\nginx\nginx.exe"),
            PathBuf::from(r"C:\laragon\bin\nginx\nginx.exe"),
            PathBuf::from(r"C:\Program Files\nginx\nginx.exe"),
        ],
        ServiceName::Frankenphp => vec![
            PathBuf::from(r"C:\frankenphp\frankenphp.exe"),
            PathBuf::from(r"C:\Program Files\FrankenPHP\frankenphp.exe"),
            PathBuf::from(r"C:\Program Files\frankenphp\frankenphp.exe"),
        ],
        ServiceName::Mysql => vec![
            PathBuf::from(r"C:\Program Files\MySQL\MySQL Server 8.0\bin\mysqld.exe"),
            PathBuf::from(r"C:\xampp\mysql\bin\mysqld.exe"),
            PathBuf::from(r"C:\laragon\bin\mysql\mysqld.exe"),
        ],
        ServiceName::Mailpit => vec![
            PathBuf::from(r"C:\Program Files\Mailpit\mailpit.exe"),
            PathBuf::from(r"C:\mailpit\mailpit.exe"),
        ],
        ServiceName::Redis => vec![
            PathBuf::from(r"C:\Program Files\Redis\redis-server.exe"),
            PathBuf::from(r"C:\redis\redis-server.exe"),
        ],
    }
}

fn discover_service_binary(service: &ServiceName) -> Option<PathBuf> {
    let env_key = service_bin_env_key(service);
    if let Ok(value) = env::var(env_key) {
        if let Some(candidate) = resolve_env_binary(&value) {
            return Some(candidate);
        }
    }

    first_existing_path(&service_candidates(service)).or_else(|| match service {
        ServiceName::Apache => find_in_path("httpd.exe"),
        ServiceName::Nginx => find_in_path("nginx.exe"),
        ServiceName::Frankenphp => {
            find_in_path("frankenphp.exe").or_else(|| find_in_path("frankenphp"))
        }
        ServiceName::Mysql => find_in_path("mysqld.exe"),
        ServiceName::Mailpit => find_in_path("mailpit.exe").or_else(|| find_in_path("mailpit")),
        ServiceName::Redis => {
            find_in_path("redis-server.exe").or_else(|| find_in_path("redis-server"))
        }
    })
}

fn optional_tool_type_for_service(service: &ServiceName) -> Option<OptionalToolType> {
    match service {
        ServiceName::Mailpit => Some(OptionalToolType::Mailpit),
        _ => None,
    }
}

pub fn resolve_optional_tool_path_from_registry(
    connection: &Connection,
    tool_type: &OptionalToolType,
) -> Result<Option<PathBuf>, AppError> {
    Ok(
        OptionalToolVersionRepository::find_active_by_type(connection, tool_type)?.and_then(
            |tool| {
                let path = PathBuf::from(tool.path);
                if path.exists() && path.is_file() {
                    Some(path)
                } else {
                    None
                }
            },
        ),
    )
}

fn service_runtime_type(service: &ServiceName) -> Option<RuntimeType> {
    match service {
        ServiceName::Apache => Some(RuntimeType::Apache),
        ServiceName::Nginx => Some(RuntimeType::Nginx),
        ServiceName::Frankenphp => Some(RuntimeType::Frankenphp),
        ServiceName::Mysql => Some(RuntimeType::Mysql),
        ServiceName::Mailpit => None,
        ServiceName::Redis => None,
    }
}

fn discover_bundled_service_binaries(resources_dir: &Path, service: &ServiceName) -> Vec<PathBuf> {
    let runtime_type = match service_runtime_type(service) {
        Some(runtime_type) => runtime_type,
        None => return Vec::new(),
    };
    let root = bundled_runtime_type_dir(resources_dir, &runtime_type);

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .map(|path| match service {
            ServiceName::Apache => path.join("bin").join("httpd.exe"),
            ServiceName::Nginx => path.join("nginx.exe"),
            ServiceName::Frankenphp => path.join("frankenphp.exe"),
            ServiceName::Mysql => path.join("bin").join("mysqld.exe"),
            ServiceName::Mailpit => path.join("mailpit.exe"),
            ServiceName::Redis => path.join("redis-server.exe"),
        })
        .filter(|candidate| path_exists(candidate))
        .collect()
}

fn discover_bundled_php_binaries(resources_dir: &Path) -> Vec<PathBuf> {
    let root = bundled_runtime_type_dir(resources_dir, &RuntimeType::Php);
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| entry.ok().map(|item| item.path().join("php.exe")))
        .filter(|candidate| path_exists(candidate))
        .collect()
}

pub(crate) fn runtime_version_family(version: &str) -> String {
    let mut parts = version.trim().split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();

    if major.is_empty() || minor.is_empty() {
        version.trim().to_string()
    } else {
        format!("{major}.{minor}")
    }
}

fn runtime_version_matches(requested: &str, actual: &str) -> bool {
    let requested = requested.trim();
    let actual = actual.trim();
    if requested.eq_ignore_ascii_case(actual) {
        return true;
    }

    runtime_version_family(requested).eq_ignore_ascii_case(&runtime_version_family(actual))
}

fn resolve_runtime_entry_from_registry(
    connection: &Connection,
    runtime_type: &RuntimeType,
    version: Option<&str>,
) -> Result<Option<crate::models::runtime::RuntimeVersion>, AppError> {
    let tracked = if let Some(version) = version {
        if let Some(runtime) =
            RuntimeVersionRepository::find_by_type_and_version(connection, runtime_type, version)?
        {
            Some(runtime)
        } else {
            let family_match = RuntimeVersionRepository::list_by_type(connection, runtime_type)?
                .into_iter()
                .filter(|runtime| runtime_version_matches(version, &runtime.version))
                .max_by_key(|runtime| (runtime.is_active, runtime.updated_at.clone()));

            if family_match.is_some() {
                family_match
            } else {
                RuntimeVersionRepository::find_active_by_type(connection, runtime_type)?
            }
        }
    } else {
        RuntimeVersionRepository::find_active_by_type(connection, runtime_type)?
    };

    if let Some(runtime) = tracked {
        let candidate = PathBuf::from(&runtime.path);
        if path_exists(&candidate) {
            return Ok(Some(runtime));
        }

        return Err(AppError::with_details(
            "RUNTIME_BINARY_NOT_FOUND",
            format!(
                "The tracked {} runtime binary is missing from its saved path.",
                runtime_type.as_str()
            ),
            runtime.path,
        ));
    }

    Ok(None)
}

fn resolve_runtime_path_from_registry(
    connection: &Connection,
    runtime_type: &RuntimeType,
    version: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    Ok(
        resolve_runtime_entry_from_registry(connection, runtime_type, version)?
            .map(|runtime| PathBuf::from(runtime.path)),
    )
}

fn is_runtime_suppressed(
    connection: &Connection,
    runtime_type: &RuntimeType,
    path: &Path,
) -> Result<bool, AppError> {
    RuntimeSuppressionRepository::is_suppressed(connection, runtime_type, path)
}

pub fn resolve_service_binary(
    connection: &Connection,
    service: &ServiceName,
) -> Result<PathBuf, AppError> {
    if let Some(tool_type) = optional_tool_type_for_service(service) {
        if let Some(candidate) = resolve_optional_tool_path_from_registry(connection, &tool_type)? {
            return Ok(candidate);
        }
    }

    if let Some(runtime_type) = service_runtime_type(service) {
        if let Some(candidate) =
            resolve_runtime_path_from_registry(connection, &runtime_type, None)?
        {
            return Ok(candidate);
        }
    }

    let env_key = service_bin_env_key(service);
    if let Ok(value) = env::var(env_key) {
        if let Some(candidate) = resolve_env_binary(&value) {
            if let Some(runtime_type) = service_runtime_type(service) {
                if is_runtime_suppressed(connection, &runtime_type, &candidate)? {
                    return Err(AppError::new_validation(
                        "RUNTIME_BINARY_SUPPRESSED",
                        format!(
                            "{} runtime was removed from DevNest and is currently suppressed from automatic reuse. Install a managed runtime in Settings instead.",
                            service.display_name()
                        ),
                    ));
                }
            }
            return Ok(candidate);
        }

        return Err(AppError::with_details(
            "RUNTIME_BINARY_NOT_FOUND",
            format!(
                "{} runtime is configured but the binary path does not exist.",
                service.display_name()
            ),
            format!("{env_key}={value}"),
        ));
    }

    if let Some(candidate) = discover_service_binary(service) {
        if let Some(runtime_type) = service_runtime_type(service) {
            if !is_runtime_suppressed(connection, &runtime_type, &candidate)? {
                return Ok(candidate);
            }
        } else {
            return Ok(candidate);
        }
    }

    let message = if service_runtime_type(service).is_some() {
        format!(
            "{} runtime binary is not configured. Install a managed runtime in Settings, link an active runtime, set {}, or install the runtime in a standard location.",
            service.display_name(),
            env_key
        )
    } else {
        format!(
            "{} runtime binary is not configured. Set {}, or install the runtime so DevNest can find it in a standard location or in PATH.",
            service.display_name(),
            env_key
        )
    };

    Err(AppError::new_validation(
        "RUNTIME_BINARY_NOT_CONFIGURED",
        message,
    ))
}

pub fn resolve_php_binary(connection: &Connection, version: &str) -> Result<PathBuf, AppError> {
    if let Some(candidate) =
        resolve_runtime_path_from_registry(connection, &RuntimeType::Php, Some(version))?
    {
        return Ok(candidate);
    }

    let env_key = php_bin_env_key(version);
    if let Ok(value) = env::var(env_key) {
        if let Some(candidate) = resolve_env_binary(&value) {
            if is_runtime_suppressed(connection, &RuntimeType::Php, &candidate)? {
                return Err(AppError::new_validation(
                    "RUNTIME_BINARY_SUPPRESSED",
                    format!(
                        "PHP {version} was removed from DevNest and is currently suppressed from automatic reuse. Install it again from Settings to reactivate it."
                    ),
                ));
            }
            return Ok(candidate);
        }

        return Err(AppError::with_details(
            "RUNTIME_BINARY_NOT_FOUND",
            format!("PHP {version} is configured but the binary path does not exist."),
            format!("{env_key}={value}"),
        ));
    }

    if let Some(candidate) = find_in_path("php.exe").or_else(|| find_in_path("php")) {
        if !is_runtime_suppressed(connection, &RuntimeType::Php, &candidate)? {
            return Ok(candidate);
        }
    }

    Err(AppError::new_validation(
        "RUNTIME_BINARY_NOT_CONFIGURED",
        format!(
            "PHP {version} runtime binary is not configured. Install PHP {version} in Settings, link it explicitly, set {env_key}, or install PHP in PATH.",
        ),
    ))
}

fn php_version_slot(version: &str) -> Option<u16> {
    let normalized = version.trim();
    if normalized.is_empty() {
        return None;
    }

    let mut parts = normalized.split('.');
    let major = parts.next()?.parse::<u16>().ok()?;
    let minor = parts.next()?.parse::<u16>().ok()?;
    Some((major * 10) + minor)
}

pub fn php_fastcgi_port(version: &str) -> Result<u16, AppError> {
    let slot = php_version_slot(version).ok_or_else(|| {
        AppError::new_validation(
            "INVALID_PHP_VERSION",
            format!("PHP version `{version}` is not valid for FastCGI routing."),
        )
    })?;

    Ok(9000 + slot)
}

fn php_runtime_home_from_binary(binary_path: &Path) -> Result<PathBuf, AppError> {
    let home = binary_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_PHP_RUNTIME",
            "PHP runtime binary must live inside its runtime directory.",
        )
    })?;

    if !home.exists() || !home.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_PHP_RUNTIME",
            "PHP runtime directory was not found.",
        ));
    }

    Ok(home.to_path_buf())
}

fn php_fastcgi_binary_from_home(runtime_home: &Path) -> Result<PathBuf, AppError> {
    let candidate = runtime_home.join("php-cgi.exe");
    if path_exists(&candidate) {
        Ok(candidate)
    } else {
        Err(AppError::new_validation(
            "PHP_FASTCGI_BINARY_NOT_FOUND",
            "The selected PHP runtime does not include php-cgi.exe, so it cannot serve web requests.",
        ))
    }
}

fn normalize_php_extension_name(file_name: &str) -> Option<String> {
    let normalized = file_name.trim().to_ascii_lowercase();
    let extension = normalized
        .strip_prefix("php_")
        .and_then(|value| value.strip_suffix(".dll"))?
        .trim();

    if extension.is_empty()
        || !extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }

    Some(extension.to_string())
}

pub fn php_extension_enabled_by_default(extension_name: &str) -> bool {
    let normalized = extension_name.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "pdo_firebird" | "pdo_oci" | "snmp")
        && !normalized.starts_with("oci8")
}

fn available_php_extensions_from_dirs(extension_dirs: &[PathBuf]) -> Vec<String> {
    let mut extensions = extension_dirs
        .iter()
        .filter_map(|ext_dir| fs::read_dir(ext_dir).ok())
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter_map(|file_name| normalize_php_extension_name(&file_name))
        .collect::<Vec<_>>();

    extensions.sort();
    extensions.dedup();
    extensions
}

pub fn available_php_extensions(runtime_home: &Path) -> Vec<String> {
    available_php_extensions_from_dirs(&[runtime_home.join("ext")])
}

pub fn managed_php_functions() -> Vec<String> {
    [
        "dl",
        "exec",
        "passthru",
        "pcntl_exec",
        "popen",
        "proc_open",
        "putenv",
        "shell_exec",
        "system",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn enabled_php_extensions_from_available(
    available_extensions: &[String],
    overrides: &HashMap<String, bool>,
) -> Vec<String> {
    available_extensions
        .iter()
        .filter(|extension| {
            overrides
                .get(extension.as_str())
                .copied()
                .unwrap_or_else(|| php_extension_enabled_by_default(extension))
        })
        .cloned()
        .collect()
}

pub fn frankenphp_overlay_ext_dir(workspace_dir: &Path, php_family: &str) -> PathBuf {
    managed_service_state_dir(workspace_dir, &ServiceName::Frankenphp)
        .join("extensions")
        .join(php_family.replace('/', "-"))
}

pub fn frankenphp_available_php_extensions(
    runtime_home: &Path,
    workspace_dir: &Path,
    php_family: &str,
) -> Vec<String> {
    available_php_extensions_from_dirs(&[
        runtime_home.join("ext"),
        frankenphp_overlay_ext_dir(workspace_dir, php_family),
    ])
}

fn php_extension_directive_value(
    runtime_home: &Path,
    overlay_ext_dir: Option<&Path>,
    extension_name: &str,
) -> String {
    let dll_file = format!("php_{extension_name}.dll");

    if let Some(overlay_ext_dir) = overlay_ext_dir {
        let overlay_path = overlay_ext_dir.join(&dll_file);
        if path_exists(&overlay_path) {
            return format!("\"{}\"", normalize_for_config(&overlay_path));
        }
    }

    let bundled_path = runtime_home.join("ext").join(&dll_file);
    if path_exists(&bundled_path) {
        return dll_file;
    }

    dll_file
}

fn is_zend_php_extension(extension_name: &str) -> bool {
    matches!(extension_name, "opcache" | "xdebug")
}

fn disabled_php_functions(overrides: &HashMap<String, bool>) -> Vec<String> {
    managed_php_functions()
        .into_iter()
        .filter(|function_name| !overrides.get(function_name).copied().unwrap_or(true))
        .collect()
}

fn build_php_runtime_config(
    connection: &Connection,
    runtime_home: &Path,
    state_dir: &Path,
    error_log_path: &Path,
    version: &str,
    runtime_id: Option<&str>,
    available_extensions: &[String],
    overlay_ext_dir: Option<&Path>,
) -> Result<PathBuf, AppError> {
    let temp_dir = state_dir.join("tmp");
    let session_dir = state_dir.join("session");
    fs::create_dir_all(&temp_dir).map_err(|error| {
        AppError::with_details(
            "PHP_CONFIG_WRITE_FAILED",
            "Could not create the PHP temporary directory.",
            error.to_string(),
        )
    })?;
    fs::create_dir_all(&session_dir).map_err(|error| {
        AppError::with_details(
            "PHP_CONFIG_WRITE_FAILED",
            "Could not create the PHP session directory.",
            error.to_string(),
        )
    })?;

    let extension_overrides = if let Some(runtime_id) = runtime_id {
        PhpExtensionOverrideRepository::list_for_runtime(
            connection,
            runtime_id,
            version,
            available_extensions,
        )?
        .into_iter()
        .map(|item| (item.extension_name, item.enabled))
        .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };
    let function_overrides = if let Some(runtime_id) = runtime_id {
        PhpFunctionOverrideRepository::list_for_runtime(
            connection,
            runtime_id,
            version,
            &managed_php_functions(),
        )?
        .into_iter()
        .map(|item| (item.function_name, item.enabled))
        .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };

    let enabled_extensions =
        enabled_php_extensions_from_available(available_extensions, &extension_overrides);
    let zend_extension_lines = enabled_extensions
        .iter()
        .filter(|extension| is_zend_php_extension(extension))
        .map(|extension| {
            format!(
                "zend_extension={}",
                php_extension_directive_value(runtime_home, overlay_ext_dir, extension)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let extension_lines = enabled_extensions
        .iter()
        .filter(|extension| !is_zend_php_extension(extension))
        .map(|extension| {
            format!(
                "extension={}",
                php_extension_directive_value(runtime_home, overlay_ext_dir, extension)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let disabled_functions = disabled_php_functions(&function_overrides).join(",");
    let runtime_config = load_php_runtime_config(connection, runtime_id)?;
    let config_path = state_dir.join("php.ini");
    let content = format!(
        "[PHP]\n\
         engine=On\n\
         short_open_tag={short_open_tag}\n\
         expose_php=Off\n\
         max_execution_time={max_execution_time}\n\
         max_input_time={max_input_time}\n\
         memory_limit={memory_limit}\n\
         post_max_size={post_max_size}\n\
         file_uploads={file_uploads}\n\
         upload_max_filesize={upload_max_filesize}\n\
         max_file_uploads={max_file_uploads}\n\
         default_socket_timeout={default_socket_timeout}\n\
         variables_order=GPCS\n\
         extension_dir=\"{extension_dir}\"\n\
         sys_temp_dir=\"{temp_dir}\"\n\
         upload_tmp_dir=\"{temp_dir}\"\n\
         session.save_path=\"{session_dir}\"\n\
         error_log=\"{error_log}\"\n\
         log_errors=On\n\
         display_errors={display_errors}\n\
         error_reporting={error_reporting}\n\
         date.timezone={date_timezone}\n\
         disable_functions={disabled_functions}\n\
         cgi.fix_pathinfo=1\n\
         fastcgi.impersonate=1\n\
         {extension_lines}\n\
         {zend_extension_lines}\n",
        short_open_tag = if runtime_config.short_open_tag {
            "On"
        } else {
            "Off"
        },
        max_execution_time = runtime_config.max_execution_time,
        max_input_time = runtime_config.max_input_time,
        memory_limit = runtime_config.memory_limit,
        post_max_size = runtime_config.post_max_size,
        file_uploads = if runtime_config.file_uploads {
            "On"
        } else {
            "Off"
        },
        upload_max_filesize = runtime_config.upload_max_filesize,
        max_file_uploads = runtime_config.max_file_uploads,
        default_socket_timeout = runtime_config.default_socket_timeout,
        extension_dir = normalize_for_config(&runtime_home.join("ext")),
        temp_dir = normalize_for_config(&temp_dir),
        session_dir = normalize_for_config(&session_dir),
        error_log = normalize_for_config(error_log_path),
        display_errors = if runtime_config.display_errors {
            "On"
        } else {
            "Off"
        },
        error_reporting = runtime_config.error_reporting,
        date_timezone = runtime_config.date_timezone,
        disabled_functions = disabled_functions,
        extension_lines = extension_lines,
        zend_extension_lines = zend_extension_lines,
    );
    write_text_file(&config_path, &content)?;

    Ok(config_path)
}

fn build_php_fastcgi_config(
    connection: &Connection,
    runtime_home: &Path,
    workspace_dir: &Path,
    version: &str,
    runtime_id: Option<&str>,
) -> Result<PathBuf, AppError> {
    let php_family = runtime_version_family(version);
    let state_dir = managed_php_state_dir(workspace_dir, &php_family);
    let error_log_path = service_log_path(workspace_dir, &ServiceName::Apache)
        .with_file_name(format!("php-{php_family}.log"));
    let available_extensions = available_php_extensions(runtime_home);

    build_php_runtime_config(
        connection,
        runtime_home,
        &state_dir,
        &error_log_path,
        &php_family,
        runtime_id,
        &available_extensions,
        None,
    )
}

fn build_frankenphp_php_config(
    connection: &Connection,
    runtime_home: &Path,
    workspace_dir: &Path,
    php_family: &str,
    runtime_id: Option<&str>,
) -> Result<PathBuf, AppError> {
    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Frankenphp)
        .join("php")
        .join(php_family.replace('/', "-"));
    let error_log_path = service_log_path(workspace_dir, &ServiceName::Frankenphp)
        .with_file_name(format!("frankenphp-php-{php_family}.log"));
    let overlay_ext_dir = frankenphp_overlay_ext_dir(workspace_dir, php_family);
    let available_extensions =
        frankenphp_available_php_extensions(runtime_home, workspace_dir, php_family);

    build_php_runtime_config(
        connection,
        runtime_home,
        &state_dir,
        &error_log_path,
        php_family,
        runtime_id,
        &available_extensions,
        Some(&overlay_ext_dir),
    )
}

pub fn resolve_php_fastcgi_runtime(
    connection: &Connection,
    workspace_dir: &Path,
    version: &str,
) -> Result<RuntimeCommand, AppError> {
    let tracked_runtime =
        resolve_runtime_entry_from_registry(connection, &RuntimeType::Php, Some(version))?;
    let php_binary = tracked_runtime
        .as_ref()
        .map(|runtime| PathBuf::from(&runtime.path))
        .unwrap_or(resolve_php_binary(connection, version)?);
    let runtime_home = php_runtime_home_from_binary(&php_binary)?;
    let binary_path = php_fastcgi_binary_from_home(&runtime_home)?;
    let port = php_fastcgi_port(version)?;
    let config_path = build_php_fastcgi_config(
        connection,
        &runtime_home,
        workspace_dir,
        version,
        tracked_runtime.as_ref().map(|runtime| runtime.id.as_str()),
    )?;

    Ok(RuntimeCommand {
        binary_path,
        args: vec![
            "-b".to_string(),
            format!("127.0.0.1:{port}"),
            "-c".to_string(),
            config_path.to_string_lossy().to_string(),
        ],
        env_vars: HashMap::new(),
        working_dir: Some(runtime_home),
        port: Some(port),
        log_path: runtime_logs_dir(workspace_dir).join(format!("php-{version}.log")),
    })
}

pub fn build_managed_php_config(
    connection: &Connection,
    workspace_dir: &Path,
    version: &str,
) -> Result<PathBuf, AppError> {
    let tracked_runtime =
        resolve_runtime_entry_from_registry(connection, &RuntimeType::Php, Some(version))?;
    let php_binary = tracked_runtime
        .as_ref()
        .map(|runtime| PathBuf::from(&runtime.path))
        .unwrap_or(resolve_php_binary(connection, version)?);
    let runtime_home = php_runtime_home_from_binary(&php_binary)?;

    build_php_fastcgi_config(
        connection,
        &runtime_home,
        workspace_dir,
        version,
        tracked_runtime.as_ref().map(|runtime| runtime.id.as_str()),
    )
}

pub fn materialize_runtime_config_file(
    connection: &Connection,
    workspace_dir: &Path,
    runtime: &crate::models::runtime::RuntimeVersion,
) -> Result<PathBuf, AppError> {
    match runtime.runtime_type {
        RuntimeType::Php => build_managed_php_config(connection, workspace_dir, &runtime.version),
        RuntimeType::Apache => {
            let runtime_home =
                runtime_home_from_service_binary(&ServiceName::Apache, Path::new(&runtime.path))?;
            build_apache_bootstrap_config(
                connection,
                &runtime_home,
                workspace_dir,
                parse_service_port(&ServiceName::Apache)?.unwrap_or(80),
                Some(runtime.id.as_str()),
            )
        }
        RuntimeType::Nginx => {
            let runtime_home =
                runtime_home_from_service_binary(&ServiceName::Nginx, Path::new(&runtime.path))?;
            build_nginx_bootstrap_config(
                connection,
                &runtime_home,
                workspace_dir,
                parse_service_port(&ServiceName::Nginx)?.unwrap_or(80),
                Some(runtime.id.as_str()),
            )
        }
        RuntimeType::Frankenphp => {
            let runtime_home = runtime_home_from_service_binary(
                &ServiceName::Frankenphp,
                Path::new(&runtime.path),
            )?;
            build_frankenphp_bootstrap_config(
                &runtime_home,
                workspace_dir,
                parse_service_port(&ServiceName::Frankenphp)?.unwrap_or(80),
            )
        }
        RuntimeType::Mysql => {
            let runtime_home =
                runtime_home_from_service_binary(&ServiceName::Mysql, Path::new(&runtime.path))?;
            build_mysql_bootstrap_config(
                &runtime_home,
                workspace_dir,
                parse_service_port(&ServiceName::Mysql)?.unwrap_or(3306),
            )
        }
    }
}

fn parse_service_port(service: &ServiceName) -> Result<Option<u16>, AppError> {
    match env::var(service_port_env_key(service)) {
        Ok(value) if !value.trim().is_empty() => {
            value.trim().parse::<u16>().map(Some).map_err(|_| {
                AppError::new_validation(
                    "INVALID_RUNTIME_PORT",
                    format!(
                        "{} runtime port must be a valid TCP port.",
                        service.display_name()
                    ),
                )
            })
        }
        _ => Ok(service.default_port()),
    }
}

pub fn mailpit_smtp_port() -> Result<u16, AppError> {
    match env::var(mailpit_smtp_port_env_key()) {
        Ok(value) if !value.trim().is_empty() => value.trim().parse::<u16>().map_err(|_| {
            AppError::new_validation(
                "INVALID_RUNTIME_PORT",
                "Mailpit SMTP port must be a valid TCP port.",
            )
        }),
        _ => Ok(1025),
    }
}

fn service_version(service: &ServiceName) -> String {
    env::var(service_version_env_key(service))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match service {
            ServiceName::Apache => "2.4".to_string(),
            ServiceName::Nginx => "1.25".to_string(),
            ServiceName::Frankenphp => "1.0".to_string(),
            ServiceName::Mysql => "8.0".to_string(),
            ServiceName::Mailpit => "1.0".to_string(),
            ServiceName::Redis => "7.2".to_string(),
        })
}

fn extract_version_after_marker(text: &str, marker: &str) -> Option<String> {
    let marker_index = text.find(marker)?;
    let mut version = String::new();

    for character in text[marker_index + marker.len()..].chars() {
        if character.is_ascii_digit() || character == '.' {
            version.push(character);
            continue;
        }

        if !version.is_empty() {
            break;
        }
    }

    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

fn parse_runtime_version_output(runtime_type: &RuntimeType, output: &str) -> Option<String> {
    match runtime_type {
        RuntimeType::Apache => extract_version_after_marker(output, "Apache/"),
        RuntimeType::Nginx => extract_version_after_marker(output, "nginx/"),
        RuntimeType::Frankenphp => extract_version_after_marker(output, "FrankenPHP ")
            .or_else(|| extract_version_after_marker(output, "FrankenPHP v"))
            .or_else(|| extract_version_after_marker(output, "v")),
        RuntimeType::Mysql => extract_version_after_marker(output, "Ver "),
        RuntimeType::Php => extract_version_after_marker(output, "PHP "),
    }
}

fn parse_frankenphp_embedded_php_version_output(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        extract_version_after_marker(line, " PHP ").or_else(|| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("PHP ") {
                extract_version_after_marker(trimmed, "PHP ")
            } else {
                None
            }
        })
    })
}

fn runtime_version_args(runtime_type: &RuntimeType) -> Vec<String> {
    match runtime_type {
        RuntimeType::Apache => vec!["-v".to_string()],
        RuntimeType::Nginx => vec!["-v".to_string()],
        RuntimeType::Frankenphp => vec!["version".to_string()],
        RuntimeType::Mysql => vec!["--version".to_string()],
        RuntimeType::Php => vec!["-v".to_string()],
    }
}

pub fn frankenphp_embedded_php_family(binary_path: &Path) -> Result<String, AppError> {
    if !path_exists(binary_path) {
        return Err(AppError::new_validation(
            "RUNTIME_BINARY_NOT_FOUND",
            "FrankenPHP runtime binary was not found at the selected path.",
        ));
    }

    let mut command = std::process::Command::new(binary_path);
    command.args(["version"]);
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "RUNTIME_VERIFY_FAILED",
            "Could not inspect the embedded PHP version from the selected FrankenPHP binary.",
            error.to_string(),
        )
    })?;

    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version =
        parse_frankenphp_embedded_php_version_output(&combined_output).ok_or_else(|| {
            AppError::with_details(
                "RUNTIME_VERSION_PARSE_FAILED",
                "DevNest could not detect the embedded PHP version from the selected FrankenPHP binary.",
                combined_output.trim().to_string(),
            )
        })?;

    Ok(runtime_version_family(&version))
}

pub fn frankenphp_managed_php_environment(
    connection: &Connection,
    workspace_dir: &Path,
    binary_path: &Path,
    runtime_id: Option<&str>,
) -> Result<HashMap<String, String>, AppError> {
    let runtime_home = runtime_home_from_service_binary(&ServiceName::Frankenphp, binary_path)?;
    let embedded_php_family = frankenphp_embedded_php_family(binary_path)?;
    let config_path = build_frankenphp_php_config(
        connection,
        &runtime_home,
        workspace_dir,
        &embedded_php_family,
        runtime_id,
    )?;
    let config_dir = config_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "PHP_CONFIG_WRITE_FAILED",
            "Could not resolve the managed FrankenPHP PHP config directory.",
        )
    })?;
    let mut env_vars = HashMap::new();
    env_vars.insert(
        "PHPRC".to_string(),
        config_dir.to_string_lossy().to_string(),
    );
    env_vars.insert("PHP_INI_SCAN_DIR".to_string(), String::new());
    Ok(env_vars)
}

pub fn verify_runtime_binary(
    runtime_type: &RuntimeType,
    binary_path: &Path,
) -> Result<String, AppError> {
    if !path_exists(binary_path) {
        return Err(AppError::new_validation(
            "RUNTIME_BINARY_NOT_FOUND",
            format!(
                "{} runtime binary was not found at the selected path.",
                runtime_type.as_str()
            ),
        ));
    }

    let mut command = std::process::Command::new(binary_path);
    command.args(runtime_version_args(runtime_type));
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "RUNTIME_VERIFY_FAILED",
            format!(
                "Could not execute the selected {} runtime binary.",
                runtime_type.as_str()
            ),
            error.to_string(),
        )
    })?;

    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !output.status.success() && combined_output.trim().is_empty() {
        return Err(AppError::with_details(
            "RUNTIME_VERIFY_FAILED",
            format!(
                "The selected {} runtime binary could not be verified.",
                runtime_type.as_str()
            ),
            format!("exit status: {}", output.status),
        ));
    }

    parse_runtime_version_output(runtime_type, &combined_output).ok_or_else(|| {
        AppError::with_details(
            "RUNTIME_VERSION_PARSE_FAILED",
            format!(
                "DevNest could not detect the {} runtime version from the selected binary.",
                runtime_type.as_str()
            ),
            combined_output.trim().to_string(),
        )
    })
}

fn merge_discovered_runtime(
    connection: &Connection,
    runtime_type: &RuntimeType,
    version: &str,
    path: &Path,
) -> Result<(), AppError> {
    RuntimeSuppressionRepository::remove(connection, runtime_type, path)?;
    let existing = RuntimeVersionRepository::list_by_type(connection, runtime_type)?;
    let runtime_id = format!("{}-{version}", runtime_type.as_str());
    let has_active = existing.iter().any(|runtime| runtime.is_active);
    let existing_entry = existing.iter().find(|runtime| runtime.id == runtime_id);

    if let Some(runtime) = existing_entry {
        let current_path = Path::new(&runtime.path);
        if path_exists(current_path) {
            return Ok(());
        }
    }

    let should_be_active = existing_entry
        .map(|runtime| runtime.is_active)
        .unwrap_or(!has_active);

    RuntimeVersionRepository::upsert(
        connection,
        runtime_type,
        version,
        &path.to_string_lossy(),
        should_be_active,
    )?;

    Ok(())
}

fn runtime_home_from_service_binary(
    service: &ServiceName,
    binary_path: &Path,
) -> Result<PathBuf, AppError> {
    let binary_dir = binary_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_RUNTIME_BINARY",
            format!(
                "{} runtime binary must live inside a runtime directory.",
                service.display_name()
            ),
        )
    })?;

    let runtime_home = match service {
        ServiceName::Apache | ServiceName::Mysql => {
            if binary_dir
                .file_name()
                .map(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
                .unwrap_or(false)
            {
                binary_dir.parent().ok_or_else(|| {
                    AppError::new_validation(
                        "INVALID_RUNTIME_BINARY",
                        format!(
                            "{} runtime binary must live inside a runtime directory.",
                            service.display_name()
                        ),
                    )
                })?
            } else {
                binary_dir
            }
        }
        ServiceName::Nginx
        | ServiceName::Frankenphp
        | ServiceName::Mailpit
        | ServiceName::Redis => binary_dir,
    };

    if !runtime_home.exists() || !runtime_home.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_HOME",
            format!(
                "{} runtime home directory was not found.",
                service.display_name()
            ),
        ));
    }

    Ok(runtime_home.to_path_buf())
}

fn write_text_file(path: &Path, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "SERVICE_CONFIG_WRITE_FAILED",
                "Could not create the managed service config directory.",
                error.to_string(),
            )
        })?;
    }

    fs::write(path, content).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not write the managed service config file.",
            error.to_string(),
        )
    })
}

fn apache_source_config(runtime_home: &Path) -> PathBuf {
    let original = runtime_home
        .join("conf")
        .join("original")
        .join("httpd.conf");
    if original.exists() {
        original
    } else {
        runtime_home.join("conf").join("httpd.conf")
    }
}

fn apache_source_server_root(raw: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("ServerRoot ") {
            let value = trimmed.trim_start_matches("ServerRoot").trim();
            Some(value.trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn build_apache_bootstrap_config(
    connection: &Connection,
    runtime_home: &Path,
    workspace_dir: &Path,
    port: u16,
    runtime_id: Option<&str>,
) -> Result<PathBuf, AppError> {
    let source_config = apache_source_config(runtime_home);
    let raw = fs::read_to_string(&source_config).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_READ_FAILED",
            "Could not read the Apache runtime config template.",
            error.to_string(),
        )
    })?;
    let server_root = normalize_for_config(runtime_home);
    let source_server_root = apache_source_server_root(&raw);
    let has_ssl_module = path_exists(&runtime_home.join("modules").join("mod_ssl.so"));
    let has_socache_module =
        path_exists(&runtime_home.join("modules").join("mod_socache_shmcb.so"));
    let raw = if let Some(source_server_root) = source_server_root {
        raw.replace(&source_server_root, &server_root)
    } else {
        raw
    };
    let include_glob = normalize_for_config(
        &managed_server_config_dir(workspace_dir, &ServerType::Apache).join("*.conf"),
    );
    let runtime_config = load_apache_runtime_config(connection, runtime_id)?;
    let mut lines = Vec::new();
    let mut has_server_root = false;
    let mut has_listen = false;
    let mut has_ssl_listen = false;
    let mut has_include = false;
    let mut has_server_name = false;
    let mut has_ssl_module_line = false;
    let mut has_socache_module_line = false;
    let mut has_timeout = false;
    let mut has_keep_alive = false;
    let mut has_keep_alive_timeout = false;
    let mut has_max_keep_alive_requests = false;
    let mut has_threads_per_child = false;
    let mut has_max_connections_per_child = false;
    let mut has_max_mem_free = false;

    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed == "# LoadModule rewrite_module modules/mod_rewrite.so" {
            lines.push("LoadModule rewrite_module modules/mod_rewrite.so".to_string());
            continue;
        }
        if trimmed == "# LoadModule proxy_module modules/mod_proxy.so" {
            lines.push("LoadModule proxy_module modules/mod_proxy.so".to_string());
            continue;
        }
        if trimmed == "# LoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so" {
            lines.push("LoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so".to_string());
            continue;
        }
        if has_ssl_module && trimmed == "# LoadModule ssl_module modules/mod_ssl.so" {
            lines.push("LoadModule ssl_module modules/mod_ssl.so".to_string());
            has_ssl_module_line = true;
            continue;
        }
        if has_socache_module
            && trimmed == "# LoadModule socache_shmcb_module modules/mod_socache_shmcb.so"
        {
            lines.push("LoadModule socache_shmcb_module modules/mod_socache_shmcb.so".to_string());
            has_socache_module_line = true;
            continue;
        }

        if !trimmed.starts_with('#') && trimmed.starts_with("ServerRoot ") {
            lines.push(format!("ServerRoot \"{server_root}\""));
            has_server_root = true;
            continue;
        }

        if !trimmed.starts_with('#') && trimmed.starts_with("Listen ") {
            if !has_listen {
                lines.push(format!("Listen {port}"));
                has_listen = true;
            }
            if trimmed == "Listen 443" {
                has_ssl_listen = true;
            }
            continue;
        }

        if trimmed.contains("IncludeOptional") && trimmed.contains(&include_glob) {
            has_include = true;
        }

        if !trimmed.starts_with('#') && trimmed.starts_with("ServerName ") {
            has_server_name = true;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("LoadModule ssl_module ") {
            has_ssl_module_line = true;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("LoadModule socache_shmcb_module ") {
            has_socache_module_line = true;
        }

        if !trimmed.starts_with('#') && trimmed.starts_with("Timeout ") {
            lines.push(format!("Timeout {}", runtime_config.timeout));
            has_timeout = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("KeepAlive ") {
            lines.push(format!(
                "KeepAlive {}",
                if runtime_config.keep_alive {
                    "On"
                } else {
                    "Off"
                }
            ));
            has_keep_alive = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("KeepAliveTimeout ") {
            lines.push(format!(
                "KeepAliveTimeout {}",
                runtime_config.keep_alive_timeout
            ));
            has_keep_alive_timeout = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MaxKeepAliveRequests ") {
            lines.push(format!(
                "MaxKeepAliveRequests {}",
                runtime_config.max_keep_alive_requests
            ));
            has_max_keep_alive_requests = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("StartServers ") {
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MaxSpareThreads ") {
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MinSpareThreads ") {
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("ThreadsPerChild ") {
            lines.push(format!(
                "ThreadsPerChild {}",
                runtime_config.threads_per_child
            ));
            has_threads_per_child = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MaxRequestWorkers ") {
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MaxConnectionsPerChild ") {
            lines.push(format!(
                "MaxConnectionsPerChild {}",
                runtime_config.max_connections_per_child
            ));
            has_max_connections_per_child = true;
            continue;
        }
        if !trimmed.starts_with('#') && trimmed.starts_with("MaxMemFree ") {
            lines.push(format!("MaxMemFree {}", runtime_config.max_mem_free));
            has_max_mem_free = true;
            continue;
        }

        lines.push(line.to_string());
    }

    if !has_server_root {
        lines.push(format!("ServerRoot \"{server_root}\""));
    }
    if !has_listen {
        lines.push(format!("Listen {port}"));
    }
    if has_ssl_module && !has_ssl_listen {
        lines.push("Listen 443".to_string());
    }
    if !has_server_name {
        lines.push("ServerName localhost".to_string());
    }
    if has_ssl_module && !has_ssl_module_line {
        lines.push("LoadModule ssl_module modules/mod_ssl.so".to_string());
    }
    if has_socache_module && !has_socache_module_line {
        lines.push("LoadModule socache_shmcb_module modules/mod_socache_shmcb.so".to_string());
    }
    if !has_include {
        lines.push(format!("IncludeOptional \"{include_glob}\""));
    }
    if !has_timeout {
        lines.push(format!("Timeout {}", runtime_config.timeout));
    }
    if !has_keep_alive {
        lines.push(format!(
            "KeepAlive {}",
            if runtime_config.keep_alive {
                "On"
            } else {
                "Off"
            }
        ));
    }
    if !has_keep_alive_timeout {
        lines.push(format!(
            "KeepAliveTimeout {}",
            runtime_config.keep_alive_timeout
        ));
    }
    if !has_max_keep_alive_requests {
        lines.push(format!(
            "MaxKeepAliveRequests {}",
            runtime_config.max_keep_alive_requests
        ));
    }
    if !has_threads_per_child {
        lines.push(format!(
            "ThreadsPerChild {}",
            runtime_config.threads_per_child
        ));
    }
    if !has_max_connections_per_child {
        lines.push(format!(
            "MaxConnectionsPerChild {}",
            runtime_config.max_connections_per_child
        ));
    }
    if !has_max_mem_free {
        lines.push(format!("MaxMemFree {}", runtime_config.max_mem_free));
    }

    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Apache);
    let config_path = state_dir.join("httpd.conf");
    write_text_file(&config_path, &(lines.join("\n") + "\n"))?;

    Ok(config_path)
}

fn build_nginx_bootstrap_config(
    connection: &Connection,
    runtime_home: &Path,
    workspace_dir: &Path,
    port: u16,
    runtime_id: Option<&str>,
) -> Result<PathBuf, AppError> {
    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Nginx);
    let sites_dir = managed_server_config_dir(workspace_dir, &ServerType::Nginx);
    let html_root = runtime_home.join("html");
    let config_path = state_dir.join("nginx.conf");
    let pid_path = normalize_for_config(&state_dir.join("nginx.pid"));
    let mime_types = normalize_for_config(&runtime_home.join("conf").join("mime.types"));
    let fastcgi_params_source = runtime_home.join("conf").join("fastcgi_params");
    let fastcgi_params_target = state_dir.join("fastcgi_params");
    let html_root = normalize_for_config(&html_root);
    let include_glob = normalize_for_config(&sites_dir.join("*.conf"));
    let runtime_config = load_nginx_runtime_config(connection, runtime_id)?;

    fs::create_dir_all(&state_dir).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not create the Nginx service state directory.",
            error.to_string(),
        )
    })?;
    fs::create_dir_all(&sites_dir).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not create the managed Nginx sites directory.",
            error.to_string(),
        )
    })?;
    if path_exists(&fastcgi_params_source) {
        fs::copy(&fastcgi_params_source, &fastcgi_params_target).map_err(|error| {
            AppError::with_details(
                "SERVICE_CONFIG_WRITE_FAILED",
                "Could not stage the Nginx fastcgi_params file.",
                error.to_string(),
            )
        })?;
    }

    let content = format!(
        "worker_processes  {worker_processes};\n\
         daemon off;\n\
         master_process off;\n\
         error_log stderr warn;\n\
         pid \"{pid_path}\";\n\n\
         events {{\n    worker_connections  {worker_connections};\n}}\n\n\
         http {{\n    include       \"{mime_types}\";\n    default_type  application/octet-stream;\n    server_names_hash_bucket_size 128;\n    server_names_hash_max_size 2048;\n    sendfile        on;\n    send_timeout  {timeout};\n    keepalive_timeout  {keep_alive_timeout};\n    keepalive_requests  {keep_alive_requests};\n    access_log off;\n    include \"{include_glob}\";\n\n    server {{\n        listen       {port} default_server;\n        server_name  localhost;\n        root   \"{html_root}\";\n        index  index.html index.htm;\n\n        location / {{\n            try_files $uri $uri/ =404;\n        }}\n    }}\n}}\n",
        worker_processes = runtime_config.worker_processes,
        worker_connections = runtime_config.worker_connections,
        pid_path = pid_path,
        mime_types = mime_types,
        timeout = runtime_config.timeout,
        keep_alive_timeout = if runtime_config.keep_alive {
            runtime_config.keep_alive_timeout
        } else {
            0
        },
        keep_alive_requests = if runtime_config.keep_alive {
            runtime_config.keep_alive_requests
        } else {
            0
        },
        include_glob = include_glob,
        port = port,
        html_root = html_root,
    );
    write_text_file(&config_path, &content)?;

    Ok(config_path)
}

fn build_frankenphp_bootstrap_config(
    runtime_home: &Path,
    workspace_dir: &Path,
    port: u16,
) -> Result<PathBuf, AppError> {
    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Frankenphp);
    let sites_dir = managed_server_config_dir(workspace_dir, &ServerType::Frankenphp);
    let config_path = state_dir.join("Caddyfile");
    let import_glob = normalize_for_config(&sites_dir.join("*.conf"));
    let runtime_home = normalize_for_config(runtime_home);

    fs::create_dir_all(&state_dir).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not create the FrankenPHP service state directory.",
            error.to_string(),
        )
    })?;
    fs::create_dir_all(&sites_dir).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not create the managed FrankenPHP sites directory.",
            error.to_string(),
        )
    })?;

    let content = format!(
        "{{\n    admin off\n    persist_config off\n    http_port {port}\n    https_port 443\n}}\n\nimport \"{import_glob}\"\n\nhttp://localhost:{port} {{\n    root * \"{runtime_home}\"\n    respond \"DevNest FrankenPHP runtime is ready.\" 200\n}}\n",
        port = port,
        import_glob = import_glob,
        runtime_home = runtime_home,
    );
    write_text_file(&config_path, &content)?;

    Ok(config_path)
}

fn mysql_data_dir_initialized(data_dir: &Path) -> bool {
    data_dir.join("mysql").exists()
}

fn mysql_install_binary(runtime_home: &Path) -> Option<PathBuf> {
    let candidates = [
        runtime_home.join("bin").join("mariadb-install-db.exe"),
        runtime_home.join("bin").join("mysql_install_db.exe"),
    ];

    candidates
        .into_iter()
        .find(|candidate| path_exists(candidate))
}

fn initialize_mysql_data_dir(
    runtime_home: &Path,
    data_dir: &Path,
    port: u16,
) -> Result<(), AppError> {
    if let Some(installer) = mysql_install_binary(runtime_home) {
        let mut command = Command::new(&installer);
        command
            .arg(format!("--datadir={}", data_dir.to_string_lossy()))
            .arg(format!("--port={port}"))
            .current_dir(runtime_home.join("bin"));
        configure_background_command(&mut command);
        let output = command.output().map_err(|error| {
            AppError::with_details(
                "MYSQL_INIT_FAILED",
                "Could not start the MySQL data directory bootstrap process.",
                error.to_string(),
            )
        })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() { stderr } else { stdout };
        return Err(AppError::with_details(
            "MYSQL_INIT_FAILED",
            "Could not initialize the MySQL data directory for the managed runtime.",
            details,
        ));
    }

    let mysqld = runtime_home.join("bin").join("mysqld.exe");
    let mut command = Command::new(&mysqld);
    command
        .arg("--initialize-insecure")
        .arg(format!("--basedir={}", runtime_home.to_string_lossy()))
        .arg(format!("--datadir={}", data_dir.to_string_lossy()))
        .current_dir(runtime_home.join("bin"));
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "MYSQL_INIT_FAILED",
            "Could not start the MySQL data directory bootstrap process.",
            error.to_string(),
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() { stderr } else { stdout };
    Err(AppError::with_details(
        "MYSQL_INIT_FAILED",
        "Could not initialize the MySQL data directory for the managed runtime.",
        details,
    ))
}

fn ensure_mysql_data_dir(
    runtime_home: &Path,
    workspace_dir: &Path,
    port: u16,
) -> Result<PathBuf, AppError> {
    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Mysql);
    let data_dir = state_dir.join("data");
    fs::create_dir_all(&state_dir).map_err(|error| {
        AppError::with_details(
            "MYSQL_INIT_FAILED",
            "Could not create the MySQL service state directory.",
            error.to_string(),
        )
    })?;

    if mysql_data_dir_initialized(&data_dir) {
        return Ok(data_dir);
    }

    fs::create_dir_all(&data_dir).map_err(|error| {
        AppError::with_details(
            "MYSQL_INIT_FAILED",
            "Could not create the MySQL data directory.",
            error.to_string(),
        )
    })?;
    initialize_mysql_data_dir(runtime_home, &data_dir, port)?;

    Ok(data_dir)
}

fn build_mysql_bootstrap_config(
    runtime_home: &Path,
    workspace_dir: &Path,
    port: u16,
) -> Result<PathBuf, AppError> {
    let state_dir = managed_service_state_dir(workspace_dir, &ServiceName::Mysql);
    let data_dir = ensure_mysql_data_dir(runtime_home, workspace_dir, port)?;
    let tmp_dir = state_dir.join("tmp");
    fs::create_dir_all(&tmp_dir).map_err(|error| {
        AppError::with_details(
            "SERVICE_CONFIG_WRITE_FAILED",
            "Could not create the MySQL temporary directory.",
            error.to_string(),
        )
    })?;

    let config_path = state_dir.join("my.ini");
    let content = format!(
        "[mysqld]\n\
         basedir={basedir}\n\
         datadir={datadir}\n\
         port={port}\n\
         bind-address=127.0.0.1\n\
         plugin-dir={plugin_dir}\n\
         tmpdir={tmp_dir}\n\
         skip-log-bin\n\
         character-set-server=utf8mb4\n\
         collation-server=utf8mb4_unicode_ci\n",
        basedir = normalize_for_config(runtime_home),
        datadir = normalize_for_config(&data_dir),
        plugin_dir = normalize_for_config(&runtime_home.join("lib").join("plugin")),
        tmp_dir = normalize_for_config(&tmp_dir),
    );
    write_text_file(&config_path, &content)?;

    Ok(config_path)
}

pub fn service_log_path(workspace_dir: &Path, service: &ServiceName) -> PathBuf {
    env::var(service_log_env_key(service))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            runtime_logs_dir(workspace_dir).join(format!("{}.log", service.as_str()))
        })
}

pub fn resolve_service_runtime(
    connection: &Connection,
    workspace_dir: &Path,
    service: ServiceName,
) -> Result<RuntimeCommand, AppError> {
    let binary_path = resolve_service_binary(connection, &service)?;
    let port = parse_service_port(&service)?;
    let args = env::var(service_args_env_key(&service))
        .ok()
        .map(|value| split_command_args(&value))
        .unwrap_or_default();
    let working_dir = env::var(service_cwd_env_key(&service))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let runtime_home = runtime_home_from_service_binary(&service, &binary_path)?;
    let frankenphp_runtime_id = if matches!(service, ServiceName::Frankenphp) {
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?
            .as_ref()
            .map(|runtime| runtime.id.clone())
    } else {
        None
    };

    let (args, working_dir) = if args.is_empty() && working_dir.is_none() {
        match service {
            ServiceName::Apache => {
                let config_path = build_apache_bootstrap_config(
                    connection,
                    &runtime_home,
                    workspace_dir,
                    port.unwrap_or(80),
                    RuntimeVersionRepository::find_active_by_type(
                        connection,
                        &RuntimeType::Apache,
                    )?
                    .as_ref()
                    .map(|runtime| runtime.id.as_str()),
                )?;
                (
                    vec![
                        "-X".to_string(),
                        "-d".to_string(),
                        runtime_home.to_string_lossy().to_string(),
                        "-f".to_string(),
                        config_path.to_string_lossy().to_string(),
                    ],
                    Some(runtime_home.clone()),
                )
            }
            ServiceName::Nginx => {
                let config_path = build_nginx_bootstrap_config(
                    connection,
                    &runtime_home,
                    workspace_dir,
                    port.unwrap_or(80),
                    RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Nginx)?
                        .as_ref()
                        .map(|runtime| runtime.id.as_str()),
                )?;
                (
                    vec![
                        "-p".to_string(),
                        format!("{}\\", runtime_home.to_string_lossy()),
                        "-c".to_string(),
                        config_path.to_string_lossy().to_string(),
                    ],
                    Some(runtime_home.clone()),
                )
            }
            ServiceName::Frankenphp => {
                let config_path = build_frankenphp_bootstrap_config(
                    &runtime_home,
                    workspace_dir,
                    port.unwrap_or(80),
                )?;
                (
                    vec![
                        "run".to_string(),
                        "--config".to_string(),
                        config_path.to_string_lossy().to_string(),
                    ],
                    Some(runtime_home.clone()),
                )
            }
            ServiceName::Mysql => {
                let config_path = build_mysql_bootstrap_config(
                    &runtime_home,
                    workspace_dir,
                    port.unwrap_or(3306),
                )?;
                (
                    vec![
                        format!("--defaults-file={}", config_path.to_string_lossy()),
                        "--console".to_string(),
                    ],
                    Some(runtime_home.join("bin")),
                )
            }
            ServiceName::Mailpit => (
                vec![
                    "--smtp".to_string(),
                    format!("127.0.0.1:{}", mailpit_smtp_port()?),
                    "--listen".to_string(),
                    format!("127.0.0.1:{}", port.unwrap_or(8025)),
                ],
                Some(runtime_home.clone()),
            ),
            ServiceName::Redis => (
                vec![
                    "--bind".to_string(),
                    "127.0.0.1".to_string(),
                    "--port".to_string(),
                    port.unwrap_or(6379).to_string(),
                    "--save".to_string(),
                    "".to_string(),
                    "--appendonly".to_string(),
                    "no".to_string(),
                ],
                Some(runtime_home.clone()),
            ),
        }
    } else {
        (args, working_dir)
    };
    let env_vars = if matches!(service, ServiceName::Frankenphp) {
        frankenphp_managed_php_environment(
            connection,
            workspace_dir,
            &binary_path,
            frankenphp_runtime_id.as_deref(),
        )?
    } else {
        HashMap::new()
    };

    Ok(RuntimeCommand {
        port,
        log_path: service_log_path(workspace_dir, &service),
        binary_path,
        args,
        env_vars,
        working_dir,
    })
}

pub fn sync_runtime_versions(
    connection: &Connection,
    _workspace_dir: &Path,
    resources_dir: &Path,
) -> Result<(), AppError> {
    for service in [
        ServiceName::Apache,
        ServiceName::Nginx,
        ServiceName::Frankenphp,
        ServiceName::Mysql,
    ] {
        let Some(runtime_type) = service_runtime_type(&service) else {
            continue;
        };

        for bundled_path in discover_bundled_service_binaries(resources_dir, &service) {
            let version = verify_runtime_binary(&runtime_type, &bundled_path)?;
            merge_discovered_runtime(connection, &runtime_type, &version, &bundled_path)?;
        }

        if let Some(path) = discover_service_binary(&service) {
            if RuntimeSuppressionRepository::is_suppressed(connection, &runtime_type, &path)? {
                continue;
            }
            let version = verify_runtime_binary(&runtime_type, &path)
                .unwrap_or_else(|_| service_version(&service));
            merge_discovered_runtime(connection, &runtime_type, &version, &path)?;
        }
    }

    let active_php = env::var("DEVNEST_RUNTIME_PHP_ACTIVE")
        .ok()
        .filter(|value| {
            let family = runtime_version_family(value);
            SUPPORTED_PHP_FAMILIES.contains(&family.as_str())
        })
        .map(|value| runtime_version_family(&value));

    for bundled_path in discover_bundled_php_binaries(resources_dir) {
        let version = verify_runtime_binary(&RuntimeType::Php, &bundled_path)?;
        merge_discovered_runtime(connection, &RuntimeType::Php, &version, &bundled_path)?;
    }

    let mut discovered_versions = Vec::new();
    for version in SUPPORTED_PHP_FAMILIES {
        if let Ok(path) = env::var(php_bin_env_key(version)) {
            let candidate = PathBuf::from(path.trim());
            if path_exists(&candidate) {
                if RuntimeSuppressionRepository::is_suppressed(
                    connection,
                    &RuntimeType::Php,
                    &candidate,
                )? {
                    continue;
                }
                discovered_versions.push(((*version).to_string(), candidate));
            }
        }
    }

    if active_php.is_none() && discovered_versions.is_empty() {
        return Ok(());
    }

    for (version, path) in discovered_versions {
        let should_force_active = active_php.as_deref() == Some(version.as_str());
        if should_force_active {
            RuntimeVersionRepository::clear_active_for_type(connection, &RuntimeType::Php)?;
            RuntimeVersionRepository::upsert(
                connection,
                &RuntimeType::Php,
                &version,
                &path.to_string_lossy(),
                true,
            )?;
        } else {
            merge_discovered_runtime(connection, &RuntimeType::Php, &version, &path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_frankenphp_php_config, parse_frankenphp_embedded_php_version_output,
        parse_runtime_version_output, php_extension_enabled_by_default, php_fastcgi_port,
        resolve_php_fastcgi_runtime, resolve_runtime_path_from_registry, resolve_service_runtime,
        runtime_version_family, runtime_version_matches, sync_runtime_versions,
    };
    use crate::models::runtime::RuntimeType;
    use crate::models::service::ServiceName;
    use crate::storage::db::init_database;
    use crate::storage::repositories::{
        RuntimeConfigOverrideRepository, RuntimeSuppressionRepository, RuntimeVersionRepository,
    };
    use rusqlite::Connection;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use uuid::Uuid;

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn setup_test_db() -> (PathBuf, Connection) {
        let db_path = std::env::temp_dir().join(format!(
            "devnest-runtime-registry-{}.sqlite3",
            Uuid::new_v4()
        ));
        init_database(&db_path).expect("database initialization should succeed");
        let connection = Connection::open(&db_path).expect("test database connection should open");
        (db_path, connection)
    }

    fn make_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root should exist");
        root
    }

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // Runtime registry tests serialize environment mutations with env_lock.
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // Runtime registry tests serialize environment mutations with env_lock.
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn parses_php_version_output() {
        let version = parse_runtime_version_output(
            &RuntimeType::Php,
            "PHP 8.2.12 (cli) (built: Oct 24 2023 12:00:00)",
        );
        assert_eq!(version.as_deref(), Some("8.2.12"));
    }

    #[test]
    fn parses_embedded_php_version_from_frankenphp_version_output() {
        let family = parse_frankenphp_embedded_php_version_output(
            "FrankenPHP 1.12.2 PHP 8.5.5 Caddy v2.11.2 h1:example",
        )
        .map(|value| runtime_version_family(&value));
        assert_eq!(family.as_deref(), Some("8.5"));
    }

    #[test]
    fn disables_problematic_php_extensions_by_default() {
        assert!(php_extension_enabled_by_default("mbstring"));
        assert!(php_extension_enabled_by_default("pdo_mysql"));
        assert!(!php_extension_enabled_by_default("snmp"));
        assert!(!php_extension_enabled_by_default("pdo_firebird"));
        assert!(!php_extension_enabled_by_default("pdo_oci"));
        assert!(!php_extension_enabled_by_default("oci8_19"));
    }

    #[test]
    fn parses_nginx_version_output() {
        let version =
            parse_runtime_version_output(&RuntimeType::Nginx, "nginx version: nginx/1.22.0");
        assert_eq!(version.as_deref(), Some("1.22.0"));
    }

    #[test]
    fn parses_apache_version_output() {
        let version = parse_runtime_version_output(
            &RuntimeType::Apache,
            "Server version: Apache/2.4.54 (Win64)",
        );
        assert_eq!(version.as_deref(), Some("2.4.54"));
    }

    #[test]
    fn parses_mysql_version_output() {
        let version = parse_runtime_version_output(
            &RuntimeType::Mysql,
            r"D:\mysql\bin\mysqld.exe  Ver 8.0.30 for Win64 on x86_64 (MySQL Community Server - GPL)",
        );
        assert_eq!(version.as_deref(), Some("8.0.30"));
    }

    #[test]
    fn does_not_rediscover_suppressed_external_runtime() {
        let (db_path, connection) = setup_test_db();
        let root = std::env::temp_dir().join(format!("devnest-runtime-sync-{}", Uuid::new_v4()));
        let resources_dir = root.join("resources");
        fs::create_dir_all(&resources_dir).expect("resources dir should exist");

        let fake_apache = root.join("httpd.exe");
        fs::write(&fake_apache, "fake").expect("fake apache binary should write");

        set_env_var("DEVNEST_RUNTIME_APACHE_BIN", &fake_apache);
        set_env_var("DEVNEST_RUNTIME_APACHE_VERSION", "2.4.99");
        RuntimeSuppressionRepository::suppress(&connection, &RuntimeType::Apache, &fake_apache)
            .expect("suppression should succeed");

        sync_runtime_versions(&connection, &root, &resources_dir).expect("sync should succeed");
        let runtimes = RuntimeVersionRepository::list_by_type(&connection, &RuntimeType::Apache)
            .expect("list should succeed");

        assert!(
            !runtimes
                .iter()
                .any(|runtime| runtime.path == fake_apache.to_string_lossy()),
            "suppressed external runtime should stay hidden"
        );

        remove_env_var("DEVNEST_RUNTIME_APACHE_BIN");
        remove_env_var("DEVNEST_RUNTIME_APACHE_VERSION");
        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_apache_bootstrap_config_for_managed_runtime() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-apache-runtime");
        let runtime_home = root.join("Apache24");
        let workspace_dir = root.join("workspace");
        let binary_path = runtime_home.join("bin").join("httpd.exe");
        let source_config = runtime_home
            .join("conf")
            .join("original")
            .join("httpd.conf");
        let ssl_module = runtime_home.join("modules").join("mod_ssl.so");
        let socache_module = runtime_home.join("modules").join("mod_socache_shmcb.so");

        fs::create_dir_all(binary_path.parent().expect("binary parent"))
            .expect("bin dir should exist");
        fs::create_dir_all(source_config.parent().expect("config parent"))
            .expect("conf dir should exist");
        fs::create_dir_all(ssl_module.parent().expect("modules parent"))
            .expect("modules dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake binary should write");
        fs::write(&ssl_module, "fake").expect("ssl module should write");
        fs::write(&socache_module, "fake").expect("socache module should write");
        fs::write(
            &source_config,
            "ServerRoot \"C:/Apache24-64\"\nListen 80\nLoadModule rewrite_module modules/mod_rewrite.so\n",
        )
        .expect("apache config should write");

        set_env_var("DEVNEST_RUNTIME_APACHE_BIN", &binary_path);
        let runtime = resolve_service_runtime(&connection, &workspace_dir, ServiceName::Apache)
            .expect("apache runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_APACHE_BIN");

        let config_path = PathBuf::from(&runtime.args[4]);
        let config_content =
            fs::read_to_string(&config_path).expect("bootstrap config should exist");

        assert_eq!(runtime.working_dir.as_deref(), Some(runtime_home.as_path()));
        assert!(runtime.args.iter().any(|arg| arg == "-X"));
        assert!(config_content.contains(&format!(
            "ServerRoot \"{}\"",
            runtime_home.to_string_lossy().replace('\\', "/")
        )));
        assert!(config_content.contains("IncludeOptional"));
        assert!(config_content.contains("Listen 443"));
        assert!(config_content.contains("LoadModule ssl_module modules/mod_ssl.so"));
        assert!(
            config_content.contains("LoadModule socache_shmcb_module modules/mod_socache_shmcb.so")
        );
        assert!(!config_content.contains("C:/Apache24-64/htdocs"));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_nginx_bootstrap_config_for_managed_runtime() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-nginx-runtime");
        let runtime_home = root.join("nginx-1.28.0");
        let workspace_dir = root.join("workspace");
        let binary_path = runtime_home.join("nginx.exe");
        let mime_types = runtime_home.join("conf").join("mime.types");
        let fastcgi_params = runtime_home.join("conf").join("fastcgi_params");

        fs::create_dir_all(mime_types.parent().expect("config parent"))
            .expect("conf dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake nginx binary should write");
        fs::write(&mime_types, "types {}").expect("mime types should write");
        fs::write(&fastcgi_params, "fastcgi_param QUERY_STRING $query_string;")
            .expect("fastcgi params should write");

        set_env_var("DEVNEST_RUNTIME_NGINX_BIN", &binary_path);
        let runtime = resolve_service_runtime(&connection, &workspace_dir, ServiceName::Nginx)
            .expect("nginx runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_NGINX_BIN");

        let config_path = PathBuf::from(&runtime.args[3]);
        let config_content =
            fs::read_to_string(&config_path).expect("bootstrap config should exist");

        assert_eq!(runtime.working_dir.as_deref(), Some(runtime_home.as_path()));
        assert!(config_content.contains("daemon off;"));
        assert!(config_content.contains("master_process off;"));
        assert!(config_content.contains("server_names_hash_bucket_size 128;"));
        assert!(config_content.contains("server_names_hash_max_size 2048;"));
        assert!(config_content.contains("include"));
        assert!(
            workspace_dir
                .join("service-state")
                .join("nginx")
                .join("fastcgi_params")
                .exists()
        );

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn applies_runtime_config_overrides_to_managed_php_ini() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-php-config-overrides");
        let workspace_dir = root.join("workspace");
        let runtime_home = root.join("php84");
        let php_binary = runtime_home.join("php.exe");
        let php_cgi_binary = runtime_home.join("php-cgi.exe");
        let ext_dir = runtime_home.join("ext");

        fs::create_dir_all(&ext_dir).expect("ext dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&php_binary, "fake").expect("fake php binary should write");
        fs::write(&php_cgi_binary, "fake").expect("fake php-cgi binary should write");
        fs::write(ext_dir.join("php_mbstring.dll"), "fake").expect("fake extension should write");

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.4.20",
            &php_binary.to_string_lossy(),
            true,
        )
        .expect("runtime should upsert");
        RuntimeConfigOverrideRepository::upsert_many(
            &connection,
            &runtime.id,
            &BTreeMap::from([
                ("memory_limit".to_string(), "768M".to_string()),
                ("display_errors".to_string(), "off".to_string()),
                ("date_timezone".to_string(), "Asia/Ho_Chi_Minh".to_string()),
            ]),
        )
        .expect("runtime config overrides should save");

        set_env_var("DEVNEST_RUNTIME_PHP_84_BIN", &php_binary);
        let runtime_command = resolve_php_fastcgi_runtime(&connection, &workspace_dir, "8.4")
            .expect("php runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_PHP_84_BIN");

        let config_path = PathBuf::from(&runtime_command.args[3]);
        let config_content = fs::read_to_string(&config_path).expect("php.ini should exist");

        assert!(config_path.ends_with(PathBuf::from("php").join("8.4").join("php.ini")));
        assert!(config_content.contains("memory_limit=768M"));
        assert!(config_content.contains("display_errors=Off"));
        assert!(config_content.contains("date.timezone=Asia/Ho_Chi_Minh"));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn applies_runtime_config_overrides_to_apache_bootstrap() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-apache-config-overrides");
        let runtime_home = root.join("Apache24");
        let workspace_dir = root.join("workspace");
        let binary_path = runtime_home.join("bin").join("httpd.exe");
        let source_config = runtime_home
            .join("conf")
            .join("original")
            .join("httpd.conf");

        fs::create_dir_all(binary_path.parent().expect("binary parent"))
            .expect("bin dir should exist");
        fs::create_dir_all(source_config.parent().expect("config parent"))
            .expect("conf dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake binary should write");
        fs::write(
            &source_config,
            "ServerRoot \"C:/Apache24-64\"\nListen 80\nLoadModule rewrite_module modules/mod_rewrite.so\n",
        )
        .expect("apache config should write");

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Apache,
            "2.4.61",
            &binary_path.to_string_lossy(),
            true,
        )
        .expect("runtime should upsert");
        RuntimeConfigOverrideRepository::upsert_many(
            &connection,
            &runtime.id,
            &BTreeMap::from([
                ("timeout".to_string(), "90".to_string()),
                ("keep_alive".to_string(), "off".to_string()),
                ("threads_per_child".to_string(), "200".to_string()),
                ("max_mem_free".to_string(), "1024".to_string()),
            ]),
        )
        .expect("runtime config overrides should save");

        set_env_var("DEVNEST_RUNTIME_APACHE_BIN", &binary_path);
        let runtime_command =
            resolve_service_runtime(&connection, &workspace_dir, ServiceName::Apache)
                .expect("apache runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_APACHE_BIN");

        let config_path = PathBuf::from(&runtime_command.args[4]);
        let config_content = fs::read_to_string(&config_path).expect("apache config should exist");

        assert!(config_content.contains("Timeout 90"));
        assert!(config_content.contains("KeepAlive Off"));
        assert!(config_content.contains("ThreadsPerChild 200"));
        assert!(config_content.contains("MaxMemFree 1024"));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_mysql_bootstrap_config_without_reinitializing_existing_data() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-mysql-runtime");
        let runtime_home = root.join("mariadb");
        let workspace_dir = root.join("workspace");
        let binary_path = runtime_home.join("bin").join("mysqld.exe");
        let data_dir = workspace_dir
            .join("service-state")
            .join("mysql")
            .join("data")
            .join("mysql");

        fs::create_dir_all(binary_path.parent().expect("binary parent"))
            .expect("bin dir should exist");
        fs::create_dir_all(&data_dir).expect("existing mysql data dir should exist");
        fs::create_dir_all(runtime_home.join("lib").join("plugin"))
            .expect("plugin dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake mysql binary should write");

        set_env_var("DEVNEST_RUNTIME_MYSQL_BIN", &binary_path);
        let runtime = resolve_service_runtime(&connection, &workspace_dir, ServiceName::Mysql)
            .expect("mysql runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_MYSQL_BIN");

        let defaults_file = runtime
            .args
            .iter()
            .find(|arg| arg.starts_with("--defaults-file="))
            .expect("mysql defaults file should be configured")
            .trim_start_matches("--defaults-file=");
        let config_content = fs::read_to_string(defaults_file).expect("mysql config should exist");

        assert_eq!(
            runtime.working_dir.as_deref(),
            Some(runtime_home.join("bin").as_path())
        );
        assert!(runtime.args.iter().any(|arg| arg == "--console"));
        assert!(config_content.contains("basedir="));
        assert!(config_content.contains("datadir="));
        assert!(config_content.contains("plugin-dir="));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_mailpit_runtime_command_from_env_binary() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-mailpit-runtime");
        let workspace_dir = root.join("workspace");
        let binary_path = root.join("mailpit.exe");

        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake mailpit binary should write");

        set_env_var("DEVNEST_RUNTIME_MAILPIT_BIN", &binary_path);
        let runtime = resolve_service_runtime(&connection, &workspace_dir, ServiceName::Mailpit)
            .expect("mailpit runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_MAILPIT_BIN");

        assert_eq!(runtime.binary_path, binary_path);
        assert_eq!(runtime.port, Some(8025));
        assert!(runtime.args.iter().any(|arg| arg == "--listen"));
        assert!(runtime.args.iter().any(|arg| arg == "--smtp"));
        assert_eq!(runtime.working_dir.as_deref(), Some(root.as_path()));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_redis_runtime_command_from_env_binary() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-redis-runtime");
        let workspace_dir = root.join("workspace");
        let binary_path = root.join("redis-server.exe");

        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&binary_path, "fake").expect("fake redis binary should write");

        set_env_var("DEVNEST_RUNTIME_REDIS_BIN", &binary_path);
        let runtime = resolve_service_runtime(&connection, &workspace_dir, ServiceName::Redis)
            .expect("redis runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_REDIS_BIN");

        assert_eq!(runtime.binary_path, binary_path);
        assert_eq!(runtime.port, Some(6379));
        assert!(runtime.args.iter().any(|arg| arg == "--bind"));
        assert!(runtime.args.iter().any(|arg| arg == "--appendonly"));
        assert_eq!(runtime.working_dir.as_deref(), Some(root.as_path()));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_php_fastcgi_runtime_with_managed_ini() {
        let _guard = env_lock().lock().expect("env lock should succeed");
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-php-fastcgi");
        let workspace_dir = root.join("workspace");
        let runtime_home = root.join("php84");
        let php_binary = runtime_home.join("php.exe");
        let php_cgi_binary = runtime_home.join("php-cgi.exe");
        let ext_dir = runtime_home.join("ext");

        fs::create_dir_all(&ext_dir).expect("ext dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(&php_binary, "fake").expect("fake php binary should write");
        fs::write(&php_cgi_binary, "fake").expect("fake php-cgi binary should write");
        fs::write(ext_dir.join("php_mbstring.dll"), "fake").expect("fake extension should write");
        fs::write(ext_dir.join("php_pdo_mysql.dll"), "fake").expect("fake extension should write");

        set_env_var("DEVNEST_RUNTIME_PHP_84_BIN", &php_binary);
        let runtime = resolve_php_fastcgi_runtime(&connection, &workspace_dir, "8.4")
            .expect("php fastcgi runtime should resolve");
        remove_env_var("DEVNEST_RUNTIME_PHP_84_BIN");

        let config_path = PathBuf::from(&runtime.args[3]);
        let config_content = fs::read_to_string(&config_path).expect("php.ini should exist");

        assert_eq!(runtime.port, Some(9084));
        assert_eq!(runtime.binary_path, php_cgi_binary);
        assert!(config_path.ends_with(PathBuf::from("php").join("8.4").join("php.ini")));
        assert!(config_content.contains("extension=php_mbstring.dll"));
        assert!(config_content.contains("extension=php_pdo_mysql.dll"));

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn builds_frankenphp_php_config_with_managed_extensions() {
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-frankenphp-php-config");
        let workspace_dir = root.join("workspace");
        let runtime_home = root.join("frankenphp");
        let ext_dir = runtime_home.join("ext");

        fs::create_dir_all(&ext_dir).expect("ext dir should exist");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::write(ext_dir.join("php_mbstring.dll"), "fake").expect("fake extension should write");
        fs::write(ext_dir.join("php_pdo_mysql.dll"), "fake").expect("fake extension should write");

        let config_path =
            build_frankenphp_php_config(&connection, &runtime_home, &workspace_dir, "8.5", None)
                .expect("frankenphp php.ini should build");
        let config_content = fs::read_to_string(&config_path).expect("config should exist");

        assert!(config_content.contains("extension=php_mbstring.dll"));
        assert!(config_content.contains("extension=php_pdo_mysql.dll"));
        assert!(
            config_path.ends_with(
                PathBuf::from("frankenphp")
                    .join("php")
                    .join("8.5")
                    .join("php.ini")
            )
        );

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn maps_php_version_to_fastcgi_port() {
        assert_eq!(php_fastcgi_port("8.1").expect("port should resolve"), 9081);
        assert_eq!(
            php_fastcgi_port("8.4.20").expect("port should resolve"),
            9084
        );
    }

    #[test]
    fn resolves_php_family_version_before_falling_back_to_active_runtime() {
        let (db_path, connection) = setup_test_db();
        let root = make_root("devnest-php-family");
        let php84 = root.join("php84").join("php.exe");
        let php85 = root.join("php85").join("php.exe");
        fs::create_dir_all(php84.parent().expect("php84 parent")).expect("php84 dir should exist");
        fs::create_dir_all(php85.parent().expect("php85 parent")).expect("php85 dir should exist");
        fs::write(&php84, "fake").expect("php84 binary should exist");
        fs::write(&php85, "fake").expect("php85 binary should exist");

        RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.4.20",
            &php84.to_string_lossy(),
            false,
        )
        .expect("php 8.4 should upsert");
        RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.5.5",
            &php85.to_string_lossy(),
            true,
        )
        .expect("php 8.5 should upsert");

        let candidate =
            resolve_runtime_path_from_registry(&connection, &RuntimeType::Php, Some("8.4"))
                .expect("runtime resolution should succeed");

        assert_eq!(candidate.as_deref(), Some(php84.as_path()));

        let family_match = RuntimeVersionRepository::list_by_type(&connection, &RuntimeType::Php)
            .expect("php runtimes should list")
            .into_iter()
            .find(|runtime| runtime_version_matches("8.4", &runtime.version))
            .expect("family match should exist");

        assert_eq!(family_match.version, "8.4.20");

        fs::remove_dir_all(root).ok();
        fs::remove_file(db_path).ok();
    }
}
