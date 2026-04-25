use crate::core::log_reader;
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::worker::{ProjectWorker, ProjectWorkerPresetType, ProjectWorkerStatus};
use crate::state::{AppState, ManagedWorkerProcess};
use crate::storage::project_workers::ProjectWorkerRepository;
use crate::storage::repositories::{ProjectRepository, now_iso};
use crate::utils::process::{configure_background_command, is_process_running, kill_process_tree};
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

struct SyncResult {
    running_pid: Option<u32>,
    exited: bool,
    exit_code: Option<i64>,
    exit_success: bool,
    exit_message: Option<String>,
}

fn mutex_error() -> AppError {
    AppError::new_validation(
        "WORKER_STATE_LOCK_FAILED",
        "Could not access the in-memory worker state cache.",
    )
}

fn worker_key(worker_id: &str) -> String {
    worker_id.to_string()
}

fn worker_status_message(worker: &ProjectWorker, exit_status: &ExitStatus) -> Option<String> {
    if exit_status.success() {
        return None;
    }

    let detail = exit_status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "an unknown exit status".to_string());

    Some(format!(
        "{} stopped unexpectedly with {}.",
        worker.name, detail
    ))
}

fn sync_tracked_process(state: &AppState, worker: &ProjectWorker) -> Result<SyncResult, AppError> {
    let mut processes = state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?;
    let key = worker_key(&worker.id);
    let mut running_pid = None;
    let mut exited = false;
    let mut exit_code = None;
    let mut exit_success = false;
    let mut exit_message = None;

    if let Some(process) = processes.get_mut(&key) {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                exited = true;
                exit_code = status.code().map(i64::from);
                exit_success = status.success();
                exit_message = worker_status_message(worker, &status);
            }
            Ok(None) => {
                running_pid = Some(process.pid);
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "WORKER_STATUS_FAILED",
                    format!(
                        "Could not inspect the {} worker process state.",
                        worker.name
                    ),
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
        exit_code,
        exit_success,
        exit_message,
    })
}

fn save_running_state(
    connection: &Connection,
    worker_id: &str,
    pid: u32,
) -> Result<ProjectWorker, AppError> {
    let timestamp = now_iso()?;
    ProjectWorkerRepository::set_status(
        connection,
        worker_id,
        &ProjectWorkerStatus::Running,
        Some(i64::from(pid)),
        Some(timestamp.as_str()),
        None,
        None,
        None,
    )
}

fn save_stopped_state(
    connection: &Connection,
    worker_id: &str,
    exit_code: Option<i64>,
) -> Result<ProjectWorker, AppError> {
    let timestamp = now_iso()?;
    ProjectWorkerRepository::set_status(
        connection,
        worker_id,
        &ProjectWorkerStatus::Stopped,
        None,
        None,
        Some(timestamp.as_str()),
        exit_code,
        None,
    )
}

fn save_error_state(
    connection: &Connection,
    worker_id: &str,
    exit_code: Option<i64>,
    message: &str,
) -> Result<ProjectWorker, AppError> {
    let timestamp = now_iso()?;
    ProjectWorkerRepository::set_status(
        connection,
        worker_id,
        &ProjectWorkerStatus::Error,
        None,
        None,
        Some(timestamp.as_str()),
        exit_code,
        Some(message),
    )
}

fn default_preset_args(preset_type: &ProjectWorkerPresetType) -> Vec<String> {
    match preset_type {
        ProjectWorkerPresetType::Queue => vec!["artisan".to_string(), "queue:work".to_string()],
        ProjectWorkerPresetType::Schedule => {
            vec!["artisan".to_string(), "schedule:work".to_string()]
        }
        ProjectWorkerPresetType::Custom => Vec::new(),
    }
}

fn worker_uses_php_cli(worker: &ProjectWorker) -> bool {
    if worker.command.eq_ignore_ascii_case("php") {
        return true;
    }

    PathBuf::from(&worker.command)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("php.exe"))
        .unwrap_or(false)
}

fn resolve_worker_binary(
    connection: &Connection,
    worker: &ProjectWorker,
) -> Result<PathBuf, AppError> {
    if worker_uses_php_cli(worker) {
        let project = ProjectRepository::get(connection, &worker.project_id)?;
        return runtime_registry::resolve_php_binary(connection, &project.php_version);
    }

    Ok(PathBuf::from(&worker.command))
}

fn resolve_worker_args(worker: &ProjectWorker) -> Vec<String> {
    if worker.args.is_empty() {
        return default_preset_args(&worker.preset_type);
    }

    worker.args.clone()
}

fn decorate_worker_args_for_runtime(
    connection: &Connection,
    state: &AppState,
    worker: &ProjectWorker,
    args: Vec<String>,
) -> Result<Vec<String>, AppError> {
    if !worker_uses_php_cli(worker) {
        return Ok(args);
    }

    if args
        .iter()
        .any(|value| value == "-c" || value == "--php-ini")
    {
        return Ok(args);
    }

    let project = ProjectRepository::get(connection, &worker.project_id)?;
    let config_path = runtime_registry::build_managed_php_config(
        connection,
        &state.workspace_dir,
        &project.php_version,
    )?;
    let mut next_args = Vec::with_capacity(args.len() + 2);
    next_args.push("-c".to_string());
    next_args.push(config_path.to_string_lossy().to_string());
    next_args.extend(args);

    Ok(next_args)
}

fn ensure_launch_paths(worker: &ProjectWorker) -> Result<PathBuf, AppError> {
    let working_dir = PathBuf::from(&worker.working_directory);
    if !working_dir.exists() || !working_dir.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_WORKER_DIRECTORY",
            format!(
                "{} cannot start because its working directory is missing.",
                worker.name
            ),
        ));
    }

    Ok(working_dir)
}

pub fn get_project_worker_status(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<ProjectWorker, AppError> {
    let current = ProjectWorkerRepository::get(connection, worker_id)?;
    let sync_result = sync_tracked_process(state, &current)?;

    if let Some(pid) = sync_result.running_pid {
        return save_running_state(connection, worker_id, pid);
    }

    if sync_result.exited {
        if sync_result.exit_success {
            return save_stopped_state(connection, worker_id, sync_result.exit_code);
        }

        return save_error_state(
            connection,
            worker_id,
            sync_result.exit_code,
            sync_result
                .exit_message
                .as_deref()
                .unwrap_or("The worker stopped unexpectedly."),
        );
    }

    if let Some(pid) = current.pid {
        if is_process_running(pid as u32)? {
            return save_running_state(connection, worker_id, pid as u32);
        }

        return save_stopped_state(connection, worker_id, current.last_exit_code);
    }

    Ok(current)
}

pub fn list_project_workers(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Vec<ProjectWorker>, AppError> {
    let workers = ProjectWorkerRepository::list_by_project(connection, project_id)?;
    let mut synchronized = Vec::with_capacity(workers.len());

    for worker in workers {
        synchronized.push(get_project_worker_status(connection, state, &worker.id)?);
    }

    Ok(synchronized)
}

pub fn list_all_workers(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<ProjectWorker>, AppError> {
    let workers = ProjectWorkerRepository::list_all(connection)?;
    let mut synchronized = Vec::with_capacity(workers.len());

    for worker in workers {
        synchronized.push(get_project_worker_status(connection, state, &worker.id)?);
    }

    Ok(synchronized)
}

pub fn start_project_worker(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<ProjectWorker, AppError> {
    let current = get_project_worker_status(connection, state, worker_id)?;
    if matches!(current.status, ProjectWorkerStatus::Running) && current.pid.is_some() {
        return Ok(current);
    }

    ProjectWorkerRepository::set_status(
        connection,
        worker_id,
        &ProjectWorkerStatus::Starting,
        None,
        None,
        None,
        None,
        None,
    )?;
    let worker = ProjectWorkerRepository::get(connection, worker_id)?;

    let binary_path = match resolve_worker_binary(connection, &worker) {
        Ok(path) => path,
        Err(error) => {
            let _ = save_error_state(connection, worker_id, None, &error.message);
            return Err(error);
        }
    };
    let working_dir = match ensure_launch_paths(&worker) {
        Ok(path) => path,
        Err(error) => {
            let _ = save_error_state(connection, worker_id, None, &error.message);
            return Err(error);
        }
    };
    let args = match decorate_worker_args_for_runtime(
        connection,
        state,
        &worker,
        resolve_worker_args(&worker),
    ) {
        Ok(args) => args,
        Err(error) => {
            let _ = save_error_state(connection, worker_id, None, &error.message);
            return Err(error);
        }
    };
    let log_path = PathBuf::from(&worker.log_path);

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "WORKER_START_FAILED",
                format!("Could not create the log directory for {}.", worker.name),
                error.to_string(),
            )
        })?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            AppError::with_details(
                "WORKER_START_FAILED",
                format!("Could not open the log file for {}.", worker.name),
                error.to_string(),
            )
        })?;
    let stderr = stdout.try_clone().map_err(|error| {
        AppError::with_details(
            "WORKER_START_FAILED",
            format!("Could not duplicate the log stream for {}.", worker.name),
            error.to_string(),
        )
    })?;

    let mut command = Command::new(&binary_path);
    command.args(&args);
    configure_background_command(&mut command);
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    command.stdin(Stdio::null());
    command.current_dir(&working_dir);

    let mut child = command.spawn().map_err(|error| {
        AppError::with_details(
            "WORKER_START_FAILED",
            format!("Could not start {}.", worker.name),
            error.to_string(),
        )
    })?;

    thread::sleep(Duration::from_millis(250));
    match child.try_wait() {
        Ok(Some(status)) => {
            let exit_code = status.code().map(i64::from);
            if status.success() {
                return save_stopped_state(connection, worker_id, exit_code);
            }

            let log_tail = log_reader::read_tail(&log_path, 20)?;
            let error_message = worker_status_message(&worker, &status)
                .unwrap_or_else(|| format!("{} exited immediately after launch.", worker.name));
            let _ = save_error_state(connection, worker_id, exit_code, &error_message);

            return Err(AppError::with_details(
                "WORKER_START_FAILED",
                error_message,
                if log_tail.is_empty() {
                    "No log output was captured.".to_string()
                } else {
                    log_tail
                },
            ));
        }
        Ok(None) => {}
        Err(error) => {
            let _ = save_error_state(
                connection,
                worker_id,
                None,
                "The worker started but its process state could not be verified.",
            );

            return Err(AppError::with_details(
                "WORKER_START_FAILED",
                format!("Could not confirm that {} is running.", worker.name),
                error.to_string(),
            ));
        }
    }

    let pid = child.id();
    state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?
        .insert(
            worker_key(worker_id),
            ManagedWorkerProcess {
                pid,
                child,
                log_path,
            },
        );

    save_running_state(connection, worker_id, pid)
}

pub fn stop_project_worker(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<ProjectWorker, AppError> {
    let current = get_project_worker_status(connection, state, worker_id)?;
    let tracked = state
        .managed_worker_processes
        .lock()
        .map_err(|_| mutex_error())?
        .remove(worker_id);

    if let Some(mut process) = tracked {
        match process.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                process.child.kill().map_err(|error| {
                    AppError::with_details(
                        "WORKER_STOP_FAILED",
                        format!("Could not stop {}.", current.name),
                        error.to_string(),
                    )
                })?;
                let _ = process.child.wait();
            }
            Err(error) => {
                return Err(AppError::with_details(
                    "WORKER_STOP_FAILED",
                    format!("Could not inspect {} before stopping it.", current.name),
                    error.to_string(),
                ));
            }
        }

        return save_stopped_state(connection, worker_id, None);
    }

    if let Some(pid) = current.pid {
        if is_process_running(pid as u32)? {
            kill_process_tree(pid as u32)?;
        }
    }

    save_stopped_state(connection, worker_id, None)
}

pub fn restart_project_worker(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<ProjectWorker, AppError> {
    ProjectWorkerRepository::set_status(
        connection,
        worker_id,
        &ProjectWorkerStatus::Restarting,
        None,
        None,
        None,
        None,
        None,
    )?;
    let _ = stop_project_worker(connection, state, worker_id)?;
    start_project_worker(connection, state, worker_id)
}

pub fn read_project_worker_logs(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
    lines: usize,
) -> Result<log_reader::ProjectWorkerLogPayload, AppError> {
    let worker = ProjectWorkerRepository::get(connection, worker_id)?;
    let log_path = resolve_project_worker_log_path(state, &worker)?;
    log_reader::read_tail_payload(&log_path, &worker.name, lines)
}

pub fn clear_project_worker_logs(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<(), AppError> {
    let worker = ProjectWorkerRepository::get(connection, worker_id)?;
    let log_path = resolve_project_worker_log_path(state, &worker)?;
    log_reader::clear(&log_path)
}

pub fn resolve_project_worker_log_path(
    state: &AppState,
    worker: &ProjectWorker,
) -> Result<PathBuf, AppError> {
    Ok(
        if let Some(process) = state
            .managed_worker_processes
            .lock()
            .map_err(|_| mutex_error())?
            .get(&worker.id)
        {
            process.log_path.clone()
        } else {
            PathBuf::from(&worker.log_path)
        },
    )
}

pub fn delete_project_worker(
    connection: &Connection,
    state: &AppState,
    worker_id: &str,
) -> Result<bool, AppError> {
    let _ = stop_project_worker(connection, state, worker_id);
    ProjectWorkerRepository::delete(connection, worker_id)
}

pub fn delete_workers_for_project(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<usize, AppError> {
    for worker in ProjectWorkerRepository::list_by_project(connection, project_id)? {
        let _ = stop_project_worker(connection, state, &worker.id);
    }

    ProjectWorkerRepository::delete_by_project(connection, project_id)
}

pub fn auto_start_project_workers(connection: &Connection, state: &AppState) {
    let workers = match ProjectWorkerRepository::list_all(connection) {
        Ok(workers) => workers,
        Err(error) => {
            eprintln!("DevNest boot worker load failed: {}", error);
            return;
        }
    };

    for worker in workers.into_iter().filter(|worker| worker.auto_start) {
        if let Err(error) = start_project_worker(connection, state, &worker.id) {
            let _ = save_error_state(connection, &worker.id, None, &error.message);
            eprintln!(
                "DevNest boot auto-start failed for worker {}: {}",
                worker.name, error
            );
        }
    }
}

pub fn stop_all_project_workers(state: &AppState) {
    let worker_ids = match state.managed_worker_processes.lock() {
        Ok(processes) => processes.keys().cloned().collect::<Vec<_>>(),
        Err(_) => {
            eprintln!("DevNest could not acquire the worker process lock during exit.");
            return;
        }
    };

    for worker_id in worker_ids {
        let process = match state.managed_worker_processes.lock() {
            Ok(mut processes) => processes.remove(&worker_id),
            Err(_) => None,
        };

        let Some(mut process) = process else {
            continue;
        };

        if let Ok(None) = process.child.try_wait() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clear_project_worker_logs, decorate_worker_args_for_runtime, read_project_worker_logs,
        start_project_worker, stop_project_worker,
    };
    use crate::models::project::{CreateProjectInput, FrameworkType, ServerType};
    use crate::models::worker::{
        CreateProjectWorkerInput, ProjectWorkerPresetType, ProjectWorkerStatus,
    };
    use crate::state::AppState;
    use crate::storage::db::init_database;
    use crate::storage::project_workers::ProjectWorkerRepository;
    use crate::storage::repositories::ProjectRepository;
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn setup_state() -> (std::path::PathBuf, AppState, Connection, String) {
        let root = std::env::temp_dir().join(format!("devnest-worker-manager-{}", Uuid::new_v4()));
        let workspace_dir = root.join("workspace");
        let project_dir = root.join("project");
        let db_path = workspace_dir.join("devnest.sqlite3");
        fs::create_dir_all(&workspace_dir).expect("workspace should exist");
        fs::create_dir_all(&project_dir).expect("project should exist");
        init_database(&db_path).expect("database should initialize");
        let connection = Connection::open(&db_path).expect("db should open");
        let project = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                name: "Worker Project".to_string(),
                path: project_dir.to_string_lossy().to_string(),
                domain: "worker-project.test".to_string(),
                server_type: ServerType::Apache,
                php_version: "8.2".to_string(),
                framework: FrameworkType::Php,
                document_root: ".".to_string(),
                ssl_enabled: false,
                database_name: None,
                database_port: None,
                frankenphp_mode: None,
            },
        )
        .expect("project should create");

        let state = AppState {
            db_path,
            workspace_dir,
            resources_dir: root.join("resources"),
            started_at: "2026-04-21T00:00:00Z".to_string(),
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

        (root, state, connection, project.id)
    }

    #[test]
    fn starts_stops_reads_and_clears_logs_for_custom_worker() {
        let (root, state, connection, project_id) = setup_state();
        let script_path = root.join("worker-loop.ps1");
        fs::write(
            &script_path,
            "[Console]::Out.WriteLine('worker boot'); [Console]::Out.Flush(); while ($true) { [Console]::Out.WriteLine('worker heartbeat'); [Console]::Out.Flush(); Start-Sleep -Milliseconds 200 }",
        )
        .expect("script should exist");

        let log_path = state
            .workspace_dir
            .join("runtime-logs")
            .join("workers")
            .join(format!("{project_id}.log"));
        let created = ProjectWorkerRepository::create(
            &connection,
            "",
            CreateProjectWorkerInput {
                project_id,
                name: "Worker Loop".to_string(),
                preset_type: ProjectWorkerPresetType::Custom,
                command_line: format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -File {}",
                    script_path.to_string_lossy()
                ),
                working_directory: Some(root.join("project").to_string_lossy().to_string()),
                auto_start: false,
            },
            &root.join("project").to_string_lossy(),
            &log_path.to_string_lossy(),
        )
        .expect("worker should create");

        let started =
            start_project_worker(&connection, &state, &created.id).expect("worker should start");
        assert_eq!(started.status, ProjectWorkerStatus::Running);
        assert!(started.pid.is_some());

        let logs = read_project_worker_logs(&connection, &state, &created.id, 20)
            .expect("logs should read");
        assert_eq!(logs.name, "Worker Loop");

        clear_project_worker_logs(&connection, &state, &created.id).expect("logs should clear");
        let cleared_logs = read_project_worker_logs(&connection, &state, &created.id, 20)
            .expect("cleared logs should read");
        assert!(cleared_logs.content.is_empty());

        let stopped =
            stop_project_worker(&connection, &state, &created.id).expect("worker should stop");
        assert_eq!(stopped.status, ProjectWorkerStatus::Stopped);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn prepends_managed_php_config_for_php_workers() {
        let (root, state, connection, project_id) = setup_state();
        let runtime_home = root.join("php83");
        let ext_dir = runtime_home.join("ext");
        let php_binary = runtime_home.join("php.exe");
        fs::create_dir_all(&ext_dir).expect("ext dir should exist");
        fs::write(&php_binary, "fake").expect("php binary should exist");
        fs::write(ext_dir.join("php_mbstring.dll"), "fake").expect("mbstring should exist");
        fs::write(ext_dir.join("php_pdo_mysql.dll"), "fake").expect("pdo_mysql should exist");

        crate::storage::repositories::RuntimeVersionRepository::upsert(
            &connection,
            &crate::models::runtime::RuntimeType::Php,
            "8.3",
            &php_binary.to_string_lossy(),
            true,
        )
        .expect("php runtime should upsert");

        let worker = crate::models::worker::ProjectWorker {
            id: Uuid::new_v4().to_string(),
            project_id,
            name: "PHP CLI".to_string(),
            preset_type: ProjectWorkerPresetType::Custom,
            command: "php".to_string(),
            args: vec!["-v".to_string()],
            working_directory: root.join("project").to_string_lossy().to_string(),
            auto_start: false,
            status: ProjectWorkerStatus::Stopped,
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
            log_path: PathBuf::from("worker.log").to_string_lossy().to_string(),
            created_at: "2026-04-21T00:00:00Z".to_string(),
            updated_at: "2026-04-21T00:00:00Z".to_string(),
        };

        let args =
            decorate_worker_args_for_runtime(&connection, &state, &worker, worker.args.clone())
                .expect("php args should be decorated");

        assert_eq!(args.first().map(String::as_str), Some("-c"));
        assert!(args.get(1).is_some_and(|value| value.ends_with("php.ini")));
        assert_eq!(args.last().map(String::as_str), Some("-v"));

        fs::remove_dir_all(root).ok();
    }
}
