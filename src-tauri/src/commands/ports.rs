use crate::core::ports::{self, PortCheckResult};
use crate::error::AppError;

#[tauri::command]
pub fn check_port(port: u16) -> Result<PortCheckResult, AppError> {
    ports::check_port(port)
}
