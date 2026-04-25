use crate::core::runtime_registry::php_extension_enabled_by_default;
use crate::error::AppError;
use crate::models::optional_tool::{OptionalToolType, OptionalToolVersion};
use crate::models::persistent_tunnel::{
    PersistentTunnelManagedSetup, PersistentTunnelProvider, ProjectPersistentHostname,
};
use crate::models::project::{
    CreateProjectInput, FrameworkType, FrankenphpMode, Project, ProjectStatus, ServerType,
    UpdateProjectPatch,
};
use crate::models::project_env_var::{
    CreateProjectEnvVarInput, ProjectEnvVar, UpdateProjectEnvVarInput,
};
use crate::models::runtime::{PhpExtensionState, PhpFunctionState, RuntimeType, RuntimeVersion};
use crate::models::service::{ServiceName, ServiceState, ServiceStatus};
use rusqlite::{Connection, Error as SqlError, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::path::{Component, Path};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn now_iso() -> Result<String, AppError> {
    OffsetDateTime::now_utc().format(&Rfc3339).map_err(|error| {
        AppError::with_details(
            "TIME_FORMAT_FAILED",
            "Could not format timestamp.",
            error.to_string(),
        )
    })
}

fn parse_server_type(value: &str) -> Result<ServerType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SERVER_TYPE",
            "Stored project server type is invalid.",
        )
    })
}

fn parse_framework(value: &str) -> Result<FrameworkType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_FRAMEWORK_TYPE",
            "Stored project framework type is invalid.",
        )
    })
}

fn parse_project_status(value: &str) -> Result<ProjectStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_PROJECT_STATUS",
            "Stored project status is invalid.",
        )
    })
}

fn parse_frankenphp_mode(value: &str) -> Result<FrankenphpMode, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_FRANKENPHP_MODE",
            "Stored FrankenPHP mode is invalid.",
        )
    })
}

fn parse_service_name(value: &str) -> Result<ServiceName, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation("INVALID_SERVICE_NAME", "Stored service name is invalid.")
    })
}

fn parse_service_status(value: &str) -> Result<ServiceStatus, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_SERVICE_STATUS",
            "Stored service status is invalid.",
        )
    })
}

fn parse_runtime_type(value: &str) -> Result<RuntimeType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation("INVALID_RUNTIME_TYPE", "Stored runtime type is invalid.")
    })
}

fn parse_optional_tool_type(value: &str) -> Result<OptionalToolType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_OPTIONAL_TOOL_TYPE",
            "Stored optional tool type is invalid.",
        )
    })
}

fn parse_persistent_tunnel_provider(value: &str) -> Result<PersistentTunnelProvider, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_PERSISTENT_TUNNEL_PROVIDER",
            "Stored persistent tunnel provider is invalid.",
        )
    })
}

fn map_project_row(row: &Row<'_>) -> Result<Project, AppError> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        path: row.get("path")?,
        domain: row.get("domain")?,
        server_type: parse_server_type(&row.get::<_, String>("server_type")?)?,
        php_version: row.get("php_version")?,
        framework: parse_framework(&row.get::<_, String>("framework")?)?,
        document_root: row.get("document_root")?,
        ssl_enabled: row.get::<_, i64>("ssl_enabled")? == 1,
        database_name: row.get("database_name")?,
        database_port: row.get("database_port")?,
        status: parse_project_status(&row.get::<_, String>("status")?)?,
        frankenphp_mode: parse_frankenphp_mode(
            &row.get::<_, Option<String>>("frankenphp_mode")?
                .unwrap_or_else(|| "classic".to_string()),
        )?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_service_row(row: &Row<'_>) -> Result<ServiceState, AppError> {
    Ok(ServiceState {
        name: parse_service_name(&row.get::<_, String>("name")?)?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        auto_start: row.get::<_, i64>("auto_start")? == 1,
        port: row.get("port")?,
        pid: row.get("pid")?,
        status: parse_service_status(&row.get::<_, String>("status")?)?,
        last_error: row.get("last_error")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_runtime_row(row: &Row<'_>) -> Result<RuntimeVersion, AppError> {
    Ok(RuntimeVersion {
        id: row.get("id")?,
        runtime_type: parse_runtime_type(&row.get::<_, String>("runtime_type")?)?,
        version: row.get("version")?,
        path: row.get("path")?,
        is_active: row.get::<_, i64>("is_active")? == 1,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_optional_tool_row(row: &Row<'_>) -> Result<OptionalToolVersion, AppError> {
    Ok(OptionalToolVersion {
        id: row.get("id")?,
        tool_type: parse_optional_tool_type(&row.get::<_, String>("tool_type")?)?,
        version: row.get("version")?,
        path: row.get("path")?,
        is_active: row.get::<_, i64>("is_active")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_project_env_var_row(row: &Row<'_>) -> Result<ProjectEnvVar, AppError> {
    Ok(ProjectEnvVar {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        env_key: row.get("env_key")?,
        env_value: row.get("env_value")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_project_persistent_hostname_row(
    row: &Row<'_>,
) -> Result<ProjectPersistentHostname, AppError> {
    Ok(ProjectPersistentHostname {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        provider: parse_persistent_tunnel_provider(&row.get::<_, String>("provider")?)?,
        hostname: row.get("hostname")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_persistent_tunnel_setup_row(
    row: &Row<'_>,
) -> Result<PersistentTunnelManagedSetup, AppError> {
    Ok(PersistentTunnelManagedSetup {
        provider: parse_persistent_tunnel_provider(&row.get::<_, String>("provider")?)?,
        auth_cert_path: row.get("auth_cert_path")?,
        credentials_path: row.get("credentials_path")?,
        tunnel_id: row.get("tunnel_id")?,
        tunnel_name: row.get("tunnel_name")?,
        default_hostname_zone: row.get("default_hostname_zone")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn normalize_runtime_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim()
        .to_ascii_lowercase()
}

fn map_sql_error(error: SqlError) -> AppError {
    match error {
        SqlError::SqliteFailure(_, Some(message)) if message.contains("projects.path") => {
            AppError::new_validation(
                "PROJECT_PATH_EXISTS",
                "A project with this path already exists.",
            )
        }
        SqlError::SqliteFailure(_, Some(message)) if message.contains("projects.domain") => {
            AppError::new_validation(
                "DOMAIN_ALREADY_EXISTS",
                "A project with this domain already exists.",
            )
        }
        other => AppError::from(other),
    }
}

fn validate_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();

    if trimmed.len() < 2 || trimmed.len() > 80 {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_NAME",
            "Project name must be between 2 and 80 characters.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_domain(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_lowercase();

    if normalized.len() < 3 || normalized.len() > 120 {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_DOMAIN",
            "Project domain must be between 3 and 120 characters.",
        ));
    }

    let labels = normalized.split('.').collect::<Vec<_>>();
    let valid = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        });

    if !valid {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_DOMAIN",
            "Project domain must look like a valid local domain.",
        ));
    }

    Ok(normalized)
}

fn validate_public_hostname(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().trim_end_matches('.').to_ascii_lowercase();

    if normalized.len() < 3 || normalized.len() > 255 {
        return Err(AppError::new_validation(
            "INVALID_PERSISTENT_HOSTNAME",
            "Persistent hostname must be between 3 and 255 characters.",
        ));
    }

    let labels = normalized.split('.').collect::<Vec<_>>();
    let valid = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        });

    if !valid {
        return Err(AppError::new_validation(
            "INVALID_PERSISTENT_HOSTNAME",
            "Persistent hostname must look like a valid public hostname.",
        ));
    }

    Ok(normalized)
}

fn validate_public_hostname_zone(value: &str) -> Result<String, AppError> {
    let normalized = validate_public_hostname(value)?;
    if normalized.starts_with("*.") {
        return Err(AppError::new_validation(
            "INVALID_PERSISTENT_TUNNEL_ZONE",
            "Persistent tunnel zone must be a base domain like example.com, not a wildcard hostname.",
        ));
    }

    Ok(normalized)
}

fn validate_project_path(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let path = Path::new(trimmed);

    if trimmed.is_empty() || !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_PATH",
            "Project path does not exist or is not a directory.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_php_version(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "INVALID_PHP_VERSION",
            "PHP version is required.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_document_root(project_path: &Path, value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    let document_root = Path::new(trimmed);

    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "INVALID_DOCUMENT_ROOT",
            "Document root is required.",
        ));
    }

    for component in document_root.components() {
        if matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        ) {
            return Err(AppError::new_validation(
                "INVALID_DOCUMENT_ROOT",
                "Document root must stay inside the project path.",
            ));
        }
    }

    let joined = if trimmed == "." {
        project_path.to_path_buf()
    } else {
        project_path.join(document_root)
    };

    if !joined.exists() || !joined.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_DOCUMENT_ROOT",
            "Document root must point to an existing directory inside the project path.",
        ));
    }

    Ok(trimmed.replace('\\', "/"))
}

fn normalize_database_name(value: Option<String>) -> Option<String> {
    value.and_then(|name| {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn validate_database_port(value: Option<i64>) -> Result<Option<i64>, AppError> {
    match value {
        Some(port) if (1..=65535).contains(&port) => Ok(Some(port)),
        Some(_) => Err(AppError::new_validation(
            "INVALID_DATABASE_PORT",
            "Database port must be between 1 and 65535.",
        )),
        None => Ok(None),
    }
}

fn validate_frankenphp_mode(
    server_type: &ServerType,
    framework: &FrameworkType,
    mode: FrankenphpMode,
) -> Result<FrankenphpMode, AppError> {
    if matches!(mode, FrankenphpMode::Octane)
        && (!matches!(server_type, ServerType::Frankenphp)
            || !matches!(framework, FrameworkType::Laravel))
    {
        return Err(AppError::new_validation(
            "INVALID_FRANKENPHP_MODE",
            "Laravel Octane Worker mode is only available for Laravel projects using FrankenPHP.",
        ));
    }

    Ok(mode)
}

fn validate_env_key(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim().to_ascii_uppercase();

    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::new_validation(
            "INVALID_ENV_KEY",
            "Environment key must be between 1 and 64 characters.",
        ));
    }

    let mut characters = trimmed.chars();
    let Some(first) = characters.next() else {
        return Err(AppError::new_validation(
            "INVALID_ENV_KEY",
            "Environment key is required.",
        ));
    };

    if !first.is_ascii_alphabetic() {
        return Err(AppError::new_validation(
            "INVALID_ENV_KEY",
            "Environment key must start with a letter.",
        ));
    }

    if !characters.all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(AppError::new_validation(
            "INVALID_ENV_KEY",
            "Environment key may only contain uppercase letters, numbers, and underscores.",
        ));
    }

    Ok(trimmed)
}

fn validate_env_value(value: &str) -> Result<String, AppError> {
    if value.len() > 4000 {
        return Err(AppError::new_validation(
            "INVALID_ENV_VALUE",
            "Environment value must stay under 4000 characters.",
        ));
    }

    Ok(value.to_string())
}

pub struct ProjectRepository;

impl ProjectRepository {
    pub fn list(connection: &Connection) -> Result<Vec<Project>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM projects
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            map_project_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut projects = Vec::new();

        for row in rows {
            match row {
                Ok(project) => projects.push(project),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(projects)
    }

    pub fn get(connection: &Connection, project_id: &str) -> Result<Project, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM projects
            WHERE id = ?1
            ",
        )?;

        let project = statement
            .query_row([project_id], |row| {
                map_project_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        project.ok_or_else(|| AppError::new_validation("PROJECT_NOT_FOUND", "Project not found."))
    }

    pub fn create(connection: &Connection, input: CreateProjectInput) -> Result<Project, AppError> {
        let name = validate_name(&input.name)?;
        let path = validate_project_path(&input.path)?;
        let domain = validate_domain(&input.domain)?;
        let php_version = validate_php_version(&input.php_version)?;
        let document_root = validate_document_root(Path::new(&path), &input.document_root)?;
        let database_name = normalize_database_name(input.database_name);
        let database_port = validate_database_port(input.database_port)?;
        let frankenphp_mode = validate_frankenphp_mode(
            &input.server_type,
            &input.framework,
            input.frankenphp_mode.unwrap_or(FrankenphpMode::Classic),
        )?;
        let timestamp = now_iso()?;
        let project_id = Uuid::new_v4().to_string();

        connection
            .execute(
                "
                INSERT INTO projects (
                    id, name, path, domain, server_type, php_version, framework, document_root,
                    ssl_enabled, database_name, database_port, status, frankenphp_mode, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'stopped', ?12, ?13, ?13)
                ",
                params![
                    project_id,
                    name,
                    path,
                    domain,
                    input.server_type.as_str(),
                    php_version,
                    input.framework.as_str(),
                    document_root,
                    if input.ssl_enabled { 1 } else { 0 },
                    database_name,
                    database_port,
                    frankenphp_mode.as_str(),
                    timestamp,
                ],
            )
            .map_err(map_sql_error)?;

        Self::get(connection, &project_id)
    }

    pub fn update(
        connection: &Connection,
        project_id: &str,
        patch: UpdateProjectPatch,
    ) -> Result<Project, AppError> {
        let current = Self::get(connection, project_id)?;
        let timestamp = now_iso()?;
        let project_path = validate_project_path(&current.path)?;

        let name = match patch.name {
            Some(value) => validate_name(&value)?,
            None => current.name.clone(),
        };
        let domain = match patch.domain {
            Some(value) => validate_domain(&value)?,
            None => current.domain.clone(),
        };
        let php_version = match patch.php_version {
            Some(value) => validate_php_version(&value)?,
            None => current.php_version.clone(),
        };
        let document_root = match patch.document_root {
            Some(value) => validate_document_root(Path::new(&project_path), &value)?,
            None => current.document_root.clone(),
        };
        let database_name = match patch.database_name {
            Some(value) => normalize_database_name(value),
            None => current.database_name.clone(),
        };
        let database_port = match patch.database_port {
            Some(value) => validate_database_port(value)?,
            None => current.database_port,
        };
        let server_type = patch
            .server_type
            .unwrap_or_else(|| current.server_type.clone());
        let framework = patch.framework.unwrap_or_else(|| current.framework.clone());
        let frankenphp_mode = validate_frankenphp_mode(
            &server_type,
            &framework,
            patch.frankenphp_mode.unwrap_or_else(|| {
                if matches!(server_type, ServerType::Frankenphp)
                    && matches!(framework, FrameworkType::Laravel)
                {
                    current.frankenphp_mode.clone()
                } else {
                    FrankenphpMode::Classic
                }
            }),
        )?;

        connection
            .execute(
                "
                UPDATE projects
                SET
                    name = ?2,
                    domain = ?3,
                    server_type = ?4,
                    php_version = ?5,
                    framework = ?6,
                    document_root = ?7,
                    ssl_enabled = ?8,
                    database_name = ?9,
                    database_port = ?10,
                    status = ?11,
                    frankenphp_mode = ?12,
                    updated_at = ?13
                WHERE id = ?1
                ",
                params![
                    project_id,
                    name,
                    domain,
                    server_type.as_str(),
                    php_version,
                    framework.as_str(),
                    document_root,
                    if patch.ssl_enabled.unwrap_or(current.ssl_enabled) {
                        1
                    } else {
                        0
                    },
                    database_name,
                    database_port,
                    patch
                        .status
                        .unwrap_or_else(|| current.status.clone())
                        .as_str(),
                    frankenphp_mode.as_str(),
                    timestamp,
                ],
            )
            .map_err(map_sql_error)?;

        Self::get(connection, project_id)
    }

    pub fn delete(connection: &Connection, project_id: &str) -> Result<bool, AppError> {
        let deleted = connection.execute("DELETE FROM projects WHERE id = ?1", [project_id])?;

        if deleted == 0 {
            return Err(AppError::new_validation(
                "PROJECT_NOT_FOUND",
                "Project not found.",
            ));
        }

        Ok(true)
    }
}

pub struct ServiceRepository;

impl ServiceRepository {
    pub fn seed_defaults(connection: &Connection) -> Result<(), AppError> {
        let timestamp = now_iso()?;

        for (name, port) in [
            ("apache", 80),
            ("nginx", 80),
            ("frankenphp", 80),
            ("mysql", 3306),
            ("mailpit", 8025),
            ("redis", 6379),
        ] {
            connection.execute(
                "
                INSERT OR IGNORE INTO services (
                    name, enabled, auto_start, port, pid, status, last_error, updated_at
                )
                VALUES (?1, 1, 0, ?2, NULL, 'stopped', NULL, ?3)
                ",
                params![name, port, timestamp],
            )?;
        }

        Ok(())
    }

    pub fn list(connection: &Connection) -> Result<Vec<ServiceState>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM services
            ORDER BY name ASC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            map_service_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;
        let mut services = Vec::new();

        for row in rows {
            match row {
                Ok(service) => services.push(service),
                Err(error) => return Err(AppError::from(error)),
            }
        }

        Ok(services)
    }

    pub fn get(connection: &Connection, name: &str) -> Result<ServiceState, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM services
            WHERE name = ?1
            ",
        )?;

        let service = statement
            .query_row([name], |row| {
                map_service_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        service.ok_or_else(|| AppError::new_validation("SERVICE_NOT_FOUND", "Service not found."))
    }

    pub fn save_state(
        connection: &Connection,
        name: &ServiceName,
        status: &ServiceStatus,
        pid: Option<i64>,
        port: Option<i64>,
        last_error: Option<&str>,
    ) -> Result<ServiceState, AppError> {
        let timestamp = now_iso()?;

        connection.execute(
            "
            UPDATE services
            SET
                status = ?2,
                pid = ?3,
                port = ?4,
                last_error = ?5,
                updated_at = ?6
            WHERE name = ?1
            ",
            params![
                name.as_str(),
                status.as_str(),
                pid,
                port,
                last_error,
                timestamp,
            ],
        )?;

        Self::get(connection, name.as_str())
    }
}

pub struct RuntimeVersionRepository;

impl RuntimeVersionRepository {
    pub fn upsert(
        connection: &Connection,
        runtime_type: &RuntimeType,
        version: &str,
        path: &str,
        is_active: bool,
    ) -> Result<RuntimeVersion, AppError> {
        let timestamp = now_iso()?;
        let runtime_id = format!("{}-{version}", runtime_type.as_str());

        connection.execute(
            "
            INSERT INTO runtime_versions (id, runtime_type, version, path, is_active, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                is_active = excluded.is_active,
                updated_at = excluded.updated_at
            ",
            params![
                runtime_id,
                runtime_type.as_str(),
                version,
                path,
                if is_active { 1 } else { 0 },
                timestamp,
            ],
        )?;

        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            WHERE id = ?1
            ",
        )?;

        statement
            .query_row([runtime_id], |row| {
                map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .map_err(AppError::from)
    }

    pub fn clear_active_for_type(
        connection: &Connection,
        runtime_type: &RuntimeType,
    ) -> Result<(), AppError> {
        connection.execute(
            "
            UPDATE runtime_versions
            SET is_active = 0
            WHERE runtime_type = ?1
            ",
            [runtime_type.as_str()],
        )?;

        Ok(())
    }

    pub fn list_by_type(
        connection: &Connection,
        runtime_type: &RuntimeType,
    ) -> Result<Vec<RuntimeVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            WHERE runtime_type = ?1
            ORDER BY version ASC
            ",
        )?;

        let rows = statement.query_map([runtime_type.as_str()], |row| {
            map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn find_active_by_type(
        connection: &Connection,
        runtime_type: &RuntimeType,
    ) -> Result<Option<RuntimeVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            WHERE runtime_type = ?1 AND is_active = 1
            ORDER BY updated_at DESC
            LIMIT 1
            ",
        )?;

        statement
            .query_row([runtime_type.as_str()], |row| {
                map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn find_by_type_and_version(
        connection: &Connection,
        runtime_type: &RuntimeType,
        version: &str,
    ) -> Result<Option<RuntimeVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            WHERE runtime_type = ?1 AND version = ?2
            ORDER BY updated_at DESC
            LIMIT 1
            ",
        )?;

        statement
            .query_row([runtime_type.as_str(), version], |row| {
                map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn get_by_id(
        connection: &Connection,
        runtime_id: &str,
    ) -> Result<RuntimeVersion, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            WHERE id = ?1
            LIMIT 1
            ",
        )?;

        let runtime = statement
            .query_row([runtime_id], |row| {
                map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        runtime.ok_or_else(|| {
            AppError::new_validation("RUNTIME_NOT_FOUND", "Runtime entry was not found.")
        })
    }

    pub fn delete_by_id(connection: &Connection, runtime_id: &str) -> Result<bool, AppError> {
        let deleted =
            connection.execute("DELETE FROM runtime_versions WHERE id = ?1", [runtime_id])?;

        if deleted == 0 {
            return Err(AppError::new_validation(
                "RUNTIME_NOT_FOUND",
                "Runtime entry was not found.",
            ));
        }

        Ok(true)
    }

    pub fn list(connection: &Connection) -> Result<Vec<RuntimeVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM runtime_versions
            ORDER BY
                CASE runtime_type
                    WHEN 'apache' THEN 1
                    WHEN 'nginx' THEN 2
                    WHEN 'frankenphp' THEN 3
                    WHEN 'mysql' THEN 4
                    WHEN 'php' THEN 5
                    ELSE 99
                END,
                version ASC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            map_runtime_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }
}

pub struct OptionalToolVersionRepository;

impl OptionalToolVersionRepository {
    pub fn repair_invalid_versions(connection: &Connection) -> Result<(), AppError> {
        let tools = Self::list(connection)?;

        for tool in tools {
            if !tool.version.contains('\\') && !tool.version.contains('/') {
                continue;
            }

            let inferred_version = Path::new(&tool.path)
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|value| value.to_string_lossy().to_string())
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AppError::new_validation(
                        "OPTIONAL_TOOL_VERSION_REPAIR_FAILED",
                        "DevNest found an invalid optional tool version entry but could not infer the managed version from its install path.",
                    )
                })?;

            let next_id = format!("{}-{inferred_version}", tool.tool_type.as_str());
            connection.execute(
                "
                UPDATE optional_tool_versions
                SET id = ?2, version = ?3, updated_at = ?4
                WHERE id = ?1
                ",
                params![tool.id, next_id, inferred_version, now_iso()?],
            )?;
        }

        Ok(())
    }

    pub fn upsert(
        connection: &Connection,
        tool_type: &OptionalToolType,
        version: &str,
        path: &str,
        is_active: bool,
    ) -> Result<OptionalToolVersion, AppError> {
        let timestamp = now_iso()?;
        let tool_id = format!("{}-{version}", tool_type.as_str());

        connection.execute(
            "
            INSERT INTO optional_tool_versions (id, tool_type, version, path, is_active, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                is_active = excluded.is_active,
                updated_at = excluded.updated_at
            ",
            params![
                tool_id,
                tool_type.as_str(),
                version,
                path,
                if is_active { 1 } else { 0 },
                timestamp,
            ],
        )?;

        let mut statement = connection.prepare(
            "
            SELECT *
            FROM optional_tool_versions
            WHERE id = ?1
            ",
        )?;

        statement
            .query_row([tool_id], |row| {
                map_optional_tool_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .map_err(AppError::from)
    }

    pub fn clear_active_for_type(
        connection: &Connection,
        tool_type: &OptionalToolType,
    ) -> Result<(), AppError> {
        connection.execute(
            "
            UPDATE optional_tool_versions
            SET is_active = 0
            WHERE tool_type = ?1
            ",
            [tool_type.as_str()],
        )?;

        Ok(())
    }

    pub fn find_active_by_type(
        connection: &Connection,
        tool_type: &OptionalToolType,
    ) -> Result<Option<OptionalToolVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM optional_tool_versions
            WHERE tool_type = ?1 AND is_active = 1
            ORDER BY updated_at DESC
            LIMIT 1
            ",
        )?;

        statement
            .query_row([tool_type.as_str()], |row| {
                map_optional_tool_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn list(connection: &Connection) -> Result<Vec<OptionalToolVersion>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM optional_tool_versions
            ORDER BY
                CASE tool_type
                    WHEN 'mailpit' THEN 1
                    WHEN 'cloudflared' THEN 2
                    ELSE 99
                END,
                version ASC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            map_optional_tool_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn get_by_id(
        connection: &Connection,
        tool_id: &str,
    ) -> Result<OptionalToolVersion, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM optional_tool_versions
            WHERE id = ?1
            LIMIT 1
            ",
        )?;

        let tool = statement
            .query_row([tool_id], |row| {
                map_optional_tool_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        tool.ok_or_else(|| {
            AppError::new_validation(
                "OPTIONAL_TOOL_NOT_FOUND",
                "Optional tool entry was not found.",
            )
        })
    }

    pub fn delete_by_id(connection: &Connection, tool_id: &str) -> Result<bool, AppError> {
        let deleted = connection.execute(
            "DELETE FROM optional_tool_versions WHERE id = ?1",
            [tool_id],
        )?;

        if deleted == 0 {
            return Err(AppError::new_validation(
                "OPTIONAL_TOOL_NOT_FOUND",
                "Optional tool entry was not found.",
            ));
        }

        Ok(true)
    }
}

pub struct RuntimeConfigOverrideRepository;

impl RuntimeConfigOverrideRepository {
    pub fn list_for_runtime(
        connection: &Connection,
        runtime_id: &str,
    ) -> Result<HashMap<String, String>, AppError> {
        RuntimeVersionRepository::get_by_id(connection, runtime_id)?;

        let mut statement = connection.prepare(
            "
            SELECT config_key, config_value
            FROM runtime_config_overrides
            WHERE runtime_id = ?1
            ",
        )?;
        let rows = statement.query_map([runtime_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut overrides = HashMap::new();
        for row in rows {
            let (config_key, config_value) = row?;
            overrides.insert(config_key, config_value);
        }

        Ok(overrides)
    }

    pub fn upsert_many(
        connection: &Connection,
        runtime_id: &str,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), AppError> {
        RuntimeVersionRepository::get_by_id(connection, runtime_id)?;

        let timestamp = now_iso()?;
        let transaction = connection.unchecked_transaction()?;
        for (key, value) in values {
            transaction.execute(
                "
                INSERT INTO runtime_config_overrides (runtime_id, config_key, config_value, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(runtime_id, config_key) DO UPDATE SET
                    config_value = excluded.config_value,
                    updated_at = excluded.updated_at
                ",
                params![runtime_id, key, value, timestamp],
            )?;
        }
        transaction.commit()?;

        Ok(())
    }
}

pub struct ProjectEnvVarRepository;

impl ProjectEnvVarRepository {
    pub fn list_by_project(
        connection: &Connection,
        project_id: &str,
    ) -> Result<Vec<ProjectEnvVar>, AppError> {
        ProjectRepository::get(connection, project_id)?;

        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_env_vars
            WHERE project_id = ?1
            ORDER BY env_key ASC, updated_at DESC
            ",
        )?;

        let rows = statement.query_map([project_id], |row| {
            map_project_env_var_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    fn get_by_id(
        connection: &Connection,
        project_id: &str,
        env_var_id: &str,
    ) -> Result<ProjectEnvVar, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_env_vars
            WHERE project_id = ?1 AND id = ?2
            LIMIT 1
            ",
        )?;

        let env_var = statement
            .query_row(params![project_id, env_var_id], |row| {
                map_project_env_var_row(row).map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()?;

        env_var.ok_or_else(|| {
            AppError::new_validation(
                "PROJECT_ENV_VAR_NOT_FOUND",
                "Project environment variable was not found.",
            )
        })
    }

    fn ensure_unique_key(
        connection: &Connection,
        project_id: &str,
        env_key: &str,
        excluding_id: Option<&str>,
    ) -> Result<(), AppError> {
        let mut statement = connection.prepare(
            "
            SELECT id
            FROM project_env_vars
            WHERE project_id = ?1 AND env_key = ?2
            LIMIT 1
            ",
        )?;

        let existing_id = statement
            .query_row(params![project_id, env_key], |row| row.get::<_, String>(0))
            .optional()?;

        if let Some(existing_id) = existing_id {
            if excluding_id != Some(existing_id.as_str()) {
                return Err(AppError::new_validation(
                    "PROJECT_ENV_KEY_EXISTS",
                    "This project already tracks an environment variable with that key.",
                ));
            }
        }

        Ok(())
    }

    pub fn create(
        connection: &Connection,
        input: CreateProjectEnvVarInput,
    ) -> Result<ProjectEnvVar, AppError> {
        ProjectRepository::get(connection, &input.project_id)?;
        let env_key = validate_env_key(&input.env_key)?;
        let env_value = validate_env_value(&input.env_value)?;
        Self::ensure_unique_key(connection, &input.project_id, &env_key, None)?;
        let timestamp = now_iso()?;
        let env_var_id = Uuid::new_v4().to_string();

        connection.execute(
            "
            INSERT INTO project_env_vars (
                id, project_id, env_key, env_value, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            ",
            params![env_var_id, input.project_id, env_key, env_value, timestamp,],
        )?;

        Self::get_by_id(connection, &input.project_id, &env_var_id)
    }

    pub fn update(
        connection: &Connection,
        input: UpdateProjectEnvVarInput,
    ) -> Result<ProjectEnvVar, AppError> {
        let current = Self::get_by_id(connection, &input.project_id, &input.env_var_id)?;
        let env_key = validate_env_key(&input.env_key)?;
        let env_value = validate_env_value(&input.env_value)?;
        Self::ensure_unique_key(
            connection,
            &input.project_id,
            &env_key,
            Some(current.id.as_str()),
        )?;
        let timestamp = now_iso()?;

        connection.execute(
            "
            UPDATE project_env_vars
            SET env_key = ?3, env_value = ?4, updated_at = ?5
            WHERE project_id = ?1 AND id = ?2
            ",
            params![
                input.project_id,
                input.env_var_id,
                env_key,
                env_value,
                timestamp,
            ],
        )?;

        Self::get_by_id(connection, &current.project_id, &current.id)
    }

    pub fn delete(
        connection: &Connection,
        project_id: &str,
        env_var_id: &str,
    ) -> Result<bool, AppError> {
        let deleted = connection.execute(
            "DELETE FROM project_env_vars WHERE project_id = ?1 AND id = ?2",
            params![project_id, env_var_id],
        )?;

        if deleted == 0 {
            return Err(AppError::new_validation(
                "PROJECT_ENV_VAR_NOT_FOUND",
                "Project environment variable was not found.",
            ));
        }

        Ok(true)
    }
}

pub struct PhpExtensionOverrideRepository;

impl PhpExtensionOverrideRepository {
    pub fn list_for_runtime(
        connection: &Connection,
        runtime_id: &str,
        runtime_version: &str,
        available_extensions: &[String],
    ) -> Result<Vec<PhpExtensionState>, AppError> {
        let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;
        if !matches!(
            runtime.runtime_type,
            RuntimeType::Php | RuntimeType::Frankenphp
        ) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP extensions can only be managed for PHP or FrankenPHP runtimes.",
            ));
        }

        let mut statement = connection.prepare(
            "
            SELECT extension_name, enabled, updated_at
            FROM php_extension_overrides
            WHERE runtime_id = ?1
            ",
        )?;
        let rows = statement.query_map([runtime_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? == 1,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut overrides = std::collections::HashMap::new();
        for row in rows {
            let (extension_name, enabled, updated_at) = row?;
            overrides.insert(extension_name, (enabled, updated_at));
        }

        let fallback_updated_at = now_iso()?;
        Ok(available_extensions
            .iter()
            .map(|extension_name| {
                let override_entry = overrides.get(extension_name);
                PhpExtensionState {
                    runtime_id: runtime_id.to_string(),
                    runtime_version: runtime_version.to_string(),
                    extension_name: extension_name.to_string(),
                    dll_file: format!("php_{extension_name}.dll"),
                    enabled: override_entry
                        .map(|entry| entry.0)
                        .unwrap_or_else(|| php_extension_enabled_by_default(extension_name)),
                    updated_at: override_entry
                        .map(|entry| entry.1.clone())
                        .unwrap_or_else(|| fallback_updated_at.clone()),
                }
            })
            .collect())
    }

    pub fn set_enabled(
        connection: &Connection,
        runtime_id: &str,
        extension_name: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;
        if !matches!(
            runtime.runtime_type,
            RuntimeType::Php | RuntimeType::Frankenphp
        ) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP extensions can only be managed for PHP or FrankenPHP runtimes.",
            ));
        }

        let timestamp = now_iso()?;
        connection.execute(
            "
            INSERT INTO php_extension_overrides (runtime_id, extension_name, enabled, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(runtime_id, extension_name) DO UPDATE
            SET enabled = excluded.enabled, updated_at = excluded.updated_at
            ",
            params![
                runtime_id,
                extension_name.trim().to_ascii_lowercase(),
                if enabled { 1 } else { 0 },
                timestamp
            ],
        )?;

        Ok(())
    }

    pub fn delete(
        connection: &Connection,
        runtime_id: &str,
        extension_name: &str,
    ) -> Result<(), AppError> {
        let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;
        if !matches!(
            runtime.runtime_type,
            RuntimeType::Php | RuntimeType::Frankenphp
        ) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP extensions can only be managed for PHP or FrankenPHP runtimes.",
            ));
        }

        connection.execute(
            "
            DELETE FROM php_extension_overrides
            WHERE runtime_id = ?1 AND extension_name = ?2
            ",
            params![runtime_id, extension_name.trim().to_ascii_lowercase()],
        )?;

        Ok(())
    }
}

pub struct PhpFunctionOverrideRepository;

impl PhpFunctionOverrideRepository {
    pub fn list_for_runtime(
        connection: &Connection,
        runtime_id: &str,
        runtime_version: &str,
        managed_functions: &[String],
    ) -> Result<Vec<PhpFunctionState>, AppError> {
        let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;
        if !matches!(
            runtime.runtime_type,
            RuntimeType::Php | RuntimeType::Frankenphp
        ) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP functions can only be managed for PHP or FrankenPHP runtimes.",
            ));
        }

        let mut statement = connection.prepare(
            "
            SELECT function_name, enabled, updated_at
            FROM php_function_overrides
            WHERE runtime_id = ?1
            ",
        )?;
        let rows = statement.query_map([runtime_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? == 1,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut overrides = std::collections::HashMap::new();
        for row in rows {
            let (function_name, enabled, updated_at) = row?;
            overrides.insert(function_name, (enabled, updated_at));
        }

        let fallback_updated_at = now_iso()?;
        Ok(managed_functions
            .iter()
            .map(|function_name| {
                let override_entry = overrides.get(function_name);
                PhpFunctionState {
                    runtime_id: runtime_id.to_string(),
                    runtime_version: runtime_version.to_string(),
                    function_name: function_name.to_string(),
                    enabled: override_entry.map(|entry| entry.0).unwrap_or(true),
                    updated_at: override_entry
                        .map(|entry| entry.1.clone())
                        .unwrap_or_else(|| fallback_updated_at.clone()),
                }
            })
            .collect())
    }

    pub fn set_enabled(
        connection: &Connection,
        runtime_id: &str,
        function_name: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        let runtime = RuntimeVersionRepository::get_by_id(connection, runtime_id)?;
        if !matches!(
            runtime.runtime_type,
            RuntimeType::Php | RuntimeType::Frankenphp
        ) {
            return Err(AppError::new_validation(
                "INVALID_RUNTIME_TYPE",
                "PHP functions can only be managed for PHP or FrankenPHP runtimes.",
            ));
        }

        let timestamp = now_iso()?;
        connection.execute(
            "
            INSERT INTO php_function_overrides (runtime_id, function_name, enabled, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(runtime_id, function_name) DO UPDATE
            SET enabled = excluded.enabled, updated_at = excluded.updated_at
            ",
            params![
                runtime_id,
                function_name.trim().to_ascii_lowercase(),
                if enabled { 1 } else { 0 },
                timestamp
            ],
        )?;

        Ok(())
    }
}

pub struct RuntimeSuppressionRepository;

impl RuntimeSuppressionRepository {
    pub fn suppress(
        connection: &Connection,
        runtime_type: &RuntimeType,
        path: &Path,
    ) -> Result<(), AppError> {
        let timestamp = now_iso()?;
        let path_key = normalize_runtime_path_key(path);

        connection.execute(
            "
            INSERT INTO runtime_suppressions (runtime_type, path_key, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(runtime_type, path_key) DO UPDATE SET created_at = excluded.created_at
            ",
            params![runtime_type.as_str(), path_key, timestamp],
        )?;

        Ok(())
    }

    pub fn remove(
        connection: &Connection,
        runtime_type: &RuntimeType,
        path: &Path,
    ) -> Result<(), AppError> {
        let path_key = normalize_runtime_path_key(path);

        connection.execute(
            "
            DELETE FROM runtime_suppressions
            WHERE runtime_type = ?1 AND path_key = ?2
            ",
            params![runtime_type.as_str(), path_key],
        )?;

        Ok(())
    }

    pub fn is_suppressed(
        connection: &Connection,
        runtime_type: &RuntimeType,
        path: &Path,
    ) -> Result<bool, AppError> {
        let path_key = normalize_runtime_path_key(path);
        let mut statement = connection.prepare(
            "
            SELECT 1
            FROM runtime_suppressions
            WHERE runtime_type = ?1 AND path_key = ?2
            LIMIT 1
            ",
        )?;

        let exists = statement
            .query_row(params![runtime_type.as_str(), path_key], |_row| Ok(true))
            .optional()?
            .unwrap_or(false);

        Ok(exists)
    }
}

pub struct ProjectPersistentHostnameRepository;

impl ProjectPersistentHostnameRepository {
    pub fn get_by_project(
        connection: &Connection,
        project_id: &str,
    ) -> Result<Option<ProjectPersistentHostname>, AppError> {
        ProjectRepository::get(connection, project_id)?;

        let mut statement = connection.prepare(
            "
            SELECT *
            FROM project_persistent_hostnames
            WHERE project_id = ?1
            LIMIT 1
            ",
        )?;

        statement
            .query_row([project_id], |row| {
                map_project_persistent_hostname_row(row)
                    .map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn upsert(
        connection: &Connection,
        project_id: &str,
        provider: &PersistentTunnelProvider,
        hostname: &str,
    ) -> Result<ProjectPersistentHostname, AppError> {
        ProjectRepository::get(connection, project_id)?;
        let normalized_hostname = validate_public_hostname(hostname)?;
        let timestamp = now_iso()?;
        let record_id = format!("{}-{project_id}", provider.as_str());

        connection
            .execute(
                "
                INSERT INTO project_persistent_hostnames (id, project_id, provider, hostname, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                ON CONFLICT(project_id) DO UPDATE SET
                    provider = excluded.provider,
                    hostname = excluded.hostname,
                    updated_at = excluded.updated_at
                ",
                params![
                    record_id,
                    project_id,
                    provider.as_str(),
                    normalized_hostname,
                    timestamp,
                ],
            )
            .map_err(|error| match error {
                SqlError::SqliteFailure(_, Some(message))
                    if message.contains("project_persistent_hostnames.hostname") =>
                {
                    AppError::new_validation(
                        "PERSISTENT_HOSTNAME_EXISTS",
                        "This persistent hostname is already assigned to another project.",
                    )
                }
                other => AppError::from(other),
            })?;

        Self::get_by_project(connection, project_id)?.ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_HOSTNAME_NOT_FOUND",
                "Persistent hostname was saved, but DevNest could not load it back.",
            )
        })
    }

    pub fn delete_by_project(connection: &Connection, project_id: &str) -> Result<bool, AppError> {
        ProjectRepository::get(connection, project_id)?;
        let deleted = connection.execute(
            "DELETE FROM project_persistent_hostnames WHERE project_id = ?1",
            [project_id],
        )?;

        Ok(deleted > 0)
    }
}

pub struct PersistentTunnelSetupRepository;

impl PersistentTunnelSetupRepository {
    pub fn get(
        connection: &Connection,
        provider: &PersistentTunnelProvider,
    ) -> Result<Option<PersistentTunnelManagedSetup>, AppError> {
        let mut statement = connection.prepare(
            "
            SELECT *
            FROM persistent_tunnel_setups
            WHERE provider = ?1
            LIMIT 1
            ",
        )?;

        statement
            .query_row([provider.as_str()], |row| {
                map_persistent_tunnel_setup_row(row)
                    .map_err(|_| rusqlite::Error::ExecuteReturnedResults)
            })
            .optional()
            .map_err(AppError::from)
    }

    pub fn upsert(
        connection: &Connection,
        provider: &PersistentTunnelProvider,
        auth_cert_path: Option<&str>,
        credentials_path: Option<&str>,
        tunnel_id: Option<&str>,
        tunnel_name: Option<&str>,
        default_hostname_zone: Option<&str>,
    ) -> Result<PersistentTunnelManagedSetup, AppError> {
        let normalized_auth_cert_path = auth_cert_path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let normalized_credentials_path = credentials_path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let normalized_tunnel_id = tunnel_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let normalized_tunnel_name = tunnel_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let normalized_default_hostname_zone = default_hostname_zone
            .map(validate_public_hostname_zone)
            .transpose()?;
        let timestamp = now_iso()?;
        let existing = Self::get(connection, provider)?;
        let created_at = existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| timestamp.clone());

        connection.execute(
            "
            INSERT INTO persistent_tunnel_setups (
                provider,
                auth_cert_path,
                credentials_path,
                tunnel_id,
                tunnel_name,
                default_hostname_zone,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(provider) DO UPDATE SET
                auth_cert_path = excluded.auth_cert_path,
                credentials_path = excluded.credentials_path,
                tunnel_id = excluded.tunnel_id,
                tunnel_name = excluded.tunnel_name,
                default_hostname_zone = excluded.default_hostname_zone,
                updated_at = excluded.updated_at
            ",
            params![
                provider.as_str(),
                normalized_auth_cert_path,
                normalized_credentials_path,
                normalized_tunnel_id,
                normalized_tunnel_name,
                normalized_default_hostname_zone,
                created_at,
                timestamp,
            ],
        )?;

        Self::get(connection, provider)?.ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_SETUP_NOT_FOUND",
                "Persistent tunnel setup was saved, but DevNest could not load it back.",
            )
        })
    }

    pub fn delete(
        connection: &Connection,
        provider: &PersistentTunnelProvider,
    ) -> Result<bool, AppError> {
        let deleted = connection.execute(
            "DELETE FROM persistent_tunnel_setups WHERE provider = ?1",
            [provider.as_str()],
        )?;

        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OptionalToolVersionRepository, PersistentTunnelSetupRepository,
        PhpExtensionOverrideRepository, ProjectEnvVarRepository,
        ProjectPersistentHostnameRepository, ProjectRepository, RuntimeConfigOverrideRepository,
        RuntimeSuppressionRepository, RuntimeVersionRepository, ServiceRepository,
    };
    use crate::models::optional_tool::OptionalToolType;
    use crate::models::persistent_tunnel::PersistentTunnelProvider;
    use crate::models::project::{
        CreateProjectInput, FrameworkType, ServerType, UpdateProjectPatch,
    };
    use crate::models::project_env_var::{CreateProjectEnvVarInput, UpdateProjectEnvVarInput};
    use crate::models::runtime::RuntimeType;
    use crate::storage::db::init_database;
    use rusqlite::{Connection, params};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn setup_test_db() -> (PathBuf, Connection) {
        let db_path = std::env::temp_dir().join(format!("devnest-test-{}.sqlite3", Uuid::new_v4()));
        init_database(&db_path).expect("database initialization should succeed");
        let connection = Connection::open(&db_path).expect("test database connection should open");
        (db_path, connection)
    }

    fn make_temp_project_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("devnest-project-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("public")).expect("project root should be created");
        root
    }

    fn cleanup_paths(db_path: PathBuf, project_paths: &[&Path]) {
        fs::remove_file(db_path).ok();
        for path in project_paths {
            fs::remove_dir_all(path).ok();
        }
    }

    fn sample_project_input(project_path: &Path) -> CreateProjectInput {
        CreateProjectInput {
            name: "Shop API".to_string(),
            path: project_path.to_string_lossy().to_string(),
            domain: "shop-api.test".to_string(),
            server_type: ServerType::Nginx,
            php_version: "8.2".to_string(),
            framework: FrameworkType::Laravel,
            document_root: "public".to_string(),
            ssl_enabled: false,
            database_name: Some("shop_api".to_string()),
            database_port: Some(3306),
            frankenphp_mode: None,
        }
    }

    #[test]
    fn seeds_default_services() {
        let (db_path, connection) = setup_test_db();
        let services = ServiceRepository::list(&connection).expect("service list should load");
        assert_eq!(services.len(), 5);
        fs::remove_file(db_path).ok();
    }

    #[test]
    fn lists_runtime_versions_after_upsert() {
        let (db_path, connection) = setup_test_db();

        RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Apache,
            "2.4",
            r"D:\apache\bin\httpd.exe",
            true,
        )
        .expect("apache runtime should upsert");
        RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.2",
            r"D:\php\php82\php.exe",
            true,
        )
        .expect("php runtime should upsert");

        let runtimes =
            RuntimeVersionRepository::list(&connection).expect("runtime list should load");
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0].runtime_type.as_str(), "apache");
        assert_eq!(runtimes[1].runtime_type.as_str(), "php");

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn deletes_runtime_version_by_id() {
        let (db_path, connection) = setup_test_db();

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.2",
            r"D:\php\php.exe",
            true,
        )
        .expect("php runtime should upsert");

        let deleted = RuntimeVersionRepository::delete_by_id(&connection, &runtime.id)
            .expect("runtime delete should succeed");
        assert!(deleted);
        assert!(
            RuntimeVersionRepository::get_by_id(&connection, &runtime.id).is_err(),
            "deleted runtime should no longer exist",
        );

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn upserts_lists_and_deletes_optional_tool_versions() {
        let (db_path, connection) = setup_test_db();

        let tool = OptionalToolVersionRepository::upsert(
            &connection,
            &OptionalToolType::Mailpit,
            "1.29.7",
            r"D:\devnest\optional-tools\mailpit\1.29.7\mailpit.exe",
            true,
        )
        .expect("mailpit optional tool should upsert");

        let listed = OptionalToolVersionRepository::list(&connection)
            .expect("optional tool list should load");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].tool_type.as_str(), "mailpit");
        assert!(listed[0].is_active);
        assert_eq!(listed[0].version, "1.29.7");

        let active = OptionalToolVersionRepository::find_active_by_type(
            &connection,
            &OptionalToolType::Mailpit,
        )
        .expect("active optional tool should load")
        .expect("mailpit should be active");
        assert_eq!(active.id, tool.id);

        let deleted = OptionalToolVersionRepository::delete_by_id(&connection, &tool.id)
            .expect("optional tool delete should succeed");
        assert!(deleted);
        assert!(
            OptionalToolVersionRepository::get_by_id(&connection, &tool.id).is_err(),
            "deleted optional tool should no longer exist",
        );

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn repairs_optional_tool_versions_that_accidentally_store_the_path() {
        let (db_path, connection) = setup_test_db();
        let managed_path = std::env::temp_dir()
            .join(format!("devnest-mailpit-{}", Uuid::new_v4()))
            .join("1.29.7")
            .join("mailpit.exe");
        fs::create_dir_all(managed_path.parent().expect("managed parent should exist"))
            .expect("managed path parent should be created");
        fs::write(&managed_path, b"mock").expect("managed file should exist");

        connection
            .execute(
                "
                INSERT INTO optional_tool_versions (id, tool_type, version, path, is_active, created_at, updated_at)
                VALUES (?1, 'mailpit', ?2, ?2, 1, '2026-04-18T00:00:00Z', '2026-04-18T00:00:00Z')
                ",
                params![
                    format!("mailpit-{}", managed_path.to_string_lossy()),
                    managed_path.to_string_lossy().to_string(),
                ],
            )
            .expect("broken optional tool row should insert");

        OptionalToolVersionRepository::repair_invalid_versions(&connection)
            .expect("repair should succeed");

        let repaired = OptionalToolVersionRepository::find_active_by_type(
            &connection,
            &OptionalToolType::Mailpit,
        )
        .expect("repaired optional tool should load")
        .expect("mailpit should remain active");

        assert_eq!(repaired.version, "1.29.7");
        assert_eq!(repaired.id, "mailpit-1.29.7");

        fs::remove_file(db_path).ok();
        fs::remove_dir_all(
            managed_path
                .parent()
                .and_then(|parent| parent.parent())
                .expect("managed root should exist"),
        )
        .ok();
    }

    #[test]
    fn suppresses_and_unsuppresses_runtime_path() {
        let (db_path, connection) = setup_test_db();
        let path = Path::new(r"D:\laragon\bin\apache\httpd.exe");

        RuntimeSuppressionRepository::suppress(&connection, &RuntimeType::Apache, path)
            .expect("suppression should succeed");
        assert!(
            RuntimeSuppressionRepository::is_suppressed(&connection, &RuntimeType::Apache, path)
                .expect("suppression lookup should succeed")
        );

        RuntimeSuppressionRepository::remove(&connection, &RuntimeType::Apache, path)
            .expect("suppression remove should succeed");
        assert!(
            !RuntimeSuppressionRepository::is_suppressed(&connection, &RuntimeType::Apache, path)
                .expect("suppression lookup should succeed")
        );

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn creates_updates_and_deletes_project() {
        let (db_path, connection) = setup_test_db();
        let project_root = make_temp_project_root();
        let created = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project create should succeed");
        assert_eq!(created.domain, "shop-api.test");

        let fetched =
            ProjectRepository::get(&connection, &created.id).expect("project get should succeed");
        assert_eq!(fetched.name, "Shop API");

        let updated = ProjectRepository::update(
            &connection,
            &created.id,
            UpdateProjectPatch {
                name: Some("Shop API Updated".to_string()),
                domain: None,
                server_type: None,
                php_version: Some("8.3".to_string()),
                framework: None,
                document_root: None,
                ssl_enabled: Some(true),
                database_name: None,
                database_port: None,
                status: None,
                frankenphp_mode: None,
            },
        )
        .expect("project update should succeed");

        assert_eq!(updated.name, "Shop API Updated");
        assert_eq!(updated.php_version, "8.3");
        assert!(updated.ssl_enabled);

        let deleted = ProjectRepository::delete(&connection, &created.id)
            .expect("project delete should succeed");
        assert!(deleted);
        cleanup_paths(db_path, &[&project_root]);
    }

    #[test]
    fn rejects_duplicate_domain() {
        let (db_path, connection) = setup_test_db();
        let first_root = make_temp_project_root();
        let second_root = make_temp_project_root();
        ProjectRepository::create(&connection, sample_project_input(&first_root))
            .expect("first insert should succeed");

        let duplicate = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                path: second_root.to_string_lossy().to_string(),
                ..sample_project_input(&first_root)
            },
        )
        .expect_err("second insert should fail");

        assert_eq!(duplicate.code, "DOMAIN_ALREADY_EXISTS");
        cleanup_paths(db_path, &[&first_root, &second_root]);
    }

    #[test]
    fn rejects_document_root_outside_project_path() {
        let (db_path, connection) = setup_test_db();
        let project_root = make_temp_project_root();
        let invalid = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                document_root: "../public".to_string(),
                ..sample_project_input(&project_root)
            },
        )
        .expect_err("document root should be rejected");

        assert_eq!(invalid.code, "INVALID_DOCUMENT_ROOT");
        cleanup_paths(db_path, &[&project_root]);
    }

    #[test]
    fn rejects_missing_project_path() {
        let (db_path, connection) = setup_test_db();
        let missing_root = std::env::temp_dir().join(format!("devnest-missing-{}", Uuid::new_v4()));
        let invalid = ProjectRepository::create(&connection, sample_project_input(&missing_root))
            .expect_err("missing project path should be rejected");

        assert_eq!(invalid.code, "INVALID_PROJECT_PATH");
        cleanup_paths(db_path, &[]);
    }

    #[test]
    fn creates_updates_and_deletes_project_env_var() {
        let (db_path, connection) = setup_test_db();
        let project_root = make_temp_project_root();
        let project = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project create should succeed");

        let created = ProjectEnvVarRepository::create(
            &connection,
            CreateProjectEnvVarInput {
                project_id: project.id.clone(),
                env_key: "app_env".to_string(),
                env_value: "local".to_string(),
            },
        )
        .expect("env var create should succeed");
        assert_eq!(created.env_key, "APP_ENV");

        let updated = ProjectEnvVarRepository::update(
            &connection,
            UpdateProjectEnvVarInput {
                project_id: project.id.clone(),
                env_var_id: created.id.clone(),
                env_key: "app_name".to_string(),
                env_value: "DevNest".to_string(),
            },
        )
        .expect("env var update should succeed");
        assert_eq!(updated.env_key, "APP_NAME");
        assert_eq!(updated.env_value, "DevNest");

        let listed = ProjectEnvVarRepository::list_by_project(&connection, &project.id)
            .expect("list should work");
        assert_eq!(listed.len(), 1);

        let deleted = ProjectEnvVarRepository::delete(&connection, &project.id, &created.id)
            .expect("env var delete should succeed");
        assert!(deleted);

        cleanup_paths(db_path, &[&project_root]);
    }

    #[test]
    fn rejects_duplicate_project_env_var_key() {
        let (db_path, connection) = setup_test_db();
        let project_root = make_temp_project_root();
        let project = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project create should succeed");

        ProjectEnvVarRepository::create(
            &connection,
            CreateProjectEnvVarInput {
                project_id: project.id.clone(),
                env_key: "app_env".to_string(),
                env_value: "local".to_string(),
            },
        )
        .expect("first env var create should succeed");

        let duplicate = ProjectEnvVarRepository::create(
            &connection,
            CreateProjectEnvVarInput {
                project_id: project.id.clone(),
                env_key: "APP_ENV".to_string(),
                env_value: "staging".to_string(),
            },
        )
        .expect_err("duplicate env key should fail");

        assert_eq!(duplicate.code, "PROJECT_ENV_KEY_EXISTS");
        cleanup_paths(db_path, &[&project_root]);
    }

    #[test]
    fn creates_updates_and_deletes_project_persistent_hostname() {
        let (db_path, connection) = setup_test_db();
        let project_root = make_temp_project_root();
        let project = ProjectRepository::create(&connection, sample_project_input(&project_root))
            .expect("project create should succeed");

        let created = ProjectPersistentHostnameRepository::upsert(
            &connection,
            &project.id,
            &PersistentTunnelProvider::Cloudflared,
            "preview.example.com",
        )
        .expect("persistent hostname should save");
        assert_eq!(created.hostname, "preview.example.com");

        let updated = ProjectPersistentHostnameRepository::upsert(
            &connection,
            &project.id,
            &PersistentTunnelProvider::Cloudflared,
            "vietruyen.example.com",
        )
        .expect("persistent hostname should update");
        assert_eq!(updated.hostname, "vietruyen.example.com");

        let fetched = ProjectPersistentHostnameRepository::get_by_project(&connection, &project.id)
            .expect("persistent hostname should load")
            .expect("persistent hostname should exist");
        assert_eq!(fetched.hostname, "vietruyen.example.com");

        let deleted =
            ProjectPersistentHostnameRepository::delete_by_project(&connection, &project.id)
                .expect("persistent hostname should delete");
        assert!(deleted);

        cleanup_paths(db_path, &[&project_root]);
    }

    #[test]
    fn rejects_duplicate_persistent_hostname_across_projects() {
        let (db_path, connection) = setup_test_db();
        let first_root = make_temp_project_root();
        let second_root = make_temp_project_root();
        let first = ProjectRepository::create(&connection, sample_project_input(&first_root))
            .expect("first project should create");
        let second = ProjectRepository::create(
            &connection,
            CreateProjectInput {
                path: second_root.to_string_lossy().to_string(),
                domain: "shop-api-2.test".to_string(),
                ..sample_project_input(&first_root)
            },
        )
        .expect("second project should create");

        ProjectPersistentHostnameRepository::upsert(
            &connection,
            &first.id,
            &PersistentTunnelProvider::Cloudflared,
            "shared.example.com",
        )
        .expect("first persistent hostname should save");

        let duplicate = ProjectPersistentHostnameRepository::upsert(
            &connection,
            &second.id,
            &PersistentTunnelProvider::Cloudflared,
            "shared.example.com",
        )
        .expect_err("duplicate persistent hostname should fail");

        assert_eq!(duplicate.code, "PERSISTENT_HOSTNAME_EXISTS");
        cleanup_paths(db_path, &[&first_root, &second_root]);
    }

    #[test]
    fn creates_and_updates_persistent_tunnel_setup() {
        let (db_path, connection) = setup_test_db();

        let created = PersistentTunnelSetupRepository::upsert(
            &connection,
            &PersistentTunnelProvider::Cloudflared,
            Some(r"D:\DevNest\cert.pem"),
            Some(r"D:\DevNest\credentials\tunnel.json"),
            Some("demo-tunnel-id"),
            Some("DevNest Main"),
            Some("preview.example.com"),
        )
        .expect("persistent tunnel setup should save");
        assert_eq!(created.tunnel_id.as_deref(), Some("demo-tunnel-id"));
        assert_eq!(
            created.default_hostname_zone.as_deref(),
            Some("preview.example.com")
        );

        let updated = PersistentTunnelSetupRepository::upsert(
            &connection,
            &PersistentTunnelProvider::Cloudflared,
            Some(r"D:\DevNest\cert.pem"),
            Some(r"D:\DevNest\credentials\tunnel.json"),
            Some("demo-tunnel-id-2"),
            Some("DevNest Shared"),
            Some("projects.example.com"),
        )
        .expect("persistent tunnel setup should update");
        assert_eq!(updated.tunnel_name.as_deref(), Some("DevNest Shared"));
        assert_eq!(
            updated.default_hostname_zone.as_deref(),
            Some("projects.example.com")
        );

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn stores_php_extension_overrides_per_runtime() {
        let (db_path, connection) = setup_test_db();

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.4.20",
            r"D:\php\php84\php.exe",
            true,
        )
        .expect("php runtime should upsert");

        PhpExtensionOverrideRepository::set_enabled(&connection, &runtime.id, "intl", false)
            .expect("override should save");

        let states = PhpExtensionOverrideRepository::list_for_runtime(
            &connection,
            &runtime.id,
            &runtime.version,
            &["intl".to_string(), "mbstring".to_string()],
        )
        .expect("states should load");

        let intl = states
            .iter()
            .find(|item| item.extension_name == "intl")
            .expect("intl state should exist");
        let mbstring = states
            .iter()
            .find(|item| item.extension_name == "mbstring")
            .expect("mbstring state should exist");

        assert!(!intl.enabled);
        assert!(mbstring.enabled);

        fs::remove_file(db_path).ok();
    }

    #[test]
    fn stores_runtime_config_overrides_per_runtime() {
        let (db_path, connection) = setup_test_db();

        let runtime = RuntimeVersionRepository::upsert(
            &connection,
            &RuntimeType::Php,
            "8.4.20",
            r"D:\php\php84\php.exe",
            true,
        )
        .expect("php runtime should upsert");

        RuntimeConfigOverrideRepository::upsert_many(
            &connection,
            &runtime.id,
            &BTreeMap::from([
                ("memory_limit".to_string(), "768M".to_string()),
                ("date_timezone".to_string(), "Asia/Ho_Chi_Minh".to_string()),
            ]),
        )
        .expect("runtime config overrides should save");

        let overrides = RuntimeConfigOverrideRepository::list_for_runtime(&connection, &runtime.id)
            .expect("runtime config overrides should load");

        assert_eq!(
            overrides.get("memory_limit").map(String::as_str),
            Some("768M")
        );
        assert_eq!(
            overrides.get("date_timezone").map(String::as_str),
            Some("Asia/Ho_Chi_Minh")
        );

        fs::remove_file(db_path).ok();
    }
}
