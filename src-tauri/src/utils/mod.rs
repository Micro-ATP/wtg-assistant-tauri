pub mod command;
pub mod log;

use sysinfo::System;

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
