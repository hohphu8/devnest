use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelProvider {
    Cloudflared,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTunnelState {
    pub project_id: String,
    pub provider: TunnelProvider,
    pub status: TunnelStatus,
    pub local_url: String,
    pub public_url: Option<String>,
    #[serde(default)]
    pub public_host_alias_synced: bool,
    pub log_path: String,
    pub binary_path: Option<String>,
    pub updated_at: String,
    pub details: Option<String>,
}
