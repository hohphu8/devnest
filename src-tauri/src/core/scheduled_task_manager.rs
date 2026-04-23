use crate::core::{log_reader, runtime_registry};
use crate::error::AppError;
use crate::models::scheduled_task::{
    ProjectScheduledTask, ProjectScheduledTaskRun, ProjectScheduledTaskRunStatus,
    ProjectScheduledTaskScheduleMode, ProjectScheduledTaskSimpleScheduleKind,
    ProjectScheduledTaskStatus, ProjectScheduledTaskType,
};
use crate::state::{AppState, ManagedScheduledTaskRun};
use crate::storage::project_scheduled_tasks::{
    ProjectScheduledTaskRepository, ProjectScheduledTaskRunRepository,
};
use crate::storage::repositories::{ProjectRepository, now_iso};
use crate::utils::process::{configure_background_command, is_process_running, kill_process_tree};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, Local, LocalResult, NaiveDateTime, NaiveTime,
    TimeZone, Timelike, Utc, Weekday,
};
use reqwest::blocking::Client;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn mutex_error() -> AppError {
    AppError::new_validation(
        "SCHEDULED_TASK_STATE_LOCK_FAILED",
        "Could not access the in-memory scheduled task state cache.",
    )
}

fn parse_iso_utc(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            AppError::with_details(
                "INVALID_SCHEDULED_TASK_TIMESTAMP",
                "A stored scheduled task timestamp is invalid.",
                error.to_string(),
            )
        })
}

fn format_iso_utc(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn resolve_local_datetime(value: NaiveDateTime) -> Result<DateTime<Local>, AppError> {
    match Local.from_local_datetime(&value) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(primary, _) => Ok(primary),
        LocalResult::None => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_LOCAL_TIME",
            "The selected local schedule time could not be resolved on this machine.",
        )),
    }
}

fn parse_daily_time(value: &str) -> Result<NaiveTime, AppError> {
    NaiveTime::parse_from_str(value, "%H:%M").map_err(|error| {
        AppError::with_details(
            "INVALID_SCHEDULED_TASK_DAILY_TIME",
            "The stored daily schedule time is invalid.",
            error.to_string(),
        )
    })
}

fn weekly_day_to_weekday(value: i64) -> Result<Weekday, AppError> {
    match value {
        0 => Ok(Weekday::Mon),
        1 => Ok(Weekday::Tue),
        2 => Ok(Weekday::Wed),
        3 => Ok(Weekday::Thu),
        4 => Ok(Weekday::Fri),
        5 => Ok(Weekday::Sat),
        6 => Ok(Weekday::Sun),
        _ => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_WEEKDAY",
            "The stored weekly schedule day is invalid.",
        )),
    }
}

fn cron_weekday_index(weekday: Weekday) -> u32 {
    weekday.num_days_from_sunday()
}

fn cron_number(value: &str, min: u32, max: u32, allow_sunday_alias: bool) -> Option<u32> {
    let parsed = value.parse::<u32>().ok()?;
    if allow_sunday_alias && parsed == 7 {
        return Some(0);
    }

    if (min..=max).contains(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

fn cron_segment_matches(
    value: u32,
    segment: &str,
    min: u32,
    max: u32,
    allow_sunday_alias: bool,
) -> bool {
    let segment = segment.trim();
    if segment == "*" {
        return true;
    }

    if let Some((base, step_value)) = segment.split_once('/') {
        let Some(step) = step_value.trim().parse::<u32>().ok() else {
            return false;
        };
        if step == 0 {
            return false;
        }

        if base.trim() == "*" {
            return value >= min && (value - min).is_multiple_of(step);
        }

        if let Some((start, end)) = base.split_once('-') {
            let Some(start) = cron_number(start.trim(), min, max, allow_sunday_alias) else {
                return false;
            };
            let Some(end) = cron_number(end.trim(), min, max, allow_sunday_alias) else {
                return false;
            };
            return value >= start && value <= end && (value - start).is_multiple_of(step);
        }
    }

    if let Some((start, end)) = segment.split_once('-') {
        let Some(start) = cron_number(start.trim(), min, max, allow_sunday_alias) else {
            return false;
        };
        let Some(end) = cron_number(end.trim(), min, max, allow_sunday_alias) else {
            return false;
        };
        return value >= start && value <= end;
    }

    cron_number(segment, min, max, allow_sunday_alias)
        .map(|candidate| candidate == value)
        .unwrap_or(false)
}

fn cron_field_matches(
    value: u32,
    expression: &str,
    min: u32,
    max: u32,
    allow_sunday_alias: bool,
) -> bool {
    expression
        .split(',')
        .any(|segment| cron_segment_matches(value, segment, min, max, allow_sunday_alias))
}

fn cron_matches(candidate: DateTime<Local>, expression: &str) -> Result<bool, AppError> {
    let fields = expression.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_CRON",
            "Cron expression must contain five fields.",
        ));
    }

    Ok(
        cron_field_matches(candidate.minute(), fields[0], 0, 59, false)
            && cron_field_matches(candidate.hour(), fields[1], 0, 23, false)
            && cron_field_matches(candidate.day(), fields[2], 1, 31, false)
            && cron_field_matches(candidate.month(), fields[3], 1, 12, false)
            && cron_field_matches(
                cron_weekday_index(candidate.weekday()),
                fields[4],
                0,
                6,
                true,
            ),
    )
}

fn cron_next_run(
    expression: &str,
    after_local: DateTime<Local>,
) -> Result<DateTime<Utc>, AppError> {
    let mut candidate = after_local
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| {
            AppError::new_validation(
                "INVALID_SCHEDULED_TASK_CRON",
                "Could not normalize the next cron candidate timestamp.",
            )
        })?
        + ChronoDuration::minutes(1);

    for _ in 0..(366 * 24 * 60) {
        if cron_matches(candidate, expression)? {
            return Ok(candidate.with_timezone(&Utc));
        }
        candidate += ChronoDuration::minutes(1);
    }

    Err(AppError::new_validation(
        "INVALID_SCHEDULED_TASK_CRON",
        "Could not compute the next run time for this cron expression.",
    ))
}

fn interval_next_run(
    interval_seconds: i64,
    after_utc: DateTime<Utc>,
) -> Result<DateTime<Utc>, AppError> {
    let interval = ChronoDuration::seconds(interval_seconds);
    let now = Utc::now();
    let mut next = after_utc + interval;
    while next <= now {
        next += interval;
    }

    Ok(next)
}

fn daily_next_run(value: &str, after_local: DateTime<Local>) -> Result<DateTime<Utc>, AppError> {
    let daily_time = parse_daily_time(value)?;
    let mut candidate_date = after_local.date_naive();

    loop {
        let naive = NaiveDateTime::new(candidate_date, daily_time);
        let candidate = resolve_local_datetime(naive)?;
        if candidate > after_local {
            return Ok(candidate.with_timezone(&Utc));
        }
        candidate_date += ChronoDuration::days(1);
    }
}

fn weekly_next_run(
    daily_time: &str,
    weekly_day: i64,
    after_local: DateTime<Local>,
) -> Result<DateTime<Utc>, AppError> {
    let target_weekday = weekly_day_to_weekday(weekly_day)?;
    let daily_time = parse_daily_time(daily_time)?;
    let mut candidate_date = after_local.date_naive();

    for _ in 0..8 {
        if candidate_date.weekday() == target_weekday {
            let naive = NaiveDateTime::new(candidate_date, daily_time);
            let candidate = resolve_local_datetime(naive)?;
            if candidate > after_local {
                return Ok(candidate.with_timezone(&Utc));
            }
        }
        candidate_date += ChronoDuration::days(1);
    }

    Err(AppError::new_validation(
        "INVALID_SCHEDULED_TASK_WEEKLY_SCHEDULE",
        "Could not compute the next weekly run time.",
    ))
}

fn compute_next_run_at_from(
    task: &ProjectScheduledTask,
    after_utc: DateTime<Utc>,
) -> Result<Option<String>, AppError> {
    if !task.enabled {
        return Ok(None);
    }

    let next = match task.schedule_mode {
        ProjectScheduledTaskScheduleMode::Cron => {
            cron_next_run(&task.schedule_expression, after_utc.with_timezone(&Local))?
        }
        ProjectScheduledTaskScheduleMode::Simple => {
            match task.simple_schedule_kind.as_ref().ok_or_else(|| {
                AppError::new_validation(
                    "INVALID_SIMPLE_SCHEDULE_KIND",
                    "Simple schedules require a stored schedule kind.",
                )
            })? {
                ProjectScheduledTaskSimpleScheduleKind::EverySeconds
                | ProjectScheduledTaskSimpleScheduleKind::EveryMinutes
                | ProjectScheduledTaskSimpleScheduleKind::EveryHours => interval_next_run(
                    task.interval_seconds.ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_SCHEDULED_TASK_INTERVAL",
                            "Interval schedule is missing interval seconds.",
                        )
                    })?,
                    after_utc,
                )?,
                ProjectScheduledTaskSimpleScheduleKind::Daily => daily_next_run(
                    task.daily_time.as_deref().ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_SCHEDULED_TASK_DAILY_TIME",
                            "Daily schedule is missing its daily time.",
                        )
                    })?,
                    after_utc.with_timezone(&Local),
                )?,
                ProjectScheduledTaskSimpleScheduleKind::Weekly => weekly_next_run(
                    task.daily_time.as_deref().ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_SCHEDULED_TASK_DAILY_TIME",
                            "Weekly schedule is missing its daily time.",
                        )
                    })?,
                    task.weekly_day.ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_SCHEDULED_TASK_WEEKDAY",
                            "Weekly schedule is missing its weekday.",
                        )
                    })?,
                    after_utc.with_timezone(&Local),
                )?,
            }
        }
    };

    Ok(Some(format_iso_utc(next)))
}

fn resolve_schedule_state_after_resume(
    connection: &Connection,
    task: &ProjectScheduledTask,
) -> Result<ProjectScheduledTask, AppError> {
    if task.enabled && task.auto_resume {
        let next_run_at = compute_next_run_at_from(task, Utc::now())?;
        return ProjectScheduledTaskRepository::update_runtime_state(
            connection,
            &task.id,
            &ProjectScheduledTaskStatus::Scheduled,
            next_run_at.as_deref(),
            task.last_run_at.as_deref(),
            task.last_success_at.as_deref(),
            task.last_error.as_deref(),
            Some(true),
        );
    }

    ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        &task.id,
        &ProjectScheduledTaskStatus::Idle,
        None,
        task.last_run_at.as_deref(),
        task.last_success_at.as_deref(),
        task.last_error.as_deref(),
        Some(task.enabled),
    )
}

fn task_uses_php_cli(task: &ProjectScheduledTask) -> bool {
    let Some(command) = task.command.as_deref() else {
        return false;
    };

    if command.eq_ignore_ascii_case("php") {
        return true;
    }

    PathBuf::from(command)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("php.exe"))
        .unwrap_or(false)
}

fn resolve_task_binary(
    connection: &Connection,
    task: &ProjectScheduledTask,
) -> Result<PathBuf, AppError> {
    let command = task.command.as_deref().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_COMMAND",
            "Command task is missing its command binary.",
        )
    })?;

    if task_uses_php_cli(task) {
        let project = ProjectRepository::get(connection, &task.project_id)?;
        return runtime_registry::resolve_php_binary(connection, &project.php_version);
    }

    Ok(PathBuf::from(command))
}

fn decorate_task_args_for_runtime(
    connection: &Connection,
    state: &AppState,
    task: &ProjectScheduledTask,
    args: Vec<String>,
) -> Result<Vec<String>, AppError> {
    if !task_uses_php_cli(task) {
        return Ok(args);
    }

    if args
        .iter()
        .any(|value| value == "-c" || value == "--php-ini")
    {
        return Ok(args);
    }

    let project = ProjectRepository::get(connection, &task.project_id)?;
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

fn ensure_launch_paths(task: &ProjectScheduledTask) -> Result<PathBuf, AppError> {
    let working_dir = PathBuf::from(task.working_directory.as_deref().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_DIRECTORY",
            "Command task is missing its working directory.",
        )
    })?);

    if !working_dir.exists() || !working_dir.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_DIRECTORY",
            format!(
                "{} cannot start because its working directory is missing.",
                task.name
            ),
        ));
    }

    Ok(working_dir)
}

fn active_runs_lock(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, HashMap<String, ManagedScheduledTaskRun>>, AppError> {
    state
        .managed_scheduled_task_runs
        .lock()
        .map_err(|_| mutex_error())
}

fn is_task_running(state: &AppState, task_id: &str) -> Result<bool, AppError> {
    let active_runs = active_runs_lock(state)?;
    if let Some(active) = active_runs.get(task_id) {
        if let Some(pid) = active.pid {
            return is_process_running(pid);
        }
        return Ok(true);
    }
    Ok(false)
}

fn sync_task_status(
    connection: &Connection,
    state: &AppState,
    task: ProjectScheduledTask,
) -> Result<ProjectScheduledTask, AppError> {
    if is_task_running(state, &task.id)? {
        if matches!(task.status, ProjectScheduledTaskStatus::Running) {
            return Ok(task);
        }

        return ProjectScheduledTaskRepository::update_runtime_state(
            connection,
            &task.id,
            &ProjectScheduledTaskStatus::Running,
            task.next_run_at.as_deref(),
            task.last_run_at.as_deref(),
            task.last_success_at.as_deref(),
            task.last_error.as_deref(),
            Some(task.enabled),
        );
    }

    if matches!(task.status, ProjectScheduledTaskStatus::Running) {
        let fallback_status = if task.enabled && task.next_run_at.is_some() {
            ProjectScheduledTaskStatus::Scheduled
        } else {
            ProjectScheduledTaskStatus::Idle
        };

        return ProjectScheduledTaskRepository::update_runtime_state(
            connection,
            &task.id,
            &fallback_status,
            task.next_run_at.as_deref(),
            task.last_run_at.as_deref(),
            task.last_success_at.as_deref(),
            task.last_error.as_deref(),
            Some(task.enabled),
        );
    }

    Ok(task)
}

fn task_run_log_path(state: &AppState, task: &ProjectScheduledTask, run_id: &str) -> PathBuf {
    task_log_directory(state, task).join(format!("{run_id}.log"))
}

fn task_log_directory(state: &AppState, task: &ProjectScheduledTask) -> PathBuf {
    state
        .workspace_dir
        .join("runtime-logs")
        .join("scheduled-tasks")
        .join(&task.project_id)
        .join(&task.id)
}

fn remove_task_log_file(path: &PathBuf) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|error| {
        AppError::with_details(
            "SCHEDULED_TASK_LOG_DELETE_FAILED",
            "Could not delete a scheduled task log file.",
            error.to_string(),
        )
    })
}

fn cleanup_task_log_directory(state: &AppState, task: &ProjectScheduledTask) {
    let task_log_directory = task_log_directory(state, task);
    if task_log_directory.exists() {
        fs::remove_dir_all(&task_log_directory).ok();
    }

    if let Some(project_log_directory) = task_log_directory.parent() {
        fs::remove_dir(project_log_directory).ok();
    }
}

fn append_log_line(path: &PathBuf, line: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn complete_run(
    db_path: PathBuf,
    active_runs: Arc<Mutex<HashMap<String, ManagedScheduledTaskRun>>>,
    task_id: String,
    run_id: String,
    final_status: ProjectScheduledTaskStatus,
    run_status: ProjectScheduledTaskRunStatus,
    scheduled_next_run_at: Option<String>,
    exit_code: Option<i64>,
    response_status: Option<i64>,
    error_message: Option<String>,
    started_at: String,
    duration_ms: i64,
) {
    let connection = match Connection::open(&db_path) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!(
                "DevNest could not open the database to complete scheduled task run {}: {}",
                run_id, error
            );
            return;
        }
    };

    let finished_at = match now_iso() {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "DevNest could not create a timestamp to complete scheduled task run {}: {}",
                run_id, error
            );
            return;
        }
    };

    let _ = ProjectScheduledTaskRunRepository::update_result(
        &connection,
        &run_id,
        &finished_at,
        duration_ms,
        &run_status,
        exit_code,
        response_status,
        error_message.as_deref(),
    );

    if let Ok(mut active_runs) = active_runs.lock() {
        active_runs.remove(&task_id);
    }

    let task = match ProjectScheduledTaskRepository::get(&connection, &task_id) {
        Ok(task) => task,
        Err(error) => {
            eprintln!(
                "DevNest could not reload scheduled task {} after run {}: {}",
                task_id, run_id, error
            );
            return;
        }
    };

    let next_run_at = if task.enabled {
        scheduled_next_run_at.as_deref()
    } else {
        None
    };
    let disabled_mid_run =
        !task.enabled && !matches!(task.status, ProjectScheduledTaskStatus::Running);
    let last_success_at = if disabled_mid_run {
        task.last_success_at.as_deref()
    } else if matches!(run_status, ProjectScheduledTaskRunStatus::Success) {
        Some(finished_at.as_str())
    } else {
        task.last_success_at.as_deref()
    };
    let last_error = if disabled_mid_run {
        task.last_error.as_deref()
    } else if matches!(run_status, ProjectScheduledTaskRunStatus::Error) {
        error_message.as_deref()
    } else {
        None
    };
    let status = if !task.enabled {
        ProjectScheduledTaskStatus::Idle
    } else {
        final_status
    };

    let _ = ProjectScheduledTaskRepository::update_runtime_state(
        &connection,
        &task_id,
        &status,
        next_run_at,
        Some(started_at.as_str()),
        last_success_at,
        last_error,
        Some(task.enabled),
    );
}

fn spawn_command_run(
    connection: &Connection,
    state: &AppState,
    task: ProjectScheduledTask,
    run: ProjectScheduledTaskRun,
    next_run_at: Option<String>,
) -> Result<(), AppError> {
    let binary_path = resolve_task_binary(connection, &task)?;
    let args = decorate_task_args_for_runtime(connection, state, &task, task.args.clone())?;
    let working_dir = ensure_launch_paths(&task)?;
    let log_path = PathBuf::from(&run.log_path);

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            AppError::with_details(
                "SCHEDULED_TASK_RUN_FAILED",
                format!("Could not open the log file for {}.", task.name),
                error.to_string(),
            )
        })?;
    let stderr = stdout.try_clone().map_err(|error| {
        AppError::with_details(
            "SCHEDULED_TASK_RUN_FAILED",
            format!("Could not duplicate the log stream for {}.", task.name),
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
            "SCHEDULED_TASK_RUN_FAILED",
            format!("Could not run {}.", task.name),
            error.to_string(),
        )
    })?;
    let pid = child.id();
    {
        let mut active_runs = active_runs_lock(state)?;
        active_runs.insert(task.id.clone(), ManagedScheduledTaskRun { pid: Some(pid) });
    }

    let db_path = state.db_path.clone();
    let active_runs = Arc::clone(&state.managed_scheduled_task_runs);
    thread::spawn(move || {
        let started = Instant::now();
        let waited = child.wait();
        let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);

        match waited {
            Ok(status) if status.success() => complete_run(
                db_path,
                active_runs,
                task.id.clone(),
                run.id.clone(),
                ProjectScheduledTaskStatus::Success,
                ProjectScheduledTaskRunStatus::Success,
                next_run_at,
                status.code().map(i64::from),
                None,
                None,
                run.started_at.clone(),
                duration_ms,
            ),
            Ok(status) => complete_run(
                db_path,
                active_runs,
                task.id.clone(),
                run.id.clone(),
                ProjectScheduledTaskStatus::Error,
                ProjectScheduledTaskRunStatus::Error,
                next_run_at,
                status.code().map(i64::from),
                None,
                Some(format!(
                    "{} exited with {}.",
                    task.name,
                    status
                        .code()
                        .map(|code| format!("exit code {code}"))
                        .unwrap_or_else(|| "an unknown exit status".to_string())
                )),
                run.started_at.clone(),
                duration_ms,
            ),
            Err(error) => complete_run(
                db_path,
                active_runs,
                task.id.clone(),
                run.id.clone(),
                ProjectScheduledTaskStatus::Error,
                ProjectScheduledTaskRunStatus::Error,
                next_run_at,
                None,
                None,
                Some(format!("{} could not complete: {}", task.name, error)),
                run.started_at.clone(),
                duration_ms,
            ),
        }
    });

    Ok(())
}

fn spawn_url_run(
    state: &AppState,
    task: ProjectScheduledTask,
    run: ProjectScheduledTaskRun,
    next_run_at: Option<String>,
) -> Result<(), AppError> {
    let url = task.url.clone().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_URL",
            "URL task is missing its URL target.",
        )
    })?;
    let log_path = PathBuf::from(&run.log_path);
    {
        let mut active_runs = active_runs_lock(state)?;
        active_runs.insert(task.id.clone(), ManagedScheduledTaskRun { pid: None });
    }

    let db_path = state.db_path.clone();
    let active_runs = Arc::clone(&state.managed_scheduled_task_runs);
    thread::spawn(move || {
        let started = Instant::now();
        let _ = append_log_line(&log_path, &format!("[request] GET {url}"));

        let client = Client::new();
        let response = client.get(&url).send();
        let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);

        match response {
            Ok(response) => {
                let status_code = i64::from(response.status().as_u16());
                let body = response.text().unwrap_or_default();
                let _ = append_log_line(&log_path, &format!("[response] HTTP {status_code}"));
                if !body.trim().is_empty() {
                    let _ = append_log_line(&log_path, "");
                    let _ = append_log_line(&log_path, body.trim());
                }

                if (200..300).contains(&(status_code as u16)) {
                    complete_run(
                        db_path,
                        active_runs,
                        task.id.clone(),
                        run.id.clone(),
                        ProjectScheduledTaskStatus::Success,
                        ProjectScheduledTaskRunStatus::Success,
                        next_run_at,
                        None,
                        Some(status_code),
                        None,
                        run.started_at.clone(),
                        duration_ms,
                    );
                } else {
                    complete_run(
                        db_path,
                        active_runs,
                        task.id.clone(),
                        run.id.clone(),
                        ProjectScheduledTaskStatus::Error,
                        ProjectScheduledTaskRunStatus::Error,
                        next_run_at,
                        None,
                        Some(status_code),
                        Some(format!("{} returned HTTP {}.", task.name, status_code)),
                        run.started_at.clone(),
                        duration_ms,
                    );
                }
            }
            Err(error) => {
                let _ = append_log_line(&log_path, &format!("[error] {}", error));
                complete_run(
                    db_path,
                    active_runs,
                    task.id.clone(),
                    run.id.clone(),
                    ProjectScheduledTaskStatus::Error,
                    ProjectScheduledTaskRunStatus::Error,
                    next_run_at,
                    None,
                    None,
                    Some(format!("{} request failed: {}", task.name, error)),
                    run.started_at.clone(),
                    duration_ms,
                );
            }
        }
    });

    Ok(())
}

fn create_running_run(
    connection: &Connection,
    state: &AppState,
    task: &ProjectScheduledTask,
    next_run_at: Option<String>,
) -> Result<ProjectScheduledTaskRun, AppError> {
    let started_at = now_iso()?;
    let run_id = Uuid::new_v4().to_string();
    let log_path = task_run_log_path(state, task, &run_id);

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let run = ProjectScheduledTaskRunRepository::create(
        connection,
        &run_id,
        &task.id,
        &task.project_id,
        &started_at,
        &log_path.to_string_lossy(),
    )?;
    let _ = ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        &task.id,
        &ProjectScheduledTaskStatus::Running,
        next_run_at.as_deref(),
        Some(started_at.as_str()),
        task.last_success_at.as_deref(),
        None,
        Some(task.enabled),
    )?;

    Ok(run)
}

fn dispatch_task_run(
    connection: &Connection,
    state: &AppState,
    task: ProjectScheduledTask,
    next_run_at: Option<String>,
) -> Result<ProjectScheduledTask, AppError> {
    if is_task_running(state, &task.id)? {
        return Err(AppError::new_validation(
            "SCHEDULED_TASK_ALREADY_RUNNING",
            format!("{} is already running.", task.name),
        ));
    }

    let run = create_running_run(connection, state, &task, next_run_at.clone())?;
    match task.task_type {
        ProjectScheduledTaskType::Command => {
            if let Err(error) =
                spawn_command_run(connection, state, task.clone(), run.clone(), next_run_at)
            {
                let _ = append_log_line(
                    &PathBuf::from(&run.log_path),
                    &format!("[error] {}", error.message),
                );
                let _ = ProjectScheduledTaskRunRepository::update_result(
                    connection,
                    &run.id,
                    &now_iso()?,
                    0,
                    &ProjectScheduledTaskRunStatus::Error,
                    None,
                    None,
                    Some(error.message.as_str()),
                );
                let _ = ProjectScheduledTaskRepository::update_runtime_state(
                    connection,
                    &task.id,
                    &ProjectScheduledTaskStatus::Error,
                    task.next_run_at.as_deref(),
                    Some(run.started_at.as_str()),
                    task.last_success_at.as_deref(),
                    Some(error.message.as_str()),
                    Some(task.enabled),
                );
                return Err(error);
            }
        }
        ProjectScheduledTaskType::Url => {
            if let Err(error) = spawn_url_run(state, task.clone(), run.clone(), next_run_at) {
                let _ = append_log_line(
                    &PathBuf::from(&run.log_path),
                    &format!("[error] {}", error.message),
                );
                let _ = ProjectScheduledTaskRunRepository::update_result(
                    connection,
                    &run.id,
                    &now_iso()?,
                    0,
                    &ProjectScheduledTaskRunStatus::Error,
                    None,
                    None,
                    Some(error.message.as_str()),
                );
                let _ = ProjectScheduledTaskRepository::update_runtime_state(
                    connection,
                    &task.id,
                    &ProjectScheduledTaskStatus::Error,
                    task.next_run_at.as_deref(),
                    Some(run.started_at.as_str()),
                    task.last_success_at.as_deref(),
                    Some(error.message.as_str()),
                    Some(task.enabled),
                );
                return Err(error);
            }
        }
    }

    get_project_scheduled_task_status(connection, state, &task.id)
}

fn write_skipped_run(
    connection: &Connection,
    state: &AppState,
    task: &ProjectScheduledTask,
    next_run_at: Option<String>,
) -> Result<(), AppError> {
    let started_at = now_iso()?;
    let run_id = Uuid::new_v4().to_string();
    let log_path = task_run_log_path(state, task, &run_id);
    append_log_line(
        &log_path,
        &format!(
            "{} skipped a due run because the previous execution is still active.",
            task.name
        ),
    )?;
    let run = ProjectScheduledTaskRunRepository::create(
        connection,
        &run_id,
        &task.id,
        &task.project_id,
        &started_at,
        &log_path.to_string_lossy(),
    )?;
    ProjectScheduledTaskRunRepository::update_result(
        connection,
        &run.id,
        &started_at,
        0,
        &ProjectScheduledTaskRunStatus::Skipped,
        None,
        None,
        Some("Skipped because the previous run is still active."),
    )?;
    let status = if matches!(task.status, ProjectScheduledTaskStatus::Running) {
        ProjectScheduledTaskStatus::Running
    } else {
        ProjectScheduledTaskStatus::Skipped
    };
    let _ = ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        &task.id,
        &status,
        next_run_at.as_deref(),
        task.last_run_at.as_deref(),
        task.last_success_at.as_deref(),
        task.last_error.as_deref(),
        Some(task.enabled),
    )?;
    Ok(())
}

pub fn prepare_auto_resume_project_scheduled_tasks(
    connection: &Connection,
) -> Result<(), AppError> {
    for task in ProjectScheduledTaskRepository::list_all(connection)? {
        let _ = resolve_schedule_state_after_resume(connection, &task)?;
    }
    Ok(())
}

pub fn run_scheduler_loop(
    db_path: PathBuf,
    workspace_dir: PathBuf,
    active_runs: Arc<Mutex<HashMap<String, ManagedScheduledTaskRun>>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match Connection::open(&db_path) {
            Ok(connection) => {
                let state = AppState {
                    db_path: db_path.clone(),
                    workspace_dir: workspace_dir.clone(),
                    resources_dir: PathBuf::new(),
                    started_at: String::new(),
                    allow_exit: Mutex::new(false),
                    managed_processes: Mutex::new(HashMap::new()),
                    managed_worker_processes: Mutex::new(HashMap::new()),
                    managed_scheduled_task_runs: Arc::clone(&active_runs),
                    scheduled_task_scheduler_shutdown: Arc::clone(&shutdown),
                    runtime_install_task: Mutex::new(None),
                    optional_tool_install_task: Mutex::new(None),
                    project_tunnels: Mutex::new(HashMap::new()),
                    project_persistent_tunnels: Mutex::new(HashMap::new()),
                    project_mobile_previews: Mutex::new(HashMap::new()),
                };

                if let Ok(tasks) = ProjectScheduledTaskRepository::list_all(&connection) {
                    for task in tasks
                        .into_iter()
                        .filter(|task| task.enabled && task.next_run_at.is_some())
                    {
                        let Ok(current) = sync_task_status(&connection, &state, task) else {
                            continue;
                        };
                        let Some(next_run_at) = current.next_run_at.as_deref() else {
                            continue;
                        };
                        let Ok(next_run_at) = parse_iso_utc(next_run_at) else {
                            continue;
                        };
                        if next_run_at > Utc::now() {
                            continue;
                        }

                        let advanced_next = match compute_next_run_at_from(&current, next_run_at) {
                            Ok(value) => value,
                            Err(error) => {
                                eprintln!(
                                    "DevNest could not compute the next run for scheduled task {}: {}",
                                    current.name, error
                                );
                                let _ = ProjectScheduledTaskRepository::update_runtime_state(
                                    &connection,
                                    &current.id,
                                    &ProjectScheduledTaskStatus::Error,
                                    current.next_run_at.as_deref(),
                                    current.last_run_at.as_deref(),
                                    current.last_success_at.as_deref(),
                                    Some(error.message.as_str()),
                                    Some(current.enabled),
                                );
                                continue;
                            }
                        };
                        if is_task_running(&state, &current.id).unwrap_or(false) {
                            let _ = write_skipped_run(&connection, &state, &current, advanced_next);
                            continue;
                        }

                        let _ = dispatch_task_run(&connection, &state, current, advanced_next);
                    }
                }
            }
            Err(error) => {
                eprintln!(
                    "DevNest scheduled task scheduler could not open the database: {}",
                    error
                );
            }
        }

        thread::sleep(Duration::from_secs(1));
    }
}

pub fn list_project_scheduled_tasks(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<Vec<ProjectScheduledTask>, AppError> {
    let tasks = ProjectScheduledTaskRepository::list_by_project(connection, project_id)?;
    let mut synchronized = Vec::with_capacity(tasks.len());
    for task in tasks {
        synchronized.push(sync_task_status(connection, state, task)?);
    }
    Ok(synchronized)
}

pub fn list_all_scheduled_tasks(
    connection: &Connection,
    state: &AppState,
) -> Result<Vec<ProjectScheduledTask>, AppError> {
    let tasks = ProjectScheduledTaskRepository::list_all(connection)?;
    let mut synchronized = Vec::with_capacity(tasks.len());
    for task in tasks {
        synchronized.push(sync_task_status(connection, state, task)?);
    }
    Ok(synchronized)
}

pub fn get_project_scheduled_task_status(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<ProjectScheduledTask, AppError> {
    let task = ProjectScheduledTaskRepository::get(connection, task_id)?;
    sync_task_status(connection, state, task)
}

pub fn enable_project_scheduled_task(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<ProjectScheduledTask, AppError> {
    let task = ProjectScheduledTaskRepository::get(connection, task_id)?;
    let mut enabled_task = ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        task_id,
        &ProjectScheduledTaskStatus::Idle,
        None,
        task.last_run_at.as_deref(),
        task.last_success_at.as_deref(),
        task.last_error.as_deref(),
        Some(true),
    )?;
    let next_run_at = compute_next_run_at_from(&enabled_task, Utc::now())?;
    enabled_task = ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        task_id,
        &ProjectScheduledTaskStatus::Scheduled,
        next_run_at.as_deref(),
        enabled_task.last_run_at.as_deref(),
        enabled_task.last_success_at.as_deref(),
        enabled_task.last_error.as_deref(),
        Some(true),
    )?;
    sync_task_status(connection, state, enabled_task)
}

pub fn disable_project_scheduled_task(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<ProjectScheduledTask, AppError> {
    let task = ProjectScheduledTaskRepository::get(connection, task_id)?;
    if let Ok(mut active_runs) = active_runs_lock(state) {
        if let Some(active) = active_runs.remove(task_id) {
            if let Some(pid) = active.pid {
                let _ = kill_process_tree(pid);
            }
        }
    }

    ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        task_id,
        &ProjectScheduledTaskStatus::Idle,
        None,
        task.last_run_at.as_deref(),
        task.last_success_at.as_deref(),
        task.last_error.as_deref(),
        Some(false),
    )
}

pub fn run_project_scheduled_task_now(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<ProjectScheduledTask, AppError> {
    let task = get_project_scheduled_task_status(connection, state, task_id)?;
    dispatch_task_run(connection, state, task.clone(), task.next_run_at.clone())
}

pub fn list_project_scheduled_task_runs(
    connection: &Connection,
    task_id: &str,
    limit: usize,
) -> Result<Vec<ProjectScheduledTaskRun>, AppError> {
    ProjectScheduledTaskRunRepository::list_by_task(connection, task_id, limit.clamp(1, 200))
}

pub fn read_project_scheduled_task_run_logs(
    connection: &Connection,
    run_id: &str,
    lines: usize,
) -> Result<log_reader::ProjectScheduledTaskRunLogPayload, AppError> {
    let run = ProjectScheduledTaskRunRepository::get(connection, run_id)?;
    log_reader::read_tail_payload(
        &PathBuf::from(&run.log_path),
        &format!("Task Run {}", run.id),
        lines,
    )
}

pub fn clear_project_scheduled_task_logs(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<(), AppError> {
    let task = ProjectScheduledTaskRepository::get(connection, task_id)?;
    for run in ProjectScheduledTaskRunRepository::list_by_task(connection, task_id, 5000)? {
        remove_task_log_file(&PathBuf::from(run.log_path))?;
    }
    cleanup_task_log_directory(state, &task);
    Ok(())
}

pub fn clear_project_scheduled_task_history(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<ProjectScheduledTask, AppError> {
    let task = get_project_scheduled_task_status(connection, state, task_id)?;
    if matches!(task.status, ProjectScheduledTaskStatus::Running) {
        return Err(AppError::new_validation(
            "SCHEDULED_TASK_HISTORY_CLEAR_BLOCKED",
            "Stop the scheduled task before clearing its run history.",
        ));
    }

    for run in ProjectScheduledTaskRunRepository::list_by_task(connection, task_id, 5000)? {
        remove_task_log_file(&PathBuf::from(run.log_path))?;
    }
    ProjectScheduledTaskRunRepository::delete_by_task(connection, task_id)?;
    cleanup_task_log_directory(state, &task);

    let next_run_at = if task.enabled {
        compute_next_run_at_from(&task, Utc::now())?
    } else {
        None
    };
    let status = if task.enabled && next_run_at.is_some() {
        ProjectScheduledTaskStatus::Scheduled
    } else {
        ProjectScheduledTaskStatus::Idle
    };

    ProjectScheduledTaskRepository::update_runtime_state(
        connection,
        task_id,
        &status,
        next_run_at.as_deref(),
        None,
        None,
        None,
        Some(task.enabled),
    )
}

pub fn delete_project_scheduled_task(
    connection: &Connection,
    state: &AppState,
    task_id: &str,
) -> Result<bool, AppError> {
    let task = ProjectScheduledTaskRepository::get(connection, task_id)?;
    let _ = disable_project_scheduled_task(connection, state, task_id);
    let deleted = ProjectScheduledTaskRepository::delete(connection, task_id)?;
    if deleted {
        cleanup_task_log_directory(state, &task);
    }
    Ok(deleted)
}

pub fn delete_tasks_for_project(
    connection: &Connection,
    state: &AppState,
    project_id: &str,
) -> Result<usize, AppError> {
    for task in ProjectScheduledTaskRepository::list_by_project(connection, project_id)? {
        let _ = disable_project_scheduled_task(connection, state, &task.id);
        cleanup_task_log_directory(state, &task);
    }

    ProjectScheduledTaskRepository::delete_by_project(connection, project_id)
}

pub fn stop_all_project_scheduled_tasks(state: &AppState) {
    let task_ids = match active_runs_lock(state) {
        Ok(active_runs) => active_runs.keys().cloned().collect::<Vec<_>>(),
        Err(_) => {
            eprintln!("DevNest could not acquire the scheduled task lock during exit.");
            return;
        }
    };

    for task_id in task_ids {
        let active = match active_runs_lock(state) {
            Ok(mut active_runs) => active_runs.remove(&task_id),
            Err(_) => None,
        };
        if let Some(active) = active.and_then(|active| active.pid) {
            let _ = kill_process_tree(active);
        }
    }
}
