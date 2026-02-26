use crate::models::FirmwareType;
use crate::services::{boot, diskpart};
use crate::utils::command::CommandExecutor;
use crate::{AppError, Result};

fn normalize_drive_root(input: &str) -> Result<String> {
    let trimmed = input.trim().trim_end_matches('\\').trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::InvalidParameter(
            "Drive letter is required, e.g. E or E:".to_string(),
        ));
    }

    let letter = trimmed
        .chars()
        .next()
        .ok_or_else(|| AppError::InvalidParameter("Invalid drive letter".to_string()))?;

    if !letter.is_ascii_alphabetic() {
        return Err(AppError::InvalidParameter(
            "Drive letter must start with A-Z".to_string(),
        ));
    }

    Ok(format!("{}:\\", letter.to_ascii_uppercase()))
}

fn parse_firmware(input: &str) -> Result<FirmwareType> {
    match input.trim().to_ascii_lowercase().as_str() {
        "uefi" => Ok(FirmwareType::UEFI),
        "bios" => Ok(FirmwareType::BIOS),
        "all" => Ok(FirmwareType::ALL),
        _ => Err(AppError::InvalidParameter(
            "Firmware must be one of: uefi, bios, all".to_string(),
        )),
    }
}

fn resolve_disk_number_from_drive(drive_root: &str) -> Result<String> {
    let drive_letter = drive_root
        .chars()
        .next()
        .ok_or_else(|| AppError::InvalidParameter("Invalid target drive".to_string()))?
        .to_ascii_uppercase();

    let ps = format!(
        "(Get-Partition -DriveLetter {letter} -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty DiskNumber)",
        letter = drive_letter
    );

    let output =
        CommandExecutor::execute_allow_fail("powershell.exe", &["-NoProfile", "-Command", &ps])?;

    let disk_no = output.trim().to_string();
    if disk_no.is_empty() {
        return Err(AppError::DeviceNotFound(format!(
            "Cannot resolve disk number for {}",
            drive_root
        )));
    }

    Ok(disk_no)
}

#[tauri::command]
pub async fn repair_boot(target_disk: String, firmware: String) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        let target_root = normalize_drive_root(&target_disk)?;
        let fw_type = parse_firmware(&firmware)?;
        let disk_no = resolve_disk_number_from_drive(&target_root)?;

        if !std::path::Path::new(&target_root).exists() {
            return Err(AppError::DeviceNotFound(format!(
                "Target path not found: {}",
                target_root
            )));
        }

        let target_root_for_task = target_root.clone();
        let disk_no_for_task = disk_no.clone();
        let firmware_for_msg = firmware.to_ascii_uppercase();
        let result = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut mounted_efi: Option<String> = None;
            let mut mounted_efi_temporary = false;

            let run_uefi = matches!(fw_type, FirmwareType::UEFI | FirmwareType::ALL);
            if run_uefi {
                let (esp_letter, temporary) = diskpart::mount_efi_partition(&disk_no_for_task)?;
                mounted_efi = Some(format!("{}\\", esp_letter));
                mounted_efi_temporary = temporary;
            }

            let op_result: Result<()> = match fw_type {
                FirmwareType::UEFI => {
                    let esp = mounted_efi
                        .as_ref()
                        .ok_or_else(|| AppError::DiskError("EFI mount failed".to_string()))?;
                    boot::bcdboot_write_boot_file(&target_root_for_task, esp, &FirmwareType::UEFI)?;
                    boot::bcdedit_fix_boot_file_typical(
                        esp,
                        &target_root_for_task,
                        &FirmwareType::UEFI,
                    )?;
                    Ok(())
                }
                FirmwareType::BIOS => {
                    boot::bcdboot_write_boot_file(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    boot::bcdedit_fix_boot_file_typical(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    Ok(())
                }
                FirmwareType::ALL => {
                    boot::bcdboot_write_boot_file(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    boot::bcdedit_fix_boot_file_typical(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;

                    let esp = mounted_efi
                        .as_ref()
                        .ok_or_else(|| AppError::DiskError("EFI mount failed".to_string()))?;
                    boot::bcdboot_write_boot_file(&target_root_for_task, esp, &FirmwareType::UEFI)?;
                    boot::bcdedit_fix_boot_file_typical(
                        esp,
                        &target_root_for_task,
                        &FirmwareType::UEFI,
                    )?;
                    Ok(())
                }
            };

            if mounted_efi_temporary {
                if let Some(esp) = mounted_efi {
                    let _ = diskpart::remove_drive_letter(&esp);
                }
            }

            op_result
        })
        .await
        .map_err(|e| AppError::SystemError(e.to_string()))?;

        result?;
        Ok(format!(
            "Boot repair completed for {} ({})",
            target_root, firmware_for_msg
        ))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (target_disk, firmware);
        Err(AppError::Unsupported(
            "Boot repair is currently implemented on Windows only".to_string(),
        ))
    }
}
