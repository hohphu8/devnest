use crate::models::diagnostics::DiagnosticItem;
use crate::models::persistent_tunnel::{
    PersistentTunnelHealthReport, ProjectPersistentHostname, ProjectPersistentTunnelState,
};
use crate::models::project::{Project, ServerType};
use crate::models::service::ServiceState;
use crate::models::tunnel::ProjectTunnelState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityLayer {
    Project,
    Runtime,
    Config,
    Service,
    Dns,
    Tunnel,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityAction {
    ProvisionProject,
    PublishPersistentDomain,
    StartProjectRuntime,
    RestoreAppMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepairWorkflow {
    Project,
    Tunnel,
    RuntimeLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityCheck {
    pub code: String,
    pub layer: ReliabilityLayer,
    pub status: ReliabilityStatus,
    pub blocking: bool,
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPreflightReport {
    pub action: ReliabilityAction,
    pub project_id: Option<String>,
    pub ready: bool,
    pub summary: String,
    pub checks: Vec<ReliabilityCheck>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairWorkflowInfo {
    pub workflow: RepairWorkflow,
    pub title: String,
    pub summary: String,
    pub touches: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairExecutionResult {
    pub workflow: RepairWorkflow,
    pub success: bool,
    pub message: String,
    pub touched_layers: Vec<ReliabilityLayer>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorConfigSnapshot {
    pub server_type: ServerType,
    pub output_path: String,
    pub preview: Option<String>,
    pub local_domain_alias_present: bool,
    pub persistent_alias_present: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorRuntimeBinding {
    pub kind: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub active: bool,
    pub available: bool,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorRuntimeSnapshot {
    pub server: InspectorRuntimeBinding,
    pub php: InspectorRuntimeBinding,
    pub mysql: Option<InspectorRuntimeBinding>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityInspectorSnapshot {
    pub project: Project,
    pub diagnostics: Vec<DiagnosticItem>,
    pub services: Vec<ServiceState>,
    pub config: InspectorConfigSnapshot,
    pub runtime: InspectorRuntimeSnapshot,
    pub quick_tunnel: Option<ProjectTunnelState>,
    pub persistent_hostname: Option<ProjectPersistentHostname>,
    pub persistent_tunnel: Option<ProjectPersistentTunnelState>,
    pub persistent_health: Option<PersistentTunnelHealthReport>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityTransferResult {
    pub success: bool,
    pub path: String,
}
