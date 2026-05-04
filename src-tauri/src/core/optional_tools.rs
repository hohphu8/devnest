use crate::error::AppError;
use crate::models::optional_tool::{
    OptionalToolArchiveKind, OptionalToolPackage, OptionalToolPackageManifest,
};
use crate::utils::paths::downloaded_optional_tool_type_dir;
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
    env::var("DEVNEST_OPTIONAL_TOOL_MANIFEST_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn default_manifest_candidates(resources_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![resources_dir.join("optional-tools").join("packages.json")];

    let repo_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("optional-tools")
        .join("packages.json");
    if !candidates.contains(&repo_manifest) {
        candidates.push(repo_manifest);
    }

    if let Ok(current_dir) = env::current_dir() {
        let cwd_manifest = current_dir
            .join("src-tauri")
            .join("resources")
            .join("optional-tools")
            .join("packages.json");
        if !candidates.contains(&cwd_manifest) {
            candidates.push(cwd_manifest);
        }
    }

    candidates
}

pub fn optional_tool_manifest_path(resources_dir: &Path) -> Option<PathBuf> {
    if let Some(override_path) = manifest_override_path() {
        return Some(override_path);
    }

    default_manifest_candidates(resources_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn list_optional_tool_packages(
    resources_dir: &Path,
) -> Result<Vec<OptionalToolPackage>, AppError> {
    let manifest_path = optional_tool_manifest_path(resources_dir).ok_or_else(|| {
        AppError::new_validation(
            "OPTIONAL_TOOL_MANIFEST_NOT_FOUND",
            "Optional tool package manifest was not found. Dev mode expects src-tauri/resources/optional-tools/packages.json; packaged builds expect resources/optional-tools/packages.json; DEVNEST_OPTIONAL_TOOL_MANIFEST_PATH can override both.",
        )
    })?;

    let content = fs::read_to_string(&manifest_path).map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_MANIFEST_READ_FAILED",
            "DevNest could not read the optional tool package manifest.",
            error.to_string(),
        )
    })?;

    let manifest: OptionalToolPackageManifest =
        serde_json::from_str(&content).map_err(|error| {
            AppError::with_details(
                "OPTIONAL_TOOL_MANIFEST_INVALID",
                "Optional tool package manifest is invalid JSON.",
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

fn archive_file_name(package: &OptionalToolPackage) -> String {
    match package.archive_kind {
        OptionalToolArchiveKind::Zip => {
            format!("{}-{}.zip", package.tool_type.as_str(), package.version)
        }
        OptionalToolArchiveKind::Binary => package
            .entry_binary
            .rsplit(['\\', '/'])
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("{}-{}.exe", package.tool_type.as_str(), package.version)),
    }
}

fn downloads_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("tool-downloads")
}

fn file_url_to_path(file_url: &str) -> PathBuf {
    let trimmed = file_url.trim_start_matches("file://");
    if trimmed.starts_with('/') && trimmed.chars().nth(2) == Some(':') {
        PathBuf::from(&trimmed[1..])
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn download_optional_tool_archive(
    package: &OptionalToolPackage,
    workspace_dir: &Path,
) -> Result<PathBuf, AppError> {
    fs::create_dir_all(downloads_dir(workspace_dir))?;
    let archive_path = downloads_dir(workspace_dir).join(archive_file_name(package));

    if package.download_url.trim().is_empty() {
        return Err(AppError::new_validation(
            "OPTIONAL_TOOL_PACKAGE_URL_MISSING",
            "This optional tool package does not define a download URL yet.",
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

    let client = Client::builder().build().map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_DOWNLOAD_FAILED",
            "DevNest could not prepare the optional tool downloader.",
            error.to_string(),
        )
    })?;

    let mut response = client.get(&package.download_url).send().map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_DOWNLOAD_FAILED",
            "Optional tool package download failed.",
            error.to_string(),
        )
    })?;

    if !response.status().is_success() {
        return Err(AppError::with_details(
            "OPTIONAL_TOOL_DOWNLOAD_FAILED",
            "Optional tool package download returned a non-success HTTP status.",
            response.status().to_string(),
        ));
    }

    let mut archive = File::create(&archive_path)?;
    response.copy_to(&mut archive).map_err(|error| {
        AppError::with_details(
            "OPTIONAL_TOOL_DOWNLOAD_FAILED",
            "Optional tool package download could not be written to disk.",
            error.to_string(),
        )
    })?;

    Ok(archive_path)
}

pub fn verify_archive_checksum(
    archive_path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), AppError> {
    let Some(expected_sha256) = expected_sha256
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

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
    if actual.eq_ignore_ascii_case(expected_sha256) {
        return Ok(());
    }

    Err(AppError::with_details(
        "OPTIONAL_TOOL_PACKAGE_CHECKSUM_MISMATCH",
        "Downloaded optional tool package checksum did not match the manifest.",
        format!("expected={}, actual={actual}", expected_sha256),
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
                "OPTIONAL_TOOL_ARCHIVE_INVALID",
                "Optional tool archive contains an unsafe path entry.",
            ));
        }
    }

    Ok(path)
}

pub fn extract_optional_tool_package(
    package: &OptionalToolPackage,
    archive_path: &Path,
    workspace_dir: &Path,
) -> Result<PathBuf, AppError> {
    let destination_root = downloaded_optional_tool_type_dir(workspace_dir, &package.tool_type)
        .join(package.version.replace('/', "-"));

    if destination_root.exists() {
        fs::remove_dir_all(&destination_root)?;
    }
    fs::create_dir_all(&destination_root)?;

    match package.archive_kind {
        OptionalToolArchiveKind::Zip => {
            let archive = File::open(archive_path)?;
            let mut zip = ZipArchive::new(archive).map_err(|error| {
                AppError::with_details(
                    "OPTIONAL_TOOL_EXTRACT_FAILED",
                    "Optional tool archive is not a valid zip file.",
                    error.to_string(),
                )
            })?;

            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).map_err(|error| {
                    AppError::with_details(
                        "OPTIONAL_TOOL_EXTRACT_FAILED",
                        "DevNest could not read a file from the optional tool archive.",
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
        OptionalToolArchiveKind::Binary => {
            let output_name = package
                .entry_binary
                .rsplit(['\\', '/'])
                .next()
                .ok_or_else(|| {
                    AppError::new_validation(
                        "OPTIONAL_TOOL_ENTRY_NOT_FOUND",
                        "Optional tool manifest entry binary is invalid.",
                    )
                })?;
            fs::copy(archive_path, destination_root.join(output_name))?;
        }
    }

    Ok(destination_root)
}

pub fn resolve_package_entry_path(
    package: &OptionalToolPackage,
    extracted_root: &Path,
) -> Result<PathBuf, AppError> {
    let relative_path = sanitize_extract_path(&package.entry_binary)?;
    let entry_path = extracted_root.join(relative_path);

    if !entry_path.exists() || !entry_path.is_file() {
        return Err(AppError::new_validation(
            "OPTIONAL_TOOL_ENTRY_NOT_FOUND",
            "Optional tool package was extracted, but the configured entry binary was not found.",
        ));
    }

    Ok(entry_path)
}

#[cfg(test)]
mod tests {
    use super::list_optional_tool_packages;
    use crate::models::optional_tool::OptionalToolType;
    use std::path::Path;

    #[test]
    fn bundled_manifest_includes_phase_29_redis_and_restic_packages() {
        let packages = list_optional_tool_packages(Path::new("missing-resources"))
            .expect("bundled optional tool manifest should parse");

        let redis = packages
            .iter()
            .find(|package| package.tool_type == OptionalToolType::Redis)
            .expect("redis package should be present");
        assert_eq!(redis.version, "8.6.2");
        assert_eq!(
            redis.entry_binary,
            "Redis-8.6.2-Windows-x64-msys2/redis-server.exe"
        );
        assert!(redis.checksum_sha256.is_some());

        let restic = packages
            .iter()
            .find(|package| package.tool_type == OptionalToolType::Restic)
            .expect("restic package should be present");
        assert_eq!(restic.version, "0.18.1");
        assert!(restic.entry_binary.ends_with(".exe"));
        assert!(restic.checksum_sha256.is_some());
    }
}
