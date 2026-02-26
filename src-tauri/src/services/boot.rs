//! Boot file service - manages boot file operations
//! Translated from BootFileOperation.cs

use crate::models::FirmwareType;
use crate::utils::command::CommandExecutor;
use crate::utils::first_two_chars;
use crate::{AppError, Result};
use tracing::{info, warn};

/// Write boot files using bcdboot
/// Equivalent to BootFileOperation.BcdbootWriteBootFile()
pub fn bcdboot_write_boot_file(
    source_disk: &str,
    target_disk: &str,
    fw_type: &FirmwareType,
) -> Result<()> {
    let target = first_two_chars(target_disk); // e.g., "E:"

    // Ensure source_disk has proper path format (should be like "D:\")
    let source_windows = if source_disk.ends_with('\\') {
        format!("{}windows", source_disk)
    } else {
        format!("{}\\windows", source_disk)
    };

    let fw_flag = match fw_type {
        FirmwareType::ALL => "all",
        FirmwareType::BIOS => "bios",
        FirmwareType::UEFI => "uefi",
    };

    // Build arguments as a proper vector instead of splitting a string
    let args = vec![
        source_windows.as_str(),
        "/s",
        target,
        "/f",
        fw_flag,
        "/l",
        "zh-CN",
        "/v",
    ];

    info!("Running bcdboot with args: {:?}", args);
    info!("  Source Windows: {}", source_windows);
    info!("  Target disk: {}", target);
    info!("  Firmware type: {}", fw_flag);

    // Verify source Windows path exists before running bcdboot
    if !std::path::Path::new(&source_windows).exists() {
        warn!("Source Windows path does not exist: {}", source_windows);
        return Err(AppError::CommandFailed(format!(
            "Source Windows path not found: {}. Cannot run bcdboot.",
            source_windows
        )));
    }

    // Check if target partition is accessible before running bcdboot
    info!("Checking if target partition {} is accessible...", target);
    if !std::path::Path::new(target).exists() {
        warn!(
            "Target partition {} does not exist or is not accessible!",
            target
        );
        return Err(AppError::CommandFailed(format!(
            "Target partition {} is not accessible. Check if it's properly mounted and has read/write permissions.",
            target
        )));
    }

    // Try to create a test file to verify write access
    let test_file = format!("{}\\__test_write_access__.tmp", target);
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            info!("✓ Target partition {} is writable", target);
        }
        Err(e) => {
            warn!("Target partition {} is NOT writable: {}", target, e);
            return Err(AppError::CommandFailed(format!(
                "Cannot write to target partition {}. Error: {}. \
                 Make sure the partition is formatted, not read-only, and you have administrator permissions.",
                target, e
            )));
        }
    }

    // Execute bcdboot
    let result = CommandExecutor::execute_allow_fail("bcdboot.exe", &args);

    match result {
        Ok(output) => {
            info!("bcdboot output: {}", output);

            // Validate that boot files were actually created
            validate_boot_files_created(target, fw_type)?;

            Ok(())
        }
        Err(e) => {
            warn!("bcdboot execution failed: {}", e);
            Err(AppError::CommandFailed(format!("bcdboot failed: {}", e)))
        }
    }
}

/// Validate that boot files were actually created by bcdboot
fn validate_boot_files_created(target_disk: &str, fw_type: &FirmwareType) -> Result<()> {
    use std::path::Path;

    // Check for BCD file at the expected location
    let bcd_path = match fw_type {
        FirmwareType::UEFI => {
            // UEFI boot files go to EFI partition
            format!("{}\\EFI\\Microsoft\\Boot\\BCD", target_disk)
        }
        _ => {
            // BIOS boot files go to boot partition
            format!("{}\\Boot\\BCD", target_disk)
        }
    };

    // Also check for bootmgr (BIOS only) or bootmgr.efi (UEFI)
    let bootmgr_path = match fw_type {
        FirmwareType::UEFI => format!("{}\\EFI\\Microsoft\\Boot\\bootmgfw.efi", target_disk),
        _ => format!("{}\\bootmgr", target_disk),
    };

    info!("Validating boot files at target disk: {}", target_disk);
    info!("  Expected BCD path: {}", bcd_path);
    info!("  Expected bootmgr path: {}", bootmgr_path);

    // Try to list files in the target directory for debugging
    if let Ok(entries) = std::fs::read_dir(target_disk) {
        info!("Files in target disk root ({}): ", target_disk);
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(metadata) = entry.metadata() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if metadata.is_dir() {
                        info!("    [DIR] {}", file_name);
                    } else {
                        info!("    [FILE] {}", file_name);
                    }
                }
            }
        }
    } else {
        warn!("Could not read target disk directory: {}", target_disk);
    }

    if Path::new(&bcd_path).exists() {
        info!("Boot files validation SUCCESS: BCD found at {}", bcd_path);
        Ok(())
    } else {
        warn!(
            "Boot files validation FAILED: BCD not found at {}",
            bcd_path
        );
        warn!("Expected bootmgr at: {}", bootmgr_path);

        // Try to check if the EFI directory itself exists
        if fw_type == &FirmwareType::UEFI {
            let efi_dir = format!("{}\\EFI", target_disk);
            if Path::new(&efi_dir).exists() {
                warn!("EFI directory exists, but BCD is missing");
            } else {
                warn!("EFI directory does not exist at {}", efi_dir);
            }
        }

        Err(AppError::CommandFailed(
            format!("bcdboot completed but boot files not found at {}. Check if target partition is accessible and has proper permissions.", bcd_path)
        ))
    }
}

/// Fix BCD for typical (direct) mode
/// Equivalent to BootFileOperation.BcdeditFixBootFileTypical()
pub fn bcdedit_fix_boot_file_typical(
    bcd_disk: &str,
    osdevice: &str,
    fw_type: &FirmwareType,
) -> Result<()> {
    // Wait for bcdboot to finish
    wait_for_bcdboot();

    let bcd_path = match fw_type {
        FirmwareType::UEFI => "\\EFI\\Microsoft\\Boot\\BCD",
        _ => "\\Boot\\BCD",
    };

    let bcd_full_path = format!("{}{}", first_two_chars(bcd_disk), bcd_path);

    // Check if BCD exists
    if !std::path::Path::new(&bcd_full_path).exists() {
        warn!("BCD file not found: {}", bcd_full_path);
        return Ok(());
    }

    let bcd_disk_short = first_two_chars(bcd_disk);
    let osdevice_short = first_two_chars(osdevice);

    info!(
        "Fixing BCD: store={}, bootmgr={}, osdevice={}",
        bcd_full_path, bcd_disk_short, osdevice_short
    );

    // Set bootmgr device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{bootmgr}",
            "device",
            &format!("partition={}", bcd_disk_short),
        ],
    );

    // Set default device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{default}",
            "device",
            &format!("partition={}", osdevice_short),
        ],
    );

    // Set default osdevice
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{default}",
            "osdevice",
            &format!("partition={}", osdevice_short),
        ],
    );

    info!("BCD fix completed for typical mode");
    Ok(())
}

/// Fix BCD for VHD mode
/// Equivalent to BootFileOperation.BcdeditFixBootFileVHD()
pub fn bcdedit_fix_boot_file_vhd(
    bcd_disk: &str,
    osdevice: &str,
    vhd_filename: &str,
    fw_type: &FirmwareType,
) -> Result<()> {
    wait_for_bcdboot();

    let bcd_path = match fw_type {
        FirmwareType::UEFI => "\\EFI\\Microsoft\\Boot\\BCD",
        _ => "\\Boot\\BCD",
    };

    let bcd_full_path = format!("{}{}", first_two_chars(bcd_disk), bcd_path);

    if !std::path::Path::new(&bcd_full_path).exists() {
        warn!("BCD file not found: {}", bcd_full_path);
        return Ok(());
    }

    let bcd_disk_short = first_two_chars(bcd_disk);
    let osdevice_short = first_two_chars(osdevice);

    info!(
        "Fixing BCD for VHD: store={}, vhd={}",
        bcd_full_path, vhd_filename
    );

    // Set bootmgr device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{bootmgr}",
            "device",
            &format!("partition={}", bcd_disk_short),
        ],
    );

    // Set default device to VHD
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{default}",
            "device",
            &format!("vhd=[{}]\\{}", osdevice_short, vhd_filename),
        ],
    );

    // Set default osdevice to VHD
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store",
            &bcd_full_path,
            "/set",
            "{default}",
            "osdevice",
            &format!("vhd=[{}]\\{}", osdevice_short, vhd_filename),
        ],
    );

    info!("BCD fix completed for VHD mode");
    Ok(())
}

/// Write MBR, PBR, and activate using bootice
/// Equivalent to BootFileOperation.BooticeWriteMBRPBRAndAct()
pub fn bootice_write_mbr_pbr_and_act(target_disk: &str, app_files_path: &str) -> Result<()> {
    bootice_mbr(target_disk, app_files_path)?;
    bootice_pbr(target_disk, app_files_path)?;
    bootice_act(target_disk, app_files_path)?;
    Ok(())
}

/// Write MBR using bootice
/// Equivalent to BootFileOperation.BooticeMbr()
pub fn bootice_mbr(target_disk: &str, app_files_path: &str) -> Result<()> {
    let bootice_path = format!("{}\\BOOTICE.exe", app_files_path);

    if !std::path::Path::new(&bootice_path).exists() {
        warn!("BOOTICE.exe not found at {}", bootice_path);
        return Ok(());
    }

    let target = first_two_chars(target_disk);
    info!("Writing MBR to {}", target);

    let _ = CommandExecutor::execute_allow_fail(
        &bootice_path,
        &[
            &format!("/DEVICE={}", target),
            "/mbr",
            "/install",
            "/type=nt60",
            "/quiet",
        ],
    );

    Ok(())
}

/// Write PBR using bootice
/// Equivalent to BootFileOperation.BooticePbr()
pub fn bootice_pbr(target_disk: &str, app_files_path: &str) -> Result<()> {
    let bootice_path = format!("{}\\BOOTICE.exe", app_files_path);

    if !std::path::Path::new(&bootice_path).exists() {
        return Ok(());
    }

    let target = first_two_chars(target_disk);
    info!("Writing PBR to {}", target);

    let _ = CommandExecutor::execute_allow_fail(
        &bootice_path,
        &[
            &format!("/DEVICE={}", target),
            "/pbr",
            "/install",
            "/type=bootmgr",
            "/quiet",
        ],
    );

    Ok(())
}

/// Activate partition using bootice
/// Equivalent to BootFileOperation.BooticeAct()
pub fn bootice_act(target_disk: &str, app_files_path: &str) -> Result<()> {
    let bootice_path = format!("{}\\bootice.exe", app_files_path);

    if !std::path::Path::new(&bootice_path).exists() {
        return Ok(());
    }

    let target = first_two_chars(target_disk);
    info!("Activating partition {}", target);

    let _ = CommandExecutor::execute_allow_fail(
        &bootice_path,
        &[
            &format!("/DEVICE={}", target),
            "/partitions",
            "/activate",
            "/quiet",
        ],
    );

    Ok(())
}

/// Wait for bcdboot.exe to finish
fn wait_for_bcdboot() {
    for _ in 0..100 {
        // Check if bcdboot is still running
        #[cfg(target_os = "windows")]
        {
            let output = std::process::Command::new("tasklist")
                .args(&["/FI", "IMAGENAME eq bcdboot.exe", "/NH"])
                .output();

            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if !stdout.contains("bcdboot.exe") {
                    return;
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        return;

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
