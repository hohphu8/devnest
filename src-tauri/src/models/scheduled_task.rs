use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectScheduledTaskType {
    Command,
    Url,
}

impl ProjectScheduledTaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Url => "url",
        }
    }
}

impl FromStr for ProjectScheduledTaskType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "command" => Ok(Self::Command),
            "url" => Ok(Self::Url),
            _ => Err("Invalid scheduled task type"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectScheduledTaskScheduleMode {
    Simple,
    Cron,
}

impl ProjectScheduledTaskScheduleMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Cron => "cron",
        }
    }
}

impl FromStr for ProjectScheduledTaskScheduleMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "simple" => Ok(Self::Simple),
            "cron" => Ok(Self::Cron),
            _ => Err("Invalid scheduled task schedule mode"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectScheduledTaskSimpleScheduleKind {
    EverySeconds,
    EveryMinutes,
    EveryHours,
    Daily,
    Weekly,
}

impl ProjectScheduledTaskSimpleScheduleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EverySeconds => "everySeconds",
            Self::EveryMinutes => "everyMinutes",
            Self::EveryHours => "everyHours",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }
}

impl FromStr for ProjectScheduledTaskSimpleScheduleKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "everySeconds" => Ok(Self::EverySeconds),
            "everyMinutes" => Ok(Self::EveryMinutes),
            "everyHours" => Ok(Self::EveryHours),
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            _ => Err("Invalid simple schedule kind"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectScheduledTaskOverlapPolicy {
    SkipIfRunning,
}

impl FromStr for ProjectScheduledTaskOverlapPolicy {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "skip_if_running" => Ok(Self::SkipIfRunning),
            _ => Err("Invalid overlap policy"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectScheduledTaskStatus {
    Idle,
    Scheduled,
    Running,
    Success,
    Error,
    Skipped,
}

impl ProjectScheduledTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Scheduled => "scheduled",
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

impl FromStr for ProjectScheduledTaskStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "idle" => Ok(Self::Idle),
            "scheduled" => Ok(Self::Scheduled),
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "error" => Ok(Self::Error),
            "skipped" => Ok(Self::Skipped),
            _ => Err("Invalid scheduled task status"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectScheduledTaskRunStatus {
    Running,
    Success,
    Error,
    Skipped,
}

impl ProjectScheduledTaskRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

impl FromStr for ProjectScheduledTaskRunStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "error" => Ok(Self::Error),
            "skipped" => Ok(Self::Skipped),
            _ => Err("Invalid scheduled task run status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScheduledTask {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub task_type: ProjectScheduledTaskType,
    pub schedule_mode: ProjectScheduledTaskScheduleMode,
    pub simple_schedule_kind: Option<ProjectScheduledTaskSimpleScheduleKind>,
    pub schedule_expression: String,
    pub interval_seconds: Option<i64>,
    pub daily_time: Option<String>,
    pub weekly_day: Option<i64>,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    pub enabled: bool,
    pub auto_resume: bool,
    pub overlap_policy: ProjectScheduledTaskOverlapPolicy,
    pub status: ProjectScheduledTaskStatus,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScheduledTaskRun {
    pub id: String,
    pub task_id: String,
    pub project_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub status: ProjectScheduledTaskRunStatus,
    pub exit_code: Option<i64>,
    pub response_status: Option<i64>,
    pub error_message: Option<String>,
    pub log_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectScheduledTaskInput {
    pub project_id: String,
    pub name: String,
    pub task_type: ProjectScheduledTaskType,
    pub schedule_mode: ProjectScheduledTaskScheduleMode,
    pub simple_schedule_kind: Option<ProjectScheduledTaskSimpleScheduleKind>,
    pub schedule_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub daily_time: Option<String>,
    pub weekly_day: Option<i64>,
    pub url: Option<String>,
    pub command_line: Option<String>,
    pub working_directory: Option<String>,
    pub enabled: bool,
    pub auto_resume: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectScheduledTaskPatch {
    pub name: Option<String>,
    pub task_type: Option<ProjectScheduledTaskType>,
    pub schedule_mode: Option<ProjectScheduledTaskScheduleMode>,
    pub simple_schedule_kind: Option<Option<ProjectScheduledTaskSimpleScheduleKind>>,
    pub schedule_expression: Option<Option<String>>,
    pub interval_seconds: Option<Option<i64>>,
    pub daily_time: Option<Option<String>>,
    pub weekly_day: Option<Option<i64>>,
    pub url: Option<Option<String>>,
    pub command_line: Option<Option<String>>,
    pub working_directory: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub auto_resume: Option<bool>,
}
