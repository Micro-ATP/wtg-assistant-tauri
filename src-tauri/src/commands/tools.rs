use crate::models::FirmwareType;
#[cfg(target_os = "windows")]
use crate::services::{boot, diskpart};
use crate::utils::command::CommandExecutor;
use crate::{AppError, Result};
#[cfg(target_os = "windows")]
use serde::Deserialize;
use serde::Serialize;
#[cfg(target_os = "macos")]
use serde_json::Value;
use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::SystemInformation::GetPhysicallyInstalledSystemMemory;

#[derive(Debug, Serialize)]
pub struct HardwareOverview {
    pub processors: Vec<String>,
    pub motherboard: String,
    pub memory_summary: String,
    pub graphics: Vec<String>,
    pub monitors: Vec<String>,
    pub disks: Vec<String>,
    pub audio_devices: Vec<String>,
    pub network_adapters: Vec<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32Processor {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "NumberOfCores")]
    number_of_cores: Option<u32>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32BaseBoard {
    #[serde(rename = "Manufacturer")]
    manufacturer: Option<String>,
    #[serde(rename = "Product")]
    product: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32PhysicalMemory {
    #[serde(rename = "Capacity")]
    capacity: Option<String>,
    #[serde(rename = "ConfiguredClockSpeed")]
    configured_clock_speed: Option<u32>,
    #[serde(rename = "Speed")]
    speed: Option<u32>,
    #[serde(rename = "SMBIOSMemoryType")]
    smbios_memory_type: Option<u16>,
    #[serde(rename = "MemoryType")]
    memory_type: Option<u16>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32VideoController {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "AdapterRAM")]
    adapter_ram: Option<u64>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32DesktopMonitor {
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32PnPEntity {
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32DiskDrive {
    #[serde(rename = "Model")]
    model: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct PsDiskBrief {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "Number")]
    number: Option<u32>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32SoundDevice {
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct Win32NetworkAdapter {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "PhysicalAdapter")]
    physical_adapter: Option<bool>,
    #[serde(rename = "NetEnabled")]
    net_enabled: Option<bool>,
}

fn dedup_keep_order(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn memory_type_name(code: u16) -> &'static str {
    match code {
        20 => "DDR",
        21 => "DDR2",
        24 => "DDR3",
        26 => "DDR4",
        34 => "DDR5",
        _ => "Unknown",
    }
}

fn parse_u64_text(value: Option<&str>) -> Option<u64> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(v) = raw.parse::<u64>() {
        return Some(v);
    }
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

fn format_gb_from_bytes(bytes: u64) -> u64 {
    ((bytes as f64) / 1024_f64.powi(3)).round() as u64
}

#[cfg(target_os = "windows")]
fn format_mb_from_bytes(bytes: u64) -> u64 {
    ((bytes as f64) / 1024_f64.powi(2)).round() as u64
}

#[cfg(target_os = "windows")]
fn get_installed_memory_bytes() -> Option<u64> {
    let mut total_kb: u64 = 0;
    // SAFETY: Windows API writes the value to a valid mutable pointer.
    let ok = unsafe { GetPhysicallyInstalledSystemMemory(&mut total_kb) }.is_ok();
    if ok && total_kb > 0 {
        Some(total_kb.saturating_mul(1024))
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn format_memory_summary(
    memories: &[Win32PhysicalMemory],
    fallback_total_bytes: Option<u64>,
) -> String {
    let module_gb: Vec<u64> = memories
        .iter()
        .filter_map(|m| parse_u64_text(m.capacity.as_deref()))
        .map(format_gb_from_bytes)
        .filter(|gb| *gb > 0)
        .collect();

    let speed_mhz = memories
        .iter()
        .filter_map(|m| m.configured_clock_speed.or(m.speed))
        .max();
    let mem_type = memories
        .iter()
        .find_map(|m| m.smbios_memory_type.or(m.memory_type))
        .map(memory_type_name)
        .unwrap_or("RAM");

    if module_gb.is_empty() {
        let Some(total_bytes) = fallback_total_bytes else {
            return "Unknown".to_string();
        };
        let total_gb = format_gb_from_bytes(total_bytes);
        return match speed_mhz {
            Some(mhz) if mem_type != "Unknown" && mem_type != "RAM" => {
                format!("{total_gb}GB {mem_type} {mhz}MHz")
            }
            Some(mhz) => format!("{total_gb}GB RAM {mhz}MHz"),
            None if mem_type != "Unknown" && mem_type != "RAM" => {
                format!("{total_gb}GB {mem_type}")
            }
            None => format!("{total_gb}GB RAM"),
        };
    }

    let total_gb: u64 = module_gb.iter().sum();
    let modules_text = module_gb
        .iter()
        .map(|gb| format!("{gb}GB"))
        .collect::<Vec<_>>()
        .join(" + ");

    match speed_mhz {
        Some(mhz) if mem_type == "Unknown" => format!("{total_gb}GB RAM {mhz}MHz ({modules_text})"),
        Some(mhz) => format!("{total_gb}GB {mem_type} {mhz}MHz ({modules_text})"),
        None if mem_type == "Unknown" => format!("{total_gb}GB RAM ({modules_text})"),
        None => format!("{total_gb}GB {mem_type} ({modules_text})"),
    }
}

#[cfg(target_os = "windows")]
fn utf16z_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

#[cfg(target_os = "windows")]
fn format_gpu_vram(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    let gb = format_gb_from_bytes(bytes);
    if gb > 0 {
        format!("{gb}GB")
    } else {
        format!("{}MB", format_mb_from_bytes(bytes).max(1))
    }
}

#[cfg(target_os = "windows")]
fn collect_gpus_via_dxgi() -> Vec<String> {
    let mut result = Vec::new();
    // SAFETY: DXGI factory and adapter enumeration follow Windows COM API contracts.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return result;
        };

        let mut index = 0u32;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            let mut desc = DXGI_ADAPTER_DESC1::default();
            if adapter.GetDesc1(&mut desc).is_err() {
                index += 1;
                continue;
            }

            if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
                index += 1;
                continue;
            }

            let name = utf16z_to_string(&desc.Description);
            if name.is_empty() {
                index += 1;
                continue;
            }

            let vram_text = format_gpu_vram(desc.DedicatedVideoMemory as u64);
            if vram_text.is_empty() {
                result.push(name);
            } else {
                result.push(format!("{name} ({vram_text})"));
            }

            index += 1;
        }
    }
    dedup_keep_order(result)
}

fn extract_json_value(output: &str) -> Option<serde_json::Value> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed.find('{').or_else(|| trimmed.find('['))?;
    serde_json::from_str(&trimmed[start..]).ok()
}

#[cfg(target_os = "windows")]
fn collect_disks_via_powershell() -> Vec<String> {
    let script = r#"
$ErrorActionPreference='SilentlyContinue'
Get-Disk | Sort-Object Number | ForEach-Object {
  [PSCustomObject]@{
    Number=[int]$_.Number
    Name=[string]$_.FriendlyName
    Size=[UInt64]$_.Size
  }
} | ConvertTo-Json -Compress
"#;

    let Ok(output) =
        CommandExecutor::execute_allow_fail("powershell.exe", &["-NoProfile", "-Command", script])
    else {
        return Vec::new();
    };

    let Some(json) = extract_json_value(&output) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    if let Ok(v) = serde_json::from_value::<Vec<PsDiskBrief>>(json.clone()) {
        rows = v;
    } else if let Ok(single) = serde_json::from_value::<PsDiskBrief>(json) {
        rows.push(single);
    }

    rows.into_iter()
        .filter_map(|d| {
            let name = d
                .name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("Disk {}", d.number.unwrap_or(0)));
            let size_gb = d.size.map(format_gb_from_bytes).unwrap_or(0);
            if size_gb > 0 {
                Some(format!("{name} ({size_gb}GB)"))
            } else {
                Some(name)
            }
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn gather_hardware_overview_windows() -> Result<HardwareOverview> {
    use wmi::{COMLibrary, WMIConnection};

    let com = COMLibrary::new().map_err(|e| AppError::SystemError(e.to_string()))?;
    let wmi = WMIConnection::new(com.into()).map_err(|e| AppError::SystemError(e.to_string()))?;

    let processors: Vec<Win32Processor> = wmi
        .raw_query("SELECT Name, NumberOfCores FROM Win32_Processor")
        .unwrap_or_default();
    let motherboards: Vec<Win32BaseBoard> = wmi
        .raw_query("SELECT Manufacturer, Product FROM Win32_BaseBoard")
        .unwrap_or_default();
    let memories: Vec<Win32PhysicalMemory> = wmi
        .raw_query("SELECT Capacity, ConfiguredClockSpeed, Speed, SMBIOSMemoryType, MemoryType FROM Win32_PhysicalMemory")
        .unwrap_or_default();
    let gpus: Vec<Win32VideoController> = wmi
        .raw_query("SELECT Name, AdapterRAM FROM Win32_VideoController")
        .unwrap_or_default();
    let monitors: Vec<Win32DesktopMonitor> = wmi
        .raw_query("SELECT Name FROM Win32_DesktopMonitor")
        .unwrap_or_default();
    let pnp_monitors: Vec<Win32PnPEntity> = wmi
        .raw_query("SELECT Name FROM Win32_PnPEntity WHERE PNPClass='Monitor'")
        .unwrap_or_default();
    let disks: Vec<Win32DiskDrive> = wmi
        .raw_query("SELECT Model, Size FROM Win32_DiskDrive")
        .unwrap_or_default();
    let audios: Vec<Win32SoundDevice> = wmi
        .raw_query("SELECT Name FROM Win32_SoundDevice")
        .unwrap_or_default();
    let nics: Vec<Win32NetworkAdapter> = wmi
        .raw_query("SELECT Name, PhysicalAdapter, NetEnabled FROM Win32_NetworkAdapter")
        .unwrap_or_default();

    let processors_text = dedup_keep_order(
        processors
            .into_iter()
            .filter_map(|cpu| {
                let name = cpu.name?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let cores = cpu.number_of_cores.unwrap_or(0);
                if cores > 0 {
                    Some(format!("{name} ({cores}C)"))
                } else {
                    Some(name)
                }
            })
            .collect(),
    );

    let motherboard = motherboards
        .into_iter()
        .find_map(|board| {
            let maker = board.manufacturer.unwrap_or_default().trim().to_string();
            let model = board.product.unwrap_or_default().trim().to_string();
            let text = format!("{maker} {model}").trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let installed_memory = get_installed_memory_bytes();
    let memory_summary = format_memory_summary(&memories, installed_memory);

    let graphics_from_dxgi = collect_gpus_via_dxgi();
    let graphics = if !graphics_from_dxgi.is_empty() {
        graphics_from_dxgi
    } else {
        dedup_keep_order(
            gpus.into_iter()
                .filter_map(|gpu| {
                    let name = gpu.name?.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let vram = gpu.adapter_ram.unwrap_or(0);
                    let vram_text = format_gpu_vram(vram);
                    if vram_text.is_empty() {
                        Some(name)
                    } else {
                        Some(format!("{name} ({vram_text})"))
                    }
                })
                .collect(),
        )
    };

    let monitors = dedup_keep_order(
        monitors
            .into_iter()
            .filter_map(|m| m.name)
            .chain(pnp_monitors.into_iter().filter_map(|m| m.name))
            .map(|s| s.trim().to_string())
            .filter(|s| {
                let lower = s.to_ascii_lowercase();
                !s.is_empty()
                    && !s.eq_ignore_ascii_case("Generic PnP Monitor")
                    && !s.contains("通用即插即用监视器")
                    && !s.contains("默认监视器")
                    && !(lower.starts_with("generic monitor") && !s.contains('('))
            })
            .collect(),
    );

    let mut disks = dedup_keep_order(
        disks
            .into_iter()
            .filter_map(|disk| {
                let name = disk.model?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let size_gb = parse_u64_text(disk.size.as_deref())
                    .map(format_gb_from_bytes)
                    .unwrap_or(0);
                if size_gb > 0 {
                    Some(format!("{name} ({size_gb}GB)"))
                } else {
                    Some(name)
                }
            })
            .collect(),
    );
    if disks.is_empty() {
        disks = dedup_keep_order(collect_disks_via_powershell());
    }

    let audio_devices = dedup_keep_order(
        audios
            .into_iter()
            .filter_map(|a| a.name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    );

    let network_adapters = dedup_keep_order(
        nics.into_iter()
            .filter(|nic| nic.physical_adapter.unwrap_or(false) || nic.net_enabled.unwrap_or(false))
            .filter_map(|nic| nic.name)
            .map(|s| s.trim().to_string())
            .filter(|s| {
                let lower = s.to_ascii_lowercase();
                !s.is_empty()
                    && !lower.eq("card")
                    && !lower.contains("virtual")
                    && !lower.contains("miniport")
                    && !lower.contains("bluetooth device (personal area network)")
            })
            .collect(),
    );

    Ok(HardwareOverview {
        processors: processors_text,
        motherboard,
        memory_summary,
        graphics,
        monitors,
        disks,
        audio_devices,
        network_adapters,
    })
}

#[cfg(target_os = "macos")]
fn command_output_text(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

#[cfg(target_os = "macos")]
fn command_output_json(cmd: &str, args: &[&str]) -> Option<Value> {
    let output = command_output_text(cmd, args)?;
    extract_json_value(&output)
}

#[cfg(target_os = "macos")]
fn value_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(target_os = "macos")]
fn value_u64(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .or_else(|| value.get(key).and_then(Value::as_f64).map(|n| n.max(0.0) as u64))
}

#[cfg(target_os = "macos")]
fn collect_named_values_recursive(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(arr) => {
            for item in arr {
                collect_named_values_recursive(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(name) = map.get("_name").and_then(Value::as_str) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
            for child in map.values() {
                collect_named_values_recursive(child, out);
            }
        }
        _ => {}
    }
}

#[cfg(target_os = "macos")]
fn get_first_hardware_item() -> Option<Value> {
    let json = command_output_json("system_profiler", &["SPHardwareDataType", "-json"])?;
    json.get("SPHardwareDataType")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first().cloned())
}

#[cfg(target_os = "macos")]
fn build_macos_processors() -> Vec<String> {
    let hardware = get_first_hardware_item();
    let mut candidates = Vec::new();

    if let Some(hw) = hardware.as_ref() {
        candidates.push(value_str(hw, "chip_type"));
        candidates.push(value_str(hw, "cpu_type"));
        candidates.push(value_str(hw, "boot_rom_version"));
    }

    candidates.push(
        command_output_text("sysctl", &["-n", "machdep.cpu.brand_string"]).unwrap_or_default(),
    );

    let mut name = candidates
        .into_iter()
        .find(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("unknowncpu"))
        .unwrap_or_else(|| crate::utils::get_cpu_model());

    if name.trim().is_empty() || name.eq_ignore_ascii_case("unknowncpu") {
        name = "Unknown CPU".to_string();
    }

    let physical = command_output_text("sysctl", &["-n", "hw.physicalcpu"])
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let logical = command_output_text("sysctl", &["-n", "hw.logicalcpu"])
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(0);

    if physical > 0 && logical > 0 && logical >= physical {
        vec![format!("{name} ({physical}C/{logical}T)")]
    } else if physical > 0 {
        vec![format!("{name} ({physical}C)")]
    } else {
        vec![name]
    }
}

#[cfg(target_os = "macos")]
fn build_macos_motherboard() -> String {
    if let Some(hw) = get_first_hardware_item() {
        let machine_name = value_str(&hw, "machine_name");
        let machine_model = value_str(&hw, "machine_model");
        let model_identifier = value_str(&hw, "model_identifier");
        let chip = value_str(&hw, "chip_type");

        let mut parts = Vec::new();
        if !machine_name.is_empty() {
            parts.push(machine_name);
        }
        if !machine_model.is_empty() {
            parts.push(machine_model);
        } else if !model_identifier.is_empty() {
            parts.push(model_identifier);
        }
        if !chip.is_empty() {
            parts.push(chip);
        }
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }

    command_output_text("sysctl", &["-n", "hw.model"]).unwrap_or_else(|| "Unknown".to_string())
}

#[cfg(target_os = "macos")]
fn build_macos_memory_summary() -> String {
    if let Some(hw) = get_first_hardware_item() {
        let text = value_str(&hw, "memory");
        if !text.is_empty() {
            if value_str(&hw, "chip_type").to_ascii_lowercase().contains("apple") {
                return format!("{text} Unified Memory");
            }
            return text;
        }
    }

    let total = crate::utils::get_total_memory();
    let gb = format_gb_from_bytes(total);
    if gb > 0 {
        format!("{gb}GB RAM")
    } else {
        "Unknown".to_string()
    }
}

#[cfg(target_os = "macos")]
fn build_macos_displays() -> (Vec<String>, Vec<String>) {
    let mut graphics = Vec::new();
    let mut monitors = Vec::new();

    let Some(json) = command_output_json("system_profiler", &["SPDisplaysDataType", "-json"])
    else {
        return (graphics, monitors);
    };

    let Some(items) = json.get("SPDisplaysDataType").and_then(Value::as_array) else {
        return (graphics, monitors);
    };

    for item in items {
        let model = value_str(item, "sppci_model");
        let fallback_name = value_str(item, "_name");
        let name = if !model.is_empty() {
            model
        } else {
            fallback_name
        };
        if !name.is_empty() {
            let mut vram = value_str(item, "spdisplays_vram");
            if vram.is_empty() {
                vram = value_str(item, "spdisplays_vram_shared");
            }
            if !vram.is_empty() {
                graphics.push(format!("{name} ({vram})"));
            } else {
                graphics.push(name);
            }
        }

        if let Some(ndrvs) = item.get("spdisplays_ndrvs").and_then(Value::as_array) {
            for display in ndrvs {
                let display_name = value_str(display, "_name");
                if display_name.is_empty() {
                    continue;
                }
                let resolution = value_str(display, "_spdisplays_resolution");
                if resolution.is_empty() {
                    monitors.push(display_name);
                } else {
                    monitors.push(format!("{display_name} ({resolution})"));
                }
            }
        }
    }

    (dedup_keep_order(graphics), dedup_keep_order(monitors))
}

#[cfg(target_os = "macos")]
fn parse_storage_size_bytes(item: &Value) -> u64 {
    value_u64(item, "size_in_bytes")
        .or_else(|| value_u64(item, "spsize_bytes"))
        .or_else(|| {
            value_str(item, "size")
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn build_macos_disks() -> Vec<String> {
    let Some(json) = command_output_json("system_profiler", &["SPStorageDataType", "-json"]) else {
        return Vec::new();
    };
    let Some(items) = json.get("SPStorageDataType").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let name = value_str(item, "_name");
        if name.is_empty() {
            continue;
        }
        let size_bytes = parse_storage_size_bytes(item);
        if size_bytes > 0 {
            out.push(format!("{name} ({}GB)", format_gb_from_bytes(size_bytes)));
        } else {
            out.push(name);
        }
    }
    dedup_keep_order(out)
}

#[cfg(target_os = "macos")]
fn build_macos_audio_devices() -> Vec<String> {
    let Some(json) = command_output_json("system_profiler", &["SPAudioDataType", "-json"]) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    collect_named_values_recursive(&json, &mut values);

    dedup_keep_order(
        values
            .into_iter()
            .filter(|v| {
                let lower = v.to_ascii_lowercase();
                !v.trim().is_empty()
                    && !lower.contains("audio:")
                    && !lower.contains("devices")
                    && !lower.eq("audio")
            })
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn build_macos_network_adapters() -> Vec<String> {
    let Some(json) = command_output_json("system_profiler", &["SPNetworkDataType", "-json"]) else {
        return Vec::new();
    };
    let Some(items) = json.get("SPNetworkDataType").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let name = value_str(item, "_name");
        if name.is_empty() {
            continue;
        }
        let interface = value_str(item, "interface");
        if interface.is_empty() {
            out.push(name);
        } else {
            out.push(format!("{name} ({interface})"));
        }
    }
    dedup_keep_order(out)
}

#[cfg(target_os = "macos")]
fn gather_hardware_overview_macos() -> HardwareOverview {
    let processors = dedup_keep_order(build_macos_processors());
    let motherboard = build_macos_motherboard();
    let memory_summary = build_macos_memory_summary();
    let (graphics, monitors) = build_macos_displays();
    let disks = build_macos_disks();
    let audio_devices = build_macos_audio_devices();
    let network_adapters = build_macos_network_adapters();

    HardwareOverview {
        processors: if processors.is_empty() {
            vec!["Unknown CPU".to_string()]
        } else {
            processors
        },
        motherboard: if motherboard.trim().is_empty() {
            "Unknown".to_string()
        } else {
            motherboard
        },
        memory_summary: if memory_summary.trim().is_empty() {
            "Unknown".to_string()
        } else {
            memory_summary
        },
        graphics,
        monitors,
        disks,
        audio_devices,
        network_adapters,
    }
}

fn normalize_drive_root(input: &str) -> Result<String> {
    let trimmed = input.trim().trim_end_matches('\\').trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::InvalidParameter(
            "Drive letter is required, e.g. E or E:".to_string(),
        ));
    }

    let letter = trimmed
        .chars()
        .next()
        .ok_or_else(|| AppError::InvalidParameter("Invalid drive letter".to_string()))?;

    if !letter.is_ascii_alphabetic() {
        return Err(AppError::InvalidParameter(
            "Drive letter must start with A-Z".to_string(),
        ));
    }

    Ok(format!("{}:\\", letter.to_ascii_uppercase()))
}

fn parse_firmware(input: &str) -> Result<FirmwareType> {
    match input.trim().to_ascii_lowercase().as_str() {
        "uefi" => Ok(FirmwareType::UEFI),
        "bios" => Ok(FirmwareType::BIOS),
        "all" => Ok(FirmwareType::ALL),
        _ => Err(AppError::InvalidParameter(
            "Firmware must be one of: uefi, bios, all".to_string(),
        )),
    }
}

fn resolve_disk_number_from_drive(drive_root: &str) -> Result<String> {
    let drive_letter = drive_root
        .chars()
        .next()
        .ok_or_else(|| AppError::InvalidParameter("Invalid target drive".to_string()))?
        .to_ascii_uppercase();

    let ps = format!(
        "(Get-Partition -DriveLetter {letter} -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty DiskNumber)",
        letter = drive_letter
    );

    let output =
        CommandExecutor::execute_allow_fail("powershell.exe", &["-NoProfile", "-Command", &ps])?;

    let disk_no = output.trim().to_string();
    if disk_no.is_empty() {
        return Err(AppError::DeviceNotFound(format!(
            "Cannot resolve disk number for {}",
            drive_root
        )));
    }

    Ok(disk_no)
}

#[tauri::command]
pub async fn repair_boot(target_disk: String, firmware: String) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        let target_root = normalize_drive_root(&target_disk)?;
        let fw_type = parse_firmware(&firmware)?;
        let disk_no = resolve_disk_number_from_drive(&target_root)?;

        if !std::path::Path::new(&target_root).exists() {
            return Err(AppError::DeviceNotFound(format!(
                "Target path not found: {}",
                target_root
            )));
        }

        let target_root_for_task = target_root.clone();
        let disk_no_for_task = disk_no.clone();
        let firmware_for_msg = firmware.to_ascii_uppercase();
        let result = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut mounted_efi: Option<String> = None;
            let mut mounted_efi_temporary = false;

            let run_uefi = matches!(fw_type, FirmwareType::UEFI | FirmwareType::ALL);
            if run_uefi {
                let (esp_letter, temporary) = diskpart::mount_efi_partition(&disk_no_for_task)?;
                mounted_efi = Some(format!("{}\\", esp_letter));
                mounted_efi_temporary = temporary;
            }

            let op_result: Result<()> = match fw_type {
                FirmwareType::UEFI => {
                    let esp = mounted_efi
                        .as_ref()
                        .ok_or_else(|| AppError::DiskError("EFI mount failed".to_string()))?;
                    boot::bcdboot_write_boot_file(&target_root_for_task, esp, &FirmwareType::UEFI)?;
                    boot::bcdedit_fix_boot_file_typical(
                        esp,
                        &target_root_for_task,
                        &FirmwareType::UEFI,
                    )?;
                    Ok(())
                }
                FirmwareType::BIOS => {
                    boot::bcdboot_write_boot_file(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    boot::bcdedit_fix_boot_file_typical(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    Ok(())
                }
                FirmwareType::ALL => {
                    boot::bcdboot_write_boot_file(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;
                    boot::bcdedit_fix_boot_file_typical(
                        &target_root_for_task,
                        &target_root_for_task,
                        &FirmwareType::BIOS,
                    )?;

                    let esp = mounted_efi
                        .as_ref()
                        .ok_or_else(|| AppError::DiskError("EFI mount failed".to_string()))?;
                    boot::bcdboot_write_boot_file(&target_root_for_task, esp, &FirmwareType::UEFI)?;
                    boot::bcdedit_fix_boot_file_typical(
                        esp,
                        &target_root_for_task,
                        &FirmwareType::UEFI,
                    )?;
                    Ok(())
                }
            };

            if mounted_efi_temporary {
                if let Some(esp) = mounted_efi {
                    let _ = diskpart::remove_drive_letter(&esp);
                }
            }

            op_result
        })
        .await
        .map_err(|e| AppError::SystemError(e.to_string()))?;

        result?;
        Ok(format!(
            "Boot repair completed for {} ({})",
            target_root, firmware_for_msg
        ))
    }

    #[cfg(target_os = "macos")]
    {
        let fw_type = parse_firmware(&firmware)?;
        match fw_type {
            FirmwareType::UEFI => {
                tokio::task::spawn_blocking(move || {
                    crate::services::write_macos::repair_boot_uefi_for_target(&target_disk)
                })
                .await
                .map_err(|e| AppError::SystemError(e.to_string()))?
            }
            FirmwareType::BIOS | FirmwareType::ALL => Err(AppError::Unsupported(
                "macOS boot repair currently supports UEFI mode only".to_string(),
            )),
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (target_disk, firmware);
        Err(AppError::Unsupported(
            "Boot repair is currently implemented on Windows only".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn get_hardware_overview() -> Result<HardwareOverview> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(gather_hardware_overview_windows)
            .await
            .map_err(|e| AppError::SystemError(e.to_string()))?
    }

    #[cfg(target_os = "macos")]
    {
        Ok(tokio::task::spawn_blocking(gather_hardware_overview_macos)
            .await
            .map_err(|e| AppError::SystemError(e.to_string()))?)
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Err(AppError::Unsupported(
            "Hardware overview is currently implemented on Windows only".to_string(),
        ))
    }
}
