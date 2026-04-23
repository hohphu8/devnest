use crate::error::AppError;
use crate::storage::repositories::now_iso;
use crate::utils::paths::{managed_database_time_machine_dir, managed_database_time_machine_root};
use crate::utils::process::is_process_running;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

const TIME_MACHINE_FORMAT_VERSION: u32 = 2;
const SNAPSHOT_RETENTION_LIMIT: usize = 3;
const DEFAULT_SCHEDULE_INTERVAL_MINUTES: u32 = 5;
const CONFIG_FILE_NAME: &str = "time-machine.json";
const LOCK_FILE_NAME: &str = "operation.lock";
const SNAPSHOTS_DIR_NAME: &str = "snapshots";
const LOCK_STALE_AFTER_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseTimeMachineProtectionState {
    Off,
    Protected,
    Busy,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotTriggerSource {
    Manual,
    PreAction,
    Scheduled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTimeMachineStatus {
    pub name: String,
    pub enabled: bool,
    pub status: DatabaseTimeMachineProtectionState,
    pub snapshot_count: usize,
    pub latest_snapshot_at: Option<String>,
    pub schedule_enabled: bool,
    pub schedule_interval_minutes: u32,
    pub linked_project_action_snapshots_enabled: bool,
    pub next_scheduled_snapshot_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSnapshotSummary {
    pub id: String,
    pub database_name: String,
    pub created_at: String,
    pub trigger_source: SnapshotTriggerSource,
    pub size_bytes: u64,
    #[serde(default)]
    pub linked_project_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_interval_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSnapshotResult {
    pub success: bool,
    pub name: String,
    pub snapshot: DatabaseSnapshotSummary,
    pub status: DatabaseTimeMachineStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSnapshotRollbackResult {
    pub success: bool,
    pub name: String,
    pub snapshot_id: String,
    pub restored_at: String,
    pub restored_snapshot: DatabaseSnapshotSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseTimeMachineConfig {
    format_version: u32,
    enabled: bool,
    schedule_enabled: bool,
    schedule_interval_minutes: u32,
    linked_project_action_snapshots_enabled: bool,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotMetadata {
    format_version: u32,
    snapshot: DatabaseSnapshotSummary,
    dump_file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationLockDocument {
    operation: String,
    acquired_at: String,
    #[serde(default)]
    process_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SnapshotCaptureRequest {
    pub trigger_source: SnapshotTriggerSource,
    pub note: Option<String>,
    pub linked_project_names: Vec<String>,
    pub scheduled_interval_minutes: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PendingSnapshot {
    pub summary: DatabaseSnapshotSummary,
    pub dump_path: PathBuf,
    metadata_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SnapshotRestoreTarget {
    pub summary: DatabaseSnapshotSummary,
    pub dump_path: PathBuf,
}

#[derive(Debug)]
pub struct DatabaseTimeMachineOperationGuard {
    lock_path: PathBuf,
}

impl Drop for DatabaseTimeMachineOperationGuard {
    fn drop(&mut self) {
        fs::remove_file(&self.lock_path).ok();
    }
}

fn default_config() -> Result<DatabaseTimeMachineConfig, AppError> {
    Ok(DatabaseTimeMachineConfig {
        format_version: TIME_MACHINE_FORMAT_VERSION,
        enabled: false,
        schedule_enabled: true,
        schedule_interval_minutes: DEFAULT_SCHEDULE_INTERVAL_MINUTES,
        linked_project_action_snapshots_enabled: true,
        updated_at: now_iso()?,
    })
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, AppError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest found invalid Time Machine timestamp metadata.",
            error.to_string(),
        )
    })
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, AppError> {
    value.format(&Rfc3339).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest could not format Time Machine timestamp metadata.",
            error.to_string(),
        )
    })
}

fn config_path(workspace_dir: &Path, database_name: &str) -> PathBuf {
    managed_database_time_machine_dir(workspace_dir, database_name).join(CONFIG_FILE_NAME)
}

fn lock_path(workspace_dir: &Path, database_name: &str) -> PathBuf {
    managed_database_time_machine_dir(workspace_dir, database_name).join(LOCK_FILE_NAME)
}

fn snapshots_dir(workspace_dir: &Path, database_name: &str) -> PathBuf {
    managed_database_time_machine_dir(workspace_dir, database_name).join(SNAPSHOTS_DIR_NAME)
}

fn ensure_database_dir(workspace_dir: &Path, database_name: &str) -> Result<PathBuf, AppError> {
    let root = managed_database_time_machine_dir(workspace_dir, database_name);
    fs::create_dir_all(&root).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not prepare the Time Machine workspace for this database.",
            error.to_string(),
        )
    })?;
    Ok(root)
}

fn ensure_snapshots_dir(workspace_dir: &Path, database_name: &str) -> Result<PathBuf, AppError> {
    let dir = snapshots_dir(workspace_dir, database_name);
    fs::create_dir_all(&dir).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not prepare the snapshot storage folder.",
            error.to_string(),
        )
    })?;
    Ok(dir)
}

fn read_operation_lock(path: &Path) -> Result<Option<OperationLockDocument>, AppError> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest could not read the Time Machine lock state.",
            error.to_string(),
        )
    })?;
    let document = serde_json::from_str::<OperationLockDocument>(&raw).map_err(|error| {
        AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest found a corrupted Time Machine lock file.",
            error.to_string(),
        )
    })?;
    Ok(Some(document))
}

fn remove_operation_lock(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|error| {
        AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest could not clear the stale Time Machine lock state.",
            error.to_string(),
        )
    })
}

fn operation_lock_is_stale(document: &OperationLockDocument) -> Result<bool, AppError> {
    if let Some(process_id) = document.process_id {
        if !is_process_running(process_id)? {
            return Ok(true);
        }

        return Ok(false);
    }

    let acquired_at = parse_timestamp(&document.acquired_at)?;
    Ok(
        acquired_at + time::Duration::minutes(LOCK_STALE_AFTER_MINUTES)
            <= OffsetDateTime::now_utc(),
    )
}

fn operation_lock_details(document: &OperationLockDocument) -> String {
    format!(
        "operation={}, acquiredAt={}, processId={}",
        document.operation,
        document.acquired_at,
        document
            .process_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )
}

fn ensure_lock_is_current(path: &Path) -> Result<Option<OperationLockDocument>, AppError> {
    let Some(document) = read_operation_lock(path)? else {
        return Ok(None);
    };

    match operation_lock_is_stale(&document) {
        Ok(true) => {
            remove_operation_lock(path)?;
            Ok(None)
        }
        Ok(false) => Ok(Some(document)),
        Err(_) => {
            remove_operation_lock(path)?;
            Ok(None)
        }
    }
}

fn read_config(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<DatabaseTimeMachineConfig, AppError> {
    let path = config_path(workspace_dir, database_name);
    if !path.exists() {
        return default_config();
    }

    let raw = fs::read_to_string(&path).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest could not read the Time Machine configuration for this database.",
            error.to_string(),
        )
    })?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "The Time Machine configuration for this database is corrupted.",
            error.to_string(),
        )
    })?;

    let format_version = value
        .get("formatVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as u32;
    if format_version > TIME_MACHINE_FORMAT_VERSION {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "This database uses an unsupported Time Machine metadata format.",
        ));
    }

    let defaults = default_config()?;
    Ok(DatabaseTimeMachineConfig {
        format_version: TIME_MACHINE_FORMAT_VERSION,
        enabled: value
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.enabled),
        schedule_enabled: value
            .get("scheduleEnabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.schedule_enabled),
        schedule_interval_minutes: value
            .get("scheduleIntervalMinutes")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as u32)
            .filter(|value| *value > 0)
            .unwrap_or(defaults.schedule_interval_minutes),
        linked_project_action_snapshots_enabled: value
            .get("linkedProjectActionSnapshotsEnabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.linked_project_action_snapshots_enabled),
        updated_at: value
            .get("updatedAt")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or(defaults.updated_at),
    })
}

fn write_config(
    workspace_dir: &Path,
    database_name: &str,
    enabled: bool,
) -> Result<DatabaseTimeMachineConfig, AppError> {
    ensure_database_dir(workspace_dir, database_name)?;
    let previous = read_config(workspace_dir, database_name).unwrap_or(default_config()?);
    let config = DatabaseTimeMachineConfig {
        format_version: TIME_MACHINE_FORMAT_VERSION,
        enabled,
        schedule_enabled: previous.schedule_enabled,
        schedule_interval_minutes: previous.schedule_interval_minutes,
        linked_project_action_snapshots_enabled: previous.linked_project_action_snapshots_enabled,
        updated_at: now_iso()?,
    };
    let payload = serde_json::to_string_pretty(&config).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not serialize the Time Machine configuration.",
            error.to_string(),
        )
    })?;
    fs::write(config_path(workspace_dir, database_name), payload).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not save the Time Machine configuration for this database.",
            error.to_string(),
        )
    })?;
    Ok(config)
}

fn validate_snapshot_id(snapshot_id: &str) -> Result<String, AppError> {
    let trimmed = snapshot_id.trim();
    if trimmed.is_empty()
        || !trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_NOT_FOUND",
            "The requested snapshot identifier is invalid.",
        ));
    }

    Ok(trimmed.to_string())
}

fn parse_snapshot_metadata(path: &Path) -> Result<SnapshotMetadata, AppError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest could not read a stored database snapshot.",
            error.to_string(),
        )
    })?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "A stored database snapshot metadata file is corrupted.",
            error.to_string(),
        )
    })?;

    let format_version = value
        .get("formatVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as u32;
    if format_version > TIME_MACHINE_FORMAT_VERSION {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "A stored database snapshot uses an unsupported metadata format.",
        ));
    }

    let metadata: SnapshotMetadata = serde_json::from_value(value).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "A stored database snapshot metadata file is corrupted.",
            error.to_string(),
        )
    })?;
    parse_timestamp(&metadata.snapshot.created_at)?;
    Ok(metadata)
}

fn metadata_dump_path(
    workspace_dir: &Path,
    database_name: &str,
    metadata: &SnapshotMetadata,
) -> PathBuf {
    snapshots_dir(workspace_dir, database_name).join(&metadata.dump_file_name)
}

fn list_snapshot_metadata(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<Vec<SnapshotMetadata>, AppError> {
    let snapshots_root = snapshots_dir(workspace_dir, database_name);
    if !snapshots_root.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in fs::read_dir(&snapshots_root).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest could not inspect the database snapshot folder.",
            error.to_string(),
        )
    })? {
        let entry = entry.map_err(|error| {
            AppError::with_details(
                "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
                "DevNest could not inspect a database snapshot file.",
                error.to_string(),
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let metadata = parse_snapshot_metadata(&path)?;
        let dump_path = metadata_dump_path(workspace_dir, database_name, &metadata);
        if !dump_path.exists() {
            return Err(AppError::new_validation(
                "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
                "A managed database snapshot is missing its SQL dump file.",
            ));
        }

        snapshots.push(metadata);
    }

    snapshots.sort_by(|left, right| {
        right
            .snapshot
            .created_at
            .cmp(&left.snapshot.created_at)
            .then_with(|| right.snapshot.id.cmp(&left.snapshot.id))
    });

    Ok(snapshots)
}

fn list_snapshot_summaries(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<Vec<DatabaseSnapshotSummary>, AppError> {
    Ok(list_snapshot_metadata(workspace_dir, database_name)?
        .into_iter()
        .map(|metadata| metadata.snapshot)
        .collect())
}

fn next_scheduled_snapshot_at(
    config: &DatabaseTimeMachineConfig,
    latest_snapshot_at: Option<&str>,
) -> Result<Option<String>, AppError> {
    if !config.enabled || !config.schedule_enabled {
        return Ok(None);
    }

    let base = latest_snapshot_at.unwrap_or(&config.updated_at);
    let interval = time::Duration::minutes(i64::from(config.schedule_interval_minutes));
    let base_timestamp = parse_timestamp(base)?;
    let now = OffsetDateTime::now_utc();
    let mut scheduled_at = base_timestamp + interval;

    if scheduled_at <= now {
        let interval_seconds = interval.whole_seconds();
        let elapsed_seconds = (now - base_timestamp).whole_seconds();
        let completed_intervals = elapsed_seconds.div_euclid(interval_seconds);
        scheduled_at =
            base_timestamp + time::Duration::seconds(interval_seconds * (completed_intervals + 1));
    }

    Ok(Some(format_timestamp(scheduled_at)?))
}

pub fn list_managed_databases(workspace_dir: &Path) -> Result<Vec<String>, AppError> {
    let root = managed_database_time_machine_root(workspace_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut databases = Vec::new();
    for entry in fs::read_dir(&root).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "DevNest could not inspect the Time Machine workspace root.",
            error.to_string(),
        )
    })? {
        let entry = entry.map_err(|error| {
            AppError::with_details(
                "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
                "DevNest could not inspect a Time Machine workspace entry.",
                error.to_string(),
            )
        })?;
        if !entry.path().is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        databases.push(name.to_string());
    }

    databases.sort();
    Ok(databases)
}

pub fn inspect_status(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    let root = managed_database_time_machine_dir(workspace_dir, database_name);
    let busy = ensure_lock_is_current(&lock_path(workspace_dir, database_name))?.is_some();
    let config = read_config(workspace_dir, database_name)?;
    if !root.exists() {
        return Ok(DatabaseTimeMachineStatus {
            name: database_name.to_string(),
            enabled: false,
            status: if busy {
                DatabaseTimeMachineProtectionState::Busy
            } else {
                DatabaseTimeMachineProtectionState::Off
            },
            snapshot_count: 0,
            latest_snapshot_at: None,
            schedule_enabled: config.schedule_enabled,
            schedule_interval_minutes: config.schedule_interval_minutes,
            linked_project_action_snapshots_enabled: config.linked_project_action_snapshots_enabled,
            next_scheduled_snapshot_at: None,
            last_error: None,
        });
    }

    match list_snapshot_summaries(workspace_dir, database_name) {
        Ok(snapshots) => {
            let latest_snapshot_at = snapshots
                .first()
                .map(|snapshot| snapshot.created_at.clone());
            Ok(DatabaseTimeMachineStatus {
                name: database_name.to_string(),
                enabled: config.enabled,
                status: if busy {
                    DatabaseTimeMachineProtectionState::Busy
                } else if config.enabled {
                    DatabaseTimeMachineProtectionState::Protected
                } else {
                    DatabaseTimeMachineProtectionState::Off
                },
                snapshot_count: snapshots.len(),
                latest_snapshot_at: latest_snapshot_at.clone(),
                schedule_enabled: config.schedule_enabled,
                schedule_interval_minutes: config.schedule_interval_minutes,
                linked_project_action_snapshots_enabled: config
                    .linked_project_action_snapshots_enabled,
                next_scheduled_snapshot_at: next_scheduled_snapshot_at(
                    &config,
                    latest_snapshot_at.as_deref(),
                )?,
                last_error: None,
            })
        }
        Err(error) => Ok(DatabaseTimeMachineStatus {
            name: database_name.to_string(),
            enabled: config.enabled,
            status: DatabaseTimeMachineProtectionState::Error,
            snapshot_count: 0,
            latest_snapshot_at: None,
            schedule_enabled: config.schedule_enabled,
            schedule_interval_minutes: config.schedule_interval_minutes,
            linked_project_action_snapshots_enabled: config.linked_project_action_snapshots_enabled,
            next_scheduled_snapshot_at: None,
            last_error: Some(error.message),
        }),
    }
}

pub fn enable(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    write_config(workspace_dir, database_name, true)?;
    inspect_status(workspace_dir, database_name)
}

pub fn disable(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<DatabaseTimeMachineStatus, AppError> {
    write_config(workspace_dir, database_name, false)?;
    inspect_status(workspace_dir, database_name)
}

pub fn list_snapshots(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<Vec<DatabaseSnapshotSummary>, AppError> {
    list_snapshot_summaries(workspace_dir, database_name)
}

pub fn is_enabled(workspace_dir: &Path, database_name: &str) -> Result<bool, AppError> {
    Ok(read_config(workspace_dir, database_name)?.enabled)
}

pub fn linked_project_action_snapshots_enabled(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<bool, AppError> {
    let config = read_config(workspace_dir, database_name)?;
    Ok(config.enabled && config.linked_project_action_snapshots_enabled)
}

pub fn scheduled_snapshot_due(
    workspace_dir: &Path,
    database_name: &str,
) -> Result<Option<u32>, AppError> {
    let status = inspect_status(workspace_dir, database_name)?;
    if !status.enabled
        || !status.schedule_enabled
        || !matches!(status.status, DatabaseTimeMachineProtectionState::Protected)
    {
        return Ok(None);
    }

    let config = read_config(workspace_dir, database_name)?;
    let base = status
        .latest_snapshot_at
        .as_deref()
        .unwrap_or(&config.updated_at);
    let due_at = parse_timestamp(base)?
        + time::Duration::minutes(i64::from(status.schedule_interval_minutes));
    if due_at <= OffsetDateTime::now_utc() {
        return Ok(Some(status.schedule_interval_minutes));
    }

    Ok(None)
}

pub fn acquire_operation_lock(
    workspace_dir: &Path,
    database_name: &str,
    operation: &str,
) -> Result<DatabaseTimeMachineOperationGuard, AppError> {
    ensure_database_dir(workspace_dir, database_name)?;
    let path = lock_path(workspace_dir, database_name);
    if let Some(document) = ensure_lock_is_current(&path)? {
        return Err(AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest is already performing another Time Machine action for this database.",
            operation_lock_details(&document),
        ));
    }

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            AppError::with_details(
                "DATABASE_TIME_MACHINE_BUSY",
                "DevNest could not acquire the Time Machine lock for this database.",
                error.to_string(),
            )
        })?;
    let payload = serde_json::to_string_pretty(&OperationLockDocument {
        operation: operation.to_string(),
        acquired_at: now_iso()?,
        process_id: Some(std::process::id()),
    })
    .map_err(|error| {
        AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest could not serialize the Time Machine lock state.",
            error.to_string(),
        )
    })?;
    file.write_all(payload.as_bytes()).map_err(|error| {
        AppError::with_details(
            "DATABASE_TIME_MACHINE_BUSY",
            "DevNest could not persist the Time Machine lock state.",
            error.to_string(),
        )
    })?;

    Ok(DatabaseTimeMachineOperationGuard { lock_path: path })
}

pub fn begin_snapshot_capture(
    workspace_dir: &Path,
    database_name: &str,
    request: SnapshotCaptureRequest,
) -> Result<PendingSnapshot, AppError> {
    ensure_snapshots_dir(workspace_dir, database_name)?;
    let created_at = now_iso()?;
    let snapshot_id = format!(
        "{}-{}",
        created_at
            .replace(':', "")
            .replace('.', "-")
            .replace('T', "-")
            .replace('Z', "z"),
        Uuid::new_v4().simple()
    );
    let snapshots_root = snapshots_dir(workspace_dir, database_name);
    let dump_file_name = format!("{snapshot_id}.sql");
    Ok(PendingSnapshot {
        summary: DatabaseSnapshotSummary {
            id: snapshot_id.clone(),
            database_name: database_name.to_string(),
            created_at,
            trigger_source: request.trigger_source,
            size_bytes: 0,
            linked_project_names: request.linked_project_names,
            scheduled_interval_minutes: request.scheduled_interval_minutes,
            note: request.note,
        },
        dump_path: snapshots_root.join(&dump_file_name),
        metadata_path: snapshots_root.join(format!("{snapshot_id}.json")),
    })
}

pub fn abort_snapshot_capture(pending: &PendingSnapshot) {
    fs::remove_file(&pending.dump_path).ok();
    fs::remove_file(&pending.metadata_path).ok();
}

pub fn finalize_snapshot_capture(
    workspace_dir: &Path,
    database_name: &str,
    pending: PendingSnapshot,
) -> Result<DatabaseSnapshotSummary, AppError> {
    let mut snapshot = pending.summary.clone();
    let dump_metadata = fs::metadata(&pending.dump_path).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not verify the generated snapshot dump file.",
            error.to_string(),
        )
    })?;
    if dump_metadata.len() == 0 {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "The managed database snapshot dump file is empty.",
        ));
    }
    snapshot.size_bytes = dump_metadata.len();

    let dump_file_name = pending
        .dump_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| {
            AppError::new_validation(
                "DATABASE_SNAPSHOT_FAILED",
                "DevNest could not determine the snapshot dump file name.",
            )
        })?;
    let payload = serde_json::to_string_pretty(&SnapshotMetadata {
        format_version: TIME_MACHINE_FORMAT_VERSION,
        snapshot: snapshot.clone(),
        dump_file_name,
    })
    .map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not serialize the snapshot metadata.",
            error.to_string(),
        )
    })?;
    fs::write(&pending.metadata_path, payload).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_FAILED",
            "DevNest could not save the snapshot metadata.",
            error.to_string(),
        )
    })?;

    retain_latest_snapshots(workspace_dir, database_name)?;
    Ok(snapshot)
}

fn retain_latest_snapshots(workspace_dir: &Path, database_name: &str) -> Result<(), AppError> {
    let snapshots = list_snapshot_metadata(workspace_dir, database_name)?;
    if snapshots.len() <= SNAPSHOT_RETENTION_LIMIT {
        return Ok(());
    }

    for snapshot in snapshots.iter().skip(SNAPSHOT_RETENTION_LIMIT) {
        let dump_path = metadata_dump_path(workspace_dir, database_name, snapshot);
        let metadata_path = snapshots_dir(workspace_dir, database_name)
            .join(format!("{}.json", snapshot.snapshot.id));
        fs::remove_file(dump_path).ok();
        fs::remove_file(metadata_path).ok();
    }

    Ok(())
}

pub fn resolve_snapshot_for_restore(
    workspace_dir: &Path,
    database_name: &str,
    snapshot_id: &str,
) -> Result<SnapshotRestoreTarget, AppError> {
    let snapshot_id = validate_snapshot_id(snapshot_id)?;
    let metadata_path =
        snapshots_dir(workspace_dir, database_name).join(format!("{snapshot_id}.json"));
    if !metadata_path.exists() {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_NOT_FOUND",
            "The selected managed database snapshot does not exist anymore.",
        ));
    }
    let metadata = parse_snapshot_metadata(&metadata_path)?;
    if metadata.snapshot.id != snapshot_id {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "The selected snapshot metadata does not match the requested snapshot ID.",
        ));
    }
    let dump_path = metadata_dump_path(workspace_dir, database_name, &metadata);
    let dump_metadata = fs::metadata(&dump_path).map_err(|error| {
        AppError::with_details(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "The selected managed database snapshot is missing its SQL dump file.",
            error.to_string(),
        )
    })?;
    if dump_metadata.len() == 0 {
        return Err(AppError::new_validation(
            "DATABASE_SNAPSHOT_INTEGRITY_FAILED",
            "The selected managed database snapshot dump file is empty.",
        ));
    }

    Ok(SnapshotRestoreTarget {
        summary: metadata.snapshot,
        dump_path,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SCHEDULE_INTERVAL_MINUTES, SnapshotCaptureRequest, SnapshotTriggerSource, disable,
        enable, inspect_status, list_managed_databases, list_snapshots,
        resolve_snapshot_for_restore, scheduled_snapshot_due,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn setup_workspace() -> PathBuf {
        let root = std::env::temp_dir().join(format!("devnest-time-machine-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("workspace should exist");
        root
    }

    fn cleanup_workspace(path: &Path) {
        fs::remove_dir_all(path).ok();
    }

    fn write_snapshot(
        workspace_dir: &Path,
        database_name: &str,
        trigger_source: SnapshotTriggerSource,
        note: Option<&str>,
        linked_project_names: &[&str],
        sql: &str,
    ) {
        let pending = super::begin_snapshot_capture(
            workspace_dir,
            database_name,
            SnapshotCaptureRequest {
                trigger_source: trigger_source.clone(),
                note: note.map(ToOwned::to_owned),
                linked_project_names: linked_project_names
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                scheduled_interval_minutes: if matches!(
                    trigger_source,
                    SnapshotTriggerSource::Scheduled
                ) {
                    Some(DEFAULT_SCHEDULE_INTERVAL_MINUTES)
                } else {
                    None
                },
            },
        )
        .expect("pending snapshot should create");
        fs::write(&pending.dump_path, sql).expect("snapshot dump should write");
        super::finalize_snapshot_capture(workspace_dir, database_name, pending)
            .expect("snapshot should finalize");
    }

    #[test]
    fn enable_and_disable_time_machine_transitions() {
        let workspace = setup_workspace();

        let enabled = enable(&workspace, "shop_api").expect("time machine should enable");
        assert!(enabled.enabled);
        assert_eq!(
            enabled.status,
            super::DatabaseTimeMachineProtectionState::Protected
        );
        assert!(enabled.schedule_enabled);
        assert!(enabled.linked_project_action_snapshots_enabled);

        let disabled = disable(&workspace, "shop_api").expect("time machine should disable");
        assert!(!disabled.enabled);
        assert_eq!(
            disabled.status,
            super::DatabaseTimeMachineProtectionState::Off
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn retention_keeps_only_latest_three_snapshots() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("time machine should enable");

        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Manual,
            None,
            &[],
            "SELECT 1;",
        );
        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::PreAction,
            Some("before restore"),
            &["Shop API"],
            "SELECT 2;",
        );
        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Manual,
            None,
            &[],
            "SELECT 3;",
        );
        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Scheduled,
            Some("scheduled protection checkpoint"),
            &["Shop API"],
            "SELECT 4;",
        );

        let snapshots = list_snapshots(&workspace, "shop_api").expect("snapshots should list");
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots[0].created_at >= snapshots[1].created_at);
        assert!(snapshots[1].created_at >= snapshots[2].created_at);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn list_snapshots_returns_newest_first_with_richer_metadata() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("time machine should enable");

        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Manual,
            Some("first"),
            &["Shop API"],
            "SELECT 'first';",
        );
        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Scheduled,
            Some("scheduled protection checkpoint"),
            &["Shop API", "Admin Portal"],
            "SELECT 'second';",
        );

        let snapshots = list_snapshots(&workspace, "shop_api").expect("snapshots should list");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots[0].trigger_source,
            SnapshotTriggerSource::Scheduled
        );
        assert_eq!(snapshots[0].linked_project_names.len(), 2);
        assert_eq!(
            snapshots[0].scheduled_interval_minutes,
            Some(DEFAULT_SCHEDULE_INTERVAL_MINUTES)
        );
        assert_eq!(snapshots[1].note.as_deref(), Some("first"));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn resolve_snapshot_rejects_invalid_snapshot_id() {
        let workspace = setup_workspace();
        let error = resolve_snapshot_for_restore(&workspace, "shop_api", "../escape")
            .expect_err("snapshot id should be rejected");
        assert_eq!(error.code, "DATABASE_SNAPSHOT_NOT_FOUND");
        cleanup_workspace(&workspace);
    }

    #[test]
    fn resolve_snapshot_rejects_missing_or_empty_dump_files() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("time machine should enable");

        let pending = super::begin_snapshot_capture(
            &workspace,
            "shop_api",
            SnapshotCaptureRequest {
                trigger_source: SnapshotTriggerSource::Manual,
                note: None,
                linked_project_names: Vec::new(),
                scheduled_interval_minutes: None,
            },
        )
        .expect("pending snapshot should create");
        fs::write(&pending.dump_path, "").expect("empty dump should write");
        let error = super::finalize_snapshot_capture(&workspace, "shop_api", pending)
            .expect_err("empty dump should fail integrity");
        assert_eq!(error.code, "DATABASE_SNAPSHOT_INTEGRITY_FAILED");

        write_snapshot(
            &workspace,
            "shop_api",
            SnapshotTriggerSource::Manual,
            None,
            &[],
            "SELECT 1;",
        );
        let snapshot = list_snapshots(&workspace, "shop_api")
            .expect("snapshots should list")
            .into_iter()
            .next()
            .expect("snapshot should exist");
        let dump_path = super::resolve_snapshot_for_restore(&workspace, "shop_api", &snapshot.id)
            .expect("snapshot should resolve")
            .dump_path;
        fs::remove_file(dump_path).expect("dump should remove");

        let missing_error = resolve_snapshot_for_restore(&workspace, "shop_api", &snapshot.id)
            .expect_err("missing dump should fail");
        assert_eq!(missing_error.code, "DATABASE_SNAPSHOT_INTEGRITY_FAILED");

        cleanup_workspace(&workspace);
    }

    #[test]
    fn inspect_status_surfaces_corrupt_snapshot_state() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("time machine should enable");

        let snapshots_dir = super::snapshots_dir(&workspace, "shop_api");
        fs::create_dir_all(&snapshots_dir).expect("snapshots dir should exist");
        fs::write(snapshots_dir.join("broken.json"), "{not-json")
            .expect("broken metadata should write");

        let status = inspect_status(&workspace, "shop_api").expect("status should load");
        assert_eq!(
            status.status,
            super::DatabaseTimeMachineProtectionState::Error
        );
        assert!(status.last_error.is_some());

        cleanup_workspace(&workspace);
    }

    #[test]
    fn schedule_due_uses_latest_snapshot_timestamp() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("time machine should enable");

        let status = inspect_status(&workspace, "shop_api").expect("status should load");
        assert!(status.next_scheduled_snapshot_at.is_some());
        assert!(
            scheduled_snapshot_due(&workspace, "shop_api")
                .expect("due check should succeed")
                .is_none()
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn inspect_status_rolls_next_schedule_forward_after_missed_intervals() {
        let workspace = setup_workspace();
        let database_name = "shop_api";
        enable(&workspace, database_name).expect("time machine should enable");
        write_snapshot(
            &workspace,
            database_name,
            SnapshotTriggerSource::Scheduled,
            Some("scheduled protection checkpoint"),
            &[],
            "SELECT 1;",
        );

        let config_path = super::config_path(&workspace, database_name);
        let raw_config = fs::read_to_string(&config_path).expect("config should exist");
        let mut config =
            serde_json::from_str::<serde_json::Value>(&raw_config).expect("config should parse");
        config["updatedAt"] = serde_json::Value::String("2000-01-01T00:00:00Z".to_string());
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&config).expect("config should serialize"),
        )
        .expect("config should update");

        let snapshots_dir = super::snapshots_dir(&workspace, database_name);
        let snapshot_path = fs::read_dir(&snapshots_dir)
            .expect("snapshots should list")
            .find_map(|entry| {
                let path = entry.ok()?.path();
                (path.extension().and_then(|value| value.to_str()) == Some("json")).then_some(path)
            })
            .expect("snapshot metadata should exist");
        let raw_snapshot =
            fs::read_to_string(&snapshot_path).expect("snapshot metadata should read");
        let mut snapshot = serde_json::from_str::<serde_json::Value>(&raw_snapshot)
            .expect("snapshot metadata should parse");
        snapshot["snapshot"]["createdAt"] =
            serde_json::Value::String("2000-01-01T00:00:00Z".to_string());
        fs::write(
            &snapshot_path,
            serde_json::to_string_pretty(&snapshot).expect("snapshot should serialize"),
        )
        .expect("snapshot metadata should update");

        let status = inspect_status(&workspace, database_name).expect("status should load");
        let next_scheduled_at = status
            .next_scheduled_snapshot_at
            .as_deref()
            .expect("next scheduled timestamp should exist");
        let next_timestamp =
            super::parse_timestamp(next_scheduled_at).expect("next timestamp should parse");
        assert!(next_timestamp > OffsetDateTime::now_utc());
        assert!(
            scheduled_snapshot_due(&workspace, database_name)
                .expect("due check should succeed")
                .is_some()
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn stale_operation_lock_is_reclaimed() {
        let workspace = setup_workspace();
        let database_name = "shop_api";
        let database_dir = super::ensure_database_dir(&workspace, database_name)
            .expect("database dir should exist");
        let lock_path = database_dir.join(super::LOCK_FILE_NAME);
        fs::write(
            &lock_path,
            r#"{
  "operation": "scheduled-snapshot",
  "acquiredAt": "2000-01-01T00:00:00Z"
}"#,
        )
        .expect("stale lock should write");

        let _guard = super::acquire_operation_lock(&workspace, database_name, "snapshot")
            .expect("stale lock should be reclaimed");

        cleanup_workspace(&workspace);
    }

    #[test]
    fn active_operation_lock_stays_busy() {
        let workspace = setup_workspace();
        let database_name = "shop_api";
        let _guard = super::acquire_operation_lock(&workspace, database_name, "snapshot")
            .expect("first lock should succeed");

        let error = super::acquire_operation_lock(&workspace, database_name, "snapshot")
            .expect_err("second lock should fail");
        assert_eq!(error.code, "DATABASE_TIME_MACHINE_BUSY");

        cleanup_workspace(&workspace);
    }

    #[test]
    fn lists_managed_databases_from_workspace() {
        let workspace = setup_workspace();
        enable(&workspace, "shop_api").expect("shop_api should enable");
        enable(&workspace, "legacy-db").expect("legacy-db should enable");

        let databases = list_managed_databases(&workspace).expect("managed databases should list");
        assert_eq!(databases, vec!["legacy-db", "shop_api"]);

        cleanup_workspace(&workspace);
    }
}
