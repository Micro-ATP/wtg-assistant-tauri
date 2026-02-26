//! macOS write service (migration stage)
//! Provides a usable write pipeline on macOS:
//! - preflight validation
//! - target disk writable check (including NTFS remount helper)
//! - staged file write to target volume

use crate::models::{BootMode, ImageInfo, WriteProgress, WriteStatus, WtgConfig};
use crate::utils::macos_admin;
use crate::utils::progress::PROGRESS_REPORTER;
use crate::{AppError, Result};
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

fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .args(["-lc", &format!("command -v {} >/dev/null 2>&1", cmd)])
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

fn resolve_disk_id(config: &WtgConfig) -> Result<String> {
    if let Some(v) = parse_disk_id(&config.target_disk.device) {
        return Ok(v);
    }
    if let Some(v) = parse_disk_id(&config.target_disk.id) {
        return Ok(v);
    }
    if !config.target_disk.index.trim().is_empty() {
        let index = config.target_disk.index.trim();
        if index.chars().all(|c| c.is_ascii_digit()) {
            return Ok(format!("disk{}", index));
        }
    }
    Err(AppError::InvalidParameter(format!(
        "Invalid target disk identifier: id='{}', device='{}', index='{}'",
        config.target_disk.id, config.target_disk.device, config.target_disk.index
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

fn run_ntfs_mount_script_as_admin() -> Result<()> {
    let script = find_ntfs_mount_script().ok_or_else(|| {
        AppError::SystemError("Cannot find useable_software/ntfs-mount.sh".to_string())
    })?;

    let command = format!(
        "bash '{}'",
        shell_escape_single_quotes(script.to_string_lossy().as_ref())
    );
    macos_admin::run_shell_with_auto_privilege(&command)
}

fn partition_scheme_for_boot_mode(mode: &BootMode) -> &'static str {
    match mode {
        BootMode::UefiGpt => "GPT",
        BootMode::UefiMbr | BootMode::NonUefi => "MBRFormat",
    }
}

fn prepare_target_disk(config: &WtgConfig, disk_id: &str) -> Result<()> {
    let scheme = partition_scheme_for_boot_mode(&config.boot_mode);
    let node = format!("/dev/{}", disk_id);
    let command = format!("diskutil eraseDisk ExFAT WTGA {} {}", scheme, node);
    if let Err(e) = macos_admin::run_shell_with_auto_privilege(&command) {
        warn!(
            "diskutil eraseDisk failed on /dev/{} after auto-privilege retry: {}",
            disk_id, e
        );
        return Err(e);
    }

    // Let Disk Arbitration settle before querying partitions.
    thread::sleep(Duration::from_secs(2));
    Ok(())
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

    ensure_disk_mounted(&disk_id)?;

    let mut mounted = find_partition_for_disk(&disk_id)?;
    if mounted.is_none() {
        if allow_repartition {
            PROGRESS_REPORTER.report_status(
                &task_id,
                18.0,
                "Preparing target disk partition for macOS write",
                "partitioning",
            );
            prepare_target_disk(config, &disk_id)?;
            ensure_disk_mounted(&disk_id)?;
            mounted = find_partition_for_disk(&disk_id)?;
        }
    }

    let (partition_id, mount_point) = mounted.ok_or_else(|| {
        AppError::DiskError(format!(
            "No mounted partition found on target disk {}. Please mount the target volume first.",
            disk_id
        ))
    })?;

    let mut part_info = get_partition_info_json(&partition_id)?;
    let mut filesystem = json_str(&part_info, "FilesystemType").to_ascii_lowercase();
    let mut writable_volume = json_bool(&part_info, "WritableVolume");
    let mut mount_path = PathBuf::from(&mount_point);

    PROGRESS_REPORTER.report_status(
        &task_id,
        25.0,
        "Validating target disk writable state",
        "partitioning",
    );

    let mut writable = writable_volume && is_dir_writable(&mount_path);
    if !writable && filesystem == "ntfs" {
        if !command_exists("ntfs-3g") {
            return Err(AppError::DiskError(
                "NTFS target detected but ntfs-3g is missing. Install with: brew install --cask macfuse && brew tap gromgit/homebrew-fuse && brew install ntfs-3g-mac".to_string(),
            ));
        }

        PROGRESS_REPORTER.report_status(
            &task_id,
            38.0,
            "Remounting NTFS disk as writable",
            "partitioning",
        );
        run_ntfs_mount_script_as_admin()?;

        ensure_disk_mounted(&disk_id)?;
        let (pid, mnt) = find_partition_for_disk(&disk_id)?.ok_or_else(|| {
            AppError::DiskError(
                "Target disk was remounted but no mounted partition was found".to_string(),
            )
        })?;
        part_info = get_partition_info_json(&pid)?;
        filesystem = json_str(&part_info, "FilesystemType").to_ascii_lowercase();
        writable_volume = json_bool(&part_info, "WritableVolume");
        mount_path = PathBuf::from(mnt);
        writable = writable_volume && is_dir_writable(&mount_path);
    }

    if !writable {
        if allow_repartition {
            PROGRESS_REPORTER.report_status(
                &task_id,
                42.0,
                "Target not writable, repartitioning for writable volume",
                "partitioning",
            );
            prepare_target_disk(config, &disk_id)?;
            ensure_disk_mounted(&disk_id)?;
            let (pid, mnt) = find_partition_for_disk(&disk_id)?.ok_or_else(|| {
                AppError::DiskError(
                    "Target disk was repartitioned but no mounted partition was found".to_string(),
                )
            })?;
            part_info = get_partition_info_json(&pid)?;
            filesystem = json_str(&part_info, "FilesystemType").to_ascii_lowercase();
            writable_volume = json_bool(&part_info, "WritableVolume");
            mount_path = PathBuf::from(mnt);
            writable = writable_volume && is_dir_writable(&mount_path);
        }
    }

    if !writable {
        return Err(AppError::DiskError(format!(
            "Target volume is not writable: {} (filesystem: {}).",
            mount_path.display(),
            if filesystem.is_empty() {
                "unknown".to_string()
            } else {
                filesystem
            }
        )));
    }

    PROGRESS_REPORTER.report_status(
        &task_id,
        55.0,
        "Writing image payload to target disk",
        "applyingimage",
    );

    let staging_dir = mount_path.join("WTGA");
    fs::create_dir_all(&staging_dir).map_err(AppError::io)?;
    fs::create_dir_all(app_files_path).map_err(AppError::io)?;

    let file_name = image_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::ImageError("Invalid image filename".to_string()))?;
    let target_image = staging_dir.join(file_name);

    fs::copy(image_path, &target_image).map_err(AppError::io)?;

    PROGRESS_REPORTER.report_status(&task_id, 90.0, "Verifying write result", "verifying");

    let copied_size = fs::metadata(&target_image).map_err(AppError::io)?.len();
    let source_size = fs::metadata(image_path).map_err(AppError::io)?.len();
    if copied_size != source_size {
        return Err(AppError::DiskError(format!(
            "Write verification failed: copied {} bytes, expected {} bytes",
            copied_size, source_size
        )));
    }

    let elapsed = started.elapsed().as_secs();
    info!(
        "macOS write migration stage completed: {} -> {}",
        image_path.display(),
        target_image.display()
    );

    PROGRESS_REPORTER.report_status(&task_id, 100.0, "Write completed", "completed");

    Ok(WriteProgress {
        task_id,
        status: WriteStatus::Completed,
        progress: 100.0,
        message: format!(
            "Image payload written to {}. (macOS migration stage: image deployment/boot setup will continue in next iterations)",
            target_image.display()
        ),
        speed: 0.0,
        elapsed_seconds: elapsed,
        estimated_remaining_seconds: 0,
    })
}
