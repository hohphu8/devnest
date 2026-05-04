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
          server_type TEXT NOT NULL CHECK(server_type IN ('apache', 'nginx', 'frankenphp')),
          php_version TEXT NOT NULL,
          framework TEXT NOT NULL CHECK(framework IN ('laravel', 'symfony', 'wordpress', 'php', 'unknown')),
          document_root TEXT NOT NULL,
          ssl_enabled INTEGER NOT NULL DEFAULT 0,
          database_name TEXT,
          database_port INTEGER,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error')),
          frankenphp_mode TEXT NOT NULL DEFAULT 'classic' CHECK(frankenphp_mode IN ('classic', 'octane', 'symfony', 'custom')),
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
          runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'frankenphp', 'mysql')),
          version TEXT NOT NULL,
          path TEXT NOT NULL,
          is_active INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS runtime_suppressions (
          runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'frankenphp', 'mysql')),
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
          tool_type TEXT NOT NULL CHECK(tool_type IN ('mailpit', 'cloudflared', 'phpmyadmin', 'redis', 'restic')),
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

        CREATE TABLE IF NOT EXISTS project_frankenphp_octane_workers (
          project_id TEXT PRIMARY KEY NOT NULL,
          worker_port INTEGER NOT NULL,
          admin_port INTEGER NOT NULL,
          workers INTEGER NOT NULL DEFAULT 1,
          max_requests INTEGER NOT NULL DEFAULT 500,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_error TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS project_frankenphp_workers (
          project_id TEXT PRIMARY KEY NOT NULL,
          mode TEXT NOT NULL DEFAULT 'octane' CHECK(mode IN ('octane', 'symfony', 'custom')),
          worker_port INTEGER NOT NULL,
          admin_port INTEGER NOT NULL,
          workers INTEGER NOT NULL DEFAULT 1,
          max_requests INTEGER NOT NULL DEFAULT 500,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_error TEXT,
          log_path TEXT NOT NULL,
          custom_worker_relative_path TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
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
        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_octane_workers_status ON project_frankenphp_octane_workers(status);
        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_workers_status ON project_frankenphp_workers(status);
        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_workers_mode ON project_frankenphp_workers(mode);

        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES ('0001_initial_schema', CURRENT_TIMESTAMP);
        ",
    )?;

    migrate_optional_tool_versions_for_phpmyadmin(&connection)?;
    migrate_optional_tool_versions_for_redis_restic(&connection)?;
    migrate_php_function_overrides(&connection)?;
    migrate_runtime_config_overrides(&connection)?;
    migrate_project_workers(&connection)?;
    migrate_project_scheduled_tasks(&connection)?;
    migrate_frankenphp_runtime_support(&connection)?;
    migrate_repair_project_foreign_keys(&connection)?;
    migrate_frankenphp_octane_workers(&connection)?;
    migrate_frankenphp_worker_framework_expansion(&connection)?;
    ServiceRepository::seed_defaults(&connection)?;

    Ok(())
}

fn migrate_frankenphp_octane_workers(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0009_frankenphp_octane_workers";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    if !table_sql_contains(connection, "projects", "frankenphp_mode")? {
        connection.execute_batch(
            "
            ALTER TABLE projects ADD COLUMN frankenphp_mode TEXT NOT NULL DEFAULT 'classic'
            CHECK(frankenphp_mode IN ('classic', 'octane'));
            ",
        )?;
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS project_frankenphp_octane_workers (
          project_id TEXT PRIMARY KEY NOT NULL,
          worker_port INTEGER NOT NULL,
          admin_port INTEGER NOT NULL,
          workers INTEGER NOT NULL DEFAULT 1,
          max_requests INTEGER NOT NULL DEFAULT 500,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_error TEXT,
          log_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_octane_workers_status
        ON project_frankenphp_octane_workers(status);
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

fn migrate_frankenphp_worker_framework_expansion(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0010_frankenphp_worker_framework_expansion";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    let projects_sql = connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'projects'
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let projects_support_new_modes = projects_sql
        .as_deref()
        .map(|sql| sql.contains("'symfony'") && sql.contains("'custom'"))
        .unwrap_or(false);

    if !projects_support_new_modes {
        let rebuild_result = connection.execute_batch(
            "
            PRAGMA foreign_keys = OFF;
            PRAGMA legacy_alter_table = ON;

            BEGIN;

            ALTER TABLE projects RENAME TO projects_phase24_legacy;

            CREATE TABLE projects (
              id TEXT PRIMARY KEY NOT NULL,
              name TEXT NOT NULL,
              path TEXT NOT NULL UNIQUE,
              domain TEXT NOT NULL UNIQUE,
              server_type TEXT NOT NULL CHECK(server_type IN ('apache', 'nginx', 'frankenphp')),
              php_version TEXT NOT NULL,
              framework TEXT NOT NULL CHECK(framework IN ('laravel', 'symfony', 'wordpress', 'php', 'unknown')),
              document_root TEXT NOT NULL,
              ssl_enabled INTEGER NOT NULL DEFAULT 0,
              database_name TEXT,
              database_port INTEGER,
              status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error')),
              frankenphp_mode TEXT NOT NULL DEFAULT 'classic' CHECK(frankenphp_mode IN ('classic', 'octane', 'symfony', 'custom')),
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            INSERT INTO projects (
              id, name, path, domain, server_type, php_version, framework, document_root,
              ssl_enabled, database_name, database_port, status, frankenphp_mode, created_at, updated_at
            )
            SELECT
              id, name, path, domain, server_type, php_version, framework, document_root,
              ssl_enabled, database_name, database_port, status,
              CASE WHEN frankenphp_mode IN ('classic', 'octane', 'symfony', 'custom')
                THEN frankenphp_mode
                ELSE 'classic'
              END,
              created_at, updated_at
            FROM projects_phase24_legacy;

            DROP TABLE projects_phase24_legacy;
            CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
            CREATE INDEX IF NOT EXISTS idx_projects_domain ON projects(domain);

            COMMIT;
            ",
        );

        if let Err(error) = rebuild_result {
            let _ = connection.execute_batch(
                "
                ROLLBACK;
                PRAGMA legacy_alter_table = OFF;
                PRAGMA foreign_keys = ON;
                ",
            );
            return Err(error.into());
        }

        connection.execute_batch(
            "
            PRAGMA legacy_alter_table = OFF;
            PRAGMA foreign_keys = ON;
            ",
        )?;
    }

    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS project_frankenphp_workers (
          project_id TEXT PRIMARY KEY NOT NULL,
          mode TEXT NOT NULL DEFAULT 'octane' CHECK(mode IN ('octane', 'symfony', 'custom')),
          worker_port INTEGER NOT NULL,
          admin_port INTEGER NOT NULL,
          workers INTEGER NOT NULL DEFAULT 1,
          max_requests INTEGER NOT NULL DEFAULT 500,
          status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('running', 'stopped', 'error', 'starting', 'restarting')),
          pid INTEGER,
          last_started_at TEXT,
          last_stopped_at TEXT,
          last_error TEXT,
          log_path TEXT NOT NULL,
          custom_worker_relative_path TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        INSERT OR IGNORE INTO project_frankenphp_workers (
          project_id, mode, worker_port, admin_port, workers, max_requests, status, pid,
          last_started_at, last_stopped_at, last_error, log_path, custom_worker_relative_path,
          created_at, updated_at
        )
        SELECT
          project_id, 'octane', worker_port, admin_port, workers, max_requests, status, pid,
          last_started_at, last_stopped_at, last_error, log_path, NULL, created_at, updated_at
        FROM project_frankenphp_octane_workers;

        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_workers_status
        ON project_frankenphp_workers(status);

        CREATE INDEX IF NOT EXISTS idx_project_frankenphp_workers_mode
        ON project_frankenphp_workers(mode);
        ",
    )?;

    record_migration(connection, MIGRATION)?;

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

fn table_sql_contains(
    connection: &Connection,
    table_name: &str,
    needle: &str,
) -> Result<bool, AppError> {
    Ok(connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = ?1
            ",
            [table_name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .as_deref()
        .map(|sql| sql.contains(needle))
        .unwrap_or(false))
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

fn migrate_optional_tool_versions_for_redis_restic(
    connection: &Connection,
) -> Result<(), AppError> {
    const MIGRATION: &str = "0011_optional_tools_redis_restic";
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
        .map(|sql| sql.contains("'redis'") && sql.contains("'restic'"))
        .unwrap_or(false)
    {
        return record_migration(connection, MIGRATION);
    }

    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        ALTER TABLE optional_tool_versions RENAME TO optional_tool_versions_phase29_legacy;

        CREATE TABLE optional_tool_versions (
          id TEXT PRIMARY KEY NOT NULL,
          tool_type TEXT NOT NULL CHECK(tool_type IN ('mailpit', 'cloudflared', 'phpmyadmin', 'redis', 'restic')),
          version TEXT NOT NULL,
          path TEXT NOT NULL,
          is_active INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        INSERT INTO optional_tool_versions (id, tool_type, version, path, is_active, created_at, updated_at)
        SELECT id, tool_type, version, path, is_active, created_at, updated_at
        FROM optional_tool_versions_phase29_legacy;

        DROP TABLE optional_tool_versions_phase29_legacy;
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

fn migrate_frankenphp_runtime_support(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0007_frankenphp_runtime_support";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    let projects_sql = connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'projects'
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let runtimes_sql = connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'runtime_versions'
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let suppressions_sql = connection
        .query_row(
            "
            SELECT sql
            FROM sqlite_master
            WHERE type = 'table' AND name = 'runtime_suppressions'
            ",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    let already_supported = projects_sql
        .as_deref()
        .map(|sql| sql.contains("'frankenphp'"))
        .unwrap_or(false)
        && runtimes_sql
            .as_deref()
            .map(|sql| sql.contains("'frankenphp'"))
            .unwrap_or(false)
        && suppressions_sql
            .as_deref()
            .map(|sql| sql.contains("'frankenphp'"))
            .unwrap_or(false);

    if !already_supported {
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(
            "
            ALTER TABLE projects RENAME TO projects_legacy;

            CREATE TABLE projects (
              id TEXT PRIMARY KEY NOT NULL,
              name TEXT NOT NULL,
              path TEXT NOT NULL UNIQUE,
              domain TEXT NOT NULL UNIQUE,
              server_type TEXT NOT NULL CHECK(server_type IN ('apache', 'nginx', 'frankenphp')),
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

            INSERT INTO projects (
              id, name, path, domain, server_type, php_version, framework, document_root,
              ssl_enabled, database_name, database_port, status, created_at, updated_at
            )
            SELECT
              id, name, path, domain, server_type, php_version, framework, document_root,
              ssl_enabled, database_name, database_port, status, created_at, updated_at
            FROM projects_legacy;

            DROP TABLE projects_legacy;
            CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
            CREATE INDEX IF NOT EXISTS idx_projects_domain ON projects(domain);

            ALTER TABLE runtime_versions RENAME TO runtime_versions_legacy;

            CREATE TABLE runtime_versions (
              id TEXT PRIMARY KEY NOT NULL,
              runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'frankenphp', 'mysql')),
              version TEXT NOT NULL,
              path TEXT NOT NULL,
              is_active INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            INSERT INTO runtime_versions (id, runtime_type, version, path, is_active, created_at, updated_at)
            SELECT id, runtime_type, version, path, is_active, created_at, updated_at
            FROM runtime_versions_legacy;

            DROP TABLE runtime_versions_legacy;
            CREATE INDEX IF NOT EXISTS idx_runtime_versions_runtime_type ON runtime_versions(runtime_type);

            ALTER TABLE runtime_suppressions RENAME TO runtime_suppressions_legacy;

            CREATE TABLE runtime_suppressions (
              runtime_type TEXT NOT NULL CHECK(runtime_type IN ('php', 'apache', 'nginx', 'frankenphp', 'mysql')),
              path_key TEXT NOT NULL,
              created_at TEXT NOT NULL,
              PRIMARY KEY (runtime_type, path_key)
            );

            INSERT INTO runtime_suppressions (runtime_type, path_key, created_at)
            SELECT runtime_type, path_key, created_at
            FROM runtime_suppressions_legacy;

            DROP TABLE runtime_suppressions_legacy;
            CREATE INDEX IF NOT EXISTS idx_runtime_suppressions_runtime_type ON runtime_suppressions(runtime_type);
            ",
        )?;
        transaction.execute(
            "
            INSERT OR IGNORE INTO schema_migrations (version, applied_at)
            VALUES (?1, CURRENT_TIMESTAMP)
            ",
            params![MIGRATION],
        )?;
        transaction.commit()?;
    } else {
        record_migration(connection, MIGRATION)?;
    }

    connection.execute(
        "
        INSERT OR IGNORE INTO services (
          name, enabled, auto_start, port, pid, status, last_error, updated_at
        )
        VALUES ('frankenphp', 1, 0, 80, NULL, 'stopped', NULL, CURRENT_TIMESTAMP)
        ",
        [],
    )?;

    Ok(())
}

fn migrate_repair_project_foreign_keys(connection: &Connection) -> Result<(), AppError> {
    const MIGRATION: &str = "0008_repair_project_foreign_keys";
    if migration_applied(connection, MIGRATION)? {
        return Ok(());
    }

    let affected_tables = [
        "project_env_vars",
        "diagnostic_reports",
        "project_persistent_hostnames",
        "project_workers",
        "project_scheduled_tasks",
        "project_scheduled_task_runs",
    ];
    let needs_repair = affected_tables
        .iter()
        .map(|table| table_sql_contains(connection, table, "projects_legacy"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .any(|affected| affected);

    if !needs_repair {
        return record_migration(connection, MIGRATION);
    }

    connection.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        BEGIN;

        ALTER TABLE project_env_vars RENAME TO project_env_vars_fk_repair_legacy;
        CREATE TABLE project_env_vars (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          env_key TEXT NOT NULL,
          env_value TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        INSERT INTO project_env_vars (id, project_id, env_key, env_value, created_at, updated_at)
        SELECT id, project_id, env_key, env_value, created_at, updated_at
        FROM project_env_vars_fk_repair_legacy;
        DROP TABLE project_env_vars_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_project_env_vars_project_id
        ON project_env_vars(project_id);

        ALTER TABLE diagnostic_reports RENAME TO diagnostic_reports_fk_repair_legacy;
        CREATE TABLE diagnostic_reports (
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
        INSERT INTO diagnostic_reports (
          id, project_id, level, code, title, message, suggestion, created_at
        )
        SELECT id, project_id, level, code, title, message, suggestion, created_at
        FROM diagnostic_reports_fk_repair_legacy;
        DROP TABLE diagnostic_reports_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_diagnostic_reports_project_id
        ON diagnostic_reports(project_id);

        ALTER TABLE project_persistent_hostnames RENAME TO project_persistent_hostnames_fk_repair_legacy;
        CREATE TABLE project_persistent_hostnames (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL UNIQUE,
          provider TEXT NOT NULL CHECK(provider IN ('cloudflared')),
          hostname TEXT NOT NULL UNIQUE,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        INSERT INTO project_persistent_hostnames (
          id, project_id, provider, hostname, created_at, updated_at
        )
        SELECT id, project_id, provider, hostname, created_at, updated_at
        FROM project_persistent_hostnames_fk_repair_legacy;
        DROP TABLE project_persistent_hostnames_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_project_persistent_hostnames_project_id
        ON project_persistent_hostnames(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_persistent_hostnames_hostname
        ON project_persistent_hostnames(hostname);

        ALTER TABLE project_workers RENAME TO project_workers_fk_repair_legacy;
        CREATE TABLE project_workers (
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
        INSERT INTO project_workers (
          id, project_id, name, preset_type, command, args_json, working_directory,
          auto_start, status, pid, last_started_at, last_stopped_at, last_exit_code,
          last_error, log_path, created_at, updated_at
        )
        SELECT
          id, project_id, name, preset_type, command, args_json, working_directory,
          auto_start, status, pid, last_started_at, last_stopped_at, last_exit_code,
          last_error, log_path, created_at, updated_at
        FROM project_workers_fk_repair_legacy;
        DROP TABLE project_workers_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_project_workers_project_id
        ON project_workers(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_workers_status
        ON project_workers(status);
        CREATE INDEX IF NOT EXISTS idx_project_workers_auto_start
        ON project_workers(auto_start);

        ALTER TABLE project_scheduled_tasks RENAME TO project_scheduled_tasks_fk_repair_legacy;
        CREATE TABLE project_scheduled_tasks (
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
        INSERT INTO project_scheduled_tasks (
          id, project_id, name, task_type, schedule_mode, simple_schedule_kind,
          schedule_expression, interval_seconds, daily_time, weekly_day, url,
          command, args_json, working_directory, enabled, auto_resume,
          overlap_policy, status, next_run_at, last_run_at, last_success_at,
          last_error, created_at, updated_at
        )
        SELECT
          id, project_id, name, task_type, schedule_mode, simple_schedule_kind,
          schedule_expression, interval_seconds, daily_time, weekly_day, url,
          command, args_json, working_directory, enabled, auto_resume,
          overlap_policy, status, next_run_at, last_run_at, last_success_at,
          last_error, created_at, updated_at
        FROM project_scheduled_tasks_fk_repair_legacy;
        DROP TABLE project_scheduled_tasks_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_project_id
        ON project_scheduled_tasks(project_id);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_enabled
        ON project_scheduled_tasks(enabled);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_tasks_next_run_at
        ON project_scheduled_tasks(next_run_at);

        ALTER TABLE project_scheduled_task_runs RENAME TO project_scheduled_task_runs_fk_repair_legacy;
        CREATE TABLE project_scheduled_task_runs (
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
        INSERT INTO project_scheduled_task_runs (
          id, task_id, project_id, started_at, finished_at, duration_ms,
          status, exit_code, response_status, error_message, log_path, created_at
        )
        SELECT
          id, task_id, project_id, started_at, finished_at, duration_ms,
          status, exit_code, response_status, error_message, log_path, created_at
        FROM project_scheduled_task_runs_fk_repair_legacy;
        DROP TABLE project_scheduled_task_runs_fk_repair_legacy;
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_task_id
        ON project_scheduled_task_runs(task_id);
        CREATE INDEX IF NOT EXISTS idx_project_scheduled_task_runs_project_id
        ON project_scheduled_task_runs(project_id);

        INSERT OR IGNORE INTO schema_migrations (version, applied_at)
        VALUES ('0008_repair_project_foreign_keys', CURRENT_TIMESTAMP);

        COMMIT;

        PRAGMA foreign_keys = ON;
        ",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::init_database;
    use rusqlite::{Connection, params};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_db_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("devnest-{name}-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn optional_tool_migration_preserves_rows_and_accepts_redis_restic() {
        let db_path = temp_db_path("optional-tools-phase29");
        let connection = Connection::open(&db_path).expect("legacy db should open");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version TEXT PRIMARY KEY NOT NULL,
                    applied_at TEXT NOT NULL
                );

                CREATE TABLE optional_tool_versions (
                    id TEXT PRIMARY KEY NOT NULL,
                    tool_type TEXT NOT NULL CHECK(tool_type IN ('mailpit', 'cloudflared')),
                    version TEXT NOT NULL,
                    path TEXT NOT NULL,
                    is_active INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                INSERT INTO schema_migrations (version, applied_at)
                VALUES ('0001_initial_schema', CURRENT_TIMESTAMP);

                INSERT INTO optional_tool_versions (
                    id, tool_type, version, path, is_active, created_at, updated_at
                )
                VALUES (
                    'mailpit-1.29.7', 'mailpit', '1.29.7',
                    'D:\\devnest\\optional-tools\\mailpit.exe', 1,
                    CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
                );
                ",
            )
            .expect("legacy optional tool schema should seed");
        drop(connection);

        init_database(&db_path).expect("database migrations should run");

        let connection = Connection::open(&db_path).expect("migrated db should open");
        let mailpit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM optional_tool_versions WHERE tool_type = 'mailpit'",
                [],
                |row| row.get(0),
            )
            .expect("legacy row should be queryable");
        assert_eq!(mailpit_count, 1);

        for tool_type in ["redis", "restic"] {
            connection
                .execute(
                    "
                    INSERT INTO optional_tool_versions (
                        id, tool_type, version, path, is_active, created_at, updated_at
                    )
                    VALUES (?1, ?2, '1.0.0', 'D:\\devnest\\optional-tools\\tool.exe', 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                    ",
                    params![format!("{tool_type}-1.0.0"), tool_type],
                )
                .expect("phase 29 optional tool type should be accepted");
        }

        fs::remove_file(db_path).ok();
    }
}
