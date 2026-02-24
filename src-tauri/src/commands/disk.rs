use serde::{Deserialize, Serialize};
use crate::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
    pub device: String,
    /// Drive type string, e.g. "Removable", "Fixed"
    #[serde(default)]
    pub drive_type: String,
    /// Media type, e.g. "SSD", "HDD"
    #[serde(default)]
    pub media_type: String,
    /// Disk number as string, e.g. "0", "1"
    #[serde(default)]
    pub index: String,
    /// Volume/drive letter, e.g. "E"
    #[serde(default)]
    pub volume: String,
    /// Whether the disk is the system disk
    #[serde(default)]
    pub is_system: bool,
}

/// List all available disks
#[tauri::command]
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::list_disks().await
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::list_disks().await
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::list_disks().await
    }
}

/// Get detailed information about a specific disk
#[tauri::command]
pub async fn get_disk_info(disk_id: String) -> Result<DiskInfo> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::get_disk_info(&disk_id).await
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::get_disk_info(&disk_id).await
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::get_disk_info(&disk_id).await
    }
}
