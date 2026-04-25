use crate::error::AppError;
use crate::models::project::{CreateProjectInput, Project};
use crate::models::project_env_var::{CreateProjectEnvVarInput, ProjectEnvVar};
use crate::state::AppState;
use crate::storage::repositories::{ProjectEnvVarRepository, ProjectRepository};
use rfd::FileDialog;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectProfileProjectSnapshot {
    name: String,
    path: String,
    domain: String,
    server_type: String,
    php_version: String,
    framework: String,
    document_root: String,
    ssl_enabled: bool,
    database_name: Option<String>,
    database_port: Option<i64>,
    frankenphp_mode: Option<String>,
    env_vars: Vec<ProjectEnvVarSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEnvVarSnapshot {
    env_key: String,
    env_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectProfileDocument {
    format_version: u32,
    exported_at: String,
    source: String,
    project: ProjectProfileProjectSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectProfileSnapshot {
    name: String,
    root_name_hint: String,
    domain: String,
    server_type: String,
    php_version: String,
    framework: String,
    document_root: String,
    ssl_enabled: bool,
    database_name: Option<String>,
    database_port: Option<i64>,
    frankenphp_mode: Option<String>,
    env_vars: Vec<ProjectEnvVarSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectHandoffSnapshot {
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectProfileDocument {
    format_version: u32,
    profile_kind: String,
    exported_at: String,
    source: String,
    project: TeamProjectProfileSnapshot,
    machine_handoff: TeamProjectHandoffSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProfileTransferResult {
    pub success: bool,
    pub path: String,
}

fn snapshot_env_vars(items: Vec<ProjectEnvVar>) -> Vec<ProjectEnvVarSnapshot> {
    items
        .into_iter()
        .map(|item| ProjectEnvVarSnapshot {
            env_key: item.env_key,
            env_value: item.env_value,
        })
        .collect()
}

fn file_name_for_project(project: &Project) -> String {
    format!("{}.devnest-project.json", project.domain.replace('.', "-"))
}

fn team_share_file_name_for_project(project: &Project) -> String {
    format!(
        "{}.devnest-team-project.json",
        project.domain.replace('.', "-")
    )
}

fn project_root_name_hint(project: &Project) -> String {
    Path::new(&project.path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project.name.clone())
}

fn map_server_type(value: &str) -> Result<crate::models::project::ServerType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_PROJECT_PROFILE",
            "Imported project profile has an invalid server type.",
        )
    })
}

fn map_framework(value: &str) -> Result<crate::models::project::FrameworkType, AppError> {
    value.parse().map_err(|_| {
        AppError::new_validation(
            "INVALID_PROJECT_PROFILE",
            "Imported project profile has an invalid framework type.",
        )
    })
}

fn map_frankenphp_mode(
    value: Option<String>,
) -> Result<Option<crate::models::project::FrankenphpMode>, AppError> {
    value
        .map(|mode| {
            mode.parse().map_err(|_| {
                AppError::new_validation(
                    "INVALID_PROJECT_PROFILE",
                    "Imported project profile has an invalid FrankenPHP mode.",
                )
            })
        })
        .transpose()
}

fn read_profile_document(path: &std::path::Path) -> Result<ProjectProfileDocument, AppError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_READ_FAILED",
            "DevNest could not read the selected project profile file.",
            error.to_string(),
        )
    })?;

    let document: ProjectProfileDocument = serde_json::from_str(&raw).map_err(|error| {
        AppError::with_details(
            "INVALID_PROJECT_PROFILE",
            "The selected file is not a valid DevNest project profile.",
            error.to_string(),
        )
    })?;

    if document.format_version != 1 {
        return Err(AppError::new_validation(
            "UNSUPPORTED_PROJECT_PROFILE_VERSION",
            "This DevNest project profile version is not supported by the current app build.",
        ));
    }

    Ok(document)
}

fn read_team_profile_document(
    path: &std::path::Path,
) -> Result<TeamProjectProfileDocument, AppError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_READ_FAILED",
            "DevNest could not read the selected team-share project profile file.",
            error.to_string(),
        )
    })?;

    let document: TeamProjectProfileDocument = serde_json::from_str(&raw).map_err(|error| {
        AppError::with_details(
            "INVALID_PROJECT_PROFILE",
            "The selected file is not a valid DevNest team-share profile.",
            error.to_string(),
        )
    })?;

    if document.format_version != 1 {
        return Err(AppError::new_validation(
            "UNSUPPORTED_PROJECT_PROFILE_VERSION",
            "This DevNest team-share profile version is not supported by the current app build.",
        ));
    }

    if document.profile_kind.trim() != "team-share" {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_PROFILE",
            "The selected file is not a DevNest team-share project profile.",
        ));
    }

    Ok(document)
}

fn create_project_from_snapshot(
    connection: &Connection,
    name: String,
    path: String,
    domain: String,
    server_type: String,
    php_version: String,
    framework: String,
    document_root: String,
    ssl_enabled: bool,
    database_name: Option<String>,
    database_port: Option<i64>,
    frankenphp_mode: Option<String>,
    env_vars: Vec<ProjectEnvVarSnapshot>,
) -> Result<Project, AppError> {
    let project = ProjectRepository::create(
        connection,
        CreateProjectInput {
            name,
            path,
            domain,
            server_type: map_server_type(&server_type)?,
            php_version,
            framework: map_framework(&framework)?,
            document_root,
            ssl_enabled,
            database_name,
            database_port,
            frankenphp_mode: map_frankenphp_mode(frankenphp_mode)?,
        },
    )?;

    for env_var in env_vars {
        ProjectEnvVarRepository::create(
            connection,
            CreateProjectEnvVarInput {
                project_id: project.id.clone(),
                env_key: env_var.env_key,
                env_value: env_var.env_value,
            },
        )?;
    }

    ProjectRepository::get(connection, &project.id)
}

#[tauri::command]
pub async fn export_project_profile(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectProfileTransferResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let env_vars = ProjectEnvVarRepository::list_by_project(&connection, &project.id)?;

    let target_path = match FileDialog::new()
        .add_filter("DevNest Project Profile", &["json"])
        .set_file_name(&file_name_for_project(&project))
        .save_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };
    let exported_path = target_path.clone();

    let env_vars = snapshot_env_vars(env_vars);
    tauri::async_runtime::spawn_blocking(move || {
        let document = ProjectProfileDocument {
            format_version: 1,
            exported_at: crate::storage::repositories::now_iso()?,
            source: "DevNest".to_string(),
            project: ProjectProfileProjectSnapshot {
                name: project.name,
                path: project.path,
                domain: project.domain,
                server_type: project.server_type.as_str().to_string(),
                php_version: project.php_version,
                framework: project.framework.as_str().to_string(),
                document_root: project.document_root,
                ssl_enabled: project.ssl_enabled,
                database_name: project.database_name,
                database_port: project.database_port,
                frankenphp_mode: Some(project.frankenphp_mode.as_str().to_string()),
                env_vars,
            },
        };

        let payload = serde_json::to_string_pretty(&document).map_err(|error| {
            AppError::with_details(
                "PROJECT_PROFILE_WRITE_FAILED",
                "DevNest could not serialize the selected project profile.",
                error.to_string(),
            )
        })?;

        fs::write(&target_path, payload).map_err(|error| {
            AppError::with_details(
                "PROJECT_PROFILE_WRITE_FAILED",
                "DevNest could not write the exported project profile file.",
                error.to_string(),
            )
        })?;

        Ok::<(), AppError>(())
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_WRITE_FAILED",
            "DevNest could not finish exporting the selected project profile.",
            error.to_string(),
        )
    })??;

    Ok(Some(ProjectProfileTransferResult {
        success: true,
        path: exported_path.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub async fn export_team_project_profile(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectProfileTransferResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let project = ProjectRepository::get(&connection, &project_id)?;
    let env_vars = ProjectEnvVarRepository::list_by_project(&connection, &project.id)?;

    let target_path = match FileDialog::new()
        .add_filter("DevNest Team Project Profile", &["json"])
        .set_file_name(&team_share_file_name_for_project(&project))
        .save_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };
    let root_name_hint = project_root_name_hint(&project);
    let exported_path = target_path.clone();
    let env_vars = snapshot_env_vars(env_vars);
    tauri::async_runtime::spawn_blocking(move || {
        let document = TeamProjectProfileDocument {
            format_version: 1,
            profile_kind: "team-share".to_string(),
            exported_at: crate::storage::repositories::now_iso()?,
            source: "DevNest".to_string(),
            project: TeamProjectProfileSnapshot {
                name: project.name,
                root_name_hint,
                domain: project.domain,
                server_type: project.server_type.as_str().to_string(),
                php_version: project.php_version.clone(),
                framework: project.framework.as_str().to_string(),
                document_root: project.document_root,
                ssl_enabled: project.ssl_enabled,
                database_name: project.database_name,
                database_port: project.database_port,
                frankenphp_mode: Some(project.frankenphp_mode.as_str().to_string()),
                env_vars,
            },
            machine_handoff: TeamProjectHandoffSnapshot {
                notes: vec![
                    "Choose a local project folder when importing on the target machine. DevNest does not reuse the source machine path.".to_string(),
                    format!(
                        "Link or import {} plus PHP {} in Settings before starting the project.",
                        project.server_type.as_str(),
                        project.php_version
                    ),
                    "Run Reliability > Repair Project if local config, hosts, or runtime links drift after import.".to_string(),
                ],
            },
        };

        let payload = serde_json::to_string_pretty(&document).map_err(|error| {
            AppError::with_details(
                "PROJECT_PROFILE_WRITE_FAILED",
                "DevNest could not serialize the selected team-share project profile.",
                error.to_string(),
            )
        })?;

        fs::write(&target_path, payload).map_err(|error| {
            AppError::with_details(
                "PROJECT_PROFILE_WRITE_FAILED",
                "DevNest could not write the exported team-share project profile file.",
                error.to_string(),
            )
        })?;

        Ok::<(), AppError>(())
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_WRITE_FAILED",
            "DevNest could not finish exporting the selected team-share profile.",
            error.to_string(),
        )
    })??;

    Ok(Some(ProjectProfileTransferResult {
        success: true,
        path: exported_path.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub async fn import_project_profile(
    state: tauri::State<'_, AppState>,
) -> Result<Option<Project>, AppError> {
    let source_path = match FileDialog::new()
        .add_filter("DevNest Project Profile", &["json"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let document = read_profile_document(&source_path)?;
        let connection = Connection::open(&db_path)?;
        let project_snapshot = document.project;

        create_project_from_snapshot(
            &connection,
            project_snapshot.name,
            project_snapshot.path,
            project_snapshot.domain,
            project_snapshot.server_type,
            project_snapshot.php_version,
            project_snapshot.framework,
            project_snapshot.document_root,
            project_snapshot.ssl_enabled,
            project_snapshot.database_name,
            project_snapshot.database_port,
            project_snapshot.frankenphp_mode,
            project_snapshot.env_vars,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_READ_FAILED",
            "DevNest could not finish importing the selected project profile.",
            error.to_string(),
        )
    })?
    .map(Some)
}

#[tauri::command]
pub async fn import_team_project_profile(
    state: tauri::State<'_, AppState>,
) -> Result<Option<Project>, AppError> {
    let source_path = match FileDialog::new()
        .add_filter("DevNest Team Project Profile", &["json"])
        .pick_file()
    {
        Some(path) => path,
        None => return Ok(None),
    };

    let target_path = match FileDialog::new().pick_folder() {
        Some(path) => path,
        None => return Ok(None),
    };
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let document = read_team_profile_document(&source_path)?;
        let connection = Connection::open(&db_path)?;
        let project_snapshot = document.project;

        create_project_from_snapshot(
            &connection,
            project_snapshot.name,
            target_path.to_string_lossy().to_string(),
            project_snapshot.domain,
            project_snapshot.server_type,
            project_snapshot.php_version,
            project_snapshot.framework,
            project_snapshot.document_root,
            project_snapshot.ssl_enabled,
            project_snapshot.database_name,
            project_snapshot.database_port,
            project_snapshot.frankenphp_mode,
            project_snapshot.env_vars,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "PROJECT_PROFILE_READ_FAILED",
            "DevNest could not finish importing the selected team-share profile.",
            error.to_string(),
        )
    })?
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::{project_root_name_hint, team_share_file_name_for_project};
    use crate::models::project::{
        FrameworkType, FrankenphpMode, Project, ProjectStatus, ServerType,
    };

    fn sample_project() -> Project {
        Project {
            id: "project-1".to_string(),
            name: "Shop API".to_string(),
            path: r"D:\Projects\shop-api".to_string(),
            domain: "shop-api.test".to_string(),
            server_type: ServerType::Nginx,
            php_version: "8.4".to_string(),
            framework: FrameworkType::Laravel,
            document_root: "public".to_string(),
            ssl_enabled: true,
            database_name: Some("shop_api".to_string()),
            database_port: Some(3306),
            status: ProjectStatus::Stopped,
            frankenphp_mode: FrankenphpMode::Classic,
            created_at: "2026-04-18T00:00:00Z".to_string(),
            updated_at: "2026-04-18T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn builds_team_share_file_name() {
        let project = sample_project();
        assert_eq!(
            team_share_file_name_for_project(&project),
            "shop-api-test.devnest-team-project.json"
        );
    }

    #[test]
    fn derives_project_root_name_hint_from_path() {
        let project = sample_project();
        assert_eq!(project_root_name_hint(&project), "shop-api");
    }
}
