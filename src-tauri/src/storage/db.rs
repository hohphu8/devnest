use crate::error::AppError;
use crate::storage::repositories::ServiceRepository;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::path::Path;

pub fn init_database(db_path: &Path) -> Result<(), AppError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(db_path)?;
    connection.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY NOT NULL,
            applied_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
          id TEXT PRIMARY KEY NOT NULL,
          name TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          domain TEXT NOT NULL UNIQUE,
          server_type TEXT NOT NULL CHECK(server_type IN ('apache', 'nginx')),
          php_version TEXT NOT NULL,
          framework TEXT NOT NULL CHECK(framework IN ('laravel', 'wordpress', 'php', 'unknown')),
          document_root TEXT NOT NULL,
          ssl_enabled INTEGER NOT NULL DEFAULT 0,
          database_name TEXT,
          database_port INTEGER,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error')),
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS services (
          name TEXT PRIMARY KEY NOT NULL,
          enabled INTEGER NOT NULL DEFAULT 1,
          auto_start INTEGER NOT NULL DEFAULT 0,
          port INTEGER,
          pid INTEGER,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error')),
          last_error TEXT,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS project_env_vars (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          env_key TEXT NOT NULL,
          env_value TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS diagnostic_reports (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          level TEXT NOT NULL CHECK(level IN ('info', 'warning', 'error')),
          code TEXT NOT NULL,
          title TEXT NOT NULL,
          message TEXT NOT NULL,
          suggestion TEXT,
          created_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS runtime_versions (
          id TEXT PRIMARY KEY NOT NULL,
          runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'mysql')),
          version TEXT NOT NULL,
          path TEXT NOT NULL,
          is_active INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS runtime_suppressions (
          runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'mysql')),
          path_key TEXT NOT NULL,
          created_at TEXT NOT NULL,
          PRIMARY KEY (runtime_type, path_key)
        );

        CREATE TABLE IF NOT EXISTS php_extension_overrides (
          runtime_id TEXT NOT NULL,
          extension_name TEXT NOT NULL,
          enabled INTEGER NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (runtime_id, extension_name),
          FOREIGN KEY(runtime_id) REFERENCES runtime_versions(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS optional_tool_versions (
          id TEXT PRIMARY KEY NOT NULL,
          tool_type TEXT NOT NULL CHECK(tool_type IN ('mailpit', 'cloudflared', 'phpmyadmin')),
          version TEXT NOT NULL,
          path TEXT NOT NULL,
          is_active INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS project_persistent_hostnames (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL UNIQUE,
          provider TEXT NOT NULL CHECK(provider IN ('cloudflared')),
          hostname TEXT NOT NULL UNIQUE,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS persistent_tunnel_setups (
          provider TEXT PRIMARY KEY NOT NULL CHECK(provider IN ('cloudflared')),
          auth_cert_path TEXT,
          credentials_path TEXT,
          tunnel_id TEXT,
          tunnel_name TEXT,
          default_hostname_zone TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS runtime_config_overrides (
          runtime_id TEXT NOT NULL,
          config_key TEXT NOT NULL,
          config_value TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (runtime_id, config_key),
          FOREIGN KEY(runtime_id) REFERENCES runtime_versions(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS project_workers (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          name TEXT NOT NULL,
          preset_type TEXT NOT NULL CHECK(preset_type IN ('queue', 'schedule', 'custom')),
          command TEXT NOT NULL,
          args_json TEXT NOT NULL,
          working_directory TEXT NOT NULL,
          auto_start INTEGER NOT NULL DEFAULT 0,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_exit_code INTEGER,
          last_error TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS project_scheduled_tasks (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          name TEXT NOT NULL,
          task_type TEXT NOT NULL CHECK(task_type IN ('command', 'url')),
          schedule_mode TEXT NOT NULL CHECK(schedule_mode IN ('simple', 'cron')),
          simple_schedule_kind TEXT CHECK(simple_schedule_kind IN ('everySeconds', 'everyMinutes', 'everyHours', 'daily', 'weekly')),
          schedule_expression TEXT NOT NULL,
          interval_seconds INTEGER,
          daily_time TEXT,
          weekly_day INTEGER,
          url TEXT,
          command TEXT,
          args_json TEXT NOT NULL,
          working_directory TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          auto_resume INTEGER NOT NULL DEFAULT 1,
          overlap_policy TEXT NOT NULL DEFAULT 'skip_if_running' CHECK(overlap_policy IN ('skip_if_running')),
          status TEXT NOT NULL DEFAULT 'idle' CHECK(status IN ('idle', 'scheduled', 'running', 'success', 'error', 'skipped')),
          next_run_at TEXT,
          last_run_at TEXT,
          last_success_at TEXT,
          last_error TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS project_scheduled_task_runs (
          id TEXT PRIMARY KEY NOT NULL,
          task_id TEXT NOT NULL,
          project_id TEXT NOT NULL,
          started_at TEXT NOT NULL,
          finished_at TEXT,
          duration_ms INTEGER,
          status TEXT NOT NULL CHECK(status IN ('running', 'success', 'error', 'skipped')),
          exit_code INTEGER,
          response_status INTEGER,
          error_message TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          FOREIGN KEY(task_id) REFERENCES project_scheduled_tasks(id) ON DELETE CASCADE,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
        CREATE INDEX IF NOT EXISTS idx_projects_domain ON projects(domain);
        CREATE INDEX IF NOT EXISTS idx_project_env_vars_project_id ON project_env_vars(project_id);
        CREATE INDEX IF NOT EXISTS idx_diagnostic_reports_project_id ON diagnostic_reports(project_id);
        CREATE INDEX IF NOT EXISTS idx_runtime_versions_runtime_type ON runtime_versions(runtime_type);
        CREATE INDEX IF NOT EXISTS idx_runtime_suppressions_runtime_type ON runtime_suppressions(runtime_type);
        CREATE INDEX IF NOT EXISTS idx_php_extension_overrides_runtime_id ON php_extension_overrides(runtime_id);
        CREATE INDEX IF NOT EXISTS idx_optional_tool_versions_tool_type ON optional_tool_versions(tool_type);
        CREATE INDEX IF NOT EXISTS idx_project_persistent_hostnames_project_id ON project_persistent_hostnames(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_persistent_hostnames_hostname ON project_persistent_hostnames(hostname);
        CREATE INDEX IF NOT EXISTS idx_persistent_tunnel_setups_provider ON persistent_tunnel_setups(provider);
        CREATE INDEX IF NOT EXISTS idx_runtime_config_overrides_runtime_id ON runtime_config_overrides(runtime_id);
        CREATE INDEX IF NOT EXISTS idx_project_workers_project_id ON project_workers(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_workers_status ON project_workers(status);
        CREATE INDEX IF NOT EXISTS idx_project_workers_auto_start ON project_workers(auto_start);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_project_id ON project_scheduled_tasks(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_enabled ON project_scheduled_tasks(enabled);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_next_run_at ON project_scheduled_tasks(next_run_at);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_task_id ON project_scheduled_task_runs(task_id);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_project_id ON project_scheduled_task_runs(project_id);

        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES ('0001_initial_schema', CURRENT_TIMESTAMP);
        ",
    )?;

    migrate_optional_tool_versions_for_phpmyadmin(&connection)?;
    migrate_php_function_overrides(&connection)?;
    migrate_runtime_config_overrides(&connection)?;
    migrate_project_workers(&connection)?;
    migrate_project_scheduled_tasks(&connection)?;
    ServiceRepository::seed_defaults(&connection)?;

    Ok(())
}

fn migration_applied(connection: &Connection, version: &str) -> Result<bool, AppError> {
    Ok(connection
        .query_row(
            "SELECT version FROM schema_migrations WHERE version = ?1 LIMIT 1",
            [version],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

fn record_migration(connection: &Connection, version: &str) -> Result<(), AppError> {
    connection.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        [version],
    )?;

    Ok(())
}

fn migrate_optional_tool_versions_for_phpmyadmin(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0002_optional_tools_phpmyadmin";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    let current_sql = connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'optional_tool_versions'
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if current_sql
        .as_deref()
        .map(|sql| sql.contains("'phpmyadmin'"))
        .unwrap_or(false)
    {
        return record_migration(connection, MIGRATION);
    }

    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        ALTER TABLE optional_tool_versions RENAME TO optional_tool_versions_legacy;

        CREATE TABLE optional_tool_versions (
          id TEXT PRIMARY KEY NOT NULL,
          tool_type TEXT NOT NULL CHECK(tool_type IN ('mailpit', 'cloudflared', 'phpmyadmin')),
          version TEXT NOT NULL,
          path TEXT NOT NULL,
          is_active INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        INSERT INTO optional_tool_versions (id, tool_type, version, path, is_active, created_at, updated_at)
        SELECT id, tool_type, version, path, is_active, created_at, updated_at
        FROM optional_tool_versions_legacy;

        DROP TABLE optional_tool_versions_legacy;
        CREATE INDEX IF NOT EXISTS idx_optional_tool_versions_tool_type ON optional_tool_versions(tool_type);
        ",
    )?;
    transaction.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        [MIGRATION],
    )?;
    transaction.commit()?;

    Ok(())
}

fn migrate_php_function_overrides(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0003_php_function_overrides";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS php_function_overrides (
          runtime_id TEXT NOT NULL,
          function_name TEXT NOT NULL,
          enabled INTEGER NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (runtime_id, function_name),
          FOREIGN KEY(runtime_id) REFERENCES runtime_versions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_php_function_overrides_runtime_id
        ON php_function_overrides(runtime_id);
        ",
    )?;

    connection.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        params![MIGRATION],
    )?;

    Ok(())
}

fn migrate_runtime_config_overrides(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0004_runtime_config_overrides";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS runtime_config_overrides (
          runtime_id TEXT NOT NULL,
          config_key TEXT NOT NULL,
          config_value TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (runtime_id, config_key),
          FOREIGN KEY(runtime_id) REFERENCES runtime_versions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_runtime_config_overrides_runtime_id
        ON runtime_config_overrides(runtime_id);
        ",
    )?;

    connection.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        params![MIGRATION],
    )?;

    Ok(())
}

fn migrate_project_workers(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0005_project_workers";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS project_workers (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          name TEXT NOT NULL,
          preset_type TEXT NOT NULL CHECK(preset_type IN ('queue', 'schedule', 'custom')),
          command TEXT NOT NULL,
          args_json TEXT NOT NULL,
          working_directory TEXT NOT NULL,
          auto_start INTEGER NOT NULL DEFAULT 0,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_exit_code INTEGER,
          last_error TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_project_workers_project_id
        ON project_workers(project_id);

        CREATE INDEX IF NOT EXISTS idx_project_workers_status
        ON project_workers(status);

        CREATE INDEX IF NOT EXISTS idx_project_workers_auto_start
        ON project_workers(auto_start);
        ",
    )?;

    connection.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        params![MIGRATION],
    )?;

    Ok(())
}

fn migrate_project_scheduled_tasks(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0006_project_scheduled_tasks";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS project_scheduled_tasks (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          name TEXT NOT NULL,
          task_type TEXT NOT NULL CHECK(task_type IN ('command', 'url')),
          schedule_mode TEXT NOT NULL CHECK(schedule_mode IN ('simple', 'cron')),
          simple_schedule_kind TEXT CHECK(simple_schedule_kind IN ('everySeconds', 'everyMinutes', 'everyHours', 'daily', 'weekly')),
          schedule_expression TEXT NOT NULL,
          interval_seconds INTEGER,
          daily_time TEXT,
          weekly_day INTEGER,
          url TEXT,
          command TEXT,
          args_json TEXT NOT NULL,
          working_directory TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          auto_resume INTEGER NOT NULL DEFAULT 1,
          overlap_policy TEXT NOT NULL DEFAULT 'skip_if_running' CHECK(overlap_policy IN ('skip_if_running')),
          status TEXT NOT NULL DEFAULT 'idle' CHECK(status IN ('idle', 'scheduled', 'running', 'success', 'error', 'skipped')),
          next_run_at TEXT,
          last_run_at TEXT,
          last_success_at TEXT,
          last_error TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS project_scheduled_task_runs (
          id TEXT PRIMARY KEY NOT NULL,
          task_id TEXT NOT NULL,
          project_id TEXT NOT NULL,
          started_at TEXT NOT NULL,
          finished_at TEXT,
          duration_ms INTEGER,
          status TEXT NOT NULL CHECK(status IN ('running', 'success', 'error', 'skipped')),
          exit_code INTEGER,
          response_status INTEGER,
          error_message TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          FOREIGN KEY(task_id) REFERENCES project_scheduled_tasks(id) ON DELETE CASCADE,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_project_id
        ON project_scheduled_tasks(project_id);

        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_enabled
        ON project_scheduled_tasks(enabled);

        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_next_run_at
        ON project_scheduled_tasks(next_run_at);

        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_task_id
        ON project_scheduled_task_runs(task_id);

        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_project_id
        ON project_scheduled_task_runs(project_id);
        ",
    )?;

    connection.execute(
        "
        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES (?1, CURRENT_TIMESTAMP)
        ",
        params![MIGRATION],
    )?;

    Ok(())
}
