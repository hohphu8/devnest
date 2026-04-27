use crate::commands::{
    persistent_tunnels,
    runtimes::{list_runtime_inventory_snapshot, set_active_runtime_internal},
};
use crate::core::{frankenphp_octane_manager, service_manager};
use crate::error::AppError;
use crate::models::runtime::{RuntimeHealthStatus, RuntimeType};
use crate::models::service::{ServiceName, ServiceState, ServiceStatus};
use crate::state::AppState;
use crate::storage::repositories::ServiceRepository;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashSet;
use std::error::Error;
use std::str::FromStr;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "devnest-tray";
const TRAY_START_ALL_ID: &str = "tray-start-all";
const TRAY_STOP_ALL_ID: &str = "tray-stop-all";
const TRAY_SHOW_ID: &str = "tray-show";
const TRAY_EXIT_ID: &str = "tray-exit";
const TRAY_PHP_RUNTIME_PREFIX: &str = "tray-php-runtime:";
const TRAY_SERVICE_ACTION_PREFIX: &str = "tray-service:";
const TRAY_LOGS_NAVIGATION_EVENT: &str = "devnest:navigate-logs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayServiceAction {
    Start,
    Stop,
    Restart,
    Logs,
}

impl TrayServiceAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Logs => "logs",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "restart" => Some(Self::Restart),
            "logs" => Some(Self::Logs),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayServiceActionId {
    service: ServiceName,
    action: TrayServiceAction,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayLogsNavigationPayload {
    source: String,
}

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn service_start_rank(name: &ServiceName) -> usize {
    match name {
        ServiceName::Mysql => 0,
        ServiceName::Redis => 1,
        ServiceName::Mailpit => 2,
        ServiceName::Apache => 3,
        ServiceName::Nginx => 4,
        ServiceName::Frankenphp => 5,
    }
}

fn tray_service_rank(name: &ServiceName) -> Option<usize> {
    match name {
        ServiceName::Apache => Some(0),
        ServiceName::Frankenphp => Some(1),
        ServiceName::Nginx => Some(2),
        ServiceName::Mysql => Some(3),
        ServiceName::Mailpit => Some(4),
        ServiceName::Redis => Some(5),
    }
}

fn tray_service_action_id(service: &ServiceName, action: TrayServiceAction) -> String {
    format!(
        "{TRAY_SERVICE_ACTION_PREFIX}{}:{}",
        service.as_str(),
        action.as_str()
    )
}

fn parse_tray_service_action_id(event_id: &str) -> Option<TrayServiceActionId> {
    let rest = event_id.strip_prefix(TRAY_SERVICE_ACTION_PREFIX)?;
    let (service, action) = rest.split_once(':')?;
    let service = ServiceName::from_str(service).ok()?;
    let action = TrayServiceAction::from_str(action)?;

    Some(TrayServiceActionId { service, action })
}

fn tray_service_label(service: &ServiceState) -> String {
    format!(
        "{} - {}",
        service.name.display_name(),
        service.status.as_str()
    )
}

fn start_all_services<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let connection = connection_from_state(&state)?;
    let mut services = ServiceRepository::list(&connection)?;
    let mut reserved_ports = HashSet::new();

    services.sort_by_key(|service| service_start_rank(&service.name));

    for service in services.into_iter().filter(|service| service.enabled) {
        if let Some(port) = service.port.and_then(|port| u16::try_from(port).ok()) {
            if reserved_ports.contains(&port) {
                continue;
            }
            reserved_ports.insert(port);
        }

        if matches!(service.status, ServiceStatus::Running) {
            continue;
        }

        service_manager::start_service(&connection, &state, service.name)?;
    }

    Ok(())
}

fn stop_all_services<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let connection = connection_from_state(&state)?;

    for service in ServiceRepository::list(&connection)?
        .into_iter()
        .filter(|service| matches!(service.status, ServiceStatus::Running))
    {
        stop_service_with_tray_cleanup(&connection, &state, service.name)?;
    }

    Ok(())
}

fn stop_service_with_tray_cleanup(
    connection: &Connection,
    state: &AppState,
    service_name: ServiceName,
) -> Result<ServiceState, AppError> {
    let stopped = service_manager::stop_service(connection, state, service_name.clone())?;
    if matches!(service_name, ServiceName::Frankenphp) {
        frankenphp_octane_manager::mark_stale_for_frankenphp_stop(connection, state)?;
    }
    persistent_tunnels::reset_persistent_tunnels_for_origin_service_stop(
        connection,
        state,
        &service_name,
    )?;
    Ok(stopped)
}

fn handle_service_lifecycle_action<R: Runtime>(
    app: &AppHandle<R>,
    action_id: TrayServiceActionId,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let connection = connection_from_state(&state)?;
    let service = ServiceRepository::get(&connection, action_id.service.as_str())?;

    match action_id.action {
        TrayServiceAction::Start => {
            service_manager::start_service(&connection, &state, service.name)?;
            refresh(app)?;
        }
        TrayServiceAction::Stop => {
            stop_service_with_tray_cleanup(&connection, &state, service.name)?;
            refresh(app)?;
        }
        TrayServiceAction::Restart => {
            service_manager::restart_service(&connection, &state, service.name)?;
            refresh(app)?;
        }
        TrayServiceAction::Logs => {
            open_service_logs_from_tray(app, service.name)?;
        }
    }

    Ok(())
}

fn open_service_logs_from_tray<R: Runtime>(
    app: &AppHandle<R>,
    service_name: ServiceName,
) -> Result<(), AppError> {
    crate::show_main_window(app);
    app.emit(
        TRAY_LOGS_NAVIGATION_EVENT,
        TrayLogsNavigationPayload {
            source: service_name.as_str().to_string(),
        },
    )
    .map_err(|error| {
        AppError::with_details(
            "TRAY_NAVIGATION_FAILED",
            "DevNest could not open service logs from the tray.",
            error.to_string(),
        )
    })?;

    Ok(())
}

fn build_php_runtime_submenu<R: Runtime>(app: &AppHandle<R>) -> Result<Submenu<R>, Box<dyn Error>> {
    let state = app.state::<AppState>();
    let connection = connection_from_state(&state)?;
    let mut php_runtimes = list_runtime_inventory_snapshot(&connection, &state)?
        .into_iter()
        .filter(|runtime| matches!(runtime.runtime_type, RuntimeType::Php))
        .collect::<Vec<_>>();

    php_runtimes.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| right.version.cmp(&left.version))
    });

    if php_runtimes.is_empty() {
        let empty = MenuItem::with_id(
            app,
            "tray-php-runtime-empty",
            "No PHP runtimes linked",
            false,
            None::<&str>,
        )?;
        let items: Vec<&dyn IsMenuItem<R>> = vec![&empty];
        return Ok(Submenu::with_items(
            app,
            "Switch PHP Version",
            true,
            &items,
        )?);
    }

    let runtime_items = php_runtimes
        .into_iter()
        .map(|runtime| {
            let enabled = matches!(runtime.status, RuntimeHealthStatus::Available);
            let label = if enabled {
                format!("PHP {}", runtime.version)
            } else {
                format!("PHP {} (Missing)", runtime.version)
            };

            CheckMenuItem::with_id(
                app,
                format!("{TRAY_PHP_RUNTIME_PREFIX}{}", runtime.id),
                label,
                enabled,
                runtime.is_active,
                None::<&str>,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let item_refs = runtime_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<R>)
        .collect::<Vec<_>>();

    Ok(Submenu::with_items(
        app,
        "Switch PHP Version",
        true,
        &item_refs,
    )?)
}

fn build_service_submenu<R: Runtime>(
    app: &AppHandle<R>,
    service: &ServiceState,
) -> Result<Submenu<R>, Box<dyn Error>> {
    let running = matches!(service.status, ServiceStatus::Running);
    let start = MenuItem::with_id(
        app,
        tray_service_action_id(&service.name, TrayServiceAction::Start),
        "Start",
        service.enabled && !running,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        tray_service_action_id(&service.name, TrayServiceAction::Stop),
        "Stop",
        running,
        None::<&str>,
    )?;
    let restart = MenuItem::with_id(
        app,
        tray_service_action_id(&service.name, TrayServiceAction::Restart),
        "Restart",
        service.enabled,
        None::<&str>,
    )?;
    let logs = MenuItem::with_id(
        app,
        tray_service_action_id(&service.name, TrayServiceAction::Logs),
        "Logs",
        true,
        None::<&str>,
    )?;
    let items: Vec<&dyn IsMenuItem<R>> = vec![&start, &stop, &restart, &logs];
    let label = tray_service_label(service);

    Ok(Submenu::with_items(app, label, true, &items)?)
}

fn build_services_submenu<R: Runtime>(app: &AppHandle<R>) -> Result<Submenu<R>, Box<dyn Error>> {
    let state = app.state::<AppState>();
    let connection = connection_from_state(&state)?;
    let mut services = ServiceRepository::list(&connection)?
        .into_iter()
        .filter(|service| tray_service_rank(&service.name).is_some())
        .collect::<Vec<_>>();

    services.sort_by_key(|service| tray_service_rank(&service.name).unwrap_or(usize::MAX));

    if services.is_empty() {
        let empty = MenuItem::with_id(
            app,
            "tray-services-empty",
            "No services tracked",
            false,
            None::<&str>,
        )?;
        let items: Vec<&dyn IsMenuItem<R>> = vec![&empty];
        return Ok(Submenu::with_items(app, "Services", true, &items)?);
    }

    let service_submenus = services
        .iter()
        .map(|service| build_service_submenu(app, service))
        .collect::<Result<Vec<_>, _>>()?;
    let item_refs = service_submenus
        .iter()
        .map(|item| item as &dyn IsMenuItem<R>)
        .collect::<Vec<_>>();

    Ok(Submenu::with_items(app, "Services", true, &item_refs)?)
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, Box<dyn Error>> {
    let start_all = MenuItem::with_id(app, TRAY_START_ALL_ID, "Start All", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, TRAY_STOP_ALL_ID, "Stop All", true, None::<&str>)?;
    let switch_php = build_php_runtime_submenu(app)?;
    let services = build_services_submenu(app)?;
    let open = MenuItem::with_id(app, TRAY_SHOW_ID, "Open DevNest", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, TRAY_EXIT_ID, "Exit", true, None::<&str>)?;
    let items: Vec<&dyn IsMenuItem<R>> =
        vec![&start_all, &stop_all, &switch_php, &services, &open, &exit];

    Ok(Menu::with_items(app, &items)?)
}

pub(crate) fn handle_menu_event<R: Runtime>(
    app: &AppHandle<R>,
    event_id: &str,
) -> Result<(), AppError> {
    match event_id {
        TRAY_START_ALL_ID => {
            start_all_services(app)?;
            refresh(app)?;
        }
        TRAY_STOP_ALL_ID => {
            stop_all_services(app)?;
            refresh(app)?;
        }
        TRAY_SHOW_ID => crate::show_main_window(app),
        TRAY_EXIT_ID => crate::request_full_exit(app),
        _ if event_id.starts_with(TRAY_PHP_RUNTIME_PREFIX) => {
            let runtime_id = event_id.trim_start_matches(TRAY_PHP_RUNTIME_PREFIX);
            let state = app.state::<AppState>();
            let connection = connection_from_state(&state)?;
            set_active_runtime_internal(&connection, &state, runtime_id)?;
            refresh(app)?;
        }
        _ if event_id.starts_with(TRAY_SERVICE_ACTION_PREFIX) => {
            if let Some(action_id) = parse_tray_service_action_id(event_id) {
                handle_service_lifecycle_action(app, action_id)?;
            }
        }
        _ => {}
    }

    Ok(())
}

pub(crate) fn initialize<R: Runtime>(app: &App<R>) -> Result<(), Box<dyn Error>> {
    let tray_menu = build_menu(app.handle())?;
    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&tray_menu)
        .tooltip("DevNest")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Err(error) = handle_menu_event(app, event.id().as_ref()) {
                eprintln!("DevNest tray action failed: {}", error);
            }
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => crate::show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder.build(app)?;
    Ok(())
}

pub(crate) fn refresh<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let menu = build_menu(app).map_err(|error| {
        AppError::with_details(
            "TRAY_REFRESH_FAILED",
            "DevNest could not rebuild the system tray menu.",
            error.to_string(),
        )
    })?;

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|error| {
            AppError::with_details(
                "TRAY_REFRESH_FAILED",
                "DevNest could not refresh the system tray menu.",
                error.to_string(),
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_state(name: ServiceName, status: ServiceStatus) -> ServiceState {
        ServiceState {
            name,
            enabled: true,
            auto_start: false,
            port: None,
            pid: None,
            status,
            last_error: None,
            updated_at: "2026-04-27T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn parses_tray_service_action_ids() {
        let action_id = parse_tray_service_action_id("tray-service:frankenphp:restart")
            .expect("valid tray service action id");

        assert_eq!(action_id.service, ServiceName::Frankenphp);
        assert_eq!(action_id.action, TrayServiceAction::Restart);
        assert!(parse_tray_service_action_id("tray-service:frankenphp:unknown").is_none());
        assert!(parse_tray_service_action_id("tray-service:unknown:start").is_none());
    }

    #[test]
    fn builds_stable_service_labels() {
        assert_eq!(
            tray_service_label(&service_state(ServiceName::Apache, ServiceStatus::Running)),
            "Apache - running"
        );
        assert_eq!(
            tray_service_label(&service_state(
                ServiceName::Frankenphp,
                ServiceStatus::Error
            )),
            "FrankenPHP - error"
        );
    }

    #[test]
    fn tray_services_have_expected_order() {
        let mut services = [
            ServiceName::Redis,
            ServiceName::Mysql,
            ServiceName::Nginx,
            ServiceName::Apache,
            ServiceName::Mailpit,
            ServiceName::Frankenphp,
        ];
        services.sort_by_key(|service| tray_service_rank(service).unwrap_or(usize::MAX));

        assert_eq!(
            services,
            [
                ServiceName::Apache,
                ServiceName::Frankenphp,
                ServiceName::Nginx,
                ServiceName::Mysql,
                ServiceName::Mailpit,
                ServiceName::Redis,
            ]
        );
    }
}
