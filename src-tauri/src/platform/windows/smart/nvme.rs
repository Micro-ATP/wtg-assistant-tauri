// NVMe SMART API implementation
// Migrated from CrystalDiskInfo (MIT License)
// Original Author: hiyohiyo (https://crystalmark.info/)

use std::mem;
use windows::core::PCSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;

// CrystalDiskInfo uses StorageAdapterProtocolSpecificProperty first.
const STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY: u32 = 49;
const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 50;
const PROPERTY_STANDARD_QUERY: u32 = 0;

const PROTOCOL_TYPE_NVME: u32 = 3;
const NVME_DATA_TYPE_IDENTIFY: u32 = 1;
const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
const NVME_LOG_PAGE_HEALTH_INFO: u32 = 0x02;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct StoragePropertyQuery {
    property_id: u32,
    query_type: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct StorageProtocolSpecificData {
    protocol_type: u32,
    data_type: u32,
    protocol_data_request_value: u32,
    protocol_data_request_sub_value: u32,
    protocol_data_offset: u32,
    protocol_data_length: u32,
    fixed_protocol_return_data: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct StorageQueryWithBuffer {
    query: StoragePropertyQuery,
    protocol_specific: StorageProtocolSpecificData,
    buffer: [u8; 4096],
}

#[derive(Debug, Clone)]
pub struct NVMeSmartData {
    pub critical_warning: u8,
    pub temperature: i32,
    pub available_spare: u8,
    pub available_spare_threshold: u8,
    pub percentage_used: u8,
    pub data_units_read: u128,
    pub data_units_written: u128,
    pub host_read_commands: u128,
    pub host_write_commands: u128,
    pub controller_busy_time: u128,
    pub power_cycles: u128,
    pub power_on_hours: u128,
    pub unsafe_shutdowns: u128,
    pub media_errors: u128,
    pub num_err_log_entries: u128,
    pub warning_temp_time: u32,
    pub critical_temp_time: u32,
    pub temp_sensors: [i32; 8],
}

#[derive(Debug, Clone, Default)]
pub struct NVMeIdentifyInfo {
    pub model: String,
    pub serial_number: String,
    pub firmware_version: String,
}

pub struct NVMeHandle {
    handle: HANDLE,
}

impl NVMeHandle {
    pub fn open(physical_drive_id: u32) -> Result<Self, String> {
        let path = format!("\\\\.\\PhysicalDrive{}\0", physical_drive_id);
        let access_attempts = [GENERIC_READ | GENERIC_WRITE, GENERIC_READ, 0];

        for access in access_attempts {
            unsafe {
                if let Ok(handle) = CreateFileA(
                    PCSTR(path.as_ptr()),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                ) {
                    if handle != INVALID_HANDLE_VALUE {
                        return Ok(NVMeHandle { handle });
                    }
                }
            }
        }

        Err(format!(
            "Failed to open NVMe disk path {} with required access",
            path.trim_end_matches('\0')
        ))
    }

    pub fn read_smart_data(&self) -> Result<NVMeSmartData, String> {
        let mut last_err = String::new();
        for property_id in [
            STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY,
            STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY,
        ] {
            for sub_value in [0u32, 0xFFFF_FFFFu32] {
                match self.run_storage_query(
                    property_id,
                    NVME_DATA_TYPE_LOG_PAGE,
                    NVME_LOG_PAGE_HEALTH_INFO,
                    sub_value,
                    4096,
                ) {
                    Ok(data) if data.iter().take(512).any(|b| *b != 0) => {
                        return Self::parse_smart_data(&data[..512])
                    }
                    Ok(_) => {}
                    Err(e) => last_err = e,
                }
            }
        }

        Err(if last_err.is_empty() {
            "Failed to query NVMe SMART log page".to_string()
        } else {
            last_err
        })
    }

    pub fn read_identify_controller(&self) -> Result<NVMeIdentifyInfo, String> {
        let mut last_err = String::new();
        for property_id in [
            STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY,
            STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY,
        ] {
            match self.run_storage_query(property_id, NVME_DATA_TYPE_IDENTIFY, 1, 0, 4096) {
                Ok(data) if data.iter().any(|b| *b != 0) => {
                    return Ok(parse_identify_controller(&data));
                }
                Ok(_) => {}
                Err(e) => last_err = e,
            }
        }

        Err(if last_err.is_empty() {
            "Failed to query NVMe identify controller data".to_string()
        } else {
            last_err
        })
    }

    fn run_storage_query(
        &self,
        property_id: u32,
        data_type: u32,
        request_value: u32,
        request_sub_value: u32,
        data_len: u32,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            let mut query_buffer = StorageQueryWithBuffer {
                query: StoragePropertyQuery {
                    property_id,
                    query_type: PROPERTY_STANDARD_QUERY,
                },
                protocol_specific: StorageProtocolSpecificData {
                    protocol_type: PROTOCOL_TYPE_NVME,
                    data_type,
                    protocol_data_request_value: request_value,
                    protocol_data_request_sub_value: request_sub_value,
                    protocol_data_offset: mem::size_of::<StorageProtocolSpecificData>() as u32,
                    protocol_data_length: data_len.min(4096),
                    fixed_protocol_return_data: 0,
                    reserved: [0; 3],
                },
                buffer: [0; 4096],
            };

            let mut bytes_returned: u32 = 0;
            let buffer_size = mem::size_of::<StorageQueryWithBuffer>() as u32;
            let result = DeviceIoControl(
                self.handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(&query_buffer as *const _ as *const _),
                buffer_size,
                Some(&mut query_buffer as *mut _ as *mut _),
                buffer_size,
                Some(&mut bytes_returned),
                None,
            );

            if result.is_err() {
                return Err("DeviceIoControl(IOCTL_STORAGE_QUERY_PROPERTY) failed".to_string());
            }

            let reported_len = query_buffer.protocol_specific.protocol_data_length;
            let copy_len = if reported_len == 0 {
                data_len.min(4096) as usize
            } else {
                reported_len.min(4096) as usize
            };
            Ok(query_buffer.buffer[..copy_len].to_vec())
        }
    }

    fn parse_smart_data(buffer: &[u8]) -> Result<NVMeSmartData, String> {
        if buffer.len() < 512 {
            return Err("NVMe SMART buffer too small".to_string());
        }

        let temp_kelvin = u16::from_le_bytes([buffer[1], buffer[2]]) as i32;
        let temperature = if (1..0x7FFF).contains(&temp_kelvin) {
            temp_kelvin - 273
        } else {
            -1000
        };

        let mut temp_sensors = [-1000_i32; 8];
        for i in 0..8 {
            let offset = 200 + i * 2;
            let sensor_k = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]) as i32;
            if (1..0x7FFF).contains(&sensor_k) {
                temp_sensors[i] = sensor_k - 273;
            }
        }

        Ok(NVMeSmartData {
            critical_warning: buffer[0],
            temperature,
            available_spare: buffer[3],
            available_spare_threshold: buffer[4],
            percentage_used: buffer[5],
            data_units_read: parse_u128_le_at(buffer, 32),
            data_units_written: parse_u128_le_at(buffer, 48),
            host_read_commands: parse_u128_le_at(buffer, 64),
            host_write_commands: parse_u128_le_at(buffer, 80),
            controller_busy_time: parse_u128_le_at(buffer, 96),
            power_cycles: parse_u128_le_at(buffer, 112),
            power_on_hours: parse_u128_le_at(buffer, 128),
            unsafe_shutdowns: parse_u128_le_at(buffer, 144),
            media_errors: parse_u128_le_at(buffer, 160),
            num_err_log_entries: parse_u128_le_at(buffer, 176),
            warning_temp_time: u32::from_le_bytes([
                buffer[192],
                buffer[193],
                buffer[194],
                buffer[195],
            ]),
            critical_temp_time: u32::from_le_bytes([
                buffer[196],
                buffer[197],
                buffer[198],
                buffer[199],
            ]),
            temp_sensors,
        })
    }
}

impl Drop for NVMeHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

fn parse_u128_le_at(buf: &[u8], offset: usize) -> u128 {
    if offset + 16 > buf.len() {
        return 0;
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&buf[offset..offset + 16]);
    u128::from_le_bytes(bytes)
}

fn parse_ascii_field(data: &[u8]) -> String {
    let s = String::from_utf8_lossy(data).into_owned();
    s.trim_matches(char::from(0)).trim().to_string()
}

fn parse_identify_controller(data: &[u8]) -> NVMeIdentifyInfo {
    if data.len() < 72 {
        return NVMeIdentifyInfo::default();
    }
    NVMeIdentifyInfo {
        serial_number: parse_ascii_field(&data[4..24]),
        model: parse_ascii_field(&data[24..64]),
        firmware_version: parse_ascii_field(&data[64..72]),
    }
}
