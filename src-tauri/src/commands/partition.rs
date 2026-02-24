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
    pub media_type: String,
}

/// List mounted partitions with drive letters (Windows only)
#[tauri::command]
pub async fn list_partitions() -> Result<Vec<PartitionInfo>> {
    #[cfg(target_os = "windows")]
    {
        use crate::utils::command::CommandExecutor;

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
                    res.push(PartitionInfo {
                        drive_letter: v["DriveLetter"].as_str().unwrap_or("").to_string(),
                        label: v["Label"].as_str().unwrap_or("").to_string(),
                        filesystem: v["FileSystem"].as_str().unwrap_or("").to_string(),
                        size: v["Size"].as_u64().unwrap_or(0),
                        free: v["Free"].as_u64().unwrap_or(0),
                        disk_number: v["DiskNumber"].as_u64().unwrap_or(0) as u32,
                        media_type: v["MediaType"].as_str().unwrap_or("").to_string(),
                    });
                }
            }
            serde_json::Value::Object(_) => {
                let v = parsed;
                res.push(PartitionInfo {
                    drive_letter: v["DriveLetter"].as_str().unwrap_or("").to_string(),
                    label: v["Label"].as_str().unwrap_or("").to_string(),
                    filesystem: v["FileSystem"].as_str().unwrap_or("").to_string(),
                    size: v["Size"].as_u64().unwrap_or(0),
                    free: v["Free"].as_u64().unwrap_or(0),
                    disk_number: v["DiskNumber"].as_u64().unwrap_or(0) as u32,
                    media_type: v["MediaType"].as_str().unwrap_or("").to_string(),
                });
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
