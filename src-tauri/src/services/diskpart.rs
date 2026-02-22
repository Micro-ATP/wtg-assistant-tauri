//! Diskpart service - manages disk partitioning operations
//! Translated from DiskOperation.cs

#![allow(dead_code)]

use crate::utils::command::{run_diskpart_script, wait_for_path, CommandExecutor};
use crate::Result;

/// Generate and execute GPT + UEFI partition script
/// Equivalent to DiskOperation.DiskPartGPTAndUEFI()
pub fn diskpart_gpt_uefi(
    efi_size: &str,
    disk_index: &str,
    volume_letter: &str,
    drive_type: &str,
    partition_sizes: &[u32],
) -> Result<()> {
    let mut script = String::new();
    let is_removable = drive_type.contains("Removable");

    script.push_str(&format!("select disk {}\n", disk_index));
    script.push_str("clean\n");
    script.push_str("convert gpt NOERR\n");

    // Removable devices can't create EFI partition properly
    if is_removable {
        script.push_str(&format!("create partition primary size {}\n", efi_size));
    } else {
        script.push_str(&format!("create partition efi size {}\n", efi_size));
    }

    // Filter non-zero partition sizes
    let valid_sizes: Vec<u32> = partition_sizes.iter().copied().filter(|&s| s > 0).collect();

    // Create intermediate partitions (all except last)
    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("create partition primary size {}\n", valid_sizes[i]));
    }

    // Create last partition (uses remaining space)
    script.push_str("create partition primary\n");

    // Select and format the main data partition
    if is_removable {
        script.push_str("select partition 2\n");
    } else {
        script.push_str("select partition 3\n");
    }
    script.push_str("format fs=ntfs quick\n");
    script.push_str(&format!("assign letter={}\n", &volume_letter[..1]));

    // Format additional partitions
    let start_partition = if is_removable { 3 } else { 4 };
    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("select partition {}\n", start_partition + i));
        script.push_str("format fs=ntfs quick\n");
        script.push_str("assign\n");
    }

    // Format EFI partition as FAT32
    if is_removable {
        script.push_str("select partition 1\n");
        script.push_str("remove NOERR\n");
    } else {
        script.push_str("select partition 2\n");
    }
    script.push_str("format fs=fat32 quick\n");
    script.push_str("assign\n");
    script.push_str("exit\n");

    run_diskpart_script(&script)?;

    // Wait for the disk to become available
    let disk_path = format!("{}:\\", &volume_letter[..1]);
    wait_for_path(&disk_path, 100, 100);

    Ok(())
}

/// Generate and execute MBR + UEFI partition script
/// Equivalent to DiskOperation.DiskPartMBRAndUEFI()
pub fn diskpart_mbr_uefi(
    efi_size: &str,
    disk_index: &str,
    volume_letter: &str,
    partition_sizes: &[u32],
    keep_drive_letter: bool,
) -> Result<()> {
    let mut script = String::new();

    script.push_str(&format!("select disk {}\n", disk_index));
    script.push_str("clean\n");
    script.push_str("convert mbr\n");
    script.push_str(&format!("create partition primary size {}\n", efi_size));

    let valid_sizes: Vec<u32> = partition_sizes.iter().copied().filter(|&s| s > 0).collect();

    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("create partition primary size {}\n", valid_sizes[i]));
    }

    script.push_str("create partition primary\n");
    script.push_str("select partition 2\n");
    script.push_str("remove noerr\n");
    script.push_str("format fs=ntfs quick\n");

    if keep_drive_letter {
        script.push_str(&format!("assign letter={}\n", &volume_letter[..1]));
    } else {
        script.push_str("assign\n");
    }

    // Format additional partitions
    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("select partition {}\n", i + 3));
        script.push_str("format fs=ntfs quick\n");
        script.push_str("assign\n");
    }

    // Format EFI partition as FAT32, set active
    script.push_str("select partition 1\n");
    script.push_str("remove noerr\n");
    script.push_str("format fs=fat32 quick\n");
    script.push_str("active\n");
    script.push_str("assign\n");
    script.push_str("exit\n");

    run_diskpart_script(&script)?;
    Ok(())
}

/// Re-partition USB disk with MBR layout
/// Equivalent to DiskOperation.DiskPartRePartitionUD()
pub fn diskpart_repartition(
    volume_letter: &str,
    partition_sizes: &[u32],
) -> Result<()> {
    let mut script = String::new();

    script.push_str(&format!("select volume {}\n", &volume_letter[..1]));
    script.push_str("clean\n");
    script.push_str("convert mbr\n");

    let valid_sizes: Vec<u32> = partition_sizes.iter().copied().filter(|&s| s > 0).collect();

    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("create partition primary size {}\n", valid_sizes[i]));
    }

    script.push_str("create partition primary\n");
    script.push_str("select partition 1\n");
    script.push_str("format fs=ntfs quick\n");
    script.push_str("active\n");
    script.push_str(&format!("assign letter={}\n", &volume_letter[..1]));

    for i in 0..valid_sizes.len().saturating_sub(1) {
        script.push_str(&format!("select partition {}\n", i + 2));
        script.push_str("format fs=ntfs quick\n");
        script.push_str("assign\n");
    }

    script.push_str("exit\n");

    run_diskpart_script(&script)?;

    let disk_path = format!("{}:\\", &volume_letter[..1]);
    wait_for_path(&disk_path, 100, 100);

    Ok(())
}

/// Repartition and auto-assign drive letter
/// Equivalent to DiskOperation.RepartitionAndAutoAssignDriveLetter()
pub fn repartition_auto_assign(disk_index: &str) -> Result<()> {
    let script = format!(
        "select disk {}\nclean\ncreate partition primary\nselect partition 1\nformat fs=ntfs quick\nactive\nassign\nexit\n",
        disk_index
    );
    run_diskpart_script(&script)?;
    Ok(())
}

/// Assign drive letter to a disk
pub fn assign_drive_letter(disk_index: &str, letter: &str) -> Result<()> {
    let script = format!(
        "select disk {}\nclean\ncreate partition primary\nselect partition 1\nformat fs=ntfs quick\nassign letter={}\nexit\n",
        disk_index, letter
    );
    run_diskpart_script(&script)?;
    Ok(())
}

/// Set no default drive letter attribute
pub fn set_no_default_drive_letter(volume_letter: &str) -> Result<()> {
    let script = format!(
        "select volume {}\nattributes volume set nodefaultdriveletter\n",
        &volume_letter[..1]
    );
    run_diskpart_script(&script)?;
    Ok(())
}

/// Remove drive letter from a volume
pub fn remove_drive_letter(volume_letter: &str) -> Result<()> {
    let script = format!(
        "select volume {}\nremove\nexit\n",
        &volume_letter[..1]
    );
    run_diskpart_script(&script)?;
    Ok(())
}

/// Quick format a drive as NTFS
#[cfg(target_os = "windows")]
pub fn format_ntfs(drive_letter: &str) -> Result<()> {
    let cmd = format!("format {}:/FS:ntfs /q /V: /Y", &drive_letter[..1]);
    CommandExecutor::run_cmd(&cmd)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn format_ntfs(_drive_letter: &str) -> Result<()> {
    Err(AppError::SystemError("Format is only available on Windows".to_string()))
}
