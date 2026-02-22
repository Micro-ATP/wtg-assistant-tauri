// Tauri backend entry point
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod models;
mod platform;
mod services;
mod utils;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::disk::list_disks,
            commands::disk::get_disk_info,
            commands::usb::start_usb_monitoring,
            commands::usb::stop_usb_monitoring,
            commands::system::get_system_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
