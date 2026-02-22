#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod linux;

use crate::Result;
use crate::commands::{DiskInfo, UsbDevice, UsbEvent};

pub trait DiskOperations {
    async fn list_disks() -> Result<Vec<DiskInfo>>;
    async fn get_disk_info(disk_id: &str) -> Result<DiskInfo>;
}

pub trait UsbMonitoring {
    async fn start_monitoring(app_handle: tauri::AppHandle) -> Result<String>;
    async fn stop_monitoring(monitor_id: &str) -> Result<()>;
}
