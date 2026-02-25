use serde::{Deserialize, Serialize};
use crate::Result;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartitionInfo {
    pub drive_letter: String,
    pub label: String,
    pub filesystem: String,
    pub size: u64,
    pub free: u64,
    pub disk_number: u32,
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

fn parse_partition_info(
    v: &serde_json::Value,
    disk_media_map: &HashMap<u32, String>,
) -> Option<PartitionInfo> {
    let drive_letter = normalize_drive_letter(&v["DriveLetter"]);
    if drive_letter.is_empty() {
        return None;
    }

    let disk_number = parse_u64(&v["DiskNumber"]) as u32;
    let media_type = disk_media_map
        .get(&disk_number)
        .cloned()
        .unwrap_or_else(|| normalize_media_type(&v["MediaType"]));

    Some(PartitionInfo {
        drive_letter,
        label: v["Label"].as_str().unwrap_or("").to_string(),
        filesystem: v["FileSystem"].as_str().unwrap_or("").to_string(),
        size: parse_u64(&v["Size"]),
        free: parse_u64(&v["Free"]),
        disk_number,
        media_type,
    })
}

/// List mounted partitions with drive letters (Windows only)
#[tauri::command]
pub async fn list_partitions() -> Result<Vec<PartitionInfo>> {
    #[cfg(target_os = "windows")]
    {
        use crate::utils::command::CommandExecutor;

        // Reuse the same disk detection source as Configure page to keep media type consistent.
        let disks = crate::platform::windows::list_disks().await?;
        let mut disk_media_map: HashMap<u32, String> = HashMap::new();
        for d in disks {
            if let Ok(n) = d.index.parse::<u32>() {
                disk_media_map.insert(n, normalize_media_type(&serde_json::Value::String(d.media_type)));
            }
        }

        let ps = r#"
Get-Partition | Where-Object DriveLetter -ne $null | ForEach-Object {
    $p = $_
    $v = Get-Volume -DriveLetter $p.DriveLetter -ErrorAction SilentlyContinue
    $d = Get-Disk -Number $p.DiskNumber -ErrorAction SilentlyContinue
    [PSCustomObject]@{
        DriveLetter = $p.DriveLetter
        Label       = if($v){$v.FileSystemLabel}else{""}
        FileSystem  = if($v){$v.FileSystem}else{""}
        Size        = if($v){$v.Size}else{0}
        Free        = if($v){$v.SizeRemaining}else{0}
        DiskNumber  = $p.DiskNumber
        MediaType   = if($d){$d.MediaType}else{""}
    }
} | ConvertTo-Json
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
                    if let Some(info) = parse_partition_info(&v, &disk_media_map) {
                        res.push(info);
                    }
                }
            }
            serde_json::Value::Object(_) => {
                let v = parsed;
                if let Some(info) = parse_partition_info(&v, &disk_media_map) {
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
