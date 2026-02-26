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
    match utils::log::init_logger() {
        Ok(logs_dir) => {
            tracing::info!(
                "WTGA startup: version={} os={} arch={} logs_dir={}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                logs_dir.display()
            );
        }
        Err(e) => {
            eprintln!("Failed to initialize file logger: {e}");
        }
    }

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
            commands::system::get_logs_directory,
            commands::system::open_logs_directory,
            commands::write::get_image_info,
            commands::write::start_write,
            commands::write::cancel_write,
            commands::write::verify_system_files,
            commands::benchmark::run_benchmark,
            commands::benchmark::cancel_benchmark,
            commands::partition::list_partitions,
            commands::tools::repair_boot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
