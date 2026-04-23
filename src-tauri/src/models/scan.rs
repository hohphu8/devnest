use crate::models::project::{FrameworkType, ServerType};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub framework: FrameworkType,
    pub recommended_server: ServerType,
    pub server_reason: Option<String>,
    pub recommended_php_version: Option<String>,
    pub suggested_domain: String,
    pub document_root: String,
    pub document_root_reason: Option<String>,
    pub detected_files: Vec<String>,
    pub warnings: Vec<String>,
    pub missing_php_extensions: Vec<String>,
}
