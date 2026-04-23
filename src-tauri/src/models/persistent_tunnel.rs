use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistentTunnelProvider {
    Cloudflared,
}

impl PersistentTunnelProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cloudflared => "cloudflared",
        }
    }
}

impl FromStr for PersistentTunnelProvider {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cloudflared" => Ok(Self::Cloudflared),
            _ => Err("Invalid persistent tunnel provider"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPersistentHostname {
    pub id: String,
    pub project_id: String,
    pub provider: PersistentTunnelProvider,
    pub hostname: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertProjectPersistentHostnameInput {
    pub project_id: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProjectPersistentHostnameInput {
    pub project_id: String,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentTunnelSetupStatus {
    pub provider: PersistentTunnelProvider,
    pub ready: bool,
    pub managed: bool,
    pub binary_path: Option<String>,
    pub auth_cert_path: Option<String>,
    pub credentials_path: Option<String>,
    pub tunnel_id: Option<String>,
    pub tunnel_name: Option<String>,
    pub default_hostname_zone: Option<String>,
    pub details: String,
    pub guidance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentTunnelManagedSetup {
    pub provider: PersistentTunnelProvider,
    pub auth_cert_path: Option<String>,
    pub credentials_path: Option<String>,
    pub tunnel_id: Option<String>,
    pub tunnel_name: Option<String>,
    pub default_hostname_zone: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentTunnelNamedTunnelSummary {
    pub tunnel_id: String,
    pub tunnel_name: String,
    pub credentials_path: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePersistentNamedTunnelInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectPersistentNamedTunnelInput {
    pub tunnel_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePersistentTunnelSetupInput {
    pub default_hostname_zone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistentTunnelStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPersistentTunnelState {
    pub project_id: String,
    pub provider: PersistentTunnelProvider,
    pub status: PersistentTunnelStatus,
    pub hostname: String,
    pub local_url: String,
    pub public_url: String,
    pub log_path: String,
    pub binary_path: Option<String>,
    pub tunnel_id: Option<String>,
    pub credentials_path: Option<String>,
    pub updated_at: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProjectPersistentHostnameResult {
    pub hostname: ProjectPersistentHostname,
    pub tunnel: ProjectPersistentTunnelState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectPersistentHostnameResult {
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentTunnelHealthCheck {
    pub code: String,
    pub label: String,
    pub status: PersistentTunnelStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentTunnelHealthReport {
    pub project_id: String,
    pub hostname: Option<String>,
    pub overall_status: PersistentTunnelStatus,
    pub checks: Vec<PersistentTunnelHealthCheck>,
    pub updated_at: String,
}
