use crate::commands::disk::{DiskDiagnostics, DiskInfo, SmartAttribute};
use crate::{AppError, Result};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
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

fn parse_disk_number(input: &str) -> Option<u32> {
    normalize_disk_id(input)?
        .trim_start_matches("disk")
        .parse::<u32>()
        .ok()
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

fn system_disk_identifier() -> Option<String> {
    let root_info = diskutil_plist_json(&["info", "-plist", "/"]).ok()?;
    let parent = json_str(&root_info, "ParentWholeDisk");
    if normalize_disk_id(&parent).is_some() {
        return Some(parent);
    }
    let id = json_str(&root_info, "DeviceIdentifier");
    normalize_disk_id(&id)
}

fn parse_diskutil_smart_status(raw_status: &str) -> (String, bool, bool) {
    let raw = raw_status.trim();
    if raw.is_empty() {
        return ("Unknown".to_string(), false, false);
    }

    let normalized = raw.to_ascii_lowercase();
    if normalized.contains("fail") {
        return ("Warning".to_string(), true, true);
    }
    if normalized.contains("verified")
        || normalized.contains("passed")
        || normalized.contains("ok")
        || normalized.contains("healthy")
    {
        return ("Healthy".to_string(), true, true);
    }
    if normalized.contains("not supported") || normalized.contains("unsupported") {
        return ("Unsupported".to_string(), false, false);
    }

    (raw.to_string(), true, true)
}

fn is_nvme_hint(info_json: &Value, bus: &str, model: &str) -> bool {
    if bus.to_ascii_uppercase().contains("NVME") {
        return true;
    }

    let mut haystack = String::new();
    haystack.push_str(bus);
    haystack.push(' ');
    haystack.push_str(model);
    haystack.push(' ');
    haystack.push_str(&json_str(info_json, "MediaName"));
    haystack.push(' ');
    haystack.push_str(&json_str(info_json, "IORegistryEntryName"));
    haystack.push(' ');
    haystack.push_str(&json_str(info_json, "DeviceTreePath"));

    haystack.to_ascii_uppercase().contains("NVME")
}

fn build_disk_diagnostics(info_json: &Value, system_disk_id: Option<&str>) -> DiskDiagnostics {
    let id = json_str(info_json, "DeviceIdentifier");
    let disk_number = parse_disk_number(&id).unwrap_or(0);

    let model = {
        let media_name = json_str(info_json, "MediaName");
        if !media_name.is_empty() {
            media_name
        } else {
            let io_name = json_str(info_json, "IORegistryEntryName");
            if !io_name.is_empty() { io_name } else { id.clone() }
        }
    };

    let bus = json_str(info_json, "BusProtocol");
    let is_usb = bus.eq_ignore_ascii_case("USB");
    let transport_type = if is_nvme_hint(info_json, &bus, &model) {
        "NVMe".to_string()
    } else if !bus.is_empty() {
        bus.clone()
    } else {
        "Unknown".to_string()
    };
    let interface_type = if transport_type.eq_ignore_ascii_case("NVMe") {
        "NVMExpress".to_string()
    } else if !bus.is_empty() {
        bus.clone()
    } else {
        "Unknown".to_string()
    };

    let size_bytes = {
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

    let smart_raw = json_str(info_json, "SMARTStatus");
    let (health_status, smart_supported, smart_enabled) = parse_diskutil_smart_status(&smart_raw);

    let mut reliability = Map::new();
    if !smart_raw.is_empty() {
        reliability.insert("Diskutil.SMARTStatus".to_string(), Value::String(smart_raw));
    }

    let unique_id = {
        let disk_uuid = json_str(info_json, "DiskUUID");
        if !disk_uuid.is_empty() {
            disk_uuid
        } else {
            json_str(info_json, "DeviceTreePath")
        }
    };

    let system = system_disk_id
        .map(|sys| sys.eq_ignore_ascii_case(&id))
        .unwrap_or(false);

    DiskDiagnostics {
        id,
        disk_number,
        model: model.clone(),
        friendly_name: model,
        serial_number: json_str(info_json, "SerialNumber"),
        firmware_version: json_str(info_json, "FirmwareVersion"),
        interface_type,
        pnp_device_id: json_str(info_json, "DeviceNode"),
        usb_vendor_id: String::new(),
        usb_product_id: String::new(),
        transport_type,
        is_usb,
        bus_type: if bus.is_empty() {
            "Unknown".to_string()
        } else {
            bus
        },
        unique_id,
        media_type: parse_media_type(info_json),
        size_bytes,
        is_system: system,
        health_status,
        smart_supported,
        smart_enabled,
        smart_data_source: "NONE".to_string(),
        ata_smart_available: false,
        reliability_available: !reliability.is_empty(),
        temperature_c: None,
        power_on_hours: None,
        power_cycle_count: None,
        percentage_used: None,
        read_errors_total: None,
        write_errors_total: None,
        host_reads_total: None,
        host_writes_total: None,
        smart_attributes: Vec::new(),
        reliability: Value::Object(reliability),
        notes: Vec::new(),
    }
}

/// List detailed disk diagnostics on macOS
pub async fn list_disk_diagnostics() -> Result<Vec<DiskDiagnostics>> {
    let list_json = list_disks_json()?;
    let disks = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::DiskError("Invalid diskutil list output".to_string()))?;

    let system_disk = system_disk_identifier();
    let mut diagnostics = Vec::new();

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

        diagnostics.push(build_disk_diagnostics(&info_json, system_disk.as_deref()));
    }

    diagnostics.sort_by_key(|d| d.disk_number);
    enrich_with_smartctl(&mut diagnostics);
    normalize_endurance_percentage(&mut diagnostics);
    Ok(diagnostics)
}

fn enrich_with_smartctl(diagnostics: &mut [DiskDiagnostics]) {
    if diagnostics.is_empty() {
        return;
    }

    if !smartctl_installed() {
        for diag in diagnostics.iter_mut() {
            add_note_unique(
                diag,
                "smartctl not found in PATH; install smartmontools to enable extended SMART details.",
            );
        }
        return;
    }

    let scan_entries = smartctl_scan_entries();
    for diag in diagnostics.iter_mut() {
        if let Some(payload) = get_smartctl_payload_for_disk(diag, scan_entries.as_deref()) {
            apply_smartctl_payload(diag, &payload);
            add_note_unique(diag, "Extended SMART details were enhanced via smartctl.");
        }
    }
}

fn smartctl_installed() -> bool {
    run_smartctl_allow_fail(&["--version"]).is_some()
}

#[derive(Debug, Clone)]
struct SmartctlScanEntry {
    name: String,
    device_type: Option<String>,
    info_name: String,
}

fn smartctl_scan_entries() -> Option<Vec<SmartctlScanEntry>> {
    let scan_cmds: [&[&str]; 4] = [
        &["--scan-open", "-j"],
        &["--scan", "-j"],
        &["--scan-open"],
        &["--scan"],
    ];

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for args in scan_cmds {
        let Some(output) = run_smartctl_allow_fail(args) else {
            continue;
        };

        let parsed = if args.contains(&"-j") {
            extract_json_value(&output)
                .map(|payload| parse_smartctl_scan_entries_json(&payload))
                .unwrap_or_default()
        } else {
            parse_smartctl_scan_entries_text(&output)
        };

        for entry in parsed {
            let key = format!(
                "{}\u{1F}{}",
                entry.name.to_ascii_lowercase(),
                entry
                    .device_type
                    .clone()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            );
            if seen.insert(key) {
                entries.push(entry);
            }
        }
    }

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn parse_smartctl_scan_entries_json(payload: &Value) -> Vec<SmartctlScanEntry> {
    let mut entries = Vec::new();
    let Some(devices) = payload.get("devices").and_then(Value::as_array) else {
        return entries;
    };

    for dev in devices {
        let name = dev
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let info_name = dev
            .get("info_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let device_type = dev
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        entries.push(SmartctlScanEntry {
            name,
            device_type,
            info_name,
        });
    }

    entries
}

fn parse_smartctl_scan_entries_text(output: &str) -> Vec<SmartctlScanEntry> {
    let mut entries = Vec::new();

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (main_part, info_part) = line
            .split_once('#')
            .map(|(a, b)| (a.trim(), b.trim()))
            .unwrap_or((line, ""));

        let mut parts = main_part.split_whitespace();
        let Some(name) = parts.next().map(str::trim) else {
            continue;
        };
        if !name.starts_with('/') {
            continue;
        }

        let mut device_type: Option<String> = None;
        while let Some(token) = parts.next() {
            if token == "-d" {
                device_type = parts
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned);
                break;
            }
        }

        entries.push(SmartctlScanEntry {
            name: name.to_string(),
            device_type,
            info_name: info_part.to_string(),
        });
    }

    entries
}

fn smartctl_entry_matches_disk(entry: &SmartctlScanEntry, disk_number: u32) -> bool {
    let key_disk = format!("disk{}", disk_number).to_ascii_lowercase();
    let key_rdisk = format!("rdisk{}", disk_number).to_ascii_lowercase();
    let name = entry.name.to_ascii_lowercase();
    let info = entry.info_name.to_ascii_lowercase();

    [name, info]
        .iter()
        .any(|v| v.contains(&key_disk) || v.contains(&key_rdisk))
}

fn push_smartctl_attempt(
    attempts: &mut Vec<Vec<String>>,
    seen: &mut HashSet<String>,
    device: &str,
    device_type: Option<&str>,
) {
    let mut args = vec!["-x".to_string(), "-j".to_string()];
    if let Some(dt) = device_type.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("-d".to_string());
        args.push(dt.to_string());
    }
    args.push(device.to_string());

    let key = args.join("\u{1F}");
    if seen.insert(key) {
        attempts.push(args);
    }
}

fn normalized_eq(a: &str, b: &str) -> bool {
    let na: String = a
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let nb: String = b
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    !na.is_empty() && !nb.is_empty() && na == nb
}

fn payload_matches_diag(payload: &Value, diag: &DiskDiagnostics) -> bool {
    let disk_name = format!("/dev/disk{}", diag.disk_number).to_ascii_lowercase();
    let rdisk_name = format!("/dev/rdisk{}", diag.disk_number).to_ascii_lowercase();
    if get_string_path(payload, &["device", "name"])
        .map(|n| {
            let lower = n.to_ascii_lowercase();
            lower.contains(&disk_name) || lower.contains(&rdisk_name)
        })
        .unwrap_or(false)
    {
        return true;
    }

    let payload_serial = get_string_path(payload, &["serial_number"]).unwrap_or_default();
    let diag_serial = diag.serial_number.trim();
    if !diag_serial.is_empty()
        && !is_masked_serial(diag_serial)
        && !payload_serial.is_empty()
        && normalized_eq(diag_serial, &payload_serial)
    {
        return true;
    }

    let payload_model = get_string_path(payload, &["model_name"]).unwrap_or_default();
    let diag_model = if diag.model.trim().is_empty() {
        diag.friendly_name.trim()
    } else {
        diag.model.trim()
    };
    let model_like = !payload_model.is_empty()
        && !diag_model.is_empty()
        && (payload_model
            .to_ascii_uppercase()
            .contains(&diag_model.to_ascii_uppercase())
            || diag_model
                .to_ascii_uppercase()
                .contains(&payload_model.to_ascii_uppercase()));

    if model_like {
        if let Some(cap) = get_u64_path(payload, &["user_capacity", "bytes"]) {
            let size = diag.size_bytes;
            if size > 0 {
                let diff = cap.abs_diff(size);
                let tolerance = (size / 20).max(64 * 1024 * 1024);
                if diff <= tolerance {
                    return true;
                }
            }
        }
    }

    false
}

fn get_smartctl_payload_for_disk(
    diag: &DiskDiagnostics,
    scan_entries: Option<&[SmartctlScanEntry]>,
) -> Option<Value> {
    let disk = format!("/dev/disk{}", diag.disk_number);
    let rdisk = format!("/dev/rdisk{}", diag.disk_number);
    let mut attempts: Vec<Vec<String>> = Vec::new();
    let mut seen = HashSet::new();

    for dev in [&disk, &rdisk] {
        push_smartctl_attempt(&mut attempts, &mut seen, dev, None);
    }

    let is_nvme = is_nvme_diag(diag);

    if diag.is_usb {
        for dev in [&disk, &rdisk] {
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sat,auto"));
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sat"));
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sat,12"));
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sat,16"));
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("scsi"));
        }
    }

    if is_nvme {
        for dev in [&disk, &rdisk] {
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("nvme"));
        }
        if diag.is_usb {
            for dev in [&disk, &rdisk] {
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntjmicron"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntjmicron,0"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntjmicron,1"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntasmedia"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntrealtek"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntrealtek,0"));
                push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sntrealtek,1"));
            }
        }
    } else {
        for dev in [&disk, &rdisk] {
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("sat"));
            push_smartctl_attempt(&mut attempts, &mut seen, dev, Some("scsi"));
        }
    }

    if let Some(entries) = scan_entries {
        for entry in entries
            .iter()
            .filter(|e| smartctl_entry_matches_disk(e, diag.disk_number))
        {
            push_smartctl_attempt(
                &mut attempts,
                &mut seen,
                &entry.name,
                entry.device_type.as_deref(),
            );
        }
    }

    for args in attempts {
        if let Some(payload) = run_smartctl_json(&args) {
            if is_useful_smartctl_payload(&payload) {
                return Some(payload);
            }
        }
    }

    if let Some(entries) = scan_entries {
        for entry in entries {
            let mut fallback = vec!["-x".to_string(), "-j".to_string()];
            if let Some(dt) = entry
                .device_type
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                fallback.push("-d".to_string());
                fallback.push(dt.to_string());
            }
            fallback.push(entry.name.clone());

            if let Some(payload) = run_smartctl_json(&fallback) {
                if is_useful_smartctl_payload(&payload) && payload_matches_diag(&payload, diag) {
                    return Some(payload);
                }
            }
        }
    }

    None
}

fn run_smartctl_json(args: &[String]) -> Option<Value> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_smartctl_allow_fail(&arg_refs)?;
    extract_json_value(&output)
}

fn run_smartctl_allow_fail(args: &[&str]) -> Option<String> {
    for cmd in smartctl_candidates() {
        let output = match Command::new(&cmd).args(args).output() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let merged = if stdout.trim().is_empty() {
            stderr
        } else if stderr.trim().is_empty() {
            stdout
        } else {
            format!("{stdout}\n{stderr}")
        };

        if !merged.trim().is_empty() {
            return Some(merged);
        }
    }
    None
}

fn smartctl_candidates() -> Vec<String> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push_candidate_path(
                &mut candidates,
                dir.join("smartmontools").join("bin").join("smartctl"),
            );
            push_candidate_path(&mut candidates, dir.join("resources").join("smartctl"));
            push_candidate_path(
                &mut candidates,
                dir.join("..").join("resources").join("smartctl"),
            );
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        push_candidate_path(
            &mut candidates,
            cwd.join("src-tauri")
                .join("resources")
                .join("smartmontools")
                .join("bin")
                .join("smartctl"),
        );
        push_candidate_path(
            &mut candidates,
            cwd.join("src-tauri")
                .join("resources")
                .join("smartctl")
                .join("smartctl"),
        );
        push_candidate_path(
            &mut candidates,
            cwd.join("useable_software")
                .join("smartmontools")
                .join("bin")
                .join("smartctl"),
        );
    }

    candidates.extend([
        "smartctl".to_string(),
        "/opt/homebrew/sbin/smartctl".to_string(),
        "/usr/local/sbin/smartctl".to_string(),
        "/usr/sbin/smartctl".to_string(),
    ]);

    let mut filtered = Vec::new();
    for c in candidates {
        if c.contains('/') {
            if std::path::Path::new(&c).exists() {
                filtered.push(c);
            }
        } else {
            filtered.push(c);
        }
    }

    let mut seen = HashSet::new();
    filtered
        .into_iter()
        .filter(|s| seen.insert(s.to_ascii_lowercase()))
        .collect()
}

fn push_candidate_path(candidates: &mut Vec<String>, path: std::path::PathBuf) {
    candidates.push(path.to_string_lossy().to_string());
}

fn extract_json_value(output: &str) -> Option<Value> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed.find('{').or_else(|| trimmed.find('['))?;
    serde_json::from_str(&trimmed[start..]).ok()
}

fn is_useful_smartctl_payload(payload: &Value) -> bool {
    payload.get("smartctl").is_some()
        && (payload.get("model_name").is_some()
            || payload.get("serial_number").is_some()
            || payload.pointer("/ata_smart_attributes/table").is_some()
            || payload.get("nvme_smart_health_information_log").is_some()
            || payload.pointer("/temperature/current").is_some()
            || payload.pointer("/power_on_time/hours").is_some())
}

fn apply_smartctl_payload(diag: &mut DiskDiagnostics, payload: &Value) {
    if let Some(model) = get_string_path(payload, &["model_name"]).filter(|s| !s.is_empty()) {
        if diag.model.trim().is_empty() {
            diag.model = model;
        }
    }

    if let Some(firmware) =
        get_string_path(payload, &["firmware_version"]).filter(|s| !s.is_empty())
    {
        if diag.firmware_version.trim().is_empty() {
            diag.firmware_version = firmware;
        }
    }

    if let Some(serial) = get_string_path(payload, &["serial_number"]).filter(|s| !s.is_empty()) {
        if diag.serial_number.trim().is_empty() || is_masked_serial(&diag.serial_number) {
            diag.serial_number = serial;
        }
    }

    if let Some(protocol) = get_string_path(payload, &["device", "protocol"]) {
        if protocol.eq_ignore_ascii_case("nvme") {
            diag.transport_type = "NVMe".to_string();
            diag.interface_type = "NVMExpress".to_string();
            if diag.media_type.eq_ignore_ascii_case("unknown") || diag.media_type.is_empty() {
                diag.media_type = "SSD".to_string();
            }
        }
    }

    if let Some(rotation) = get_u64_path(payload, &["rotation_rate"]) {
        if rotation == 0 {
            diag.media_type = "SSD".to_string();
        } else if rotation > 0 {
            diag.media_type = "HDD".to_string();
        }
    }

    if diag.temperature_c.is_none() {
        diag.temperature_c = extract_smartctl_temperature(payload);
    }
    if diag.power_on_hours.is_none() {
        diag.power_on_hours = get_u64_path(payload, &["power_on_time", "hours"]).or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "power_on_hours"],
            )
        });
    }
    if diag.power_cycle_count.is_none() {
        diag.power_cycle_count = get_u64_path(payload, &["power_cycle_count"]).or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "power_cycles"],
            )
        });
    }
    if diag.percentage_used.is_none() {
        diag.percentage_used = get_f64_path(
            payload,
            &["nvme_smart_health_information_log", "percentage_used"],
        );
    }
    if diag.host_reads_total.is_none() {
        diag.host_reads_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "host_reads"],
        )
        .or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "data_units_read"],
            )
        });
    }
    if diag.host_writes_total.is_none() {
        diag.host_writes_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "host_writes"],
        )
        .or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "data_units_written"],
            )
        });
    }
    if diag.read_errors_total.is_none() {
        diag.read_errors_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "media_errors"],
        );
    }
    if diag.write_errors_total.is_none() {
        diag.write_errors_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "num_err_log_entries"],
        );
    }

    if let Some(enabled) = get_bool_path(payload, &["smart_support", "enabled"]) {
        diag.smart_enabled = enabled;
        diag.smart_supported = true;
    }
    if get_bool_path(payload, &["smart_status", "passed"]).is_some() {
        diag.smart_supported = true;
    }

    let smartctl_attrs = parse_smartctl_ata_attributes(payload);
    if !smartctl_attrs.is_empty() {
        if diag.smart_attributes.len() < smartctl_attrs.len() {
            diag.smart_attributes = smartctl_attrs.clone();
        }
        diag.ata_smart_available = true;
        diag.smart_supported = true;
        diag.smart_data_source = merge_smart_source(&diag.smart_data_source, "SMARTCTL_ATA");

        if diag.temperature_c.is_none() {
            diag.temperature_c = smartctl_attr_raw(&smartctl_attrs, 194)
                .or_else(|| smartctl_attr_raw(&smartctl_attrs, 190))
                .map(raw_temp_to_celsius);
        }
        if diag.power_on_hours.is_none() {
            diag.power_on_hours = smartctl_attr_raw_u64(&smartctl_attrs, 9);
        }
        if diag.power_cycle_count.is_none() {
            diag.power_cycle_count = smartctl_attr_raw_u64(&smartctl_attrs, 12);
        }
        if diag.host_writes_total.is_none() {
            diag.host_writes_total = smartctl_attr_raw_u64(&smartctl_attrs, 241);
        }
        if diag.host_reads_total.is_none() {
            diag.host_reads_total = smartctl_attr_raw_u64(&smartctl_attrs, 242);
        }
        if diag.read_errors_total.is_none() {
            diag.read_errors_total = smartctl_attr_raw_u64(&smartctl_attrs, 1)
                .or_else(|| smartctl_attr_raw_u64(&smartctl_attrs, 187));
        }
        if diag.write_errors_total.is_none() {
            diag.write_errors_total = smartctl_attr_raw_u64(&smartctl_attrs, 200)
                .or_else(|| smartctl_attr_raw_u64(&smartctl_attrs, 181));
        }
        if let Some(used) = derive_endurance_used_from_attrs(&smartctl_attrs) {
            if diag.percentage_used.is_none() || diag.percentage_used.unwrap_or(0.0) <= 0.0 {
                diag.percentage_used = Some(used);
            }
        }
    }

    if payload.get("nvme_smart_health_information_log").is_some() {
        diag.smart_supported = true;
        diag.smart_data_source = merge_smart_source(&diag.smart_data_source, "SMARTCTL_NVME");
    }

    let mut rel = match std::mem::take(&mut diag.reliability) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    merge_smartctl_reliability(&mut rel, payload, diag.smart_attributes.len());
    diag.reliability_available = !rel.is_empty();
    diag.reliability = Value::Object(rel);
}

fn merge_smartctl_reliability(
    reliability: &mut Map<String, Value>,
    payload: &Value,
    attr_count: usize,
) {
    insert_if_absent(
        reliability,
        "Smartctl.Device",
        get_string_path(payload, &["device", "name"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.DeviceType",
        get_string_path(payload, &["device", "type"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.Protocol",
        get_string_path(payload, &["device", "protocol"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.ExitStatus",
        get_u64_path(payload, &["smartctl", "exit_status"]).map(Value::from),
    );
    insert_if_absent(
        reliability,
        "Smartctl.RotationRate",
        get_u64_path(payload, &["rotation_rate"]).map(Value::from),
    );
    if let Some(capacity) = get_u64_path(payload, &["user_capacity", "bytes"]) {
        insert_if_absent(
            reliability,
            "Smartctl.UserCapacityBytes",
            Some(Value::from(capacity)),
        );
    }
    if payload.get("ata_smart_attributes").is_some() {
        insert_if_absent(
            reliability,
            "Smartctl.AtaAttributeCount",
            Some(Value::from(attr_count as u64)),
        );
    }

    let nvme_fields = [
        ("Nvme.CriticalWarning", "critical_warning"),
        ("Nvme.AvailableSpare", "available_spare"),
        ("Nvme.AvailableSpareThreshold", "available_spare_threshold"),
        ("Nvme.PercentageUsed", "percentage_used"),
        ("Nvme.DataUnitsRead", "data_units_read"),
        ("Nvme.DataUnitsWritten", "data_units_written"),
        ("Nvme.HostReads", "host_reads"),
        ("Nvme.HostWrites", "host_writes"),
        ("Nvme.ControllerBusyTime", "controller_busy_time"),
        ("Nvme.PowerCycles", "power_cycles"),
        ("Nvme.PowerOnHours", "power_on_hours"),
        ("Nvme.UnsafeShutdowns", "unsafe_shutdowns"),
        ("Nvme.MediaErrors", "media_errors"),
        ("Nvme.ErrorLogEntries", "num_err_log_entries"),
    ];

    for (key, field) in nvme_fields {
        let path = ["nvme_smart_health_information_log", field];
        if let Some(v) = get_path(payload, &path).and_then(value_to_json_scalar) {
            insert_if_absent(reliability, key, Some(v));
        }
    }
}

fn parse_smartctl_ata_attributes(payload: &Value) -> Vec<SmartAttribute> {
    let mut attrs = Vec::new();
    let Some(table) =
        get_path(payload, &["ata_smart_attributes", "table"]).and_then(Value::as_array)
    else {
        return attrs;
    };

    for item in table {
        let Some(id) = get_path(item, &["id"]).and_then(value_to_u64) else {
            continue;
        };
        let name = get_string_path(item, &["name"]).unwrap_or_else(|| format!("Attribute {}", id));
        let current = get_path(item, &["value"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let worst = get_path(item, &["worst"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let threshold = get_path(item, &["thresh"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let raw = get_path(item, &["raw"])
            .and_then(|v| get_path(v, &["value"]).or(Some(v)))
            .and_then(value_to_u64);
        let raw_hex = raw
            .map(|v| format!("0x{:X}", v))
            .unwrap_or_else(String::new);

        attrs.push(SmartAttribute {
            id: id as u32,
            name,
            current,
            worst,
            threshold,
            raw,
            raw_hex,
        });
    }

    attrs
}

fn smartctl_attr_raw(attrs: &[SmartAttribute], id: u32) -> Option<f64> {
    attrs
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.raw)
        .map(|v| v as f64)
}

fn smartctl_attr_raw_u64(attrs: &[SmartAttribute], id: u32) -> Option<u64> {
    attrs.iter().find(|a| a.id == id).and_then(|a| a.raw)
}

fn smartctl_attr_current(attrs: &[SmartAttribute], id: u32) -> Option<u32> {
    attrs.iter().find(|a| a.id == id).and_then(|a| a.current)
}

fn derive_endurance_used_from_attrs(attrs: &[SmartAttribute]) -> Option<f64> {
    let life_left = smartctl_attr_current(attrs, 231)
        .or_else(|| smartctl_attr_current(attrs, 233))
        .or_else(|| smartctl_attr_current(attrs, 202));

    if let Some(v) = life_left {
        if v > 0 && v < 100 {
            return Some((100.0 - v as f64).clamp(0.0, 100.0));
        }
    }

    if let Some(raw_used) = smartctl_attr_raw_u64(attrs, 202) {
        if raw_used > 0 && raw_used <= 100 {
            return Some(raw_used as f64);
        }
    }

    None
}

fn normalize_endurance_percentage(diagnostics: &mut [DiskDiagnostics]) {
    for diag in diagnostics.iter_mut() {
        if is_nvme_diag(diag) {
            continue;
        }

        let estimated = derive_endurance_used_from_attrs(&diag.smart_attributes);
        match (diag.percentage_used, estimated) {
            (Some(v), Some(used)) if v <= 0.0 && used > 0.0 => {
                diag.percentage_used = Some(used);
            }
            (Some(v), None) if v <= 0.0 => {
                diag.percentage_used = None;
            }
            (None, Some(used)) if used > 0.0 => {
                diag.percentage_used = Some(used);
            }
            _ => {}
        }
    }
}

fn is_nvme_diag(diag: &DiskDiagnostics) -> bool {
    let haystack = format!(
        "{} {} {} {} {}",
        diag.transport_type, diag.interface_type, diag.bus_type, diag.model, diag.pnp_device_id
    )
    .to_ascii_uppercase();
    haystack.contains("NVME")
}

fn raw_temp_to_celsius(raw: f64) -> f64 {
    if raw <= 200.0 {
        raw
    } else {
        (raw as u64 & 0xFF) as f64
    }
}

fn extract_smartctl_temperature(payload: &Value) -> Option<f64> {
    if let Some(temp) = get_f64_path(payload, &["temperature", "current"]) {
        return Some(temp);
    }
    if let Some(mut nvme_temp) = get_f64_path(
        payload,
        &["nvme_smart_health_information_log", "temperature"],
    ) {
        if nvme_temp > 200.0 {
            nvme_temp -= 273.15;
        }
        return Some((nvme_temp * 10.0).round() / 10.0);
    }
    None
}

fn merge_smart_source(existing: &str, add: &str) -> String {
    if existing.is_empty() || existing.eq_ignore_ascii_case("none") {
        return add.to_string();
    }
    if existing.split('+').any(|x| x.eq_ignore_ascii_case(add)) {
        return existing.to_string();
    }
    format!("{existing}+{add}")
}

fn add_note_unique(diag: &mut DiskDiagnostics, note: &str) {
    if !diag.notes.iter().any(|n| n == note) {
        diag.notes.push(note.to_string());
    }
}

fn insert_if_absent(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if map.contains_key(key) {
        return;
    }
    if let Some(v) = value {
        if !v.is_null() {
            map.insert(key.to_string(), v);
        }
    }
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn get_string_path(value: &Value, path: &[&str]) -> Option<String> {
    let v = get_path(value, path)?;
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn get_u64_path(value: &Value, path: &[&str]) -> Option<u64> {
    get_path(value, path).and_then(value_to_u64)
}

fn get_f64_path(value: &Value, path: &[&str]) -> Option<f64> {
    get_path(value, path).and_then(value_to_f64)
}

fn get_bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    get_path(value, path).and_then(Value::as_bool)
}

fn value_to_json_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => Some(value.clone()),
        Value::Object(_) => {
            if let Some(n) = value_to_u64(value) {
                Some(Value::from(n))
            } else if let Some(f) = value_to_f64(value) {
                Some(Value::from(f))
            } else {
                None
            }
        }
        Value::Array(_) => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => parse_u64_string(s),
        Value::Object(map) => map.get("value").and_then(value_to_u64),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_f64_string(s),
        Value::Object(map) => map.get("value").and_then(value_to_f64),
        _ => None,
    }
}

fn parse_u64_string(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(n);
    }

    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

fn parse_f64_string(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        return Some(n);
    }

    let normalized: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if normalized.is_empty() {
        None
    } else {
        normalized.parse::<f64>().ok()
    }
}

fn is_masked_serial(serial: &str) -> bool {
    let normalized: String = serial
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if normalized.len() < 4 {
        return true;
    }
    if normalized.chars().all(|c| c == '0' || c == 'D') {
        return true;
    }
    if normalized.chars().all(|c| c == '0') {
        return true;
    }
    if normalized.chars().all(|c| c == 'F') {
        return true;
    }
    false
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
