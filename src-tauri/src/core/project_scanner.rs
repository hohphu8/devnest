use crate::error::AppError;
use crate::models::project::{FrameworkType, ServerType};
use crate::models::scan::ScanResult;
use serde_json::Value;
use std::fs;
use std::path::Path;

fn normalize_folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn read_json(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn extract_php_version_family(value: &str) -> Option<String> {
    for version in ["8.5", "8.4", "8.3", "8.2", "8.1", "8.0", "7.4"] {
        if value.contains(version) {
            return Some(version.to_string());
        }
    }

    None
}

fn php_hint_from_composer(project_path: &Path) -> Option<String> {
    let composer = read_json(&project_path.join("composer.json"))?;

    if let Some(platform_php) = composer
        .get("config")
        .and_then(|value| value.get("platform"))
        .and_then(|value| value.get("php"))
        .and_then(|value| value.as_str())
        .and_then(extract_php_version_family)
    {
        return Some(platform_php);
    }

    composer
        .get("require")
        .and_then(|value| value.get("php"))
        .and_then(|value| value.as_str())
        .and_then(extract_php_version_family)
}

fn required_extensions_from_composer(project_path: &Path) -> Vec<String> {
    let composer = read_json(&project_path.join("composer.json"));
    let Some(composer) = composer else {
        return Vec::new();
    };

    let Some(require) = composer.get("require").and_then(|value| value.as_object()) else {
        return Vec::new();
    };

    let mut extensions = require
        .keys()
        .filter_map(|key| key.strip_prefix("ext-").map(str::to_string))
        .collect::<Vec<_>>();
    extensions.sort();
    extensions.dedup();
    extensions
}

fn composer_requires_package(project_path: &Path, package_name: &str) -> bool {
    let composer = read_json(&project_path.join("composer.json"));
    let Some(composer) = composer else {
        return false;
    };

    ["require", "require-dev"].iter().any(|section| {
        composer
            .get(section)
            .and_then(|value| value.as_object())
            .map(|packages| packages.contains_key(package_name))
            .unwrap_or(false)
    })
}

fn composer_package_name(project_path: &Path) -> Option<String> {
    read_json(&project_path.join("composer.json"))?
        .get("name")?
        .as_str()
        .map(|value| value.to_ascii_lowercase())
}

fn push_if_exists(base: &Path, relative: &str, detected: &mut Vec<String>) -> bool {
    let exists = base.join(relative).exists();
    if exists {
        detected.push(relative.replace('\\', "/"));
    }
    exists
}

fn detect_htaccess_reason(project_path: &Path, relative_path: &str) -> Option<String> {
    let content = read_text(&project_path.join(relative_path))?;
    let normalized = content.to_ascii_lowercase();

    let signals = [
        ("php_value", "PHP ini overrides"),
        ("php_flag", "PHP flag overrides"),
        ("authtype", "authentication directives"),
        ("fallbackresource", "FallbackResource routing"),
        ("rewriterule", "Apache rewrite rules"),
        ("rewritecond", "Apache rewrite conditions"),
        ("rewriteengine", "Apache rewrite engine directives"),
        ("options ", "Apache directory options"),
        ("errordocument", "Apache error document rules"),
    ];

    for (signal, description) in signals {
        if normalized.contains(signal) {
            return Some(format!("Found {description} in {relative_path}."));
        }
    }

    Some(format!("Found .htaccess at {relative_path}."))
}

fn framework_extension_defaults(framework: &FrameworkType) -> &'static [&'static str] {
    match framework {
        FrameworkType::Laravel => &[
            "bcmath",
            "ctype",
            "fileinfo",
            "intl",
            "mbstring",
            "openssl",
            "pdo_mysql",
            "tokenizer",
            "xml",
        ],
        FrameworkType::Wordpress => &[
            "curl", "dom", "gd", "json", "mysqli", "openssl", "xml", "zip",
        ],
        FrameworkType::Php | FrameworkType::Unknown => &[],
    }
}

fn detect_framework(
    project_path: &Path,
    artisan: bool,
    bootstrap: bool,
    public_index: bool,
    wordpress_config: bool,
    wordpress_content: bool,
    root_index: bool,
) -> FrameworkType {
    let laravel_package = composer_requires_package(project_path, "laravel/framework")
        || composer_package_name(project_path)
            .map(|value| value == "laravel/laravel")
            .unwrap_or(false);

    if (artisan && bootstrap && public_index)
        || (public_index && laravel_package && (artisan || bootstrap))
    {
        FrameworkType::Laravel
    } else if wordpress_config && wordpress_content {
        FrameworkType::Wordpress
    } else if public_index || root_index {
        FrameworkType::Php
    } else {
        FrameworkType::Unknown
    }
}

fn recommend_server(
    framework: &FrameworkType,
    project_path: &Path,
    root_htaccess: bool,
    public_htaccess: bool,
) -> (ServerType, Option<String>) {
    if root_htaccess {
        return (
            ServerType::Apache,
            Some("Found .htaccess at the project root, so Apache is recommended.".to_string()),
        );
    }

    if public_htaccess {
        return (
            ServerType::Apache,
            detect_htaccess_reason(project_path, "public/.htaccess"),
        );
    }

    match framework {
        FrameworkType::Laravel => (
            ServerType::Nginx,
            Some("Laravel markers were found, so Nginx is the default recommendation.".to_string()),
        ),
        FrameworkType::Wordpress => (
            ServerType::Apache,
            Some("WordPress projects usually rely on Apache-style permalink handling.".to_string()),
        ),
        FrameworkType::Php => (
            ServerType::Apache,
            Some("Defaulted to Apache for a generic PHP project.".to_string()),
        ),
        FrameworkType::Unknown => (
            ServerType::Apache,
            Some("Fell back to Apache because no stronger server signal was detected.".to_string()),
        ),
    }
}

fn infer_document_root(
    framework: &FrameworkType,
    public_index: bool,
    root_index: bool,
) -> (String, Option<String>) {
    match framework {
        FrameworkType::Laravel => (
            "public".to_string(),
            Some(
                "Found public/index.php, so DevNest will serve the public/ directory.".to_string(),
            ),
        ),
        FrameworkType::Wordpress => (
            ".".to_string(),
            Some("WordPress serves from the project root by default.".to_string()),
        ),
        FrameworkType::Php if public_index => (
            "public".to_string(),
            Some(
                "Found public/index.php, so DevNest will serve the public/ directory.".to_string(),
            ),
        ),
        FrameworkType::Php if root_index => (
            ".".to_string(),
            Some("Found index.php in the project root.".to_string()),
        ),
        FrameworkType::Php => (
            ".".to_string(),
            Some(
                "Defaulted to the project root because no stronger web-root signal was detected."
                    .to_string(),
            ),
        ),
        FrameworkType::Unknown => (
            ".".to_string(),
            Some(
                "Fell back to the project root because no web entrypoint was confirmed."
                    .to_string(),
            ),
        ),
    }
}

pub fn scan_project(project_path: &Path) -> Result<ScanResult, AppError> {
    if !project_path.exists() || !project_path.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_PATH",
            "Project path does not exist or is not a directory.",
        ));
    }

    let mut detected_files = Vec::new();
    let mut warnings = Vec::new();

    let folder_slug = normalize_folder_name(project_path);
    let suggested_domain = if folder_slug.is_empty() {
        "project.test".to_string()
    } else {
        format!("{folder_slug}.test")
    };

    let recommended_php_version = php_hint_from_composer(project_path);

    let root_index = push_if_exists(project_path, "index.php", &mut detected_files);
    let public_index = push_if_exists(project_path, "public/index.php", &mut detected_files);
    let artisan = push_if_exists(project_path, "artisan", &mut detected_files);
    let bootstrap = push_if_exists(project_path, "bootstrap/app.php", &mut detected_files);
    let wordpress_config = push_if_exists(project_path, "wp-config.php", &mut detected_files)
        || push_if_exists(project_path, "wp-config-sample.php", &mut detected_files);
    let wordpress_content = project_path.join("wp-content").is_dir();
    if wordpress_content {
        detected_files.push("wp-content/".to_string());
    }
    let root_htaccess = push_if_exists(project_path, ".htaccess", &mut detected_files);
    let public_htaccess = push_if_exists(project_path, "public/.htaccess", &mut detected_files);

    let framework = detect_framework(
        project_path,
        artisan,
        bootstrap,
        public_index,
        wordpress_config,
        wordpress_content,
        root_index,
    );

    if matches!(framework, FrameworkType::Unknown) {
        warnings.push("Framework could not be detected automatically.".to_string());
    }

    let (recommended_server, server_reason) =
        recommend_server(&framework, project_path, root_htaccess, public_htaccess);
    let (document_root, document_root_reason) =
        infer_document_root(&framework, public_index, root_index);

    let mut missing_php_extensions = required_extensions_from_composer(project_path);
    for extension in framework_extension_defaults(&framework) {
        if !missing_php_extensions
            .iter()
            .any(|existing| existing == extension)
        {
            missing_php_extensions.push((*extension).to_string());
        }
    }
    missing_php_extensions.sort();
    missing_php_extensions.dedup();

    Ok(ScanResult {
        framework,
        recommended_server,
        server_reason,
        recommended_php_version,
        suggested_domain,
        document_root,
        document_root_reason,
        detected_files,
        warnings,
        missing_php_extensions,
    })
}

#[cfg(test)]
mod tests {
    use super::scan_project;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn make_temp_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!("devnest-scan-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp project root should be created");
        root
    }

    #[test]
    fn detects_laravel_project() {
        let root = make_temp_project();
        fs::create_dir_all(root.join("bootstrap")).expect("bootstrap dir should be created");
        fs::create_dir_all(root.join("public")).expect("public dir should be created");
        fs::write(root.join("artisan"), "").expect("artisan should be created");
        fs::write(root.join("bootstrap/app.php"), "").expect("bootstrap/app.php should be created");
        fs::write(root.join("public/index.php"), "").expect("public/index.php should be created");
        fs::write(
            root.join("composer.json"),
            r#"{ "require": { "php": "^8.2", "ext-intl": "*" } }"#,
        )
        .expect("composer.json should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.framework.as_str(), "laravel");
        assert_eq!(result.recommended_server.as_str(), "nginx");
        assert_eq!(result.document_root, "public");
        assert_eq!(result.recommended_php_version.as_deref(), Some("8.2"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn detects_wordpress_project() {
        let root = make_temp_project();
        fs::create_dir_all(root.join("wp-content")).expect("wp-content dir should be created");
        fs::write(root.join("wp-config.php"), "").expect("wp-config.php should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.framework.as_str(), "wordpress");
        assert_eq!(result.recommended_server.as_str(), "apache");
        assert_eq!(result.document_root, ".");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn detects_plain_php_project() {
        let root = make_temp_project();
        fs::write(root.join("index.php"), "").expect("index.php should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.framework.as_str(), "php");
        assert_eq!(result.recommended_server.as_str(), "apache");
        assert_eq!(result.document_root, ".");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn detects_public_index_php_project_without_root_index() {
        let root = make_temp_project();
        fs::create_dir_all(root.join("public")).expect("public dir should be created");
        fs::write(root.join("public/index.php"), "").expect("public/index.php should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.framework.as_str(), "php");
        assert_eq!(result.document_root, "public");
        assert_eq!(
            result.document_root_reason.as_deref(),
            Some("Found public/index.php, so DevNest will serve the public/ directory.")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn prefers_apache_when_htaccess_rules_are_detected() {
        let root = make_temp_project();
        fs::create_dir_all(root.join("bootstrap")).expect("bootstrap dir should be created");
        fs::create_dir_all(root.join("public")).expect("public dir should be created");
        fs::write(root.join("artisan"), "").expect("artisan should be created");
        fs::write(root.join("bootstrap/app.php"), "").expect("bootstrap/app.php should be created");
        fs::write(root.join("public/index.php"), "").expect("public/index.php should be created");
        fs::write(
            root.join("public/.htaccess"),
            "RewriteEngine On\nRewriteRule ^ index.php [L]\n",
        )
        .expect("public/.htaccess should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.recommended_server.as_str(), "apache");
        assert!(
            result
                .server_reason
                .as_deref()
                .unwrap_or_default()
                .contains("rewrite")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reads_php_hint_from_composer_platform() {
        let root = make_temp_project();
        fs::write(root.join("index.php"), "").expect("index.php should be created");
        fs::write(
            root.join("composer.json"),
            r#"{ "config": { "platform": { "php": "8.1.18" } } }"#,
        )
        .expect("composer.json should be created");

        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.recommended_php_version.as_deref(), Some("8.1"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn falls_back_to_unknown_when_no_markers_exist() {
        let root = make_temp_project();
        let result = scan_project(&root).expect("scan should succeed");
        assert_eq!(result.framework.as_str(), "unknown");
        assert!(!result.warnings.is_empty());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_invalid_project_path() {
        let root = std::env::temp_dir().join(format!("devnest-scan-missing-{}", Uuid::new_v4()));
        let result = scan_project(&root).expect_err("scan should fail");
        assert_eq!(result.code, "INVALID_PROJECT_PATH");
    }
}
