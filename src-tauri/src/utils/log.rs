#![allow(dead_code)]

use tracing::{error, info, warn};

pub fn init_logger() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
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
