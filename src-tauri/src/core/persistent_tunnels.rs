use crate::error::AppError;
use crate::models::optional_tool::OptionalToolType;
use crate::models::persistent_tunnel::{
    PersistentTunnelManagedSetup, PersistentTunnelProvider, PersistentTunnelSetupStatus,
};
use crate::storage::repositories::{
    OptionalToolVersionRepository, PersistentTunnelSetupRepository,
};
use base64::Engine;
use rusqlite::Connection;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DEVNEST_TUNNEL_BIN: &str = "DEVNEST_TUNNEL_BIN";
const DEVNEST_CLOUDFLARED_CERT_PATH: &str = "DEVNEST_CLOUDFLARED_CERT_PATH";
const DEVNEST_CLOUDFLARED_TUNNEL_ID: &str = "DEVNEST_CLOUDFLARED_TUNNEL_ID";
const DEVNEST_CLOUDFLARED_CREDENTIALS_PATH: &str = "DEVNEST_CLOUDFLARED_CREDENTIALS_PATH";

#[derive(Debug, Clone)]
pub struct PersistentTunnelRuntime {
    pub binary_path: PathBuf,
    pub auth_cert_path: PathBuf,
    pub credentials_path: PathBuf,
    pub tunnel_id: String,
    pub default_hostname_zone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CloudflaredAuthCertMetadata {
    pub zone_id: Option<String>,
    pub api_token: String,
}

fn path_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

fn find_in_path(file_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let mut executable_names = vec![file_name.to_string()];
    if !file_name.contains('.') {
        executable_names.extend(
            [".exe", ".cmd", ".bat"]
                .into_iter()
                .map(|suffix| format!("{file_name}{suffix}")),
        );
    }

    env::split_paths(&path_var)
        .flat_map(|entry| {
            executable_names
                .iter()
                .map(move |name| entry.join(name))
                .collect::<Vec<_>>()
        })
        .find(|candidate| path_exists(candidate))
}

fn resolve_cloudflared_binary(connection: &Connection) -> Option<PathBuf> {
    OptionalToolVersionRepository::find_active_by_type(connection, &OptionalToolType::Cloudflared)
        .ok()
        .flatten()
        .map(|tool| PathBuf::from(tool.path))
        .filter(|path| path_exists(path))
        .or_else(|| {
            env::var(DEVNEST_TUNNEL_BIN)
                .ok()
                .map(|value| PathBuf::from(value.trim()))
                .filter(|path| path_exists(path))
        })
        .or_else(|| {
            [
                PathBuf::from(r"C:\Program Files\cloudflared\cloudflared.exe"),
                PathBuf::from(r"C:\cloudflared\cloudflared.exe"),
            ]
            .into_iter()
            .find(|candidate| path_exists(candidate))
        })
        .or_else(|| find_in_path("cloudflared.exe").or_else(|| find_in_path("cloudflared")))
}

pub fn parse_cloudflared_auth_cert_metadata(
    auth_cert_path: &Path,
) -> Result<CloudflaredAuthCertMetadata, AppError> {
    let content = fs::read_to_string(auth_cert_path).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_AUTH_INVALID",
            "DevNest could not read the cloudflared auth cert file.",
            error.to_string(),
        )
    })?;
    let encoded = content
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .map(str::trim)
        .collect::<String>();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_AUTH_INVALID",
                "DevNest could not decode the cloudflared auth cert payload.",
                error.to_string(),
            )
        })?;
    let document: serde_json::Value = serde_json::from_slice(&decoded).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_AUTH_INVALID",
            "DevNest could not parse the cloudflared auth cert payload.",
            error.to_string(),
        )
    })?;

    let api_token = document
        .get("apiToken")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::new_validation(
                "PERSISTENT_TUNNEL_AUTH_INVALID",
                "Cloudflare API token is missing from the auth cert payload.",
            )
        })?
        .to_string();
    let zone_id = document
        .get("zoneID")
        .or_else(|| document.get("zoneId"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Ok(CloudflaredAuthCertMetadata { zone_id, api_token })
}

pub fn discover_default_zone_from_auth_cert(
    auth_cert_path: &Path,
) -> Result<Option<String>, AppError> {
    let metadata = parse_cloudflared_auth_cert_metadata(auth_cert_path)?;
    let Some(zone_id) = metadata.zone_id else {
        return Ok(None);
    };
    let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_ZONE_DISCOVERY_FAILED",
                "DevNest could not prepare the Cloudflare zone lookup client.",
                error.to_string(),
            )
        })?;
    let response = client
        .get(url)
        .bearer_auth(metadata.api_token)
        .send()
        .map_err(|error| {
            AppError::with_details(
                "PERSISTENT_TUNNEL_ZONE_DISCOVERY_FAILED",
                "DevNest could not query Cloudflare for the default zone.",
                error.to_string(),
            )
        })?;
    let body_text = response.text().map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_ZONE_DISCOVERY_FAILED",
            "DevNest could not read the Cloudflare zone lookup response.",
            error.to_string(),
        )
    })?;
    let body: serde_json::Value = serde_json::from_str(&body_text).map_err(|error| {
        AppError::with_details(
            "PERSISTENT_TUNNEL_ZONE_DISCOVERY_FAILED",
            "DevNest could not parse the Cloudflare zone lookup response.",
            error.to_string(),
        )
    })?;
    let zone_name = body
        .get("result")
        .and_then(|result| result.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    Ok(zone_name)
}

pub fn current_managed_setup(
    connection: &Connection,
) -> Result<Option<PersistentTunnelManagedSetup>, AppError> {
    PersistentTunnelSetupRepository::get(connection, &PersistentTunnelProvider::Cloudflared)
}

pub fn default_cloudflared_home_dir() -> Option<PathBuf> {
    env::var("USERPROFILE")
        .ok()
        .map(PathBuf::from)
        .map(|root| root.join(".cloudflared"))
}

pub fn default_cloudflared_cert_path() -> Option<PathBuf> {
    default_cloudflared_home_dir()
        .map(|root| root.join("cert.pem"))
        .filter(|path| path_exists(path))
}

pub fn default_cloudflared_credentials_path(tunnel_id: &str) -> Option<PathBuf> {
    let trimmed = tunnel_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    default_cloudflared_home_dir()
        .map(|root| root.join(format!("{trimmed}.json")))
        .filter(|path| path_exists(path))
}

pub fn credentials_path_next_to_auth_cert(
    auth_cert_path: &Path,
    tunnel_id: &str,
) -> Option<PathBuf> {
    let trimmed = tunnel_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    auth_cert_path
        .parent()
        .map(|parent| parent.join(format!("{trimmed}.json")))
        .filter(|path| path_exists(path))
}

fn resolve_cert_path(connection: &Connection) -> Result<Option<PathBuf>, AppError> {
    let managed = current_managed_setup(connection)?
        .and_then(|setup| setup.auth_cert_path.map(PathBuf::from))
        .filter(|path| path_exists(path));

    Ok(managed.or_else(|| {
        env::var(DEVNEST_CLOUDFLARED_CERT_PATH)
            .ok()
            .map(|value| PathBuf::from(value.trim()))
            .filter(|path| path_exists(path))
    }))
}

fn resolve_credentials_path(connection: &Connection) -> Result<Option<PathBuf>, AppError> {
    let managed_setup = current_managed_setup(connection)?;
    let managed_credentials = managed_setup
        .as_ref()
        .and_then(|setup| setup.credentials_path.as_ref())
        .map(PathBuf::from)
        .filter(|path| path_exists(path));

    if managed_credentials.is_some() {
        return Ok(managed_credentials);
    }

    let env_credentials = env::var(DEVNEST_CLOUDFLARED_CREDENTIALS_PATH)
        .ok()
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| path_exists(path));

    if env_credentials.is_some() {
        return Ok(env_credentials);
    }

    let fallback_from_auth_cert = managed_setup.as_ref().and_then(|setup| {
        setup
            .auth_cert_path
            .as_ref()
            .zip(setup.tunnel_id.as_ref())
            .and_then(|(auth_cert_path, tunnel_id)| {
                credentials_path_next_to_auth_cert(Path::new(auth_cert_path), tunnel_id)
            })
    });

    Ok(fallback_from_auth_cert)
}

fn resolve_tunnel_id(connection: &Connection) -> Result<Option<String>, AppError> {
    let managed = current_managed_setup(connection)?
        .and_then(|setup| setup.tunnel_id)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(managed.or_else(|| {
        env::var(DEVNEST_CLOUDFLARED_TUNNEL_ID)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }))
}

pub fn resolve_named_tunnel_runtime(
    connection: &Connection,
) -> Result<PersistentTunnelRuntime, AppError> {
    let binary_path = resolve_cloudflared_binary(connection).ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_BINARY_MISSING",
            "Install cloudflared before starting a persistent domain tunnel.",
        )
    })?;
    let setup = current_managed_setup(connection)?;
    let auth_cert_path = resolve_cert_path(connection)?.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_AUTH_MISSING",
            "Connect Cloudflare or import the cloudflared cert before starting a persistent tunnel.",
        )
    })?;
    let credentials_path = resolve_credentials_path(connection)?.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_CREDENTIALS_MISSING",
            "Create or import named tunnel credentials before starting a persistent tunnel.",
        )
    })?;
    let tunnel_id = resolve_tunnel_id(connection)?.ok_or_else(|| {
        AppError::new_validation(
            "PERSISTENT_TUNNEL_ID_MISSING",
            "Select or create a named tunnel before starting a persistent tunnel.",
        )
    })?;

    Ok(PersistentTunnelRuntime {
        binary_path,
        auth_cert_path,
        credentials_path,
        tunnel_id,
        default_hostname_zone: setup.and_then(|item| item.default_hostname_zone),
    })
}

pub fn persistent_tunnel_setup_status(
    connection: &Connection,
) -> Result<PersistentTunnelSetupStatus, AppError> {
    let managed_setup = current_managed_setup(connection)?;
    let binary_path =
        resolve_cloudflared_binary(connection).map(|path| path.to_string_lossy().to_string());
    let auth_cert_path =
        resolve_cert_path(connection)?.map(|path| path.to_string_lossy().to_string());
    let credentials_path =
        resolve_credentials_path(connection)?.map(|path| path.to_string_lossy().to_string());
    let tunnel_id = resolve_tunnel_id(connection)?;
    let tunnel_name = managed_setup
        .as_ref()
        .and_then(|setup| setup.tunnel_name.clone());
    let default_hostname_zone = managed_setup
        .as_ref()
        .and_then(|setup| setup.default_hostname_zone.clone());
    let zone_configured = default_hostname_zone
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let ready = binary_path.is_some()
        && auth_cert_path.is_some()
        && credentials_path.is_some()
        && tunnel_id.is_some();

    let details = if ready && zone_configured {
        "Named tunnel setup is ready. DevNest can publish stable project hostnames through the selected cloudflared tunnel."
            .to_string()
    } else if binary_path.is_none() {
        "Install cloudflared first so DevNest can manage stable public project domains.".to_string()
    } else if auth_cert_path.is_none() {
        "Connect Cloudflare once or import the cloudflared cert so DevNest can manage named tunnels."
            .to_string()
    } else if tunnel_id.is_none() {
        "Create or select a named tunnel so DevNest knows which tunnel should own stable hostnames."
            .to_string()
    } else if !zone_configured {
        "Named tunnel connection is ready. Set a default public zone so projects can auto-publish stable hostnames with one click."
            .to_string()
    } else {
        "Named tunnel credentials are still missing. Create a tunnel or import its credentials JSON so DevNest can publish stable hostnames."
            .to_string()
    };

    let guidance = if ready && zone_configured {
        None
    } else {
        Some(
            "One-time setup: install cloudflared, connect Cloudflare, create or select a named tunnel, then set your default public zone like previews.example.com."
                .to_string(),
        )
    };

    Ok(PersistentTunnelSetupStatus {
        provider: PersistentTunnelProvider::Cloudflared,
        ready,
        managed: managed_setup.is_some(),
        binary_path,
        auth_cert_path,
        credentials_path,
        tunnel_id,
        tunnel_name,
        default_hostname_zone,
        details,
        guidance,
    })
}

#[cfg(test)]
mod tests {
    use super::credentials_path_next_to_auth_cert;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn prefers_credentials_written_next_to_managed_auth_cert() {
        let root = std::env::temp_dir().join(format!("devnest-cloudflared-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root should exist");
        let cert_path = root.join("cert.pem");
        let credentials_path = root.join("demo-tunnel.json");
        fs::write(&cert_path, b"cert").expect("cert file should exist");
        fs::write(&credentials_path, b"{}").expect("credentials file should exist");

        let resolved = credentials_path_next_to_auth_cert(&cert_path, "demo-tunnel")
            .expect("credentials path should resolve");
        assert_eq!(resolved, credentials_path);

        fs::remove_dir_all(root).ok();
    }
}
