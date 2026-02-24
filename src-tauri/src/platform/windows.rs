use crate::commands::disk::DiskInfo;
use crate::utils::command::CommandExecutor;
use crate::{AppError, Result};
use tracing::info;

/// PowerShell script to get disk info with volume letters.
/// For each disk, we query its partitions → volumes → drive letters.
const PS_LIST_DISKS: &str = r#"
$physical = Get-PhysicalDisk | Select-Object FriendlyName, MediaType, SpindleSpeed
$disks = Get-Disk | Select-Object Number, FriendlyName, Size, BusType, MediaType, IsBoot, IsSystem
$result = @()
foreach ($d in $disks) {
    # Try to resolve media type (SSD/HDD)
    $pd = $physical | Where-Object { $_.FriendlyName -eq $d.FriendlyName } | Select-Object -First 1
    $media = $d.MediaType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($pd) { $media = $pd.MediaType }
    }
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($pd -and $pd.SpindleSpeed -eq 0) { $media = 'SSD' }
    }
    if (-not $media -or $media -eq '') { $media = 'HDD' }

    $volumes = Get-Partition -DiskNumber $d.Number -ErrorAction SilentlyContinue |
        Get-Volume -ErrorAction SilentlyContinue |
        Where-Object { $_.DriveLetter -ne $null -and $_.DriveLetter -ne '' } |
        Select-Object -ExpandProperty DriveLetter
    $vol = if ($volumes -is [array]) { $volumes[0] } else { $volumes }
    $busName = switch ($d.BusType) {
        7  { "USB" }
        17 { "USB" }
        3  { "ATA" }
        11 { "SATA" }
        default { "Other" }
    }
    $result += [PSCustomObject]@{
        Number       = $d.Number
        FriendlyName = $d.FriendlyName
        Size         = $d.Size
        BusType      = $d.BusType
        MediaType    = $media
        IsBoot       = $d.IsBoot
        IsSystem     = $d.IsSystem
        BusTypeName  = $busName
        VolumeLetter = if ($vol) { [string]$vol } else { "" }
    }
}
$result | ConvertTo-Json
"#;

/// List all disks on Windows using PowerShell
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    let output = CommandExecutor::execute_allow_fail(
        "powershell.exe",
        &["-NoProfile", "-Command", PS_LIST_DISKS],
    )?;

    let trimmed = output.trim();

    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // Find the JSON portion (skip any non-JSON output before it)
    let json_start = trimmed.find('[').or_else(|| trimmed.find('{'));
    let json_str = match json_start {
        Some(pos) => &trimmed[pos..],
        None => {
            info!("No JSON found in PowerShell output: {}", &trimmed[..trimmed.len().min(200)]);
            return Ok(vec![]);
        }
    };

    let disks: Vec<DiskInfo> = if json_str.starts_with('[') {
        let raw: Vec<serde_json::Value> = serde_json::from_str(json_str)
            .map_err(|e| AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)])))?;
        raw.iter().map(|d| parse_disk(d)).collect()
    } else {
        let raw: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)])))?;
        vec![parse_disk(&raw)]
    };

    info!("Found {} disks", disks.len());
    Ok(disks)
}

fn parse_disk(v: &serde_json::Value) -> DiskInfo {
    let number = v["Number"].as_u64().unwrap_or(0);
    let name = v["FriendlyName"].as_str().unwrap_or("Unknown").to_string();
    let size = v["Size"].as_u64().unwrap_or(0);
    let bus_type = v["BusType"].as_u64().unwrap_or(0);
    let media_type_raw = v["MediaType"].as_str().unwrap_or("").to_string();
    let is_system = v["IsSystem"].as_bool().unwrap_or(false) || v["IsBoot"].as_bool().unwrap_or(false);
    // BusType 7 = USB, 17 = USB (SD reader)
    let removable = bus_type == 7 || bus_type == 17;
    let device = format!("PhysicalDrive{}", number);

    let bus_name = v["BusTypeName"].as_str().unwrap_or("Other").to_string();
    let drive_type = if removable {
        "Removable".to_string()
    } else {
        format!("Fixed ({})", bus_name)
    };

    let volume = v["VolumeLetter"].as_str().unwrap_or("").to_string();

    DiskInfo {
        id: format!("disk{}", number),
        name,
        size,
        removable,
        device,
        drive_type,
        media_type: normalize_media_type(&media_type_raw),
        index: number.to_string(),
        volume,
        is_system,
    }
}

fn normalize_media_type(raw: &str) -> String {
    let up = raw.to_uppercase();
    if up.contains("SSD") {
        "SSD".to_string()
    } else if up.contains("HDD") || up.contains("ROTATIONAL") {
        "HDD".to_string()
    } else {
        "HDD".to_string()
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
