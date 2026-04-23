use crate::core::database_time_machine::{
    self, DatabaseSnapshotResult, DatabaseSnapshotRollbackResult, DatabaseSnapshotSummary,
    DatabaseTimeMachineStatus, SnapshotCaptureRequest, SnapshotTriggerSource,
};
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::project::Project;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::AppState;
use crate::storage::repositories::{ProjectRepository, ServiceRepository, now_iso};
use crate::utils::process::configure_background_command;
use rfd::FileDialog;
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseActionResult {
    pub success: bool,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTransferResult {
    pub success: bool,
    pub name: String,
    pub path: String,
}

fn validate_database_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "INVALID_DATABASE_NAME",
            "Database name is required.",
        ));
    }

    if trimmed.len() > 64
        || !trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return Err(AppError::new_validation(
            "INVALID_DATABASE_NAME",
            "Database name may only contain letters, numbers, underscores, and dashes.",
        ));
    }

    Ok(trimmed.to_string())
}

fn mysql_client_binary_from_runtime(binary_path: &Path) -> Result<PathBuf, AppError> {
    let bin_dir = binary_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "DATABASE_RUNTIME_CLIENT_MISSING",
            "The active MySQL runtime path is missing its bin directory.",
        )
    })?;

    for candidate in ["mysql.exe", "mariadb.exe"] {
        let path = bin_dir.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(AppError::new_validation(
        "DATABASE_RUNTIME_CLIENT_MISSING",
        "The active MySQL runtime does not include a supported database client.",
    ))
}

fn mysql_dump_binary_from_runtime(binary_path: &Path) -> Result<PathBuf, AppError> {
    let bin_dir = binary_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "DATABASE_BACKUP_CLIENT_MISSING",
            "The active MySQL runtime path is missing its bin directory.",
        )
    })?;

    for candidate in ["mysqldump.exe", "mariadb-dump.exe"] {
        let path = bin_dir.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(AppError::new_validation(
        "DATABASE_BACKUP_CLIENT_MISSING",
        "The active MySQL runtime does not include a supported dump client.",
    ))
}

fn mysql_service_port(connection: &Connection) -> Result<u16, AppError> {
    let service = ServiceRepository::get(connection, ServiceName::Mysql.as_str())?;

    if service.status != ServiceStatus::Running {
        return Err(AppError::with_details(
            "DATABASE_SERVICE_STOPPED",
            "Start MySQL before managing databases.",
            service
                .last_error
                .unwrap_or_else(|| "The MySQL service is not currently running.".to_string()),
        ));
    }

    let port = service
        .port
        .unwrap_or(ServiceName::Mysql.default_port().unwrap_or(3306) as i64);
    if !(1..=65535).contains(&port) {
        return Err(AppError::new_validation(
            "DATABASE_PORT_INVALID",
            "The active MySQL service port is invalid.",
        ));
    }

    Ok(port as u16)
}

fn run_mysql_query(
    client_binary: &Path,
    port: u16,
    query: &str,
    error_code: &str,
    error_message: &str,
) -> Result<String, AppError> {
    let mut command = Command::new(client_binary);
    command
        .arg("--protocol=tcp")
        .arg("--host=127.0.0.1")
        .arg(format!("--port={port}"))
        .arg("--user=root")
        .arg("--batch")
        .arg("--skip-column-names")
        .arg("--raw")
        .arg("--silent")
        .arg("-e")
        .arg(query)
        .current_dir(
            client_binary
                .parent()
                .ok_or_else(|| AppError::new_validation(error_code, error_message))?,
        );
    configure_background_command(&mut command);
    let output = command
        .output()
        .map_err(|error| AppError::with_details(error_code, error_message, error.to_string()))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() { stderr } else { stdout };
    Err(AppError::with_details(error_code, error_message, details))
}

fn resolve_mysql_client_and_port(
    connection: &Connection,
    workspace_dir: &Path,
) -> Result<(PathBuf, u16), AppError> {
    let runtime =
        runtime_registry::resolve_service_runtime(connection, workspace_dir, ServiceName::Mysql)?;
    let client_binary = mysql_client_binary_from_runtime(&runtime.binary_path)?;
    let port = mysql_service_port(connection)?;
    Ok((client_binary, port))
}

fn resolve_mysql_backup_client_and_port(
    connection: &Connection,
    workspace_dir: &Path,
) -> Result<(PathBuf, u16), AppError> {
    let runtime =
        runtime_registry::resolve_service_runtime(connection, workspace_dir, ServiceName::Mysql)?;
    let client_binary = mysql_dump_binary_from_runtime(&runtime.binary_path)?;
    let port = mysql_service_port(connection)?;
    Ok((client_binary, port))
}

fn parse_database_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn is_mysql_access_denied(error: &AppError) -> bool {
    error
        .details
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("access denied")
}

fn map_auth_error(error: AppError) -> AppError {
    if is_mysql_access_denied(&error) {
        AppError::new_validation(
            "DATABASE_AUTH_UNSUPPORTED",
            "Database tools currently expect the local root account without a password.",
        )
    } else {
        error
    }
}

fn database_exists(client_binary: &Path, port: u16, database_name: &str) -> Result<bool, AppError> {
    let query = format!(
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name = '{}';",
        database_name
    );
    let output = run_mysql_query(
        client_binary,
        port,
        &query,
        "DATABASE_LOOKUP_FAILED",
        "Could not verify the selected database.",
    )?;

    Ok(output
        .lines()
        .map(str::trim)
        .any(|line| line.eq_ignore_ascii_case(database_name)))
}

fn ensure_database_exists(
    client_binary: &Path,
    port: u16,
    database_name: &str,
    missing_message: &str,
) -> Result<(), AppError> {
    match database_exists(client_binary, port, database_name) {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::new_validation(
            "DATABASE_NOT_FOUND",
            missing_message,
        )),
        Err(error) => Err(map_auth_error(error)),
    }
}

fn backup_file_name(database_name: &str) -> Result<String, AppError> {
    let timestamp = now_iso()?
        .replace(':', "-")
        .replace('.', "-")
        .replace('T', "_")
        .replace('Z', "Z");
    Ok(format!("{database_name}-{timestamp}.sql"))
}

fn file_dialog_backup_path(database_name: &str) -> Result<Option<PathBuf>, AppError> {
    let suggested_name = backup_file_name(database_name)?;
    Ok(FileDialog::new()
        .add_filter("SQL Dump", &["sql"])
        .set_file_name(&suggested_name)
        .save_file())
}

fn run_mysql_dump_to_path(
    dump_binary: &Path,
    port: u16,
    database_name: &str,
    target_path: &Path,
    error_code: &str,
    error_message: &str,
) -> Result<(), AppError> {
    let working_dir = dump_binary.parent().ok_or_else(|| {
        AppError::new_validation(
            error_code,
            "The dump client is missing its runtime directory.",
        )
    })?;

    let mut command = Command::new(dump_binary);
    command
        .arg("--protocol=tcp")
        .arg("--host=127.0.0.1")
        .arg(format!("--port={port}"))
        .arg("--user=root")
        .arg("--single-transaction")
        .arg("--skip-comments")
        .arg("--default-character-set=utf8mb4")
        .arg("--databases")
        .arg(database_name)
        .current_dir(working_dir);
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            error_code,
            "Could not start the database dump client.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() { stderr } else { stdout };
        return Err(AppError::with_details(error_code, error_message, details));
    }

    fs::write(target_path, output.stdout).map_err(|error| {
        AppError::with_details(
            error_code,
            "DevNest could not write the SQL dump file.",
            error.to_string(),
        )
    })?;

    Ok(())
}

fn run_mysql_restore_from_path(
    client_binary: &Path,
    port: u16,
    database_name: &str,
    source_path: &Path,
    error_code: &str,
    error_message: &str,
) -> Result<(), AppError> {
    let working_dir = client_binary.parent().ok_or_else(|| {
        AppError::new_validation(
            error_code,
            "The database client is missing its runtime directory.",
        )
    })?;
    let dump_bytes = fs::read(source_path).map_err(|error| {
        AppError::with_details(
            error_code,
            "DevNest could not read the selected SQL dump file.",
            error.to_string(),
        )
    })?;

    let mut command = Command::new(client_binary);
    command
        .arg("--protocol=tcp")
        .arg("--host=127.0.0.1")
        .arg(format!("--port={port}"))
        .arg("--user=root")
        .arg(format!("--database={database_name}"))
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_background_command(&mut command);
    let mut child = command.spawn().map_err(|error| {
        AppError::with_details(
            error_code,
            "Could not start the database restore client.",
            error.to_string(),
        )
    })?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(&dump_bytes).map_err(|error| {
            AppError::with_details(
                error_code,
                "DevNest could not stream the SQL dump into MySQL.",
                error.to_string(),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|error| {
        AppError::with_details(
            error_code,
            "DevNest could not finish the database restore process.",
            error.to_string(),
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() { stderr } else { stdout };
    Err(AppError::with_details(error_code, error_message, details))
}

fn recreate_database(client_binary: &Path, port: u16, database_name: &str) -> Result<(), AppError> {
    let drop_query = format!("DROP DATABASE IF EXISTS `{database_name}`;");
    run_mysql_query(
        client_binary,
        port,
        &drop_query,
        "DATABASE_ROLLBACK_FAILED",
        "DevNest could not clear the target database before restoring the snapshot.",
    )?;
    let create_query = format!(
        "CREATE DATABASE `{database_name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
    );
    run_mysql_query(
        client_binary,
        port,
        &create_query,
        "DATABASE_ROLLBACK_FAILED",
        "DevNest could not recreate the target database before restoring the snapshot.",
    )?;
    Ok(())
}

fn build_snapshot_result(
    workspace_dir: &Path,
    database_name: &str,
    snapshot: DatabaseSnapshotSummary,
) -> Result<DatabaseSnapshotResult, AppError> {
    Ok(DatabaseSnapshotResult {
        success: true,
        name: database_name.to_string(),
        snapshot,
        status: database_time_machine::inspect_status(workspace_dir, database_name)?,
    })
}

fn linked_project_names_for_database(
    connection: &Connection,
    database_name: &str,
) -> Result<Vec<String>, AppError> {
    Ok(ProjectRepository::list(connection)?
        .into_iter()
        .filter(|project| project.database_name.as_deref() == Some(database_name))
        .map(|project| project.name)
        .collect())
}

fn take_snapshot_inner(
    connection: &Connection,
    workspace_dir: &Path,
    database_name: &str,
    request: SnapshotCaptureRequest,
) -> Result<DatabaseSnapshotResult, AppError> {
    let (client_binary, port) = resolve_mysql_client_and_port(connection, workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        database_name,
        "The selected database does not exist anymore.",
    )?;
    let (dump_binary, dump_port) = resolve_mysql_backup_client_and_port(connection, workspace_dir)?;
    let pending =
        database_time_machine::begin_snapshot_capture(workspace_dir, database_name, request)?;

    let dump_result = run_mysql_dump_to_path(
        &dump_binary,
        dump_port,
        database_name,
        &pending.dump_path,
        "DATABASE_SNAPSHOT_FAILED",
        "The active MySQL runtime could not create a managed database snapshot.",
    );

    match dump_result {
        Ok(()) => {
            let snapshot = database_time_machine::finalize_snapshot_capture(
                workspace_dir,
                database_name,
                pending,
            )?;
            build_snapshot_result(workspace_dir, database_name, snapshot)
        }
        Err(error) => {
            database_time_machine::abort_snapshot_capture(&pending);
            Err(map_auth_error(error))
        }
    }
}

fn take_pre_action_snapshot_if_enabled(
    connection: &Connection,
    workspace_dir: &Path,
    database_name: &str,
    note: &str,
) -> Result<Option<DatabaseSnapshotSummary>, AppError> {
    if !database_time_machine::is_enabled(workspace_dir, database_name)? {
        return Ok(None);
    }

    let snapshot = take_snapshot_inner(
        connection,
        workspace_dir,
        database_name,
        SnapshotCaptureRequest {
            trigger_source: SnapshotTriggerSource::PreAction,
            note: Some(note.to_string()),
            linked_project_names: linked_project_names_for_database(connection, database_name)?,
            scheduled_interval_minutes: None,
        },
    )?;
    Ok(Some(snapshot.snapshot))
}

pub fn take_project_linked_database_snapshot_if_enabled(
    connection: &Connection,
    state: &AppState,
    project: &Project,
    note: &str,
) -> Result<Option<DatabaseSnapshotSummary>, AppError> {
    let Some(database_name) = project.database_name.as_deref() else {
        return Ok(None);
    };
    if !database_time_machine::linked_project_action_snapshots_enabled(
        &state.workspace_dir,
        database_name,
    )? {
        return Ok(None);
    }

    let _guard = match database_time_machine::acquire_operation_lock(
        &state.workspace_dir,
        database_name,
        "project-action",
    ) {
        Ok(guard) => guard,
        Err(error) if error.code == "DATABASE_TIME_MACHINE_BUSY" => return Ok(None),
        Err(error) => return Err(error),
    };

    let snapshot = take_snapshot_inner(
        connection,
        &state.workspace_dir,
        database_name,
        SnapshotCaptureRequest {
            trigger_source: SnapshotTriggerSource::PreAction,
            note: Some(note.to_string()),
            linked_project_names: linked_project_names_for_database(connection, database_name)?,
            scheduled_interval_minutes: None,
        },
    )?;
    Ok(Some(snapshot.snapshot))
}

pub fn run_scheduled_database_snapshot_cycle(
    db_path: &Path,
    workspace_dir: &Path,
) -> Result<(), AppError> {
    let connection = Connection::open(db_path)?;
    let service = ServiceRepository::get(&connection, ServiceName::Mysql.as_str())?;
    if service.status != ServiceStatus::Running {
        return Ok(());
    }

    for database_name in database_time_machine::list_managed_databases(workspace_dir)? {
        let Some(schedule_interval_minutes) =
            database_time_machine::scheduled_snapshot_due(workspace_dir, &database_name)?
        else {
            continue;
        };

        let _guard = match database_time_machine::acquire_operation_lock(
            workspace_dir,
            &database_name,
            "scheduled-snapshot",
        ) {
            Ok(guard) => guard,
            Err(error) if error.code == "DATABASE_TIME_MACHINE_BUSY" => continue,
            Err(error) => {
                eprintln!(
                    "DevNest scheduled Time Machine lock failed for {}: {}",
                    database_name, error
                );
                continue;
            }
        };

        let request = SnapshotCaptureRequest {
            trigger_source: SnapshotTriggerSource::Scheduled,
            note: Some("scheduled protection checkpoint".to_string()),
            linked_project_names: linked_project_names_for_database(&connection, &database_name)?,
            scheduled_interval_minutes: Some(schedule_interval_minutes),
        };

        if let Err(error) = take_snapshot_inner(&connection, workspace_dir, &database_name, request)
        {
            eprintln!(
                "DevNest scheduled Time Machine snapshot failed for {}: {}",
                database_name, error
            );
        }
    }

    Ok(())
}

#[tauri::command]
pub fn list_databases(state: tauri::State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let connection = connection_from_state(&state)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    let output = run_mysql_query(
        &client_binary,
        port,
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys') ORDER BY schema_name;",
        "DATABASE_LIST_FAILED",
        "Could not list databases from the active MySQL runtime.",
    )
    .map_err(map_auth_error)?;

    Ok(parse_database_list(&output))
}

#[tauri::command]
pub fn create_database(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseActionResult, AppError> {
    let connection = connection_from_state(&state)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    let database_name = validate_database_name(&name)?;
    let query = format!(
        "CREATE DATABASE `{database_name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
    );

    match run_mysql_query(
        &client_binary,
        port,
        &query,
        "DATABASE_CREATE_FAILED",
        "Could not create the requested database.",
    ) {
        Ok(_) => Ok(DatabaseActionResult {
            success: true,
            name: database_name,
        }),
        Err(error)
            if error
                .details
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains("exists") =>
        {
            Err(AppError::new_validation(
                "DATABASE_ALREADY_EXISTS",
                "A database with this name already exists.",
            ))
        }
        Err(error) => Err(map_auth_error(error)),
    }
}

#[tauri::command]
pub fn drop_database(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseActionResult, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let linked_projects = ProjectRepository::list(&connection)?
        .into_iter()
        .filter(|project| project.database_name.as_deref() == Some(database_name.as_str()))
        .map(|project| format!("{} ({})", project.name, project.domain))
        .collect::<Vec<_>>();

    if !linked_projects.is_empty() {
        return Err(AppError::with_details(
            "DATABASE_IN_USE",
            "Unlink this database from tracked projects before deleting it.",
            format!("Linked projects: {}", linked_projects.join(", ")),
        ));
    }

    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    let query = format!("DROP DATABASE `{database_name}`;");

    match run_mysql_query(
        &client_binary,
        port,
        &query,
        "DATABASE_DROP_FAILED",
        "Could not drop the requested database.",
    ) {
        Ok(_) => Ok(DatabaseActionResult {
            success: true,
            name: database_name,
        }),
        Err(error)
            if error
                .details
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains("unknown database") =>
        {
            Err(AppError::new_validation(
                "DATABASE_NOT_FOUND",
                "The selected database does not exist anymore.",
            ))
        }
        Err(error) => Err(map_auth_error(error)),
    }
}

#[tauri::command]
pub fn backup_database(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<DatabaseTransferResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The selected database does not exist anymore.",
    )?;

    let target_path = match file_dialog_backup_path(&database_name)? {
        Some(path) => path,
        None => return Ok(None),
    };
    let (dump_binary, dump_port) =
        resolve_mysql_backup_client_and_port(&connection, &state.workspace_dir)?;

    run_mysql_dump_to_path(
        &dump_binary,
        dump_port,
        &database_name,
        &target_path,
        "DATABASE_BACKUP_FAILED",
        "The active MySQL runtime could not export the selected database.",
    )
    .map_err(map_auth_error)?;

    Ok(Some(DatabaseTransferResult {
        success: true,
        name: database_name,
        path: target_path.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub fn restore_database(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<DatabaseTransferResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "Create the target database before restoring a SQL backup into it.",
    )?;

    let source_path = match FileDialog::new()
        .add_filter("SQL Dump", &["sql"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };

    let _guard = if database_time_machine::is_enabled(&state.workspace_dir, &database_name)? {
        Some(database_time_machine::acquire_operation_lock(
            &state.workspace_dir,
            &database_name,
            "restore",
        )?)
    } else {
        None
    };
    take_pre_action_snapshot_if_enabled(
        &connection,
        &state.workspace_dir,
        &database_name,
        "before restore",
    )?;

    run_mysql_restore_from_path(
        &client_binary,
        port,
        &database_name,
        &source_path,
        "DATABASE_RESTORE_FAILED",
        "The selected SQL backup could not be restored into MySQL.",
    )
    .map_err(map_auth_error)?;

    Ok(Some(DatabaseTransferResult {
        success: true,
        name: database_name,
        path: source_path.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub fn get_database_time_machine_status(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The selected database does not exist anymore.",
    )?;
    database_time_machine::inspect_status(&state.workspace_dir, &database_name)
}

#[tauri::command]
pub fn enable_database_time_machine(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The selected database does not exist anymore.",
    )?;
    database_time_machine::enable(&state.workspace_dir, &database_name)
}

#[tauri::command]
pub fn disable_database_time_machine(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The selected database does not exist anymore.",
    )?;
    database_time_machine::disable(&state.workspace_dir, &database_name)
}

#[tauri::command]
pub fn take_database_snapshot(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseSnapshotResult, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    database_time_machine::enable(&state.workspace_dir, &database_name)?;
    let _guard = database_time_machine::acquire_operation_lock(
        &state.workspace_dir,
        &database_name,
        "snapshot",
    )?;
    take_snapshot_inner(
        &connection,
        &state.workspace_dir,
        &database_name,
        SnapshotCaptureRequest {
            trigger_source: SnapshotTriggerSource::Manual,
            note: None,
            linked_project_names: linked_project_names_for_database(&connection, &database_name)?,
            scheduled_interval_minutes: None,
        },
    )
}

#[tauri::command]
pub fn list_database_snapshots(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DatabaseSnapshotSummary>, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The selected database does not exist anymore.",
    )?;
    database_time_machine::list_snapshots(&state.workspace_dir, &database_name)
}

#[tauri::command]
pub fn rollback_database_snapshot(
    name: String,
    snapshot_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseSnapshotRollbackResult, AppError> {
    let connection = connection_from_state(&state)?;
    let database_name = validate_database_name(&name)?;
    if !database_time_machine::is_enabled(&state.workspace_dir, &database_name)? {
        return Err(AppError::new_validation(
            "DATABASE_TIME_MACHINE_DISABLED",
            "Enable Time Machine before rolling a database back to a managed snapshot.",
        ));
    }

    let _guard = database_time_machine::acquire_operation_lock(
        &state.workspace_dir,
        &database_name,
        "rollback",
    )?;
    let restore_target = database_time_machine::resolve_snapshot_for_restore(
        &state.workspace_dir,
        &database_name,
        &snapshot_id,
    )?;
    let (client_binary, port) = resolve_mysql_client_and_port(&connection, &state.workspace_dir)?;
    ensure_database_exists(
        &client_binary,
        port,
        &database_name,
        "The target database does not exist anymore.",
    )?;

    let safety_snapshot = take_pre_action_snapshot_if_enabled(
        &connection,
        &state.workspace_dir,
        &database_name,
        "before rollback",
    )?;
    recreate_database(&client_binary, port, &database_name).map_err(map_auth_error)?;
    run_mysql_restore_from_path(
        &client_binary,
        port,
        &database_name,
        &restore_target.dump_path,
        "DATABASE_ROLLBACK_FAILED",
        "DevNest could not restore the selected managed snapshot into MySQL.",
    )
    .map_err(map_auth_error)?;

    Ok(DatabaseSnapshotRollbackResult {
        success: true,
        name: database_name,
        snapshot_id: restore_target.summary.id.clone(),
        restored_at: now_iso()?,
        restored_snapshot: restore_target.summary,
        safety_snapshot_id: safety_snapshot.map(|snapshot| snapshot.id),
    })
}

#[cfg(test)]
mod tests {
    use super::{backup_file_name, parse_database_list, validate_database_name};

    #[test]
    fn validates_database_names() {
        assert_eq!(
            validate_database_name("devnest_app").expect("valid database name should pass"),
            "devnest_app"
        );
        assert_eq!(
            validate_database_name("with-dash").expect("database names may include dashes"),
            "with-dash"
        );
        assert!(validate_database_name("with space").is_err());
        assert!(validate_database_name("").is_err());
    }

    #[test]
    fn parses_database_output_lines() {
        let parsed = parse_database_list("app_main\nshop_api\r\n\nlegacy_db\n");
        assert_eq!(parsed, vec!["app_main", "shop_api", "legacy_db"]);
    }

    #[test]
    fn builds_backup_file_name_with_sql_suffix() {
        let file_name = backup_file_name("vietruyen_app").expect("backup file name should build");
        assert!(file_name.starts_with("vietruyen_app-"));
        assert!(file_name.ends_with(".sql"));
        assert!(!file_name.contains(':'));
    }
}
