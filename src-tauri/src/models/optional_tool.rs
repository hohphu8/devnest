use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionalToolType {
    Mailpit,
    Cloudflared,
    Phpmyadmin,
}

impl OptionalToolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mailpit => "mailpit",
            Self::Cloudflared => "cloudflared",
            Self::Phpmyadmin => "phpmyadmin",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Mailpit => "Mailpit",
            Self::Cloudflared => "cloudflared",
            Self::Phpmyadmin => "phpMyAdmin",
        }
    }
}

impl FromStr for OptionalToolType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mailpit" => Ok(Self::Mailpit),
            "cloudflared" => Ok(Self::Cloudflared),
            "phpmyadmin" => Ok(Self::Phpmyadmin),
            _ => Err("Invalid optional tool type"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolVersion {
    pub id: String,
    pub tool_type: OptionalToolType,
    pub version: String,
    pub path: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionalToolHealthStatus {
    Available,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolInventoryItem {
    pub id: String,
    pub tool_type: OptionalToolType,
    pub version: String,
    pub path: String,
    pub is_active: bool,
    pub status: OptionalToolHealthStatus,
    pub created_at: String,
    pub updated_at: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionalToolArchiveKind {
    Zip,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolPackageManifest {
    pub packages: Vec<OptionalToolPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolPackage {
    pub id: String,
    pub tool_type: OptionalToolType,
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub display_name: String,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub archive_kind: OptionalToolArchiveKind,
    pub entry_binary: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionalToolInstallStage {
    Queued,
    Downloading,
    Verifying,
    Extracting,
    Registering,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolInstallTask {
    pub package_id: String,
    pub display_name: String,
    pub tool_type: OptionalToolType,
    pub version: String,
    pub stage: OptionalToolInstallStage,
    pub message: String,
    pub updated_at: String,
    pub error_code: Option<String>,
}
