use crate::models::project::FrankenphpMode;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrankenphpOctaneWorkerStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Restarting,
}

impl FrankenphpOctaneWorkerStatus {
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

impl FromStr for FrankenphpOctaneWorkerStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            "starting" => Ok(Self::Starting),
            "restarting" => Ok(Self::Restarting),
            _ => Err("Invalid Octane worker status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpOctaneWorkerSettings {
    pub project_id: String,
    pub mode: FrankenphpMode,
    pub worker_port: i64,
    pub admin_port: i64,
    pub workers: i64,
    pub max_requests: i64,
    pub status: FrankenphpOctaneWorkerStatus,
    pub pid: Option<i64>,
    pub last_started_at: Option<String>,
    pub last_stopped_at: Option<String>,
    pub last_error: Option<String>,
    pub log_path: String,
    pub custom_worker_relative_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpRuntimeExtensionHealth {
    pub extension_name: String,
    pub available: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpRuntimeHealth {
    pub runtime_id: String,
    pub version: String,
    pub php_family: Option<String>,
    pub path: String,
    pub managed_php_config_path: Option<String>,
    pub extensions: Vec<FrankenphpRuntimeExtensionHealth>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpOctaneWorkerHealth {
    pub project_id: String,
    pub mode: FrankenphpMode,
    pub status: FrankenphpOctaneWorkerStatus,
    pub pid: Option<i64>,
    pub uptime_seconds: Option<i64>,
    pub worker_port: i64,
    pub admin_port: i64,
    pub last_started_at: Option<String>,
    pub last_restarted_at: Option<String>,
    pub last_error: Option<String>,
    pub request_count: Option<i64>,
    pub metrics_available: bool,
    pub log_tail: String,
    pub restart_recommended: bool,
    pub restart_reason: Option<String>,
    pub runtime: Option<FrankenphpRuntimeHealth>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFrankenphpOctaneWorkerSettingsInput {
    pub mode: Option<FrankenphpMode>,
    pub worker_port: Option<i64>,
    pub admin_port: Option<i64>,
    pub workers: Option<i64>,
    pub max_requests: Option<i64>,
    pub custom_worker_relative_path: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FrankenphpOctanePreflightLevel {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpOctanePreflightCheck {
    pub code: String,
    pub level: FrankenphpOctanePreflightLevel,
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpOctanePreflight {
    pub project_id: String,
    pub mode: FrankenphpMode,
    pub ready: bool,
    pub summary: String,
    pub install_commands: Vec<String>,
    pub checks: Vec<FrankenphpOctanePreflightCheck>,
    pub generated_at: String,
}
