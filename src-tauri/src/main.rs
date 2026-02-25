// Tauri backend entry point
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod error;
mod models;
mod platform;
mod services;
mod utils;

pub use error::{AppError, Result};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::disk::list_disks,
            commands::disk::get_disk_info,
            commands::disk::list_disk_diagnostics,
            commands::usb::start_usb_monitoring,
            commands::usb::stop_usb_monitoring,
            commands::system::get_system_info,
            commands::write::get_image_info,
            commands::write::start_write,
            commands::write::cancel_write,
            commands::write::verify_system_files,
            commands::benchmark::run_benchmark,
            commands::partition::list_partitions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
