//! VHD service - manages VHD/VHDX operations
//! Translated from VHDOperation.cs

#![allow(dead_code)]

use crate::utils::command::{run_diskpart_script, wait_for_path, CommandExecutor};
use crate::services::boot;
use crate::models::{FirmwareType, BootMode};
use crate::{AppError, Result};
use regex::Regex;
use tracing::info;

/// VHD operation context
pub struct VhdOperation {
    pub extension_type: String,
    pub vhd_path: String,
    pub vhd_size: String,
    pub vhd_type: String,
    pub need_copy: bool,
    pub vhd_filename: String,
}

impl VhdOperation {
    /// Create new VHD operation with configuration
    pub fn new(
        image_type: &str,
        image_file_path: &str,
        is_fixed: bool,
        user_set_size: u32,
        ud: &str,
        vhd_temp_path: &str,
        vhd_name: &str,
        vhd_ext: &str,
        boot_mode: &BootMode,
        is_wimboot: bool,
        is_no_temp: bool,
        ntfs_uefi_support: bool,
    ) -> Self {
        let image_type_lower = image_type.to_lowercase();
        let _ = ntfs_uefi_support; // Used by caller to determine need_copy

        if image_type_lower == "vhd" || image_type_lower == "vhdx" {
            // Importing existing VHD/VHDX
            return VhdOperation {
                extension_type: String::new(),
                vhd_path: image_file_path.to_string(),
                vhd_size: String::new(),
                vhd_type: String::new(),
                need_copy: true,
                vhd_filename: String::new(),
            };
        }

        let vhd_type_str = if is_fixed { "fixed" } else { "expandable" };
        let default_size: u32 = 40960;

        // Calculate VHD size
        let vhd_size = if user_set_size != 0 {
            user_set_size.to_string()
        } else {
            default_size.to_string()
        };

        // Determine if we need to copy (create in temp then copy to USB)
        let is_uefi = *boot_mode == BootMode::UefiGpt || *boot_mode == BootMode::UefiMbr;
        let vhd_filename = format!("{}.{}", vhd_name, vhd_ext);

        let need_copy = !is_uefi && !is_wimboot && !is_no_temp;
        let vhd_path = if need_copy {
            format!("{}\\{}", vhd_temp_path, vhd_filename)
        } else {
            format!("{}{}", ud, vhd_filename)
        };

        VhdOperation {
            extension_type: vhd_ext.to_string(),
            vhd_path,
            vhd_size: vhd_size.to_string(),
            vhd_type: vhd_type_str.to_string(),
            need_copy,
            vhd_filename,
        }
    }
}

/// Clean up existing VHD temp files and processes
pub fn clean_temp(vhd_name: &str) -> Result<()> {
    info!("Cleaning VHD temp files");

    let _ = CommandExecutor::kill_process("dism.exe");
    let _ = CommandExecutor::kill_process("diskpart.exe");

    // Detach any existing VHDs on V:
    if std::path::Path::new("V:\\").exists() {
        let _ = detach_vhd_extra(vhd_name);
    }
    if std::path::Path::new("V:\\").exists() {
        let _ = detach_vhd_extra(vhd_name);
    }

    if std::path::Path::new("V:\\").exists() {
        return Err(AppError::DiskError(
            "Drive letter V: is occupied and cannot be released".to_string(),
        ));
    }

    // Clean temp files
    let temp_dir = std::env::temp_dir();
    let patterns = ["create.txt", "removex.txt", "detach.txt", "uefi.txt", "uefimbr.txt", "dp.txt", "attach.txt"];
    for pattern in &patterns {
        let path = temp_dir.join(pattern);
        let _ = std::fs::remove_file(&path);
    }

    // Clean VHD temp files
    let vhd_path = temp_dir.join(format!("{}.vhd", vhd_name));
    let vhdx_path = temp_dir.join(format!("{}.vhdx", vhd_name));
    let _ = std::fs::remove_file(&vhd_path);
    let _ = std::fs::remove_file(&vhdx_path);

    Ok(())
}

/// Create and attach a VHD, then apply image
/// Equivalent to VHDOperation.CreateVHD()
pub fn create_vhd(
    vhd_path: &str,
    vhd_type: &str,
    vhd_size: &str,
    image_type: &str,
    vhd_partition_type: u8,
) -> Result<()> {
    let mut script = String::new();

    let image_type_lower = image_type.to_lowercase();
    if image_type_lower == "vhd" || image_type_lower == "vhdx" {
        // Import existing VHD
        script.push_str(&format!("select vdisk file=\"{}\"\n", vhd_path));
        script.push_str("attach vdisk\n");
        script.push_str("assign letter=v\n");
        script.push_str("exit\n");
    } else {
        // Create new VHD
        script.push_str(&format!(
            "create vdisk file=\"{}\" type={} maximum={}\n",
            vhd_path, vhd_type, vhd_size
        ));
        script.push_str(&format!("select vdisk file=\"{}\"\n", vhd_path));
        script.push_str("attach vdisk\n");
        if vhd_partition_type == 1 {
            script.push_str("convert gpt\n");
        }
        script.push_str("create partition primary\n");
        script.push_str("format fs=ntfs quick\n");
        script.push_str("assign letter=v\n");
        script.push_str("exit\n");
    }

    info!("Creating VHD: {}", vhd_path);
    run_diskpart_script(&script)?;

    // Verify V: drive exists
    if !std::path::Path::new("V:\\").exists() {
        return Err(AppError::DiskError("VHD creation failed - V: drive not found".to_string()));
    }

    Ok(())
}

/// Attach an existing VHD and assign letter V:
pub fn attach_vhd(vhd_path: &str) -> Result<()> {
    let script = format!(
        "select vdisk file=\"{}\"\nattach vdisk\nsel partition 1\nassign letter=v\nexit\n",
        vhd_path
    );

    info!("Attaching VHD: {}", vhd_path);
    run_diskpart_script(&script)?;
    Ok(())
}

/// Detach a VHD
pub fn detach_vhd(vhd_path: &str) -> Result<()> {
    let script = format!(
        "select vdisk file=\"{}\"\ndetach vdisk\n",
        vhd_path
    );

    info!("Detaching VHD: {}", vhd_path);
    run_diskpart_script(&script)?;
    Ok(())
}

/// Detach VHD by searching for it in list vdisk output
fn detach_vhd_extra(vhd_name: &str) -> Result<()> {
    let (output, _) = crate::utils::command::run_diskpart_script_with_output("list vdisk")?;

    let pattern = format!(r"(?i)([a-z]:\\.*{}\.(vhd|vhdx))", regex::escape(vhd_name));
    let re = Regex::new(&pattern).map_err(|e| AppError::SystemError(e.to_string()))?;

    for cap in re.captures_iter(&output) {
        let path = &cap[1];
        info!("Found VHD to detach: {}", path);
        let _ = detach_vhd(path);
    }

    Ok(())
}

/// Copy VHD to USB drive using robocopy
/// Equivalent to VHDOperation.CopyVHD()
#[cfg(target_os = "windows")]
pub fn copy_vhd(vhd_path: &str, target_ud: &str, _vhd_ext: &str) -> Result<()> {
    if !std::path::Path::new(vhd_path).exists() {
        return Err(AppError::DiskError("VHD file not found for copying".to_string()));
    }

    info!("Copying VHD from {} to {}", vhd_path, target_ud);

    let source_dir = std::path::Path::new(vhd_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let filename = std::path::Path::new(vhd_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    // Use robocopy for efficient file copy
    let windir = std::env::var("windir").unwrap_or_else(|_| "C:\\Windows".to_string());
    let robocopy_path = format!("{}\\system32\\robocopy.exe", windir);

    if std::path::Path::new(&robocopy_path).exists() {
        let _ = CommandExecutor::execute_allow_fail(
            &robocopy_path,
            &[
                &format!("{}\\", source_dir),
                target_ud,
                &filename,
                "/ETA",
            ],
        );
    } else {
        // Fallback to standard file copy
        let dest = format!("{}{}", target_ud, filename);
        std::fs::copy(vhd_path, &dest).map_err(AppError::io)?;
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn copy_vhd(vhd_path: &str, target_ud: &str, _vhd_ext: &str) -> Result<()> {
    let filename = std::path::Path::new(vhd_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let dest = format!("{}{}", target_ud, filename);
    std::fs::copy(vhd_path, &dest).map_err(AppError::io)?;
    Ok(())
}

/// Twice-attach VHD and write boot files (after copy to USB)
/// Equivalent to VHDOperation.TwiceAttachVHDAndWriteBootFile()
pub fn twice_attach_and_write_boot(
    ud: &str,
    vhd_filename: &str,
    ntfs_uefi_support: bool,
) -> Result<()> {
    let vhd_full_path = format!("{}{}", ud, vhd_filename);

    let script = format!(
        "select vdisk file={}\nattach vdisk\nsel partition 1\nassign letter=v\nexit\n",
        vhd_full_path
    );

    run_diskpart_script(&script)?;
    wait_for_path("V:\\", 100, 100);

    if !std::path::Path::new("V:\\").exists() {
        return Err(AppError::DiskError("Second VHD attach failed".to_string()));
    }

    // Write boot files
    if ntfs_uefi_support {
        boot::bcdboot_write_boot_file("V:\\", ud, &FirmwareType::ALL)?;
    } else {
        boot::bcdboot_write_boot_file("V:\\", ud, &FirmwareType::BIOS)?;
    }

    // Detach
    detach_vhd(&vhd_full_path)?;

    Ok(())
}

/// Write dynamic VHD size instruction file
pub fn write_dynamic_size_instruction(ud: &str, vhd_size: &str) -> Result<()> {
    let instruction = format!(
        "VHD Dynamic Size: {}MB\nThe VHD will expand to this size after the system boots.\nPlease ensure sufficient space on the USB drive.\n",
        vhd_size
    );

    let path = format!("{}VHD_Info.txt", ud);
    std::fs::write(&path, &instruction).map_err(AppError::io)?;

    Ok(())
}
