//! Write orchestrator service - orchestrates the full WTG write process
//! Translated from CreateMain.cs and GoWrite.cs (Write class)
//! Contains the 7 write modes:
//! 1. UefiGptTypical
//! 2. UefiGptVhdVhdx
//! 3. NonUefiTypical
//! 4. NonUefiVhdVhdx
//! 5. UefiMbrTypical
//! 6. UefiMbrVhdVhdx

use crate::models::*;
use crate::services::{diskpart, image, boot, vhd};
use crate::utils::command::{self, CommandExecutor, wait_for_path};
use crate::utils::first_char;
use crate::{AppError, Result};
use std::time::Instant;
use tracing::{info, warn, error};

/// Main write orchestrator
/// Equivalent to CreateMain.GoWrite()
pub fn execute_write(config: &WtgConfig, app_files_path: &str) -> Result<WriteProgress> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let start_time = Instant::now();

    info!("Starting write operation: task_id={}", task_id);
    info!("Config: boot_mode={:?}, apply_mode={:?}", config.boot_mode, config.apply_mode);

    // Prevent system sleep
    command::prevent_sleep();

    // Kill any lingering processes
    let _ = CommandExecutor::kill_process("dism.exe");
    let _ = CommandExecutor::kill_process("diskpart.exe");

    let result = execute_write_inner(config, app_files_path, &task_id);

    // Restore system sleep
    command::restore_sleep();

    let elapsed = start_time.elapsed().as_secs();

    match result {
        Ok(()) => {
            // Handle no-default-drive-letter option
            if config.extra_features.no_default_drive_letter {
                let ud = &config.target_disk.volume;
                if !ud.is_empty() {
                    let _ = diskpart::set_no_default_drive_letter(ud);
                }
            }

            info!("Write completed successfully in {}s", elapsed);
            Ok(WriteProgress {
                task_id,
                status: WriteStatus::Completed,
                progress: 100.0,
                message: "Write completed successfully".to_string(),
                speed: 0.0,
                elapsed_seconds: elapsed,
                estimated_remaining_seconds: 0,
            })
        }
        Err(e) => {
            error!("Write failed: {}", e);
            Ok(WriteProgress {
                task_id,
                status: WriteStatus::Failed,
                progress: 0.0,
                message: format!("Write failed: {}", e),
                speed: 0.0,
                elapsed_seconds: elapsed,
                estimated_remaining_seconds: 0,
            })
        }
    }
}

/// Inner write logic (separates error handling from cleanup)
fn execute_write_inner(config: &WtgConfig, app_files_path: &str, _task_id: &str) -> Result<()> {
    // Validate critical fields
    let disk_index = &config.target_disk.index;
    let volume_letter = &config.target_disk.volume;
    let drive_type = &config.target_disk.drive_type;

    if disk_index.is_empty() {
        return Err(AppError::InvalidParameter(
            "Disk index is empty. The selected disk has no valid disk number.".to_string()
        ));
    }

    info!(
        "Write target: disk_index={}, volume={}, drive_type={}, device={}",
        disk_index, volume_letter, drive_type, config.target_disk.device
    );

    // For modes that need volume letter (Non-UEFI format/repartition), validate it
    let needs_volume = matches!(config.boot_mode, BootMode::NonUefi)
        && (config.extra_features.repartition || !config.extra_features.do_not_format);

    if needs_volume && volume_letter.is_empty() {
        return Err(AppError::InvalidParameter(
            "Volume letter is empty. The selected disk has no assigned drive letter. \
             Please assign a drive letter in Disk Management first.".to_string()
        ));
    }

    // Resolve actual image path — for ISO, mount and use the WIM/ESD inside
    let is_iso = config.image_path.to_lowercase().ends_with(".iso");
    let actual_image_path = if is_iso {
        info!("Image is ISO, mounting to extract WIM/ESD path...");
        image::mount_iso_and_find_wim(&config.image_path)?
    } else {
        config.image_path.clone()
    };
    info!("Actual image path for DISM: {}", actual_image_path);

    // Wrap the actual write in a closure so we can always dismount ISO on exit
    let result = execute_write_with_image(config, &actual_image_path, app_files_path);

    // Always dismount ISO if we mounted one
    if is_iso {
        image::dismount_iso(&config.image_path);
    }

    result
}

/// The actual write logic, using the resolved image path (WIM/ESD, not ISO)
fn execute_write_with_image(config: &WtgConfig, image_path: &str, app_files_path: &str) -> Result<()> {
    let disk_index = &config.target_disk.index;
    let volume_letter = &config.target_disk.volume;
    let drive_type = &config.target_disk.drive_type;

    // Build UD path
    let ud = if !volume_letter.is_empty() {
        format!("{}:\\", first_char(volume_letter))
    } else {
        String::new()
    };

    let partition_sizes: Vec<u32> = config.partition_config.extra_partition_sizes.clone();

    match config.boot_mode {
        BootMode::UefiGpt => {
            // UEFI + GPT mode
            info!("Starting UEFI+GPT partition");
            let resolved_ud = diskpart::diskpart_gpt_uefi(
                &config.efi_partition_size,
                disk_index,
                volume_letter,
                drive_type,
                &partition_sizes,
            )?;

            // Use the resolved volume path (diskpart may have auto-assigned a letter)
            let ud = if !resolved_ud.is_empty() { resolved_ud } else { ud };

            if !ud.is_empty() {
                wait_for_path(&ud, 100, 100);
            }

            // Get ESP letter — after partitioning, query for the FAT32 partition
            let esp_letter = resolve_esp_letter(disk_index, &config.efi_partition_path);

            match config.apply_mode {
                ApplyMode::Legacy => {
                    uefi_gpt_typical(config, image_path, &ud, &esp_letter, app_files_path)?;
                }
                ApplyMode::VHD | ApplyMode::VHDX => {
                    uefi_gpt_vhd_vhdx(config, image_path, &ud, &esp_letter, app_files_path)?;
                }
            }
        }
        BootMode::UefiMbr => {
            // UEFI + MBR mode
            info!("Starting UEFI+MBR partition");
            diskpart::diskpart_mbr_uefi(
                &config.efi_partition_size,
                disk_index,
                volume_letter,
                &partition_sizes,
                false,
            )?;

            // Resolve UD path after partitioning
            let ud = if ud.is_empty() {
                diskpart::resolve_volume_after_partition(disk_index)
            } else {
                wait_for_path(&ud, 100, 100);
                ud
            };

            let esp_letter = resolve_esp_letter(disk_index, &config.efi_partition_path);

            match config.apply_mode {
                ApplyMode::Legacy => {
                    uefi_mbr_typical(config, image_path, &ud, &esp_letter, app_files_path)?;
                }
                ApplyMode::VHD | ApplyMode::VHDX => {
                    uefi_mbr_vhd_vhdx(config, image_path, &ud, &esp_letter, app_files_path)?;
                }
            }
        }
        BootMode::NonUefi => {
            // Non-UEFI (Legacy) mode
            info!("Starting Non-UEFI partition");

            if config.extra_features.repartition {
                diskpart::diskpart_repartition(volume_letter, &partition_sizes)?;
            } else if !config.extra_features.do_not_format {
                diskpart::format_ntfs(volume_letter)?;
            }

            match config.apply_mode {
                ApplyMode::Legacy => {
                    non_uefi_typical(config, image_path, &ud, app_files_path)?;
                }
                ApplyMode::VHD | ApplyMode::VHDX => {
                    non_uefi_vhd_vhdx(config, image_path, &ud, app_files_path)?;
                }
            }
        }
    }

    Ok(())
}

// ============================================================
// Write Mode 1: UEFI + GPT Typical
// ============================================================
fn uefi_gpt_typical(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    esp_letter: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: UEFI+GPT Typical");

    // Enable Bitlocker if requested
    if config.extra_features.enable_bitlocker {
        enable_bitlocker(ud, app_files_path)?;
    }

    // Auto-choose WIM index and apply image
    let wim_index = image::auto_choose_wim_index(image_path, &config.wim_index)?;
    image::dism_apply_image(image_path, ud, &wim_index, config.extra_features.compact_os)?;

    // Verify system files
    if !image::verify_system_files(ud) {
        return Err(AppError::ImageError("System files not found after image apply".to_string()));
    }

    // Apply extra features
    image::image_extra(
        config.extra_features.install_dotnet35,
        config.extra_features.block_local_disk,
        config.extra_features.disable_winre,
        config.extra_features.skip_oobe,
        config.extra_features.disable_uasp,
        ud,
        image_path,
        app_files_path,
        config.extra_features.driver_path.as_deref(),
    )?;

    // Write boot files
    let esp = format!("{}:\\", first_char(esp_letter));
    boot::bcdboot_write_boot_file(ud, &esp, &FirmwareType::UEFI)?;
    boot::bcdedit_fix_boot_file_typical(&esp, ud, &FirmwareType::UEFI)?;

    // Remove ESP letter
    let _ = diskpart::remove_drive_letter(esp_letter);

    Ok(())
}

// ============================================================
// Write Mode 2: UEFI + GPT VHD/VHDX
// ============================================================
fn uefi_gpt_vhd_vhdx(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    esp_letter: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: UEFI+GPT VHD/VHDX");

    let vhd_config = config.vhd_config.as_ref()
        .ok_or_else(|| AppError::InvalidParameter("VHD config required for VHD mode".to_string()))?;

    execute_vhd_workflow(config, image_path, ud, Some(esp_letter), app_files_path, vhd_config)?;

    // Remove ESP letter
    let _ = diskpart::remove_drive_letter(esp_letter);

    // Verify VHD exists
    let vhd_filename = format!("{}.{}", vhd_config.filename, vhd_config.extension);
    let vhd_on_disk = format!("{}{}", ud, vhd_filename);
    if !std::path::Path::new(&vhd_on_disk).exists() {
        return Err(AppError::DiskError("VHD file not found on target disk".to_string()));
    }

    Ok(())
}

// ============================================================
// Write Mode 3: Non-UEFI Typical
// ============================================================
fn non_uefi_typical(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: Non-UEFI Typical");

    if config.extra_features.enable_bitlocker {
        enable_bitlocker(ud, app_files_path)?;
    }

    let wim_index = image::auto_choose_wim_index(image_path, &config.wim_index)?;

    image::image_apply(
        config.extra_features.wimboot,
        config.image_type == ImageType::Esd,
        true, // allow_esd
        "imagex_x86.exe",
        image_path,
        &wim_index,
        ud,
        ud,
        config.extra_features.compact_os,
    )?;

    // Apply extras
    image::image_extra(
        config.extra_features.install_dotnet35,
        config.extra_features.block_local_disk,
        config.extra_features.disable_winre,
        config.extra_features.skip_oobe,
        config.extra_features.disable_uasp,
        ud,
        image_path,
        app_files_path,
        config.extra_features.driver_path.as_deref(),
    )?;

    // Write boot files based on UEFI support
    if let Some(ref efi_part) = config.efi_partition_path {
        if std::path::Path::new(efi_part).exists() {
            boot::bcdboot_write_boot_file(ud, efi_part, &FirmwareType::ALL)?;
            boot::bcdedit_fix_boot_file_typical(ud, efi_part, &FirmwareType::BIOS)?;
            boot::bcdedit_fix_boot_file_typical(ud, efi_part, &FirmwareType::UEFI)?;
        }
    } else if config.extra_features.ntfs_uefi_support {
        boot::bcdboot_write_boot_file(ud, ud, &FirmwareType::ALL)?;
        boot::bcdedit_fix_boot_file_typical(ud, ud, &FirmwareType::BIOS)?;
        boot::bcdedit_fix_boot_file_typical(ud, ud, &FirmwareType::UEFI)?;
    } else {
        boot::bcdboot_write_boot_file(ud, ud, &FirmwareType::BIOS)?;
        boot::bcdedit_fix_boot_file_typical(ud, ud, &FirmwareType::BIOS)?;
    }

    // Write MBR/PBR and activate
    if let Some(ref efi_part) = config.efi_partition_path {
        boot::bootice_write_mbr_pbr_and_act(efi_part, app_files_path)?;
    } else {
        boot::bootice_write_mbr_pbr_and_act(ud, app_files_path)?;
    }

    // Verify boot files
    let bootmgr_path = format!("{}bootmgr", ud);
    if !std::path::Path::new(&bootmgr_path).exists() {
        return Err(AppError::ImageError("bootmgr not found - boot file write may have failed".to_string()));
    }

    Ok(())
}

// ============================================================
// Write Mode 4: Non-UEFI VHD/VHDX
// ============================================================
fn non_uefi_vhd_vhdx(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: Non-UEFI VHD/VHDX");

    let vhd_config = config.vhd_config.as_ref()
        .ok_or_else(|| AppError::InvalidParameter("VHD config required for VHD mode".to_string()))?;

    execute_vhd_workflow(config, image_path, ud, None, app_files_path, vhd_config)?;

    // Write MBR/PBR
    boot::bootice_write_mbr_pbr_and_act(ud, app_files_path)?;

    // Verify
    let vhd_filename = format!("{}.{}", vhd_config.filename, vhd_config.extension);
    let vhd_on_disk = format!("{}{}", ud, vhd_filename);
    if !std::path::Path::new(&vhd_on_disk).exists() {
        return Err(AppError::DiskError("VHD file not found".to_string()));
    }

    Ok(())
}

// ============================================================
// Write Mode 5: UEFI + MBR Typical
// ============================================================
fn uefi_mbr_typical(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    esp_letter: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: UEFI+MBR Typical");

    let wim_index = image::auto_choose_wim_index(image_path, &config.wim_index)?;

    if config.extra_features.enable_bitlocker {
        enable_bitlocker(ud, app_files_path)?;
    }

    // Apply image
    image::image_apply(
        config.extra_features.wimboot,
        config.image_type == ImageType::Esd,
        true,
        "imagex_x86.exe",
        image_path,
        &wim_index,
        ud,
        ud,
        config.extra_features.compact_os,
    )?;

    if !image::verify_system_files(ud) {
        return Err(AppError::ImageError("System files verification failed".to_string()));
    }

    // Apply extras
    image::image_extra(
        config.extra_features.install_dotnet35,
        config.extra_features.block_local_disk,
        config.extra_features.disable_winre,
        config.extra_features.skip_oobe,
        config.extra_features.disable_uasp,
        ud,
        image_path,
        app_files_path,
        config.extra_features.driver_path.as_deref(),
    )?;

    // Write boot files to ESP
    let esp = format!("{}:\\", first_char(esp_letter));
    boot::bcdboot_write_boot_file(ud, &esp, &FirmwareType::ALL)?;
    boot::bcdedit_fix_boot_file_typical(&esp, ud, &FirmwareType::UEFI)?;

    // Remove ESP letter
    let _ = diskpart::remove_drive_letter(esp_letter);

    Ok(())
}

// ============================================================
// Write Mode 6: UEFI + MBR VHD/VHDX
// ============================================================
fn uefi_mbr_vhd_vhdx(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    esp_letter: &str,
    app_files_path: &str,
) -> Result<()> {
    info!("Write mode: UEFI+MBR VHD/VHDX");

    let vhd_config = config.vhd_config.as_ref()
        .ok_or_else(|| AppError::InvalidParameter("VHD config required for VHD mode".to_string()))?;

    execute_vhd_workflow(config, image_path, ud, Some(esp_letter), app_files_path, vhd_config)?;

    // Remove ESP letter
    let _ = diskpart::remove_drive_letter(esp_letter);

    // Verify
    let vhd_filename = format!("{}.{}", vhd_config.filename, vhd_config.extension);
    let vhd_on_disk = format!("{}{}", ud, vhd_filename);
    if !std::path::Path::new(&vhd_on_disk).exists() {
        return Err(AppError::DiskError("VHD file not found".to_string()));
    }

    Ok(())
}

// ============================================================
// VHD Workflow (shared logic)
// ============================================================
fn execute_vhd_workflow(
    config: &WtgConfig,
    image_path: &str,
    ud: &str,
    esp_letter: Option<&str>,
    app_files_path: &str,
    vhd_config: &VhdConfig,
) -> Result<()> {
    let vhd_filename = format!("{}.{}", vhd_config.filename, vhd_config.extension);
    let image_type = match config.image_type {
        ImageType::Vhd => "vhd",
        ImageType::Vhdx => "vhdx",
        _ => "",
    };

    // Create VHD operation context
    let vhd_op = vhd::VhdOperation::new(
        image_type,
        image_path,
        vhd_config.vhd_type == VhdType::Fixed,
        vhd_config.size_mb,
        ud,
        &std::env::temp_dir().to_string_lossy(),
        &vhd_config.filename,
        &vhd_config.extension,
        &config.boot_mode,
        config.extra_features.wimboot,
        false,
        config.extra_features.ntfs_uefi_support,
    );

    // Clean temp
    vhd::clean_temp(&vhd_config.filename)?;

    if image_type == "vhd" || image_type == "vhdx" {
        // Import existing VHD
        vhd::copy_vhd(image_path, ud, &vhd_config.extension)?;
        vhd::twice_attach_and_write_boot(ud, &vhd_filename, config.extra_features.ntfs_uefi_support)?;
    } else {
        // Create new VHD
        let vhd_type_str = if vhd_config.vhd_type == VhdType::Fixed { "fixed" } else { "expandable" };

        vhd::create_vhd(
            &vhd_op.vhd_path,
            vhd_type_str,
            &vhd_op.vhd_size,
            image_type,
            vhd_config.partition_type,
        )?;

        // Enable Bitlocker on VHD if requested
        if config.extra_features.enable_bitlocker {
            enable_bitlocker("V:", app_files_path)?;
        }

        // Apply image to VHD
        let wim_index = image::auto_choose_wim_index(image_path, &config.wim_index)?;
        image::image_apply(
            config.extra_features.wimboot,
            config.image_type == ImageType::Esd,
            true,
            "imagex_x86.exe",
            image_path,
            &wim_index,
            "V:\\",
            ud,
            config.extra_features.compact_os,
        )?;

        // Apply extras to VHD
        image::image_extra(
            config.extra_features.install_dotnet35,
            config.extra_features.block_local_disk,
            config.extra_features.disable_winre,
            config.extra_features.skip_oobe,
            config.extra_features.disable_uasp,
            "V:\\",
            image_path,
            app_files_path,
            config.extra_features.driver_path.as_deref(),
        )?;

        // Fix drive letter for VHD
        if config.extra_features.fix_letter {
            let _ = image::fix_letter("C:", "V:");
        }

        // Write boot files if not copying (direct on USB)
        if !vhd_op.need_copy {
            write_vhd_boot_files(config, ud, esp_letter, &vhd_filename, app_files_path)?;
        }

        // Detach VHD
        vhd::detach_vhd(&vhd_op.vhd_path)?;

        // Copy VHD to USB if needed
        if vhd_op.need_copy {
            vhd::copy_vhd(&vhd_op.vhd_path, ud, &vhd_config.extension)?;
            std::thread::sleep(std::time::Duration::from_millis(1500));
            vhd::twice_attach_and_write_boot(ud, &vhd_filename, config.extra_features.ntfs_uefi_support)?;
        }

        // Write dynamic size instruction
        if vhd_config.vhd_type != VhdType::Fixed {
            let _ = vhd::write_dynamic_size_instruction(ud, &vhd_op.vhd_size);
        }

        // Fix BCD for VHD
        fix_vhd_bcd(config, ud, esp_letter, &vhd_filename)?;
    }

    Ok(())
}

/// Write boot files for VHD into USB drive
fn write_vhd_boot_files(
    config: &WtgConfig,
    ud: &str,
    esp_letter: Option<&str>,
    vhd_filename: &str,
    app_files_path: &str,
) -> Result<()> {
    match config.boot_mode {
        BootMode::UefiGpt => {
            if let Some(esp) = esp_letter {
                let esp_path = format!("{}:\\", first_char(esp));
                boot::bcdboot_write_boot_file("V:\\", &esp_path, &FirmwareType::UEFI)?;
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::UEFI)?;
            }
        }
        BootMode::UefiMbr => {
            if let Some(esp) = esp_letter {
                let esp_path = format!("{}:\\", first_char(esp));
                boot::bcdboot_write_boot_file("V:\\", &esp_path, &FirmwareType::ALL)?;
                boot::bootice_write_mbr_pbr_and_act(&esp_path, app_files_path)?;
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::UEFI)?;
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::BIOS)?;
            }
        }
        BootMode::NonUefi => {
            if let Some(ref efi_part) = config.efi_partition_path {
                if std::path::Path::new(efi_part).exists() {
                    boot::bcdboot_write_boot_file("V:\\", efi_part, &FirmwareType::ALL)?;
                    boot::bcdedit_fix_boot_file_vhd(ud, efi_part, vhd_filename, &FirmwareType::BIOS)?;
                    boot::bcdedit_fix_boot_file_vhd(ud, efi_part, vhd_filename, &FirmwareType::UEFI)?;
                }
            } else if config.extra_features.ntfs_uefi_support {
                boot::bcdboot_write_boot_file("V:\\", ud, &FirmwareType::ALL)?;
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::BIOS)?;
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::UEFI)?;
            } else {
                boot::bcdboot_write_boot_file("V:\\", ud, &FirmwareType::BIOS)?;
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::BIOS)?;
            }
        }
    }
    Ok(())
}

/// Fix BCD entries for VHD after copy
fn fix_vhd_bcd(
    config: &WtgConfig,
    ud: &str,
    esp_letter: Option<&str>,
    vhd_filename: &str,
) -> Result<()> {
    match config.boot_mode {
        BootMode::UefiGpt => {
            if let Some(esp) = esp_letter {
                let esp_path = format!("{}:\\", first_char(esp));
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::UEFI)?;
            }
        }
        BootMode::UefiMbr => {
            if let Some(esp) = esp_letter {
                let esp_path = format!("{}:\\", first_char(esp));
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::UEFI)?;
                boot::bcdedit_fix_boot_file_vhd(&esp_path, ud, vhd_filename, &FirmwareType::BIOS)?;
            }
        }
        BootMode::NonUefi => {
            if config.extra_features.ntfs_uefi_support {
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::BIOS)?;
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::UEFI)?;
            } else {
                boot::bcdedit_fix_boot_file_vhd(ud, ud, vhd_filename, &FirmwareType::BIOS)?;
            }
        }
    }
    Ok(())
}

/// Resolve ESP (EFI System Partition) letter after disk partitioning.
/// If user specified one, use it. Otherwise, query via PowerShell.
fn resolve_esp_letter(disk_index: &str, user_efi_path: &Option<String>) -> String {
    if let Some(ref path) = user_efi_path {
        if !path.is_empty() {
            return path.clone();
        }
    }

    // Query for FAT32 volumes on this disk (the ESP)
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = CommandExecutor::execute_allow_fail(
            "powershell.exe",
            &["-NoProfile", "-Command",
              &format!(
                  "Get-Partition -DiskNumber {} -ErrorAction SilentlyContinue | \
                   Get-Volume -ErrorAction SilentlyContinue | \
                   Where-Object {{ $_.FileSystemType -eq 'FAT32' -and $_.DriveLetter }} | \
                   Select-Object -First 1 -ExpandProperty DriveLetter",
                  disk_index
              )],
        ) {
            let letter = output.trim().to_string();
            if !letter.is_empty() && letter.len() == 1 {
                info!("Found ESP letter: {}", letter);
                return format!("{}:", letter);
            }
        }
    }

    // Fallback
    info!("Could not detect ESP letter, using X:");
    "X:".to_string()
}

/// Enable Bitlocker on a target drive
fn enable_bitlocker(target: &str, app_files_path: &str) -> Result<()> {
    info!("Enabling Bitlocker on {}", target);

    #[cfg(target_os = "windows")]
    {
        let exe_name = if cfg!(target_pointer_width = "64") {
            "BitlockerConfig_x64.exe"
        } else {
            "BitlockerConfig_x86.exe"
        };

        let exe_path = format!("{}\\{}", app_files_path, exe_name);
        if std::path::Path::new(&exe_path).exists() {
            let _ = CommandExecutor::execute_allow_fail(&exe_path, &[target]);
        } else {
            warn!("Bitlocker config tool not found: {}", exe_path);
        }
    }

    Ok(())
}
