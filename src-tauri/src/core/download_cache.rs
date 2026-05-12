use crate::error::AppError;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCacheBucket {
    pub id: String,
    pub display_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCacheSummary {
    pub total_size_bytes: u64,
    pub file_count: u64,
    pub buckets: Vec<DownloadCacheBucket>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearDownloadCacheResult {
    pub deleted_bytes: u64,
    pub deleted_files: u64,
    pub summary: DownloadCacheSummary,
}

fn cache_dirs(workspace_dir: &Path) -> Vec<(&'static str, &'static str, PathBuf)> {
    vec![
        (
            "runtime",
            "Runtime downloads",
            workspace_dir.join("runtime-downloads"),
        ),
        (
            "optionalTool",
            "Optional tool downloads",
            workspace_dir.join("tool-downloads"),
        ),
        (
            "phpExtension",
            "PHP extension downloads",
            workspace_dir.join("php-extension-downloads"),
        ),
    ]
}

fn scan_dir(path: &Path) -> Result<(u64, u64), AppError> {
    if !path.exists() {
        return Ok((0, 0));
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok((metadata.len(), 1));
    }

    if !metadata.is_dir() {
        return Ok((0, 0));
    }

    let mut size_bytes = 0;
    let mut file_count = 0;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let entry_metadata = fs::symlink_metadata(&entry_path)?;

        if entry_metadata.is_file() {
            size_bytes += entry_metadata.len();
            file_count += 1;
            continue;
        }

        if entry_metadata.is_dir() {
            let (child_size_bytes, child_file_count) = scan_dir(&entry_path)?;
            size_bytes += child_size_bytes;
            file_count += child_file_count;
        }
    }

    Ok((size_bytes, file_count))
}

pub fn summarize_download_cache(workspace_dir: &Path) -> Result<DownloadCacheSummary, AppError> {
    let buckets = cache_dirs(workspace_dir)
        .into_iter()
        .map(|(id, display_name, path)| {
            let (size_bytes, file_count) = scan_dir(&path)?;
            Ok(DownloadCacheBucket {
                id: id.to_string(),
                display_name: display_name.to_string(),
                path: path.to_string_lossy().to_string(),
                size_bytes,
                file_count,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let total_size_bytes = buckets.iter().map(|bucket| bucket.size_bytes).sum();
    let file_count = buckets.iter().map(|bucket| bucket.file_count).sum();

    Ok(DownloadCacheSummary {
        total_size_bytes,
        file_count,
        buckets,
    })
}

pub fn clear_download_cache(workspace_dir: &Path) -> Result<ClearDownloadCacheResult, AppError> {
    let before = summarize_download_cache(workspace_dir)?;

    for (_, _, path) in cache_dirs(workspace_dir) {
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(&path)?;
    }

    let summary = summarize_download_cache(workspace_dir)?;

    Ok(ClearDownloadCacheResult {
        deleted_bytes: before.total_size_bytes,
        deleted_files: before.file_count,
        summary,
    })
}

pub fn remove_archive_best_effort(archive_path: &Path) {
    if archive_path.is_file() {
        let _ = fs::remove_file(archive_path);
    }
}
