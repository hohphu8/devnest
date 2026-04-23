use crate::error::AppError;
use crate::models::project::Project;
use crate::models::project_env_var::{
    ProjectDiskEnvVar, ProjectEnvComparisonItem, ProjectEnvComparisonStatus, ProjectEnvInspection,
    ProjectEnvVar,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

pub fn inspect_project_env(
    project: &Project,
    tracked_vars: Vec<ProjectEnvVar>,
) -> Result<ProjectEnvInspection, AppError> {
    let env_file_path = PathBuf::from(&project.path).join(".env");
    let env_file_exists = env_file_path.is_file();
    let (disk_vars, disk_read_error) = if env_file_exists {
        match fs::read_to_string(&env_file_path) {
            Ok(content) => (parse_disk_env_vars(&content), None),
            Err(error) => (
                Vec::new(),
                Some(format!(
                    "DevNest could not read the project .env file: {error}"
                )),
            ),
        }
    } else {
        (Vec::new(), None)
    };

    let comparison = build_env_comparison(&tracked_vars, &disk_vars);

    Ok(ProjectEnvInspection {
        project_id: project.id.clone(),
        env_file_path: env_file_path.to_string_lossy().to_string(),
        env_file_exists,
        disk_read_error,
        tracked_count: tracked_vars.len(),
        disk_count: disk_vars.len(),
        disk_vars,
        comparison,
    })
}

fn parse_disk_env_vars(content: &str) -> Vec<ProjectDiskEnvVar> {
    let mut entries = Vec::new();
    let mut index_by_key = HashMap::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let candidate = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let Some((raw_key, raw_value)) = candidate.split_once('=') else {
            continue;
        };

        let key = raw_key.trim();
        if !is_valid_env_key(key) {
            continue;
        }

        let entry = ProjectDiskEnvVar {
            key: key.to_string(),
            value: normalize_env_value(raw_value),
            source_line: index + 1,
        };

        if let Some(existing_index) = index_by_key.get(key).copied() {
            entries[existing_index] = entry;
            continue;
        }

        index_by_key.insert(key.to_string(), entries.len());
        entries.push(entry);
    }

    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn is_valid_env_key(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn normalize_env_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        if let Some(unquoted) = trimmed
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
        {
            return unquoted
                .replace("\\\"", "\"")
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t");
        }

        if let Some(unquoted) = trimmed
            .strip_prefix('\'')
            .and_then(|inner| inner.strip_suffix('\''))
        {
            return unquoted.to_string();
        }
    }

    strip_unquoted_inline_comment(trimmed).trim().to_string()
}

fn strip_unquoted_inline_comment(value: &str) -> &str {
    let mut previous_was_whitespace = true;

    for (index, character) in value.char_indices() {
        if character == '#' && previous_was_whitespace {
            return value[..index].trim_end();
        }

        previous_was_whitespace = character.is_whitespace();
    }

    value
}

fn build_env_comparison(
    tracked_vars: &[ProjectEnvVar],
    disk_vars: &[ProjectDiskEnvVar],
) -> Vec<ProjectEnvComparisonItem> {
    let tracked_map = tracked_vars
        .iter()
        .map(|item| (item.env_key.clone(), item.env_value.clone()))
        .collect::<BTreeMap<_, _>>();
    let disk_map = disk_vars
        .iter()
        .map(|item| (item.key.clone(), item.value.clone()))
        .collect::<BTreeMap<_, _>>();

    let keys = tracked_map
        .keys()
        .chain(disk_map.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    keys.into_iter()
        .map(|key| {
            let tracked_value = tracked_map.get(&key).cloned();
            let disk_value = disk_map.get(&key).cloned();
            let status = match (&tracked_value, &disk_value) {
                (Some(left), Some(right)) if left == right => ProjectEnvComparisonStatus::Match,
                (Some(_), Some(_)) => ProjectEnvComparisonStatus::ValueMismatch,
                (Some(_), None) => ProjectEnvComparisonStatus::OnlyTracked,
                (None, Some(_)) => ProjectEnvComparisonStatus::OnlyDisk,
                (None, None) => ProjectEnvComparisonStatus::OnlyTracked,
            };

            ProjectEnvComparisonItem {
                key,
                tracked_value,
                disk_value,
                status,
            }
        })
        .collect()
}
