use crate::error::AppError;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: String,
    pub text: String,
    pub severity: LogSeverity,
    pub line_number: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPayload {
    pub name: String,
    pub total_lines: usize,
    pub truncated: bool,
    pub lines: Vec<LogLine>,
    pub content: String,
}

pub type ServiceLogPayload = LogPayload;
pub type ProjectWorkerLogPayload = LogPayload;
pub type ProjectScheduledTaskRunLogPayload = LogPayload;

fn infer_severity(text: &str) -> LogSeverity {
    let normalized = text.to_ascii_lowercase();
    if normalized.contains("error") || normalized.contains("fatal") {
        return LogSeverity::Error;
    }

    if normalized.contains("warn") {
        return LogSeverity::Warning;
    }

    LogSeverity::Info
}

pub fn clear(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::with_details(
                "LOG_CLEAR_FAILED",
                "Could not create the log directory.",
                error.to_string(),
            )
        })?;
    }

    fs::write(path, "").map_err(|error| {
        AppError::with_details(
            "LOG_CLEAR_FAILED",
            "Could not clear the log file.",
            error.to_string(),
        )
    })
}

pub fn read_tail(path: &Path, lines: usize) -> Result<String, AppError> {
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not read the log file.",
            error.to_string(),
        )
    })?;

    let normalized = content.replace("\r\n", "\n");
    let selected = normalized
        .lines()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    Ok(selected)
}

pub fn read_tail_payload(
    path: &Path,
    source_name: &str,
    lines: usize,
) -> Result<LogPayload, AppError> {
    let line_count = lines.clamp(1, 1000);
    if !path.exists() {
        return Ok(LogPayload {
            name: source_name.to_string(),
            total_lines: 0,
            truncated: false,
            lines: Vec::new(),
            content: String::new(),
        });
    }

    let content = fs::read_to_string(path).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not read the log file.",
            error.to_string(),
        )
    })?;

    let normalized = content.replace("\r\n", "\n");
    let all_lines = normalized.lines().collect::<Vec<_>>();
    let total_lines = all_lines.len();
    let start_index = total_lines.saturating_sub(line_count);
    let selected_lines = all_lines
        .iter()
        .enumerate()
        .skip(start_index)
        .map(|(index, text)| LogLine {
            id: format!("{source_name}:{index}"),
            text: (*text).to_string(),
            severity: infer_severity(text),
            line_number: Some(index + 1),
        })
        .collect::<Vec<_>>();

    Ok(LogPayload {
        name: source_name.to_string(),
        total_lines,
        truncated: total_lines > line_count,
        content: selected_lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        lines: selected_lines,
    })
}

#[cfg(test)]
mod tests {
    use super::{clear, read_tail};
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn reads_last_requested_lines() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));
        fs::write(&path, "one\ntwo\nthree\nfour\n").expect("log file should write");

        let tail = read_tail(&path, 2).expect("tail read should succeed");

        assert_eq!(tail, "three\nfour");
        fs::remove_file(path).ok();
    }

    #[test]
    fn clears_log_content() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));
        fs::write(&path, "one\ntwo\nthree\n").expect("log file should write");

        clear(&path).expect("log file should clear");

        let tail = read_tail(&path, 10).expect("tail read should succeed after clear");
        assert!(tail.is_empty());
        fs::remove_file(path).ok();
    }
}
