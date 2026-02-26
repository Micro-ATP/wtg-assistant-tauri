//! Write commands - Tauri command handlers for write operations

use crate::models::{ImageInfo, WriteProgress, WtgConfig};
use crate::services;
use crate::utils::progress::PROGRESS_REPORTER;
use crate::utils::task_manager;
use crate::Result;
use tracing::info;

/// Get image index information from a WIM/ESD file
#[tauri::command]
pub async fn get_image_info(image_path: String) -> Result<Vec<ImageInfo>> {
    info!("Getting image info for: {}", image_path);
    let info = tokio::task::spawn_blocking(move || services::image::get_image_info(&image_path))
        .await
        .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

    Ok(info)
}

/// Start a WTG write operation
#[tauri::command]
pub async fn start_write(config: WtgConfig, app_handle: tauri::AppHandle) -> Result<WriteProgress> {
    info!(
        "Starting write operation with config: {:?}",
        config.boot_mode
    );

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

    Ok(progress)
}

/// Cancel a running write operation
#[tauri::command]
pub async fn cancel_write(task_id: String) -> Result<()> {
    info!("Cancelling write operation: {}", task_id);

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
    Ok(services::image::verify_system_files(&target_disk))
}
