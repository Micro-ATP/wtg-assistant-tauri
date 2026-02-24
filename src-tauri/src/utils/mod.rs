pub mod command;
pub mod log;
pub mod task_manager;
pub mod progress;
pub mod output_capture;

use sysinfo::System;

/// Safely extract the first character of a string (e.g. drive letter "E" from "E:").
/// Returns empty string if input is empty.
pub fn first_char(s: &str) -> &str {
    if s.is_empty() {
        ""
    } else {
        &s[..s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(s.len())]
    }
}

/// Safely extract the first two characters of a string (e.g. "E:" from "E:\\").
/// Returns the full string if it's shorter than 2 characters.
pub fn first_two_chars(s: &str) -> &str {
    if s.len() < 2 {
        s
    } else {
        &s[..s.char_indices().nth(2).map(|(i, _)| i).unwrap_or(s.len())]
    }
}

pub fn get_os_version() -> String {
    System::long_os_version().unwrap_or_else(|| "Unknown".to_string())
}

pub fn get_total_memory() -> u64 {
    let sys = System::new_all();
    sys.total_memory()
}

pub fn get_available_memory() -> u64 {
    let sys = System::new_all();
    sys.available_memory()
}
