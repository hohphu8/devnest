use crate::error::AppError;
use crate::utils::process::{configure_background_command, process_names};
use std::collections::{BTreeSet, HashMap};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortCheckResult {
    pub port: u16,
    pub available: bool,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone)]
struct PortCheckCache {
    ports: Vec<u16>,
    checked_at: Instant,
    results: Vec<PortCheckResult>,
}

static PORT_CHECK_CACHE: OnceLock<Mutex<Option<PortCheckCache>>> = OnceLock::new();

fn listening_port_for_address(local_address: &str) -> Option<u16> {
    local_address
        .rsplit(':')
        .next()
        .and_then(|value| value.trim_end_matches(']').parse::<u16>().ok())
}

fn parse_netstat_pids(output: &str, requested_ports: &BTreeSet<u16>) -> HashMap<u16, u32> {
    let mut pids = HashMap::new();

    for line in output.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 5 {
            continue;
        }

        let local_address = columns[1];
        let state = columns[3];
        let Some(port) = listening_port_for_address(local_address) else {
            continue;
        };

        if state != "LISTENING" || !requested_ports.contains(&port) {
            continue;
        }

        if let Ok(pid) = columns[4].parse::<u32>() {
            pids.insert(port, pid);
        }
    }

    pids
}

fn run_netstat_output() -> Result<String, AppError> {
    let mut command = Command::new("netstat");
    command.args(["-ano", "-p", "tcp"]);
    configure_background_command(&mut command);
    let output = command.output().map_err(|error| {
        AppError::with_details(
            "PORT_CHECK_FAILED",
            "Could not inspect local TCP ports.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "PORT_CHECK_FAILED",
            "Could not inspect local TCP ports.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn check_ports(ports: &[u16]) -> Result<Vec<PortCheckResult>, AppError> {
    let requested_ports = ports.iter().copied().collect::<BTreeSet<_>>();
    if requested_ports.is_empty() {
        return Ok(Vec::new());
    }

    let stdout = run_netstat_output()?;
    let pid_by_port = parse_netstat_pids(&stdout, &requested_ports);
    let unique_pids = pid_by_port.values().copied().collect::<BTreeSet<_>>();
    let process_name_by_pid = process_names(&unique_pids.into_iter().collect::<Vec<_>>())?;

    Ok(requested_ports
        .into_iter()
        .map(|port| {
            let pid = pid_by_port.get(&port).copied();
            PortCheckResult {
                port,
                available: pid.is_none(),
                pid,
                process_name: pid
                    .and_then(|value| process_name_by_pid.get(&value).cloned().flatten()),
            }
        })
        .collect())
}

pub fn check_ports_cached(ports: &[u16], ttl: Duration) -> Result<Vec<PortCheckResult>, AppError> {
    let requested_ports = ports.iter().copied().collect::<BTreeSet<_>>();
    if requested_ports.is_empty() {
        return Ok(Vec::new());
    }

    let cache_key = requested_ports.iter().copied().collect::<Vec<_>>();
    let cache = PORT_CHECK_CACHE.get_or_init(|| Mutex::new(None));
    if let Some(cached) = cache
        .lock()
        .map_err(|_| {
            AppError::new_validation(
                "PORT_CHECK_CACHE_FAILED",
                "Could not read cached port checks.",
            )
        })?
        .as_ref()
        .filter(|cached| cached.ports == cache_key && cached.checked_at.elapsed() <= ttl)
        .cloned()
    {
        return Ok(cached.results);
    }

    let results = check_ports(&cache_key)?;
    *cache.lock().map_err(|_| {
        AppError::new_validation(
            "PORT_CHECK_CACHE_FAILED",
            "Could not update cached port checks.",
        )
    })? = Some(PortCheckCache {
        ports: cache_key,
        checked_at: Instant::now(),
        results: results.clone(),
    });

    Ok(results)
}

pub fn check_port(port: u16) -> Result<PortCheckResult, AppError> {
    check_ports(&[port])?.into_iter().next().ok_or_else(|| {
        AppError::new_validation(
            "PORT_CHECK_FAILED",
            "Could not inspect the requested local TCP port.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{check_port, check_ports_cached};
    use std::net::TcpListener;
    use std::time::Duration;

    #[test]
    fn reports_port_conflict_for_bound_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();

        let result = check_port(port).expect("port check should succeed");

        assert!(!result.available);
        assert!(result.pid.is_some());
    }

    #[test]
    fn cached_port_check_handles_empty_requests() {
        let result = check_ports_cached(&[], Duration::from_secs(1)).expect("empty cache read");

        assert!(result.is_empty());
    }
}
