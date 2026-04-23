use crate::error::AppError;
use crate::models::runtime::{RuntimeType, RuntimeVersion};
use crate::models::runtime_config::{
    RuntimeConfigField, RuntimeConfigFieldKind, RuntimeConfigFieldOption, RuntimeConfigSchema,
    RuntimeConfigSection, RuntimeConfigValues,
};
use crate::models::service::ServiceName;
use crate::storage::repositories::{RuntimeConfigOverrideRepository, now_iso};
use crate::utils::paths::{managed_php_state_dir, managed_service_state_dir};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PhpRuntimeConfig {
    pub short_open_tag: bool,
    pub max_execution_time: u32,
    pub max_input_time: u32,
    pub memory_limit: String,
    pub post_max_size: String,
    pub file_uploads: bool,
    pub upload_max_filesize: String,
    pub max_file_uploads: u32,
    pub default_socket_timeout: u32,
    pub error_reporting: String,
    pub display_errors: bool,
    pub date_timezone: String,
}

#[derive(Debug, Clone)]
pub struct ApacheRuntimeConfig {
    pub timeout: u32,
    pub keep_alive: bool,
    pub keep_alive_timeout: u32,
    pub max_keep_alive_requests: u32,
    pub threads_per_child: u32,
    pub max_connections_per_child: u32,
    pub max_mem_free: u32,
}

#[derive(Debug, Clone)]
pub struct NginxRuntimeConfig {
    pub timeout: u32,
    pub keep_alive: bool,
    pub keep_alive_timeout: u32,
    pub keep_alive_requests: u32,
    pub worker_processes: String,
    pub worker_connections: u32,
}

fn toggle_options() -> Vec<RuntimeConfigFieldOption> {
    vec![
        RuntimeConfigFieldOption {
            value: "on".to_string(),
            label: "On".to_string(),
        },
        RuntimeConfigFieldOption {
            value: "off".to_string(),
            label: "Off".to_string(),
        },
    ]
}

fn error_reporting_options() -> Vec<RuntimeConfigFieldOption> {
    vec![
        RuntimeConfigFieldOption {
            value: "E_ALL".to_string(),
            label: "E_ALL".to_string(),
        },
        RuntimeConfigFieldOption {
            value: "E_ALL & ~E_DEPRECATED & ~E_STRICT".to_string(),
            label: "E_ALL without deprecated".to_string(),
        },
        RuntimeConfigFieldOption {
            value: "E_ALL & ~E_NOTICE".to_string(),
            label: "E_ALL without notices".to_string(),
        },
        RuntimeConfigFieldOption {
            value: "E_ERROR | E_WARNING | E_PARSE".to_string(),
            label: "Errors and warnings only".to_string(),
        },
    ]
}

fn config_path_for_runtime(runtime: &RuntimeVersion, workspace_dir: &Path) -> PathBuf {
    match runtime.runtime_type {
        RuntimeType::Php => managed_php_state_dir(workspace_dir, &runtime.version).join("php.ini"),
        RuntimeType::Apache => {
            managed_service_state_dir(workspace_dir, &ServiceName::Apache).join("httpd.conf")
        }
        RuntimeType::Nginx => {
            managed_service_state_dir(workspace_dir, &ServiceName::Nginx).join("nginx.conf")
        }
        RuntimeType::Mysql => {
            managed_service_state_dir(workspace_dir, &ServiceName::Mysql).join("my.ini")
        }
    }
}

fn config_access_error(message: &str) -> AppError {
    AppError::new_validation("RUNTIME_CONFIG_NOT_SUPPORTED", message.to_string())
}

pub fn ensure_runtime_config_supported(runtime: &RuntimeVersion) -> Result<(), AppError> {
    match runtime.runtime_type {
        RuntimeType::Php => Ok(()),
        RuntimeType::Apache | RuntimeType::Nginx => {
            if runtime.is_active {
                Ok(())
            } else {
                Err(config_access_error(
                    "Only the active Apache or Nginx runtime exposes the managed config editor.",
                ))
            }
        }
        RuntimeType::Mysql => {
            if runtime.is_active {
                Ok(())
            } else {
                Err(config_access_error(
                    "Only the active MySQL runtime exposes the managed config file.",
                ))
            }
        }
    }
}

fn parse_toggle(key: &str, value: &str) -> Result<bool, AppError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        _ => Err(AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            format!("{key} must be either On or Off."),
        )),
    }
}

fn parse_u32_with_min(key: &str, value: &str, min: u32) -> Result<u32, AppError> {
    let parsed = value.trim().parse::<u32>().map_err(|_| {
        AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            format!("{key} must be a whole number."),
        )
    })?;

    if parsed < min {
        return Err(AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            format!("{key} must be at least {min}."),
        ));
    }

    Ok(parsed)
}

fn parse_size_string(key: &str, value: &str, allow_unlimited: bool) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            format!("{key} is required."),
        ));
    }

    if allow_unlimited && normalized == "-1" {
        return Ok(normalized);
    }

    let valid = if normalized.len() > 1 {
        let (body, suffix) = normalized.split_at(normalized.len() - 1);
        if matches!(suffix, "K" | "M" | "G") {
            !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
        } else {
            normalized
                .chars()
                .all(|character| character.is_ascii_digit())
        }
    } else {
        normalized
            .chars()
            .all(|character| character.is_ascii_digit())
    };

    if !valid {
        return Err(AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            format!("{key} must look like 64M, 1G, 2048, or -1 when unlimited is allowed."),
        ));
    }

    Ok(normalized)
}

fn parse_error_reporting(value: &str) -> Result<String, AppError> {
    let normalized = value.trim();
    let allowed = error_reporting_options()
        .into_iter()
        .map(|option| option.value)
        .collect::<Vec<_>>();

    if allowed.iter().any(|option| option == normalized) {
        return Ok(normalized.to_string());
    }

    Err(AppError::new_validation(
        "RUNTIME_CONFIG_INVALID_VALUE",
        "Error reporting must use one of the supported presets.",
    ))
}

fn parse_timezone(value: &str) -> Result<String, AppError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(AppError::new_validation(
            "RUNTIME_CONFIG_INVALID_VALUE",
            "Date timezone is required.",
        ));
    }

    Ok(normalized.to_string())
}

fn parse_worker_processes(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "auto" {
        return Ok("auto".to_string());
    }

    parse_u32_with_min("worker_processes", &normalized, 1)?;
    Ok(normalized)
}

fn merge_overrides(
    defaults: BTreeMap<String, String>,
    overrides: HashMap<String, String>,
) -> BTreeMap<String, String> {
    let mut values = defaults;
    for (key, value) in overrides {
        values.insert(key, value);
    }
    values
}

fn default_php_values() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("short_open_tag".to_string(), "off".to_string()),
        ("max_execution_time".to_string(), "120".to_string()),
        ("max_input_time".to_string(), "60".to_string()),
        ("memory_limit".to_string(), "512M".to_string()),
        ("post_max_size".to_string(), "64M".to_string()),
        ("file_uploads".to_string(), "on".to_string()),
        ("upload_max_filesize".to_string(), "64M".to_string()),
        ("max_file_uploads".to_string(), "20".to_string()),
        ("default_socket_timeout".to_string(), "60".to_string()),
        ("error_reporting".to_string(), "E_ALL".to_string()),
        ("display_errors".to_string(), "on".to_string()),
        ("date_timezone".to_string(), "UTC".to_string()),
    ])
}

fn default_apache_values() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("timeout".to_string(), "60".to_string()),
        ("keep_alive".to_string(), "on".to_string()),
        ("keep_alive_timeout".to_string(), "5".to_string()),
        ("max_keep_alive_requests".to_string(), "100".to_string()),
        ("threads_per_child".to_string(), "150".to_string()),
        ("max_connections_per_child".to_string(), "0".to_string()),
        ("max_mem_free".to_string(), "2048".to_string()),
    ])
}

fn default_nginx_values() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("timeout".to_string(), "60".to_string()),
        ("keep_alive".to_string(), "on".to_string()),
        ("keep_alive_timeout".to_string(), "65".to_string()),
        ("keep_alive_requests".to_string(), "100".to_string()),
        ("worker_processes".to_string(), "1".to_string()),
        ("worker_connections".to_string(), "1024".to_string()),
    ])
}

pub fn load_php_runtime_config(
    connection: &Connection,
    runtime_id: Option<&str>,
) -> Result<PhpRuntimeConfig, AppError> {
    let values = if let Some(runtime_id) = runtime_id {
        merge_overrides(
            default_php_values(),
            RuntimeConfigOverrideRepository::list_for_runtime(connection, runtime_id)?,
        )
    } else {
        default_php_values()
    };

    Ok(PhpRuntimeConfig {
        short_open_tag: parse_toggle(
            "short_open_tag",
            values
                .get("short_open_tag")
                .map(String::as_str)
                .unwrap_or("off"),
        )?,
        max_execution_time: parse_u32_with_min(
            "max_execution_time",
            values
                .get("max_execution_time")
                .map(String::as_str)
                .unwrap_or("120"),
            0,
        )?,
        max_input_time: parse_u32_with_min(
            "max_input_time",
            values
                .get("max_input_time")
                .map(String::as_str)
                .unwrap_or("60"),
            0,
        )?,
        memory_limit: parse_size_string(
            "memory_limit",
            values
                .get("memory_limit")
                .map(String::as_str)
                .unwrap_or("512M"),
            true,
        )?,
        post_max_size: parse_size_string(
            "post_max_size",
            values
                .get("post_max_size")
                .map(String::as_str)
                .unwrap_or("64M"),
            false,
        )?,
        file_uploads: parse_toggle(
            "file_uploads",
            values
                .get("file_uploads")
                .map(String::as_str)
                .unwrap_or("on"),
        )?,
        upload_max_filesize: parse_size_string(
            "upload_max_filesize",
            values
                .get("upload_max_filesize")
                .map(String::as_str)
                .unwrap_or("64M"),
            false,
        )?,
        max_file_uploads: parse_u32_with_min(
            "max_file_uploads",
            values
                .get("max_file_uploads")
                .map(String::as_str)
                .unwrap_or("20"),
            1,
        )?,
        default_socket_timeout: parse_u32_with_min(
            "default_socket_timeout",
            values
                .get("default_socket_timeout")
                .map(String::as_str)
                .unwrap_or("60"),
            0,
        )?,
        error_reporting: parse_error_reporting(
            values
                .get("error_reporting")
                .map(String::as_str)
                .unwrap_or("E_ALL"),
        )?,
        display_errors: parse_toggle(
            "display_errors",
            values
                .get("display_errors")
                .map(String::as_str)
                .unwrap_or("on"),
        )?,
        date_timezone: parse_timezone(
            values
                .get("date_timezone")
                .map(String::as_str)
                .unwrap_or("UTC"),
        )?,
    })
}

pub fn load_apache_runtime_config(
    connection: &Connection,
    runtime_id: Option<&str>,
) -> Result<ApacheRuntimeConfig, AppError> {
    let values = if let Some(runtime_id) = runtime_id {
        merge_overrides(
            default_apache_values(),
            RuntimeConfigOverrideRepository::list_for_runtime(connection, runtime_id)?,
        )
    } else {
        default_apache_values()
    };

    let threads_per_child = parse_u32_with_min(
        "threads_per_child",
        values
            .get("threads_per_child")
            .map(String::as_str)
            .unwrap_or("150"),
        1,
    )?;

    Ok(ApacheRuntimeConfig {
        timeout: parse_u32_with_min(
            "timeout",
            values.get("timeout").map(String::as_str).unwrap_or("60"),
            1,
        )?,
        keep_alive: parse_toggle(
            "keep_alive",
            values.get("keep_alive").map(String::as_str).unwrap_or("on"),
        )?,
        keep_alive_timeout: parse_u32_with_min(
            "keep_alive_timeout",
            values
                .get("keep_alive_timeout")
                .map(String::as_str)
                .unwrap_or("5"),
            0,
        )?,
        max_keep_alive_requests: parse_u32_with_min(
            "max_keep_alive_requests",
            values
                .get("max_keep_alive_requests")
                .map(String::as_str)
                .unwrap_or("100"),
            0,
        )?,
        threads_per_child,
        max_connections_per_child: parse_u32_with_min(
            "max_connections_per_child",
            values
                .get("max_connections_per_child")
                .map(String::as_str)
                .unwrap_or("0"),
            0,
        )?,
        max_mem_free: parse_u32_with_min(
            "max_mem_free",
            values
                .get("max_mem_free")
                .map(String::as_str)
                .unwrap_or("2048"),
            0,
        )?,
    })
}

pub fn load_nginx_runtime_config(
    connection: &Connection,
    runtime_id: Option<&str>,
) -> Result<NginxRuntimeConfig, AppError> {
    let values = if let Some(runtime_id) = runtime_id {
        merge_overrides(
            default_nginx_values(),
            RuntimeConfigOverrideRepository::list_for_runtime(connection, runtime_id)?,
        )
    } else {
        default_nginx_values()
    };

    Ok(NginxRuntimeConfig {
        timeout: parse_u32_with_min(
            "timeout",
            values.get("timeout").map(String::as_str).unwrap_or("60"),
            1,
        )?,
        keep_alive: parse_toggle(
            "keep_alive",
            values.get("keep_alive").map(String::as_str).unwrap_or("on"),
        )?,
        keep_alive_timeout: parse_u32_with_min(
            "keep_alive_timeout",
            values
                .get("keep_alive_timeout")
                .map(String::as_str)
                .unwrap_or("65"),
            0,
        )?,
        keep_alive_requests: parse_u32_with_min(
            "keep_alive_requests",
            values
                .get("keep_alive_requests")
                .map(String::as_str)
                .unwrap_or("100"),
            0,
        )?,
        worker_processes: parse_worker_processes(
            values
                .get("worker_processes")
                .map(String::as_str)
                .unwrap_or("1"),
        )?,
        worker_connections: parse_u32_with_min(
            "worker_connections",
            values
                .get("worker_connections")
                .map(String::as_str)
                .unwrap_or("1024"),
            1,
        )?,
    })
}

pub fn schema_for_runtime(
    runtime: &RuntimeVersion,
    workspace_dir: &Path,
) -> Result<RuntimeConfigSchema, AppError> {
    ensure_runtime_config_supported(runtime)?;
    let config_path = config_path_for_runtime(runtime, workspace_dir)
        .to_string_lossy()
        .to_string();

    let (supports_editor, open_file_only, sections) = match runtime.runtime_type {
        RuntimeType::Php => (
            true,
            false,
            vec![
                RuntimeConfigSection {
                    id: "php-core".to_string(),
                    title: "PHP Core".to_string(),
                    description: Some(
                        "Generated into DevNest-managed php.ini alongside extensions and guarded functions."
                            .to_string(),
                    ),
                    fields: vec![
                        RuntimeConfigField {
                            key: "short_open_tag".to_string(),
                            label: "Short Open Tag".to_string(),
                            description: Some("Enable or disable short PHP open tags.".to_string()),
                            kind: RuntimeConfigFieldKind::Toggle,
                            placeholder: None,
                            options: toggle_options(),
                        },
                        RuntimeConfigField {
                            key: "max_execution_time".to_string(),
                            label: "Max Execution Time".to_string(),
                            description: Some("Seconds before a PHP request times out.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("120".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "max_input_time".to_string(),
                            label: "Max Input Time".to_string(),
                            description: Some("Seconds allowed to parse request input.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("60".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "memory_limit".to_string(),
                            label: "Memory Limit".to_string(),
                            description: Some("Examples: 512M, 1G, or -1 for unlimited.".to_string()),
                            kind: RuntimeConfigFieldKind::Size,
                            placeholder: Some("512M".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "post_max_size".to_string(),
                            label: "Post Max Size".to_string(),
                            description: Some("Maximum POST body size.".to_string()),
                            kind: RuntimeConfigFieldKind::Size,
                            placeholder: Some("64M".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "file_uploads".to_string(),
                            label: "File Uploads".to_string(),
                            description: Some("Allow uploaded files in PHP requests.".to_string()),
                            kind: RuntimeConfigFieldKind::Toggle,
                            placeholder: None,
                            options: toggle_options(),
                        },
                        RuntimeConfigField {
                            key: "upload_max_filesize".to_string(),
                            label: "Upload Max Filesize".to_string(),
                            description: Some("Maximum size of each uploaded file.".to_string()),
                            kind: RuntimeConfigFieldKind::Size,
                            placeholder: Some("64M".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "max_file_uploads".to_string(),
                            label: "Max File Uploads".to_string(),
                            description: Some("How many files one request may upload.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("20".to_string()),
                            options: vec![],
                        },
                    ],
                },
                RuntimeConfigSection {
                    id: "php-runtime".to_string(),
                    title: "Errors and Runtime".to_string(),
                    description: Some("Common runtime, error, and timezone controls.".to_string()),
                    fields: vec![
                        RuntimeConfigField {
                            key: "default_socket_timeout".to_string(),
                            label: "Default Socket Timeout".to_string(),
                            description: Some("Network socket timeout in seconds.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("60".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "error_reporting".to_string(),
                            label: "Error Reporting".to_string(),
                            description: Some("Choose one supported error reporting preset.".to_string()),
                            kind: RuntimeConfigFieldKind::Select,
                            placeholder: None,
                            options: error_reporting_options(),
                        },
                        RuntimeConfigField {
                            key: "display_errors".to_string(),
                            label: "Display Errors".to_string(),
                            description: Some("Show PHP errors in responses.".to_string()),
                            kind: RuntimeConfigFieldKind::Toggle,
                            placeholder: None,
                            options: toggle_options(),
                        },
                        RuntimeConfigField {
                            key: "date_timezone".to_string(),
                            label: "Date Timezone".to_string(),
                            description: Some("Examples: UTC, Asia/Ho_Chi_Minh.".to_string()),
                            kind: RuntimeConfigFieldKind::Text,
                            placeholder: Some("UTC".to_string()),
                            options: vec![],
                        },
                    ],
                },
            ],
        ),
        RuntimeType::Apache => (
            true,
            false,
            vec![
                RuntimeConfigSection {
                    id: "apache-connections".to_string(),
                    title: "Connections".to_string(),
                    description: Some("Managed Apache connection and keep-alive directives.".to_string()),
                    fields: vec![
                        RuntimeConfigField {
                            key: "timeout".to_string(),
                            label: "Timeout".to_string(),
                            description: Some("Connection timeout in seconds.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("60".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "keep_alive".to_string(),
                            label: "KeepAlive".to_string(),
                            description: Some("Enable or disable HTTP keep-alive.".to_string()),
                            kind: RuntimeConfigFieldKind::Toggle,
                            placeholder: None,
                            options: toggle_options(),
                        },
                        RuntimeConfigField {
                            key: "keep_alive_timeout".to_string(),
                            label: "KeepAlive Timeout".to_string(),
                            description: Some("Seconds to keep the connection open.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("5".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "max_keep_alive_requests".to_string(),
                            label: "Max KeepAlive Requests".to_string(),
                            description: Some("Requests allowed per keep-alive connection.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("100".to_string()),
                            options: vec![],
                        },
                    ],
                },
                RuntimeConfigSection {
                    id: "apache-winnt-mpm".to_string(),
                    title: "WinNT MPM".to_string(),
                    description: Some(
                        "Windows-safe Apache thread pool controls for the WinNT MPM.".to_string(),
                    ),
                    fields: vec![
                        RuntimeConfigField {
                            key: "threads_per_child".to_string(),
                            label: "ThreadsPerChild".to_string(),
                            description: Some(
                                "Worker thread count for the single WinNT server process."
                                    .to_string(),
                            ),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("150".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "max_connections_per_child".to_string(),
                            label: "MaxConnectionsPerChild".to_string(),
                            description: Some(
                                "How many connections the WinNT process serves before recycling. 0 means unlimited."
                                    .to_string(),
                            ),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("0".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "max_mem_free".to_string(),
                            label: "MaxMemFree".to_string(),
                            description: Some(
                                "Maximum free KB each threaded allocator may hold before releasing memory."
                                    .to_string(),
                            ),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("2048".to_string()),
                            options: vec![],
                        },
                    ],
                },
            ],
        ),
        RuntimeType::Nginx => (
            true,
            false,
            vec![
                RuntimeConfigSection {
                    id: "nginx-connections".to_string(),
                    title: "Connections".to_string(),
                    description: Some("Nginx-native keep-alive and request timeout controls.".to_string()),
                    fields: vec![
                        RuntimeConfigField {
                            key: "timeout".to_string(),
                            label: "Timeout".to_string(),
                            description: Some("Mapped to send_timeout in seconds.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("60".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "keep_alive".to_string(),
                            label: "KeepAlive".to_string(),
                            description: Some("Turn HTTP keep-alive on or off.".to_string()),
                            kind: RuntimeConfigFieldKind::Toggle,
                            placeholder: None,
                            options: toggle_options(),
                        },
                        RuntimeConfigField {
                            key: "keep_alive_timeout".to_string(),
                            label: "KeepAlive Timeout".to_string(),
                            description: Some("Seconds to keep idle connections alive.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("65".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "keep_alive_requests".to_string(),
                            label: "KeepAlive Requests".to_string(),
                            description: Some("Maximum requests per keep-alive connection.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("100".to_string()),
                            options: vec![],
                        },
                    ],
                },
                RuntimeConfigSection {
                    id: "nginx-workers".to_string(),
                    title: "Workers".to_string(),
                    description: Some("Nginx worker process and concurrency controls.".to_string()),
                    fields: vec![
                        RuntimeConfigField {
                            key: "worker_processes".to_string(),
                            label: "Worker Processes".to_string(),
                            description: Some("Use `auto` or a fixed process count.".to_string()),
                            kind: RuntimeConfigFieldKind::Text,
                            placeholder: Some("1".to_string()),
                            options: vec![],
                        },
                        RuntimeConfigField {
                            key: "worker_connections".to_string(),
                            label: "Worker Connections".to_string(),
                            description: Some("Maximum connections per worker process.".to_string()),
                            kind: RuntimeConfigFieldKind::Number,
                            placeholder: Some("1024".to_string()),
                            options: vec![],
                        },
                    ],
                },
            ],
        ),
        RuntimeType::Mysql => (false, true, vec![]),
    };

    Ok(RuntimeConfigSchema {
        runtime_id: runtime.id.clone(),
        runtime_type: runtime.runtime_type.clone(),
        runtime_version: runtime.version.clone(),
        config_path,
        supports_editor,
        open_file_only,
        sections,
    })
}

pub fn values_for_runtime(
    connection: &Connection,
    runtime: &RuntimeVersion,
    workspace_dir: &Path,
) -> Result<RuntimeConfigValues, AppError> {
    ensure_runtime_config_supported(runtime)?;
    let values = match runtime.runtime_type {
        RuntimeType::Php => {
            let config = load_php_runtime_config(connection, Some(&runtime.id))?;
            BTreeMap::from([
                (
                    "short_open_tag".to_string(),
                    if config.short_open_tag { "on" } else { "off" }.to_string(),
                ),
                (
                    "max_execution_time".to_string(),
                    config.max_execution_time.to_string(),
                ),
                (
                    "max_input_time".to_string(),
                    config.max_input_time.to_string(),
                ),
                ("memory_limit".to_string(), config.memory_limit),
                ("post_max_size".to_string(), config.post_max_size),
                (
                    "file_uploads".to_string(),
                    if config.file_uploads { "on" } else { "off" }.to_string(),
                ),
                (
                    "upload_max_filesize".to_string(),
                    config.upload_max_filesize,
                ),
                (
                    "max_file_uploads".to_string(),
                    config.max_file_uploads.to_string(),
                ),
                (
                    "default_socket_timeout".to_string(),
                    config.default_socket_timeout.to_string(),
                ),
                ("error_reporting".to_string(), config.error_reporting),
                (
                    "display_errors".to_string(),
                    if config.display_errors { "on" } else { "off" }.to_string(),
                ),
                ("date_timezone".to_string(), config.date_timezone),
            ])
        }
        RuntimeType::Apache => {
            let config = load_apache_runtime_config(connection, Some(&runtime.id))?;
            BTreeMap::from([
                ("timeout".to_string(), config.timeout.to_string()),
                (
                    "keep_alive".to_string(),
                    if config.keep_alive { "on" } else { "off" }.to_string(),
                ),
                (
                    "keep_alive_timeout".to_string(),
                    config.keep_alive_timeout.to_string(),
                ),
                (
                    "max_keep_alive_requests".to_string(),
                    config.max_keep_alive_requests.to_string(),
                ),
                (
                    "threads_per_child".to_string(),
                    config.threads_per_child.to_string(),
                ),
                (
                    "max_connections_per_child".to_string(),
                    config.max_connections_per_child.to_string(),
                ),
                ("max_mem_free".to_string(), config.max_mem_free.to_string()),
            ])
        }
        RuntimeType::Nginx => {
            let config = load_nginx_runtime_config(connection, Some(&runtime.id))?;
            BTreeMap::from([
                ("timeout".to_string(), config.timeout.to_string()),
                (
                    "keep_alive".to_string(),
                    if config.keep_alive { "on" } else { "off" }.to_string(),
                ),
                (
                    "keep_alive_timeout".to_string(),
                    config.keep_alive_timeout.to_string(),
                ),
                (
                    "keep_alive_requests".to_string(),
                    config.keep_alive_requests.to_string(),
                ),
                ("worker_processes".to_string(), config.worker_processes),
                (
                    "worker_connections".to_string(),
                    config.worker_connections.to_string(),
                ),
            ])
        }
        RuntimeType::Mysql => BTreeMap::new(),
    };

    Ok(RuntimeConfigValues {
        runtime_id: runtime.id.clone(),
        runtime_type: runtime.runtime_type.clone(),
        runtime_version: runtime.version.clone(),
        config_path: config_path_for_runtime(runtime, workspace_dir)
            .to_string_lossy()
            .to_string(),
        values,
        updated_at: now_iso()?,
    })
}

pub fn validate_patch(
    runtime: &RuntimeVersion,
    patch: &HashMap<String, String>,
) -> Result<BTreeMap<String, String>, AppError> {
    ensure_runtime_config_supported(runtime)?;
    if runtime.runtime_type == RuntimeType::Mysql {
        return Err(config_access_error(
            "MySQL currently supports opening the managed config file only.",
        ));
    }

    let mut normalized = BTreeMap::new();
    match runtime.runtime_type {
        RuntimeType::Php => {
            for (key, value) in patch {
                match key.as_str() {
                    "short_open_tag" | "file_uploads" | "display_errors" => {
                        normalized.insert(
                            key.clone(),
                            if parse_toggle(key, value)? {
                                "on"
                            } else {
                                "off"
                            }
                            .to_string(),
                        );
                    }
                    "max_execution_time" | "max_input_time" | "default_socket_timeout" => {
                        normalized
                            .insert(key.clone(), parse_u32_with_min(key, value, 0)?.to_string());
                    }
                    "max_file_uploads" => {
                        normalized
                            .insert(key.clone(), parse_u32_with_min(key, value, 1)?.to_string());
                    }
                    "memory_limit" => {
                        normalized.insert(key.clone(), parse_size_string(key, value, true)?);
                    }
                    "post_max_size" | "upload_max_filesize" => {
                        normalized.insert(key.clone(), parse_size_string(key, value, false)?);
                    }
                    "error_reporting" => {
                        normalized.insert(key.clone(), parse_error_reporting(value)?);
                    }
                    "date_timezone" => {
                        normalized.insert(key.clone(), parse_timezone(value)?);
                    }
                    _ => {
                        return Err(AppError::new_validation(
                            "RUNTIME_CONFIG_INVALID_VALUE",
                            format!("{key} is not a supported PHP runtime config field."),
                        ));
                    }
                }
            }
        }
        RuntimeType::Apache => {
            for (key, value) in patch {
                match key.as_str() {
                    "keep_alive" => {
                        normalized.insert(
                            key.clone(),
                            if parse_toggle(key, value)? {
                                "on"
                            } else {
                                "off"
                            }
                            .to_string(),
                        );
                    }
                    "timeout"
                    | "keep_alive_timeout"
                    | "max_keep_alive_requests"
                    | "threads_per_child"
                    | "max_connections_per_child"
                    | "max_mem_free" => {
                        normalized
                            .insert(key.clone(), parse_u32_with_min(key, value, 0)?.to_string());
                    }
                    _ => {
                        return Err(AppError::new_validation(
                            "RUNTIME_CONFIG_INVALID_VALUE",
                            format!("{key} is not a supported Apache runtime config field."),
                        ));
                    }
                }
            }

            let mut values = default_apache_values();
            for (key, value) in &normalized {
                values.insert(key.clone(), value.clone());
            }
            load_apache_runtime_config_from_values(&values)?;
        }
        RuntimeType::Nginx => {
            for (key, value) in patch {
                match key.as_str() {
                    "keep_alive" => {
                        normalized.insert(
                            key.clone(),
                            if parse_toggle(key, value)? {
                                "on"
                            } else {
                                "off"
                            }
                            .to_string(),
                        );
                    }
                    "timeout"
                    | "keep_alive_timeout"
                    | "keep_alive_requests"
                    | "worker_connections" => {
                        normalized
                            .insert(key.clone(), parse_u32_with_min(key, value, 0)?.to_string());
                    }
                    "worker_processes" => {
                        normalized.insert(key.clone(), parse_worker_processes(value)?);
                    }
                    _ => {
                        return Err(AppError::new_validation(
                            "RUNTIME_CONFIG_INVALID_VALUE",
                            format!("{key} is not a supported Nginx runtime config field."),
                        ));
                    }
                }
            }

            let mut values = default_nginx_values();
            for (key, value) in &normalized {
                values.insert(key.clone(), value.clone());
            }
            load_nginx_runtime_config_from_values(&values)?;
        }
        RuntimeType::Mysql => unreachable!(),
    }

    Ok(normalized)
}

fn load_apache_runtime_config_from_values(
    values: &BTreeMap<String, String>,
) -> Result<ApacheRuntimeConfig, AppError> {
    let threads_per_child = parse_u32_with_min(
        "threads_per_child",
        values
            .get("threads_per_child")
            .map(String::as_str)
            .unwrap_or("150"),
        1,
    )?;

    Ok(ApacheRuntimeConfig {
        timeout: parse_u32_with_min(
            "timeout",
            values.get("timeout").map(String::as_str).unwrap_or("60"),
            1,
        )?,
        keep_alive: parse_toggle(
            "keep_alive",
            values.get("keep_alive").map(String::as_str).unwrap_or("on"),
        )?,
        keep_alive_timeout: parse_u32_with_min(
            "keep_alive_timeout",
            values
                .get("keep_alive_timeout")
                .map(String::as_str)
                .unwrap_or("5"),
            0,
        )?,
        max_keep_alive_requests: parse_u32_with_min(
            "max_keep_alive_requests",
            values
                .get("max_keep_alive_requests")
                .map(String::as_str)
                .unwrap_or("100"),
            0,
        )?,
        threads_per_child,
        max_connections_per_child: parse_u32_with_min(
            "max_connections_per_child",
            values
                .get("max_connections_per_child")
                .map(String::as_str)
                .unwrap_or("0"),
            0,
        )?,
        max_mem_free: parse_u32_with_min(
            "max_mem_free",
            values
                .get("max_mem_free")
                .map(String::as_str)
                .unwrap_or("2048"),
            0,
        )?,
    })
}

fn load_nginx_runtime_config_from_values(
    values: &BTreeMap<String, String>,
) -> Result<NginxRuntimeConfig, AppError> {
    Ok(NginxRuntimeConfig {
        timeout: parse_u32_with_min(
            "timeout",
            values.get("timeout").map(String::as_str).unwrap_or("60"),
            1,
        )?,
        keep_alive: parse_toggle(
            "keep_alive",
            values.get("keep_alive").map(String::as_str).unwrap_or("on"),
        )?,
        keep_alive_timeout: parse_u32_with_min(
            "keep_alive_timeout",
            values
                .get("keep_alive_timeout")
                .map(String::as_str)
                .unwrap_or("65"),
            0,
        )?,
        keep_alive_requests: parse_u32_with_min(
            "keep_alive_requests",
            values
                .get("keep_alive_requests")
                .map(String::as_str)
                .unwrap_or("100"),
            0,
        )?,
        worker_processes: parse_worker_processes(
            values
                .get("worker_processes")
                .map(String::as_str)
                .unwrap_or("1"),
        )?,
        worker_connections: parse_u32_with_min(
            "worker_connections",
            values
                .get("worker_connections")
                .map(String::as_str)
                .unwrap_or("1024"),
            1,
        )?,
    })
}
