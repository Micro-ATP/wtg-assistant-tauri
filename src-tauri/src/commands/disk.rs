use serde::{Deserialize, Serialize};
use crate::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
    pub device: String,
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
