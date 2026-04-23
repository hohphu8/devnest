use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvVar {
    pub id: String,
    pub project_id: String,
    pub env_key: String,
    pub env_value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectEnvVarInput {
    pub project_id: String,
    pub env_key: String,
    pub env_value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectEnvVarInput {
    pub project_id: String,
    pub env_var_id: String,
    pub env_key: String,
    pub env_value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiskEnvVar {
    pub key: String,
    pub value: String,
    pub source_line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectEnvComparisonStatus {
    Match,
    OnlyTracked,
    OnlyDisk,
    ValueMismatch,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvComparisonItem {
    pub key: String,
    pub tracked_value: Option<String>,
    pub disk_value: Option<String>,
    pub status: ProjectEnvComparisonStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvInspection {
    pub project_id: String,
    pub env_file_path: String,
    pub env_file_exists: bool,
    pub disk_read_error: Option<String>,
    pub tracked_count: usize,
    pub disk_count: usize,
    pub disk_vars: Vec<ProjectDiskEnvVar>,
    pub comparison: Vec<ProjectEnvComparisonItem>,
}
