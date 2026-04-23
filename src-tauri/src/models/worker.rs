use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectWorkerPresetType {
    Queue,
    Schedule,
    Custom,
}

impl ProjectWorkerPresetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Schedule => "schedule",
            Self::Custom => "custom",
        }
    }
}

impl FromStr for ProjectWorkerPresetType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queue" => Ok(Self::Queue),
            "schedule" => Ok(Self::Schedule),
            "custom" => Ok(Self::Custom),
            _ => Err("Invalid worker preset type"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectWorkerStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Restarting,
}

impl ProjectWorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
            Self::Starting => "starting",
            Self::Restarting => "restarting",
        }
    }
}

impl FromStr for ProjectWorkerStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            "starting" => Ok(Self::Starting),
            "restarting" => Ok(Self::Restarting),
            _ => Err("Invalid worker status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorker {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub preset_type: ProjectWorkerPresetType,
    pub command: String,
    pub args: Vec<String>,
    pub working_directory: String,
    pub auto_start: bool,
    pub status: ProjectWorkerStatus,
    pub pid: Option<i64>,
    pub last_started_at: Option<String>,
    pub last_stopped_at: Option<String>,
    pub last_exit_code: Option<i64>,
    pub last_error: Option<String>,
    pub log_path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectWorkerInput {
    pub project_id: String,
    pub name: String,
    pub preset_type: ProjectWorkerPresetType,
    pub command_line: String,
    pub working_directory: Option<String>,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectWorkerPatch {
    pub name: Option<String>,
    pub preset_type: Option<ProjectWorkerPresetType>,
    pub command_line: Option<String>,
    pub working_directory: Option<Option<String>>,
    pub auto_start: Option<bool>,
}
