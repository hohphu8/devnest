use crate::error::AppError;
use crate::models::runtime::{
    PhpExtensionPackage, PhpExtensionPackageKind, PhpExtensionPackageManifest,
    PhpExtensionThreadSafety,
};
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
    env::var("DEVNEST_PHP_EXTENSION_MANIFEST_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn default_manifest_candidates(resources_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![resources_dir.join("php-extensions").join("packages.json")];

    let repo_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("php-extensions")
        .join("packages.json");
    if !candidates.contains(&repo_manifest) {
        candidates.push(repo_manifest);
    }

    if let Ok(current_dir) = env::current_dir() {
        let cwd_manifest = current_dir
            .join("src-tauri")
            .join("resources")
            .join("php-extensions")
            .join("packages.json");
        if !candidates.contains(&cwd_manifest) {
            candidates.push(cwd_manifest);
        }
    }

    candidates
}

pub fn php_extension_manifest_path(resources_dir: &Path) -> Option<PathBuf> {
    if let Some(override_path) = manifest_override_path() {
        return Some(override_path);
    }

    default_manifest_candidates(resources_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn list_php_extension_packages(
    resources_dir: &Path,
    php_family: &str,
    thread_safety: Option<&PhpExtensionThreadSafety>,
) -> Result<Vec<PhpExtensionPackage>, AppError> {
    let manifest_path = php_extension_manifest_path(resources_dir).ok_or_else(|| {
        AppError::new_validation(
            "PHP_EXTENSION_MANIFEST_NOT_FOUND",
            "PHP extension package manifest was not found. Dev mode expects src-tauri/resources/php-extensions/packages.json; packaged builds expect resources/php-extensions/packages.json; DEVNEST_PHP_EXTENSION_MANIFEST_PATH can override both.",
        )
    })?;

    let content = fs::read_to_string(&manifest_path).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_MANIFEST_READ_FAILED",
            "DevNest could not read the PHP extension package manifest.",
            error.to_string(),
        )
    })?;

    let manifest: PhpExtensionPackageManifest =
        serde_json::from_str(&content).map_err(|error| {
            AppError::with_details(
                "PHP_EXTENSION_MANIFEST_INVALID",
                "PHP extension package manifest is invalid JSON.",
                error.to_string(),
            )
        })?;

    Ok(manifest
        .packages
        .into_iter()
        .filter(|package| {
            package.platform.eq_ignore_ascii_case(current_platform())
                && package.arch.eq_ignore_ascii_case(current_arch())
                && package.php_family == php_family
                && match thread_safety {
                    Some(required) => package.thread_safety.as_ref() == Some(required),
                    None => package
                        .thread_safety
                        .as_ref()
                        .map(|value| matches!(value, PhpExtensionThreadSafety::Nts))
                        .unwrap_or(true),
                }
        })
        .collect())
}

fn downloads_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("php-extension-downloads")
}

fn archive_file_name(package: &PhpExtensionPackage) -> String {
    match package.package_kind {
        PhpExtensionPackageKind::Zip => {
            format!("{}-{}.zip", package.extension_name, package.version)
        }
        PhpExtensionPackageKind::Binary => package
            .dll_file
            .rsplit(['\\', '/'])
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("php_{}.dll", package.extension_name)),
    }
}

fn file_url_to_path(file_url: &str) -> PathBuf {
    let trimmed = file_url.trim_start_matches("file://");
    if trimmed.starts_with('/') && trimmed.chars().nth(2) == Some(':') {
        PathBuf::from(&trimmed[1..])
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn download_php_extension_archive(
    package: &PhpExtensionPackage,
    workspace_dir: &Path,
) -> Result<PathBuf, AppError> {
    fs::create_dir_all(downloads_dir(workspace_dir))?;
    let archive_path = downloads_dir(workspace_dir).join(archive_file_name(package));

    if package.download_url.trim().is_empty() {
        return Err(AppError::new_validation(
            "PHP_EXTENSION_PACKAGE_URL_MISSING",
            "This PHP extension package does not define a download URL yet.",
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
            "PHP_EXTENSION_DOWNLOAD_FAILED",
            "DevNest could not prepare the PHP extension downloader.",
            error.to_string(),
        )
    })?;

    let mut response = client.get(&package.download_url).send().map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_DOWNLOAD_FAILED",
            "PHP extension package download failed.",
            error.to_string(),
        )
    })?;

    if !response.status().is_success() {
        return Err(AppError::with_details(
            "PHP_EXTENSION_DOWNLOAD_FAILED",
            "PHP extension package download returned a non-success HTTP status.",
            response.status().to_string(),
        ));
    }

    let mut archive = File::create(&archive_path)?;
    response.copy_to(&mut archive).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_DOWNLOAD_FAILED",
            "PHP extension package download could not be written to disk.",
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
        .map(str::trim)
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
        "PHP_EXTENSION_PACKAGE_CHECKSUM_MISMATCH",
        "Downloaded PHP extension package checksum did not match the manifest.",
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
                "PHP_EXTENSION_ARCHIVE_INVALID",
                "PHP extension archive contains an unsafe path entry.",
            ));
        }
    }

    Ok(path)
}

pub fn install_php_extension_package(
    package: &PhpExtensionPackage,
    archive_path: &Path,
    ext_dir: &Path,
) -> Result<Vec<String>, AppError> {
    fs::create_dir_all(ext_dir).map_err(|error| {
        AppError::with_details(
            "PHP_EXTENSION_INSTALL_FAILED",
            "DevNest could not create the PHP ext directory.",
            error.to_string(),
        )
    })?;

    match package.package_kind {
        PhpExtensionPackageKind::Zip => {
            let archive = File::open(archive_path).map_err(|error| {
                AppError::with_details(
                    "PHP_EXTENSION_INSTALL_FAILED",
                    "DevNest could not open the downloaded PHP extension archive.",
                    error.to_string(),
                )
            })?;
            let mut zip = ZipArchive::new(archive).map_err(|error| {
                AppError::with_details(
                    "PHP_EXTENSION_INSTALL_FAILED",
                    "The downloaded PHP extension archive is not a valid zip file.",
                    error.to_string(),
                )
            })?;

            let mut installed_extensions = Vec::new();
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).map_err(|error| {
                    AppError::with_details(
                        "PHP_EXTENSION_INSTALL_FAILED",
                        "DevNest could not read a file from the PHP extension archive.",
                        error.to_string(),
                    )
                })?;

                if entry.is_dir() {
                    continue;
                }

                let relative_path = sanitize_extract_path(entry.name())?;
                let Some(file_name) = relative_path.file_name() else {
                    continue;
                };
                let output_path = ext_dir.join(file_name);

                let mut output = File::create(&output_path).map_err(|error| {
                    AppError::with_details(
                        "PHP_EXTENSION_INSTALL_FAILED",
                        "DevNest could not write a PHP extension file into the runtime.",
                        error.to_string(),
                    )
                })?;
                std::io::copy(&mut entry, &mut output).map_err(|error| {
                    AppError::with_details(
                        "PHP_EXTENSION_INSTALL_FAILED",
                        "DevNest could not extract a PHP extension file into the runtime.",
                        error.to_string(),
                    )
                })?;
                output.flush().map_err(|error| {
                    AppError::with_details(
                        "PHP_EXTENSION_INSTALL_FAILED",
                        "DevNest could not finish writing a PHP extension file into the runtime.",
                        error.to_string(),
                    )
                })?;

                let file_name = file_name.to_string_lossy().to_string();
                if let Some(extension_name) = file_name
                    .trim()
                    .to_ascii_lowercase()
                    .strip_prefix("php_")
                    .and_then(|value| value.strip_suffix(".dll"))
                    .map(str::to_string)
                {
                    installed_extensions.push(extension_name);
                }
            }

            installed_extensions.sort();
            installed_extensions.dedup();
            if installed_extensions.is_empty() {
                return Err(AppError::new_validation(
                    "PHP_EXTENSION_INSTALL_FAILED",
                    "The downloaded package did not contain any `php_<name>.dll` extension files.",
                ));
            }

            Ok(installed_extensions)
        }
        PhpExtensionPackageKind::Binary => {
            let target_path = ext_dir.join(&package.dll_file);
            fs::copy(archive_path, &target_path).map_err(|error| {
                AppError::with_details(
                    "PHP_EXTENSION_INSTALL_FAILED",
                    "DevNest could not copy the downloaded PHP extension DLL into the runtime.",
                    error.to_string(),
                )
            })?;

            Ok(vec![package.extension_name.clone()])
        }
    }
}
