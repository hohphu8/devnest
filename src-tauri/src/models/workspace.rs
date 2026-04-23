use crate::models::project::Project;
use crate::models::scheduled_task::ProjectScheduledTask;
use crate::models::service::{ServiceName, ServiceState};
use crate::models::worker::ProjectWorker;
use crate::state::BootState;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePortStatus {
    pub port: u16,
    pub available: bool,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub managed_owner: Option<ServiceName>,
    pub expected_services: Vec<ServiceName>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOverviewPayload {
    pub boot_state: BootState,
    pub projects: Vec<Project>,
    pub services: Vec<ServiceState>,
    pub workers: Vec<ProjectWorker>,
    pub scheduled_tasks: Vec<ProjectScheduledTask>,
    pub port_summary: Vec<WorkspacePortStatus>,
}
