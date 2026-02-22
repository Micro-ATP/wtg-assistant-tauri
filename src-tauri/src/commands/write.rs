//! Write commands - Tauri command handlers for write operations

use crate::models::{WtgConfig, WriteProgress, ImageInfo};
use crate::services;
use crate::Result;
use tracing::info;

/// Get image index information from a WIM/ESD file
#[tauri::command]
pub async fn get_image_info(image_path: String) -> Result<Vec<ImageInfo>> {
    info!("Getting image info for: {}", image_path);
    let info = tokio::task::spawn_blocking(move || {
        services::image::get_image_info(&image_path)
    })
    .await
    .map_err(|e| crate::AppError::SystemError(e.to_string()))??;

    Ok(info)
}

/// Start a WTG write operation
#[tauri::command]
pub async fn start_write(config: WtgConfig) -> Result<WriteProgress> {
    info!("Starting write operation with config: {:?}", config.boot_mode);

    // Determine app files path
    let app_files_path = std::env::temp_dir().join("WTGA").to_string_lossy().to_string();
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
    // In a full implementation, this would signal the write task to stop
    // For now, we kill the active processes
    let _ = crate::utils::command::CommandExecutor::kill_process("dism.exe");
    let _ = crate::utils::command::CommandExecutor::kill_process("diskpart.exe");
    Ok(())
}

/// Verify system files on a target disk
#[tauri::command]
pub async fn verify_system_files(target_disk: String) -> Result<bool> {
    Ok(services::image::verify_system_files(&target_disk))
}
