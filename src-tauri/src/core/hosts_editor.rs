use crate::error::AppError;
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct HostsOperationResult {
    pub domain: String,
    pub target_ip: String,
}

#[derive(Debug, Clone)]
struct ParsedHostsEntry {
    ip: String,
    domains: Vec<String>,
    comment: Option<String>,
}

fn validate_domain(domain: &str) -> Result<String, AppError> {
    let normalized = domain.trim().to_ascii_lowercase();

    if normalized.len() < 3 || normalized.len() > 120 {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_DOMAIN",
            "Project domain must be between 3 and 120 characters.",
        ));
    }

    let labels = normalized.split('.').collect::<Vec<_>>();
    let valid = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        });

    if !valid {
        return Err(AppError::new_validation(
            "INVALID_PROJECT_DOMAIN",
            "Project domain must look like a valid local domain.",
        ));
    }

    Ok(normalized)
}

fn validate_target_ip(target_ip: &str) -> Result<String, AppError> {
    let normalized = target_ip.trim();
    IpAddr::from_str(normalized).map_err(|_| {
        AppError::new_validation(
            "INVALID_TARGET_IP",
            "Hosts target IP must be a valid IPv4 or IPv6 address.",
        )
    })?;
    Ok(normalized.to_string())
}

fn parse_hosts_entry(line: &str) -> Option<ParsedHostsEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let mut parts = line.splitn(2, '#');
    let body = parts.next()?.trim();
    if body.is_empty() {
        return None;
    }

    let tokens = body.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }

    let comment = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("# {value}"));

    Some(ParsedHostsEntry {
        ip: tokens[0].to_string(),
        domains: tokens[1..]
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect(),
        comment,
    })
}

fn dedupe_domains(domains: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();

    for domain in domains {
        if seen.insert(domain.to_ascii_lowercase()) {
            ordered.push(domain.to_ascii_lowercase());
        }
    }

    ordered
}

fn render_hosts_entry(entry: ParsedHostsEntry) -> String {
    let mut line = format!("{}\t{}", entry.ip, entry.domains.join(" "));
    if let Some(comment) = entry.comment {
        line.push(' ');
        line.push_str(&comment);
    }
    line
}

fn map_hosts_error(error: std::io::Error, action: &str) -> AppError {
    let is_permission_denied = error.kind() == std::io::ErrorKind::PermissionDenied
        || error.raw_os_error() == Some(5)
        || error
            .to_string()
            .to_ascii_lowercase()
            .contains("access is denied");

    if is_permission_denied {
        return AppError::with_details(
            "HOSTS_PERMISSION_DENIED",
            format!("Administrator permission is required to {action} the Windows hosts file."),
            error.to_string(),
        );
    }

    AppError::with_details(
        "HOSTS_UPDATE_FAILED",
        format!("Could not {action} the Windows hosts file."),
        error.to_string(),
    )
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

pub fn apply_hosts_entry(
    hosts_path: &Path,
    domain: &str,
    target_ip: &str,
) -> Result<HostsOperationResult, AppError> {
    let domain = validate_domain(domain)?;
    let target_ip = validate_target_ip(target_ip)?;
    let content =
        fs::read_to_string(hosts_path).map_err(|error| map_hosts_error(error, "update"))?;
    let mut lines = Vec::new();
    let mut kept_target_entry = false;

    for raw_line in content.lines() {
        match parse_hosts_entry(raw_line) {
            Some(entry) if entry.domains.iter().any(|item| item == &domain) => {
                let domains = dedupe_domains(&entry.domains);
                if entry.ip == target_ip && !kept_target_entry {
                    let normalized = ParsedHostsEntry { domains, ..entry };
                    lines.push(render_hosts_entry(normalized));
                    kept_target_entry = true;
                } else {
                    let remaining = domains
                        .into_iter()
                        .filter(|item| item != &domain)
                        .collect::<Vec<_>>();
                    if !remaining.is_empty() {
                        lines.push(render_hosts_entry(ParsedHostsEntry {
                            domains: remaining,
                            ..entry
                        }));
                    }
                }
            }
            _ => lines.push(raw_line.to_string()),
        }
    }

    if !kept_target_entry {
        if !lines.is_empty() && !lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("{target_ip}\t{domain} # devnest"));
    }

    fs::write(hosts_path, join_lines(&lines)).map_err(|error| map_hosts_error(error, "update"))?;

    Ok(HostsOperationResult { domain, target_ip })
}

pub fn remove_hosts_entry(
    hosts_path: &Path,
    domain: &str,
) -> Result<HostsOperationResult, AppError> {
    let domain = validate_domain(domain)?;
    let content =
        fs::read_to_string(hosts_path).map_err(|error| map_hosts_error(error, "update"))?;
    let mut lines = Vec::new();

    for raw_line in content.lines() {
        match parse_hosts_entry(raw_line) {
            Some(entry) if entry.domains.iter().any(|item| item == &domain) => {
                let remaining = dedupe_domains(&entry.domains)
                    .into_iter()
                    .filter(|item| item != &domain)
                    .collect::<Vec<_>>();
                if !remaining.is_empty() {
                    lines.push(render_hosts_entry(ParsedHostsEntry {
                        domains: remaining,
                        ..entry
                    }));
                }
            }
            _ => lines.push(raw_line.to_string()),
        }
    }

    fs::write(hosts_path, join_lines(&lines)).map_err(|error| map_hosts_error(error, "update"))?;

    Ok(HostsOperationResult {
        domain,
        target_ip: "127.0.0.1".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_hosts_entry, remove_hosts_entry};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn make_hosts_file(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("devnest-hosts-{}", Uuid::new_v4()));
        fs::write(&path, content).expect("hosts fixture should be written");
        path
    }

    #[test]
    fn applies_new_domain_without_duplicates() {
        let hosts_path = make_hosts_file("127.0.0.1\tlocalhost\n");

        apply_hosts_entry(&hosts_path, "shop.test", "127.0.0.1").expect("apply should succeed");
        apply_hosts_entry(&hosts_path, "shop.test", "127.0.0.1")
            .expect("duplicate apply should remain safe");

        let content = fs::read_to_string(&hosts_path).expect("hosts file should be readable");
        assert_eq!(content.matches("shop.test").count(), 1);

        fs::remove_file(hosts_path).ok();
    }

    #[test]
    fn removes_existing_domain_safely() {
        let hosts_path = make_hosts_file(
            "127.0.0.1\tlocalhost shop.test # devnest\n192.168.1.50\tlegacy.test\n",
        );

        remove_hosts_entry(&hosts_path, "shop.test").expect("remove should succeed");

        let content = fs::read_to_string(&hosts_path).expect("hosts file should be readable");
        assert!(!content.contains("shop.test"));
        assert!(content.contains("legacy.test"));

        fs::remove_file(hosts_path).ok();
    }

    #[test]
    fn rejects_invalid_target_ip() {
        let hosts_path = make_hosts_file("127.0.0.1\tlocalhost\n");
        let error = apply_hosts_entry(&hosts_path, "shop.test", "not-an-ip")
            .expect_err("invalid ip should be rejected");

        assert_eq!(error.code, "INVALID_TARGET_IP");
        fs::remove_file(hosts_path).ok();
    }

    #[test]
    fn returns_permission_denied_for_unwritable_hosts_target() {
        let hosts_path = make_hosts_file("127.0.0.1\tlocalhost\n");
        let mut permissions = fs::metadata(&hosts_path)
            .expect("hosts fixture metadata should load")
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&hosts_path, permissions)
            .expect("hosts fixture should become readonly");

        let error = apply_hosts_entry(&hosts_path, "shop.test", "127.0.0.1")
            .expect_err("readonly hosts target should fail");

        assert_eq!(error.code, "HOSTS_PERMISSION_DENIED");

        let mut cleanup_permissions = fs::metadata(&hosts_path)
            .expect("readonly fixture metadata should load")
            .permissions();
        cleanup_permissions.set_readonly(false);
        fs::set_permissions(&hosts_path, cleanup_permissions).ok();
        fs::remove_file(hosts_path).ok();
    }
}
