use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MobilePreviewStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMobilePreviewState {
    pub project_id: String,
    pub status: MobilePreviewStatus,
    pub local_project_url: String,
    pub lan_ip: Option<String>,
    pub port: Option<u16>,
    pub proxy_url: Option<String>,
    pub qr_url: Option<String>,
    pub updated_at: String,
    pub details: Option<String>,
}
