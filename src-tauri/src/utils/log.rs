#![allow(dead_code)]

use chrono::Local;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static LOGGER_INIT: OnceLock<()> = OnceLock::new();

fn resolve_base_dir() -> io::Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return Ok(parent.to_path_buf());
        }
    }

    std::env::current_dir()
}

fn build_log_filename() -> String {
    let ts = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let pid = std::process::id();
    format!("wtga_{}_pid{}.log", ts, pid)
}

pub fn ensure_logs_dir() -> io::Result<PathBuf> {
    if let Some(existing) = LOG_DIR.get() {
        return Ok(existing.clone());
    }

    let base_dir = resolve_base_dir()?;
    let logs_dir = base_dir.join("logs");
    fs::create_dir_all(&logs_dir)?;

    let _ = LOG_DIR.set(logs_dir.clone());
    Ok(logs_dir)
}

pub fn get_logs_dir() -> Option<PathBuf> {
    LOG_DIR.get().cloned()
}

pub fn init_logger() -> io::Result<PathBuf> {
    let logs_dir = ensure_logs_dir()?;
    if LOGGER_INIT.get().is_some() {
        return Ok(logs_dir);
    }

    let file_name = build_log_filename();
    let appender = tracing_appender::rolling::never(&logs_dir, &file_name);
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::registry()
        .with(LevelFilter::INFO)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(writer),
        )
        .try_init()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("logger init failed: {e}")))?;

    let _ = LOG_GUARD.set(guard);
    let _ = LOGGER_INIT.set(());
    info!("Logger initialized");
    info!("Log file path: {}", logs_dir.join(file_name).display());
    Ok(logs_dir)
}

pub fn log_info(message: &str) {
    info!("{}", message);
}

pub fn log_warn(message: &str) {
    warn!("{}", message);
}

pub fn log_error(message: &str) {
    error!("{}", message);
}
