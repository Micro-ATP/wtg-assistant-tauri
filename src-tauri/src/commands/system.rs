use serde::{Deserialize, Serialize};
use crate::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
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
        total_memory: crate::utils::get_total_memory(),
        available_memory: crate::utils::get_available_memory(),
        cpu_count: num_cpus::get(),
    })
}
