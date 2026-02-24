use crate::commands::DiskInfo;
use crate::{AppError, Result};

/// List all disks on Linux
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    Err(AppError::Unsupported(
        "Linux disk enumeration is not implemented yet".to_string(),
    ))
}

/// Get disk info on Linux
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let _ = disk_id;
    Err(AppError::Unsupported(
        "Linux disk info retrieval is not implemented yet".to_string(),
    ))
}

/// Start USB monitoring on Linux
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Err(AppError::Unsupported(
        "Linux USB monitoring is not implemented yet".to_string(),
    ))
}

/// Stop USB monitoring on Linux
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Err(AppError::Unsupported(
        "Linux USB monitoring is not implemented yet".to_string(),
    ))
}
