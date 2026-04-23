use crate::error::AppError;
use crate::models::worker::{
    CreateProjectWorkerInput, ProjectWorker, ProjectWorkerPresetType, ProjectWorkerStatus,
    UpdateProjectWorkerPatch,
};
use crate::storage::repositories::{ProjectRepository, now_iso};
use crate::utils::process::split_command_args;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{from_str, to_string};
use std::path::Path;
use uuid::Uuid;

fn parse_preset_type(value: &str) -> Result<ProjectWorkerPresetType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_WORKER_PRESET_TYPE",
            "Stored worker preset type is invalid.",
        )
    })
}

fn parse_status(value: &str) -> Result<ProjectWorkerStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation("INVALID_WORKER_STATUS", "Stored worker status is invalid.")
    })
}

fn map_worker_row(row: &Row<'_>) -> Result<ProjectWorker, AppError> {
    let args_json = row.get::<_, String>("args_json")?;
    let args = from_str::<Vec<String>>(&args_json).map_err(|error| {
        AppError::with_details(
            "INVALID_WORKER_ARGS",
            "Stored worker arguments are invalid.",
            error.to_string(),
        )
    })?;

    Ok(ProjectWorker {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        name: row.get("name")?,
        preset_type: parse_preset_type(&row.get::<_, String>("preset_type")?)?,
        command: row.get("command")?,
        args,
        working_directory: row.get("working_directory")?,
        auto_start: row.get::<_, i64>("auto_start")? == 1,
        status: parse_status(&row.get::<_, String>("status")?)?,
        pid: row.get("pid")?,
        last_started_at: row.get("last_started_at")?,
        last_stopped_at: row.get("last_stopped_at")?,
        last_exit_code: row.get("last_exit_code")?,
        last_error: row.get("last_error")?,
        log_path: row.get("log_path")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn validate_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || trimmed.len() > 80 {
        return Err(AppError::new_validation(
            "INVALID_WORKER_NAME",
            "Worker name must be between 2 and 80 characters.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_working_directory(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let path = Path::new(trimmed);

    if trimmed.is_empty() || !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_WORKER_DIRECTORY",
            "Working directory does not exist or is not a directory.",
        ));
    }

    Ok(trimmed.to_string())
}

fn parse_command_line(command_line: &str) -> Result<(String, Vec<String>), AppError> {
    let parts = split_command_args(command_line.trim());
    let Some((command, args)) = parts.split_first() else {
        return Err(AppError::new_validation(
            "INVALID_WORKER_COMMAND",
            "Worker command line is required.",
        ));
    };

    Ok((command.to_string(), args.to_vec()))
}

fn args_json(args: &[String]) -> Result<String, AppError> {
    to_string(args).map_err(|error| {
        AppError::with_details(
            "WORKER_ARGS_SERIALIZE_FAILED",
            "Could not store the worker arguments.",
            error.to_string(),
        )
    })
}

pub struct ProjectWorkerRepository;

impl ProjectWorkerRepository {
    pub fn list_by_project(
        connection: &Connection,
        project_id: &str,
    ) -> Result<Vec<ProjectWorker>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_workers
            WHERE project_id = ?1
            ORDER BY updated_at DESC, created_at DESC
            ",
        )?;

        let rows = statement.query_map([project_id], |row| {
            map_worker_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut workers = Vec::new();

        for row in rows {
            match row {
                Ok(worker) => workers.push(worker),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(workers)
    }

    pub fn list_all(connection: &Connection) -> Result<Vec<ProjectWorker>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_workers
            ORDER BY updated_at DESC, created_at DESC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            map_worker_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut workers = Vec::new();

        for row in rows {
            match row {
                Ok(worker) => workers.push(worker),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(workers)
    }

    pub fn get(connection: &Connection, worker_id: &str) -> Result<ProjectWorker, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_workers
            WHERE id = ?1
            ",
        )?;

        let worker = statement
            .query_row([worker_id], |row| {
                map_worker_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        worker.ok_or_else(|| AppError::new_validation("WORKER_NOT_FOUND", "Worker not found."))
    }

    pub fn create(
        connection: &Connection,
        worker_id: &str,
        input: CreateProjectWorkerInput,
        default_working_directory: &str,
        log_path: &str,
    ) -> Result<ProjectWorker, AppError> {
        let project = ProjectRepository::get(connection, &input.project_id)?;
        let name = validate_name(&input.name)?;
        let (command, args) = parse_command_line(&input.command_line)?;
        let working_directory = validate_working_directory(
            input
                .working_directory
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(default_working_directory),
        )?;
        let timestamp = now_iso()?;
        let worker_id = if worker_id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            worker_id.to_string()
        };

        connection.execute(
            "
            INSERT INTO project_workers (
                id,
                project_id,
                name,
                preset_type,
                command,
                args_json,
                working_directory,
                auto_start,
                status,
                pid,
                last_started_at,
                last_stopped_at,
                last_exit_code,
                last_error,
                log_path,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'stopped', NULL, NULL, NULL, NULL, NULL, ?9, ?10, ?10)
            ",
            params![
                worker_id,
                project.id,
                name,
                input.preset_type.as_str(),
                command,
                args_json(&args)?,
                working_directory,
                if input.auto_start { 1 } else { 0 },
                log_path,
                timestamp,
            ],
        )?;

        Self::get(connection, &worker_id)
    }

    pub fn update(
        connection: &Connection,
        worker_id: &str,
        patch: UpdateProjectWorkerPatch,
        default_working_directory: &str,
    ) -> Result<ProjectWorker, AppError> {
        let current = Self::get(connection, worker_id)?;
        let timestamp = now_iso()?;

        let name = match patch.name {
            Some(value) => validate_name(&value)?,
            None => current.name,
        };
        let preset_type = patch.preset_type.unwrap_or(current.preset_type);
        let (command, args) = match patch.command_line {
            Some(value) => parse_command_line(&value)?,
            None => (current.command, current.args),
        };
        let working_directory = match patch.working_directory {
            Some(Some(value)) => validate_working_directory(&value)?,
            Some(None) => validate_working_directory(default_working_directory)?,
            None => current.working_directory,
        };
        let auto_start = patch.auto_start.unwrap_or(current.auto_start);

        connection.execute(
            "
            UPDATE project_workers
            SET
                name = ?2,
                preset_type = ?3,
                command = ?4,
                args_json = ?5,
                working_directory = ?6,
                auto_start = ?7,
                updated_at = ?8
            WHERE id = ?1
            ",
            params![
                worker_id,
                name,
                preset_type.as_str(),
                command,
                args_json(&args)?,
                working_directory,
                if auto_start { 1 } else { 0 },
                timestamp,
            ],
        )?;

        Self::get(connection, worker_id)
    }

    pub fn set_status(
        connection: &Connection,
        worker_id: &str,
        status: &ProjectWorkerStatus,
        pid: Option<i64>,
        last_started_at: Option<&str>,
        last_stopped_at: Option<&str>,
        last_exit_code: Option<i64>,
        last_error: Option<&str>,
    ) -> Result<ProjectWorker, AppError> {
        let timestamp = now_iso()?;
        connection.execute(
            "
            UPDATE project_workers
            SET
                status = ?2,
                pid = ?3,
                last_started_at = COALESCE(?4, last_started_at),
                last_stopped_at = COALESCE(?5, last_stopped_at),
                last_exit_code = ?6,
                last_error = ?7,
                updated_at = ?8
            WHERE id = ?1
            ",
            params![
                worker_id,
                status.as_str(),
                pid,
                last_started_at,
                last_stopped_at,
                last_exit_code,
                last_error,
                timestamp,
            ],
        )?;

        Self::get(connection, worker_id)
    }

    pub fn delete(connection: &Connection, worker_id: &str) -> Result<bool, AppError> {
        Ok(connection.execute("DELETE FROM project_workers WHERE id = ?1", [worker_id])? > 0)
    }

    pub fn delete_by_project(connection: &Connection, project_id: &str) -> Result<usize, AppError> {
        Ok(connection.execute(
            "DELETE FROM project_workers WHERE project_id = ?1",
            [project_id],
        )?)
    }
}
