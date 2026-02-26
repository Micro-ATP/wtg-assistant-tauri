pub mod command;
pub mod log;
pub mod output_capture;
pub mod progress;
pub mod task_manager;

use sysinfo::System;
#[cfg(target_os = "windows")]
use {
    serde::Deserialize,
    windows::Win32::System::SystemInformation::GetPhysicallyInstalledSystemMemory,
    winreg::{enums::HKEY_LOCAL_MACHINE, RegKey},
    wmi::{COMLibrary, WMIConnection},
};

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
    get_os_version_detailed()
        .unwrap_or_else(|| System::long_os_version().unwrap_or_else(|| "Unknown".to_string()))
}

pub fn get_total_memory() -> u64 {
    #[cfg(target_os = "windows")]
    if let Some(installed) = get_installed_memory_from_api() {
        return installed;
    }

    get_total_memory_detailed().unwrap_or_else(|| {
        let sys = System::new_all();
        sys.total_memory()
    })
}

pub fn get_available_memory() -> u64 {
    let sys = System::new_all();
    sys.available_memory()
}

pub fn get_cpu_model() -> String {
    get_cpu_model_detailed().unwrap_or_else(|| {
        let mut sys = System::new();
        sys.refresh_cpu();
        let brand = sys.global_cpu_info().brand().trim().to_string();
        if brand.is_empty() {
            "Unknown CPU".to_string()
        } else {
            brand
        }
    })
}

#[cfg(target_os = "windows")]
fn get_os_version_detailed() -> Option<String> {
    // Prefer WMI because ProductName sometimes reports Windows 10 on Insider builds
    #[derive(Deserialize, Debug)]
    struct Win32OS {
        #[serde(rename = "Caption")]
        caption: Option<String>,
        #[serde(rename = "BuildNumber")]
        build_number: Option<String>,
    }

    if let Ok(com) = COMLibrary::new() {
        if let Ok(wmi_con) = WMIConnection::new(com.into()) {
            if let Ok(results) = wmi_con
                .raw_query::<Win32OS>("SELECT Caption, BuildNumber FROM Win32_OperatingSystem")
            {
                if let Some(os) = results.first() {
                    let mut s = os.caption.clone().unwrap_or_default();
                    if !s.is_empty() {
                        if let Some(build) = &os.build_number {
                            s.push_str(" Build ");
                            s.push_str(build);
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                }
            }
        }
    }

    // Fallback to registry fields
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        .ok()?;

    let product_name: String = key.get_value("ProductName").unwrap_or_default();
    let display_version: String = key.get_value("DisplayVersion").unwrap_or_default();
    let current_build: String = key.get_value("CurrentBuildNumber").unwrap_or_default();

    if product_name.is_empty() {
        return None;
    }

    let mut version = product_name;
    if !display_version.is_empty() {
        version.push(' ');
        version.push_str(&display_version);
    }
    if !current_build.is_empty() {
        version.push_str(" Build ");
        version.push_str(&current_build);
    }

    Some(version)
}

#[cfg(not(target_os = "windows"))]
fn get_os_version_detailed() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn get_total_memory_detailed() -> Option<u64> {
    #[derive(Deserialize, Debug)]
    struct Win32PhysicalMemory {
        #[serde(rename = "Capacity")]
        capacity: Option<String>,
    }

    let com = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com.into()).ok()?;
    let memories: Vec<Win32PhysicalMemory> =
        wmi_con.raw_query("SELECT Capacity FROM Win32_PhysicalMemory").ok()?;

    let total = memories
        .into_iter()
        .filter_map(|m| m.capacity)
        .filter_map(|v| v.trim().parse::<u64>().ok())
        .sum::<u64>();

    if total > 0 { Some(total) } else { None }
}

#[cfg(target_os = "windows")]
fn get_installed_memory_from_api() -> Option<u64> {
    let mut total_kb: u64 = 0;
    // SAFETY: The Windows API writes to the provided pointer and does not retain it.
    let ok = unsafe { GetPhysicallyInstalledSystemMemory(&mut total_kb) }.is_ok();
    if ok && total_kb > 0 {
        Some(total_kb.saturating_mul(1024))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_total_memory_detailed() -> Option<u64> {
    None
}

#[cfg(target_os = "windows")]
fn get_cpu_model_detailed() -> Option<String> {
    #[derive(Deserialize, Debug)]
    struct Win32Processor {
        #[serde(rename = "Name")]
        name: Option<String>,
    }

    let com = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com.into()).ok()?;
    let results: Vec<Win32Processor> =
        wmi_con.raw_query("SELECT Name FROM Win32_Processor").ok()?;
    results
        .into_iter()
        .filter_map(|p| p.name)
        .find(|s| !s.trim().is_empty())
}

#[cfg(not(target_os = "windows"))]
fn get_cpu_model_detailed() -> Option<String> {
    None
}
