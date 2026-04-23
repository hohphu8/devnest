use crate::models::optional_tool::OptionalToolType;
use crate::models::project::ServerType;
use crate::models::runtime::RuntimeType;
use crate::models::service::ServiceName;
use std::path::{Path, PathBuf};

pub fn managed_config_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("managed-configs")
}

pub fn managed_logs_dir(workspace_dir: &Path) -> PathBuf {
    managed_config_root(workspace_dir).join("logs")
}

pub fn managed_ssl_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("ssl")
}

pub fn managed_ssl_project_dir(workspace_dir: &Path, domain: &str) -> PathBuf {
    managed_ssl_root(workspace_dir).join(domain.replace('/', "-"))
}

pub fn managed_ssl_authority_dir(workspace_dir: &Path) -> PathBuf {
    managed_ssl_root(workspace_dir).join("authority")
}

pub fn managed_ssl_authority_cert_path(workspace_dir: &Path) -> PathBuf {
    managed_ssl_authority_dir(workspace_dir).join("devnest-local-ca.pem")
}

pub fn managed_ssl_authority_cert_der_path(workspace_dir: &Path) -> PathBuf {
    managed_ssl_authority_dir(workspace_dir).join("devnest-local-ca.der")
}

pub fn managed_ssl_authority_key_der_path(workspace_dir: &Path) -> PathBuf {
    managed_ssl_authority_dir(workspace_dir).join("devnest-local-ca.key.der")
}

pub fn managed_ssl_cert_path(workspace_dir: &Path, domain: &str) -> PathBuf {
    managed_ssl_project_dir(workspace_dir, domain).join("cert.pem")
}

pub fn managed_ssl_key_path(workspace_dir: &Path, domain: &str) -> PathBuf {
    managed_ssl_project_dir(workspace_dir, domain).join("key.pem")
}

pub fn managed_runtime_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("runtimes")
}

pub fn managed_cli_shims_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("cli").join("bin")
}

pub fn managed_optional_tool_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("optional-tools")
}

pub fn managed_database_time_machine_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("database-time-machine")
}

pub fn managed_database_time_machine_dir(workspace_dir: &Path, database_name: &str) -> PathBuf {
    managed_database_time_machine_root(workspace_dir).join(database_name)
}

pub fn managed_persistent_tunnel_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("persistent-tunnels").join("cloudflared")
}

pub fn managed_persistent_tunnel_auth_cert_path(workspace_dir: &Path) -> PathBuf {
    managed_persistent_tunnel_root(workspace_dir).join("cert.pem")
}

pub fn managed_persistent_tunnel_credentials_dir(workspace_dir: &Path) -> PathBuf {
    managed_persistent_tunnel_root(workspace_dir).join("credentials")
}

pub fn managed_persistent_tunnel_credentials_path(
    workspace_dir: &Path,
    tunnel_id: &str,
) -> PathBuf {
    managed_persistent_tunnel_credentials_dir(workspace_dir).join(format!("{tunnel_id}.json"))
}

pub fn managed_persistent_tunnel_config_path(workspace_dir: &Path) -> PathBuf {
    managed_persistent_tunnel_root(workspace_dir).join("config.yml")
}

pub fn managed_persistent_tunnel_log_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir
        .join("logs")
        .join("persistent-tunnels")
        .join("cloudflared.log")
}

pub fn downloaded_optional_tool_root(workspace_dir: &Path) -> PathBuf {
    managed_optional_tool_root(workspace_dir).join("downloaded")
}

pub fn downloaded_optional_tool_type_dir(
    workspace_dir: &Path,
    tool_type: &OptionalToolType,
) -> PathBuf {
    downloaded_optional_tool_root(workspace_dir).join(tool_type.as_str())
}

pub fn downloaded_runtime_root(workspace_dir: &Path) -> PathBuf {
    managed_runtime_root(workspace_dir).join("downloaded")
}

pub fn downloaded_runtime_type_dir(workspace_dir: &Path, runtime_type: &RuntimeType) -> PathBuf {
    downloaded_runtime_root(workspace_dir).join(runtime_type.as_str())
}

pub fn managed_runtime_type_dir(workspace_dir: &Path, runtime_type: &RuntimeType) -> PathBuf {
    managed_runtime_root(workspace_dir).join(runtime_type.as_str())
}

pub fn managed_service_state_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("service-state")
}

pub fn managed_service_state_dir(workspace_dir: &Path, service: &ServiceName) -> PathBuf {
    managed_service_state_root(workspace_dir).join(service.as_str())
}

pub fn managed_php_state_dir(workspace_dir: &Path, version: &str) -> PathBuf {
    managed_service_state_root(workspace_dir)
        .join("php")
        .join(version.replace('/', "-"))
}

pub fn bundled_runtime_root(resources_dir: &Path) -> PathBuf {
    resources_dir.join("runtimes")
}

pub fn bundled_runtime_type_dir(resources_dir: &Path, runtime_type: &RuntimeType) -> PathBuf {
    bundled_runtime_root(resources_dir).join(runtime_type.as_str())
}

pub fn managed_server_config_dir(workspace_dir: &Path, server_type: &ServerType) -> PathBuf {
    managed_config_root(workspace_dir)
        .join(server_type.as_str())
        .join("sites")
}

pub fn managed_config_output_path(
    workspace_dir: &Path,
    server_type: &ServerType,
    domain: &str,
) -> PathBuf {
    managed_server_config_dir(workspace_dir, server_type).join(format!("{domain}.conf"))
}

pub fn resolve_document_root(project_path: &Path, document_root: &str) -> PathBuf {
    if document_root.trim() == "." {
        project_path.to_path_buf()
    } else {
        project_path.join(document_root)
    }
}

pub fn normalize_for_config(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
