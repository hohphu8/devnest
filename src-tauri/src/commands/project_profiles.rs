use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::frankenphp_octane::UpdateFrankenphpOctaneWorkerSettingsInput;
use crate::models::project::{
    CreateProjectInput, FrameworkType, FrankenphpMode, Project, ServerType,
};
use crate::models::project_env_var::{CreateProjectEnvVarInput, ProjectEnvVar};
use crate::models::runtime::RuntimeType;
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::{PhpExtensionOverrideRepository, RuntimeVersionRepository};
use crate::storage::repositories::{ProjectEnvVarRepository, ProjectRepository};
use rfd::FileDialog;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    env_vars: Vec<ProjectEnvVarSnapshot>,
    #[serde(default)]
    env_keys: Vec<String>,
    #[serde(default)]
    frankenphp: Option<TeamProjectFrankenphpSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectHandoffSnapshot {
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectFrankenphpSnapshot {
    mode: String,
    worker_mode: Option<String>,
    worker_policy: Option<TeamProjectFrankenphpWorkerPolicySnapshot>,
    runtime_requirements: TeamProjectFrankenphpRuntimeRequirementsSnapshot,
    local_intent: TeamProjectFrankenphpLocalIntentSnapshot,
    local_diagnostics: Option<TeamProjectFrankenphpLocalDiagnosticsSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectFrankenphpWorkerPolicySnapshot {
    workers: i64,
    max_requests: i64,
    preferred_worker_port: Option<i64>,
    preferred_admin_port: Option<i64>,
    custom_worker_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectFrankenphpRuntimeRequirementsSnapshot {
    php_family: Option<String>,
    required_extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectFrankenphpLocalIntentSnapshot {
    domain: String,
    ssl_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamProjectFrankenphpLocalDiagnosticsSnapshot {
    current_worker_port: Option<i64>,
    current_admin_port: Option<i64>,
    worker_log_path: Option<String>,
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
    pub warnings: Vec<ProjectProfileCompatibilityWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProfileImportResult {
    pub project: Project,
    pub warnings: Vec<ProjectProfileCompatibilityWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProfileCompatibilityWarning {
    pub code: String,
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
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

fn warning(
    code: &str,
    title: &str,
    message: impl Into<String>,
    suggestion: Option<String>,
) -> ProjectProfileCompatibilityWarning {
    ProjectProfileCompatibilityWarning {
        code: code.to_string(),
        title: title.to_string(),
        message: message.into(),
        suggestion,
    }
}

fn env_key_names(items: &[ProjectEnvVar]) -> Vec<String> {
    let mut keys = items
        .iter()
        .map(|item| item.env_key.trim().to_ascii_uppercase())
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys
}

fn normalize_extension_list(mut values: Vec<String>) -> Vec<String> {
    values = values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| {
            !value.is_empty()
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
        .collect();
    values.sort();
    values.dedup();
    values
}

fn default_required_extensions(
    framework: &FrameworkType,
    database_name: Option<&str>,
) -> Vec<String> {
    let mut extensions = match framework {
        FrameworkType::Laravel => vec![
            "ctype",
            "curl",
            "dom",
            "fileinfo",
            "filter",
            "mbstring",
            "openssl",
            "pdo",
            "session",
            "tokenizer",
            "xml",
        ],
        FrameworkType::Symfony => vec![
            "ctype",
            "iconv",
            "mbstring",
            "openssl",
            "pdo",
            "session",
            "tokenizer",
            "xml",
        ],
        FrameworkType::Wordpress => vec!["curl", "dom", "fileinfo", "json", "mysqli", "openssl"],
        FrameworkType::Php | FrameworkType::Unknown => vec!["mbstring", "openssl"],
    }
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    if database_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        extensions.push("pdo_mysql".to_string());
    }

    normalize_extension_list(extensions)
}

fn active_frankenphp_runtime_details(
    connection: &Connection,
    workspace_dir: &Path,
) -> Result<(Option<String>, Vec<String>, Vec<String>), AppError> {
    let Some(runtime) =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?
    else {
        return Ok((None, Vec::new(), Vec::new()));
    };

    let runtime_path = PathBuf::from(&runtime.path);
    if !runtime_path.exists() {
        return Ok((None, Vec::new(), Vec::new()));
    }

    let runtime_home = runtime_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let php_family = runtime_registry::frankenphp_embedded_php_family(&runtime_path).ok();
    let available_extensions = php_family
        .as_deref()
        .map(|family| {
            runtime_registry::frankenphp_available_php_extensions(
                &runtime_home,
                workspace_dir,
                family,
            )
        })
        .unwrap_or_default();
    let label = php_family
        .as_deref()
        .map(|family| format!("FrankenPHP {} (PHP {family})", runtime.version))
        .unwrap_or_else(|| format!("FrankenPHP {}", runtime.version));
    let enabled_extensions = PhpExtensionOverrideRepository::list_for_runtime(
        connection,
        &runtime.id,
        &label,
        &available_extensions,
    )?
    .into_iter()
    .filter(|extension| extension.enabled)
    .map(|extension| extension.extension_name)
    .collect::<Vec<_>>();

    Ok((
        php_family,
        normalize_extension_list(available_extensions),
        normalize_extension_list(enabled_extensions),
    ))
}

fn build_team_frankenphp_snapshot(
    connection: &Connection,
    workspace_dir: &Path,
    project: &Project,
) -> Result<Option<TeamProjectFrankenphpSnapshot>, AppError> {
    if !matches!(project.server_type, ServerType::Frankenphp) {
        return Ok(None);
    }

    let settings = FrankenphpOctaneWorkerRepository::get(connection, &project.id)?;
    let (active_php_family, _available_extensions, enabled_extensions) =
        active_frankenphp_runtime_details(connection, workspace_dir)?;
    let mut required_extensions =
        default_required_extensions(&project.framework, project.database_name.as_deref());
    required_extensions.extend(enabled_extensions);
    let required_extensions = normalize_extension_list(required_extensions);
    let worker_mode = (!matches!(project.frankenphp_mode, FrankenphpMode::Classic))
        .then(|| project.frankenphp_mode.as_str().to_string());
    let worker_policy =
        settings
            .as_ref()
            .map(|settings| TeamProjectFrankenphpWorkerPolicySnapshot {
                workers: settings.workers,
                max_requests: settings.max_requests,
                preferred_worker_port: Some(settings.worker_port),
                preferred_admin_port: Some(settings.admin_port),
                custom_worker_relative_path: settings.custom_worker_relative_path.clone(),
            });
    let local_diagnostics =
        settings
            .as_ref()
            .map(|settings| TeamProjectFrankenphpLocalDiagnosticsSnapshot {
                current_worker_port: Some(settings.worker_port),
                current_admin_port: Some(settings.admin_port),
                worker_log_path: Some(settings.log_path.clone()),
            });

    Ok(Some(TeamProjectFrankenphpSnapshot {
        mode: project.frankenphp_mode.as_str().to_string(),
        worker_mode,
        worker_policy,
        runtime_requirements: TeamProjectFrankenphpRuntimeRequirementsSnapshot {
            php_family: active_php_family.or_else(|| Some(project.php_version.clone())),
            required_extensions,
        },
        local_intent: TeamProjectFrankenphpLocalIntentSnapshot {
            domain: project.domain.clone(),
            ssl_enabled: project.ssl_enabled,
        },
        local_diagnostics,
    }))
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

    if document.format_version != 1 && document.format_version != 2 {
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

fn mode_from_snapshot_with_warnings(
    server_type: &str,
    frankenphp_mode: Option<String>,
    frankenphp: Option<&TeamProjectFrankenphpSnapshot>,
    warnings: &mut Vec<ProjectProfileCompatibilityWarning>,
) -> Option<String> {
    if server_type != "frankenphp" {
        return frankenphp_mode;
    }

    let candidate = frankenphp
        .map(|snapshot| snapshot.mode.clone())
        .or(frankenphp_mode);
    let Some(mode) = candidate else {
        return Some("classic".to_string());
    };

    if mode.parse::<FrankenphpMode>().is_ok() {
        Some(mode)
    } else {
        warnings.push(warning(
            "FRANKENPHP_WORKER_MODE_UNSUPPORTED",
            "Unsupported FrankenPHP mode",
            format!(
                "The shared profile uses `{mode}`, which this DevNest build cannot enable."
            ),
            Some("Import will keep the project in Classic mode; switch modes after upgrading DevNest.".to_string()),
        ));
        Some("classic".to_string())
    }
}

fn team_profile_compatibility_warnings(
    connection: &Connection,
    workspace_dir: &Path,
    snapshot: &TeamProjectProfileSnapshot,
) -> Result<Vec<ProjectProfileCompatibilityWarning>, AppError> {
    let mut warnings = Vec::new();

    if snapshot.server_type != "frankenphp" {
        return Ok(warnings);
    }

    let frankenphp = snapshot.frankenphp.as_ref();
    let requirements = frankenphp.map(|item| &item.runtime_requirements);
    let required_php_family = requirements
        .and_then(|item| item.php_family.clone())
        .or_else(|| Some(snapshot.php_version.clone()));
    let required_extensions = normalize_extension_list(
        requirements
            .map(|item| item.required_extensions.clone())
            .unwrap_or_else(|| {
                default_required_extensions(
                    &snapshot
                        .framework
                        .parse::<FrameworkType>()
                        .unwrap_or(FrameworkType::Unknown),
                    snapshot.database_name.as_deref(),
                )
            }),
    );
    let active_runtime =
        RuntimeVersionRepository::find_active_by_type(connection, &RuntimeType::Frankenphp)?;

    if active_runtime.is_none() {
        warnings.push(warning(
            "FRANKENPHP_RUNTIME_MISSING",
            "No active FrankenPHP runtime",
            "This profile needs FrankenPHP, but this machine does not have an active FrankenPHP runtime linked.",
            Some("Install or link a FrankenPHP runtime in Settings before starting this project.".to_string()),
        ));
        return Ok(warnings);
    }

    let (active_php_family, available_extensions, enabled_extensions) =
        active_frankenphp_runtime_details(connection, workspace_dir)?;

    if let (Some(required), Some(active)) =
        (required_php_family.as_deref(), active_php_family.as_deref())
    {
        if !runtime_registry::runtime_version_family(required)
            .eq_ignore_ascii_case(&runtime_registry::runtime_version_family(active))
        {
            warnings.push(warning(
                "FRANKENPHP_PHP_FAMILY_MISMATCH",
                "FrankenPHP PHP family mismatch",
                format!(
                    "The profile expects embedded PHP {required}, but the active FrankenPHP runtime reports PHP {active}."
                ),
                Some("Switch to a matching FrankenPHP runtime or update the project profile after import.".to_string()),
            ));
        }
    } else if required_php_family.is_some() {
        warnings.push(warning(
            "FRANKENPHP_PHP_FAMILY_UNKNOWN",
            "FrankenPHP PHP family could not be verified",
            "DevNest could not read the active FrankenPHP embedded PHP family on this machine.",
            Some(
                "Verify the active FrankenPHP build before starting the imported project."
                    .to_string(),
            ),
        ));
    }

    let missing_extensions = required_extensions
        .iter()
        .filter(|extension| !available_extensions.iter().any(|item| item == *extension))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_extensions.is_empty() {
        warnings.push(warning(
            "FRANKENPHP_EXTENSIONS_MISSING",
            "Required PHP extensions missing",
            format!(
                "The active FrankenPHP runtime does not expose: {}.",
                missing_extensions.join(", ")
            ),
            Some(
                "Install or overlay the missing extensions for the active FrankenPHP PHP family."
                    .to_string(),
            ),
        ));
    }

    let disabled_extensions = required_extensions
        .iter()
        .filter(|extension| {
            available_extensions.iter().any(|item| item == *extension)
                && !enabled_extensions.iter().any(|item| item == *extension)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !disabled_extensions.is_empty() {
        warnings.push(warning(
            "FRANKENPHP_EXTENSIONS_DISABLED",
            "Required PHP extensions disabled",
            format!(
                "The active FrankenPHP runtime has these required extensions disabled: {}.",
                disabled_extensions.join(", ")
            ),
            Some(
                "Enable the listed extensions in Settings before starting this project."
                    .to_string(),
            ),
        ));
    }

    if let Some(mode) = frankenphp.map(|item| item.mode.as_str()) {
        if mode.parse::<FrankenphpMode>().is_err() {
            warnings.push(warning(
                "FRANKENPHP_WORKER_MODE_UNSUPPORTED",
                "Unsupported FrankenPHP mode",
                format!("The shared profile references unsupported mode `{mode}`."),
                Some(
                    "Import will continue in Classic mode until this app build supports that mode."
                        .to_string(),
                ),
            ));
        }
    }

    if frankenphp
        .and_then(|item| item.worker_policy.as_ref())
        .and_then(|policy| policy.preferred_worker_port.or(policy.preferred_admin_port))
        .is_some()
    {
        warnings.push(warning(
            "FRANKENPHP_PORTS_PORTABLE_WARNING",
            "Worker ports are local preferences",
            "The shared profile includes source-machine worker/admin ports for diagnostics only. This machine may reuse or allocate different local ports.",
            Some("Check the Runtime tab after import if the preferred ports conflict locally.".to_string()),
        ));
    }

    Ok(warnings)
}

fn apply_team_frankenphp_worker_policy(
    connection: &Connection,
    workspace_dir: &Path,
    project: &Project,
    snapshot: Option<&TeamProjectFrankenphpSnapshot>,
) -> Result<(), AppError> {
    if matches!(project.frankenphp_mode, FrankenphpMode::Classic) {
        return Ok(());
    }

    let Some(policy) = snapshot.and_then(|item| item.worker_policy.as_ref()) else {
        return Ok(());
    };

    let _ = FrankenphpOctaneWorkerRepository::get_or_create_for_mode(
        connection,
        workspace_dir,
        &project.id,
        project.frankenphp_mode.clone(),
    )?;
    FrankenphpOctaneWorkerRepository::update(
        connection,
        workspace_dir,
        &project.id,
        UpdateFrankenphpOctaneWorkerSettingsInput {
            mode: Some(project.frankenphp_mode.clone()),
            worker_port: policy.preferred_worker_port,
            admin_port: policy.preferred_admin_port,
            workers: Some(policy.workers),
            max_requests: Some(policy.max_requests),
            custom_worker_relative_path: Some(policy.custom_worker_relative_path.clone()),
        },
    )?;

    Ok(())
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
        warnings: Vec::new(),
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
    let env_keys = env_key_names(&env_vars);
    let frankenphp = build_team_frankenphp_snapshot(&connection, &state.workspace_dir, &project)?;
    let mut warnings = Vec::new();
    if !env_keys.is_empty() {
        warnings.push(warning(
            "TEAM_PROFILE_ENV_VALUES_OMITTED",
            "Environment values were not exported",
            "Team profiles include env key names only. Secret values from DevNest metadata or `.env` are not written to the shared profile.",
            Some("Share real environment values through your team's secret manager or onboarding docs.".to_string()),
        ));
    }

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
    tauri::async_runtime::spawn_blocking(move || {
        let document = TeamProjectProfileDocument {
            format_version: 2,
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
                env_vars: Vec::new(),
                env_keys,
                frankenphp,
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
        warnings,
    }))
}

#[tauri::command]
pub async fn import_project_profile(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectProfileImportResult>, AppError> {
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

        let project = create_project_from_snapshot(
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
        )?;

        Ok(ProjectProfileImportResult {
            project,
            warnings: Vec::new(),
        })
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
) -> Result<Option<ProjectProfileImportResult>, AppError> {
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
    let workspace_dir = state.workspace_dir.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let document = read_team_profile_document(&source_path)?;
        let connection = Connection::open(&db_path)?;
        let project_snapshot = document.project;
        let mut warnings =
            team_profile_compatibility_warnings(&connection, &workspace_dir, &project_snapshot)?;
        let frankenphp_mode = mode_from_snapshot_with_warnings(
            &project_snapshot.server_type,
            project_snapshot.frankenphp_mode.clone(),
            project_snapshot.frankenphp.as_ref(),
            &mut warnings,
        );

        let project = create_project_from_snapshot(
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
            frankenphp_mode,
            project_snapshot.env_vars,
        )?;
        apply_team_frankenphp_worker_policy(
            &connection,
            &workspace_dir,
            &project,
            project_snapshot.frankenphp.as_ref(),
        )?;

        Ok(ProjectProfileImportResult { project, warnings })
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
