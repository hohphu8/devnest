use crate::core::{log_reader, ports, runtime_registry, service_manager};
use crate::error::AppError;
use crate::models::frankenphp_octane::{
    FrankenphpOctanePreflight, FrankenphpOctanePreflightCheck, FrankenphpOctanePreflightLevel,
    FrankenphpOctaneWorkerHealth, FrankenphpOctaneWorkerSettings, FrankenphpOctaneWorkerStatus,
    FrankenphpRuntimeExtensionHealth, FrankenphpRuntimeHealth,
};
use crate::models::project::{FrameworkType, FrankenphpMode, Project, ServerType};
use crate::models::runtime::{RuntimeType, RuntimeVersion};
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::{AppState, ManagedWorkerProcess};
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::{
    PhpExtensionOverrideRepository, ProjectRepository, RuntimeVersionRepository, now_iso,
};
use crate::utils::process::{configure_background_command, kill_process_tree};
use rusqlite::Connection;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

const OCTANE_CADDYFILE_PARTS: [&str; 6] =
    ["vendor", "laravel", "octane", "src", "Commands", "stubs"];
const OCTANE_CADDYFILE_NAME: &str = "Caddyfile";
const OCTANE_WORKER_STUB_NAME: &str = "frankenphp-worker.php";

struct SyncResult {
    running_pid: Option<u32>,
    exited: bool,
    exit_success: bool,
    exit_message: Option<String>,
}

fn process_key(project_id: &str) -> String {
    format!("frankenphp-octane:{project_id}")
}

fn mutex_error() -> AppError {
    AppError::new_validation(
        "FRANKENPHP_WORKER_STATE_LOCK_FAILED",
        "Could not access the in-memory FrankenPHP worker state cache.",
    )
}

fn status_message(project_name: &str, exit_status: &ExitStatus) -> Option<String> {
    if exit_status.success() {
        return None;
    }

    let detail = exit_status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "an unknown exit status".to_string());

    Some(format!(
        "{project_name} Octane worker stopped unexpectedly with {detail}."
    ))
}

fn check(
    code: &str,
    level: FrankenphpOctanePreflightLevel,
    title: &str,
    message: impl Into<String>,
    suggestion: Option<String>,
    blocking: bool,
) -> FrankenphpOctanePreflightCheck {
    FrankenphpOctanePreflightCheck {
        code: code.to_string(),
        level,
        title: title.to_string(),
        message: message.into(),
        suggestion,
        blocking,
    }
}

fn composer_has_octane(project_path: &Path) -> bool {
    for file_name in ["composer.lock", "composer.json"] {
        let path = project_path.join(file_name);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };

        if content.contains("\"laravel/octane\"") || content.contains("laravel/octane") {
            return true;
        }

        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if value
                .pointer("/require/laravel~1octane")
                .or_else(|| value.pointer("/require-dev/laravel~1octane"))
                .is_some()
            {
                return true;
            }
        }
    }

    project_path
        .join("vendor")
        .join("laravel")
        .join("octane")
        .exists()
}

fn octane_stub_dir(project_path: &Path) -> PathBuf {
    OCTANE_CADDYFILE_PARTS
        .iter()
        .fold(project_path.to_path_buf(), |path, part| path.join(part))
}

fn octane_caddyfile_path(project_path: &Path) -> PathBuf {
    octane_stub_dir(project_path).join(OCTANE_CADDYFILE_NAME)
}

fn octane_worker_stub_path(project_path: &Path) -> PathBuf {
    octane_stub_dir(project_path).join(OCTANE_WORKER_STUB_NAME)
}

fn resolve_project_public_path(project: &Project) -> PathBuf {
    let document_root = PathBuf::from(&project.document_root);
    if document_root.is_absolute() {
        document_root
    } else {
        Path::new(&project.path).join(document_root)
    }
}

fn read_dotenv_value(project_path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(project_path.join(".env")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }

        return Some(
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        );
    }

    None
}

fn parse_timestamp_seconds(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}

fn uptime_seconds(started_at: Option<&str>) -> Option<i64> {
    let started_at = parse_timestamp_seconds(started_at?)?;
    let now = chrono::Utc::now().timestamp();
    Some(now.saturating_sub(started_at))
}

fn path_modified_after(path: &Path, started_at: i64) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    if metadata.is_file() {
        return metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|modified| modified.as_secs() as i64 > started_at)
            .unwrap_or(false);
    }

    if !metadata.is_dir() {
        return false;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };

    for entry in entries.flatten() {
        if path_modified_after(&entry.path(), started_at) {
            return true;
        }
    }

    false
}

fn restart_recommendation(
    project: &Project,
    settings: &FrankenphpOctaneWorkerSettings,
) -> (bool, Option<String>) {
    if !matches!(settings.status, FrankenphpOctaneWorkerStatus::Running) {
        return (false, None);
    }

    let Some(started_at) = settings
        .last_started_at
        .as_deref()
        .and_then(parse_timestamp_seconds)
    else {
        return (false, None);
    };

    let project_path = Path::new(&project.path);
    for (label, path) in [
        (".env", project_path.join(".env")),
        ("Composer metadata", project_path.join("composer.json")),
        ("Composer lock", project_path.join("composer.lock")),
        (
            "Laravel bootstrap",
            project_path.join("bootstrap").join("app.php"),
        ),
        ("Laravel application code", project_path.join("app")),
        ("Laravel routes", project_path.join("routes")),
        ("Laravel config", project_path.join("config")),
        ("Laravel database code", project_path.join("database")),
        (
            "Laravel Blade views",
            project_path.join("resources").join("views"),
        ),
    ] {
        if path_modified_after(&path, started_at) {
            return (
                true,
                Some(format!("{label} changed after this Octane worker started.")),
            );
        }
    }

    (false, None)
}

fn parse_request_count_from_metrics(body: &str) -> Option<i64> {
    let mut total = 0f64;
    let mut found = false;

    for line in body.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let lower = line.to_ascii_lowercase();
        let metric_name = lower.split('{').next().unwrap_or(lower.as_str());
        let request_counter = metric_name.contains("request")
            && (metric_name.ends_with("_total") || metric_name.ends_with("_count"));
        if !request_counter {
            continue;
        }

        let Some(value) = line
            .split_whitespace()
            .last()
            .and_then(|value| value.parse::<f64>().ok())
        else {
            continue;
        };
        if value.is_finite() {
            total += value;
            found = true;
        }
    }

    found.then_some(total.round() as i64)
}

fn worker_metrics(admin_port: i64) -> (bool, Option<i64>) {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
    else {
        return (false, None);
    };

    let url = format!("http://localhost:{admin_port}/metrics");
    let Ok(response) = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
    else {
        return (false, None);
    };

    let Ok(body) = response.text() else {
        return (true, None);
    };

    (true, parse_request_count_from_metrics(&body))
}

fn frankenphp_runtime_health(
    connection: &Connection,
    state: &AppState,
) -> Result<Option<FrankenphpRuntimeHealth>, AppError> {
    let Some(runtime) =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?
    else {
        return Ok(None);
    };

    let runtime_path = PathBuf::from(&runtime.path);
    if !runtime_path.exists() {
        return Ok(Some(FrankenphpRuntimeHealth {
            runtime_id: runtime.id,
            version: runtime.version,
            php_family: None,
            path: runtime.path,
            managed_php_config_path: None,
            extensions: vec![
                FrankenphpRuntimeExtensionHealth {
                    extension_name: "redis".to_string(),
                    available: false,
                    enabled: false,
                },
                FrankenphpRuntimeExtensionHealth {
                    extension_name: "mbstring".to_string(),
                    available: false,
                    enabled: false,
                },
                FrankenphpRuntimeExtensionHealth {
                    extension_name: "pdo_mysql".to_string(),
                    available: false,
                    enabled: false,
                },
            ],
        }));
    }

    let runtime_home = runtime_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let php_family = runtime_registry::frankenphp_embedded_php_family(&runtime_path).ok();
    let available_extensions = php_family
        .as_deref()
        .map(|family| {
            runtime_registry::frankenphp_available_php_extensions(
                &runtime_home,
                &state.workspace_dir,
                family,
            )
        })
        .unwrap_or_default();
    let label = php_family
        .as_deref()
        .map(|family| format!("FrankenPHP {} (PHP {family})", runtime.version))
        .unwrap_or_else(|| format!("FrankenPHP {}", runtime.version));
    let extension_states = PhpExtensionOverrideRepository::list_for_runtime(
        connection,
        &runtime.id,
        &label,
        &available_extensions,
    )?;
    let managed_php_config_path = runtime_registry::frankenphp_managed_php_environment(
        connection,
        &state.workspace_dir,
        &runtime_path,
        Some(&runtime.id),
    )
    .ok()
    .and_then(|env| {
        env.into_iter()
            .find(|(key, _)| key == "PHPRC")
            .map(|(_, value)| value)
    });

    let extensions = ["redis", "mbstring", "pdo_mysql"]
        .into_iter()
        .map(|extension_name| {
            let enabled = extension_states
                .iter()
                .find(|item| item.extension_name == extension_name)
                .map(|item| item.enabled)
                .unwrap_or(false);
            FrankenphpRuntimeExtensionHealth {
                extension_name: extension_name.to_string(),
                available: available_extensions
                    .iter()
                    .any(|item| item == extension_name),
                enabled,
            }
        })
        .collect();

    Ok(Some(FrankenphpRuntimeHealth {
        runtime_id: runtime.id,
        version: runtime.version,
        php_family,
        path: runtime.path,
        managed_php_config_path,
        extensions,
    }))
}

fn ensure_frankenphp_worker_file(project: &Project) -> Result<PathBuf, AppError> {
    let project_path = Path::new(&project.path);
    let worker_stub = octane_worker_stub_path(project_path);
    if !worker_stub.exists() {
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_STUB_MISSING",
            "Laravel Octane's FrankenPHP worker stub was not found. Run `composer require laravel/octane` or update the package in this project.",
        ));
    }

    let public_path = resolve_project_public_path(project);
    fs::create_dir_all(&public_path).map_err(|error| {
        AppError::with_details(
            "FRANKENPHP_WORKER_START_FAILED",
            "Could not prepare the Laravel public directory for the FrankenPHP worker.",
            error.to_string(),
        )
    })?;

    let worker_target = public_path.join(OCTANE_WORKER_STUB_NAME);
    if !worker_target.exists() {
        fs::copy(&worker_stub, &worker_target).map_err(|error| {
            AppError::with_details(
                "FRANKENPHP_WORKER_START_FAILED",
                "Could not install Laravel Octane's FrankenPHP worker file into the public directory.",
                error.to_string(),
            )
        })?;
    }

    Ok(public_path)
}

fn validate_project_is_octane_target(
    connection: &Connection,
    project_id: &str,
) -> Result<crate::models::project::Project, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    if !matches!(project.server_type, ServerType::Frankenphp) {
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_UNAVAILABLE",
            "Laravel Octane Worker mode is only available for FrankenPHP projects.",
        ));
    }
    if !matches!(project.framework, FrameworkType::Laravel) {
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_UNAVAILABLE",
            "Laravel Octane Worker mode is only available for Laravel projects.",
        ));
    }

    Ok(project)
}

fn companion_php_binary(frankenphp_binary: &Path) -> Option<PathBuf> {
    let runtime_home = frankenphp_binary.parent()?;
    for file_name in if cfg!(windows) {
        ["php.exe", "php"]
    } else {
        ["php", "php.exe"]
    } {
        let candidate = runtime_home.join(file_name);
        if candidate.exists() && candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn frankenphp_runtime_has_php_module(
    connection: &Connection,
    state: &AppState,
    runtime: &RuntimeVersion,
    module_name: &str,
) -> Result<Option<bool>, AppError> {
    let frankenphp_binary = PathBuf::from(&runtime.path);
    let Some(php_binary) = companion_php_binary(&frankenphp_binary) else {
        return Ok(None);
    };

    let mut command = Command::new(php_binary);
    command.arg("-m");
    for (key, value) in runtime_registry::frankenphp_managed_php_environment(
        connection,
        &state.workspace_dir,
        &frankenphp_binary,
        Some(&runtime.id),
    )? {
        command.env(key, value);
    }
    configure_background_command(&mut command);

    let output = command.output().map_err(|error| {
        AppError::with_details(
            "FRANKENPHP_WORKER_PREFLIGHT_FAILED",
            "Could not inspect the active FrankenPHP PHP modules.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Ok(None);
    }

    let modules = String::from_utf8_lossy(&output.stdout);
    Ok(Some(
        modules
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case(module_name)),
    ))
}

fn sync_tracked_process(
    state: &AppState,
    project_id: &str,
    project_name: &str,
) -> Result<SyncResult, AppError> {
    let mut processes = state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?;
    let key = process_key(project_id);
    let mut running_pid = None;
    let mut exited = false;
    let mut exit_success = false;
    let mut exit_message = None;

    if let Some(process) = processes.get_mut(&key) {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                exited = true;
                exit_success = status.success();
                exit_message = status_message(project_name, &status);
            }
            Ok(None) => running_pid = Some(process.pid),
            Err(error) => {
                return Err(AppError::with_details(
                    "FRANKENPHP_WORKER_STATUS_FAILED",
                    "Could not inspect the Octane worker process state.",
                    error.to_string(),
                ));
            }
        }
    }

    if exited {
        processes.remove(&key);
    }

    Ok(SyncResult {
        running_pid,
        exited,
        exit_success,
        exit_message,
    })
}

fn save_running(
    connection: &Connection,
    project_id: &str,
    pid: u32,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let timestamp = now_iso()?;
    let current = FrankenphpOctaneWorkerRepository::get(connection, project_id)?;
    let keep_existing_started_at = current
        .as_ref()
        .map(|settings| {
            matches!(settings.status, FrankenphpOctaneWorkerStatus::Running)
                && settings.pid == Some(i64::from(pid))
                && settings.last_started_at.is_some()
        })
        .unwrap_or(false);

    FrankenphpOctaneWorkerRepository::set_status(
        connection,
        project_id,
        &FrankenphpOctaneWorkerStatus::Running,
        Some(i64::from(pid)),
        if keep_existing_started_at {
            None
        } else {
            Some(timestamp.as_str())
        },
        None,
        None,
    )
}

fn save_stopped(
    connection: &Connection,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let timestamp = now_iso()?;
    FrankenphpOctaneWorkerRepository::set_status(
        connection,
        project_id,
        &FrankenphpOctaneWorkerStatus::Stopped,
        None,
        None,
        Some(timestamp.as_str()),
        None,
    )
}

fn save_error(
    connection: &Connection,
    project_id: &str,
    message: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let timestamp = now_iso()?;
    FrankenphpOctaneWorkerRepository::set_status(
        connection,
        project_id,
        &FrankenphpOctaneWorkerStatus::Error,
        None,
        None,
        Some(timestamp.as_str()),
        Some(message),
    )
}

fn is_frankenphp_process(process_name: Option<&str>) -> bool {
    process_name
        .map(|name| name.to_ascii_lowercase().contains("frankenphp"))
        .unwrap_or(false)
}

fn detect_running_worker_on_ports(
    settings: &FrankenphpOctaneWorkerSettings,
) -> Result<Option<u32>, AppError> {
    let ports = ports::check_ports(&[settings.worker_port as u16, settings.admin_port as u16])?;
    let worker = ports
        .iter()
        .find(|item| item.port == settings.worker_port as u16);
    let admin = ports
        .iter()
        .find(|item| item.port == settings.admin_port as u16);

    let Some(worker) = worker else {
        return Ok(None);
    };
    let Some(admin) = admin else {
        return Ok(None);
    };

    let Some(worker_pid) = worker.pid else {
        return Ok(None);
    };
    let Some(admin_pid) = admin.pid else {
        return Ok(None);
    };

    if worker_pid == admin_pid
        && is_frankenphp_process(worker.process_name.as_deref())
        && is_frankenphp_process(admin.process_name.as_deref())
    {
        return Ok(Some(worker_pid));
    }

    Ok(None)
}

pub fn get_settings(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    validate_project_is_octane_target(connection, project_id)?;
    FrankenphpOctaneWorkerRepository::get_or_create(connection, &state.workspace_dir, project_id)
}

pub fn preflight(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctanePreflight, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let mut settings = FrankenphpOctaneWorkerRepository::get_or_create(
        connection,
        &state.workspace_dir,
        project_id,
    )?;
    let project_path = Path::new(&project.path);
    let mut checks = Vec::new();
    let mut install_commands = vec!["composer require laravel/octane".to_string()];

    let laravel_ready = matches!(project.framework, FrameworkType::Laravel);
    checks.push(check(
        "PROJECT_FRAMEWORK",
        if laravel_ready {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Laravel project",
        if laravel_ready {
            "Project framework is Laravel."
        } else {
            "Worker mode is Laravel-only in this phase."
        },
        if laravel_ready {
            None
        } else {
            Some("Switch this project back to Classic or import a Laravel project.".to_string())
        },
        !laravel_ready,
    ));

    let server_ready = matches!(project.server_type, ServerType::Frankenphp);
    checks.push(check(
        "SERVER_TYPE",
        if server_ready {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "FrankenPHP runtime lane",
        if server_ready {
            "Project is configured for FrankenPHP."
        } else {
            "Octane worker mode must stay behind the FrankenPHP ingress."
        },
        if server_ready {
            None
        } else {
            Some("Change the project server to FrankenPHP before enabling worker mode.".to_string())
        },
        !server_ready,
    ));

    if laravel_ready && server_ready {
        if let Some(pid) = detect_running_worker_on_ports(&settings)? {
            settings = save_running(connection, project_id, pid)?;
        }
    }

    let artisan_exists = project_path.join("artisan").exists();
    checks.push(check(
        "ARTISAN_FILE",
        if artisan_exists {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Artisan entrypoint",
        if artisan_exists {
            "`artisan` exists in the project root."
        } else {
            "`artisan` was not found in the project root."
        },
        if artisan_exists {
            None
        } else {
            Some("Confirm the project path points at the Laravel application root.".to_string())
        },
        !artisan_exists,
    ));

    let composer_json_exists = project_path.join("composer.json").exists();
    checks.push(check(
        "COMPOSER_JSON",
        if composer_json_exists {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Composer metadata",
        if composer_json_exists {
            "`composer.json` exists."
        } else {
            "`composer.json` was not found."
        },
        if composer_json_exists {
            None
        } else {
            Some("Run this from a Laravel project with Composer metadata.".to_string())
        },
        !composer_json_exists,
    ));

    let octane_installed = composer_has_octane(project_path);
    checks.push(check(
        "OCTANE_PACKAGE",
        if octane_installed {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Laravel Octane package",
        if octane_installed {
            "`laravel/octane` is present in Composer metadata or vendor."
        } else {
            "`laravel/octane` is not installed yet."
        },
        if octane_installed {
            None
        } else {
            Some("Run the shown Composer command in the project terminal.".to_string())
        },
        !octane_installed,
    ));

    let caddyfile_exists = octane_caddyfile_path(project_path).exists();
    checks.push(check(
        "OCTANE_CADDYFILE",
        if caddyfile_exists {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Octane FrankenPHP Caddyfile",
        if caddyfile_exists {
            "Laravel Octane's FrankenPHP Caddyfile stub exists."
        } else {
            "Laravel Octane's FrankenPHP Caddyfile stub was not found."
        },
        if caddyfile_exists {
            None
        } else {
            Some(
                "Run `composer require laravel/octane` or `composer update laravel/octane` so DevNest can use Octane's bundled FrankenPHP stubs."
                    .to_string(),
            )
        },
        !caddyfile_exists,
    ));

    let worker_stub_exists = octane_worker_stub_path(project_path).exists();
    checks.push(check(
        "OCTANE_WORKER_STUB",
        if worker_stub_exists {
            FrankenphpOctanePreflightLevel::Ok
        } else {
            FrankenphpOctanePreflightLevel::Error
        },
        "Octane worker stub",
        if worker_stub_exists {
            "Laravel Octane's FrankenPHP worker stub exists."
        } else {
            "Laravel Octane's FrankenPHP worker stub was not found."
        },
        if worker_stub_exists {
            None
        } else {
            Some(
                "Run `composer require laravel/octane` or `composer update laravel/octane` so DevNest can use Octane's bundled FrankenPHP worker stub."
                    .to_string(),
            )
        },
        !worker_stub_exists,
    ));

    let runtime =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?;
    if let Some(runtime) = runtime.as_ref() {
        let family_result =
            runtime_registry::frankenphp_embedded_php_family(Path::new(&runtime.path));
        match family_result {
            Ok(family) => {
                let expected = runtime_registry::runtime_version_family(&project.php_version);
                let matches_family = family == expected;
                checks.push(check(
                    "FRANKENPHP_RUNTIME",
                    if matches_family {
                        FrankenphpOctanePreflightLevel::Ok
                    } else {
                        FrankenphpOctanePreflightLevel::Error
                    },
                    "Active FrankenPHP runtime",
                    format!(
                        "Active FrankenPHP and embeds PHP {}.", family
                    ),
                    if matches_family {
                        None
                    } else {
                        Some(format!(
                            "Use a FrankenPHP runtime embedding PHP {expected}, or change this project's selected PHP family."
                        ))
                    },
                    !matches_family,
                ));
            }
            Err(error) => checks.push(check(
                "FRANKENPHP_RUNTIME",
                FrankenphpOctanePreflightLevel::Error,
                "Active FrankenPHP runtime",
                error.message,
                Some("Link or activate a working FrankenPHP binary in Settings.".to_string()),
                true,
            )),
        }
    } else {
        checks.push(check(
            "FRANKENPHP_RUNTIME",
            FrankenphpOctanePreflightLevel::Error,
            "Active FrankenPHP runtime",
            "No active FrankenPHP runtime is linked.",
            Some("Link or activate FrankenPHP in Settings before starting Octane.".to_string()),
            true,
        ));
    }

    if let Some(runtime) = runtime.as_ref() {
        match frankenphp_runtime_has_php_module(connection, state, runtime, "mbstring")? {
            Some(true) => checks.push(check(
                "FRANKENPHP_PHP_MBSTRING",
                FrankenphpOctanePreflightLevel::Ok,
                "Managed PHP mbstring",
                "Through DevNest's managed PHP configuration.",
                None,
                false,
            )),
            Some(false) => checks.push(check(
                "FRANKENPHP_PHP_MBSTRING",
                FrankenphpOctanePreflightLevel::Error,
                "Managed PHP mbstring",
                "Laravel requires mbstring, but the active FrankenPHP PHP runtime is not loading it.",
                Some("Enable mbstring for the active FrankenPHP runtime in DevNest's managed PHP configuration.".to_string()),
                true,
            )),
            None => checks.push(check(
                "FRANKENPHP_PHP_MBSTRING",
                FrankenphpOctanePreflightLevel::Warning,
                "Managed PHP mbstring",
                "DevNest could not inspect whether the active FrankenPHP PHP runtime loads mbstring.",
                Some("If Start fails during Laravel boot, check the worker log for missing PHP extensions.".to_string()),
                false,
            )),
        }

        checks.push(check(
            "OCTANE_START_METHOD",
            FrankenphpOctanePreflightLevel::Ok,
            "Windows start method",
            "DevNest starts FrankenPHP directly with Laravel Octane's Caddyfile.",
            None,
            false,
        ));
    }

    let current_worker_pid = settings.pid.map(|pid| pid as u32);

    for (code, title, port) in [
        ("WORKER_PORT", "Worker port", settings.worker_port),
        ("ADMIN_PORT", "Admin port", settings.admin_port),
    ] {
        match ports::check_port(port as u16) {
            Ok(result) if !result.available && result.pid == current_worker_pid => {
                checks.push(check(
                    code,
                    FrankenphpOctanePreflightLevel::Ok,
                    title,
                    format!("Port {port} is already held by this project's running Octane worker."),
                    None,
                    false,
                ))
            }
            Ok(result) => checks.push(check(
                code,
                if result.available {
                    FrankenphpOctanePreflightLevel::Ok
                } else {
                    FrankenphpOctanePreflightLevel::Error
                },
                title,
                if result.available {
                    format!("Port {port} is available.")
                } else {
                    format!(
                        "Port {port} is already used by {}.",
                        result
                            .process_name
                            .unwrap_or_else(|| "another process".to_string())
                    )
                },
                if result.available {
                    None
                } else {
                    Some("Choose another managed port in the Octane settings.".to_string())
                },
                !result.available,
            )),
            Err(error) => checks.push(check(
                code,
                FrankenphpOctanePreflightLevel::Warning,
                title,
                error.message,
                Some("DevNest could not verify this port before start.".to_string()),
                false,
            )),
        }
    }

    if octane_installed {
        install_commands.clear();
    }

    let ready = checks.iter().all(|item| !item.blocking);
    Ok(FrankenphpOctanePreflight {
        project_id: project_id.to_string(),
        ready,
        summary: if ready {
            "Laravel Octane is ready to start behind FrankenPHP.".to_string()
        } else {
            "Fix the blocking Octane checks before starting the worker.".to_string()
        },
        install_commands,
        checks,
        generated_at: now_iso()?,
    })
}

pub fn get_status(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let project = validate_project_is_octane_target(connection, project_id)?;
    let current = FrankenphpOctaneWorkerRepository::get_or_create(
        connection,
        &state.workspace_dir,
        project_id,
    )?;
    let sync = sync_tracked_process(state, project_id, &project.name)?;

    if let Some(pid) = sync.running_pid {
        return save_running(connection, project_id, pid);
    }

    if sync.exited {
        if sync.exit_success {
            return save_stopped(connection, project_id);
        }

        return save_error(
            connection,
            project_id,
            sync.exit_message
                .as_deref()
                .unwrap_or("The Octane worker stopped unexpectedly."),
        );
    }

    if let Some(pid) = current.pid {
        if detect_running_worker_on_ports(&current)? == Some(pid as u32) {
            return save_running(connection, project_id, pid as u32);
        }
        return save_stopped(connection, project_id);
    }

    if let Some(pid) = detect_running_worker_on_ports(&current)? {
        return save_running(connection, project_id, pid);
    }

    Ok(current)
}

pub fn health(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerHealth, AppError> {
    let project = validate_project_is_octane_target(connection, project_id)?;
    let settings = get_status(connection, state, project_id)?;
    let log_tail = log_reader::read_tail(Path::new(&settings.log_path), 80).unwrap_or_default();
    let (metrics_available, request_count) =
        if matches!(settings.status, FrankenphpOctaneWorkerStatus::Running) {
            worker_metrics(settings.admin_port)
        } else {
            (false, None)
        };
    let (restart_recommended, restart_reason) = restart_recommendation(&project, &settings);

    Ok(FrankenphpOctaneWorkerHealth {
        project_id: project_id.to_string(),
        status: settings.status.clone(),
        pid: settings.pid,
        uptime_seconds: uptime_seconds(settings.last_started_at.as_deref()),
        worker_port: settings.worker_port,
        admin_port: settings.admin_port,
        last_started_at: settings.last_started_at,
        last_restarted_at: None,
        last_error: settings.last_error,
        request_count,
        metrics_available,
        log_tail,
        restart_recommended,
        restart_reason,
        runtime: frankenphp_runtime_health(connection, state)?,
        generated_at: now_iso()?,
    })
}

fn runtime_command_env(
    connection: &Connection,
    state: &AppState,
) -> Result<(PathBuf, Vec<(String, String)>), AppError> {
    let runtime =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?
            .ok_or_else(|| {
                AppError::new_validation(
                    "RUNTIME_NOT_AVAILABLE",
                    "Select an active FrankenPHP runtime before starting Laravel Octane.",
                )
            })?;
    let binary_path = PathBuf::from(&runtime.path);
    let env = runtime_registry::frankenphp_managed_php_environment(
        connection,
        &state.workspace_dir,
        &binary_path,
        Some(&runtime.id),
    )?
    .into_iter()
    .collect::<Vec<_>>();

    Ok((binary_path, env))
}

fn wait_for_worker_port(
    child: &mut std::process::Child,
    project_name: &str,
    worker_port: i64,
    log_path: &Path,
) -> Result<(), AppError> {
    for _ in 0..120 {
        match child.try_wait() {
            Ok(Some(status)) => {
                let error_message = status_message(project_name, &status).unwrap_or_else(|| {
                    "Laravel Octane exited immediately after launch.".to_string()
                });
                return Err(AppError::with_details(
                    "FRANKENPHP_WORKER_START_FAILED",
                    error_message,
                    log_reader::read_tail(log_path, 60)?,
                ));
            }
            Ok(None) => {}
            Err(error) => {
                return Err(AppError::with_details(
                    "FRANKENPHP_WORKER_START_FAILED",
                    "Could not confirm the Laravel Octane worker process state.",
                    error.to_string(),
                ));
            }
        }

        match ports::check_port(worker_port as u16) {
            Ok(result) if !result.available => return Ok(()),
            Ok(_) | Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    }

    let _ = kill_process_tree(child.id());
    Err(AppError::with_details(
        "FRANKENPHP_WORKER_START_FAILED",
        format!("Laravel Octane started but did not listen on port {worker_port}."),
        log_reader::read_tail(log_path, 60)?,
    ))
}

pub fn start(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let project = validate_project_is_octane_target(connection, project_id)?;
    if !matches!(project.frankenphp_mode, FrankenphpMode::Octane) {
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_MODE_DISABLED",
            "Switch this FrankenPHP project to Laravel Octane Worker mode before starting the worker.",
        ));
    }

    let current = get_status(connection, state, project_id)?;
    if matches!(current.status, FrankenphpOctaneWorkerStatus::Running) && current.pid.is_some() {
        return Ok(current);
    }

    let preflight = preflight(connection, state, project_id)?;
    if !preflight.ready {
        return Err(AppError::with_details(
            "FRANKENPHP_WORKER_PREFLIGHT_FAILED",
            preflight.summary,
            preflight
                .checks
                .iter()
                .filter(|check| check.blocking)
                .map(|check| format!("{}: {}", check.title, check.message))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }

    FrankenphpOctaneWorkerRepository::set_status(
        connection,
        project_id,
        &FrankenphpOctaneWorkerStatus::Starting,
        None,
        None,
        None,
        None,
    )?;
    let settings = FrankenphpOctaneWorkerRepository::get_or_create(
        connection,
        &state.workspace_dir,
        project_id,
    )?;
    let service = service_manager::get_service_status(connection, state, ServiceName::Frankenphp)?;
    if !matches!(service.status, ServiceStatus::Running) {
        service_manager::start_service(connection, state, ServiceName::Frankenphp)?;
    }

    let (binary_path, env_vars) = runtime_command_env(connection, state)?;
    let log_path = PathBuf::from(&settings.log_path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = stdout.try_clone()?;
    let binary_dir = binary_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let path_separator = if cfg!(windows) { ";" } else { ":" };
    let existing_path = std::env::var("PATH").unwrap_or_default();
    let managed_path = format!(
        "{}{}{}",
        binary_dir.to_string_lossy(),
        path_separator,
        existing_path
    );
    let project_path = Path::new(&project.path);
    let caddyfile_path = octane_caddyfile_path(project_path);
    if !caddyfile_path.exists() {
        let message = "Laravel Octane's FrankenPHP Caddyfile stub was not found. Run `composer require laravel/octane` or update the package in this project.";
        let _ = save_error(connection, project_id, message);
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_CADDYFILE_MISSING",
            message,
        ));
    }
    let public_path = ensure_frankenphp_worker_file(&project)?;
    let app_env = read_dotenv_value(project_path, "APP_ENV").unwrap_or_else(|| "local".to_string());

    let mut command = Command::new(&binary_path);
    command.args(["run", "-c"]);
    command.arg(&caddyfile_path);
    for (key, value) in env_vars {
        command.env(key, value);
    }
    command.env("PATH", managed_path);
    command.env("APP_ENV", app_env);
    command.env("APP_BASE_PATH", &project.path);
    command.env("APP_PUBLIC_PATH", public_path);
    if project.ssl_enabled {
        let app_url = format!("https://{}", project.domain);
        command.env("APP_URL", &app_url);
        command.env("ASSET_URL", app_url);
        command.env("HTTPS", "on");
        command.env("REQUEST_SCHEME", "https");
        command.env("SERVER_PORT", "443");
    }
    command.env("LARAVEL_OCTANE", "1");
    command.env("MAX_REQUESTS", settings.max_requests.to_string());
    command.env("REQUEST_MAX_EXECUTION_TIME", "30");
    command.env("CADDY_GLOBAL_OPTIONS", "auto_https disable_redirects");
    command.env("CADDY_EXTRA_CONFIG", "");
    command.env("CADDY_SERVER_ADMIN_HOST", "localhost");
    command.env("CADDY_SERVER_ADMIN_PORT", settings.admin_port.to_string());
    command.env("CADDY_SERVER_LOG_LEVEL", "INFO");
    command.env("CADDY_SERVER_LOGGER", "json");
    command.env(
        "CADDY_SERVER_SERVER_NAME",
        format!("http://:{}", settings.worker_port),
    );
    command.env("CADDY_SERVER_WORKER_COUNT", settings.workers.to_string());
    command.env(
        "CADDY_SERVER_WORKER_DIRECTIVE",
        format!("num {}", settings.workers),
    );
    command.env("CADDY_SERVER_EXTRA_DIRECTIVES", "");
    command.env("CADDY_SERVER_WATCH_DIRECTIVES", "");
    command.current_dir(project_path);
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    configure_background_command(&mut command);

    let mut child = command.spawn().map_err(|error| {
        AppError::with_details(
            "FRANKENPHP_WORKER_START_FAILED",
            "Could not start the Laravel Octane worker.",
            error.to_string(),
        )
    })?;

    if let Err(error) =
        wait_for_worker_port(&mut child, &project.name, settings.worker_port, &log_path)
    {
        let _ = save_error(connection, project_id, &error.message);
        return Err(error);
    }

    let pid = child.id();
    state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?
        .insert(
            process_key(project_id),
            ManagedWorkerProcess {
                pid,
                child,
                log_path,
            },
        );

    save_running(connection, project_id, pid)
}

pub fn stop(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    let _ = FrankenphpOctaneWorkerRepository::get_or_create(
        connection,
        &state.workspace_dir,
        project_id,
    )?;
    let key = process_key(project_id);
    let tracked = state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?
        .remove(&key);

    if let Some(mut process) = tracked {
        match process.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                kill_process_tree(process.pid)?;
                let _ = process.child.wait();
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "FRANKENPHP_WORKER_STOP_FAILED",
                    "Could not inspect the Octane worker before stopping it.",
                    error.to_string(),
                ));
            }
        }
    } else if let Some(pid) =
        FrankenphpOctaneWorkerRepository::get(connection, project_id)?.and_then(|value| value.pid)
    {
        if let Some(settings) = FrankenphpOctaneWorkerRepository::get(connection, project_id)? {
            if detect_running_worker_on_ports(&settings)? == Some(pid as u32) {
                kill_process_tree(pid as u32)?;
            }
        }
    }

    save_stopped(connection, project_id)
}

pub fn restart(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    FrankenphpOctaneWorkerRepository::set_status(
        connection,
        project_id,
        &FrankenphpOctaneWorkerStatus::Restarting,
        None,
        None,
        None,
        None,
    )?;
    let _ = stop(connection, state, project_id)?;
    start(connection, state, project_id)
}

pub fn reload(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    validate_project_is_octane_target(connection, project_id)?;
    let status = get_status(connection, state, project_id)?;
    if !matches!(status.status, FrankenphpOctaneWorkerStatus::Running) {
        return Err(AppError::new_validation(
            "FRANKENPHP_WORKER_NOT_RUNNING",
            "Start the Laravel Octane worker before sending a reload signal.",
        ));
    }

    let admin_config_url = format!(
        "http://localhost:{}/config/apps/frankenphp",
        status.admin_port
    );
    let client = reqwest::blocking::Client::new();
    let config = client
        .get(&admin_config_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| {
            AppError::with_details(
                "FRANKENPHP_WORKER_RELOAD_FAILED",
                "Could not read the running FrankenPHP worker configuration from the admin endpoint.",
                error.to_string(),
            )
        })?
        .text()
        .map_err(|error| {
            AppError::with_details(
                "FRANKENPHP_WORKER_RELOAD_FAILED",
                "Could not read the running FrankenPHP worker configuration response.",
                error.to_string(),
            )
        })?;

    client
        .patch(&admin_config_url)
        .header("Cache-Control", "must-revalidate")
        .header("Content-Type", "application/json")
        .body(config)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| {
            AppError::with_details(
                "FRANKENPHP_WORKER_RELOAD_FAILED",
                "FrankenPHP did not accept the worker reload request.",
                error.to_string(),
            )
        })?;

    get_status(connection, state, project_id)
}

pub fn read_logs(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
    lines: usize,
) -> Result<log_reader::ProjectWorkerLogPayload, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let settings = FrankenphpOctaneWorkerRepository::get_or_create(
        connection,
        &state.workspace_dir,
        project_id,
    )?;
    log_reader::read_tail_payload(
        Path::new(&settings.log_path),
        &format!("{} Octane", project.name),
        lines,
    )
}

pub fn mark_stale_for_frankenphp_stop(
    connection: &Connection,
    state: &AppState,
) -> Result<(), AppError> {
    for project in ProjectRepository::list(connection)?
        .into_iter()
        .filter(|project| matches!(project.frankenphp_mode, FrankenphpMode::Octane))
    {
        let _ = stop(connection, state, &project.id);
    }

    Ok(())
}

pub fn auto_start_previous_octane_workers(connection: &Connection, state: &AppState) {
    let workers = match FrankenphpOctaneWorkerRepository::list_all(connection) {
        Ok(workers) => workers,
        Err(error) => {
            eprintln!("DevNest boot Octane worker load failed: {}", error);
            return;
        }
    };

    for worker in workers.into_iter().filter(|worker| {
        matches!(
            worker.status,
            FrankenphpOctaneWorkerStatus::Running
                | FrankenphpOctaneWorkerStatus::Starting
                | FrankenphpOctaneWorkerStatus::Restarting
        )
    }) {
        let project = match ProjectRepository::get(connection, &worker.project_id) {
            Ok(project) => project,
            Err(error) => {
                let _ = save_error(connection, &worker.project_id, &error.message);
                eprintln!(
                    "DevNest boot Octane worker restore skipped for {}: {}",
                    worker.project_id, error
                );
                continue;
            }
        };

        if !matches!(project.frankenphp_mode, FrankenphpMode::Octane) {
            let _ = save_stopped(connection, &worker.project_id);
            continue;
        }

        if let Err(error) = start(connection, state, &worker.project_id) {
            let _ = save_error(connection, &worker.project_id, &error.message);
            eprintln!(
                "DevNest boot Octane worker restore failed for {}: {}",
                project.name, error
            );
        }
    }
}
