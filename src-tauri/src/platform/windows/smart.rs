// Windows SMART API implementation
// Migrated from CrystalDiskInfo (MIT License)
// Original Author: hiyohiyo (https://crystalmark.info/)

pub mod nvme;

use std::mem;
use windows::core::PCSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;

const READ_ATTRIBUTE_BUFFER_SIZE: usize = 512;

// ATA Commands
const SMART_CMD: u8 = 0xB0;

// SMART Sub Commands
const READ_ATTRIBUTES: u8 = 0xD0;
const READ_THRESHOLDS: u8 = 0xD1;

// IOCTL codes
const DFP_RECEIVE_DRIVE_DATA: u32 = 0x0007C088;
const IOCTL_ATA_PASS_THROUGH: u32 = 0x0004D02C;
const IOCTL_SCSI_PASS_THROUGH: u32 = 0x0004D004;

// ATA Pass Through flags
const ATA_FLAGS_DRDY_REQUIRED: u16 = 0x01;
const ATA_FLAGS_DATA_IN: u16 = 0x02;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const SCSI_IOCTL_DATA_IN: u8 = 1;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IdeRegs {
    features_register: u8,
    sector_count_register: u8,
    sector_number_register: u8,
    cyl_low_register: u8,
    cyl_high_register: u8,
    drive_head_register: u8,
    command_register: u8,
    reserved: u8,
}

#[repr(C)]
struct SendCmdInParams {
    buffer_size: u32,
    irdrives_regs: IdeRegs,
    drive_number: u8,
    reserved: [u8; 3],
    reserved2: [u32; 4],
    buffer: [u8; 1],
}

#[repr(C)]
struct DriverStatus {
    driver_error: u8,
    ide_status: u8,
    reserved: [u8; 2],
    reserved2: [u32; 2],
}

#[repr(C)]
struct SendCmdOutParams {
    buffer_size: u32,
    driver_status: DriverStatus,
    buffer: [u8; 1],
}

#[repr(C)]
struct AtaPassThroughEx {
    length: u16,
    ata_flags: u16,
    path_id: u8,
    target_id: u8,
    lun: u8,
    reserved_as_uchar: u8,
    data_transfer_length: u32,
    timeout_value: u32,
    reserved_as_ulong: u32,
    data_buffer_offset: usize,
    previous_task_file: IdeRegs,
    current_task_file: IdeRegs,
}

#[repr(C)]
struct AtaPassThroughExWithBuffers {
    apt: AtaPassThroughEx,
    filler: u32,
    buf: [u8; 512],
}

#[repr(C)]
struct ScsiPassThrough {
    length: u16,
    scsi_status: u8,
    path_id: u8,
    target_id: u8,
    lun: u8,
    cdb_length: u8,
    sense_info_length: u8,
    data_in: u8,
    data_transfer_length: u32,
    time_out_value: u32,
    data_buffer_offset: u32,
    sense_info_offset: u32,
    cdb: [u8; 16],
}

#[repr(C)]
struct ScsiPassThroughWithBuffers {
    spt: ScsiPassThrough,
    filler: u32,
    sense_buf: [u8; 32],
    data_buf: [u8; 512],
}

pub struct SmartData {
    pub read_method: SmartReadMethod,
    pub thresholds_available: bool,
    pub attributes: Vec<SmartAttribute>,
    pub temperature: Option<i32>,
    pub power_on_hours: Option<u64>,
    pub power_cycle_count: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SmartAttribute {
    pub id: u8,
    pub current: u8,
    pub worst: u8,
    pub threshold: u8,
    pub raw: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum SmartReadMethod {
    AtaPassThrough,
    PhysicalDrive,
    SatBridge,
}

pub struct DiskHandle {
    handle: HANDLE,
}

#[derive(Clone, Copy)]
enum SatPattern {
    Ata12(u8),
    Ata16(u8),
    Sunplus,
    IoData,
    Logitec,
    Prolific,
    JMicron,
    Cypress,
}

impl DiskHandle {
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
                        return Ok(DiskHandle { handle });
                    }
                }
            }
        }

        Err(format!(
            "Failed to open disk path {} with required access",
            path.trim_end_matches('\0')
        ))
    }

    pub fn read_smart_data(&self) -> Result<SmartData, String> {
        // Try different methods in order of preference
        if let Ok(data) = self.read_smart_ata_pass_through() {
            return Ok(data);
        }

        if let Ok(data) = self.read_smart_physical_drive() {
            return Ok(data);
        }

        // SAT over SCSI bridge fallback (common for USB-SATA enclosures)
        if let Ok(data) = self.read_smart_sat() {
            return Ok(data);
        }

        Err("Failed to read SMART data using any method".to_string())
    }

    fn read_smart_physical_drive(&self) -> Result<SmartData, String> {
        // Read SMART attributes
        let attr_data = self.send_smart_command(READ_ATTRIBUTES)?;

        // Threshold table is optional on some bridges/controllers.
        let threshold_data = self.send_smart_command(READ_THRESHOLDS).ok();
        parse_ata_smart_tables(
            &attr_data,
            threshold_data.as_deref(),
            SmartReadMethod::PhysicalDrive,
        )
    }

    fn read_smart_ata_pass_through(&self) -> Result<SmartData, String> {
        // Read SMART attributes using ATA Pass Through
        let attr_data = self.send_ata_pass_through_command(SMART_CMD, READ_ATTRIBUTES)?;
        let threshold_data = self
            .send_ata_pass_through_command(SMART_CMD, READ_THRESHOLDS)
            .ok();

        parse_ata_smart_tables(
            &attr_data,
            threshold_data.as_deref(),
            SmartReadMethod::AtaPassThrough,
        )
    }

    fn read_smart_sat(&self) -> Result<SmartData, String> {
        let attr_data = self.send_sat_smart_command(READ_ATTRIBUTES)?;
        let threshold_data = self.send_sat_smart_command(READ_THRESHOLDS).ok();
        parse_ata_smart_tables(
            &attr_data,
            threshold_data.as_deref(),
            SmartReadMethod::SatBridge,
        )
    }

    fn send_smart_command(&self, sub_command: u8) -> Result<Vec<u8>, String> {
        unsafe {
            let mut in_params = vec![0u8; mem::size_of::<SendCmdInParams>()];
            let params = in_params.as_mut_ptr() as *mut SendCmdInParams;

            (*params).buffer_size = READ_ATTRIBUTE_BUFFER_SIZE as u32;
            (*params).irdrives_regs.features_register = sub_command;
            (*params).irdrives_regs.sector_count_register = 1;
            (*params).irdrives_regs.sector_number_register = 1;
            (*params).irdrives_regs.cyl_low_register = 0x4F;
            (*params).irdrives_regs.cyl_high_register = 0xC2;
            (*params).irdrives_regs.drive_head_register = 0xA0;
            (*params).irdrives_regs.command_register = SMART_CMD;

            let out_size = mem::size_of::<SendCmdOutParams>() + READ_ATTRIBUTE_BUFFER_SIZE - 1;
            let mut out_params = vec![0u8; out_size];
            let mut bytes_returned: u32 = 0;

            let result = DeviceIoControl(
                self.handle,
                DFP_RECEIVE_DRIVE_DATA,
                Some(in_params.as_ptr() as *const _),
                in_params.len() as u32,
                Some(out_params.as_mut_ptr() as *mut _),
                out_params.len() as u32,
                Some(&mut bytes_returned),
                None,
            );

            if result.is_err() {
                return Err("DeviceIoControl failed".to_string());
            }

            // Extract data from output buffer
            let _out_ptr = out_params.as_ptr() as *const SendCmdOutParams;
            let data_offset = mem::size_of::<SendCmdOutParams>() - 1;
            let data = out_params[data_offset..data_offset + READ_ATTRIBUTE_BUFFER_SIZE].to_vec();

            Ok(data)
        }
    }

    fn send_ata_pass_through_command(&self, command: u8, features: u8) -> Result<Vec<u8>, String> {
        unsafe {
            let mut apt_buf = AtaPassThroughExWithBuffers {
                apt: AtaPassThroughEx {
                    length: mem::size_of::<AtaPassThroughEx>() as u16,
                    ata_flags: ATA_FLAGS_DATA_IN | ATA_FLAGS_DRDY_REQUIRED,
                    path_id: 0,
                    target_id: 0,
                    lun: 0,
                    reserved_as_uchar: 0,
                    data_transfer_length: 512,
                    timeout_value: 2,
                    reserved_as_ulong: 0,
                    data_buffer_offset: mem::size_of::<AtaPassThroughEx>() + 4,
                    previous_task_file: mem::zeroed(),
                    current_task_file: IdeRegs {
                        features_register: features,
                        sector_count_register: 1,
                        sector_number_register: 1,
                        cyl_low_register: 0x4F,
                        cyl_high_register: 0xC2,
                        drive_head_register: 0xA0,
                        command_register: command,
                        reserved: 0,
                    },
                },
                filler: 0,
                buf: [0; 512],
            };

            let mut bytes_returned: u32 = 0;
            let buffer_size = mem::size_of::<AtaPassThroughExWithBuffers>();

            let result = DeviceIoControl(
                self.handle,
                IOCTL_ATA_PASS_THROUGH,
                Some(&apt_buf as *const _ as *const _),
                buffer_size as u32,
                Some(&mut apt_buf as *mut _ as *mut _),
                buffer_size as u32,
                Some(&mut bytes_returned),
                None,
            );

            if result.is_err() {
                return Err("ATA Pass Through failed".to_string());
            }

            Ok(apt_buf.buf.to_vec())
        }
    }

    fn send_sat_smart_command(&self, sub_command: u8) -> Result<Vec<u8>, String> {
        // CrystalDiskInfo style fallback attempts for bridge variations.
        let attempts = [
            SatPattern::Ata12(0x2E),
            SatPattern::Ata16(0x2E),
            SatPattern::Ata12(0x0E),
            SatPattern::Ata16(0x0E),
            SatPattern::JMicron,
            SatPattern::Sunplus,
            SatPattern::IoData,
            SatPattern::Logitec,
            SatPattern::Prolific,
            SatPattern::Cypress,
        ];
        let mut last_err = String::new();

        for pattern in attempts {
            match self.send_sat_smart_command_once(sub_command, pattern) {
                Ok(data) if data.iter().any(|b| *b != 0) => return Ok(data),
                Ok(_) => {
                    last_err = "SAT returned empty buffer".to_string();
                }
                Err(e) => {
                    last_err = e;
                }
            }
        }

        if last_err.is_empty() {
            Err("SAT SMART command failed".to_string())
        } else {
            Err(last_err)
        }
    }

    fn send_sat_smart_command_once(
        &self,
        sub_command: u8,
        pattern: SatPattern,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            let cdb_length = sat_pattern_cdb_length(pattern);
            let mut sptwb = ScsiPassThroughWithBuffers {
                spt: ScsiPassThrough {
                    length: mem::size_of::<ScsiPassThrough>() as u16,
                    scsi_status: 0,
                    path_id: 0,
                    target_id: 0,
                    lun: 0,
                    cdb_length,
                    sense_info_length: 32,
                    data_in: SCSI_IOCTL_DATA_IN,
                    data_transfer_length: 512,
                    time_out_value: 4,
                    data_buffer_offset: (mem::size_of::<ScsiPassThrough>() + 4 + 32) as u32,
                    sense_info_offset: (mem::size_of::<ScsiPassThrough>() + 4) as u32,
                    cdb: [0; 16],
                },
                filler: 0,
                sense_buf: [0; 32],
                data_buf: [0; 512],
            };

            build_sat_cdb(&mut sptwb.spt.cdb, sub_command, pattern);

            let mut bytes_returned = 0_u32;
            let size = mem::size_of::<ScsiPassThroughWithBuffers>() as u32;
            let result = DeviceIoControl(
                self.handle,
                IOCTL_SCSI_PASS_THROUGH,
                Some(&sptwb as *const _ as *const _),
                size,
                Some(&mut sptwb as *mut _ as *mut _),
                size,
                Some(&mut bytes_returned),
                None,
            );

            if result.is_err() {
                return Err("SCSI/SAT pass-through failed".to_string());
            }

            Ok(sptwb.data_buf.to_vec())
        }
    }
}

impl Drop for DiskHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl SmartData {
    fn from_attributes(
        attributes: &[SmartAttribute],
        read_method: SmartReadMethod,
        thresholds_available: bool,
    ) -> Self {
        let temperature = attributes
            .iter()
            .find(|a| a.id == 194 || a.id == 190)
            .map(|a| {
                let raw = a.raw;
                if raw <= 200 {
                    raw as i32
                } else {
                    (raw & 0xFF) as i32
                }
            });

        let power_on_hours = attributes.iter().find(|a| a.id == 9).map(|a| a.raw);

        let power_cycle_count = attributes.iter().find(|a| a.id == 12).map(|a| a.raw);

        SmartData {
            read_method,
            thresholds_available,
            attributes: attributes.to_vec(),
            temperature,
            power_on_hours,
            power_cycle_count,
        }
    }

    pub fn get_attribute(&self, id: u8) -> Option<&SmartAttribute> {
        self.attributes.iter().find(|a| a.id == id)
    }
}

fn parse_ata_smart_tables(
    attr_data: &[u8],
    threshold_data: Option<&[u8]>,
    method: SmartReadMethod,
) -> Result<SmartData, String> {
    if attr_data.len() < 362 {
        return Err("SMART attribute table payload too small".to_string());
    }

    let threshold_data = threshold_data.filter(|buf| buf.len() >= 362);
    let thresholds_available = threshold_data.is_some();
    let mut attributes = Vec::new();
    for i in (2..362).step_by(12) {
        let id = attr_data[i];
        if id == 0 {
            continue;
        }

        let current = attr_data[i + 3];
        let worst = attr_data[i + 4];
        let raw = u64::from_le_bytes([
            attr_data[i + 5],
            attr_data[i + 6],
            attr_data[i + 7],
            attr_data[i + 8],
            attr_data[i + 9],
            attr_data[i + 10],
            0,
            0,
        ]);

        let mut threshold = 0u8;
        if let Some(tables) = threshold_data {
            for j in (2..362).step_by(12) {
                if tables[j] == id {
                    threshold = tables[j + 1];
                    break;
                }
            }
        }

        attributes.push(SmartAttribute {
            id,
            current,
            worst,
            threshold,
            raw,
        });
    }

    if attributes.is_empty() {
        return Err("SMART attributes are empty".to_string());
    }

    Ok(SmartData::from_attributes(
        &attributes,
        method,
        thresholds_available,
    ))
}

fn sat_pattern_cdb_length(pattern: SatPattern) -> u8 {
    match pattern {
        SatPattern::Ata16(_) | SatPattern::Prolific | SatPattern::Cypress => 16,
        SatPattern::Logitec => 10,
        _ => 12,
    }
}

fn build_sat_cdb(cdb: &mut [u8; 16], sub_command: u8, pattern: SatPattern) {
    let target = 0xA0_u8;

    match pattern {
        SatPattern::Ata12(flags) => {
            cdb[0] = 0xA1;
            cdb[1] = 0x08;
            cdb[2] = flags;
            cdb[3] = sub_command;
            cdb[4] = 0x01;
            cdb[5] = 0x01;
            cdb[6] = 0x4F;
            cdb[7] = 0xC2;
            cdb[8] = target;
            cdb[9] = SMART_CMD;
        }
        SatPattern::Ata16(flags) => {
            cdb[0] = 0x85;
            cdb[1] = 0x08;
            cdb[2] = flags;
            cdb[4] = sub_command;
            cdb[6] = 0x01;
            cdb[10] = 0x4F;
            cdb[12] = 0xC2;
            cdb[13] = target;
            cdb[14] = SMART_CMD;
        }
        SatPattern::Sunplus => {
            cdb[0] = 0xF8;
            cdb[2] = 0x22;
            cdb[3] = 0x10;
            cdb[4] = 0x01;
            cdb[5] = sub_command;
            cdb[6] = 0x01;
            cdb[8] = 0x4F;
            cdb[9] = 0xC2;
            cdb[10] = target;
            cdb[11] = SMART_CMD;
        }
        SatPattern::IoData => {
            cdb[0] = 0xE3;
            cdb[2] = sub_command;
            cdb[5] = 0x4F;
            cdb[6] = 0xC2;
            cdb[7] = target;
            cdb[8] = SMART_CMD;
        }
        SatPattern::Logitec => {
            cdb[0] = 0xE0;
            cdb[2] = sub_command;
            cdb[5] = 0x4F;
            cdb[6] = 0xC2;
            cdb[7] = target;
            cdb[8] = SMART_CMD;
            cdb[9] = 0x4C;
        }
        SatPattern::Prolific => {
            cdb[0] = 0xD8;
            cdb[1] = 0x15;
            cdb[3] = sub_command;
            cdb[4] = 0x06;
            cdb[5] = 0x7B;
            cdb[8] = 0x02;
            cdb[10] = 0x01;
            cdb[12] = 0x4F;
            cdb[13] = 0xC2;
            cdb[14] = target;
            cdb[15] = SMART_CMD;
        }
        SatPattern::JMicron => {
            cdb[0] = 0xDF;
            cdb[1] = 0x10;
            cdb[3] = 0x02;
            cdb[5] = sub_command;
            cdb[6] = 0x01;
            cdb[7] = 0x01;
            cdb[8] = 0x4F;
            cdb[9] = 0xC2;
            cdb[10] = target;
            cdb[11] = SMART_CMD;
        }
        SatPattern::Cypress => {
            cdb[0] = 0x24;
            cdb[1] = 0x24;
            cdb[3] = 0xBE;
            cdb[4] = 0x01;
            cdb[6] = sub_command;
            cdb[9] = 0x4F;
            cdb[10] = 0xC2;
            cdb[11] = target;
            cdb[12] = SMART_CMD;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_disk() {
        // This test requires admin privileges
        if let Ok(handle) = DiskHandle::open(0) {
            println!("Successfully opened PhysicalDrive0");
            if let Ok(smart_data) = handle.read_smart_data() {
                println!("Temperature: {:?}", smart_data.temperature);
                println!("Power On Hours: {:?}", smart_data.power_on_hours);
                println!("Attributes count: {}", smart_data.attributes.len());
            }
        }
    }
}
