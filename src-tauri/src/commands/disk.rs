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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SmartAttribute {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub current: Option<u32>,
    pub worst: Option<u32>,
    pub threshold: Option<u32>,
    pub raw: Option<u64>,
    #[serde(default)]
    pub raw_hex: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DiskDiagnostics {
    #[serde(default)]
    pub id: String,
    pub disk_number: u32,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub friendly_name: String,
    #[serde(default)]
    pub serial_number: String,
    #[serde(default)]
    pub firmware_version: String,
    #[serde(default)]
    pub interface_type: String,
    #[serde(default)]
    pub transport_type: String,
    #[serde(default)]
    pub is_usb: bool,
    #[serde(default)]
    pub bus_type: String,
    #[serde(default)]
    pub unique_id: String,
    #[serde(default)]
    pub media_type: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub health_status: String,
    #[serde(default)]
    pub smart_supported: bool,
    #[serde(default)]
    pub smart_enabled: bool,
    #[serde(default)]
    pub smart_data_source: String,
    #[serde(default)]
    pub ata_smart_available: bool,
    #[serde(default)]
    pub reliability_available: bool,
    pub temperature_c: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub power_cycle_count: Option<u64>,
    pub percentage_used: Option<f64>,
    pub read_errors_total: Option<u64>,
    pub write_errors_total: Option<u64>,
    pub host_reads_total: Option<u64>,
    pub host_writes_total: Option<u64>,
    #[serde(default)]
    pub smart_attributes: Vec<SmartAttribute>,
    #[serde(default)]
    pub reliability: serde_json::Value,
    #[serde(default)]
    pub notes: Vec<String>,
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

/// List disk diagnostics including SMART/reliability details
#[tauri::command]
pub async fn list_disk_diagnostics() -> Result<Vec<DiskDiagnostics>> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::list_disk_diagnostics().await
    }
    #[cfg(target_os = "macos")]
    {
        Err(crate::AppError::Unsupported(
            "Disk diagnostics is not implemented on macOS yet".to_string(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        Err(crate::AppError::Unsupported(
            "Disk diagnostics is not implemented on Linux yet".to_string(),
        ))
    }
}
