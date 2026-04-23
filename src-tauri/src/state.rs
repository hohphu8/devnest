use crate::models::mobile_preview::ProjectMobilePreviewState;
use crate::models::optional_tool::OptionalToolInstallTask;
use crate::models::persistent_tunnel::ProjectPersistentTunnelState;
use crate::models::runtime::RuntimeInstallTask;
use crate::models::tunnel::ProjectTunnelState;
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct ManagedServiceProcess {
    pub pid: u32,
    pub child: Child,
    pub log_path: PathBuf,
}

pub struct ManagedWorkerProcess {
    pub pid: u32,
    pub child: Child,
    pub log_path: PathBuf,
}

pub struct ManagedScheduledTaskRun {
    pub pid: Option<u32>,
}

pub struct MobilePreviewSession {
    pub state: ProjectMobilePreviewState,
    pub bind_address: SocketAddr,
    pub shutdown: Arc<AtomicBool>,
    pub worker: Option<JoinHandle<()>>,
}

pub struct AppState {
    pub db_path: PathBuf,
    pub workspace_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub started_at: String,
    pub allow_exit: Mutex<bool>,
    pub managed_processes: Mutex<HashMap<String, ManagedServiceProcess>>,
    pub managed_worker_processes: Mutex<HashMap<String, ManagedWorkerProcess>>,
    pub managed_scheduled_task_runs: Arc<Mutex<HashMap<String, ManagedScheduledTaskRun>>>,
    pub scheduled_task_scheduler_shutdown: Arc<AtomicBool>,
    pub runtime_install_task: Mutex<Option<RuntimeInstallTask>>,
    pub optional_tool_install_task: Mutex<Option<OptionalToolInstallTask>>,
    pub project_tunnels: Mutex<HashMap<String, ProjectTunnelState>>,
    pub project_persistent_tunnels: Mutex<HashMap<String, ProjectPersistentTunnelState>>,
    pub project_mobile_previews: Mutex<HashMap<String, MobilePreviewSession>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootState {
    pub app_name: String,
    pub environment: String,
    pub db_path: String,
    pub started_at: String,
}
