use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use crate::utils::macos_admin::MacosAdminSessionStatus;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
    pub cpu_model: String,
    pub total_memory: u64,
    pub available_memory: u64,
    pub cpu_count: usize,
}

/// Get system information
#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo> {
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: crate::utils::get_os_version(),
        cpu_model: crate::utils::get_cpu_model(),
        total_memory: crate::utils::get_total_memory(),
        available_memory: crate::utils::get_available_memory(),
        cpu_count: num_cpus::get(),
    })
}

#[tauri::command]
pub async fn get_logs_directory() -> Result<String> {
    let dir = crate::utils::log::ensure_logs_dir().map_err(AppError::io)?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_logs_directory() -> Result<String> {
    let dir = crate::utils::log::ensure_logs_dir().map_err(AppError::io)?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(AppError::io)?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(AppError::io)?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(AppError::io)?;
    }

    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_macos_admin_session_status() -> Result<MacosAdminSessionStatus> {
    crate::utils::macos_admin::get_macos_admin_session_status()
}

#[tauri::command]
pub async fn authorize_macos_admin_session() -> Result<MacosAdminSessionStatus> {
    crate::utils::macos_admin::authorize_macos_admin_session()
}

#[tauri::command]
pub async fn exit_app(app_handle: tauri::AppHandle) -> Result<()> {
    app_handle.exit(0);
    Ok(())
}
