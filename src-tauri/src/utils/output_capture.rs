//! Output capture and progress monitoring for command execution
//! Mimics the old architecture's real-time output capture with progress reporting

use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::process::{Command, Stdio};
use crate::utils::progress::PROGRESS_REPORTER;
use tracing::debug;

pub struct OutputCapture {
    task_id: String,
    stage: String,
}

impl OutputCapture {
    pub fn new(task_id: &str, stage: &str) -> Self {
        OutputCapture {
            task_id: task_id.to_string(),
            stage: stage.to_string(),
        }
    }

    /// Execute command and capture output in real-time, reporting progress
    pub fn execute_with_capture(
        &self,
        cmd: &str,
        args: &[&str],
    ) -> std::io::Result<i32> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().ok_or_else(|| {
            Error::new(
                ErrorKind::Other,
                "Failed to capture stdout pipe from child process",
            )
        })?;
        let reader = BufReader::new(stdout);
        let mut line_count = 0;
        let mut last_progress = 0.0;

        for line in reader.lines() {
            if let Ok(output) = line {
                line_count += 1;

                // Report progress based on keywords in output
                let progress = self.calculate_progress(&output, line_count);

                // Only report if progress changed significantly (more than 1%)
                if (progress - last_progress).abs() > 1.0 {
                    // Extract percentage from output if available for better UX
                    let message = if let Some(pct_str) = self.extract_percentage(&output) {
                        format!("{} ({}%)", self.stage, pct_str)
                    } else {
                        format!("{}...", self.stage)
                    };

                    PROGRESS_REPORTER.report_status(
                        &self.task_id,
                        progress,
                        &message,
                        &self.stage.to_lowercase(),
                    );
                    last_progress = progress;
                }

                debug!("[{}] {}", self.stage, output);
            }
        }

        let status = child.wait()?;
        Ok(status.code().unwrap_or(-1))
    }

    /// Calculate progress based on output content
    fn calculate_progress(&self, output: &str, line_count: usize) -> f64 {
        let output_lower = output.to_lowercase();

        // DISM progress indicators - look for percentage in output
        if output_lower.contains("100%") {
            return 99.0;
        }

        // Parse percentage like "45%" from DISM output
        if let Some(pos) = output.rfind('%') {
            if pos > 0 {
                // Extract number before %
                let start = output[..pos]
                    .rfind(|c: char| !c.is_numeric() && c != '.')
                    .map(|p| p + 1)
                    .unwrap_or(0);

                if let Ok(pct) = output[start..pos].parse::<f64>() {
                    if pct >= 0.0 && pct <= 100.0 {
                        // Map 0-100% to 25-99%
                        return 25.0 + (pct * 0.74);
                    }
                }
            }
        }

        // Diskpart/other completion indicators
        if output_lower.contains("successfully")
            || output_lower.contains("complete")
            || output_lower.contains("finished") {
            return 85.0;
        }

        // Generic progress based on line count (every 100 lines = ~1% progress)
        let estimated = 25.0 + (line_count as f64 / 100.0).min(70.0);
        estimated.min(95.0)
    }

    /// Extract percentage string from DISM output (e.g., "45" from "45%")
    fn extract_percentage(&self, output: &str) -> Option<String> {
        if let Some(pos) = output.rfind('%') {
            if pos > 0 {
                let start = output[..pos]
                    .rfind(|c: char| !c.is_numeric() && c != '.')
                    .map(|p| p + 1)
                    .unwrap_or(0);

                let pct_str = &output[start..pos];
                if let Ok(pct) = pct_str.parse::<f64>() {
                    if pct >= 0.0 && pct <= 100.0 {
                        return Some(pct_str.to_string());
                    }
                }
            }
        }
        None
    }
}
