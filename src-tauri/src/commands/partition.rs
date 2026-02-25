use serde::{Deserialize, Serialize};
use crate::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartitionInfo {
    pub drive_letter: String,
    pub label: String,
    pub filesystem: String,
    pub size: u64,
    pub free: u64,
    pub disk_number: u32,
    pub protocol: String,
    pub media_type: String,
}

fn parse_u64(value: &serde_json::Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        .unwrap_or(0)
}

fn normalize_drive_letter(value: &serde_json::Value) -> String {
    let raw = value
        .as_str()
        .unwrap_or("")
        .trim()
        .trim_end_matches(':');
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase().to_string(),
        _ => String::new(),
    }
}

fn normalize_media_type(value: &serde_json::Value) -> String {
    if let Some(raw) = value.as_str() {
        let up = raw.trim().to_uppercase();
        if up.contains("SSD") || up.contains("NVME") || up == "4" {
            return "SSD".to_string();
        }
        if up.contains("HDD") || up.contains("ROTATIONAL") || up == "3" {
            return "HDD".to_string();
        }
        if up.contains("UNKNOWN") || up.contains("UNSPECIFIED") || up.is_empty() {
            return "Unknown".to_string();
        }
        return raw.trim().to_string();
    }

    match value.as_u64().unwrap_or(0) {
        4 => "SSD".to_string(),
        3 => "HDD".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn normalize_protocol(value: &serde_json::Value) -> String {
    let raw = value
        .as_str()
        .map(|s| s.trim().to_uppercase())
        .unwrap_or_default();

    match raw.as_str() {
        "17" | "NVME" => "NVMe".to_string(),
        "11" | "SATA" => "SATA".to_string(),
        "7" | "USB" => "USB".to_string(),
        "10" | "SAS" => "SAS".to_string(),
        "8" | "RAID" => "RAID".to_string(),
        "3" | "ATA" => "ATA".to_string(),
        "1" | "SCSI" => "SCSI".to_string(),
        "2" | "ATAPI" => "ATAPI".to_string(),
        "12" | "SD" => "SD".to_string(),
        "13" | "MMC" => "MMC".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn parse_partition_info(
    v: &serde_json::Value,
) -> Option<PartitionInfo> {
    let drive_letter = normalize_drive_letter(&v["DriveLetter"]);
    if drive_letter.is_empty() {
        return None;
    }

    let disk_number = parse_u64(&v["DiskNumber"]) as u32;
    let protocol = normalize_protocol(&v["Protocol"]);
    let media_type = normalize_media_type(&v["MediaType"]);

    Some(PartitionInfo {
        drive_letter,
        label: v["Label"].as_str().unwrap_or("").to_string(),
        filesystem: v["FileSystem"].as_str().unwrap_or("").to_string(),
        size: parse_u64(&v["Size"]),
        free: parse_u64(&v["Free"]),
        disk_number,
        protocol,
        media_type,
    })
}

/// List mounted partitions with drive letters (Windows only)
#[tauri::command]
pub async fn list_partitions() -> Result<Vec<PartitionInfo>> {
    #[cfg(target_os = "windows")]
    {
        use crate::utils::command::CommandExecutor;

        let ps = r#"
$disks = Get-Disk | Select-Object Number, MediaType, BusType
$diskMediaMap = @{}
$diskProtocolMap = @{}
foreach ($d in $disks) {
    $bus = [string]$d.BusType
    $protocol = switch -Regex ($bus) {
        '^17$|NVMe' { 'NVMe' }
        '^11$|SATA' { 'SATA' }
        '^7$|USB' { 'USB' }
        '^10$|SAS' { 'SAS' }
        '^8$|RAID' { 'RAID' }
        '^3$|ATA' { 'ATA' }
        '^1$|SCSI' { 'SCSI' }
        '^2$|ATAPI' { 'ATAPI' }
        '^12$|SD' { 'SD' }
        '^13$|MMC' { 'MMC' }
        default { 'Unknown' }
    }
    $diskProtocolMap[[string]$d.Number] = $protocol

    $media = [string]$d.MediaType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($bus -match '(^17$|NVMe)') {
            $media = 'SSD'
        } else {
            $media = 'Unknown'
        }
    }
    $diskMediaMap[[string]$d.Number] = $media
}

$volumeMap = @{}
Get-Volume | Where-Object DriveLetter -ne $null | ForEach-Object {
    $volumeMap[[string]$_.DriveLetter] = $_
}

$result = @()
Get-Partition | Where-Object DriveLetter -ne $null | ForEach-Object {
    $p = $_
    $dl = [string]$p.DriveLetter
    $v = $volumeMap[$dl]
    $m = $diskMediaMap[[string]$p.DiskNumber]
    $proto = $diskProtocolMap[[string]$p.DiskNumber]
    $result += [PSCustomObject]@{
        DriveLetter = $dl
        Label       = if($v){$v.FileSystemLabel}else{""}
        FileSystem  = if($v){$v.FileSystem}else{""}
        Size        = if($v){$v.Size}else{0}
        Free        = if($v){$v.SizeRemaining}else{0}
        DiskNumber  = $p.DiskNumber
        Protocol    = if($proto){$proto}else{"Unknown"}
        MediaType   = if($m){$m}else{""}
    }
}

$result | ConvertTo-Json -Depth 4
"#;

        let output = CommandExecutor::execute_allow_fail(
            "powershell.exe",
            &["-NoProfile", "-Command", ps],
        )?;

        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }
        let json_start = trimmed.find('[').or_else(|| trimmed.find('{')).unwrap_or(0);
        let json_str = &trimmed[json_start..];
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| crate::AppError::JsonError(e.to_string()))?;

        let mut res = Vec::new();
        match parsed {
            serde_json::Value::Array(arr) => {
                for v in arr {
                    if let Some(info) = parse_partition_info(&v) {
                        res.push(info);
                    }
                }
            }
            serde_json::Value::Object(_) => {
                let v = parsed;
                if let Some(info) = parse_partition_info(&v) {
                    res.push(info);
                }
            }
            _ => {}
        }
        Ok(res)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(crate::AppError::Unsupported(
            "Partition listing only implemented on Windows".into(),
        ))
    }
}
