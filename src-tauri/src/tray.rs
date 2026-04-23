use crate::commands::runtimes::{list_runtime_inventory_snapshot, set_active_runtime_internal};
use crate::core::service_manager;
use crate::error::AppError;
use crate::models::runtime::{RuntimeHealthStatus, RuntimeType};
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::AppState;
use crate::storage::repositories::ServiceRepository;
use rusqlite::Connection;
use std::collections::HashSet;
use std::error::Error;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Runtime};

const TRAY_ID: &str = "devnest-tray";
const TRAY_START_ALL_ID: &str = "tray-start-all";
const TRAY_STOP_ALL_ID: &str = "tray-stop-all";
const TRAY_SHOW_ID: &str = "tray-show";
const TRAY_EXIT_ID: &str = "tray-exit";
const TRAY_PHP_RUNTIME_PREFIX: &str = "tray-php-runtime:";

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
    }
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
        service_manager::stop_service(&connection, &state, service.name)?;
    }

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

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, Box<dyn Error>> {
    let start_all = MenuItem::with_id(app, TRAY_START_ALL_ID, "Start All", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, TRAY_STOP_ALL_ID, "Stop All", true, None::<&str>)?;
    let switch_php = build_php_runtime_submenu(app)?;
    let open = MenuItem::with_id(app, TRAY_SHOW_ID, "Open DevNest", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, TRAY_EXIT_ID, "Exit", true, None::<&str>)?;
    let items: Vec<&dyn IsMenuItem<R>> = vec![&start_all, &stop_all, &switch_php, &open, &exit];

    Ok(Menu::with_items(app, &items)?)
}

pub(crate) fn handle_menu_event<R: Runtime>(
    app: &AppHandle<R>,
    event_id: &str,
) -> Result<(), AppError> {
    match event_id {
        TRAY_START_ALL_ID => start_all_services(app)?,
        TRAY_STOP_ALL_ID => stop_all_services(app)?,
        TRAY_SHOW_ID => crate::show_main_window(app),
        TRAY_EXIT_ID => crate::request_full_exit(app),
        _ if event_id.starts_with(TRAY_PHP_RUNTIME_PREFIX) => {
            let runtime_id = event_id.trim_start_matches(TRAY_PHP_RUNTIME_PREFIX);
            let state = app.state::<AppState>();
            let connection = connection_from_state(&state)?;
            set_active_runtime_internal(&connection, &state, runtime_id)?;
            refresh(app)?;
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
