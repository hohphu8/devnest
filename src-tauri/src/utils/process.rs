use crate::error::AppError;
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
    let output = run_powershell(&format!(
        "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ 'running' }}"
    ))?;

    Ok(output == "running")
}

pub fn process_name(pid: u32) -> Result<Option<String>, AppError> {
    let output = run_powershell(&format!(
        "$proc = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($proc) {{ $proc.ProcessName }}"
    ))?;

    if output.is_empty() {
        Ok(None)
    } else {
        Ok(Some(output))
    }
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
