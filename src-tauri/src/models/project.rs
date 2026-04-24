use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerType {
    Apache,
    Nginx,
    Frankenphp,
}

impl ServerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Frankenphp => "frankenphp",
        }
    }
}

impl FromStr for ServerType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "apache" => Ok(Self::Apache),
            "nginx" => Ok(Self::Nginx),
            "frankenphp" => Ok(Self::Frankenphp),
            _ => Err("Invalid server type"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameworkType {
    Laravel,
    Wordpress,
    Php,
    Unknown,
}

impl FrameworkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Laravel => "laravel",
            Self::Wordpress => "wordpress",
            Self::Php => "php",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for FrameworkType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "laravel" => Ok(Self::Laravel),
            "wordpress" => Ok(Self::Wordpress),
            "php" => Ok(Self::Php),
            "unknown" => Ok(Self::Unknown),
            _ => Err("Invalid framework type"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Running,
    Stopped,
    Error,
}

impl ProjectStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

impl FromStr for ProjectStatus {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            _ => Err("Invalid project status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub domain: String,
    pub server_type: ServerType,
    pub php_version: String,
    pub framework: FrameworkType,
    pub document_root: String,
    pub ssl_enabled: bool,
    pub database_name: Option<String>,
    pub database_port: Option<i64>,
    pub status: ProjectStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub path: String,
    pub domain: String,
    pub server_type: ServerType,
    pub php_version: String,
    pub framework: FrameworkType,
    pub document_root: String,
    pub ssl_enabled: bool,
    pub database_name: Option<String>,
    pub database_port: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectPatch {
    pub name: Option<String>,
    pub domain: Option<String>,
    pub server_type: Option<ServerType>,
    pub php_version: Option<String>,
    pub framework: Option<FrameworkType>,
    pub document_root: Option<String>,
    pub ssl_enabled: Option<bool>,
    pub database_name: Option<Option<String>>,
    pub database_port: Option<Option<i64>>,
    pub status: Option<ProjectStatus>,
}
