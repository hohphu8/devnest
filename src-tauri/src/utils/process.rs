use crate::error::AppError;
use std::collections::{BTreeSet, HashMap};
use std::process::{Command, Output};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn split_command_args(value: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escape = false;

    for character in value.chars() {
        if escape {
            current.push(character);
            escape = false;
            continue;
        }

        match character {
            '\\' if in_quotes => escape = true,
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(character),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

fn should_quote_command_arg(value: &str) -> bool {
    value.is_empty()
        || value
            .chars()
            .any(|character| character.is_whitespace() || character == '"' || character == '\\')
}

fn quote_command_arg(value: &str) -> String {
    if !should_quote_command_arg(value) {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn join_command_args(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        return command.to_string();
    }

    let serialized_args = args
        .iter()
        .map(|value| quote_command_arg(value))
        .collect::<Vec<_>>()
        .join(" ");

    format!("{command} {serialized_args}")
}

pub fn configure_background_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

pub fn run_command(command: &str, args: &[String]) -> Result<Output, AppError> {
    let mut process = Command::new(command);
    process.args(args);
    configure_background_command(&mut process);
    process.output().map_err(|error| {
        AppError::with_details(
            "PROCESS_COMMAND_FAILED",
            "Could not execute a native process command.",
            error.to_string(),
        )
    })
}

pub fn run_powershell(script: &str) -> Result<String, AppError> {
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-Command", script]);
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "POWERSHELL_EXECUTION_FAILED",
            "Could not execute the PowerShell helper command.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "POWERSHELL_EXECUTION_FAILED",
            "A PowerShell helper command failed.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_process_running(pid: u32) -> Result<bool, AppError> {
    Ok(running_process_ids(&[pid])?.contains(&pid))
}

fn parse_csv_row(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(character) = chars.next() {
        match character {
            '"' if in_quotes && matches!(chars.peek(), Some('"')) => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                values.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }

    values.push(current.trim().to_string());
    values
}

fn normalize_tasklist_process_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("INFO:") {
        return None;
    }

    Some(
        trimmed
            .strip_suffix(".exe")
            .or_else(|| trimmed.strip_suffix(".EXE"))
            .unwrap_or(trimmed)
            .to_string(),
    )
}

fn parse_tasklist_process_snapshot(
    output: &str,
    requested_pids: &BTreeSet<u32>,
) -> HashMap<u32, Option<String>> {
    let mut processes = requested_pids
        .iter()
        .copied()
        .map(|pid| (pid, None))
        .collect::<HashMap<_, _>>();

    for line in output.lines() {
        let columns = parse_csv_row(line);
        if columns.len() < 2 {
            continue;
        }

        let Ok(pid) = columns[1].trim().parse::<u32>() else {
            continue;
        };
        if !requested_pids.contains(&pid) {
            continue;
        }

        processes.insert(pid, normalize_tasklist_process_name(&columns[0]));
    }

    processes
}

fn run_tasklist_output() -> Result<String, AppError> {
    let args = vec!["/FO".to_string(), "CSV".to_string(), "/NH".to_string()];
    let output = run_command("tasklist", &args)?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PROCESS_LOOKUP_FAILED",
            "Could not inspect running processes.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn process_names(pids: &[u32]) -> Result<HashMap<u32, Option<String>>, AppError> {
    let requested_pids = pids.iter().copied().collect::<BTreeSet<_>>();
    if requested_pids.is_empty() {
        return Ok(HashMap::new());
    }

    let output = match run_tasklist_output() {
        Ok(output) => output,
        Err(_) => return Ok(parse_tasklist_process_snapshot("", &requested_pids)),
    };

    Ok(parse_tasklist_process_snapshot(&output, &requested_pids))
}

pub fn running_process_ids(pids: &[u32]) -> Result<BTreeSet<u32>, AppError> {
    Ok(process_names(pids)?
        .into_iter()
        .filter_map(|(pid, process_name)| process_name.map(|_| pid))
        .collect())
}

pub fn find_process_ids_by_commandline(
    process_name: &str,
    commandline_fragment: &str,
) -> Result<Vec<u32>, AppError> {
    let process_name = escape_powershell_single_quoted(process_name);
    let commandline_fragment = escape_powershell_single_quoted(commandline_fragment);
    let output = run_powershell(&format!(
        "$fragment = '{commandline_fragment}'; \
         Get-CimInstance Win32_Process -Filter \"Name = '{process_name}'\" -ErrorAction SilentlyContinue | \
         Where-Object {{ $_.CommandLine -and $_.CommandLine -like \"*$fragment*\" }} | \
         ForEach-Object {{ $_.ProcessId }}"
    ))?;

    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.parse::<u32>().ok())
        .collect())
}

pub fn kill_process_tree(pid: u32) -> Result<(), AppError> {
    let args = vec![
        "/PID".to_string(),
        pid.to_string(),
        "/T".to_string(),
        "/F".to_string(),
    ];
    let output = run_command("taskkill", &args)?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "SERVICE_STOP_FAILED",
            "Could not stop the service process.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_tasklist_process_snapshot;
    use std::collections::BTreeSet;

    #[test]
    fn parses_tasklist_process_snapshot_rows() {
        let requested = BTreeSet::from([101, 202, 303]);
        let parsed = parse_tasklist_process_snapshot(
            "\"nginx.exe\",\"101\",\"Console\",\"1\",\"10,000 K\"\r\n\"php-cgi.exe\",\"202\",\"Console\",\"1\",\"9,000 K\"\n",
            &requested,
        );

        assert_eq!(
            parsed.get(&101).and_then(Clone::clone).as_deref(),
            Some("nginx")
        );
        assert_eq!(
            parsed.get(&202).and_then(Clone::clone).as_deref(),
            Some("php-cgi")
        );
        assert_eq!(parsed.get(&303).and_then(Clone::clone), None);
    }
}
