use crate::commands::disk::DiskInfo;
use crate::{AppError, Result};
use std::process::Command;

/// List all disks on Windows using PowerShell
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "Get-Disk | Select-Object Number, FriendlyName, Size, BusType | ConvertTo-Json",
        ])
        .output()
        .map_err(|e| AppError::CommandFailed(format!("PowerShell: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::CommandFailed(format!("Get-Disk failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    let disks: Vec<DiskInfo> = if trimmed.starts_with('[') {
        let raw: Vec<serde_json::Value> = serde_json::from_str(trimmed)
            .map_err(|e| AppError::JsonError(e.to_string()))?;
        raw.iter().map(|d| parse_disk(d)).collect()
    } else {
        let raw: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| AppError::JsonError(e.to_string()))?;
        vec![parse_disk(&raw)]
    };

    Ok(disks)
}

fn parse_disk(v: &serde_json::Value) -> DiskInfo {
    let number = v["Number"].as_u64().unwrap_or(0);
    let name = v["FriendlyName"].as_str().unwrap_or("Unknown").to_string();
    let size = v["Size"].as_u64().unwrap_or(0);
    let bus_type = v["BusType"].as_u64().unwrap_or(0);
    let removable = bus_type == 7 || bus_type == 17;
    let device = format!("PhysicalDrive{}", number);

    DiskInfo {
        id: format!("disk{}", number),
        name,
        size,
        removable,
        device,
    }
}

/// Get disk info on Windows
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let disks = list_disks().await?;
    disks
        .into_iter()
        .find(|d| d.id == disk_id)
        .ok_or_else(|| AppError::DeviceNotFound(disk_id.to_string()))
}

/// Start USB monitoring on Windows
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Ok("monitor-windows".to_string())
}

/// Stop USB monitoring on Windows
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Ok(())
}
