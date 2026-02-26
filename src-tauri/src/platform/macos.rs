use crate::commands::disk::DiskInfo;
use crate::{AppError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use tracing::{info, warn};

fn normalize_disk_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let candidate = trimmed.strip_prefix("/dev/").unwrap_or(trimmed);
    if !candidate.starts_with("disk") {
        return None;
    }
    let suffix = &candidate[4..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("disk{}", suffix))
}

fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn plist_value_to_json(value: &[u8]) -> Result<Value> {
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

fn diskutil_plist_json(args: &[&str]) -> Result<Value> {
    let output = Command::new("diskutil")
        .args(args)
        .output()
        .map_err(AppError::io)?;

    if !output.status.success() {
        let err = to_text(&output.stderr);
        let out = to_text(&output.stdout);
        let detail = if !err.is_empty() { err } else { out };
        return Err(AppError::DiskError(format!("diskutil failed: {}", detail)));
    }

    plist_value_to_json(&output.stdout)
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn pick_volume_from_partition(partition: &Value) -> Option<String> {
    let mount_point = json_str(partition, "MountPoint");
    if !mount_point.is_empty() {
        return Some(mount_point);
    }
    let volume_name = json_str(partition, "VolumeName");
    if !volume_name.is_empty() {
        return Some(volume_name);
    }
    None
}

fn build_volume_map(list_json: &Value) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let Some(disks) = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
    else {
        return map;
    };

    for disk in disks {
        let disk_id = json_str(disk, "DeviceIdentifier");
        if disk_id.is_empty() {
            continue;
        }

        let volume = disk
            .get("Partitions")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .find_map(|p| {
                        let mount_point = json_str(p, "MountPoint");
                        if mount_point.is_empty() {
                            None
                        } else {
                            Some(mount_point)
                        }
                    })
                    .or_else(|| parts.iter().find_map(pick_volume_from_partition))
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        map.insert(disk_id, volume);
    }

    map
}

fn parse_media_type(info: &Value) -> String {
    if json_bool(info, "SolidState") {
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

fn build_disk_info(info_json: &Value, volume: String) -> DiskInfo {
    let id = json_str(info_json, "DeviceIdentifier");
    let device = json_str(info_json, "DeviceNode");
    let name = {
        let media_name = json_str(info_json, "MediaName");
        if !media_name.is_empty() {
            media_name
        } else {
            let io_name = json_str(info_json, "IORegistryEntryName");
            if !io_name.is_empty() {
                io_name
            } else {
                id.clone()
            }
        }
    };

    let internal = json_bool(info_json, "Internal");
    let bus = json_str(info_json, "BusProtocol");
    let removable = json_bool(info_json, "RemovableMediaOrExternalDevice")
        || json_bool(info_json, "RemovableMedia")
        || json_bool(info_json, "Ejectable")
        || !internal
        || bus.eq_ignore_ascii_case("USB");

    let drive_type = if bus.is_empty() {
        if internal {
            "Fixed".to_string()
        } else {
            "External".to_string()
        }
    } else {
        bus
    };

    let size = {
        let total = json_u64(info_json, "TotalSize");
        if total > 0 {
            total
        } else {
            let size = json_u64(info_json, "Size");
            if size > 0 {
                size
            } else {
                json_u64(info_json, "IOKitSize")
            }
        }
    };

    let index = id.trim_start_matches("disk").to_string();
    let media_type = parse_media_type(info_json);

    DiskInfo {
        id,
        name,
        size,
        removable,
        device,
        drive_type,
        media_type,
        index,
        volume,
        is_system: internal,
    }
}

fn list_disks_json() -> Result<Value> {
    match diskutil_plist_json(&["list", "-plist", "physical"]) {
        Ok(v) => Ok(v),
        Err(e) => {
            warn!(
                "diskutil list -plist physical failed, fallback to full list: {}",
                e
            );
            diskutil_plist_json(&["list", "-plist"])
        }
    }
}

/// List all disks on macOS
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    info!("Listing disks on macOS");

    let list_json = list_disks_json()?;
    let volume_map = build_volume_map(&list_json);

    let disks = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::DiskError("Invalid diskutil list output".to_string()))?;

    let mut result = Vec::new();
    for disk in disks {
        let disk_id = json_str(disk, "DeviceIdentifier");
        if normalize_disk_id(&disk_id).is_none() {
            continue;
        }

        let node = format!("/dev/{}", disk_id);
        let info_json = match diskutil_plist_json(&["info", "-plist", &node]) {
            Ok(v) => v,
            Err(e) => {
                warn!("Skipping {} because diskutil info failed: {}", disk_id, e);
                continue;
            }
        };

        if !json_bool(&info_json, "WholeDisk") {
            continue;
        }

        if json_str(&info_json, "VirtualOrPhysical").eq_ignore_ascii_case("Virtual") {
            continue;
        }

        let volume = volume_map.get(&disk_id).cloned().unwrap_or_default();
        result.push(build_disk_info(&info_json, volume));
    }

    result.sort_by_key(|d| d.index.parse::<u32>().unwrap_or(u32::MAX));
    Ok(result)
}

/// Get disk info on macOS
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let normalized = normalize_disk_id(disk_id).ok_or_else(|| {
        AppError::InvalidParameter(format!("Invalid disk identifier: {}", disk_id))
    })?;

    let list_json = list_disks_json()?;
    let volume_map = build_volume_map(&list_json);

    let node = format!("/dev/{}", normalized);
    let info_json = diskutil_plist_json(&["info", "-plist", &node])?;

    let target_id = if json_bool(&info_json, "WholeDisk") {
        normalized
    } else {
        let parent = json_str(&info_json, "ParentWholeDisk");
        if parent.is_empty() {
            return Err(AppError::DeviceNotFound(disk_id.to_string()));
        }
        parent
    };

    let target_node = format!("/dev/{}", target_id);
    let whole_info = diskutil_plist_json(&["info", "-plist", &target_node])?;
    if json_str(&whole_info, "VirtualOrPhysical").eq_ignore_ascii_case("Virtual") {
        return Err(AppError::DeviceNotFound(disk_id.to_string()));
    }

    let volume = volume_map.get(&target_id).cloned().unwrap_or_default();
    Ok(build_disk_info(&whole_info, volume))
}

/// Start USB monitoring on macOS
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Err(AppError::Unsupported(
        "macOS USB monitoring is not implemented yet".to_string(),
    ))
}

/// Stop USB monitoring on macOS
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Err(AppError::Unsupported(
        "macOS USB monitoring is not implemented yet".to_string(),
    ))
}
