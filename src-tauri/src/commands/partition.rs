use crate::Result;
use serde::{Deserialize, Serialize};

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

    #[cfg(not(target_os = "windows"))]
    {
        Err(crate::AppError::Unsupported(
            "Partition listing only implemented on Windows".into(),
        ))
    }
}
