use crate::core::{
    local_ssl, persistent_tunnels, ports, project_scanner, runtime_registry, service_manager,
};
use crate::error::AppError;
use crate::models::diagnostics::{DiagnosticItem, DiagnosticLevel};
use crate::models::project::{FrameworkType, Project, ServerType};
use crate::models::service::{ServiceName, ServiceState, ServiceStatus};
use crate::state::AppState;
use crate::storage::repositories::{
    ProjectPersistentHostnameRepository, ProjectRepository, RuntimeVersionRepository, now_iso,
};
use crate::utils::process::configure_background_command;
use crate::utils::windows::is_certificate_trusted_for_current_user;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn item(
    project: &Project,
    level: DiagnosticLevel,
    code: &str,
    title: &str,
    message: String,
    suggestion: Option<String>,
    created_at: &str,
) -> DiagnosticItem {
    DiagnosticItem::new(
        project.id.clone(),
        level,
        code.to_string(),
        title.to_string(),
        message,
        suggestion,
        created_at.to_string(),
    )
}

fn web_service_for_project(project: &Project) -> ServiceName {
    match &project.server_type {
        ServerType::Apache => ServiceName::Apache,
        ServerType::Nginx => ServiceName::Nginx,
        ServerType::Frankenphp => ServiceName::Frankenphp,
    }
}

fn project_server_label(project: &Project) -> &'static str {
    match &project.server_type {
        ServerType::Apache => "Apache",
        ServerType::Nginx => "Nginx",
        ServerType::Frankenphp => "FrankenPHP",
    }
}

fn normalize_extension_name(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn required_php_extensions(project: &Project, project_path: &Path) -> Vec<String> {
    let mut required = BTreeSet::new();

    if let Ok(scan) = project_scanner::scan_project(project_path) {
        for extension in scan.missing_php_extensions {
            required.insert(normalize_extension_name(&extension));
        }
    }

    let framework_defaults: &[&str] = match &project.framework {
        FrameworkType::Laravel => &[
            "bcmath",
            "ctype",
            "fileinfo",
            "intl",
            "mbstring",
            "openssl",
            "pdo_mysql",
            "tokenizer",
            "xml",
        ],
        FrameworkType::Wordpress => &[
            "curl", "dom", "gd", "json", "mysqli", "openssl", "xml", "zip",
        ],
        FrameworkType::Php | FrameworkType::Unknown => &[],
    };

    for extension in framework_defaults {
        required.insert(normalize_extension_name(extension));
    }

    required.into_iter().collect()
}

fn parse_php_modules(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(normalize_extension_name)
        .filter(|line| !line.is_empty() && line != "[php modules]" && line != "[zend modules]")
        .collect()
}

fn missing_php_extensions(
    connection: &Connection,
    state: &AppState,
    project: &Project,
) -> Result<Vec<String>, AppError> {
    let required = required_php_extensions(project, Path::new(&project.path));
    if required.is_empty() {
        return Ok(Vec::new());
    }

    let mut command = match project.server_type {
        ServerType::Frankenphp => {
            let runtime = RuntimeVersionRepository::find_active_by_type(
                connection,
                &crate::models::runtime::RuntimeType::Frankenphp,
            )?
            .ok_or_else(|| {
                AppError::new_validation(
                    "RUNTIME_NOT_AVAILABLE",
                    "Select an active FrankenPHP runtime before running diagnostics for FrankenPHP projects.",
                )
            })?;
            let env_vars = runtime_registry::frankenphp_managed_php_environment(
                connection,
                &state.workspace_dir,
                Path::new(&runtime.path),
                Some(runtime.id.as_str()),
            )?;
            let mut command = Command::new(&runtime.path);
            command
                .arg("php-cli")
                .arg("-r")
                .arg("echo implode(PHP_EOL, get_loaded_extensions());");
            for (key, value) in env_vars {
                command.env(key, value);
            }
            command.current_dir(Path::new(&project.path));
            command
        }
        ServerType::Apache | ServerType::Nginx => {
            let binary = runtime_registry::resolve_php_binary(connection, &project.php_version)?;
            let config_path = runtime_registry::build_managed_php_config(
                connection,
                &state.workspace_dir,
                &project.php_version,
            )?;
            let mut command = Command::new(&binary);
            command
                .arg("-c")
                .arg(&config_path)
                .arg("-m")
                .current_dir(binary.parent().unwrap_or_else(|| Path::new(&project.path)));
            command
        }
    };
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_CHECK_FAILED",
            format!(
                "Could not inspect PHP {} extensions for {}.",
                project.php_version, project.name
            ),
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PHP_EXTENSION_CHECK_FAILED",
            format!(
                "Could not inspect PHP {} extensions for {}.",
                project.php_version, project.name
            ),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let loaded = parse_php_modules(&String::from_utf8_lossy(&output.stdout));

    Ok(required
        .into_iter()
        .filter(|extension| !loaded.contains(extension))
        .collect())
}

fn apache_rewrite_enabled(connection: &Connection, state: &AppState) -> Result<bool, AppError> {
    let runtime = runtime_registry::resolve_service_runtime(
        connection,
        &state.workspace_dir,
        ServiceName::Apache,
    )?;
    let args = runtime
        .args
        .iter()
        .filter(|arg| arg.as_str() != "-X")
        .cloned()
        .collect::<Vec<_>>();
    let mut command = Command::new(&runtime.binary_path);
    command.args(&args);
    command.arg("-M");
    configure_background_command(&mut command);
    if let Some(current_dir) = &runtime.working_dir {
        command.current_dir(current_dir);
    }

    let output = command.output().map_err(|error| {
        AppError::with_details(
            "APACHE_MODULE_CHECK_FAILED",
            "Could not inspect Apache modules.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "APACHE_MODULE_CHECK_FAILED",
            "Could not inspect Apache modules.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    Ok(stdout.contains("rewrite_module"))
}

fn push_port_issue(
    project: &Project,
    service: &ServiceState,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) -> Result<(), AppError> {
    let Some(port) = service.port.and_then(|value| u16::try_from(value).ok()) else {
        return Ok(());
    };

    let port_check = ports::check_port(port)?;
    if !port_check.available && service.status != ServiceStatus::Running {
        items.push(item(
            project,
            DiagnosticLevel::Error,
            "PORT_IN_USE",
            "Web server port is already in use",
            format!(
                "{} cannot safely start because port {} is already used by {}.",
                project_server_label(project),
                port,
                port_check
                    .process_name
                    .as_deref()
                    .unwrap_or("another process")
            ),
            Some(format!(
                "Stop the conflicting process{} or switch the runtime to another port before starting {}.",
                port_check
                    .pid
                    .map(|pid| format!(" (PID {pid})"))
                    .unwrap_or_default(),
                project_server_label(project)
            )),
            created_at,
        ));
    }

    Ok(())
}

fn push_runtime_issue(
    project: &Project,
    service: &ServiceState,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) {
    if service.status == ServiceStatus::Error {
        items.push(item(
            project,
            DiagnosticLevel::Error,
            "SERVICE_RUNTIME_ERROR",
            "Web server last start failed",
            format!(
                "{} reported an error while starting or running this project.",
                project_server_label(project)
            ),
            Some(service.last_error.clone().unwrap_or_else(|| {
                "Open logs and review the last runtime error before retrying.".to_string()
            })),
            created_at,
        ));
    }
}

fn push_document_root_issue(project: &Project, items: &mut Vec<DiagnosticItem>, created_at: &str) {
    if matches!(&project.framework, FrameworkType::Laravel)
        && project.document_root.trim().replace('\\', "/") != "public"
    {
        items.push(item(
            project,
            DiagnosticLevel::Error,
            "LARAVEL_DOCUMENT_ROOT_MISMATCH",
            "Laravel document root should point to /public",
            format!(
                "{} is configured with document root `{}`. Laravel projects should serve from `public`.",
                project.name, project.document_root
            ),
            Some(
                "Update the project document root to `public`, then regenerate the managed config.".to_string(),
            ),
            created_at,
        ));
    }
}

fn push_apache_rewrite_issue(
    connection: &Connection,
    state: &AppState,
    project: &Project,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) {
    if !matches!(&project.server_type, ServerType::Apache) {
        return;
    }

    match apache_rewrite_enabled(connection, state) {
        Ok(true) => {}
        Ok(false) => {
            items.push(item(
                project,
                DiagnosticLevel::Error,
                "APACHE_REWRITE_DISABLED",
                "Apache rewrite module is not enabled",
                "Apache is selected for this project, but `mod_rewrite` is not enabled."
                    .to_string(),
                Some(
                    "Enable `mod_rewrite`, then reload Apache so pretty URLs work correctly."
                        .to_string(),
                ),
                created_at,
            ));
        }
        Err(error) => {
            items.push(item(
                project,
                DiagnosticLevel::Warning,
                "APACHE_REWRITE_UNVERIFIED",
                "Apache rewrite support could not be verified",
                "DevNest could not confirm whether `mod_rewrite` is enabled for Apache.".to_string(),
                Some(format!(
                    "Verify the Apache runtime path and module setup, then run diagnostics again. {}",
                    error.message
                )),
                created_at,
            ));
        }
    }
}

fn push_php_extension_issue(
    connection: &Connection,
    state: &AppState,
    project: &Project,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) {
    match missing_php_extensions(connection, state, project) {
        Ok(missing) if !missing.is_empty() => {
            items.push(item(
                project,
                DiagnosticLevel::Error,
                "PHP_MISSING_EXTENSIONS",
                "PHP is missing required extensions",
                format!(
                    "PHP {} is missing: {}.",
                    project.php_version,
                    missing.join(", ")
                ),
                Some(
                    "Enable the missing extensions in the selected PHP runtime, then restart the web server.".to_string(),
                ),
                created_at,
            ));
        }
        Ok(_) => {}
        Err(error) => {
            items.push(item(
                project,
                DiagnosticLevel::Warning,
                "PHP_EXTENSION_CHECK_UNAVAILABLE",
                "PHP extensions could not be verified",
                format!(
                    "DevNest could not verify PHP {} extensions for this project.",
                    project.php_version
                ),
                Some(format!(
                    "Configure the matching PHP runtime and run diagnostics again. {}",
                    error.message
                )),
                created_at,
            ));
        }
    }
}

fn push_mysql_issue(
    project: &Project,
    connection: &Connection,
    state: &AppState,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) -> Result<(), AppError> {
    if project.database_name.is_none() && project.database_port.is_none() {
        return Ok(());
    }

    let mysql = service_manager::get_service_status(connection, state, ServiceName::Mysql)?;
    if mysql.status == ServiceStatus::Error {
        items.push(item(
            project,
            DiagnosticLevel::Error,
            "MYSQL_STARTUP_FAILED",
            "MySQL reported a startup error",
            "MySQL is linked to this project, but the runtime reported a startup failure."
                .to_string(),
            Some(mysql.last_error.clone().unwrap_or_else(|| {
                "Check the MySQL log, data directory lock, and port usage before retrying."
                    .to_string()
            })),
            created_at,
        ));
    }

    Ok(())
}

fn push_ssl_issue(
    project: &Project,
    state: &AppState,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) -> Result<(), AppError> {
    if !project.ssl_enabled {
        return Ok(());
    }

    let authority = local_ssl::planned_ssl_authority(&state.workspace_dir);
    if !authority.cert_path.exists() {
        items.push(item(
            project,
            DiagnosticLevel::Warning,
            "SSL_AUTHORITY_MISSING",
            "Local SSL authority has not been provisioned yet",
            "This project expects HTTPS, but the DevNest local CA has not been generated yet.".to_string(),
            Some("Generate config or trust the DevNest CA once to provision SSL material for local HTTPS.".to_string()),
            created_at,
        ));
        return Ok(());
    }

    if !is_certificate_trusted_for_current_user(&authority.cert_path)? {
        items.push(item(
            project,
            DiagnosticLevel::Warning,
            "SSL_TRUST_MISSING",
            "Local SSL authority is not trusted",
            "HTTPS is enabled for this project, but the DevNest local CA is not trusted in the current user store.".to_string(),
            Some("Use `Trust DevNest CA` before opening the HTTPS domain to avoid browser certificate warnings.".to_string()),
            created_at,
        ));
    }

    let material = local_ssl::planned_ssl_material(&state.workspace_dir, &project.domain);
    if !material.cert_path.exists() || !material.key_path.exists() {
        items.push(item(
            project,
            DiagnosticLevel::Warning,
            "SSL_CERTIFICATE_MISSING",
            "Project SSL certificate files are missing",
            "HTTPS is enabled for this project, but the local certificate files have not been generated yet.".to_string(),
            Some("Generate the managed config or use `Regenerate Certificate` to provision fresh SSL files for this project.".to_string()),
            created_at,
        ));
    }

    Ok(())
}

fn push_persistent_tunnel_issue(
    connection: &Connection,
    project: &Project,
    items: &mut Vec<DiagnosticItem>,
    created_at: &str,
) -> Result<(), AppError> {
    let Some(hostname) =
        ProjectPersistentHostnameRepository::get_by_project(connection, &project.id)?
    else {
        return Ok(());
    };

    let setup = persistent_tunnels::persistent_tunnel_setup_status(connection)?;
    if !setup.ready {
        items.push(item(
            project,
            DiagnosticLevel::Warning,
            "PERSISTENT_TUNNEL_SETUP_MISSING",
            "Persistent domain setup is not ready yet",
            format!(
                "{} is reserved for this project, but the named tunnel prerequisites are still incomplete.",
                hostname.hostname
            ),
            Some(
                setup.guidance.unwrap_or_else(|| {
                    "Finish the named tunnel setup in Settings, then retry the persistent domain flow."
                        .to_string()
                }),
            ),
            created_at,
        ));
    }

    Ok(())
}

pub fn run_diagnostics(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Vec<DiagnosticItem>, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    let created_at = now_iso()?;
    let service =
        service_manager::get_service_status(connection, state, web_service_for_project(&project))?;
    let mut items = Vec::new();

    push_port_issue(&project, &service, &mut items, &created_at)?;
    push_runtime_issue(&project, &service, &mut items, &created_at);
    push_document_root_issue(&project, &mut items, &created_at);
    push_apache_rewrite_issue(connection, state, &project, &mut items, &created_at);
    push_php_extension_issue(connection, state, &project, &mut items, &created_at);
    push_mysql_issue(&project, connection, state, &mut items, &created_at)?;
    push_ssl_issue(&project, state, &mut items, &created_at)?;
    push_persistent_tunnel_issue(connection, &project, &mut items, &created_at)?;

    if items.is_empty() {
        items.push(item(
            &project,
            DiagnosticLevel::Info,
            "WORKSPACE_READY",
            "No blocking issues detected",
            "DevNest did not detect any blocking issues for the current project profile."
                .to_string(),
            Some("Open the local site and verify the expected response.".to_string()),
            &created_at,
        ));
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::run_diagnostics;
    use crate::models::persistent_tunnel::PersistentTunnelProvider;
    use crate::models::project::{CreateProjectInput, FrameworkType, ServerType};
    use crate::models::service::{ServiceName, ServiceStatus};
    use crate::state::AppState;
    use crate::storage::db::init_database;
    use crate::storage::repositories::{
        ProjectPersistentHostnameRepository, ProjectRepository, ServiceRepository,
    };
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::fs;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn setup_state() -> (PathBuf, PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("devnest-diagnostics-{}", Uuid::new_v4()));
        let workspace_dir = root.join("workspace");
        let db_path = workspace_dir.join("devnest.sqlite3");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        init_database(&db_path).expect("database should initialize");
        let connection = Connection::open(&db_path).expect("db should open");
        let state = AppState {
            db_path,
            workspace_dir,
            resources_dir: root.join("resources"),
            started_at: "2026-04-18T00:00:00Z".to_string(),
            allow_exit: Mutex::new(false),
            managed_processes: Mutex::new(HashMap::new()),
            managed_worker_processes: Mutex::new(HashMap::new()),
            managed_scheduled_task_runs: Arc::new(Mutex::new(HashMap::new())),
            scheduled_task_scheduler_shutdown: Arc::new(AtomicBool::new(false)),
            runtime_install_task: Mutex::new(None),
            optional_tool_install_task: Mutex::new(None),
            project_tunnels: Mutex::new(HashMap::new()),
            project_persistent_tunnels: Mutex::new(HashMap::new()),
            project_mobile_previews: Mutex::new(HashMap::new()),
        };

        (root, state.workspace_dir.clone(), state, connection)
    }

    fn make_project_root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("devnest-diagnostics-project-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("public")).expect("public root should be created");
        root
    }

    fn sample_project_input(project_path: &PathBuf) -> CreateProjectInput {
        CreateProjectInput {
            name: "Shop API".to_string(),
            path: project_path.to_string_lossy().to_string(),
            domain: "shop-api.test".to_string(),
            server_type: ServerType::Nginx,
            php_version: "8.2".to_string(),
            framework: FrameworkType::Laravel,
            document_root: "public".to_string(),
            ssl_enabled: false,
            database_name: Some("shop_api".to_string()),
            database_port: Some(3306),
            frankenphp_mode: None,
        }
    }

    #[test]
    fn reports_laravel_document_root_mismatch() {
        let (root, _workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let mut input = sample_project_input(&project_root);
        input.document_root = ".".to_string();
        let project = ProjectRepository::create(&connection, input).expect("project should create");

        let items =
            run_diagnostics(&connection, &state, &project.id).expect("diagnostics should run");

        assert!(
            items
                .iter()
                .any(|item| item.code == "LARAVEL_DOCUMENT_ROOT_MISMATCH")
        );
        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reports_port_conflict_for_stopped_runtime() {
        let (root, _workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let mut input = sample_project_input(&project_root);
        input.server_type = ServerType::Apache;
        input.framework = FrameworkType::Php;
        input.document_root = ".".to_string();
        let project = ProjectRepository::create(&connection, input).expect("project should create");
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have addr")
            .port();

        ServiceRepository::save_state(
            &connection,
            &ServiceName::Apache,
            &ServiceStatus::Stopped,
            None,
            Some(i64::from(port)),
            None,
        )
        .expect("service state should save");

        let items =
            run_diagnostics(&connection, &state, &project.id).expect("diagnostics should run");

        assert!(items.iter().any(|item| item.code == "PORT_IN_USE"));
        drop(listener);
        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reports_mysql_startup_failure_for_database_project() {
        let (root, _workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let project = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project should create");

        ServiceRepository::save_state(
            &connection,
            &ServiceName::Mysql,
            &ServiceStatus::Error,
            None,
            Some(3306),
            Some("Data directory is locked."),
        )
        .expect("service state should save");

        let items =
            run_diagnostics(&connection, &state, &project.id).expect("diagnostics should run");

        assert!(items.iter().any(|item| item.code == "MYSQL_STARTUP_FAILED"));
        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reports_missing_ssl_material_for_ssl_project() {
        let (root, _workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let mut input = sample_project_input(&project_root);
        input.ssl_enabled = true;
        let project = ProjectRepository::create(&connection, input).expect("project should create");

        let items =
            run_diagnostics(&connection, &state, &project.id).expect("diagnostics should run");

        assert!(
            items
                .iter()
                .any(|item| item.code == "SSL_AUTHORITY_MISSING")
        );
        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reports_missing_persistent_tunnel_setup_for_reserved_hostname() {
        let (root, _workspace_dir, state, connection) = setup_state();
        let project_root = make_project_root();
        let project = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project should create");

        ProjectPersistentHostnameRepository::upsert(
            &connection,
            &project.id,
            &PersistentTunnelProvider::Cloudflared,
            "preview.example.com",
        )
        .expect("persistent hostname should save");

        let items =
            run_diagnostics(&connection, &state, &project.id).expect("diagnostics should run");

        assert!(
            items
                .iter()
                .any(|item| item.code == "PERSISTENT_TUNNEL_SETUP_MISSING")
        );
        fs::remove_dir_all(project_root).ok();
        fs::remove_dir_all(root).ok();
    }
}
