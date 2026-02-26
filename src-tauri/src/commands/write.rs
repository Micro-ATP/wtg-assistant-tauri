//! Write commands - Tauri command handlers for write operations

use crate::models::{Disk, ImageInfo, WriteProgress, WtgConfig};
#[cfg(target_os = "macos")]
use crate::models::ApplyMode;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::services;
#[cfg(target_os = "macos")]
use crate::utils::macos_admin;
use crate::utils::progress::PROGRESS_REPORTER;
use crate::utils::task_manager;
use crate::AppError;
use crate::Result;
use serde::Serialize;
use tracing::info;

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

#[cfg(target_os = "macos")]
fn map_writable_check(
    check: crate::services::write_macos::MacosTargetWritableCheck,
) -> MacosTargetWritableCheck {
    MacosTargetWritableCheck {
        supported: check.supported,
        disk_id: check.disk_id,
        partition_id: check.partition_id,
        mount_point: check.mount_point,
        filesystem: check.filesystem,
        writable_volume: check.writable_volume,
        dir_writable: check.dir_writable,
        writable: check.writable,
        needs_ntfs_remount: check.needs_ntfs_remount,
        reason: check.reason,
    }
}

/// Get image index information from a WIM/ESD file
#[tauri::command]
pub async fn get_image_info(image_path: String) -> Result<Vec<ImageInfo>> {
    info!("Getting image info for: {}", image_path);

    #[cfg(target_os = "windows")]
    {
        let info =
            tokio::task::spawn_blocking(move || services::image::get_image_info(&image_path))
                .await
                .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

        return Ok(info);
    }

    #[cfg(target_os = "macos")]
    {
        let image_path_for_task = image_path.clone();
        let info = tokio::task::spawn_blocking(move || {
            services::write_macos::get_image_info(&image_path_for_task)
        })
        .await
        .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

        return Ok(info);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = image_path;
        Err(AppError::Unsupported(
            "Image parsing is currently implemented on Windows/macOS only".to_string(),
        ))
    }
}

/// Start a WTG write operation
#[tauri::command]
pub async fn start_write(config: WtgConfig, app_handle: tauri::AppHandle) -> Result<WriteProgress> {
    info!(
        "Starting write operation with config: {:?}",
        config.boot_mode
    );

    #[cfg(target_os = "windows")]
    {
        // Set app handle for progress reporting
        PROGRESS_REPORTER.set_app_handle(app_handle);

        // Determine app files path
        let app_files_path = std::env::temp_dir()
            .join("WTGA")
            .to_string_lossy()
            .to_string();
        let _ = std::fs::create_dir_all(&app_files_path);

        let progress = tokio::task::spawn_blocking(move || {
            services::write::execute_write(&config, &app_files_path)
        })
        .await
        .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

        return Ok(progress);
    }

    #[cfg(target_os = "macos")]
    {
        if matches!(config.apply_mode, ApplyMode::VHD | ApplyMode::VHDX) {
            return Err(AppError::Unsupported(
                "VHD/VHDX apply mode is not supported on macOS yet".to_string(),
            ));
        }

        PROGRESS_REPORTER.set_app_handle(app_handle);

        let app_files_path = std::env::temp_dir()
            .join("WTGA")
            .to_string_lossy()
            .to_string();
        let _ = std::fs::create_dir_all(&app_files_path);

        let progress = tokio::task::spawn_blocking(move || {
            services::write_macos::execute_write(&config, &app_files_path)
        })
        .await
        .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

        return Ok(progress);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (config, app_handle);
        Err(AppError::Unsupported(
            "Write operation is currently implemented on Windows/macOS only".to_string(),
        ))
    }
}

/// Cancel a running write operation
#[tauri::command]
pub async fn cancel_write(task_id: String) -> Result<()> {
    info!("Cancelling write operation: {}", task_id);

    #[cfg(target_os = "windows")]
    {
        let cancelled = if task_id.trim().is_empty() {
            task_manager::TaskManager::cancel_all_tasks() > 0
        } else {
            task_manager::TaskManager::cancel_task(&task_id)
        };

        let _ = crate::utils::command::CommandExecutor::kill_process("dism.exe");
        let _ = crate::utils::command::CommandExecutor::kill_process("diskpart.exe");
        let _ = crate::utils::command::CommandExecutor::kill_process("imagex_x86.exe");
        let _ = crate::utils::command::CommandExecutor::kill_process("imagex_amd64.exe");

        if cancelled {
            if task_id.trim().is_empty() {
                info!("Cancellation signal set for all active tasks");
            } else {
                info!("Task cancellation signal set for: {}", task_id);
            }
        } else if task_id.trim().is_empty() {
            info!("No active task found for cancellation");
        } else {
            info!("Task not found for cancellation: {}", task_id);
        }

        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let cancelled = if task_id.trim().is_empty() {
            task_manager::TaskManager::cancel_all_tasks() > 0
        } else {
            task_manager::TaskManager::cancel_task(&task_id)
        };

        for pattern in [
            "wimlib-imagex apply",
            "wimlib-imagex",
            "mkntfs",
            "mkfs.ntfs",
            "ntfs-3g",
            "diskutil",
            "hdiutil",
            "osascript",
        ] {
            let _ = crate::utils::command::CommandExecutor::kill_process(pattern);
        }

        let _ = macos_admin::run_shell_with_auto_privilege(
            "pkill -9 -f 'wimlib-imagex apply' >/dev/null 2>&1 || true; \
             pkill -9 -f 'wimlib-imagex' >/dev/null 2>&1 || true; \
             pkill -9 -f 'mkntfs' >/dev/null 2>&1 || true; \
             pkill -9 -f 'mkfs.ntfs' >/dev/null 2>&1 || true; \
             pkill -9 -f 'ntfs-3g' >/dev/null 2>&1 || true; \
             pkill -9 -f 'diskutil' >/dev/null 2>&1 || true; \
             pkill -9 -f 'hdiutil' >/dev/null 2>&1 || true",
        );

        if !task_id.trim().is_empty() {
            PROGRESS_REPORTER.report_status(&task_id, 0.0, "Write cancelled by user", "cancelled");
        }

        if cancelled {
            info!("macOS cancellation signal set for task: {}", task_id);
        } else {
            info!(
                "macOS cancellation requested; no tracked task matched, but process termination was attempted."
            );
        }

        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = task_id;
        Err(AppError::Unsupported(
            "Write cancellation is currently implemented on Windows/macOS only".to_string(),
        ))
    }
}

/// Verify system files on a target disk
#[tauri::command]
pub async fn verify_system_files(target_disk: String) -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        return Ok(services::image::verify_system_files(&target_disk));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = target_disk;
        Err(AppError::Unsupported(
            "System file verification is currently implemented on Windows only".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn check_macos_target_writable(target_disk: Disk) -> Result<MacosTargetWritableCheck> {
    #[cfg(target_os = "macos")]
    {
        let disk_for_task = target_disk.clone();
        let check =
            tokio::task::spawn_blocking(move || services::write_macos::check_target_writable(&disk_for_task))
                .await
                .map_err(|e| crate::AppError::SystemError(e.to_string()))??;
        return Ok(map_writable_check(check));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = target_disk;
        Err(AppError::Unsupported(
            "Target writable pre-check is only available on macOS".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn remount_macos_target_ntfs_writable(
    target_disk: Disk,
) -> Result<MacosTargetWritableCheck> {
    #[cfg(target_os = "macos")]
    {
        let disk_for_task = target_disk.clone();
        let check = tokio::task::spawn_blocking(move || {
            services::write_macos::remount_target_ntfs_writable(&disk_for_task)
        })
        .await
        .map_err(|e| crate::AppError::SystemError(e.to_string()))??;
        return Ok(map_writable_check(check));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = target_disk;
        Err(AppError::Unsupported(
            "NTFS remount helper is only available on macOS".to_string(),
        ))
    }
}
