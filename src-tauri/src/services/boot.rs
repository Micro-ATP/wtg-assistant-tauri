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

    let fw_flag = match fw_type {
        FirmwareType::ALL => "/f all",
        FirmwareType::BIOS => "/f bios",
        FirmwareType::UEFI => "/f uefi",
    };

    let args = format!(
        "{}windows /s {} {} /l zh-CN /v",
        source_disk, target, fw_flag
    );

    info!("Running bcdboot: {}", args);

    // Try system bcdboot first
    let result = CommandExecutor::execute_allow_fail("bcdboot.exe", &args.split_whitespace().collect::<Vec<_>>());

    match result {
        Ok(output) => {
            info!("bcdboot output: {}", output);
            Ok(())
        }
        Err(e) => {
            warn!("bcdboot failed: {}", e);
            Err(AppError::CommandFailed(format!("bcdboot failed: {}", e)))
        }
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

    info!("Fixing BCD: store={}, bootmgr={}, osdevice={}", bcd_full_path, bcd_disk_short, osdevice_short);

    // Set bootmgr device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{bootmgr}", "device",
            &format!("partition={}", bcd_disk_short),
        ],
    );

    // Set default device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{default}", "device",
            &format!("partition={}", osdevice_short),
        ],
    );

    // Set default osdevice
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{default}", "osdevice",
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

    info!("Fixing BCD for VHD: store={}, vhd={}", bcd_full_path, vhd_filename);

    // Set bootmgr device
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{bootmgr}", "device",
            &format!("partition={}", bcd_disk_short),
        ],
    );

    // Set default device to VHD
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{default}", "device",
            &format!("vhd=[{}]\\{}", osdevice_short, vhd_filename),
        ],
    );

    // Set default osdevice to VHD
    let _ = CommandExecutor::execute_allow_fail(
        "bcdedit.exe",
        &[
            "/store", &bcd_full_path,
            "/set", "{default}", "osdevice",
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
