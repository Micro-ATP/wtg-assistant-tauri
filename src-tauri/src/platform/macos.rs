use crate::commands::DiskInfo;
use crate::{AppError, Result};

/// List all disks on macOS
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    Err(AppError::Unsupported(
        "macOS disk enumeration is not implemented yet".to_string(),
    ))
}

/// Get disk info on macOS
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let _ = disk_id;
    Err(AppError::Unsupported(
        "macOS disk info retrieval is not implemented yet".to_string(),
    ))
}

/// Start USB monitoring on macOS
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Err(AppError::Unsupported(
        "macOS USB monitoring is not implemented yet".to_string(),
    ))
}

/// Stop USB monitoring on macOS
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Err(AppError::Unsupported(
        "macOS USB monitoring is not implemented yet".to_string(),
    ))
}
