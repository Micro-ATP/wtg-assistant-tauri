use serde::Serialize;
use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Error, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Disk operation failed: {0}")]
    DiskError(String),

    #[error("USB device error: {0}")]
    UsbError(String),

    #[error("Image operation failed: {0}")]
    ImageError(String),

    #[error("System error: {0}")]
    SystemError(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("JSON error: {0}")]
    JsonError(String),

    #[error("UTF8 error: {0}")]
    Utf8Error(String),

    #[error("Unknown error")]
    Unknown,
}

impl AppError {
    pub fn io(err: io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::io(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::JsonError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for AppError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        AppError::Utf8Error(err.to_string())
    }
}
