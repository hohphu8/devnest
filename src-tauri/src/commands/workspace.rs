use crate::core::{ports, scheduled_task_manager, service_manager, worker_manager};
use crate::error::AppError;
use crate::models::service::ServiceStatus;
use crate::models::workspace::{WorkspaceOverviewPayload, WorkspacePortStatus};
use crate::state::{AppState, BootState};
use crate::storage::repositories::ProjectRepository;
use crate::utils::perf;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[tauri::command]
pub fn get_workspace_overview(
    state: tauri::State<'_, AppState>,
) -> Result<WorkspaceOverviewPayload, AppError> {
    let started_at = Instant::now();
    let connection = connection_from_state(&state)?;
    let phase_started_at = Instant::now();
    let projects = ProjectRepository::list(&connection)?;
    perf::log_elapsed("workspace overview projects", phase_started_at);
    let phase_started_at = Instant::now();
    let services = service_manager::get_all_service_status(&connection, &state)?;
    perf::log_elapsed("workspace overview services", phase_started_at);
    let phase_started_at = Instant::now();
    let workers = worker_manager::list_all_workers(&connection, &state)?;
    perf::log_elapsed("workspace overview workers", phase_started_at);
    let phase_started_at = Instant::now();
    let scheduled_tasks = scheduled_task_manager::list_all_scheduled_tasks(&connection, &state)?;
    perf::log_elapsed("workspace overview scheduled tasks", phase_started_at);

    let mut expected_services_by_port = BTreeMap::<u16, Vec<_>>::new();
    for service in &services {
        let Some(port) = service
            .port
            .and_then(|value| u16::try_from(value).ok())
            .or_else(|| service.name.default_port())
        else {
            continue;
        };

        expected_services_by_port
            .entry(port)
            .or_default()
            .push(service.name.clone());
    }

    let known_ports = expected_services_by_port
        .keys()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let phase_started_at = Instant::now();
    let port_checks = ports::check_ports(&known_ports)?;
    perf::log_elapsed("workspace overview ports", phase_started_at);

    let port_summary = port_checks
        .into_iter()
        .map(|check| {
            let managed_owner = services
                .iter()
                .find(|service| {
                    matches!(service.status, ServiceStatus::Running)
                        && service.pid.and_then(|value| u32::try_from(value).ok()) == check.pid
                        && service.port.and_then(|value| u16::try_from(value).ok())
                            == Some(check.port)
                })
                .map(|service| service.name.clone());

            WorkspacePortStatus {
                port: check.port,
                available: check.available,
                pid: check.pid,
                process_name: check.process_name,
                managed_owner,
                expected_services: expected_services_by_port
                    .remove(&check.port)
                    .unwrap_or_default(),
            }
        })
        .collect();

    let payload = WorkspaceOverviewPayload {
        boot_state: BootState {
            app_name: "DevNest".to_string(),
            environment: "tauri".to_string(),
            db_path: state.db_path.display().to_string(),
            started_at: state.started_at.clone(),
        },
        projects,
        services,
        workers,
        scheduled_tasks,
        port_summary,
    };
    perf::log_elapsed("workspace overview total", started_at);

    Ok(payload)
}
