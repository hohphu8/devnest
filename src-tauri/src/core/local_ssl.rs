use crate::error::AppError;
use crate::utils::paths::{
    managed_ssl_authority_cert_der_path, managed_ssl_authority_cert_path,
    managed_ssl_authority_key_der_path, managed_ssl_cert_path, managed_ssl_key_path,
};
use crate::utils::windows::{generate_local_ssl_authority, generate_signed_certificate_pem};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LocalSslAuthority {
    pub cert_path: PathBuf,
    pub cert_der_path: PathBuf,
    pub key_der_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LocalSslMaterial {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

pub fn planned_ssl_authority(workspace_dir: &Path) -> LocalSslAuthority {
    LocalSslAuthority {
        cert_path: managed_ssl_authority_cert_path(workspace_dir),
        cert_der_path: managed_ssl_authority_cert_der_path(workspace_dir),
        key_der_path: managed_ssl_authority_key_der_path(workspace_dir),
    }
}

pub fn ensure_ssl_authority(workspace_dir: &Path) -> Result<LocalSslAuthority, AppError> {
    let authority = planned_ssl_authority(workspace_dir);
    if authority.cert_path.exists()
        && authority.cert_der_path.exists()
        && authority.key_der_path.exists()
    {
        return Ok(authority);
    }

    generate_local_ssl_authority(
        &authority.cert_path,
        &authority.cert_der_path,
        &authority.key_der_path,
    )?;
    Ok(authority)
}

pub fn planned_ssl_material(workspace_dir: &Path, domain: &str) -> LocalSslMaterial {
    LocalSslMaterial {
        cert_path: managed_ssl_cert_path(workspace_dir, domain),
        key_path: managed_ssl_key_path(workspace_dir, domain),
    }
}

fn write_ssl_material(
    workspace_dir: &Path,
    domain: &str,
    overwrite: bool,
) -> Result<LocalSslMaterial, AppError> {
    let authority = ensure_ssl_authority(workspace_dir)?;
    let material = planned_ssl_material(workspace_dir, domain);

    if overwrite {
        fs::remove_file(&material.cert_path).ok();
        fs::remove_file(&material.key_path).ok();
    } else if material.cert_path.exists() && material.key_path.exists() {
        return Ok(material);
    }

    generate_signed_certificate_pem(
        domain,
        &authority.cert_der_path,
        &authority.key_der_path,
        &material.cert_path,
        &material.key_path,
    )?;

    Ok(material)
}

pub fn ensure_ssl_material(
    workspace_dir: &Path,
    domain: &str,
) -> Result<LocalSslMaterial, AppError> {
    write_ssl_material(workspace_dir, domain, false)
}

pub fn regenerate_ssl_material(
    workspace_dir: &Path,
    domain: &str,
) -> Result<LocalSslMaterial, AppError> {
    write_ssl_material(workspace_dir, domain, true)
}

#[cfg(test)]
mod tests {
    use super::{planned_ssl_authority, planned_ssl_material};
    use std::path::Path;

    #[test]
    fn builds_expected_ssl_paths() {
        let authority = planned_ssl_authority(Path::new("D:/workspace"));
        let material = planned_ssl_material(Path::new("D:/workspace"), "demo.test");

        assert!(
            authority
                .cert_path
                .ends_with("ssl\\authority\\devnest-local-ca.pem")
        );
        assert!(
            authority
                .cert_der_path
                .ends_with("ssl\\authority\\devnest-local-ca.der")
        );
        assert!(
            authority
                .key_der_path
                .ends_with("ssl\\authority\\devnest-local-ca.key.der")
        );
        assert!(material.cert_path.ends_with("ssl\\demo.test\\cert.pem"));
        assert!(material.key_path.ends_with("ssl\\demo.test\\key.pem"));
    }
}
