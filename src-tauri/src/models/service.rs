use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceName {
    Apache,
    Nginx,
    Mysql,
    Mailpit,
    Redis,
}

impl ServiceName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Mysql => "mysql",
            Self::Mailpit => "mailpit",
            Self::Redis => "redis",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Apache => "Apache",
            Self::Nginx => "Nginx",
            Self::Mysql => "MySQL",
            Self::Mailpit => "Mailpit",
            Self::Redis => "Redis",
        }
    }

    pub fn default_port(&self) -> Option<u16> {
        match self {
            Self::Apache | Self::Nginx => Some(80),
            Self::Mysql => Some(3306),
            Self::Mailpit => Some(8025),
            Self::Redis => Some(6379),
        }
    }
}

impl FromStr for ServiceName {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "apache" => Ok(Self::Apache),
            "nginx" => Ok(Self::Nginx),
            "mysql" => Ok(Self::Mysql),
            "mailpit" => Ok(Self::Mailpit),
            "redis" => Ok(Self::Redis),
            _ => Err("Invalid service name"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Running,
    Stopped,
    Error,
}

impl ServiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

impl FromStr for ServiceStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            _ => Err("Invalid service status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceState {
    pub name: ServiceName,
    pub enabled: bool,
    pub auto_start: bool,
    pub port: Option<i64>,
    pub pid: Option<i64>,
    pub status: ServiceStatus,
    pub last_error: Option<String>,
    pub updated_at: String,
}
