use crate::core::local_ssl;
use crate::core::runtime_registry;
use crate::error::AppError;
use crate::models::project::ProjectStatus;
use crate::models::project::{FrameworkType, FrankenphpMode, Project, ServerType};
use crate::utils::paths::{
    managed_config_output_path, managed_logs_dir, managed_server_config_dir, normalize_for_config,
    resolve_document_root,
};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const PHPMYADMIN_DOMAIN: &str = "phpmyadmin.test";

#[derive(Debug, Clone)]
pub struct RenderedVhostConfig {
    pub server_type: ServerType,
    pub config_text: String,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone)]
struct RenderedSslPaths {
    cert_path: String,
    key_path: String,
}

fn validate_domain(domain: &str) -> Result<String, AppError> {
    let normalized = domain.trim().to_ascii_lowercase();

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
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        });

    if !valid {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_DOMAIN",
            "Project domain must look like a valid local domain.",
        ));
    }

    Ok(normalized)
}

fn validate_project_path(project_path: &Path) -> Result<(), AppError> {
    if !project_path.exists() || !project_path.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_PATH",
            "Project path does not exist or is not a directory.",
        ));
    }

    Ok(())
}

fn validate_document_root(project_path: &Path, document_root: &str) -> Result<PathBuf, AppError> {
    let trimmed = document_root.trim();

    if trimmed.is_empty() {
        return Err(AppError::new_validation(
            "INVALID_DOCUMENT_ROOT",
            "Document root is required.",
        ));
    }

    let root = Path::new(trimmed);
    for component in root.components() {
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

    let resolved = resolve_document_root(project_path, trimmed);
    if !resolved.exists() || !resolved.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_DOCUMENT_ROOT",
            "Document root must point to an existing directory inside the project path.",
        ));
    }

    Ok(resolved)
}

fn validate_framework_document_root(project: &Project) -> Result<(), AppError> {
    if matches!(
        &project.framework,
        FrameworkType::Laravel | FrameworkType::Symfony
    ) && project.document_root.trim() != "public"
    {
        return Err(AppError::new_validation(
            "CONFIG_INVALID_DOCUMENT_ROOT",
            "Laravel and Symfony projects must use `public` as the document root for generated local config.",
        ));
    }

    Ok(())
}

fn apache_directory_block(document_root: &str) -> String {
    format!(
        "<Directory \"{document_root}\">\n    AllowOverride All\n    Require all granted\n    Options Indexes FollowSymLinks\n</Directory>"
    )
}

fn normalized_aliases(aliases: &[String]) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();

    for alias in aliases {
        let value = validate_domain(alias)?;
        if !normalized.iter().any(|existing| existing == &value) {
            normalized.push(value);
        }
    }

    Ok(normalized)
}

fn apache_server_alias_block(aliases: &[String]) -> Result<String, AppError> {
    let aliases = normalized_aliases(aliases)?;
    if aliases.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("    ServerAlias {}\n", aliases.join(" ")))
    }
}

fn nginx_server_name(primary_domain: &str, aliases: &[String]) -> Result<String, AppError> {
    let mut names = vec![primary_domain.to_string()];

    for alias in normalized_aliases(aliases)? {
        if !names.iter().any(|existing| existing == &alias) {
            names.push(alias);
        }
    }

    Ok(names.join(" "))
}

fn frankenphp_site_addresses(
    scheme: &str,
    primary_domain: &str,
    aliases: &[String],
) -> Result<String, AppError> {
    let mut hosts = vec![primary_domain.to_string()];

    for alias in normalized_aliases(aliases)? {
        if !hosts.iter().any(|existing| existing == &alias) {
            hosts.push(alias);
        }
    }

    Ok(hosts
        .into_iter()
        .map(|host| format!("{scheme}://{host}"))
        .collect::<Vec<_>>()
        .join(", "))
}

fn render_apache(
    project: &Project,
    document_root: &str,
    access_log: &str,
    error_log: &str,
    php_port: u16,
    ssl_paths: Option<&RenderedSslPaths>,
) -> Result<String, AppError> {
    let server_alias_block = apache_server_alias_block(&[])?;
    if let Some(ssl_paths) = ssl_paths {
        let http_block = format!(
            "<VirtualHost *:80>\n    ServerName {domain}\n{server_alias_block}    RewriteEngine On\n    RewriteRule ^ https://%{{HTTP_HOST}}%{{REQUEST_URI}} [L,R=301]\n\n    ErrorLog \"{error_log}\"\n    CustomLog \"{access_log}\" combined\n</VirtualHost>\n",
            domain = project.domain,
            server_alias_block = server_alias_block,
            error_log = error_log,
            access_log = access_log,
        );
        Ok(format!(
            "{http_block}\n<VirtualHost *:443>\n    ServerName {domain}\n{server_alias_block}    DocumentRoot \"{document_root}\"\n    DirectoryIndex index.php index.html index.htm\n\n    {directory_block}\n\n    SSLEngine on\n    SSLCertificateFile \"{cert_path}\"\n    SSLCertificateKeyFile \"{key_path}\"\n\n    ProxyPassMatch \"^/(.*\\.php(/.*)?)$\" \"fcgi://127.0.0.1:{php_port}/{document_root}/$1\"\n    ProxyFCGIBackendType GENERIC\n    ProxyFCGISetEnvIf \"true\" REDIRECT_STATUS \"200\"\n    ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" SCRIPT_FILENAME \"$1\"\n    ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" PATH_TRANSLATED \"$1\"\n\n    ErrorLog \"{error_log}\"\n    CustomLog \"{access_log}\" combined\n</VirtualHost>\n",
            http_block = http_block,
            domain = project.domain,
            server_alias_block = server_alias_block,
            document_root = document_root,
            directory_block = apache_directory_block(document_root),
            cert_path = ssl_paths.cert_path,
            key_path = ssl_paths.key_path,
            php_port = php_port,
            error_log = error_log,
            access_log = access_log,
        ))
    } else {
        let http_block = format!(
            "<VirtualHost *:80>\n    ServerName {domain}\n{server_alias_block}    DocumentRoot \"{document_root}\"\n    DirectoryIndex index.php index.html index.htm\n\n    {directory_block}\n\n    ProxyPassMatch \"^/(.*\\.php(/.*)?)$\" \"fcgi://127.0.0.1:{php_port}/{document_root}/$1\"\n    ProxyFCGIBackendType GENERIC\n    ProxyFCGISetEnvIf \"true\" REDIRECT_STATUS \"200\"\n    ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" SCRIPT_FILENAME \"$1\"\n    ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" PATH_TRANSLATED \"$1\"\n\n    ErrorLog \"{error_log}\"\n    CustomLog \"{access_log}\" combined\n</VirtualHost>\n",
            domain = project.domain,
            server_alias_block = server_alias_block,
            document_root = document_root,
            directory_block = apache_directory_block(document_root),
            php_port = php_port,
            error_log = error_log,
            access_log = access_log,
        );
        Ok(http_block)
    }
}

fn nginx_try_files(framework: &FrameworkType) -> &'static str {
    match framework {
        FrameworkType::Wordpress => "try_files $uri $uri/ /index.php?$args;",
        _ => "try_files $uri $uri/ /index.php?$query_string;",
    }
}

fn render_nginx(
    project: &Project,
    document_root: &str,
    access_log: &str,
    error_log: &str,
    php_port: u16,
    ssl_paths: Option<&RenderedSslPaths>,
) -> Result<String, AppError> {
    let server_name = nginx_server_name(&project.domain, &[])?;
    if let Some(ssl_paths) = ssl_paths {
        let http_block = format!(
            "server {{\n    listen 80;\n    server_name {server_name};\n    return 301 https://$host$request_uri;\n}}\n",
            server_name = server_name,
        );
        Ok(format!(
            "{http_block}\nserver {{\n    listen 443 ssl;\n    server_name {server_name};\n    root {document_root};\n    index index.php index.html index.htm;\n\n    ssl_certificate {cert_path};\n    ssl_certificate_key {key_path};\n    access_log {access_log};\n    error_log {error_log} warn;\n\n    location / {{\n        {try_files}\n    }}\n\n    location ~ \\.php$ {{\n        try_files $uri =404;\n        include fastcgi_params;\n        fastcgi_index index.php;\n        fastcgi_param REDIRECT_STATUS 200;\n        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n        fastcgi_param PATH_TRANSLATED $document_root$fastcgi_script_name;\n        fastcgi_param DOCUMENT_ROOT $document_root;\n        fastcgi_pass 127.0.0.1:{php_port};\n    }}\n}}\n",
            http_block = http_block,
            server_name = server_name,
            document_root = document_root,
            cert_path = ssl_paths.cert_path,
            key_path = ssl_paths.key_path,
            access_log = access_log,
            error_log = error_log,
            try_files = nginx_try_files(&project.framework),
            php_port = php_port,
        ))
    } else {
        let http_block = format!(
            "server {{\n    listen 80;\n    server_name {server_name};\n    root {document_root};\n    index index.php index.html index.htm;\n\n    access_log {access_log};\n    error_log {error_log} warn;\n\n    location / {{\n        {try_files}\n    }}\n\n    location ~ \\.php$ {{\n        try_files $uri =404;\n        include fastcgi_params;\n        fastcgi_index index.php;\n        fastcgi_param REDIRECT_STATUS 200;\n        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n        fastcgi_param PATH_TRANSLATED $document_root$fastcgi_script_name;\n        fastcgi_param DOCUMENT_ROOT $document_root;\n        fastcgi_pass 127.0.0.1:{php_port};\n    }}\n}}\n",
            server_name = server_name,
            document_root = document_root,
            access_log = access_log,
            error_log = error_log,
            try_files = nginx_try_files(&project.framework),
            php_port = php_port,
        );
        Ok(http_block)
    }
}

fn render_frankenphp(
    project: &Project,
    document_root: &str,
    access_log: &str,
    _error_log: &str,
    ssl_paths: Option<&RenderedSslPaths>,
    aliases: &[String],
    worker_port: Option<i64>,
) -> Result<String, AppError> {
    let http_addresses = frankenphp_site_addresses("http", &project.domain, aliases)?;

    if !matches!(project.frankenphp_mode, FrankenphpMode::Classic) {
        let worker_port = worker_port.ok_or_else(|| {
            AppError::new_validation(
                "FRANKENPHP_WORKER_SETTINGS_MISSING",
                "FrankenPHP worker mode requires managed worker settings before config generation.",
            )
        })?;

        if let Some(ssl_paths) = ssl_paths {
            let https_addresses = frankenphp_site_addresses("https", &project.domain, aliases)?;
            return Ok(format!(
                "{http_addresses} {{\n    redir https://{primary_domain}{{uri}} 308\n}}\n\n{https_addresses} {{\n    encode zstd br gzip\n    reverse_proxy 127.0.0.1:{worker_port} {{\n        header_up Host {{host}}\n        header_up X-Forwarded-Host {{host}}\n        header_up X-Forwarded-Proto https\n        header_up X-Forwarded-Port 443\n        header_up X-Forwarded-Ssl on\n    }}\n    tls \"{cert_path}\" \"{key_path}\"\n    log {{\n        output file \"{access_log}\"\n    }}\n}}\n",
                http_addresses = http_addresses,
                primary_domain = project.domain,
                https_addresses = https_addresses,
                worker_port = worker_port,
                cert_path = ssl_paths.cert_path,
                key_path = ssl_paths.key_path,
                access_log = access_log,
            ));
        }

        return Ok(format!(
            "{http_addresses} {{\n    encode zstd br gzip\n    reverse_proxy 127.0.0.1:{worker_port} {{\n        header_up Host {{host}}\n        header_up X-Forwarded-Host {{host}}\n        header_up X-Forwarded-Proto http\n        header_up X-Forwarded-Port 80\n    }}\n    log {{\n        output file \"{access_log}\"\n    }}\n}}\n",
            http_addresses = http_addresses,
            worker_port = worker_port,
            access_log = access_log,
        ));
    }

    if let Some(ssl_paths) = ssl_paths {
        let https_addresses = frankenphp_site_addresses("https", &project.domain, aliases)?;
        Ok(format!(
            "{http_addresses} {{\n    redir https://{primary_domain}{{uri}} 308\n}}\n\n{https_addresses} {{\n    root * \"{document_root}\"\n    encode zstd br gzip\n    php_server\n    file_server\n    tls \"{cert_path}\" \"{key_path}\"\n    log {{\n        output file \"{access_log}\"\n    }}\n}}\n",
            http_addresses = http_addresses,
            primary_domain = project.domain,
            https_addresses = https_addresses,
            document_root = document_root,
            cert_path = ssl_paths.cert_path,
            key_path = ssl_paths.key_path,
            access_log = access_log,
        ))
    } else {
        Ok(format!(
            "{http_addresses} {{\n    root * \"{document_root}\"\n    encode zstd br gzip\n    php_server\n    file_server\n    log {{\n        output file \"{access_log}\"\n    }}\n}}\n",
            http_addresses = http_addresses,
            document_root = document_root,
            access_log = access_log,
        ))
    }
}

fn render_ssl_paths(
    project: &Project,
    workspace_dir: &Path,
    ensure_exists: bool,
) -> Result<Option<RenderedSslPaths>, AppError> {
    if !project.ssl_enabled {
        return Ok(None);
    }

    let material = if ensure_exists {
        local_ssl::ensure_ssl_material(workspace_dir, &project.domain)?
    } else {
        local_ssl::planned_ssl_material(workspace_dir, &project.domain)
    };

    Ok(Some(RenderedSslPaths {
        cert_path: normalize_for_config(&material.cert_path),
        key_path: normalize_for_config(&material.key_path),
    }))
}

pub fn preview_config_with_frankenphp_worker_port(
    project: &Project,
    workspace_dir: &Path,
    octane_worker_port: Option<i64>,
) -> Result<RenderedVhostConfig, AppError> {
    render_config(project, workspace_dir, false, &[], octane_worker_port)
}

fn render_config(
    project: &Project,
    workspace_dir: &Path,
    ensure_ssl_material: bool,
    aliases: &[String],
    octane_worker_port: Option<i64>,
) -> Result<RenderedVhostConfig, AppError> {
    let project_path = Path::new(&project.path);
    validate_domain(&project.domain)?;
    validate_project_path(project_path)?;
    validate_framework_document_root(project)?;

    let resolved_document_root = validate_document_root(project_path, &project.document_root)?;
    let output_path =
        managed_config_output_path(workspace_dir, &project.server_type, &project.domain);
    let logs_dir = managed_logs_dir(workspace_dir);
    let php_port = match project.server_type {
        ServerType::Apache | ServerType::Nginx => {
            Some(runtime_registry::php_fastcgi_port(&project.php_version)?)
        }
        ServerType::Frankenphp => None,
    };
    let document_root_for_config = normalize_for_config(&resolved_document_root);
    let access_log = normalize_for_config(&logs_dir.join(format!(
        "{}-{}-access.log",
        project.domain,
        project.server_type.as_str()
    )));
    let error_log = normalize_for_config(&logs_dir.join(format!(
        "{}-{}-error.log",
        project.domain,
        project.server_type.as_str()
    )));
    let ssl_paths = render_ssl_paths(project, workspace_dir, ensure_ssl_material)?;

    let config_text = match &project.server_type {
        ServerType::Apache => render_apache(
            project,
            &document_root_for_config,
            &access_log,
            &error_log,
            php_port.expect("apache configs always require php fastcgi"),
            ssl_paths.as_ref(),
        )?,
        ServerType::Nginx => render_nginx(
            project,
            &document_root_for_config,
            &access_log,
            &error_log,
            php_port.expect("nginx configs always require php fastcgi"),
            ssl_paths.as_ref(),
        )?,
        ServerType::Frankenphp => render_frankenphp(
            project,
            &document_root_for_config,
            &access_log,
            &error_log,
            ssl_paths.as_ref(),
            aliases,
            octane_worker_port,
        )?,
    };

    let config_text = if aliases.is_empty() || matches!(project.server_type, ServerType::Frankenphp)
    {
        config_text
    } else {
        match &project.server_type {
            ServerType::Apache => {
                let server_alias_block = apache_server_alias_block(aliases)?;
                config_text.replacen(
                    &format!("    ServerName {}\n", project.domain),
                    &format!("    ServerName {}\n{}", project.domain, server_alias_block),
                    if project.ssl_enabled { 2 } else { 1 },
                )
            }
            ServerType::Nginx => {
                let server_name = nginx_server_name(&project.domain, aliases)?;
                config_text.replacen(
                    &format!("server_name {};", project.domain),
                    &format!("server_name {};", server_name),
                    if project.ssl_enabled { 2 } else { 1 },
                )
            }
            ServerType::Frankenphp => config_text,
        }
    };

    Ok(RenderedVhostConfig {
        server_type: project.server_type.clone(),
        config_text,
        output_path,
    })
}

pub fn generate_config(
    project: &Project,
    workspace_dir: &Path,
) -> Result<RenderedVhostConfig, AppError> {
    generate_config_with_aliases(project, workspace_dir, &[])
}

pub fn generate_config_with_aliases(
    project: &Project,
    workspace_dir: &Path,
    aliases: &[String],
) -> Result<RenderedVhostConfig, AppError> {
    generate_config_with_aliases_and_frankenphp_worker_port(project, workspace_dir, aliases, None)
}

pub fn generate_config_with_aliases_and_frankenphp_worker_port(
    project: &Project,
    workspace_dir: &Path,
    aliases: &[String],
    octane_worker_port: Option<i64>,
) -> Result<RenderedVhostConfig, AppError> {
    let rendered = render_config(project, workspace_dir, true, aliases, octane_worker_port)?;
    let server_dir = managed_server_config_dir(workspace_dir, &project.server_type);
    let logs_dir = managed_logs_dir(workspace_dir);

    fs::create_dir_all(&server_dir).map_err(|error| {
        AppError::with_details(
            "CONFIG_GENERATION_FAILED",
            "Could not create the managed config directory.",
            error.to_string(),
        )
    })?;
    fs::create_dir_all(&logs_dir).map_err(|error| {
        AppError::with_details(
            "CONFIG_GENERATION_FAILED",
            "Could not create the managed log directory.",
            error.to_string(),
        )
    })?;
    fs::write(&rendered.output_path, &rendered.config_text).map_err(|error| {
        AppError::with_details(
            "CONFIG_GENERATION_FAILED",
            "Could not write the generated config file.",
            error.to_string(),
        )
    })?;

    Ok(rendered)
}

pub fn generate_phpmyadmin_config(
    workspace_dir: &Path,
    install_root: &Path,
    server_type: &ServerType,
    php_version: &str,
) -> Result<RenderedVhostConfig, AppError> {
    let project = Project {
        id: "devnest-phpmyadmin".to_string(),
        name: "phpMyAdmin".to_string(),
        path: install_root.to_string_lossy().to_string(),
        domain: PHPMYADMIN_DOMAIN.to_string(),
        server_type: server_type.clone(),
        php_version: php_version.to_string(),
        framework: FrameworkType::Php,
        document_root: ".".to_string(),
        ssl_enabled: true,
        database_name: None,
        database_port: None,
        status: ProjectStatus::Stopped,
        frankenphp_mode: FrankenphpMode::Classic,
        created_at: "2026-04-19T00:00:00Z".to_string(),
        updated_at: "2026-04-19T00:00:00Z".to_string(),
    };

    generate_config(&project, workspace_dir)
}

pub fn remove_managed_config(
    workspace_dir: &Path,
    server_type: &ServerType,
    domain: &str,
) -> Result<bool, AppError> {
    let config_path = managed_config_output_path(workspace_dir, server_type, domain);
    if !config_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&config_path).map_err(|error| {
        AppError::with_details(
            "CONFIG_GENERATION_FAILED",
            "Could not remove the managed config file.",
            error.to_string(),
        )
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{
        PHPMYADMIN_DOMAIN, generate_config, generate_config_with_aliases,
        generate_phpmyadmin_config, preview_config, preview_config_with_frankenphp_worker_port,
        remove_managed_config,
    };
    use crate::models::project::{
        FrameworkType, FrankenphpMode, Project, ProjectStatus, ServerType,
    };
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn make_workspace() -> PathBuf {
        let workspace =
            std::env::temp_dir().join(format!("devnest-config-workspace-{}", Uuid::new_v4()));
        fs::create_dir_all(&workspace).expect("workspace should be created");
        workspace
    }

    fn make_project_root(public_root: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!("devnest-config-project-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("project root should be created");
        if public_root {
            fs::create_dir_all(root.join("public")).expect("public root should be created");
        }
        root
    }

    fn sample_project(
        project_root: &PathBuf,
        server_type: ServerType,
        framework: FrameworkType,
    ) -> Project {
        Project {
            id: Uuid::new_v4().to_string(),
            name: "Sample".to_string(),
            path: project_root.to_string_lossy().to_string(),
            domain: "sample.test".to_string(),
            server_type,
            php_version: "8.2".to_string(),
            framework,
            document_root: "public".to_string(),
            ssl_enabled: false,
            database_name: None,
            database_port: None,
            status: ProjectStatus::Stopped,
            frankenphp_mode: FrankenphpMode::Classic,
            created_at: "2026-04-17T00:00:00Z".to_string(),
            updated_at: "2026-04-17T00:00:00Z".to_string(),
        }
    }

    fn cleanup(workspace: &PathBuf, project_root: &PathBuf) {
        fs::remove_dir_all(workspace).ok();
        fs::remove_dir_all(project_root).ok();
    }

    #[test]
    fn previews_nginx_config_for_laravel() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let project = sample_project(&project_root, ServerType::Nginx, FrameworkType::Laravel);

        let preview = preview_config_with_frankenphp_worker_port(&project, &workspace, None)
            .expect("preview should succeed");

        assert!(preview.config_text.contains("server_name sample.test;"));
        assert!(preview.config_text.contains("root"));
        assert!(preview.config_text.contains("fastcgi_pass 127.0.0.1:9082;"));
        assert!(
            preview
                .config_text
                .contains("fastcgi_param REDIRECT_STATUS 200;")
        );
        assert!(
            preview
                .config_text
                .contains("fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;")
        );
        assert!(
            preview
                .config_text
                .contains("fastcgi_param PATH_TRANSLATED $document_root$fastcgi_script_name;")
        );
        assert!(preview.output_path.ends_with("sample.test.conf"));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn generates_apache_config_with_server_alias_for_tunnel_host() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let project = sample_project(&project_root, ServerType::Apache, FrameworkType::Laravel);

        let rendered = generate_config_with_aliases(
            &project,
            &workspace,
            &[String::from("bright-lake.trycloudflare.com")],
        )
        .expect("config with aliases should generate");

        assert!(
            rendered
                .config_text
                .contains("ServerAlias bright-lake.trycloudflare.com")
        );

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn generates_apache_config_for_wordpress() {
        let workspace = make_workspace();
        let project_root = make_project_root(false);
        let mut project =
            sample_project(&project_root, ServerType::Apache, FrameworkType::Wordpress);
        project.document_root = ".".to_string();

        let rendered = generate_config(&project, &workspace).expect("generation should succeed");
        let written =
            fs::read_to_string(&rendered.output_path).expect("config file should be written");

        assert!(written.contains("<VirtualHost *:80>"));
        assert!(written.contains("ServerName sample.test"));
        assert!(written.contains("CustomLog"));
        assert!(
            written.contains("ProxyPassMatch \"^/(.*\\.php(/.*)?)$\" \"fcgi://127.0.0.1:9082/")
        );
        assert!(written.contains("$1\""));
        assert!(written.contains("ProxyFCGIBackendType GENERIC"));
        assert!(written.contains("ProxyFCGISetEnvIf \"true\" REDIRECT_STATUS \"200\""));
        assert!(written.contains("ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" SCRIPT_FILENAME \"$1\""));
        assert!(written.contains("ProxyFCGISetEnvIf \"reqenv('SCRIPT_FILENAME') =~ m|^/(.:/.*)$|\" PATH_TRANSLATED \"$1\""));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn rejects_laravel_project_with_non_public_root() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let mut project = sample_project(&project_root, ServerType::Nginx, FrameworkType::Laravel);
        project.document_root = ".".to_string();

        let error = preview_config_with_frankenphp_worker_port(&project, &workspace, None)
            .expect_err("laravel root should be rejected");
        assert_eq!(error.code, "CONFIG_INVALID_DOCUMENT_ROOT");

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn previews_ssl_nginx_config_when_enabled() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let mut project = sample_project(&project_root, ServerType::Nginx, FrameworkType::Laravel);
        project.ssl_enabled = true;

        let preview = preview_config_with_frankenphp_worker_port(&project, &workspace, None)
            .expect("preview should succeed");

        assert!(preview.config_text.contains("listen 443 ssl;"));
        assert!(
            preview
                .config_text
                .contains("return 301 https://$host$request_uri;")
        );
        assert!(preview.config_text.contains("ssl_certificate"));
        assert!(preview.config_text.contains("ssl_certificate_key"));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn previews_ssl_apache_config_with_https_redirect() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let mut project = sample_project(&project_root, ServerType::Apache, FrameworkType::Laravel);
        project.ssl_enabled = true;

        let preview = preview_config_with_frankenphp_worker_port(&project, &workspace, None)
            .expect("preview should succeed");

        assert!(
            preview
                .config_text
                .contains("RewriteRule ^ https://%{HTTP_HOST}%{REQUEST_URI} [L,R=301]")
        );
        assert!(preview.config_text.contains("<VirtualHost *:443>"));
        assert!(preview.config_text.contains("SSLEngine on"));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn previews_frankenphp_config_with_embedded_php_server() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let mut project = sample_project(
            &project_root,
            ServerType::Frankenphp,
            FrameworkType::Laravel,
        );
        project.ssl_enabled = true;

        let preview = preview_config_with_frankenphp_worker_port(&project, &workspace, None)
            .expect("preview should succeed");

        assert!(preview.config_text.contains("https://sample.test"));
        assert!(preview.config_text.contains("php_server"));
        assert!(preview.config_text.contains("file_server"));
        assert!(preview.config_text.contains("tls "));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn previews_frankenphp_octane_config_as_reverse_proxy() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);
        let mut project = sample_project(
            &project_root,
            ServerType::Frankenphp,
            FrameworkType::Laravel,
        );
        project.frankenphp_mode = FrankenphpMode::Octane;

        let preview = preview_config_with_frankenphp_worker_port(&project, &workspace, Some(8123))
            .expect("octane preview should succeed");

        assert!(preview.config_text.contains("reverse_proxy 127.0.0.1:8123"));
        assert!(
            preview
                .config_text
                .contains("header_up X-Forwarded-Proto https")
        );
        assert!(!preview.config_text.contains("php_server"));
        assert!(!preview.config_text.contains("file_server"));

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn previews_all_frankenphp_worker_modes_as_reverse_proxy() {
        let workspace = make_workspace();
        let project_root = make_project_root(true);

        for (mode, framework, port) in [
            (FrankenphpMode::Octane, FrameworkType::Laravel, 8123),
            (FrankenphpMode::Symfony, FrameworkType::Symfony, 8124),
            (FrankenphpMode::Custom, FrameworkType::Php, 8125),
        ] {
            let mut project = sample_project(&project_root, ServerType::Frankenphp, framework);
            project.frankenphp_mode = mode;
            let preview =
                preview_config_with_frankenphp_worker_port(&project, &workspace, Some(port))
                    .expect("worker preview should succeed");

            assert!(
                preview
                    .config_text
                    .contains(&format!("reverse_proxy 127.0.0.1:{port}"))
            );
            assert!(!preview.config_text.contains("php_server"));
        }

        cleanup(&workspace, &project_root);
    }

    #[test]
    fn generates_phpmyadmin_managed_config() {
        let workspace = make_workspace();
        let project_root = make_project_root(false);
        fs::write(project_root.join("index.php"), "<?php").expect("phpmyadmin index should write");

        let rendered =
            generate_phpmyadmin_config(&workspace, &project_root, &ServerType::Apache, "8.2.30")
                .expect("phpmyadmin config should generate");

        assert!(rendered.output_path.exists());
        assert!(rendered.config_text.contains(PHPMYADMIN_DOMAIN));

        let removed = remove_managed_config(&workspace, &ServerType::Apache, PHPMYADMIN_DOMAIN)
            .expect("managed config should remove");
        assert!(removed);
        assert!(!rendered.output_path.exists());

        cleanup(&workspace, &project_root);
    }
}
