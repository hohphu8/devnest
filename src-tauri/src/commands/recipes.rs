use crate::core::project_scanner;
use crate::error::AppError;
use crate::models::project::{CreateProjectInput, Project, ServerType};
use crate::state::AppState;
use crate::storage::repositories::ProjectRepository;
use crate::utils::process::configure_background_command;
use rusqlite::Connection;
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldRecipeInput {
    pub path: String,
    pub domain: String,
    pub php_version: String,
    pub server_type: ServerType,
    pub ssl_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneGitRecipeInput {
    pub repository_url: String,
    pub path: String,
    pub domain: String,
    pub php_version: String,
    pub server_type: ServerType,
    pub ssl_enabled: bool,
    pub branch: Option<String>,
}

fn validate_recipe_target(path: &str) -> Result<PathBuf, AppError> {
    let target = PathBuf::from(path.trim());
    if path.trim().is_empty() {
        return Err(AppError::new_validation(
            "INVALID_RECIPE_PATH",
            "Recipe target path is required.",
        ));
    }

    if target.exists() {
        return Err(AppError::new_validation(
            "RECIPE_TARGET_EXISTS",
            "Recipe target path already exists. Choose a new empty path.",
        ));
    }

    let parent = target.parent().ok_or_else(|| {
        AppError::new_validation(
            "INVALID_RECIPE_PATH",
            "Recipe target path must include a parent folder.",
        )
    })?;

    fs::create_dir_all(parent)?;
    Ok(target)
}

fn tool_missing_error(tool: &str, install_hint: &str) -> AppError {
    AppError::new_validation(
        "RECIPE_TOOL_MISSING",
        format!("`{tool}` was not found. {install_hint}"),
    )
}

#[cfg(target_os = "windows")]
fn windows_tool_names(tool: &str) -> Vec<String> {
    match tool {
        "composer" => vec![
            "composer.bat".to_string(),
            "composer.cmd".to_string(),
            "composer.exe".to_string(),
            "composer".to_string(),
        ],
        "git" => vec![
            "git.exe".to_string(),
            "git.cmd".to_string(),
            "git.bat".to_string(),
            "git".to_string(),
        ],
        _ => vec![
            format!("{tool}.exe"),
            format!("{tool}.cmd"),
            format!("{tool}.bat"),
            tool.to_string(),
        ],
    }
}

#[cfg(target_os = "windows")]
fn windows_tool_hints(tool: &str) -> Vec<PathBuf> {
    let mut hints = Vec::new();
    match tool {
        "composer" => {
            if let Some(program_data) = env::var_os("ProgramData") {
                hints.push(
                    PathBuf::from(program_data)
                        .join("ComposerSetup")
                        .join("bin"),
                );
            }
            if let Some(app_data) = env::var_os("APPDATA") {
                hints.push(PathBuf::from(app_data).join("ComposerSetup").join("bin"));
            }
        }
        "git" => {
            for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(program_files) = env::var_os(key) {
                    let base = PathBuf::from(program_files).join("Git");
                    hints.push(base.join("cmd"));
                    hints.push(base.join("bin"));
                }
            }
        }
        _ => {}
    }
    hints
}

#[cfg(target_os = "windows")]
fn resolve_windows_tool(tool: &str) -> Option<PathBuf> {
    let names = windows_tool_names(tool);
    let mut search_roots: Vec<PathBuf> = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    search_roots.extend(windows_tool_hints(tool));

    for root in search_roots {
        for name in &names {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn resolve_recipe_tool(tool: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(path) = resolve_windows_tool(tool) {
            return path;
        }
    }

    PathBuf::from(tool)
}

fn build_recipe_command(tool_path: &Path, args: &[String], working_dir: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let extension = tool_path
            .extension()
            .and_then(OsStr::to_str)
            .map(|value| value.to_ascii_lowercase());
        if matches!(extension.as_deref(), Some("bat" | "cmd")) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(tool_path);
            command.args(args);
            command.current_dir(working_dir);
            configure_background_command(&mut command);
            return command;
        }
    }

    let mut command = Command::new(tool_path);
    command.args(args);
    command.current_dir(working_dir);
    configure_background_command(&mut command);
    command
}

fn run_recipe_command(
    tool: &str,
    args: &[String],
    working_dir: &Path,
    install_hint: &str,
) -> Result<(), AppError> {
    let tool_path = resolve_recipe_tool(tool);
    let output = build_recipe_command(&tool_path, args, working_dir)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                tool_missing_error(tool, install_hint)
            } else {
                AppError::with_details(
                    "RECIPE_COMMAND_FAILED",
                    format!("Could not start `{tool}` for the selected recipe."),
                    error.to_string(),
                )
            }
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() { stderr } else { stdout };

    Err(AppError::with_details(
        "RECIPE_COMMAND_FAILED",
        format!("`{tool}` did not finish successfully for the selected recipe."),
        details,
    ))
}

fn composer_create_project_args(package: &str, target: &Path) -> Vec<String> {
    vec![
        "create-project".to_string(),
        "--no-interaction".to_string(),
        "--no-progress".to_string(),
        "--prefer-dist".to_string(),
        package.to_string(),
        target.to_string_lossy().to_string(),
    ]
}

fn composer_allow_plugin_args(plugin: &str) -> Vec<String> {
    vec![
        "config".to_string(),
        "--no-interaction".to_string(),
        "--no-plugins".to_string(),
        format!("allow-plugins.{plugin}"),
        "true".to_string(),
    ]
}

fn composer_install_args() -> Vec<String> {
    vec![
        "install".to_string(),
        "--no-interaction".to_string(),
        "--no-progress".to_string(),
        "--prefer-dist".to_string(),
    ]
}

fn flatten_wordpress_target(target: &Path) -> Result<(), AppError> {
    let nested_root = target.join("wordpress");
    if !nested_root.is_dir() {
        return Ok(());
    }

    let already_flattened = target.join("index.php").is_file()
        || target.join("wp-admin").is_dir()
        || target.join("wp-includes").is_dir();
    if already_flattened {
        return Ok(());
    }

    fn should_drop_nested_wordpress_entry(name: &str) -> bool {
        matches!(
            name,
            "composer.json" | "composer.lock" | ".gitignore" | ".gitattributes"
        )
    }

    let entries = fs::read_dir(&nested_root).map_err(|error| {
        AppError::with_details(
            "WORDPRESS_FLATTEN_FAILED",
            "DevNest could not inspect the generated WordPress directory.",
            error.to_string(),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::with_details(
                "WORDPRESS_FLATTEN_FAILED",
                "DevNest could not read a generated WordPress file entry.",
                error.to_string(),
            )
        })?;
        let file_name = entry.file_name();
        let file_name_text = file_name.to_string_lossy().to_string();
        let source = entry.path();
        if should_drop_nested_wordpress_entry(&file_name_text) {
            if source.is_dir() {
                fs::remove_dir_all(&source).map_err(|error| {
                    AppError::with_details(
                        "WORDPRESS_FLATTEN_FAILED",
                        "DevNest could not clean nested WordPress metadata before flattening.",
                        error.to_string(),
                    )
                })?;
            } else {
                fs::remove_file(&source).map_err(|error| {
                    AppError::with_details(
                        "WORDPRESS_FLATTEN_FAILED",
                        "DevNest could not clean nested WordPress metadata before flattening.",
                        error.to_string(),
                    )
                })?;
            }
            continue;
        }

        let destination = target.join(&file_name);
        if destination.exists() {
            return Err(AppError::new_validation(
                "WORDPRESS_FLATTEN_CONFLICT",
                format!(
                    "DevNest could not flatten the WordPress recipe because `{}` already exists in the project root.",
                    destination.to_string_lossy()
                ),
            ));
        }

        fs::rename(&source, &destination).map_err(|error| {
            AppError::with_details(
                "WORDPRESS_FLATTEN_FAILED",
                "DevNest could not move the generated WordPress files into the project root.",
                error.to_string(),
            )
        })?;
    }

    fs::remove_dir_all(&nested_root).map_err(|error| {
        AppError::with_details(
            "WORDPRESS_FLATTEN_FAILED",
            "DevNest could not remove the temporary nested WordPress directory after flattening.",
            error.to_string(),
        )
    })?;

    Ok(())
}

fn project_name_from_target(target: &Path) -> Result<String, AppError> {
    target
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::new_validation(
                "INVALID_RECIPE_PATH",
                "Recipe target path must end with a project folder name.",
            )
        })
}

fn create_project_from_recipe(
    connection: &Connection,
    target: &Path,
    domain: String,
    php_version: String,
    server_type: ServerType,
    ssl_enabled: bool,
) -> Result<Project, AppError> {
    let scan = project_scanner::scan_project(target)?;
    let name = project_name_from_target(target)?;

    ProjectRepository::create(
        connection,
        CreateProjectInput {
            name,
            path: target.to_string_lossy().to_string(),
            domain,
            server_type,
            php_version,
            framework: scan.framework,
            document_root: scan.document_root,
            ssl_enabled,
            database_name: None,
            database_port: None,
            frankenphp_mode: None,
        },
    )
}

#[tauri::command]
pub async fn create_laravel_recipe(
    input: ScaffoldRecipeInput,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let connection = Connection::open(&db_path)?;
        let target = validate_recipe_target(&input.path)?;
        let parent = target.parent().ok_or_else(|| {
            AppError::new_validation(
                "INVALID_RECIPE_PATH",
                "Recipe target path must include a parent folder.",
            )
        })?;

        run_recipe_command(
            "composer",
            &composer_create_project_args("laravel/laravel", &target),
            parent,
            "Install Composer first so DevNest can scaffold Laravel projects.",
        )?;

        create_project_from_recipe(
            &connection,
            &target,
            input.domain,
            input.php_version,
            input.server_type,
            input.ssl_enabled,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "RECIPE_COMMAND_JOIN_FAILED",
            "Laravel scaffolding did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn create_wordpress_recipe(
    input: ScaffoldRecipeInput,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let connection = Connection::open(&db_path)?;
        let target = validate_recipe_target(&input.path)?;
        let parent = target.parent().ok_or_else(|| {
            AppError::new_validation(
                "INVALID_RECIPE_PATH",
                "Recipe target path must include a parent folder.",
            )
        })?;

        run_recipe_command(
            "composer",
            &{
                let mut args = composer_create_project_args("johnpbloch/wordpress", &target);
                args.insert(3, "--no-install".to_string());
                args
            },
            parent,
            "Install Composer first so DevNest can scaffold WordPress projects.",
        )?;

        run_recipe_command(
            "composer",
            &composer_allow_plugin_args("johnpbloch/wordpress-core-installer"),
            &target,
            "Install Composer first so DevNest can scaffold WordPress projects.",
        )?;

        run_recipe_command(
            "composer",
            &composer_install_args(),
            &target,
            "Install Composer first so DevNest can scaffold WordPress projects.",
        )?;
        flatten_wordpress_target(&target)?;

        create_project_from_recipe(
            &connection,
            &target,
            input.domain,
            input.php_version,
            input.server_type,
            input.ssl_enabled,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "RECIPE_COMMAND_JOIN_FAILED",
            "WordPress scaffolding did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn clone_git_recipe(
    input: CloneGitRecipeInput,
    state: tauri::State<'_, AppState>,
) -> Result<Project, AppError> {
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let connection = Connection::open(&db_path)?;
        let target = validate_recipe_target(&input.path)?;
        let parent = target.parent().ok_or_else(|| {
            AppError::new_validation(
                "INVALID_RECIPE_PATH",
                "Recipe target path must include a parent folder.",
            )
        })?;

        let mut args = vec!["clone".to_string()];
        if let Some(branch) = input
            .branch
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            args.push("--branch".to_string());
            args.push(branch.to_string());
        }
        args.push(input.repository_url.trim().to_string());
        args.push(target.to_string_lossy().to_string());

        run_recipe_command(
            "git",
            &args,
            parent,
            "Install Git first so DevNest can clone repositories.",
        )?;

        create_project_from_recipe(
            &connection,
            &target,
            input.domain,
            input.php_version,
            input.server_type,
            input.ssl_enabled,
        )
    })
    .await
    .map_err(|error| {
        AppError::with_details(
            "RECIPE_COMMAND_JOIN_FAILED",
            "Git clone did not finish cleanly.",
            error.to_string(),
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::flatten_wordpress_target;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("devnest-{prefix}-{stamp}"));
        fs::create_dir_all(&root).expect("temp root should be created");
        root
    }

    #[test]
    fn flattens_nested_wordpress_core_into_project_root() {
        let root = make_temp_dir("wordpress-flatten");
        let nested = root.join("wordpress");
        fs::create_dir_all(nested.join("wp-admin")).expect("wp-admin should be created");
        fs::create_dir_all(nested.join("wp-content")).expect("wp-content should be created");
        fs::write(nested.join("index.php"), "<?php").expect("index.php should be created");

        flatten_wordpress_target(&root).expect("flatten should succeed");

        assert!(root.join("index.php").is_file());
        assert!(root.join("wp-admin").is_dir());
        assert!(root.join("wp-content").is_dir());
        assert!(!root.join("wordpress").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn ignores_nested_wordpress_metadata_when_root_already_has_recipe_files() {
        let root = make_temp_dir("wordpress-flatten-metadata");
        let nested = root.join("wordpress");
        fs::create_dir_all(nested.join("wp-admin")).expect("wp-admin should be created");
        fs::write(root.join("composer.json"), "{\"name\":\"root\"}")
            .expect("root composer.json should be created");
        fs::write(nested.join("composer.json"), "{\"name\":\"nested\"}")
            .expect("nested composer.json should be created");
        fs::write(nested.join("index.php"), "<?php").expect("index.php should be created");

        flatten_wordpress_target(&root).expect("flatten should succeed");

        assert!(root.join("composer.json").is_file());
        assert!(root.join("index.php").is_file());
        assert!(root.join("wp-admin").is_dir());
        assert!(!root.join("wordpress").exists());
        fs::remove_dir_all(root).ok();
    }
}
