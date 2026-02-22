use crate::commands::{DiskInfo, UsbEvent};
use crate::{AppError, Result};

/// List all disks on Linux
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    // TODO: Implement Linux disk enumeration using /sys/block or udev
    // This is a placeholder implementation
    Ok(vec![])
}

/// Get disk info on Linux
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    // TODO: Implement Linux disk info retrieval
    Err(AppError::DeviceNotFound(disk_id.to_string()))
}

/// Start USB monitoring on Linux
pub async fn start_usb_monitoring(app_handle: tauri::AppHandle) -> Result<String> {
    // TODO: Implement Linux USB monitoring using udev
    Ok("monitor-linux".to_string())
}

/// Stop USB monitoring on Linux
pub async fn stop_usb_monitoring(monitor_id: &str) -> Result<()> {
    // TODO: Implement stopping USB monitoring
    Ok(())
}
