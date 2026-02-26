//! Progress reporting system for write operations
//! Uses Tauri events to send real-time progress updates to the frontend

use crate::models::{WriteProgress, WriteStatus};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

/// Progress reporter that sends events during write operations
pub struct ProgressReporter {
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new() -> Self {
        ProgressReporter {
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the app handle (call this when the command receives it)
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        if let Ok(mut h) = self.app_handle.lock() {
            *h = Some(handle);
        }
    }

    /// Report progress update
    pub fn report(&self, progress: &WriteProgress) {
        if let Ok(h) = self.app_handle.lock() {
            if let Some(handle) = h.as_ref() {
                let _ = handle.emit("write-progress", progress);
            }
        }
    }

    /// Report progress with percentage and status
    pub fn report_status(&self, task_id: &str, progress: f64, message: &str, status: &str) {
        // Convert status string to proper enum
        let status_enum = match status {
            "preparing" => WriteStatus::Preparing,
            "partitioning" => WriteStatus::Partitioning,
            "applyingimage" => WriteStatus::ApplyingImage,
            "writingbootfiles" => WriteStatus::WritingBootFiles,
            "fixingbcd" => WriteStatus::FixingBcd,
            "copyingvhd" => WriteStatus::CopyingVhd,
            "applyingextras" => WriteStatus::ApplyingExtras,
            "verifying" => WriteStatus::Verifying,
            "completed" => WriteStatus::Completed,
            "failed" => WriteStatus::Failed,
            "cancelled" => WriteStatus::Cancelled,
            _ => WriteStatus::Idle,
        };

        let progress_obj = WriteProgress {
            task_id: task_id.to_string(),
            status: status_enum,
            progress,
            message: message.to_string(),
            speed: 0.0,
            elapsed_seconds: 0,
            estimated_remaining_seconds: 0,
        };
        self.report(&progress_obj);
    }
}

// Global progress reporter
lazy_static::lazy_static! {
    pub static ref PROGRESS_REPORTER: ProgressReporter = ProgressReporter::new();
}
