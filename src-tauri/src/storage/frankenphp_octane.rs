use crate::error::AppError;
use crate::models::frankenphp_octane::{
    FrankenphpOctaneWorkerSettings, FrankenphpOctaneWorkerStatus,
    UpdateFrankenphpOctaneWorkerSettingsInput,
};
use crate::storage::repositories::now_iso;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::Path;

const WORKER_PORT_START: i64 = 8100;
const WORKER_PORT_END: i64 = 8199;
const ADMIN_PORT_START: i64 = 9100;
const ADMIN_PORT_END: i64 = 9199;

fn parse_status(value: &str) -> Result<FrankenphpOctaneWorkerStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_FRANKENPHP_WORKER_STATUS",
            "Stored FrankenPHP worker status is invalid.",
        )
    })
}

fn map_row(row: &Row<'_>) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
    Ok(FrankenphpOctaneWorkerSettings {
        project_id: row.get("project_id")?,
        worker_port: row.get("worker_port")?,
        admin_port: row.get("admin_port")?,
        workers: row.get("workers")?,
        max_requests: row.get("max_requests")?,
        status: parse_status(&row.get::<_, String>("status")?)?,
        pid: row.get("pid")?,
        last_started_at: row.get("last_started_at")?,
        last_stopped_at: row.get("last_stopped_at")?,
        last_error: row.get("last_error")?,
        log_path: row.get("log_path")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn validate_port(
    value: i64,
    start: i64,
    end: i64,
    code: &str,
    label: &str,
) -> Result<i64, AppError> {
    if (start..=end).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::new_validation(
            code,
            format!("{label} must be between {start} and {end}."),
        ))
    }
}

fn validate_count(
    value: i64,
    min: i64,
    max: i64,
    code: &str,
    label: &str,
) -> Result<i64, AppError> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::new_validation(
            code,
            format!("{label} must be between {min} and {max}."),
        ))
    }
}

fn next_available_port(
    connection: &Connection,
    column: &str,
    start: i64,
    end: i64,
) -> Result<i64, AppError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {column} FROM project_frankenphp_octane_workers ORDER BY {column} ASC"
    ))?;
    let used = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    for port in start..=end {
        if !used.contains(&port) {
            return Ok(port);
        }
    }

    Err(AppError::new_validation(
        "FRANKENPHP_WORKER_PORTS_EXHAUSTED",
        "No available FrankenPHP Octane worker ports remain in the managed range.",
    ))
}

pub fn default_log_path(workspace_dir: &Path, project_id: &str) -> String {
    workspace_dir
        .join("runtime-logs")
        .join("frankenphp-octane")
        .join(format!("{project_id}.log"))
        .to_string_lossy()
        .to_string()
}

pub struct FrankenphpOctaneWorkerRepository;

impl FrankenphpOctaneWorkerRepository {
    pub fn list_all(
        connection: &Connection,
    ) -> Result<Vec<FrankenphpOctaneWorkerSettings>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_frankenphp_octane_workers
            ORDER BY updated_at DESC, created_at DESC
            ",
        )?;

        statement
            .query_map([], |row| {
                map_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub fn get(
        connection: &Connection,
        project_id: &str,
    ) -> Result<Option<FrankenphpOctaneWorkerSettings>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_frankenphp_octane_workers
            WHERE project_id = ?1
            ",
        )?;

        statement
            .query_row([project_id], |row| {
                map_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn get_or_create(
        connection: &Connection,
        workspace_dir: &Path,
        project_id: &str,
    ) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
        if let Some(settings) = Self::get(connection, project_id)? {
            return Ok(settings);
        }

        let timestamp = now_iso()?;
        let worker_port = next_available_port(
            connection,
            "worker_port",
            WORKER_PORT_START,
            WORKER_PORT_END,
        )?;
        let admin_port =
            next_available_port(connection, "admin_port", ADMIN_PORT_START, ADMIN_PORT_END)?;
        let log_path = default_log_path(workspace_dir, project_id);

        connection.execute(
            "
            INSERT INTO project_frankenphp_octane_workers (
              project_id, worker_port, admin_port, workers, max_requests, status, pid,
              last_started_at, last_stopped_at, last_error, log_path, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, 1, 500, 'stopped', NULL, NULL, NULL, NULL, ?4, ?5, ?5)
            ",
            params![project_id, worker_port, admin_port, log_path, timestamp],
        )?;

        Ok(Self::get(connection, project_id)?.expect("settings created above"))
    }

    pub fn update(
        connection: &Connection,
        workspace_dir: &Path,
        project_id: &str,
        input: UpdateFrankenphpOctaneWorkerSettingsInput,
    ) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
        let current = Self::get_or_create(connection, workspace_dir, project_id)?;
        let worker_port = match input.worker_port {
            Some(value) => validate_port(
                value,
                WORKER_PORT_START,
                WORKER_PORT_END,
                "INVALID_WORKER_PORT",
                "Worker port",
            )?,
            None => current.worker_port,
        };
        let admin_port = match input.admin_port {
            Some(value) => validate_port(
                value,
                ADMIN_PORT_START,
                ADMIN_PORT_END,
                "INVALID_ADMIN_PORT",
                "Admin port",
            )?,
            None => current.admin_port,
        };
        if worker_port == admin_port {
            return Err(AppError::new_validation(
                "INVALID_FRANKENPHP_WORKER_PORTS",
                "Worker and admin ports must be different.",
            ));
        }

        let workers = match input.workers {
            Some(value) => validate_count(value, 1, 16, "INVALID_WORKER_COUNT", "Worker count")?,
            None => current.workers,
        };
        let max_requests = match input.max_requests {
            Some(value) => {
                validate_count(value, 1, 100000, "INVALID_MAX_REQUESTS", "Max requests")?
            }
            None => current.max_requests,
        };
        let timestamp = now_iso()?;

        connection.execute(
            "
            UPDATE project_frankenphp_octane_workers
            SET worker_port = ?2,
                admin_port = ?3,
                workers = ?4,
                max_requests = ?5,
                updated_at = ?6
            WHERE project_id = ?1
            ",
            params![
                project_id,
                worker_port,
                admin_port,
                workers,
                max_requests,
                timestamp
            ],
        )?;

        Ok(Self::get(connection, project_id)?.expect("settings updated above"))
    }

    pub fn set_status(
        connection: &Connection,
        project_id: &str,
        status: &FrankenphpOctaneWorkerStatus,
        pid: Option<i64>,
        last_started_at: Option<&str>,
        last_stopped_at: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<FrankenphpOctaneWorkerSettings, AppError> {
        let timestamp = now_iso()?;
        connection.execute(
            "
            UPDATE project_frankenphp_octane_workers
            SET status = ?2,
                pid = ?3,
                last_started_at = COALESCE(?4, last_started_at),
                last_stopped_at = COALESCE(?5, last_stopped_at),
                last_error = ?6,
                updated_at = ?7
            WHERE project_id = ?1
            ",
            params![
                project_id,
                status.as_str(),
                pid,
                last_started_at,
                last_stopped_at,
                last_error,
                timestamp,
            ],
        )?;

        Ok(Self::get(connection, project_id)?.expect("settings row should exist"))
    }
}
