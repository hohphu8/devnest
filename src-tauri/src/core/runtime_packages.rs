use crate::error::AppError;
use crate::models::runtime::{RuntimeArchiveKind, RuntimePackage, RuntimePackageManifest};
use crate::utils::paths::downloaded_runtime_type_dir;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        env::consts::OS
    }
}

fn current_arch() -> &'static str {
    match env::consts::ARCH {
        "x86_64" => "x64",
        other => other,
    }
}

fn manifest_override_path() -> Option<PathBuf> {
    env::var("DEVNEST_RUNTIME_MANIFEST_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn default_manifest_candidates(resources_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![resources_dir.join("runtimes").join("packages.json")];

    let repo_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("runtimes")
        .join("packages.json");
    if !candidates.contains(&repo_manifest) {
        candidates.push(repo_manifest);
    }

    if let Ok(current_dir) = env::current_dir() {
        let cwd_manifest = current_dir
            .join("src-tauri")
            .join("resources")
            .join("runtimes")
            .join("packages.json");
        if !candidates.contains(&cwd_manifest) {
            candidates.push(cwd_manifest);
        }
    }

    candidates
}

pub fn runtime_manifest_path(resources_dir: &Path) -> Option<PathBuf> {
    if let Some(override_path) = manifest_override_path() {
        return Some(override_path);
    }

    default_manifest_candidates(resources_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn list_runtime_packages(resources_dir: &Path) -> Result<Vec<RuntimePackage>, AppError> {
    let manifest_path = runtime_manifest_path(resources_dir).ok_or_else(|| {
        AppError::new_validation(
            "RUNTIME_MANIFEST_NOT_FOUND",
            "Runtime package manifest was not found. Dev mode expects src-tauri/resources/runtimes/packages.json; packaged builds expect resources/runtimes/packages.json; DEVNEST_RUNTIME_MANIFEST_PATH can override both.",
        )
    })?;

    if !manifest_path.exists() {
        return Err(AppError::new_validation(
            "RUNTIME_MANIFEST_NOT_FOUND",
            "Runtime package manifest path was resolved, but the file does not exist anymore.",
        ));
    }

    let content = fs::read_to_string(&manifest_path).map_err(|error| {
        AppError::with_details(
            "RUNTIME_MANIFEST_READ_FAILED",
            "DevNest could not read the runtime package manifest.",
            error.to_string(),
        )
    })?;

    let manifest: RuntimePackageManifest = serde_json::from_str(&content).map_err(|error| {
        AppError::with_details(
            "RUNTIME_MANIFEST_INVALID",
            "Runtime package manifest is invalid JSON.",
            error.to_string(),
        )
    })?;

    Ok(manifest
        .packages
        .into_iter()
        .filter(|package| {
            package.platform.eq_ignore_ascii_case(current_platform())
                && package.arch.eq_ignore_ascii_case(current_arch())
        })
        .collect())
}

fn archive_file_name(package: &RuntimePackage) -> String {
    match package.archive_kind {
        RuntimeArchiveKind::Zip => {
            format!("{}-{}.zip", package.runtime_type.as_str(), package.version)
        }
    }
}

fn downloads_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("runtime-downloads")
}

fn file_url_to_path(file_url: &str) -> PathBuf {
    let trimmed = file_url.trim_start_matches("file://");
    if trimmed.starts_with('/') && trimmed.chars().nth(2) == Some(':') {
        PathBuf::from(&trimmed[1..])
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn download_runtime_archive(
    package: &RuntimePackage,
    workspace_dir: &Path,
) -> Result<PathBuf, AppError> {
    fs::create_dir_all(downloads_dir(workspace_dir))?;
    let archive_path = downloads_dir(workspace_dir).join(archive_file_name(package));

    if package.download_url.trim().is_empty() {
        return Err(AppError::new_validation(
            "RUNTIME_PACKAGE_URL_MISSING",
            "This runtime package does not define a download URL yet.",
        ));
    }

    if package.download_url.starts_with("file://") {
        let source_path = file_url_to_path(&package.download_url);
        fs::copy(source_path, &archive_path)?;
        return Ok(archive_path);
    }

    if Path::new(&package.download_url).exists() {
        fs::copy(&package.download_url, &archive_path)?;
        return Ok(archive_path);
    }

    if !package.download_url.starts_with("http://") && !package.download_url.starts_with("https://")
    {
        return Err(AppError::new_validation(
            "RUNTIME_PACKAGE_URL_INVALID",
            "Runtime package download URL must be an http(s) URL, file URL, or local archive path.",
        ));
    }

    let client = Client::builder().build().map_err(|error| {
        AppError::with_details(
            "RUNTIME_DOWNLOAD_FAILED",
            "DevNest could not prepare the runtime downloader.",
            error.to_string(),
        )
    })?;

    let mut response = client.get(&package.download_url).send().map_err(|error| {
        AppError::with_details(
            "RUNTIME_DOWNLOAD_FAILED",
            "Runtime package download failed.",
            error.to_string(),
        )
    })?;

    if !response.status().is_success() {
        return Err(AppError::with_details(
            "RUNTIME_DOWNLOAD_FAILED",
            "Runtime package download returned a non-success HTTP status.",
            response.status().to_string(),
        ));
    }

    let mut archive = File::create(&archive_path)?;
    response.copy_to(&mut archive).map_err(|error| {
        AppError::with_details(
            "RUNTIME_DOWNLOAD_FAILED",
            "Runtime package download could not be written to disk.",
            error.to_string(),
        )
    })?;

    Ok(archive_path)
}

pub fn verify_archive_checksum(archive_path: &Path, expected_sha256: &str) -> Result<(), AppError> {
    if expected_sha256.trim().is_empty() {
        return Err(AppError::new_validation(
            "RUNTIME_PACKAGE_CHECKSUM_MISSING",
            "Runtime package manifest must define a SHA-256 checksum before install is allowed.",
        ));
    }

    let mut file = File::open(archive_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Ok(());
    }

    Err(AppError::with_details(
        "RUNTIME_PACKAGE_CHECKSUM_MISMATCH",
        "Downloaded runtime package checksum did not match the manifest.",
        format!("expected={}, actual={actual}", expected_sha256.trim()),
    ))
}

fn sanitize_extract_path(entry_name: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(entry_name);

    for component in path.components() {
        if matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        ) {
            return Err(AppError::new_validation(
                "RUNTIME_ARCHIVE_INVALID",
                "Runtime archive contains an unsafe path entry.",
            ));
        }
    }

    Ok(path)
}

pub fn extract_runtime_archive(
    package: &RuntimePackage,
    archive_path: &Path,
    workspace_dir: &Path,
) -> Result<PathBuf, AppError> {
    let destination_root = downloaded_runtime_type_dir(workspace_dir, &package.runtime_type)
        .join(package.version.replace('/', "-"));

    if destination_root.exists() {
        fs::remove_dir_all(&destination_root)?;
    }
    fs::create_dir_all(&destination_root)?;

    match package.archive_kind {
        RuntimeArchiveKind::Zip => {
            let archive = File::open(archive_path)?;
            let mut zip = ZipArchive::new(archive).map_err(|error| {
                AppError::with_details(
                    "RUNTIME_EXTRACT_FAILED",
                    "Runtime archive is not a valid zip file.",
                    error.to_string(),
                )
            })?;

            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).map_err(|error| {
                    AppError::with_details(
                        "RUNTIME_EXTRACT_FAILED",
                        "DevNest could not read a file from the runtime archive.",
                        error.to_string(),
                    )
                })?;
                let relative_path = sanitize_extract_path(entry.name())?;
                let output_path = destination_root.join(relative_path);

                if entry.is_dir() {
                    fs::create_dir_all(&output_path)?;
                    continue;
                }

                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut output = File::create(&output_path)?;
                std::io::copy(&mut entry, &mut output)?;
                output.flush()?;
            }
        }
    }

    Ok(destination_root)
}

pub fn resolve_package_entry_path(
    package: &RuntimePackage,
    extracted_root: &Path,
) -> Result<PathBuf, AppError> {
    let relative_path = sanitize_extract_path(&package.entry_binary)?;
    let entry_path = extracted_root.join(relative_path);

    if !entry_path.exists() || !entry_path.is_file() {
        return Err(AppError::new_validation(
            "RUNTIME_ENTRY_NOT_FOUND",
            "Runtime package was extracted, but the configured entry binary was not found.",
        ));
    }

    Ok(entry_path)
}

#[cfg(test)]
mod tests {
    use super::{list_runtime_packages, sanitize_extract_path};
    use crate::models::runtime::{
        RuntimeArchiveKind, RuntimePackage, RuntimePackageManifest, RuntimeType,
    };
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // Test code serializes this mutation within the process.
        unsafe { env::set_var(key, value) }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // Test code serializes this mutation within the process.
        unsafe { env::remove_var(key) }
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        let error = sanitize_extract_path("../evil.exe").expect_err("zip-slip path should fail");
        assert_eq!(error.code, "RUNTIME_ARCHIVE_INVALID");
    }

    #[test]
    fn loads_manifest_from_env_override_and_filters_platform() {
        let root = temp_dir("devnest-runtime-manifest");
        fs::create_dir_all(&root).expect("temp root should exist");
        let manifest_path = root.join("packages.json");
        let manifest = RuntimePackageManifest {
            packages: vec![
                RuntimePackage {
                    id: "php-win".to_string(),
                    runtime_type: RuntimeType::Php,
                    version: "8.3.16".to_string(),
                    php_family: Some("8.3".to_string()),
                    platform: "windows".to_string(),
                    arch: "x64".to_string(),
                    display_name: "PHP 8.3.16".to_string(),
                    download_url: "file:///tmp/php.zip".to_string(),
                    checksum_sha256: "abc".to_string(),
                    archive_kind: RuntimeArchiveKind::Zip,
                    entry_binary: "php.exe".to_string(),
                    notes: None,
                },
                RuntimePackage {
                    id: "php-linux".to_string(),
                    runtime_type: RuntimeType::Php,
                    version: "8.3.16".to_string(),
                    php_family: Some("8.3".to_string()),
                    platform: "linux".to_string(),
                    arch: "x64".to_string(),
                    display_name: "PHP 8.3.16".to_string(),
                    download_url: "file:///tmp/php.zip".to_string(),
                    checksum_sha256: "abc".to_string(),
                    archive_kind: RuntimeArchiveKind::Zip,
                    entry_binary: "php.exe".to_string(),
                    notes: None,
                },
            ],
        };
        fs::write(
            &manifest_path,
            serde_json::to_string(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should write");

        set_env_var("DEVNEST_RUNTIME_MANIFEST_PATH", &manifest_path);
        let packages = list_runtime_packages(&root).expect("manifest should load");
        remove_env_var("DEVNEST_RUNTIME_MANIFEST_PATH");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "php-win");

        fs::remove_dir_all(root).ok();
    }
}
