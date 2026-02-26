use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

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
    #[serde(default)]
    pub has_windows: bool,
    #[serde(default)]
    pub windows_name: String,
}

fn parse_u64(value: &serde_json::Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        .unwrap_or(0)
}

fn parse_bool(value: &serde_json::Value) -> bool {
    value
        .as_bool()
        .or_else(|| value.as_str().map(|s| s.eq_ignore_ascii_case("true")))
        .unwrap_or(false)
}

fn normalize_drive_letter(value: &serde_json::Value) -> String {
    let raw = value.as_str().unwrap_or("").trim().trim_end_matches(':');
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

fn parse_partition_info(v: &serde_json::Value) -> Option<PartitionInfo> {
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
        has_windows: parse_bool(&v["HasWindows"]),
        windows_name: v["WindowsName"].as_str().unwrap_or("").trim().to_string(),
    })
}

#[cfg(target_os = "macos")]
fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

#[cfg(target_os = "macos")]
fn plist_to_json(value: &[u8]) -> Result<Value> {
    let mut child = Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AppError::io)?;

    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| AppError::SystemError("Failed to open plutil stdin".to_string()))?;
    use std::io::Write as _;
    stdin.write_all(value).map_err(AppError::io)?;

    let output = child.wait_with_output().map_err(AppError::io)?;
    if !output.status.success() {
        return Err(AppError::DiskError(format!(
            "Failed to parse plist output: {}",
            to_text(&output.stderr)
        )));
    }

    serde_json::from_slice(&output.stdout).map_err(AppError::from)
}

#[cfg(target_os = "macos")]
fn diskutil_plist_json(args: &[&str]) -> Result<Value> {
    let output = Command::new("diskutil")
        .args(args)
        .output()
        .map_err(AppError::io)?;

    if !output.status.success() {
        let err = to_text(&output.stderr);
        let out = to_text(&output.stdout);
        let detail = if err.is_empty() { out } else { err };
        return Err(AppError::DiskError(format!("diskutil failed: {}", detail)));
    }

    plist_to_json(&output.stdout)
}

#[cfg(target_os = "macos")]
fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(target_os = "macos")]
fn json_u64(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .or_else(|| value.get(key).and_then(Value::as_f64).map(|n| n.max(0.0) as u64))
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn parse_disk_number_from_any(value: &str) -> u32 {
    let trimmed = value.trim();
    let Some(pos) = trimmed.find("disk") else {
        return 0;
    };
    let tail = &trimmed[pos + 4..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn parse_media_type_macos(info: &Value) -> String {
    if info
        .get("SolidState")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return "SSD".to_string();
    }

    let bus = json_str(info, "BusProtocol");
    if bus.eq_ignore_ascii_case("USB") {
        return "USB".to_string();
    }

    let media_name = json_str(info, "MediaName").to_ascii_uppercase();
    if media_name.contains("SSD") || media_name.contains("NVME") {
        return "SSD".to_string();
    }
    if media_name.contains("HDD") {
        return "HDD".to_string();
    }

    "Unknown".to_string()
}

/// List mounted partitions with drive letters (Windows only)
#[tauri::command]
pub async fn list_partitions() -> Result<Vec<PartitionInfo>> {
    #[cfg(target_os = "windows")]
    {
        use crate::utils::command::CommandExecutor;

        let ps = r#"
$ErrorActionPreference = 'SilentlyContinue'

$diskMediaMap = @{}
$diskProtocolMap = @{}
$partitionDiskMap = @{}

try {
    $disks = Get-Disk | Select-Object Number, MediaType, BusType
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
} catch {}

try {
    Get-Partition | Where-Object DriveLetter -ne $null | ForEach-Object {
        $partitionDiskMap[[string]$_.DriveLetter] = [int]$_.DiskNumber
    }
} catch {}

$currentSystemDrive = [string]$env:SystemDrive
if ($currentSystemDrive.EndsWith(':')) { $currentSystemDrive = $currentSystemDrive.TrimEnd(':') }

function Get-WindowsNameByDrive([string]$driveLetter) {
    $result = [PSCustomObject]@{
        HasWindows  = $false
        WindowsName = ''
    }

    if (-not $driveLetter) { return $result }

    $root = "$driveLetter`:\"
    $winDir = Join-Path $root 'Windows'
    if (-not (Test-Path $winDir)) { return $result }

    $result.HasWindows = $true
    $productName = ''

    if ($driveLetter -eq $currentSystemDrive) {
        try {
            $pn = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -Name ProductName -ErrorAction SilentlyContinue).ProductName
            if ($pn) { $productName = [string]$pn }
        } catch {}
    } else {
        $hivePath = Join-Path $root 'Windows\System32\Config\SOFTWARE'
        if (Test-Path $hivePath) {
            $tempKey = "HKLM\WTGA_OFFLINE_$driveLetter"
            $loaded = $false
            try {
                & reg.exe load $tempKey $hivePath 1>$null 2>$null
                if ($LASTEXITCODE -eq 0) {
                    $loaded = $true
                    $pn = (Get-ItemProperty "Registry::$tempKey\Microsoft\Windows NT\CurrentVersion" -Name ProductName -ErrorAction SilentlyContinue).ProductName
                    if ($pn) { $productName = [string]$pn }
                }
            } catch {}
            finally {
                if ($loaded) {
                    try { & reg.exe unload $tempKey 1>$null 2>$null } catch {}
                }
            }
        }
    }

    if (-not $productName) { $productName = 'Windows' }
    $result.WindowsName = $productName
    return $result
}

$result = @()
$drives = Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Name -match '^[A-Za-z]$' } | Sort-Object Name
foreach ($d in $drives) {
    $dl = [string]$d.Name
    $di = $null
    try { $di = New-Object System.IO.DriveInfo($dl) } catch {}
    if (-not $di) { continue }
    if (-not $di.IsReady) { continue }
    if ($di.DriveType -ne [System.IO.DriveType]::Fixed -and $di.DriveType -ne [System.IO.DriveType]::Removable) { continue }

    $diskNo = 0
    if ($partitionDiskMap.ContainsKey($dl)) {
        $diskNo = [int]$partitionDiskMap[$dl]
    }
    $proto = if ($diskProtocolMap.ContainsKey([string]$diskNo)) { [string]$diskProtocolMap[[string]$diskNo] } else { 'Unknown' }
    $media = if ($diskMediaMap.ContainsKey([string]$diskNo)) { [string]$diskMediaMap[[string]$diskNo] } else { 'Unknown' }
    $osInfo = Get-WindowsNameByDrive $dl

    $result += [PSCustomObject]@{
        DriveLetter = $dl
        Label       = [string]$di.VolumeLabel
        FileSystem  = [string]$di.DriveFormat
        Size        = [uint64]$di.TotalSize
        Free        = [uint64]$di.AvailableFreeSpace
        DiskNumber  = [uint32]$diskNo
        Protocol    = $proto
        MediaType   = $media
        HasWindows  = if($osInfo){$osInfo.HasWindows}else{$false}
        WindowsName = if($osInfo){$osInfo.WindowsName}else{''}
    }
}

$__json = $result | ConvertTo-Json -Depth 4 -Compress
if ([string]::IsNullOrWhiteSpace($__json)) { $__json = '[]' }
Write-Output "__WTGA_JSON__$__json"
"#;

        let output =
            CommandExecutor::execute_allow_fail("powershell.exe", &["-NoProfile", "-Command", ps])?;

        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }

        let json_str = if let Some(marker_pos) = trimmed.rfind("__WTGA_JSON__") {
            let start = marker_pos + "__WTGA_JSON__".len();
            trimmed[start..].trim()
        } else {
            let json_start = trimmed.find('[').or_else(|| trimmed.find('{')).unwrap_or(0);
            &trimmed[json_start..]
        };

        if json_str.is_empty() || json_str.eq_ignore_ascii_case("null") {
            return Ok(vec![]);
        }

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

    #[cfg(target_os = "macos")]
    {
        let list_json = diskutil_plist_json(&["list", "-plist"])?;
        let disks = list_json
            .get("AllDisksAndPartitions")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::DiskError("Invalid diskutil list output".to_string()))?;

        let mut result = Vec::new();

        for disk in disks {
            let disk_id = json_str(disk, "DeviceIdentifier");
            if !disk_id.starts_with("disk") {
                continue;
            }
            let disk_node = format!("/dev/{}", disk_id);
            let disk_info = match diskutil_plist_json(&["info", "-plist", &disk_node]) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let protocol = {
                let bus = json_str(&disk_info, "BusProtocol");
                if bus.is_empty() {
                    "Unknown".to_string()
                } else {
                    bus
                }
            };
            let media_type = parse_media_type_macos(&disk_info);
            let disk_number = parse_disk_number_from_any(&disk_id);

            let parts = match disk.get("Partitions").and_then(Value::as_array) {
                Some(v) => v,
                None => continue,
            };

            for part in parts {
                let partition_id = json_str(part, "DeviceIdentifier");
                if partition_id.is_empty() {
                    continue;
                }
                let mount_point = json_str(part, "MountPoint");
                if mount_point.is_empty() {
                    continue;
                }

                let windows_dir = Path::new(&mount_point).join("Windows");
                if !windows_dir.exists() {
                    continue;
                }

                let partition_node = format!("/dev/{}", partition_id);
                let part_info = match diskutil_plist_json(&["info", "-plist", &partition_node]) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let fs_type = json_str(&part_info, "FilesystemType");
                let size = {
                    let total = json_u64(&part_info, "TotalSize");
                    if total > 0 {
                        total
                    } else {
                        json_u64(&part_info, "Size")
                    }
                };
                let free = json_u64(&part_info, "VolumeFreeSpace");
                let label = {
                    let volume_name = json_str(&part_info, "VolumeName");
                    if volume_name.is_empty() {
                        json_str(&part_info, "MediaName")
                    } else {
                        volume_name
                    }
                };

                result.push(PartitionInfo {
                    drive_letter: partition_id,
                    label,
                    filesystem: fs_type,
                    size,
                    free,
                    disk_number,
                    protocol: protocol.clone(),
                    media_type: media_type.clone(),
                    has_windows: true,
                    windows_name: "Windows".to_string(),
                });
            }
        }

        result.sort_by(|a, b| a.drive_letter.cmp(&b.drive_letter));
        Ok(result)
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err(AppError::Unsupported(
            "Partition listing only implemented on Windows and macOS".into(),
        ))
    }
}
