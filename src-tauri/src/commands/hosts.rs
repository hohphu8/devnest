use crate::core::hosts_editor;
use crate::error::AppError;
use crate::utils::windows::{
    apply_hosts_file_with_elevation, hosts_file_path, remove_hosts_file_with_elevation,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyHostsEntryResult {
    pub success: bool,
    pub domain: String,
    pub target_ip: String,
}

#[derive(serde::Serialize)]
pub struct RemoveHostsEntryResult {
    pub success: bool,
}

#[tauri::command]
pub fn apply_hosts_entry(
    domain: String,
    target_ip: Option<String>,
) -> Result<ApplyHostsEntryResult, AppError> {
    let hosts_path = hosts_file_path();
    let target_ip = target_ip.unwrap_or_else(|| "127.0.0.1".to_string());
    let result = match hosts_editor::apply_hosts_entry(&hosts_path, &domain, &target_ip) {
        Ok(result) => result,
        Err(error) if error.code == "HOSTS_PERMISSION_DENIED" => {
            apply_hosts_file_with_elevation(&hosts_path, &domain, &target_ip)?;
            hosts_editor::HostsOperationResult {
                domain: domain.to_ascii_lowercase(),
                target_ip: target_ip.clone(),
            }
        }
        Err(error) => return Err(error),
    };

    Ok(ApplyHostsEntryResult {
        success: true,
        domain: result.domain,
        target_ip: result.target_ip,
    })
}

#[tauri::command]
pub fn remove_hosts_entry(domain: String) -> Result<RemoveHostsEntryResult, AppError> {
    let hosts_path = hosts_file_path();
    match hosts_editor::remove_hosts_entry(&hosts_path, &domain) {
        Ok(_) => {}
        Err(error) if error.code == "HOSTS_PERMISSION_DENIED" => {
            remove_hosts_file_with_elevation(&hosts_path, &domain)?;
        }
        Err(error) => return Err(error),
    }

    Ok(RemoveHostsEntryResult { success: true })
}
