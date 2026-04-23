use crate::error::AppError;
use crate::models::scheduled_task::{
    CreateProjectScheduledTaskInput, ProjectScheduledTask, ProjectScheduledTaskOverlapPolicy,
    ProjectScheduledTaskRun, ProjectScheduledTaskRunStatus, ProjectScheduledTaskScheduleMode,
    ProjectScheduledTaskSimpleScheduleKind, ProjectScheduledTaskStatus, ProjectScheduledTaskType,
    UpdateProjectScheduledTaskPatch,
};
use crate::storage::repositories::{ProjectRepository, now_iso};
use crate::utils::process::{join_command_args, split_command_args};
use reqwest::Url;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{from_str, to_string};
use std::path::Path;
use uuid::Uuid;

fn parse_task_type(value: &str) -> Result<ProjectScheduledTaskType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_TYPE",
            "Stored scheduled task type is invalid.",
        )
    })
}

fn parse_schedule_mode(value: &str) -> Result<ProjectScheduledTaskScheduleMode, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_SCHEDULE_MODE",
            "Stored scheduled task schedule mode is invalid.",
        )
    })
}

fn parse_simple_schedule_kind(
    value: Option<String>,
) -> Result<Option<ProjectScheduledTaskSimpleScheduleKind>, AppError> {
    value
        .map(|value| {
            value.parse().map_err(|_| {
                AppError::new_validation(
                    "INVALID_SIMPLE_SCHEDULE_KIND",
                    "Stored simple schedule kind is invalid.",
                )
            })
        })
        .transpose()
}

fn parse_overlap_policy(value: &str) -> Result<ProjectScheduledTaskOverlapPolicy, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_OVERLAP_POLICY",
            "Stored overlap policy is invalid.",
        )
    })
}

fn parse_task_status(value: &str) -> Result<ProjectScheduledTaskStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_STATUS",
            "Stored scheduled task status is invalid.",
        )
    })
}

fn parse_run_status(value: &str) -> Result<ProjectScheduledTaskRunStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SCHEDULED_TASK_RUN_STATUS",
            "Stored scheduled task run status is invalid.",
        )
    })
}

fn map_task_row(row: &Row<'_>) -> Result<ProjectScheduledTask, AppError> {
    let args_json = row.get::<_, String>("args_json")?;
    let args = from_str::<Vec<String>>(&args_json).map_err(|error| {
        AppError::with_details(
            "INVALID_SCHEDULED_TASK_ARGS",
            "Stored scheduled task arguments are invalid.",
            error.to_string(),
        )
    })?;

    Ok(ProjectScheduledTask {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        name: row.get("name")?,
        task_type: parse_task_type(&row.get::<_, String>("task_type")?)?,
        schedule_mode: parse_schedule_mode(&row.get::<_, String>("schedule_mode")?)?,
        simple_schedule_kind: parse_simple_schedule_kind(row.get("simple_schedule_kind")?)?,
        schedule_expression: row.get("schedule_expression")?,
        interval_seconds: row.get("interval_seconds")?,
        daily_time: row.get("daily_time")?,
        weekly_day: row.get("weekly_day")?,
        url: row.get("url")?,
        command: row.get("command")?,
        args,
        working_directory: row.get("working_directory")?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        auto_resume: row.get::<_, i64>("auto_resume")? == 1,
        overlap_policy: parse_overlap_policy(&row.get::<_, String>("overlap_policy")?)?,
        status: parse_task_status(&row.get::<_, String>("status")?)?,
        next_run_at: row.get("next_run_at")?,
        last_run_at: row.get("last_run_at")?,
        last_success_at: row.get("last_success_at")?,
        last_error: row.get("last_error")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_run_row(row: &Row<'_>) -> Result<ProjectScheduledTaskRun, AppError> {
    Ok(ProjectScheduledTaskRun {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        project_id: row.get("project_id")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        duration_ms: row.get("duration_ms")?,
        status: parse_run_status(&row.get::<_, String>("status")?)?,
        exit_code: row.get("exit_code")?,
        response_status: row.get("response_status")?,
        error_message: row.get("error_message")?,
        log_path: row.get("log_path")?,
        created_at: row.get("created_at")?,
    })
}

fn validate_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || trimmed.len() > 80 {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_NAME",
            "Scheduled task name must be between 2 and 80 characters.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_working_directory(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let path = Path::new(trimmed);

    if trimmed.is_empty() || !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_DIRECTORY",
            "Working directory does not exist or is not a directory.",
        ));
    }

    Ok(trimmed.to_string())
}

fn parse_command_line(command_line: &str) -> Result<(String, Vec<String>), AppError> {
    let parts = split_command_args(command_line.trim());
    let Some((command, args)) = parts.split_first() else {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_COMMAND",
            "Scheduled task command line is required.",
        ));
    };

    Ok((command.to_string(), args.to_vec()))
}

fn validate_url(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let url = Url::parse(trimmed).map_err(|error| {
        AppError::with_details(
            "INVALID_SCHEDULED_TASK_URL",
            "Scheduled task URL is invalid.",
            error.to_string(),
        )
    })?;

    match url.scheme() {
        "http" | "https" => Ok(trimmed.to_string()),
        _ => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_URL",
            "Scheduled task URL must use http or https.",
        )),
    }
}

fn validate_daily_time(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let Some((hours, minutes)) = trimmed.split_once(':') else {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_DAILY_TIME",
            "Daily time must use HH:MM format.",
        ));
    };

    let hours = hours.parse::<u32>().ok();
    let minutes = minutes.parse::<u32>().ok();
    if !matches!(hours, Some(0..=23)) || !matches!(minutes, Some(0..=59)) {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_DAILY_TIME",
            "Daily time must stay within a valid 24-hour clock.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_weekly_day(value: i64) -> Result<i64, AppError> {
    if !(0..=6).contains(&value) {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_WEEKDAY",
            "Weekly schedule day must be between 0 and 6.",
        ));
    }

    Ok(value)
}

fn validate_interval_seconds(
    kind: &ProjectScheduledTaskSimpleScheduleKind,
    value: i64,
) -> Result<i64, AppError> {
    match kind {
        ProjectScheduledTaskSimpleScheduleKind::EverySeconds if value >= 5 => Ok(value),
        ProjectScheduledTaskSimpleScheduleKind::EveryMinutes if value >= 60 => Ok(value),
        ProjectScheduledTaskSimpleScheduleKind::EveryHours if value >= 3600 => Ok(value),
        ProjectScheduledTaskSimpleScheduleKind::EverySeconds => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_INTERVAL",
            "Every X seconds must use a minimum interval of 5 seconds.",
        )),
        ProjectScheduledTaskSimpleScheduleKind::EveryMinutes => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_INTERVAL",
            "Every X minutes must use a minimum interval of 60 seconds.",
        )),
        ProjectScheduledTaskSimpleScheduleKind::EveryHours => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_INTERVAL",
            "Every X hours must use a minimum interval of 3600 seconds.",
        )),
        ProjectScheduledTaskSimpleScheduleKind::Daily
        | ProjectScheduledTaskSimpleScheduleKind::Weekly => Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_INTERVAL",
            "Daily and weekly schedules do not use interval seconds.",
        )),
    }
}

fn validate_cron_expression(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_CRON",
            "Cron expression is required.",
        ));
    }

    let fields = trimmed.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(AppError::new_validation(
            "INVALID_SCHEDULED_TASK_CRON",
            "Cron expression must contain five fields.",
        ));
    }

    Ok(trimmed.to_string())
}

fn args_json(args: &[String]) -> Result<String, AppError> {
    to_string(args).map_err(|error| {
        AppError::with_details(
            "SCHEDULED_TASK_ARGS_SERIALIZE_FAILED",
            "Could not store the scheduled task arguments.",
            error.to_string(),
        )
    })
}

struct NormalizedTaskConfig {
    task_type: ProjectScheduledTaskType,
    command: Option<String>,
    args: Vec<String>,
    working_directory: Option<String>,
    url: Option<String>,
}

struct NormalizedSchedule {
    schedule_mode: ProjectScheduledTaskScheduleMode,
    simple_schedule_kind: Option<ProjectScheduledTaskSimpleScheduleKind>,
    schedule_expression: String,
    interval_seconds: Option<i64>,
    daily_time: Option<String>,
    weekly_day: Option<i64>,
}

fn normalize_task_config(
    project_path: &str,
    task_type: ProjectScheduledTaskType,
    command_line: Option<&str>,
    url: Option<&str>,
    working_directory: Option<&str>,
) -> Result<NormalizedTaskConfig, AppError> {
    match task_type {
        ProjectScheduledTaskType::Command => {
            let command_line = command_line
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::new_validation(
                        "INVALID_SCHEDULED_TASK_COMMAND",
                        "Command tasks require a command line.",
                    )
                })?;
            let (command, args) = parse_command_line(command_line)?;
            let working_directory = validate_working_directory(
                working_directory
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(project_path),
            )?;

            Ok(NormalizedTaskConfig {
                task_type,
                command: Some(command),
                args,
                working_directory: Some(working_directory),
                url: None,
            })
        }
        ProjectScheduledTaskType::Url => {
            let url = url
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::new_validation(
                        "INVALID_SCHEDULED_TASK_URL",
                        "URL tasks require a target URL.",
                    )
                })?;

            Ok(NormalizedTaskConfig {
                task_type,
                command: None,
                args: Vec::new(),
                working_directory: None,
                url: Some(validate_url(url)?),
            })
        }
    }
}

fn normalize_schedule(
    schedule_mode: ProjectScheduledTaskScheduleMode,
    simple_schedule_kind: Option<ProjectScheduledTaskSimpleScheduleKind>,
    schedule_expression: Option<&str>,
    interval_seconds: Option<i64>,
    daily_time: Option<&str>,
    weekly_day: Option<i64>,
) -> Result<NormalizedSchedule, AppError> {
    match schedule_mode {
        ProjectScheduledTaskScheduleMode::Cron => Ok(NormalizedSchedule {
            schedule_mode,
            simple_schedule_kind: None,
            schedule_expression: validate_cron_expression(schedule_expression.unwrap_or_default())?,
            interval_seconds: None,
            daily_time: None,
            weekly_day: None,
        }),
        ProjectScheduledTaskScheduleMode::Simple => {
            let kind = simple_schedule_kind.ok_or_else(|| {
                AppError::new_validation(
                    "INVALID_SIMPLE_SCHEDULE_KIND",
                    "Simple schedules require a schedule kind.",
                )
            })?;

            match kind {
                ProjectScheduledTaskSimpleScheduleKind::EverySeconds
                | ProjectScheduledTaskSimpleScheduleKind::EveryMinutes
                | ProjectScheduledTaskSimpleScheduleKind::EveryHours => {
                    let interval_seconds = interval_seconds.ok_or_else(|| {
                        AppError::new_validation(
                            "INVALID_SCHEDULED_TASK_INTERVAL",
                            "Interval seconds are required for this simple schedule.",
                        )
                    })?;
                    let interval_seconds = validate_interval_seconds(&kind, interval_seconds)?;
                    let schedule_expression = match &kind {
                        ProjectScheduledTaskSimpleScheduleKind::EverySeconds => {
                            format!("Every {interval_seconds} seconds")
                        }
                        ProjectScheduledTaskSimpleScheduleKind::EveryMinutes => {
                            format!("Every {} minutes", interval_seconds / 60)
                        }
                        ProjectScheduledTaskSimpleScheduleKind::EveryHours => {
                            format!("Every {} hours", interval_seconds / 3600)
                        }
                        ProjectScheduledTaskSimpleScheduleKind::Daily
                        | ProjectScheduledTaskSimpleScheduleKind::Weekly => unreachable!(),
                    };
                    Ok(NormalizedSchedule {
                        schedule_mode,
                        simple_schedule_kind: Some(kind),
                        schedule_expression,
                        interval_seconds: Some(interval_seconds),
                        daily_time: None,
                        weekly_day: None,
                    })
                }
                ProjectScheduledTaskSimpleScheduleKind::Daily => {
                    let daily_time = validate_daily_time(daily_time.unwrap_or_default())?;
                    Ok(NormalizedSchedule {
                        schedule_mode,
                        simple_schedule_kind: Some(kind),
                        schedule_expression: format!("Daily at {daily_time}"),
                        interval_seconds: None,
                        daily_time: Some(daily_time),
                        weekly_day: None,
                    })
                }
                ProjectScheduledTaskSimpleScheduleKind::Weekly => {
                    let daily_time = validate_daily_time(daily_time.unwrap_or_default())?;
                    let weekly_day = validate_weekly_day(weekly_day.unwrap_or(-1))?;
                    Ok(NormalizedSchedule {
                        schedule_mode,
                        simple_schedule_kind: Some(kind),
                        schedule_expression: format!("Weekly on {weekly_day} at {daily_time}"),
                        interval_seconds: None,
                        daily_time: Some(daily_time),
                        weekly_day: Some(weekly_day),
                    })
                }
            }
        }
    }
}

pub struct ProjectScheduledTaskRepository;

impl ProjectScheduledTaskRepository {
    pub fn list_by_project(
        connection: &Connection,
        project_id: &str,
    ) -> Result<Vec<ProjectScheduledTask>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_scheduled_tasks
            WHERE project_id = ?1
            ORDER BY updated_at DESC, created_at DESC
            ",
        )?;
        let rows = statement.query_map([project_id], |row| {
            map_task_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut tasks = Vec::new();

        for row in rows {
            match row {
                Ok(task) => tasks.push(task),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(tasks)
    }

    pub fn list_all(connection: &Connection) -> Result<Vec<ProjectScheduledTask>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_scheduled_tasks
            ORDER BY updated_at DESC, created_at DESC
            ",
        )?;
        let rows = statement.query_map([], |row| {
            map_task_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut tasks = Vec::new();

        for row in rows {
            match row {
                Ok(task) => tasks.push(task),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(tasks)
    }

    pub fn get(connection: &Connection, task_id: &str) -> Result<ProjectScheduledTask, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_scheduled_tasks
            WHERE id = ?1
            ",
        )?;

        let task = statement
            .query_row([task_id], |row| {
                map_task_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        task.ok_or_else(|| {
            AppError::new_validation("SCHEDULED_TASK_NOT_FOUND", "Scheduled task not found.")
        })
    }

    pub fn create(
        connection: &Connection,
        task_id: &str,
        input: CreateProjectScheduledTaskInput,
    ) -> Result<ProjectScheduledTask, AppError> {
        let project = ProjectRepository::get(connection, &input.project_id)?;
        let task_id = if task_id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            task_id.to_string()
        };
        let name = validate_name(&input.name)?;
        let normalized_config = normalize_task_config(
            &project.path,
            input.task_type,
            input.command_line.as_deref(),
            input.url.as_deref(),
            input.working_directory.as_deref(),
        )?;
        let normalized_schedule = normalize_schedule(
            input.schedule_mode,
            input.simple_schedule_kind,
            input.schedule_expression.as_deref(),
            input.interval_seconds,
            input.daily_time.as_deref(),
            input.weekly_day,
        )?;
        let timestamp = now_iso()?;

        connection.execute(
            "
            INSERT INTO project_scheduled_tasks (
                id,
                project_id,
                name,
                task_type,
                schedule_mode,
                simple_schedule_kind,
                schedule_expression,
                interval_seconds,
                daily_time,
                weekly_day,
                url,
                command,
                args_json,
                working_directory,
                enabled,
                auto_resume,
                overlap_policy,
                status,
                next_run_at,
                last_run_at,
                last_success_at,
                last_error,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'skip_if_running', 'idle', NULL, NULL, NULL, NULL, ?17, ?17)
            ",
            params![
                task_id,
                project.id,
                name,
                normalized_config.task_type.as_str(),
                normalized_schedule.schedule_mode.as_str(),
                normalized_schedule
                    .simple_schedule_kind
                    .as_ref()
                    .map(ProjectScheduledTaskSimpleScheduleKind::as_str),
                normalized_schedule.schedule_expression,
                normalized_schedule.interval_seconds,
                normalized_schedule.daily_time,
                normalized_schedule.weekly_day,
                normalized_config.url,
                normalized_config.command,
                args_json(&normalized_config.args)?,
                normalized_config.working_directory,
                if input.enabled { 1 } else { 0 },
                if input.auto_resume { 1 } else { 0 },
                timestamp,
            ],
        )?;

        Self::get(connection, &task_id)
    }

    pub fn update(
        connection: &Connection,
        task_id: &str,
        patch: UpdateProjectScheduledTaskPatch,
    ) -> Result<ProjectScheduledTask, AppError> {
        let current = Self::get(connection, task_id)?;
        let project = ProjectRepository::get(connection, &current.project_id)?;
        let timestamp = now_iso()?;

        let name = match patch.name {
            Some(value) => validate_name(&value)?,
            None => current.name,
        };
        let task_type = patch.task_type.unwrap_or(current.task_type.clone());
        let command_line = match patch.command_line {
            Some(Some(value)) => Some(value),
            Some(None) => None,
            None => current
                .command
                .as_ref()
                .map(|command| join_command_args(command, &current.args)),
        };
        let normalized_config = normalize_task_config(
            &project.path,
            task_type,
            command_line.as_deref(),
            patch
                .url
                .as_ref()
                .map(|value| value.as_deref().unwrap_or_default())
                .or(current.url.as_deref()),
            patch
                .working_directory
                .as_ref()
                .map(|value| value.as_deref().unwrap_or_default())
                .or(current.working_directory.as_deref()),
        )?;
        let schedule_mode = patch.schedule_mode.unwrap_or(current.schedule_mode.clone());
        let simple_schedule_kind = match patch.simple_schedule_kind {
            Some(value) => value,
            None => current.simple_schedule_kind.clone(),
        };
        let schedule_expression = match patch.schedule_expression {
            Some(Some(value)) => Some(value),
            Some(None) => None,
            None => Some(current.schedule_expression.clone()),
        };
        let interval_seconds = patch.interval_seconds.unwrap_or(current.interval_seconds);
        let daily_time = match patch.daily_time {
            Some(value) => value,
            None => current.daily_time.clone(),
        };
        let weekly_day = patch.weekly_day.unwrap_or(current.weekly_day);
        let normalized_schedule = normalize_schedule(
            schedule_mode,
            simple_schedule_kind,
            schedule_expression.as_deref(),
            interval_seconds,
            daily_time.as_deref(),
            weekly_day,
        )?;
        let enabled = patch.enabled.unwrap_or(current.enabled);
        let auto_resume = patch.auto_resume.unwrap_or(current.auto_resume);

        connection.execute(
            "
            UPDATE project_scheduled_tasks
            SET
                name = ?2,
                task_type = ?3,
                schedule_mode = ?4,
                simple_schedule_kind = ?5,
                schedule_expression = ?6,
                interval_seconds = ?7,
                daily_time = ?8,
                weekly_day = ?9,
                url = ?10,
                command = ?11,
                args_json = ?12,
                working_directory = ?13,
                enabled = ?14,
                auto_resume = ?15,
                updated_at = ?16
            WHERE id = ?1
            ",
            params![
                task_id,
                name,
                normalized_config.task_type.as_str(),
                normalized_schedule.schedule_mode.as_str(),
                normalized_schedule
                    .simple_schedule_kind
                    .as_ref()
                    .map(ProjectScheduledTaskSimpleScheduleKind::as_str),
                normalized_schedule.schedule_expression,
                normalized_schedule.interval_seconds,
                normalized_schedule.daily_time,
                normalized_schedule.weekly_day,
                normalized_config.url,
                normalized_config.command,
                args_json(&normalized_config.args)?,
                normalized_config.working_directory,
                if enabled { 1 } else { 0 },
                if auto_resume { 1 } else { 0 },
                timestamp,
            ],
        )?;

        Self::get(connection, task_id)
    }

    pub fn update_runtime_state(
        connection: &Connection,
        task_id: &str,
        status: &ProjectScheduledTaskStatus,
        next_run_at: Option<&str>,
        last_run_at: Option<&str>,
        last_success_at: Option<&str>,
        last_error: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<ProjectScheduledTask, AppError> {
        let timestamp = now_iso()?;
        let current = Self::get(connection, task_id)?;

        connection.execute(
            "
            UPDATE project_scheduled_tasks
            SET
                status = ?2,
                next_run_at = ?3,
                last_run_at = ?4,
                last_success_at = ?5,
                last_error = ?6,
                enabled = ?7,
                updated_at = ?8
            WHERE id = ?1
            ",
            params![
                task_id,
                status.as_str(),
                next_run_at,
                last_run_at,
                last_success_at,
                last_error,
                if enabled.unwrap_or(current.enabled) {
                    1
                } else {
                    0
                },
                timestamp,
            ],
        )?;

        Self::get(connection, task_id)
    }

    pub fn delete(connection: &Connection, task_id: &str) -> Result<bool, AppError> {
        Ok(connection.execute(
            "
            DELETE FROM project_scheduled_tasks
            WHERE id = ?1
            ",
            [task_id],
        )? > 0)
    }

    pub fn delete_by_project(connection: &Connection, project_id: &str) -> Result<usize, AppError> {
        Ok(connection.execute(
            "
            DELETE FROM project_scheduled_tasks
            WHERE project_id = ?1
            ",
            [project_id],
        )?)
    }
}

pub struct ProjectScheduledTaskRunRepository;

impl ProjectScheduledTaskRunRepository {
    pub fn create(
        connection: &Connection,
        run_id: &str,
        task_id: &str,
        project_id: &str,
        started_at: &str,
        log_path: &str,
    ) -> Result<ProjectScheduledTaskRun, AppError> {
        let run_id = if run_id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            run_id.to_string()
        };

        connection.execute(
            "
            INSERT INTO project_scheduled_task_runs (
                id,
                task_id,
                project_id,
                started_at,
                finished_at,
                duration_ms,
                status,
                exit_code,
                response_status,
                error_message,
                log_path,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, NULL, NULL, 'running', NULL, NULL, NULL, ?5, ?4)
            ",
            params![run_id, task_id, project_id, started_at, log_path],
        )?;

        Self::get(connection, &run_id)
    }

    pub fn get(connection: &Connection, run_id: &str) -> Result<ProjectScheduledTaskRun, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_scheduled_task_runs
            WHERE id = ?1
            ",
        )?;

        let run = statement
            .query_row([run_id], |row| {
                map_run_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        run.ok_or_else(|| {
            AppError::new_validation(
                "SCHEDULED_TASK_RUN_NOT_FOUND",
                "Scheduled task run not found.",
            )
        })
    }

    pub fn list_by_task(
        connection: &Connection,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ProjectScheduledTaskRun>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_scheduled_task_runs
            WHERE task_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(params![task_id, limit as i64], |row| {
            map_run_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut runs = Vec::new();

        for row in rows {
            match row {
                Ok(run) => runs.push(run),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(runs)
    }

    pub fn delete_by_task(connection: &Connection, task_id: &str) -> Result<usize, AppError> {
        Ok(connection.execute(
            "
            DELETE FROM project_scheduled_task_runs
            WHERE task_id = ?1
            ",
            [task_id],
        )?)
    }

    pub fn update_result(
        connection: &Connection,
        run_id: &str,
        finished_at: &str,
        duration_ms: i64,
        status: &ProjectScheduledTaskRunStatus,
        exit_code: Option<i64>,
        response_status: Option<i64>,
        error_message: Option<&str>,
    ) -> Result<ProjectScheduledTaskRun, AppError> {
        connection.execute(
            "
            UPDATE project_scheduled_task_runs
            SET
                finished_at = ?2,
                duration_ms = ?3,
                status = ?4,
                exit_code = ?5,
                response_status = ?6,
                error_message = ?7
            WHERE id = ?1
            ",
            params![
                run_id,
                finished_at,
                duration_ms,
                status.as_str(),
                exit_code,
                response_status,
                error_message,
            ],
        )?;

        Self::get(connection, run_id)
    }
}
