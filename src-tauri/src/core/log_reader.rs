use crate::error::AppError;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const TAIL_CHUNK_SIZE: u64 = 16 * 1024;

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

struct TailSnapshot {
    total_lines: usize,
    selected_lines: Vec<String>,
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn normalize_log_content(content: &str) -> String {
    content.replace("\r\n", "\n")
}

fn count_total_lines(
    file: &mut fs::File,
    file_size: u64,
    last_byte: u8,
) -> Result<usize, AppError> {
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not seek within the log file.",
            error.to_string(),
        )
    })?;

    let mut remaining = file_size;
    let mut total_newlines = 0usize;
    while remaining > 0 {
        let chunk_size = remaining.min(TAIL_CHUNK_SIZE);
        let mut chunk = vec![0; chunk_size as usize];
        file.read_exact(&mut chunk).map_err(|error| {
            AppError::with_details(
                "LOG_READ_FAILED",
                "Could not read the log file.",
                error.to_string(),
            )
        })?;
        total_newlines += count_newlines(&chunk);
        remaining -= chunk_size;
    }

    Ok(total_newlines + if last_byte == b'\n' { 0 } else { 1 })
}

fn read_tail_snapshot(path: &Path, requested_lines: usize) -> Result<TailSnapshot, AppError> {
    if requested_lines == 0 || !path.exists() {
        return Ok(TailSnapshot {
            total_lines: 0,
            selected_lines: Vec::new(),
        });
    }

    let mut file = fs::File::open(path).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not read the log file.",
            error.to_string(),
        )
    })?;
    let file_size = file
        .metadata()
        .map_err(|error| {
            AppError::with_details(
                "LOG_READ_FAILED",
                "Could not inspect the log file.",
                error.to_string(),
            )
        })?
        .len();
    if file_size == 0 {
        return Ok(TailSnapshot {
            total_lines: 0,
            selected_lines: Vec::new(),
        });
    }

    let mut position = file_size;
    let mut tail_bytes = Vec::<u8>::new();
    let mut last_byte = [0u8; 1];
    file.seek(SeekFrom::End(-1)).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not seek within the log file.",
            error.to_string(),
        )
    })?;
    file.read_exact(&mut last_byte).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "Could not read the log file.",
            error.to_string(),
        )
    })?;
    let total_lines = count_total_lines(&mut file, file_size, last_byte[0])?;

    while position > 0 {
        let chunk_size = position.min(TAIL_CHUNK_SIZE);
        position -= chunk_size;
        file.seek(SeekFrom::Start(position)).map_err(|error| {
            AppError::with_details(
                "LOG_READ_FAILED",
                "Could not seek within the log file.",
                error.to_string(),
            )
        })?;
        let mut chunk = vec![0; chunk_size as usize];
        file.read_exact(&mut chunk).map_err(|error| {
            AppError::with_details(
                "LOG_READ_FAILED",
                "Could not read the log file.",
                error.to_string(),
            )
        })?;
        chunk.extend(tail_bytes);
        tail_bytes = chunk;

        if count_newlines(&tail_bytes) > requested_lines {
            break;
        }
    }

    let selected_bytes = if position > 0 {
        tail_bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| tail_bytes[index + 1..].to_vec())
            .unwrap_or(tail_bytes)
    } else {
        tail_bytes
    };
    let selected_text = String::from_utf8(selected_bytes).map_err(|error| {
        AppError::with_details(
            "LOG_READ_FAILED",
            "The log file is not valid UTF-8.",
            error.to_string(),
        )
    })?;
    let normalized = normalize_log_content(&selected_text);
    let mut selected_lines = normalized
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if selected_lines.len() > requested_lines {
        selected_lines =
            selected_lines.split_off(selected_lines.len().saturating_sub(requested_lines));
    }

    Ok(TailSnapshot {
        total_lines,
        selected_lines,
    })
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
    Ok(read_tail_snapshot(path, lines)?.selected_lines.join("\n"))
}

pub fn read_tail_payload(
    path: &Path,
    source_name: &str,
    lines: usize,
) -> Result<LogPayload, AppError> {
    let line_count = lines.clamp(1, 1000);
    let snapshot = read_tail_snapshot(path, line_count)?;
    let total_lines = snapshot.total_lines;
    let start_index = total_lines.saturating_sub(snapshot.selected_lines.len());
    let selected_lines = snapshot
        .selected_lines
        .iter()
        .enumerate()
        .map(|(index, text)| LogLine {
            id: format!("{source_name}:{}", start_index + index),
            text: text.to_string(),
            severity: infer_severity(text),
            line_number: Some(start_index + index + 1),
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
    use super::{clear, read_tail, read_tail_payload};
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
    fn reads_large_file_without_losing_line_numbers() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));
        let content = (1..=3000)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, content).expect("large log file should write");

        let payload = read_tail_payload(&path, "large", 3).expect("tail payload should read");

        assert_eq!(payload.total_lines, 3000);
        assert!(payload.truncated);
        assert_eq!(payload.content, "line 2998\nline 2999\nline 3000");
        assert_eq!(payload.lines[0].line_number, Some(2998));
        fs::remove_file(path).ok();
    }

    #[test]
    fn normalizes_crlf_tail_content() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));
        fs::write(&path, "one\r\ntwo\r\nthree\r\n").expect("crlf log file should write");

        let tail = read_tail(&path, 2).expect("tail read should succeed");

        assert_eq!(tail, "two\nthree");
        fs::remove_file(path).ok();
    }

    #[test]
    fn returns_empty_payload_for_empty_file() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));
        fs::write(&path, "").expect("empty log file should write");

        let payload = read_tail_payload(&path, "empty", 10).expect("empty payload should read");

        assert_eq!(payload.total_lines, 0);
        assert!(!payload.truncated);
        assert!(payload.lines.is_empty());
        assert!(payload.content.is_empty());
        fs::remove_file(path).ok();
    }

    #[test]
    fn returns_empty_payload_for_missing_file() {
        let path = std::env::temp_dir().join(format!("devnest-log-reader-{}.log", Uuid::new_v4()));

        let payload = read_tail_payload(&path, "missing", 10).expect("missing payload should read");

        assert_eq!(payload.total_lines, 0);
        assert!(!payload.truncated);
        assert!(payload.lines.is_empty());
        assert!(payload.content.is_empty());
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
