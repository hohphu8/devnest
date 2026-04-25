mod commands;
mod core;
mod error;
mod models;
mod state;
mod storage;
mod tray;
mod utils;

use crate::commands::app::{
    check_for_app_update, get_app_release_info, get_boot_state, install_app_update, ping,
};
use crate::commands::configs::{generate_vhost_config, preview_vhost_config};
use crate::commands::database::{
    backup_database, create_database, disable_database_time_machine, drop_database,
    enable_database_time_machine, get_database_time_machine_status, list_database_snapshots,
    list_databases, restore_database, rollback_database_snapshot,
    run_scheduled_database_snapshot_cycle, take_database_snapshot,
};
use crate::commands::diagnostics::{apply_diagnostic_fix, run_diagnostics};
use crate::commands::frankenphp_octane::{
    get_project_frankenphp_octane_preflight, get_project_frankenphp_worker_settings,
    get_project_frankenphp_worker_status, read_project_frankenphp_worker_logs,
    reload_project_frankenphp_worker, restart_project_frankenphp_worker,
    start_project_frankenphp_worker, stop_project_frankenphp_worker,
    update_project_frankenphp_worker_settings,
};
use crate::commands::hosts::{apply_hosts_entry, remove_hosts_entry};
use crate::commands::logs::{clear_service_logs, read_service_logs};
use crate::commands::mobile_preview::{
    get_project_mobile_preview_state, start_project_mobile_preview, stop_project_mobile_preview,
};
use crate::commands::optional_tools::{
    get_optional_tool_install_task, install_optional_tool_package, list_optional_tool_inventory,
    list_optional_tool_packages, remove_optional_tool, reveal_optional_tool_path,
};
use crate::commands::persistent_tunnels::{
    apply_project_persistent_hostname, connect_persistent_tunnel_provider,
    create_persistent_named_tunnel, delete_persistent_named_tunnel,
    delete_project_persistent_hostname, disconnect_persistent_tunnel_provider,
    get_persistent_tunnel_setup_status, get_project_persistent_hostname,
    get_project_persistent_tunnel_state, import_persistent_tunnel_auth_cert,
    import_persistent_tunnel_credentials, inspect_project_persistent_tunnel_health,
    list_available_persistent_named_tunnels, open_project_persistent_tunnel_url,
    publish_project_persistent_tunnel, remove_project_persistent_hostname,
    select_persistent_named_tunnel, start_project_persistent_tunnel,
    stop_project_persistent_tunnel, unpublish_project_persistent_tunnel,
    update_persistent_tunnel_setup, upsert_project_persistent_hostname,
};
use crate::commands::php_extensions::{
    install_php_extension, install_php_extension_package, list_php_extension_packages,
    list_php_extensions, list_php_functions, remove_php_extension, set_php_extension_enabled,
    set_php_function_enabled,
};
use crate::commands::ports::check_port;
use crate::commands::project_env_vars::{
    create_project_env_var, delete_project_env_var, inspect_project_env, list_project_env_vars,
    update_project_env_var,
};
use crate::commands::project_profiles::{
    export_project_profile, export_team_project_profile, import_project_profile,
    import_team_project_profile,
};
use crate::commands::project_scheduled_tasks::{
    clear_project_scheduled_task_history, clear_project_scheduled_task_logs,
    create_project_scheduled_task, delete_project_scheduled_task, disable_project_scheduled_task,
    enable_project_scheduled_task, get_project_scheduled_task_status, list_all_scheduled_tasks,
    list_project_scheduled_task_runs, list_project_scheduled_tasks,
    read_project_scheduled_task_run_logs, run_project_scheduled_task_now,
    update_project_scheduled_task,
};
use crate::commands::project_workers::{
    clear_project_worker_logs, create_project_worker, delete_project_worker,
    get_project_worker_status, list_all_workers, list_project_workers, read_project_worker_logs,
    restart_project_worker, start_project_worker, stop_project_worker, update_project_worker,
};
use crate::commands::projects::{
    create_project, delete_project, get_project, list_projects, open_project_folder,
    open_project_terminal, open_project_vscode, pick_project_folder, scan_project, update_project,
};
use crate::commands::recipes::{clone_git_recipe, create_laravel_recipe, create_wordpress_recipe};
use crate::commands::reliability::{
    backup_app_metadata, export_diagnostics_bundle, inspect_reliability_state,
    list_repair_workflows, restore_app_metadata, run_action_preflight, run_repair_workflow,
};
use crate::commands::runtime_configs::{
    get_runtime_config_schema, get_runtime_config_values, open_runtime_config_file,
    update_runtime_config,
};
use crate::commands::runtimes::{
    get_runtime_install_task, import_runtime_path, install_runtime_package, link_runtime_path,
    list_runtime_inventory, list_runtime_packages, remove_runtime_reference, reveal_runtime_path,
    set_active_runtime, verify_runtime_path,
};
use crate::commands::services::{
    get_all_service_status, get_service_status, open_service_dashboard, restart_service,
    start_service, stop_service,
};
use crate::commands::ssl::{
    get_local_ssl_authority_status, open_project_site, regenerate_project_ssl_certificate,
    trust_local_ssl_authority, untrust_local_ssl_authority,
};
use crate::commands::tunnels::{
    get_project_tunnel_state, open_project_tunnel_url, start_project_tunnel, stop_project_tunnel,
};
use crate::commands::workspace::get_workspace_overview;
use crate::core::{
    frankenphp_octane_manager, php_cli_environment, runtime_registry, scheduled_task_manager,
    service_manager, worker_manager,
};
use crate::error::AppError;
use crate::models::project::ServerType;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::{AppState, MobilePreviewSession};
use crate::storage::db::init_database;
use crate::storage::repositories::ProjectRepository;
use rusqlite::Connection;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, RunEvent, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";

fn boot_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    format!("{now}")
}

fn preferred_boot_web_service(connection: &Connection) -> Result<Option<ServiceName>, AppError> {
    let Some(project) = ProjectRepository::list(connection)?.into_iter().next() else {
        return Ok(None);
    };

    let service = match project.server_type {
        ServerType::Apache => ServiceName::Apache,
        ServerType::Nginx => ServiceName::Nginx,
        ServerType::Frankenphp => ServiceName::Frankenphp,
    };

    Ok(Some(service))
}

fn ensure_service_running(
    connection: &Connection,
    state: &AppState,
    service: ServiceName,
) -> Result<(), AppError> {
    let current = service_manager::get_service_status(connection, state, service.clone())?;
    if matches!(current.status, ServiceStatus::Running) {
        return Ok(());
    }

    service_manager::start_service(connection, state, service)?;
    Ok(())
}

fn auto_start_boot_services(connection: &Connection, state: &AppState) {
    for service in [
        Some(ServiceName::Mysql),
        preferred_boot_web_service(connection).ok().flatten(),
    ] {
        let Some(service) = service else {
            continue;
        };

        if let Err(error) = ensure_service_running(connection, state, service.clone()) {
            eprintln!(
                "DevNest boot auto-start failed for {}: {}",
                service.display_name(),
                error
            );
        }
    }
}

fn auto_start_boot_workers(connection: &Connection, state: &AppState) {
    worker_manager::auto_start_project_workers(connection, state);
}

fn auto_start_boot_octane_workers(connection: &Connection, state: &AppState) {
    frankenphp_octane_manager::auto_start_previous_octane_workers(connection, state);
}

fn auto_resume_boot_scheduled_tasks(connection: &Connection) {
    if let Err(error) =
        scheduled_task_manager::prepare_auto_resume_project_scheduled_tasks(connection)
    {
        eprintln!("DevNest boot scheduled task restore failed: {}", error);
    }
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn can_exit(state: &AppState) -> bool {
    state
        .allow_exit
        .lock()
        .map(|allow_exit| *allow_exit)
        .unwrap_or(false)
}

fn stop_mobile_preview_session(mut session: MobilePreviewSession) {
    session.shutdown.store(true, Ordering::Relaxed);
    let _ = TcpStream::connect(session.bind_address);
    if let Some(worker) = session.worker.take() {
        let _ = worker.join();
    }
}

fn cleanup_mobile_preview_sessions(state: &AppState) {
    let sessions = match state.project_mobile_previews.lock() {
        Ok(mut sessions) => std::mem::take(&mut *sessions)
            .into_values()
            .collect::<Vec<_>>(),
        Err(_) => {
            eprintln!("DevNest could not acquire the mobile preview session lock during exit.");
            return;
        }
    };

    for session in sessions {
        stop_mobile_preview_session(session);
    }
}

fn request_full_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    cleanup_mobile_preview_sessions(&state);
    state
        .scheduled_task_scheduler_shutdown
        .store(true, Ordering::Relaxed);
    scheduled_task_manager::stop_all_project_scheduled_tasks(&state);
    worker_manager::stop_all_project_workers(&state);
    if let Ok(mut allow_exit) = state.allow_exit.lock() {
        *allow_exit = true;
    }
    app.exit(0);
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let workspace_dir = app.path().app_data_dir()?;
            let resources_dir = app.path().resource_dir()?;
            let db_path = workspace_dir.join("devnest.sqlite3");

            init_database(&db_path)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            let connection = Connection::open(&db_path)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            runtime_registry::sync_runtime_versions(&connection, &workspace_dir, &resources_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            php_cli_environment::sync_active_php_cli_environment(&connection, &workspace_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

            let state = AppState {
                db_path,
                workspace_dir,
                resources_dir,
                started_at: boot_timestamp(),
                allow_exit: Mutex::new(false),
                managed_processes: Mutex::new(HashMap::new()),
                managed_worker_processes: Mutex::new(HashMap::new()),
                managed_scheduled_task_runs: Arc::new(Mutex::new(HashMap::new())),
                scheduled_task_scheduler_shutdown: Arc::new(AtomicBool::new(false)),
                runtime_install_task: Mutex::new(None),
                optional_tool_install_task: Mutex::new(None),
                project_tunnels: Mutex::new(HashMap::new()),
                project_persistent_tunnels: Mutex::new(HashMap::new()),
                project_mobile_previews: Mutex::new(HashMap::new()),
            };

            auto_start_boot_services(&connection, &state);
            auto_start_boot_workers(&connection, &state);
            auto_start_boot_octane_workers(&connection, &state);
            auto_resume_boot_scheduled_tasks(&connection);
            let scheduled_task_db_path = state.db_path.clone();
            let scheduled_task_workspace_dir = state.workspace_dir.clone();
            let scheduled_task_active_runs = Arc::clone(&state.managed_scheduled_task_runs);
            let scheduled_task_shutdown = Arc::clone(&state.scheduled_task_scheduler_shutdown);
            let _ = thread::Builder::new()
                .name("devnest-scheduled-tasks".to_string())
                .spawn(move || {
                    scheduled_task_manager::run_scheduler_loop(
                        scheduled_task_db_path,
                        scheduled_task_workspace_dir,
                        scheduled_task_active_runs,
                        scheduled_task_shutdown,
                    );
                });
            let scheduler_db_path = state.db_path.clone();
            let scheduler_workspace_dir = state.workspace_dir.clone();
            let _ = thread::Builder::new()
                .name("devnest-db-time-machine".to_string())
                .spawn(move || {
                    loop {
                        if let Err(error) = run_scheduled_database_snapshot_cycle(
                            &scheduler_db_path,
                            &scheduler_workspace_dir,
                        ) {
                            eprintln!("DevNest scheduled Time Machine cycle failed: {}", error);
                        }
                        thread::sleep(Duration::from_secs(60));
                    }
                });
            app.manage(state);
            tray::initialize(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            get_boot_state,
            get_workspace_overview,
            get_app_release_info,
            check_for_app_update,
            install_app_update,
            list_projects,
            get_project,
            pick_project_folder,
            scan_project,
            create_project,
            update_project,
            delete_project,
            list_project_workers,
            list_all_workers,
            list_project_scheduled_tasks,
            list_all_scheduled_tasks,
            create_project_worker,
            create_project_scheduled_task,
            update_project_worker,
            update_project_scheduled_task,
            delete_project_worker,
            delete_project_scheduled_task,
            get_project_worker_status,
            get_project_scheduled_task_status,
            start_project_worker,
            stop_project_worker,
            restart_project_worker,
            enable_project_scheduled_task,
            disable_project_scheduled_task,
            run_project_scheduled_task_now,
            list_project_scheduled_task_runs,
            read_project_worker_logs,
            read_project_scheduled_task_run_logs,
            clear_project_worker_logs,
            clear_project_scheduled_task_logs,
            clear_project_scheduled_task_history,
            export_project_profile,
            export_team_project_profile,
            import_project_profile,
            import_team_project_profile,
            list_repair_workflows,
            run_action_preflight,
            inspect_reliability_state,
            export_diagnostics_bundle,
            backup_app_metadata,
            restore_app_metadata,
            run_repair_workflow,
            open_project_folder,
            open_project_terminal,
            open_project_vscode,
            create_laravel_recipe,
            create_wordpress_recipe,
            clone_git_recipe,
            list_php_extensions,
            set_php_extension_enabled,
            install_php_extension,
            list_php_extension_packages,
            install_php_extension_package,
            remove_php_extension,
            list_php_functions,
            set_php_function_enabled,
            list_project_env_vars,
            create_project_env_var,
            update_project_env_var,
            delete_project_env_var,
            inspect_project_env,
            preview_vhost_config,
            generate_vhost_config,
            apply_hosts_entry,
            remove_hosts_entry,
            get_all_service_status,
            get_service_status,
            start_service,
            stop_service,
            restart_service,
            open_service_dashboard,
            read_service_logs,
            clear_service_logs,
            get_project_mobile_preview_state,
            start_project_mobile_preview,
            stop_project_mobile_preview,
            check_port,
            run_diagnostics,
            apply_diagnostic_fix,
            get_project_frankenphp_worker_settings,
            update_project_frankenphp_worker_settings,
            get_project_frankenphp_octane_preflight,
            get_project_frankenphp_worker_status,
            start_project_frankenphp_worker,
            stop_project_frankenphp_worker,
            restart_project_frankenphp_worker,
            reload_project_frankenphp_worker,
            read_project_frankenphp_worker_logs,
            list_databases,
            create_database,
            drop_database,
            backup_database,
            restore_database,
            get_database_time_machine_status,
            enable_database_time_machine,
            disable_database_time_machine,
            take_database_snapshot,
            list_database_snapshots,
            rollback_database_snapshot,
            list_runtime_inventory,
            list_runtime_packages,
            verify_runtime_path,
            link_runtime_path,
            import_runtime_path,
            install_runtime_package,
            get_runtime_install_task,
            remove_runtime_reference,
            reveal_runtime_path,
            set_active_runtime,
            get_runtime_config_schema,
            get_runtime_config_values,
            update_runtime_config,
            open_runtime_config_file,
            list_optional_tool_inventory,
            list_optional_tool_packages,
            install_optional_tool_package,
            get_optional_tool_install_task,
            remove_optional_tool,
            reveal_optional_tool_path,
            get_persistent_tunnel_setup_status,
            connect_persistent_tunnel_provider,
            import_persistent_tunnel_auth_cert,
            create_persistent_named_tunnel,
            import_persistent_tunnel_credentials,
            list_available_persistent_named_tunnels,
            select_persistent_named_tunnel,
            delete_persistent_named_tunnel,
            disconnect_persistent_tunnel_provider,
            update_persistent_tunnel_setup,
            get_project_persistent_hostname,
            apply_project_persistent_hostname,
            upsert_project_persistent_hostname,
            delete_project_persistent_hostname,
            remove_project_persistent_hostname,
            get_project_persistent_tunnel_state,
            publish_project_persistent_tunnel,
            start_project_persistent_tunnel,
            stop_project_persistent_tunnel,
            unpublish_project_persistent_tunnel,
            open_project_persistent_tunnel_url,
            inspect_project_persistent_tunnel_health,
            get_local_ssl_authority_status,
            trust_local_ssl_authority,
            untrust_local_ssl_authority,
            regenerate_project_ssl_certificate,
            open_project_site,
            get_project_tunnel_state,
            start_project_tunnel,
            stop_project_tunnel,
            open_project_tunnel_url
        ])
        .build(tauri::generate_context!())
        .expect("error while building DevNest");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { .. } => {
            let state = app_handle.state::<AppState>();
            cleanup_mobile_preview_sessions(&state);
            state
                .scheduled_task_scheduler_shutdown
                .store(true, Ordering::Relaxed);
            scheduled_task_manager::stop_all_project_scheduled_tasks(&state);
            worker_manager::stop_all_project_workers(&state);
        }
        RunEvent::WindowEvent { label, event, .. } => {
            if label != MAIN_WINDOW_LABEL {
                return;
            }

            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = app_handle.state::<AppState>();
                if can_exit(&state) {
                    return;
                }

                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = window.hide();
                }
            }
        }
        _ => {}
    });
}
