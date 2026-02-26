//! Write commands - Tauri command handlers for write operations

use crate::models::{ImageInfo, WriteProgress, WtgConfig};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::services;
use crate::utils::progress::PROGRESS_REPORTER;
use crate::utils::task_manager;
#[cfg(not(target_os = "windows"))]
use crate::AppError;
use crate::Result;
use tracing::info;

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

    #[cfg(not(target_os = "windows"))]
    {
        let _ = task_id;
        return Err(AppError::Unsupported(
            "Write cancellation is currently implemented on Windows only".to_string(),
        ));
    }

    // Set cancellation flag(s): cancel the target task, or all tasks when ID is empty.
    let cancelled = if task_id.trim().is_empty() {
        task_manager::TaskManager::cancel_all_tasks() > 0
    } else {
        task_manager::TaskManager::cancel_task(&task_id)
    };

    // Kill active processes to interrupt the operation
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
    } else {
        if task_id.trim().is_empty() {
            info!("No active task found for cancellation");
        } else {
            info!("Task not found for cancellation: {}", task_id);
        }
    }

    Ok(())
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
