use crate::error::AppError;
use crate::models::project::{FrankenphpMode, Project, ServerType};
use crate::state::AppState;
use crate::storage::frankenphp_octane::FrankenphpOctaneWorkerRepository;
use crate::storage::repositories::{ProjectEnvVarRepository, ProjectRepository, now_iso};
use rfd::FileDialog;
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpProductionExportFile {
    pub relative_path: String,
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpProductionExportPreview {
    pub project_id: String,
    pub project_name: String,
    pub slug: String,
    pub generated_at: String,
    pub assumptions: Vec<String>,
    pub warnings: Vec<String>,
    pub files: Vec<FrankenphpProductionExportFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrankenphpProductionExportWriteResult {
    pub success: bool,
    pub path: String,
    pub warnings: Vec<String>,
    pub files: Vec<String>,
}

fn connection_from_state(state: &AppState) -> Result<Connection, AppError> {
    Ok(Connection::open(&state.db_path)?)
}

fn slugify(value: &str) -> String {
    let mut slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();

    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "devnest-project".to_string()
    } else {
        slug
    }
}

fn normalize_relative_path(value: &str) -> Option<String> {
    let trimmed = value.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return None;
    }

    let path = Path::new(&trimmed);
    if path.is_absolute() {
        return None;
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return None;
    }

    Some(trimmed)
}

fn linux_project_root(slug: &str) -> String {
    format!("/var/www/{slug}")
}

fn linux_document_root(project: &Project, slug: &str) -> String {
    let document_root =
        normalize_relative_path(&project.document_root).unwrap_or_else(|| "public".to_string());
    if document_root == "." {
        linux_project_root(slug)
    } else {
        format!("{}/{}", linux_project_root(slug), document_root)
    }
}

fn file_exists_under_project(project: &Project, relative_path: &str) -> bool {
    normalize_relative_path(relative_path)
        .map(|relative| Path::new(&project.path).join(relative).exists())
        .unwrap_or(false)
}

fn composer_has_package(project_path: &Path, package_name: &str) -> bool {
    let Ok(content) = fs::read_to_string(project_path.join("composer.json")) else {
        return false;
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    ["require", "require-dev"].into_iter().any(|section| {
        json.get(section)
            .and_then(|value| value.as_object())
            .map(|packages| packages.contains_key(package_name))
            .unwrap_or(false)
    })
}

fn worker_file_for_project(
    project: &Project,
    custom_worker_relative_path: Option<&str>,
    warnings: &mut Vec<String>,
) -> Option<String> {
    match project.frankenphp_mode {
        FrankenphpMode::Classic => None,
        FrankenphpMode::Octane => {
            let worker = "public/frankenphp-worker.php".to_string();
            if !file_exists_under_project(project, &worker) {
                warnings.push(
                    "Laravel Octane worker file `public/frankenphp-worker.php` was not found locally; run Octane install/update before deploying.".to_string(),
                );
            }
            Some(worker)
        }
        FrankenphpMode::Symfony => Some("public/index.php".to_string()),
        FrankenphpMode::Custom => {
            let Some(worker) = custom_worker_relative_path.and_then(normalize_relative_path) else {
                warnings.push(
                    "Custom Worker mode does not have a valid project-relative worker file saved. The Caddyfile uses `worker.php` as a placeholder.".to_string(),
                );
                return Some("worker.php".to_string());
            };
            Some(worker)
        }
    }
}

fn caddyfile_for_project(
    project: &Project,
    slug: &str,
    worker_file: Option<&str>,
    workers: i64,
    max_requests: i64,
) -> String {
    let root = linux_document_root(project, slug);
    if matches!(project.frankenphp_mode, FrankenphpMode::Classic) {
        return format!(
            "{{\n    auto_https off\n}}\n\n:80 {{\n    root * {root}\n    php_server\n}}\n"
        );
    }

    let worker_file = worker_file.unwrap_or("worker.php");
    let mut worker_env = String::new();
    if matches!(project.frankenphp_mode, FrankenphpMode::Symfony)
        && composer_has_package(Path::new(&project.path), "runtime/frankenphp-symfony")
    {
        worker_env
            .push_str("            env APP_RUNTIME Runtime\\\\FrankenPhpSymfony\\\\Runtime\n");
    }

    format!(
        "{{\n    auto_https off\n}}\n\n:80 {{\n    root * {root}\n    php_server {{\n        worker {{\n            file {worker_file}\n            num {workers}\n            max_requests {max_requests}\n{worker_env}        }}\n    }}\n}}\n"
    )
}

fn systemd_unit(slug: &str) -> String {
    format!(
        "[Unit]\nDescription=DevNest FrankenPHP project {slug}\nAfter=network.target\n\n[Service]\nType=simple\nWorkingDirectory=/var/www/{slug}\nExecStart=/usr/local/bin/frankenphp run --config /etc/devnest/{slug}/Caddyfile\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

fn dockerfile(
    project: &Project,
    slug: &str,
    php_family: Option<&str>,
    warnings: &mut Vec<String>,
) -> String {
    let base = if let Some(family) = php_family.filter(|value| !value.trim().is_empty()) {
        format!("dunglas/frankenphp:php{family}")
    } else {
        warnings.push(
            "Could not infer a FrankenPHP PHP-family Docker base; Dockerfile uses the generic `dunglas/frankenphp` image.".to_string(),
        );
        "dunglas/frankenphp".to_string()
    };
    let document_root =
        normalize_relative_path(&project.document_root).unwrap_or_else(|| "public".to_string());

    format!(
        "FROM {base}\n\nWORKDIR /app\nCOPY . /app\nCOPY Caddyfile /etc/caddy/Caddyfile\n\nENV SERVER_NAME=:80\nENV APP_ENV=prod\n# Install Composer dependencies and PHP extensions required by this app before building a final image.\nEXPOSE 80\nCMD [\"frankenphp\", \"run\", \"--config\", \"/etc/caddy/Caddyfile\"]\n\n# Linux project slug: {slug}\n# Document root expected by the generated Caddyfile: {document_root}\n"
    )
}

fn deployment_notes(
    project: &Project,
    slug: &str,
    required_extensions: &[String],
    env_keys: &[String],
    warnings: &[String],
) -> String {
    let mode = project.frankenphp_mode.as_str();
    let extensions = if required_extensions.is_empty() {
        "- Review Composer requirements and enable needed PHP extensions.".to_string()
    } else {
        required_extensions
            .iter()
            .map(|extension| format!("- {extension}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let env = if env_keys.is_empty() {
        "- No DevNest-tracked env keys were exported. Create production env manually.".to_string()
    } else {
        env_keys
            .iter()
            .map(|key| format!("- {key}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let warning_notes = if warnings.is_empty() {
        "- None generated by DevNest.".to_string()
    } else {
        warnings
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# DevNest FrankenPHP Production Starter\n\nProject: {name}\nMode: {mode}\nLinux root: /var/www/{slug}\nConfig path: /etc/devnest/{slug}/Caddyfile\n\n## Files\n\n- `Caddyfile` - FrankenPHP/Caddy starter config for Linux.\n- `devnest-frankenphp.service` - systemd unit starter.\n- `Dockerfile` - baseline FrankenPHP container starter.\n\n## Required PHP Extensions\n\n{extensions}\n\n## Environment Keys\n\n{env}\n\nDevNest does not export `.env` values or secrets. Set production values through your server, process manager, container runtime, or secret manager.\n\n## Manual Production Steps\n\n- Copy project files to `/var/www/{slug}`.\n- Put `Caddyfile` under `/etc/devnest/{slug}/Caddyfile` or adjust the systemd unit.\n- Install FrankenPHP and required PHP extensions for the expected PHP family.\n- Configure DNS, firewall, production TLS certificates, SSH, CI/CD, and process ownership outside DevNest.\n- Run database migrations and cache warmup commands appropriate for the application.\n\n## Warnings\n\n{warning_notes}\n",
        name = project.name
    )
}

fn required_extensions(project: &Project) -> Vec<String> {
    let mut extensions = match project.frankenphp_mode {
        FrankenphpMode::Octane => vec![
            "ctype",
            "curl",
            "fileinfo",
            "mbstring",
            "openssl",
            "pdo",
            "session",
            "tokenizer",
            "xml",
        ],
        FrankenphpMode::Symfony => vec!["ctype", "iconv", "mbstring", "openssl", "pdo", "xml"],
        FrankenphpMode::Classic | FrankenphpMode::Custom => vec!["mbstring", "openssl"],
    }
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    if project
        .database_name
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        extensions.push("pdo_mysql".to_string());
    }

    extensions.sort();
    extensions.dedup();
    extensions
}

fn build_preview(
    connection: &Connection,
    project_id: &str,
) -> Result<FrankenphpProductionExportPreview, AppError> {
    let project = ProjectRepository::get(connection, project_id)?;
    if !matches!(project.server_type, ServerType::Frankenphp) {
        return Err(AppError::new_validation(
            "FRANKENPHP_PRODUCTION_EXPORT_UNSUPPORTED",
            "Production export is currently available only for FrankenPHP projects.",
        ));
    }

    let slug = slugify(&project.domain);
    let settings = FrankenphpOctaneWorkerRepository::get(connection, &project.id)?;
    let mut warnings = vec![
        "This is a Linux starter recipe. DevNest does not deploy remote servers, configure DNS/firewalls, issue production TLS certificates, or manage CI/CD.".to_string(),
        "Secrets and `.env` values are not exported.".to_string(),
    ];
    let workers = settings.as_ref().map(|item| item.workers).unwrap_or(1);
    let max_requests = settings
        .as_ref()
        .map(|item| item.max_requests)
        .unwrap_or(500);
    let worker_file = worker_file_for_project(
        &project,
        settings
            .as_ref()
            .and_then(|item| item.custom_worker_relative_path.as_deref()),
        &mut warnings,
    );
    let required_extensions = required_extensions(&project);
    let mut env_keys = ProjectEnvVarRepository::list_by_project(connection, &project.id)?
        .into_iter()
        .map(|item| item.env_key)
        .collect::<Vec<_>>();
    env_keys.sort();
    env_keys.dedup();

    let php_family = project.php_version.trim();
    let caddyfile = caddyfile_for_project(
        &project,
        &slug,
        worker_file.as_deref(),
        workers,
        max_requests,
    );
    let service = systemd_unit(&slug);
    let dockerfile = dockerfile(
        &project,
        &slug,
        (!php_family.is_empty()).then_some(php_family),
        &mut warnings,
    );
    let deployment = deployment_notes(&project, &slug, &required_extensions, &env_keys, &warnings);

    Ok(FrankenphpProductionExportPreview {
        project_id: project.id,
        project_name: project.name,
        slug,
        generated_at: now_iso()?,
        assumptions: vec![
            "Target host is Linux with FrankenPHP available as `/usr/local/bin/frankenphp`."
                .to_string(),
            "Project files will live under `/var/www/{project-slug}`.".to_string(),
            "Generated files are starter recipes and should be reviewed before production use."
                .to_string(),
        ],
        warnings,
        files: vec![
            FrankenphpProductionExportFile {
                relative_path: "Caddyfile".to_string(),
                kind: "caddyfile".to_string(),
                content: caddyfile,
            },
            FrankenphpProductionExportFile {
                relative_path: "devnest-frankenphp.service".to_string(),
                kind: "systemd".to_string(),
                content: service,
            },
            FrankenphpProductionExportFile {
                relative_path: "Dockerfile".to_string(),
                kind: "dockerfile".to_string(),
                content: dockerfile,
            },
            FrankenphpProductionExportFile {
                relative_path: "DEPLOYMENT.md".to_string(),
                kind: "markdown".to_string(),
                content: deployment,
            },
        ],
    })
}

fn safe_output_path(root: &Path, relative_path: &str) -> Result<PathBuf, AppError> {
    let Some(relative_path) = normalize_relative_path(relative_path) else {
        return Err(AppError::new_validation(
            "INVALID_EXPORT_FILE_PATH",
            "Generated export file path is not project-relative.",
        ));
    };

    Ok(root.join(relative_path))
}

#[tauri::command]
pub fn preview_frankenphp_production_export(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FrankenphpProductionExportPreview, AppError> {
    let connection = connection_from_state(&state)?;
    build_preview(&connection, &project_id)
}

#[tauri::command]
pub async fn write_frankenphp_production_export(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<FrankenphpProductionExportWriteResult>, AppError> {
    let connection = connection_from_state(&state)?;
    let preview = build_preview(&connection, &project_id)?;
    let target_dir = match FileDialog::new().pick_folder() {
        Some(path) => path,
        None => return Ok(None),
    };
    let output_dir = target_dir.join(&preview.slug);
    let files = preview.files.clone();
    let warnings = preview.warnings.clone();
    let written_paths = tauri::async_runtime::spawn_blocking(move || {
        fs::create_dir_all(&output_dir).map_err(|error| {
            AppError::with_details(
                "FRANKENPHP_PRODUCTION_EXPORT_WRITE_FAILED",
                "DevNest could not create the production export folder.",
                error.to_string(),
            )
        })?;

        let mut written_paths = Vec::new();
        for file in files {
            let path = safe_output_path(&output_dir, &file.relative_path)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    AppError::with_details(
                        "FRANKENPHP_PRODUCTION_EXPORT_WRITE_FAILED",
                        "DevNest could not create a production export subfolder.",
                        error.to_string(),
                    )
                })?;
            }
            fs::write(&path, file.content).map_err(|error| {
                AppError::with_details(
                    "FRANKENPHP_PRODUCTION_EXPORT_WRITE_FAILED",
                    "DevNest could not write a production export file.",
                    error.to_string(),
                )
            })?;
            written_paths.push(path.to_string_lossy().to_string());
        }

        Ok::<Vec<String>, AppError>(written_paths)
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "FRANKENPHP_PRODUCTION_EXPORT_WRITE_FAILED",
            "DevNest could not finish writing the production export.",
            error.to_string(),
        )
    })??;

    Ok(Some(FrankenphpProductionExportWriteResult {
        success: true,
        path: target_dir.join(&preview.slug).to_string_lossy().to_string(),
        warnings,
        files: written_paths,
    }))
}

#[cfg(test)]
mod tests {
    use super::{normalize_relative_path, slugify};

    #[test]
    fn slugifies_project_domain() {
        assert_eq!(slugify("Shop.API.test"), "shop-api-test");
    }

    #[test]
    fn rejects_absolute_or_parent_worker_paths() {
        assert!(normalize_relative_path(r"C:\app\worker.php").is_none());
        assert!(normalize_relative_path("../worker.php").is_none());
        assert_eq!(
            normalize_relative_path("workers\\app.php").as_deref(),
            Some("workers/app.php")
        );
    }
}
