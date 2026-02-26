//! macOS write service (WTG migration implementation)
//! Provides a macOS-side WTG write pipeline:
//! - preflight validation
//! - target disk writable check (including NTFS remount helper)
//! - partitioning + formatting
//! - WIM/ESD apply (wimlib-imagex)
//! - basic UEFI boot file staging

use crate::models::{BootMode, Disk, ImageInfo, WriteProgress, WriteStatus, WtgConfig};
use crate::utils::macos_admin;
use crate::utils::progress::PROGRESS_REPORTER;
use crate::{AppError, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tracing::{info, warn};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .args([
            "-lc",
            &format!(
                "export PATH='/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH'; command -v {} >/dev/null 2>&1",
                cmd
            ),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn parse_u64_digits(value: &str) -> Option<u64> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u64>().ok()
}

fn requires_wimlib() -> Result<()> {
    if command_exists("wimlib-imagex") {
        return Ok(());
    }
    Err(AppError::ImageError(
        "Missing dependency: wimlib-imagex. Install with: brew install wimlib".to_string(),
    ))
}

fn requires_ntfs_tooling() -> Result<()> {
    if !command_exists("ntfs-3g") {
        return Err(AppError::DiskError(
            "Missing dependency: ntfs-3g. Install with: brew tap gromgit/homebrew-fuse && brew install ntfs-3g-mac"
                .to_string(),
        ));
    }
    if !command_exists("mkntfs") && !command_exists("mkfs.ntfs") {
        return Err(AppError::DiskError(
            "Missing dependency: mkntfs (from ntfs-3g). Install with: brew tap gromgit/homebrew-fuse && brew install ntfs-3g-mac"
                .to_string(),
        ));
    }
    Ok(())
}

fn normalize_hdiutil_detach_target(dev_entry: &str) -> Option<String> {
    let entry = dev_entry.trim();
    if !entry.starts_with("/dev/disk") {
        return None;
    }
    let suffix = &entry["/dev/disk".len()..];
    let digits: String = suffix.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("/dev/disk{}", digits))
}

fn hdiutil_attach_iso(iso_path: &Path) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let output = Command::new("hdiutil")
        .args([
            "attach",
            "-readonly",
            "-nobrowse",
            "-plist",
            iso_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(AppError::io)?;
    if !output.status.success() {
        let err = to_text(&output.stderr);
        let out = to_text(&output.stdout);
        let detail = if err.is_empty() { out } else { err };
        return Err(AppError::ImageError(format!(
            "Failed to mount ISO via hdiutil: {}",
            detail
        )));
    }

    let value = plist_to_json(&output.stdout)?;
    let entities = value
        .get("system-entities")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::ImageError("Invalid hdiutil attach output".to_string()))?;

    let mut mount_points: Vec<PathBuf> = Vec::new();
    let mut detach_set: HashSet<String> = HashSet::new();
    let mut detach_targets: Vec<String> = Vec::new();

    for entity in entities {
        let mount_point = entity
            .get("mount-point")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !mount_point.is_empty() {
            mount_points.push(PathBuf::from(mount_point));
        }

        let dev_entry = entity
            .get("dev-entry")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if let Some(target) = normalize_hdiutil_detach_target(dev_entry) {
            if detach_set.insert(target.clone()) {
                detach_targets.push(target);
            }
        }
    }

    if mount_points.is_empty() {
        return Err(AppError::ImageError(
            "ISO mounted but no mount point was found".to_string(),
        ));
    }
    if detach_targets.is_empty() {
        return Err(AppError::ImageError(
            "ISO mounted but no detach target was returned".to_string(),
        ));
    }

    Ok((mount_points, detach_targets))
}

fn hdiutil_detach_force(target: &str) {
    let _ = Command::new("hdiutil")
        .args(["detach", "-force", target])
        .status();
}

struct MountedIso {
    detach_targets: Vec<String>,
}

impl Drop for MountedIso {
    fn drop(&mut self) {
        // Detach in reverse order to release child mappings before parent device.
        for target in self.detach_targets.iter().rev() {
            hdiutil_detach_force(target);
        }
    }
}

struct ResolvedApplyImage {
    image_path: PathBuf,
    _mounted_iso: Option<MountedIso>,
}

fn resolve_apply_image(image_path: &Path) -> Result<ResolvedApplyImage> {
    let ext = image_path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext != "iso" {
        return Ok(ResolvedApplyImage {
            image_path: image_path.to_path_buf(),
            _mounted_iso: None,
        });
    }

    let (mount_points, detach_targets) = hdiutil_attach_iso(image_path)?;
    let mounted = MountedIso { detach_targets };
    let install_image = mount_points
        .iter()
        .find_map(|mount| find_install_image_in_mount(mount))
        .ok_or_else(|| {
            AppError::ImageError(
                "Cannot find sources/install.wim or sources/install.esd in ISO".to_string(),
            )
        })?;

    Ok(ResolvedApplyImage {
        image_path: install_image,
        _mounted_iso: Some(mounted),
    })
}

fn resolve_wim_index(image_path: &Path, requested: &str) -> Result<String> {
    let infos = get_wimlib_image_info(image_path)?;
    if infos.is_empty() {
        return Err(AppError::ImageError(format!(
            "No image index found in {}",
            image_path.display()
        )));
    }

    let req = requested.trim();
    if !req.is_empty() && req != "0" {
        let parsed = req.parse::<u32>().map_err(|_| {
            AppError::InvalidParameter(format!("Invalid WIM index: {}", requested))
        })?;
        if infos.iter().any(|i| i.index == parsed) {
            return Ok(parsed.to_string());
        }
        return Err(AppError::InvalidParameter(format!(
            "WIM index {} not found in image",
            parsed
        )));
    }

    Ok(infos[0].index.to_string())
}

fn find_install_image_in_mount(mount_point: &Path) -> Option<PathBuf> {
    let sources = ["sources", "Sources", "SOURCES"];
    let files = ["install.wim", "install.esd", "INSTALL.WIM", "INSTALL.ESD"];

    for src in sources {
        for file in files {
            let candidate = mount_point.join(src).join(file);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    let sources_dir = fs::read_dir(mount_point).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if !file_type.is_dir() {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("sources") {
            Some(entry.path())
        } else {
            None
        }
    })?;

    fs::read_dir(sources_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if !file_type.is_file() {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("install.wim") || name.eq_ignore_ascii_case("install.esd") {
            Some(entry.path())
        } else {
            None
        }
    })
}

fn get_image_info_from_iso(iso_path: &Path) -> Result<Vec<ImageInfo>> {
    let (mount_points, detach_targets) = hdiutil_attach_iso(iso_path)?;
    let _mounted = MountedIso { detach_targets };

    let image_in_iso = mount_points
        .iter()
        .find_map(|mount| find_install_image_in_mount(mount))
        .ok_or_else(|| {
            AppError::ImageError(
                "Cannot find sources/install.wim or install.esd in ISO".to_string(),
            )
        })?;

    let info = get_wimlib_image_info(&image_in_iso)?;
    if info.is_empty() {
        return Err(AppError::ImageError(
            "No image index found in ISO install image".to_string(),
        ));
    }
    Ok(info)
}

fn parse_wimlib_image_info(raw: &str) -> Vec<ImageInfo> {
    let mut results = Vec::new();
    let mut current: Option<ImageInfo> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("Index:") {
            if let Some(item) = current.take() {
                results.push(item);
            }
            let index = v.trim().parse::<u32>().unwrap_or(0);
            current = Some(ImageInfo {
                index,
                name: format!("Image {}", index),
                description: String::new(),
                size: 0,
            });
            continue;
        }

        let Some(item) = current.as_mut() else {
            continue;
        };

        if let Some(v) = trimmed.strip_prefix("Name:") {
            let name = v.trim();
            if !name.is_empty() {
                item.name = name.to_string();
            }
            continue;
        }

        if let Some(v) = trimmed.strip_prefix("Description:") {
            let desc = v.trim();
            if !desc.is_empty() {
                item.description = desc.to_string();
            }
            continue;
        }

        if let Some(v) = trimmed.strip_prefix("Total Bytes:") {
            if let Some(bytes) = parse_u64_digits(v.trim()) {
                item.size = bytes;
            }
            continue;
        }
    }

    if let Some(item) = current {
        results.push(item);
    }

    results.retain(|i| i.index > 0);
    results
}

fn get_wimlib_image_info(image_path: &Path) -> Result<Vec<ImageInfo>> {
    let output = Command::new("wimlib-imagex")
        .args(["info", image_path.to_string_lossy().as_ref()])
        .output()
        .map_err(AppError::io)?;
    if !output.status.success() {
        let err = to_text(&output.stderr);
        let out = to_text(&output.stdout);
        let detail = if err.is_empty() { out } else { err };
        return Err(AppError::ImageError(format!(
            "wimlib-imagex info failed: {}",
            detail
        )));
    }

    Ok(parse_wimlib_image_info(&to_text(&output.stdout)))
}

fn shell_escape_single_quotes(raw: &str) -> String {
    raw.replace('\'', "'\"'\"'")
}

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

fn diskutil_json(args: &[&str]) -> Result<Value> {
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

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn parse_disk_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let without_dev = trimmed.strip_prefix("/dev/").unwrap_or(trimmed);
    if !without_dev.starts_with("disk") {
        return None;
    }
    let suffix = &without_dev[4..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("disk{}", suffix))
}

fn parse_partition_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let without_dev = trimmed.strip_prefix("/dev/").unwrap_or(trimmed);
    if !without_dev.starts_with("disk") {
        return None;
    }

    let split_at = without_dev.rfind('s')?;
    if split_at <= 4 {
        return None;
    }
    let disk_suffix = &without_dev[4..split_at];
    let part_suffix = &without_dev[split_at + 1..];
    if disk_suffix.is_empty()
        || part_suffix.is_empty()
        || !disk_suffix.chars().all(|c| c.is_ascii_digit())
        || !part_suffix.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    Some(format!("disk{}s{}", disk_suffix, part_suffix))
}

fn resolve_disk_id(config: &WtgConfig) -> Result<String> {
    resolve_disk_id_from_target_disk(&config.target_disk)
}

fn resolve_disk_id_from_target_disk(target_disk: &Disk) -> Result<String> {
    if let Some(v) = parse_disk_id(&target_disk.device) {
        return Ok(v);
    }
    if let Some(v) = parse_disk_id(&target_disk.id) {
        return Ok(v);
    }
    if !target_disk.index.trim().is_empty() {
        let index = target_disk.index.trim();
        if index.chars().all(|c| c.is_ascii_digit()) {
            return Ok(format!("disk{}", index));
        }
    }
    Err(AppError::InvalidParameter(format!(
        "Invalid target disk identifier: id='{}', device='{}', index='{}'",
        target_disk.id, target_disk.device, target_disk.index
    )))
}

fn find_partition_for_disk(disk_id: &str) -> Result<Option<(String, String)>> {
    let list_json = diskutil_json(&["list", "-plist"])?;
    let Some(disks) = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
    else {
        return Ok(None);
    };

    let disk = disks
        .iter()
        .find(|d| json_str(d, "DeviceIdentifier") == disk_id);
    let Some(disk) = disk else {
        return Ok(None);
    };

    let Some(parts) = disk.get("Partitions").and_then(Value::as_array) else {
        return Ok(None);
    };

    let mut mounted_candidates: Vec<(String, String)> = Vec::new();
    let mut ntfs_candidate: Option<(String, String)> = None;

    for p in parts {
        let part_id = json_str(p, "DeviceIdentifier");
        let mount = json_str(p, "MountPoint");
        if !part_id.is_empty() && !mount.is_empty() {
            mounted_candidates.push((part_id, mount));
        }
    }

    for (part_id, mount) in &mounted_candidates {
        let mount_path = PathBuf::from(mount);
        let part_info = match get_partition_info_json(part_id) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let filesystem = json_str(&part_info, "FilesystemType").to_ascii_lowercase();
        let writable = json_bool(&part_info, "WritableVolume") && is_dir_writable(&mount_path);

        if writable {
            return Ok(Some((part_id.clone(), mount.clone())));
        }
        if ntfs_candidate.is_none() && filesystem == "ntfs" {
            ntfs_candidate = Some((part_id.clone(), mount.clone()));
        }
    }

    if ntfs_candidate.is_some() {
        return Ok(ntfs_candidate);
    }

    Ok(mounted_candidates.into_iter().next())
}

fn ensure_disk_mounted(disk_id: &str) -> Result<()> {
    let status = Command::new("diskutil")
        .args(["mountDisk", &format!("/dev/{}", disk_id)])
        .status()
        .map_err(AppError::io)?;
    if !status.success() {
        warn!("diskutil mountDisk /dev/{} returned non-zero", disk_id);
    }
    Ok(())
}

fn get_partition_info_json(partition_id: &str) -> Result<Value> {
    diskutil_json(&["info", "-plist", &format!("/dev/{}", partition_id)])
}

fn is_dir_writable(path: &Path) -> bool {
    let probe = path.join(".wtga_write_probe");
    match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&probe)
    {
        Ok(mut file) => {
            let _ = file.write_all(b"wtga");
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn find_ntfs_mount_script() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("useable_software/ntfs-mount.sh"));
        candidates.push(cwd.join("../useable_software/ntfs-mount.sh"));
    }
    candidates
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../useable_software/ntfs-mount.sh"));

    candidates.into_iter().find(|p| p.exists())
}

fn stage_ntfs_mount_script_for_privileged_exec() -> Result<PathBuf> {
    let source = find_ntfs_mount_script().ok_or_else(|| {
        AppError::SystemError("Cannot find useable_software/ntfs-mount.sh".to_string())
    })?;
    let staged = PathBuf::from("/tmp").join(format!("wtga-ntfs-mount-{}.sh", std::process::id()));
    fs::copy(&source, &staged).map_err(AppError::io)?;
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&staged, fs::Permissions::from_mode(0o755));
    }
    Ok(staged)
}

fn run_ntfs_mount_script_as_admin(force: bool) -> Result<()> {
    let script = stage_ntfs_mount_script_for_privileged_exec()?;
    let escaped = shell_escape_single_quotes(script.to_string_lossy().as_ref());
    let runner = if force {
        format!("WTGA_NTFS_FORCE=1 /bin/bash '{}'", escaped)
    } else {
        format!("/bin/bash '{}'", escaped)
    };

    let command = format!(
        "cd /tmp || true; export PATH='/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH'; {}; EXIT_CODE=$?; /bin/rm -f '{}'; exit $EXIT_CODE",
        runner, escaped
    );
    let result = macos_admin::run_shell_with_auto_privilege(&command);
    let _ = fs::remove_file(&script);
    result
}

#[derive(Debug, Clone, Serialize)]
pub struct MacosTargetWritableCheck {
    pub supported: bool,
    pub disk_id: String,
    pub partition_id: Option<String>,
    pub mount_point: Option<String>,
    pub filesystem: String,
    pub writable_volume: bool,
    pub dir_writable: bool,
    pub writable: bool,
    pub needs_ntfs_remount: bool,
    pub reason: Option<String>,
}

fn check_target_writable_by_disk_id(disk_id: &str) -> Result<MacosTargetWritableCheck> {
    ensure_disk_mounted(disk_id)?;
    let mounted = find_partition_for_disk(disk_id)?;
    let Some((partition_id, mount_point)) = mounted else {
        return Ok(MacosTargetWritableCheck {
            supported: true,
            disk_id: disk_id.to_string(),
            partition_id: None,
            mount_point: None,
            filesystem: String::new(),
            writable_volume: false,
            dir_writable: false,
            writable: false,
            needs_ntfs_remount: false,
            reason: Some("No mounted partition found on target disk".to_string()),
        });
    };

    let mount_path = PathBuf::from(&mount_point);
    let part_info = get_partition_info_json(&partition_id)?;
    let filesystem = json_str(&part_info, "FilesystemType").to_ascii_lowercase();
    let writable_volume = json_bool(&part_info, "WritableVolume");
    let dir_writable = is_dir_writable(&mount_path);
    let writable = writable_volume && dir_writable;
    let needs_ntfs_remount = !writable && filesystem == "ntfs";
    let reason = if writable {
        None
    } else if needs_ntfs_remount {
        Some("Target NTFS volume is mounted read-only".to_string())
    } else {
        Some(format!(
            "Target volume is not writable (filesystem: {})",
            if filesystem.is_empty() {
                "unknown".to_string()
            } else {
                filesystem.clone()
            }
        ))
    };

    Ok(MacosTargetWritableCheck {
        supported: true,
        disk_id: disk_id.to_string(),
        partition_id: Some(partition_id),
        mount_point: Some(mount_point),
        filesystem,
        writable_volume,
        dir_writable,
        writable,
        needs_ntfs_remount,
        reason,
    })
}

pub fn check_target_writable(target_disk: &Disk) -> Result<MacosTargetWritableCheck> {
    let disk_id = resolve_disk_id_from_target_disk(target_disk)?;
    check_target_writable_by_disk_id(&disk_id)
}

pub fn remount_target_ntfs_writable(target_disk: &Disk) -> Result<MacosTargetWritableCheck> {
    let check_before = check_target_writable(target_disk)?;
    if check_before.writable {
        return Ok(check_before);
    }
    if !check_before.needs_ntfs_remount {
        return Err(AppError::DiskError(
            check_before
                .reason
                .unwrap_or_else(|| "Target volume does not require NTFS remount".to_string()),
        ));
    }
    if !command_exists("ntfs-3g") {
        return Err(AppError::DiskError(
            "NTFS target detected but ntfs-3g is missing. Install with: brew install --cask macfuse && brew tap gromgit/homebrew-fuse && brew install ntfs-3g-mac".to_string(),
        ));
    }

    run_ntfs_mount_script_as_admin(true)?;

    let check_after = check_target_writable_by_disk_id(&check_before.disk_id)?;
    if !check_after.writable {
        return Err(AppError::DiskError(format!(
            "NTFS remount finished but target volume is still not writable: {}",
            check_after
                .mount_point
                .unwrap_or_else(|| check_after.disk_id.clone())
        )));
    }
    Ok(check_after)
}

#[derive(Debug, Clone)]
struct PreparedTargetDisk {
    efi_partition_id: Option<String>,
    system_partition_id: String,
}

fn expected_partition_ids_for_boot_mode(mode: &BootMode, disk_id: &str) -> PreparedTargetDisk {
    match mode {
        BootMode::UefiGpt | BootMode::UefiMbr => PreparedTargetDisk {
            efi_partition_id: Some(format!("{}s1", disk_id)),
            system_partition_id: format!("{}s2", disk_id),
        },
        BootMode::NonUefi => PreparedTargetDisk {
            efi_partition_id: None,
            system_partition_id: format!("{}s1", disk_id),
        },
    }
}

fn partition_index_from_id(partition_id: &str) -> u32 {
    let normalized = partition_id
        .trim()
        .strip_prefix("/dev/")
        .unwrap_or(partition_id.trim());
    let Some(split_at) = normalized.rfind('s') else {
        return u32::MAX;
    };
    normalized[split_at + 1..].parse::<u32>().unwrap_or(u32::MAX)
}

fn resolve_prepared_partitions(disk_id: &str, mode: &BootMode) -> Result<PreparedTargetDisk> {
    let list_json = diskutil_json(&["list", "-plist"])?;
    let Some(disks) = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
    else {
        return Err(AppError::DiskError(
            "Invalid diskutil list output while resolving target partitions".to_string(),
        ));
    };

    let disk = disks
        .iter()
        .find(|d| json_str(d, "DeviceIdentifier") == disk_id)
        .ok_or_else(|| {
            AppError::DiskError(format!(
                "Cannot find target disk /dev/{} after repartition",
                disk_id
            ))
        })?;

    let mut parts: Vec<Value> = disk
        .get("Partitions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if parts.is_empty() {
        return Err(AppError::DiskError(format!(
            "No partitions found on /dev/{} after repartition",
            disk_id
        )));
    }
    parts.sort_by_key(|p| partition_index_from_id(&json_str(p, "DeviceIdentifier")));

    let mut efi_named_partition_id: Option<String> = None;
    let mut efi_content_partition_id: Option<String> = None;
    let mut wtga_partition_id: Option<String> = None;
    let mut largest_non_efi: Option<(String, u64)> = None;

    for p in &parts {
        let part_id = json_str(p, "DeviceIdentifier");
        if part_id.is_empty() {
            continue;
        }
        let content = json_str(p, "Content").to_ascii_lowercase();
        let volume_name = json_str(p, "VolumeName").to_ascii_lowercase();
        let size = json_u64(p, "Size").unwrap_or(0);
        let is_efi_named = volume_name == "efi";
        let is_efi_content = content.contains("efi");
        let is_efi = is_efi_named || is_efi_content;

        if is_efi_named && efi_named_partition_id.is_none() {
            efi_named_partition_id = Some(part_id.clone());
        }
        if is_efi_content && efi_content_partition_id.is_none() {
            efi_content_partition_id = Some(part_id.clone());
        }
        if volume_name == "wtga" && wtga_partition_id.is_none() {
            wtga_partition_id = Some(part_id.clone());
        }

        if !is_efi {
            let should_replace = largest_non_efi
                .as_ref()
                .map(|(_, old_size)| size >= *old_size)
                .unwrap_or(true);
            if should_replace {
                largest_non_efi = Some((part_id.clone(), size));
            }
        }
    }

    let efi_partition_id = efi_named_partition_id.or(efi_content_partition_id);
    let mut system_partition_id = wtga_partition_id.or_else(|| largest_non_efi.map(|v| v.0));

    if system_partition_id.is_none() {
        system_partition_id = parts
            .iter()
            .rev()
            .map(|p| json_str(p, "DeviceIdentifier"))
            .find(|id| {
                if id.is_empty() {
                    return false;
                }
                if let Some(efi_id) = efi_partition_id.as_ref() {
                    return id != efi_id;
                }
                true
            });
    }

    let system_partition_id = system_partition_id.ok_or_else(|| {
        AppError::DiskError(format!(
            "Cannot resolve system partition on /dev/{} after repartition",
            disk_id
        ))
    })?;

    match mode {
        BootMode::UefiGpt | BootMode::UefiMbr => {
            if efi_partition_id.is_none() {
                return Err(AppError::DiskError(format!(
                    "Cannot resolve EFI partition on /dev/{} after repartition",
                    disk_id
                )));
            }
            Ok(PreparedTargetDisk {
                efi_partition_id,
                system_partition_id,
            })
        }
        BootMode::NonUefi => Ok(PreparedTargetDisk {
            efi_partition_id: None,
            system_partition_id,
        }),
    }
}

fn list_partition_ids_for_disk(disk_id: &str) -> Result<Vec<String>> {
    let list_json = diskutil_json(&["list", "-plist"])?;
    let Some(disks) = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };
    let disk = disks
        .iter()
        .find(|d| json_str(d, "DeviceIdentifier") == disk_id);
    let Some(disk) = disk else {
        return Ok(Vec::new());
    };
    let Some(parts) = disk.get("Partitions").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut ids: Vec<String> = parts
        .iter()
        .map(|p| json_str(p, "DeviceIdentifier"))
        .filter(|id| !id.is_empty())
        .collect();
    ids.sort_by_key(|id| partition_index_from_id(id));
    Ok(ids)
}

fn try_mount_alternative_efi_partition(
    disk_id: &str,
    system_partition_id: &str,
    failed_efi_id: &str,
) -> Result<Option<(String, PathBuf)>> {
    let ids = list_partition_ids_for_disk(disk_id)?;
    for id in ids {
        if id == system_partition_id || id == failed_efi_id {
            continue;
        }
        let info = match get_partition_info_json(&id) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let fs = json_str(&info, "FilesystemType").to_ascii_lowercase();
        let volume_name = json_str(&info, "VolumeName").to_ascii_lowercase();
        let content = json_str(&info, "Content").to_ascii_lowercase();
        let efi_like = volume_name == "efi"
            || content.contains("efi")
            || fs.contains("fat")
            || fs.contains("msdos");
        if !efi_like {
            continue;
        }
        if let Ok(mount) = mount_partition_and_get_mount_point(&id) {
            return Ok(Some((id, mount)));
        }
    }
    Ok(None)
}

fn pick_ntfs_mkfs_command() -> Option<&'static str> {
    if command_exists("mkntfs") {
        Some("mkntfs")
    } else if command_exists("mkfs.ntfs") {
        Some("mkfs.ntfs")
    } else {
        None
    }
}

fn parent_disk_id_from_partition(partition_id: &str) -> Option<String> {
    let normalized = partition_id
        .trim()
        .strip_prefix("/dev/")
        .unwrap_or(partition_id.trim());
    if !normalized.starts_with("disk") {
        return None;
    }

    let split_at = normalized.rfind('s')?;
    if split_at <= 4 || split_at + 1 >= normalized.len() {
        return None;
    }

    let disk = &normalized[..split_at];
    let disk_idx = &disk[4..];
    let part_idx = &normalized[split_at + 1..];
    if disk_idx.is_empty() || part_idx.is_empty() {
        return None;
    }
    if !disk_idx.chars().all(|c| c.is_ascii_digit())
        || !part_idx.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    Some(disk.to_string())
}

fn format_partition_ntfs(partition_id: &str, label: &str) -> Result<()> {
    let mkfs = pick_ntfs_mkfs_command().ok_or_else(|| {
        AppError::DiskError(
            "Cannot find mkntfs/mkfs.ntfs. Install ntfs-3g-mac first.".to_string(),
        )
    })?;
    let dev_path = format!("/dev/{}", partition_id);
    let raw_dev_path = format!("/dev/r{}", partition_id);
    let parent_disk_id = parent_disk_id_from_partition(partition_id);
    let part_info = get_partition_info_json(partition_id)?;
    let part_offset = json_u64(&part_info, "PartitionMapPartitionOffset").unwrap_or(0);
    let block_size = json_u64(&part_info, "DeviceBlockSize").unwrap_or(512);
    let start_sector = if part_offset > 0 && block_size > 0 {
        Some(part_offset / block_size)
    } else {
        None
    };
    let escaped_label = shell_escape_single_quotes(label);
    let escaped_dev = shell_escape_single_quotes(&dev_path);
    let escaped_raw = shell_escape_single_quotes(&raw_dev_path);
    let escaped_mkfs = shell_escape_single_quotes(mkfs);
    let escaped_parent = shell_escape_single_quotes(parent_disk_id.as_deref().unwrap_or(""));
    let escaped_start = shell_escape_single_quotes(
        &start_sector
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string()),
    );
    let command = format!(
        "DEV='{dev}'; RAW='{raw}'; LABEL='{label}'; MKFS='{mkfs}'; PARENT='{parent}'; START='{start}'; \
         [ -n \"$PARENT\" ] && diskutil unmountDisk force \"/dev/$PARENT\" >/dev/null 2>&1 || true; \
         diskutil unmount force \"$DEV\" >/dev/null 2>&1 || true; \
         if [ \"$START\" != \"0\" ]; then START_OPT=\"-p $START\"; else START_OPT=\"\"; fi; \
         \"$MKFS\" -Q -F -L \"$LABEL\" $START_OPT \"$RAW\" >/tmp/wtga-mkntfs.log 2>&1; EXIT_CODE=$?; \
         if [ $EXIT_CODE -ne 0 ]; then \
             \"$MKFS\" -Q -F -L \"$LABEL\" $START_OPT \"$DEV\" >>/tmp/wtga-mkntfs.log 2>&1; \
             EXIT_CODE=$?; \
         fi; \
         /bin/cat /tmp/wtga-mkntfs.log 2>/dev/null || true; \
         /bin/rm -f /tmp/wtga-mkntfs.log; \
         if [ $EXIT_CODE -ne 0 ]; then exit $EXIT_CODE; fi; \
         diskutil mount \"$DEV\" >/dev/null 2>&1 || true; exit 0",
        dev = escaped_dev,
        mkfs = escaped_mkfs,
        label = escaped_label,
        raw = escaped_raw,
        parent = escaped_parent,
        start = escaped_start
    );
    macos_admin::run_shell_with_auto_privilege(&command)
}

fn mount_partition_and_get_mount_point(partition_id: &str) -> Result<PathBuf> {
    let dev_path = format!("/dev/{}", partition_id);
    let escaped_dev = shell_escape_single_quotes(&dev_path);
    let command = format!("diskutil mount '{dev}' >/dev/null 2>&1 || true", dev = escaped_dev);
    macos_admin::run_shell_with_auto_privilege(&command)?;
    let info = get_partition_info_json(partition_id)?;
    let mount_point = json_str(&info, "MountPoint");
    if mount_point.is_empty() {
        return Err(AppError::DiskError(format!(
            "Partition {} is not mounted",
            partition_id
        )));
    }
    Ok(PathBuf::from(mount_point))
}

fn mount_ntfs_partition_writable(partition_id: &str, label: &str) -> Result<PathBuf> {
    let dev_path = format!("/dev/{}", partition_id);
    let raw_dev_path = format!("/dev/r{}", partition_id);
    let mount_point = format!("/Volumes/WTGA-{}", partition_id);
    let escaped_dev = shell_escape_single_quotes(&dev_path);
    let escaped_raw = shell_escape_single_quotes(&raw_dev_path);
    let escaped_mnt = shell_escape_single_quotes(&mount_point);
    let escaped_label = shell_escape_single_quotes(label);
    let command = format!(
        "DEV='{dev}'; RAW='{raw}'; MNT='{mnt}'; LABEL='{label}'; \
         diskutil unmount force \"$DEV\" >/dev/null 2>&1 || true; \
         /bin/mkdir -p \"$MNT\"; \
         ntfs-3g \"$RAW\" \"$MNT\" -o rw -o big_writes -o noatime -o noappledouble -o local -o volname=\"$LABEL\" -o nonempty >/tmp/wtga-ntfs3g.log 2>&1; EXIT_CODE=$?; \
         if [ $EXIT_CODE -ne 0 ]; then \
            ntfs-3g \"$DEV\" \"$MNT\" -o rw -o big_writes -o noatime -o noappledouble -o local -o volname=\"$LABEL\" -o nonempty >>/tmp/wtga-ntfs3g.log 2>&1; \
            EXIT_CODE=$?; \
         fi; \
         if [ $EXIT_CODE -ne 0 ]; then \
            ntfs-3g \"$DEV\" \"$MNT\" -o rw -o nonempty >>/tmp/wtga-ntfs3g.log 2>&1; \
            EXIT_CODE=$?; \
         fi; \
         /bin/cat /tmp/wtga-ntfs3g.log 2>/dev/null || true; \
         /bin/rm -f /tmp/wtga-ntfs3g.log; \
         exit $EXIT_CODE",
        dev = escaped_dev,
        raw = escaped_raw,
        mnt = escaped_mnt,
        label = escaped_label
    );
    macos_admin::run_shell_with_auto_privilege(&command)?;
    let mount_path = PathBuf::from(mount_point);
    if !mount_path.exists() || !is_dir_writable(&mount_path) {
        return Err(AppError::DiskError(format!(
            "NTFS partition {} mounted but is still not writable",
            partition_id
        )));
    }
    Ok(mount_path)
}

fn apply_windows_image(source_image: &Path, wim_index: &str, target_mount: &Path) -> Result<()> {
    let escaped_source = shell_escape_single_quotes(source_image.to_string_lossy().as_ref());
    let escaped_target = shell_escape_single_quotes(target_mount.to_string_lossy().as_ref());
    let escaped_index = shell_escape_single_quotes(wim_index);
    let command = format!(
        "wimlib-imagex apply '{source}' '{index}' '{target}'",
        source = escaped_source,
        index = escaped_index,
        target = escaped_target
    );
    macos_admin::run_shell_with_auto_privilege(&command)
}

fn stage_uefi_boot_payload(system_mount: &Path, efi_mount: &Path) -> Result<()> {
    let escaped_system = shell_escape_single_quotes(system_mount.to_string_lossy().as_ref());
    let escaped_efi = shell_escape_single_quotes(efi_mount.to_string_lossy().as_ref());
    let command = format!(
        "SYS='{sys}'; EFI='{efi}'; SRC=\"$SYS/Windows/Boot/EFI\"; DST=\"$EFI/EFI/Microsoft/Boot\"; FB=\"$EFI/EFI/Boot\"; if [ ! -d \"$SRC\" ]; then echo 'Windows/Boot/EFI directory not found in applied system'; exit 2; fi; /bin/mkdir -p \"$DST\" \"$FB\"; /bin/cp -R \"$SRC/.\" \"$DST/\"; if [ ! -f \"$DST/bootmgfw.efi\" ]; then echo 'bootmgfw.efi not found after payload copy'; exit 3; fi; /bin/cp -f \"$DST/bootmgfw.efi\" \"$FB/bootx64.efi\"; exit 0",
        sys = escaped_system,
        efi = escaped_efi
    );
    macos_admin::run_shell_with_auto_privilege(&command)
}

fn repair_uefi_bcd_store(system_mount: &Path, efi_mount: &Path) -> Result<()> {
    let escaped_system = shell_escape_single_quotes(system_mount.to_string_lossy().as_ref());
    let escaped_efi = shell_escape_single_quotes(efi_mount.to_string_lossy().as_ref());
    let command = format!(
        "SYS='{sys}'; EFI='{efi}'; DST=\"$EFI/EFI/Microsoft/Boot\"; FB=\"$EFI/EFI/Boot\"; BCD=\"$DST/BCD\"; C1=\"$SYS/Windows/Boot/EFI/BCD\"; C2=\"$SYS/Boot/BCD\"; C3=\"$SYS/Windows/System32/config/BCD-Template\"; /bin/mkdir -p \"$DST\" \"$FB\"; if [ ! -f \"$BCD\" ]; then if [ -f \"$C1\" ]; then /bin/cp -f \"$C1\" \"$BCD\"; elif [ -f \"$C2\" ]; then /bin/cp -f \"$C2\" \"$BCD\"; elif [ -f \"$C3\" ]; then /bin/cp -f \"$C3\" \"$BCD\"; fi; fi; if [ ! -f \"$BCD\" ]; then echo 'No valid BCD source found'; exit 4; fi; HDR=$(/usr/bin/hexdump -n 4 -ve '1/1 \"%02x\"' \"$BCD\" 2>/dev/null || true); if [ \"$HDR\" != \"72656766\" ]; then echo \"Invalid BCD hive header: $HDR\"; exit 5; fi; /bin/cp -f \"$BCD\" \"$DST/BCD.wtga.bak\"; /bin/cp -f \"$BCD\" \"$FB/BCD\"; exit 0",
        sys = escaped_system,
        efi = escaped_efi
    );
    macos_admin::run_shell_with_auto_privilege(&command)
}

fn verify_uefi_boot_files(efi_mount: &Path) -> Result<()> {
    let bcd = efi_mount.join("EFI").join("Microsoft").join("Boot").join("BCD");
    let bootmgfw = efi_mount
        .join("EFI")
        .join("Microsoft")
        .join("Boot")
        .join("bootmgfw.efi");
    let bootx64 = efi_mount.join("EFI").join("Boot").join("bootx64.efi");

    for path in [&bcd, &bootmgfw, &bootx64] {
        if !path.exists() {
            return Err(AppError::DiskError(format!(
                "UEFI boot artifact missing: {}",
                path.display()
            )));
        }
    }

    let header = Command::new("sh")
        .args([
            "-lc",
            &format!(
                "/usr/bin/hexdump -n 4 -ve '1/1 \"%02x\"' '{}'",
                shell_escape_single_quotes(bcd.to_string_lossy().as_ref())
            ),
        ])
        .output()
        .map_err(AppError::io)?;
    if !header.status.success() {
        return Err(AppError::DiskError("Failed to inspect BCD file header".to_string()));
    }
    let bcd_header = to_text(&header.stdout);
    if bcd_header != "72656766" {
        return Err(AppError::DiskError(format!(
            "Invalid BCD file header: {}",
            bcd_header
        )));
    }

    Ok(())
}

fn list_partitions_for_disk(disk_id: &str) -> Result<Vec<Value>> {
    let list_json = diskutil_json(&["list", "-plist"])?;
    let Some(disks) = list_json
        .get("AllDisksAndPartitions")
        .and_then(Value::as_array)
    else {
        return Err(AppError::DiskError(
            "Invalid diskutil list output while resolving partitions".to_string(),
        ));
    };

    let Some(disk) = disks
        .iter()
        .find(|d| json_str(d, "DeviceIdentifier") == disk_id)
    else {
        return Err(AppError::DeviceNotFound(format!(
            "Target disk not found: {}",
            disk_id
        )));
    };

    let mut parts: Vec<Value> = disk
        .get("Partitions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    parts.sort_by_key(|p| {
        let pid = json_str(p, "DeviceIdentifier");
        partition_index_from_id(&pid)
    });

    Ok(parts)
}

fn partition_looks_efi(part: &Value, info: &Value) -> bool {
    let content = json_str(part, "Content").to_ascii_uppercase();
    if content.contains("EFI") {
        return true;
    }

    let volume = json_str(part, "VolumeName").to_ascii_uppercase();
    if volume == "EFI" {
        return true;
    }

    let fs = json_str(info, "FilesystemType").to_ascii_lowercase();
    if fs.contains("ms-dos") || fs.contains("fat") || fs.contains("efi") {
        let size = json_u64(info, "TotalSize")
            .or_else(|| json_u64(info, "Size"))
            .unwrap_or(0);
        if size > 0 && size <= 2 * 1024 * 1024 * 1024 {
            return true;
        }
    }

    false
}

fn resolve_system_partition_id_from_hint(target_hint: &str) -> Result<String> {
    if let Some(pid) = parse_partition_id(target_hint) {
        return Ok(pid);
    }

    let trimmed = target_hint.trim();
    if !trimmed.is_empty() && (trimmed.starts_with('/') || trimmed.starts_with("Volume")) {
        if let Ok(info) = diskutil_json(&["info", "-plist", trimmed]) {
            let device_identifier = json_str(&info, "DeviceIdentifier");
            if let Some(pid) = parse_partition_id(&device_identifier) {
                return Ok(pid);
            }
            if let Some(disk_id) = parse_disk_id(&device_identifier) {
                let parts = list_partitions_for_disk(&disk_id)?;
                if let Some(first_data) = parts
                    .iter()
                    .find(|p| parse_partition_id(&json_str(p, "DeviceIdentifier")).is_some())
                {
                    return Ok(json_str(first_data, "DeviceIdentifier"));
                }
            }
        }
    }

    if let Some(disk_id) = parse_disk_id(target_hint) {
        let parts = list_partitions_for_disk(&disk_id)?;
        for part in &parts {
            let pid = json_str(part, "DeviceIdentifier");
            let mount_point = json_str(part, "MountPoint");
            if pid.is_empty() || mount_point.is_empty() {
                continue;
            }
            if Path::new(&mount_point).join("Windows").exists() {
                return Ok(pid);
            }
        }

        if let Some(first_data) = parts.iter().find(|p| {
            let pid = json_str(p, "DeviceIdentifier");
            let Ok(info) = get_partition_info_json(&pid) else {
                return false;
            };
            !partition_looks_efi(p, &info)
        }) {
            return Ok(json_str(first_data, "DeviceIdentifier"));
        }
    }

    Err(AppError::InvalidParameter(format!(
        "Cannot resolve system partition from target '{}'",
        target_hint
    )))
}

fn resolve_efi_partition_id_for_system(system_partition_id: &str) -> Result<String> {
    let part_info = get_partition_info_json(system_partition_id)?;
    let parent_disk = json_str(&part_info, "ParentWholeDisk");
    if parent_disk.is_empty() {
        return Err(AppError::DiskError(format!(
            "Cannot determine parent disk for partition {}",
            system_partition_id
        )));
    }

    let parts = list_partitions_for_disk(&parent_disk)?;
    for part in &parts {
        let pid = json_str(part, "DeviceIdentifier");
        if pid.is_empty() || pid == system_partition_id {
            continue;
        }
        let info = match get_partition_info_json(&pid) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if partition_looks_efi(part, &info) {
            return Ok(pid);
        }
    }

    let fallback = format!("{}s1", parent_disk);
    if fallback != system_partition_id && get_partition_info_json(&fallback).is_ok() {
        return Ok(fallback);
    }

    Err(AppError::DiskError(format!(
        "Cannot find EFI partition on {}",
        parent_disk
    )))
}

pub fn repair_boot_uefi_for_target(target_hint: &str) -> Result<String> {
    let system_partition_id = resolve_system_partition_id_from_hint(target_hint)?;
    let efi_partition_id = resolve_efi_partition_id_for_system(&system_partition_id)?;

    let system_mount = mount_partition_and_get_mount_point(&system_partition_id)?;
    let efi_mount = mount_partition_and_get_mount_point(&efi_partition_id)?;

    if !system_mount.join("Windows").exists() {
        return Err(AppError::DiskError(format!(
            "Target partition {} does not contain a Windows directory",
            system_partition_id
        )));
    }

    stage_uefi_boot_payload(&system_mount, &efi_mount)?;
    repair_uefi_bcd_store(&system_mount, &efi_mount)?;
    verify_uefi_boot_files(&efi_mount)?;

    Ok(format!(
        "Boot repair completed (UEFI). System: /dev/{} -> {} ; EFI: /dev/{} -> {}",
        system_partition_id,
        system_mount.display(),
        efi_partition_id,
        efi_mount.display()
    ))
}

fn verify_applied_system_files(system_mount: &Path) -> Result<()> {
    let windows_dir = system_mount.join("Windows");
    let system32_dir = windows_dir.join("System32");
    if !windows_dir.exists() || !system32_dir.exists() {
        return Err(AppError::ImageError(format!(
            "Applied system files are incomplete at {}",
            system_mount.display()
        )));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct MacExtraFeatureOutcome {
    applied: Vec<&'static str>,
    unsupported: Vec<&'static str>,
    notes: Vec<String>,
}

fn write_text_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(AppError::io)
}

fn ensure_setup_complete_invokes_wtga_extra(scripts_dir: &Path) -> Result<()> {
    let setup_complete_path = scripts_dir.join("SetupComplete.cmd");
    let call_line = r#"call "%SystemRoot%\Setup\Scripts\WTGA-ExtraFeatures.cmd""#;
    let lower_match = "wtga-extrafeatures.cmd";

    let mut content = if setup_complete_path.exists() {
        fs::read_to_string(&setup_complete_path).map_err(AppError::io)?
    } else {
        "@echo off\r\n".to_string()
    };

    if !content.to_ascii_lowercase().contains(lower_match) {
        if !content.ends_with('\n') {
            content.push_str("\r\n");
        }
        content.push_str(call_line);
        content.push_str("\r\n");
    }

    if !content.to_ascii_lowercase().contains("exit /b 0") {
        content.push_str("exit /b 0\r\n");
    }

    write_text_file(&setup_complete_path, &content)
}

fn write_skip_oobe_unattend_if_needed(system_mount: &Path) -> Result<()> {
    let sysprep_dir = system_mount.join("Windows").join("System32").join("Sysprep");
    fs::create_dir_all(&sysprep_dir).map_err(AppError::io)?;
    let unattend_path = sysprep_dir.join("unattend.xml");

    if unattend_path.exists() {
        return Ok(());
    }

    // Keep this minimal and architecture-agnostic by including both amd64 and arm64 blocks.
    let unattend = r#"<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend">
  <settings pass="oobeSystem">
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <OOBE>
        <HideEULAPage>true</HideEULAPage>
        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
        <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
        <ProtectYourPC>3</ProtectYourPC>
        <SkipMachineOOBE>true</SkipMachineOOBE>
        <SkipUserOOBE>true</SkipUserOOBE>
      </OOBE>
    </component>
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="arm64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <OOBE>
        <HideEULAPage>true</HideEULAPage>
        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
        <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
        <ProtectYourPC>3</ProtectYourPC>
        <SkipMachineOOBE>true</SkipMachineOOBE>
        <SkipUserOOBE>true</SkipUserOOBE>
      </OOBE>
    </component>
  </settings>
</unattend>
"#;

    write_text_file(&unattend_path, unattend)
}

fn apply_macos_extra_features(config: &WtgConfig, system_mount: &Path) -> Result<MacExtraFeatureOutcome> {
    let mut outcome = MacExtraFeatureOutcome::default();
    let features = &config.extra_features;

    let windows_dir = system_mount.join("Windows");
    if !windows_dir.exists() {
        return Err(AppError::ImageError(format!(
            "Cannot apply extra features: Windows directory missing at {}",
            windows_dir.display()
        )));
    }

    let scripts_dir = windows_dir.join("Setup").join("Scripts");
    fs::create_dir_all(&scripts_dir).map_err(AppError::io)?;

    let mut script_lines = vec![
        "@echo off".to_string(),
        "setlocal enableextensions".to_string(),
        "set WTGA_LOG=%SystemRoot%\\Setup\\Scripts\\WTGA-ExtraFeatures.log".to_string(),
        "echo [WTGA] Extra features script started %DATE% %TIME%>>\"%WTGA_LOG%\"".to_string(),
    ];

    if features.block_local_disk {
        outcome.applied.push("block_local_disk");
        script_lines.push("echo [WTGA] Applying SAN policy (block local disks)>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("reg add \"HKLM\\SYSTEM\\CurrentControlSet\\Services\\partmgr\\Parameters\" /v SanPolicy /t REG_DWORD /d 4 /f >>\"%WTGA_LOG%\" 2>&1".to_string());
        script_lines.push("(echo san policy=4) > \"%SystemRoot%\\Setup\\Scripts\\wtga-san.txt\"".to_string());
        script_lines.push("diskpart /s \"%SystemRoot%\\Setup\\Scripts\\wtga-san.txt\" >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.disable_winre {
        outcome.applied.push("disable_winre");
        script_lines.push("echo [WTGA] Disabling WinRE>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("reagentc /disable >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.skip_oobe {
        outcome.applied.push("skip_oobe");
        write_skip_oobe_unattend_if_needed(system_mount)?;
        script_lines.push("echo [WTGA] Applying skip OOBE registry flags>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("reg add \"HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE\" /v SkipMachineOOBE /t REG_DWORD /d 1 /f >>\"%WTGA_LOG%\" 2>&1".to_string());
        script_lines.push("reg add \"HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE\" /v SkipUserOOBE /t REG_DWORD /d 1 /f >>\"%WTGA_LOG%\" 2>&1".to_string());
        script_lines.push("reg add \"HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE\" /v PrivacyConsentStatus /t REG_DWORD /d 1 /f >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.disable_uasp {
        outcome.applied.push("disable_uasp");
        script_lines.push("echo [WTGA] Disabling UASP (uaspstor)>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("reg add \"HKLM\\SYSTEM\\CurrentControlSet\\Services\\uaspstor\" /v Start /t REG_DWORD /d 4 /f >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.no_default_drive_letter {
        outcome.applied.push("no_default_drive_letter");
        script_lines.push("echo [WTGA] Setting no default drive letter policy>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("mountvol /N >>\"%WTGA_LOG%\" 2>&1".to_string());
        script_lines.push("(echo select volume %SystemDrive% & echo attributes volume set nodefaultdriveletter) > \"%SystemRoot%\\Setup\\Scripts\\wtga-nodefaultdriveletter.txt\"".to_string());
        script_lines.push("diskpart /s \"%SystemRoot%\\Setup\\Scripts\\wtga-nodefaultdriveletter.txt\" >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.compact_os {
        outcome.applied.push("compact_os");
        script_lines.push("echo [WTGA] Enabling CompactOS>>\"%WTGA_LOG%\"".to_string());
        script_lines.push("compact.exe /CompactOS:always >>\"%WTGA_LOG%\" 2>&1".to_string());
    }

    if features.install_dotnet35 {
        outcome.unsupported.push("install_dotnet35");
    }
    if features.fix_letter {
        outcome.unsupported.push("fix_letter");
    }
    if features.wimboot {
        outcome.unsupported.push("wimboot");
    }
    if features.ntfs_uefi_support {
        // UEFI flow on macOS already uses a dedicated FAT EFI partition; this flag is a no-op here.
        outcome.notes.push("ntfs_uefi_support is ignored on macOS UEFI write path".to_string());
    }
    if features.enable_bitlocker {
        outcome.unsupported.push("enable_bitlocker");
    }
    if features.driver_path.is_some() {
        outcome.unsupported.push("driver_path");
    }

    script_lines.push("echo [WTGA] Extra features script finished %DATE% %TIME%>>\"%WTGA_LOG%\"".to_string());
    script_lines.push("endlocal".to_string());
    script_lines.push("exit /b 0".to_string());

    if outcome.applied.is_empty() {
        return Ok(outcome);
    }

    let extra_script_path = scripts_dir.join("WTGA-ExtraFeatures.cmd");
    let script_text = format!("{}\r\n", script_lines.join("\r\n"));
    write_text_file(&extra_script_path, &script_text)?;
    ensure_setup_complete_invokes_wtga_extra(&scripts_dir)?;

    Ok(outcome)
}

fn prepare_target_disk(config: &WtgConfig, disk_id: &str) -> Result<PreparedTargetDisk> {
    let node = format!("/dev/{}", disk_id);
    let partition_command = match config.boot_mode {
        BootMode::UefiGpt => format!(
            "diskutil partitionDisk {} GPT FAT32 EFI 512m ExFAT WTGA R",
            node
        ),
        BootMode::UefiMbr => format!(
            "diskutil partitionDisk {} MBRFormat FAT32 EFI 512m ExFAT WTGA R",
            node
        ),
        BootMode::NonUefi => format!("diskutil partitionDisk {} MBRFormat ExFAT WTGA R", node),
    };

    if let Err(e) = macos_admin::run_shell_with_auto_privilege(&partition_command) {
        warn!(
            "diskutil partitionDisk failed on /dev/{} after auto-privilege retry: {}",
            disk_id, e
        );
        return Err(e);
    }

    thread::sleep(Duration::from_secs(2));

    let expected = expected_partition_ids_for_boot_mode(&config.boot_mode, disk_id);
    let prepared = match resolve_prepared_partitions(disk_id, &config.boot_mode) {
        Ok(v) => v,
        Err(_) => expected,
    };
    let _ = get_partition_info_json(&prepared.system_partition_id)?;
    format_partition_ntfs(&prepared.system_partition_id, "WTGA")?;
    thread::sleep(Duration::from_secs(1));
    let system_info = get_partition_info_json(&prepared.system_partition_id)?;
    let filesystem = json_str(&system_info, "FilesystemType").to_ascii_lowercase();
    if !filesystem.contains("ntfs") {
        return Err(AppError::DiskError(format!(
            "System partition {} format verification failed: expected NTFS but got '{}'",
            prepared.system_partition_id,
            if filesystem.is_empty() {
                "unknown".to_string()
            } else {
                filesystem
            }
        )));
    }

    if let Some(efi_id) = &prepared.efi_partition_id {
        let _ = get_partition_info_json(efi_id)?;
    }

    Ok(prepared)
}

pub fn get_image_info(image_path: &str) -> Result<Vec<ImageInfo>> {
    let path = Path::new(image_path);
    if !path.exists() {
        return Err(AppError::ImageError(format!(
            "Image file does not exist: {}",
            image_path
        )));
    }

    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wim" | "esd" => {
            requires_wimlib()?;
            let info = get_wimlib_image_info(path)?;
            if info.is_empty() {
                return Err(AppError::ImageError(format!(
                    "No image index found in {}",
                    image_path
                )));
            }
            return Ok(info);
        }
        "iso" => {
            requires_wimlib()?;
            return get_image_info_from_iso(path);
        }
        _ => {}
    }

    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Windows Image".to_string());
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    Ok(vec![ImageInfo {
        index: 1,
        name,
        description: "macOS migration mode: single default image index".to_string(),
        size,
    }])
}

pub fn execute_write(config: &WtgConfig, app_files_path: &str) -> Result<WriteProgress> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();

    PROGRESS_REPORTER.report_status(
        &task_id,
        5.0,
        "Checking macOS write prerequisites",
        "preparing",
    );

    let image_path = Path::new(&config.image_path);
    if !image_path.exists() {
        return Err(AppError::ImageError(format!(
            "Image file does not exist: {}",
            config.image_path
        )));
    }

    if matches!(config.boot_mode, BootMode::NonUefi) {
        return Err(AppError::Unsupported(
            "macOS WTG write currently supports UEFI boot modes only".to_string(),
        ));
    }

    requires_wimlib()?;
    requires_ntfs_tooling()?;

    let resolved_image = resolve_apply_image(image_path)?;
    let wim_index = resolve_wim_index(&resolved_image.image_path, &config.wim_index)?;

    let _ = fs::create_dir_all(app_files_path);

    let disk_id = resolve_disk_id(config)?;
    let disk_info = diskutil_json(&["info", "-plist", &format!("/dev/{}", disk_id)])?;
    if json_bool(&disk_info, "Internal") {
        return Err(AppError::DiskError(format!(
            "Refusing to write to internal disk /dev/{} on macOS. Please select an external target disk.",
            disk_id
        )));
    }

    let allow_repartition =
        config.extra_features.repartition || !config.extra_features.do_not_format;
    if !allow_repartition {
        return Err(AppError::Unsupported(
            "macOS WTG write currently requires repartition/format. Please disable 'Do not format' or enable repartition."
                .to_string(),
        ));
    }

    PROGRESS_REPORTER.report_status(
        &task_id,
        15.0,
        "Partitioning target disk (EFI + NTFS system)",
        "partitioning",
    );
    let prepared = prepare_target_disk(config, &disk_id)?;

    PROGRESS_REPORTER.report_status(
        &task_id,
        28.0,
        "Mounting NTFS system partition writable",
        "partitioning",
    );
    let system_mount = mount_ntfs_partition_writable(&prepared.system_partition_id, "WTGA")?;

    PROGRESS_REPORTER.report_status(
        &task_id,
        36.0,
        "Mounting EFI partition",
        "partitioning",
    );
    let efi_mount = if let Some(efi_partition_id) = &prepared.efi_partition_id {
        match mount_partition_and_get_mount_point(efi_partition_id) {
            Ok(mount) => Some(mount),
            Err(primary_err) => {
                if let Some((alt_id, alt_mount)) = try_mount_alternative_efi_partition(
                    &disk_id,
                    &prepared.system_partition_id,
                    efi_partition_id,
                )? {
                    warn!(
                        "EFI mount fallback: {} failed, using {} instead",
                        efi_partition_id, alt_id
                    );
                    Some(alt_mount)
                } else {
                    return Err(primary_err);
                }
            }
        }
    } else {
        None
    };

    PROGRESS_REPORTER.report_status(
        &task_id,
        52.0,
        "Applying Windows image to NTFS system partition",
        "applyingimage",
    );
    apply_windows_image(&resolved_image.image_path, &wim_index, &system_mount)?;
    verify_applied_system_files(&system_mount)?;

    PROGRESS_REPORTER.report_status(
        &task_id,
        72.0,
        "Applying extra features",
        "applyingextras",
    );
    let extra_outcome = apply_macos_extra_features(config, &system_mount)?;

    PROGRESS_REPORTER.report_status(
        &task_id,
        82.0,
        "Staging UEFI boot files",
        "writingbootfiles",
    );
    let Some(efi_mount_path) = efi_mount.as_ref() else {
        return Err(AppError::DiskError(
            "EFI partition was not created/mounted; cannot stage boot files".to_string(),
        ));
    };
    stage_uefi_boot_payload(&system_mount, efi_mount_path)?;

    PROGRESS_REPORTER.report_status(
        &task_id,
        88.0,
        "Fixing BCD for UEFI boot",
        "fixingbcd",
    );
    repair_uefi_bcd_store(&system_mount, efi_mount_path)?;

    PROGRESS_REPORTER.report_status(&task_id, 90.0, "Verifying write result", "verifying");
    verify_uefi_boot_files(efi_mount_path)?;

    let elapsed = started.elapsed().as_secs();
    info!(
        "macOS WTG deploy completed: image={} system={} efi={}",
        resolved_image.image_path.display(),
        system_mount.display(),
        efi_mount_path.display()
    );
    if !extra_outcome.applied.is_empty() {
        info!(
            "macOS extra features applied: {}",
            extra_outcome.applied.join(", ")
        );
    }
    if !extra_outcome.unsupported.is_empty() {
        warn!(
            "macOS extra features not supported yet: {}",
            extra_outcome.unsupported.join(", ")
        );
    }
    for note in &extra_outcome.notes {
        info!("macOS extra features note: {}", note);
    }

    PROGRESS_REPORTER.report_status(&task_id, 100.0, "Write completed", "completed");

    let mut message = format!(
        "WTG image applied to NTFS system partition {} and UEFI boot files staged at {}.",
        system_mount.display(),
        efi_mount_path.display()
    );
    if !extra_outcome.applied.is_empty() {
        message.push_str(&format!(
            " Applied extra features: {}.",
            extra_outcome.applied.join(", ")
        ));
    }
    if !extra_outcome.unsupported.is_empty() {
        message.push_str(&format!(
            " Not yet supported on macOS: {}.",
            extra_outcome.unsupported.join(", ")
        ));
    }

    Ok(WriteProgress {
        task_id,
        status: WriteStatus::Completed,
        progress: 100.0,
        message,
        speed: 0.0,
        elapsed_seconds: elapsed,
        estimated_remaining_seconds: 0,
    })
}
