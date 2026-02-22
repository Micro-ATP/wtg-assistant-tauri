#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// ============================================================
// Core Enums (translated from WTGModel.cs)
// ============================================================

/// Partition layout type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PartitionLayout {
    MBR,
    GPT,
}

/// Write mode - how the Windows image is applied
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApplyMode {
    /// Direct image apply to disk
    Legacy,
    /// Apply into a VHD container
    VHD,
    /// Apply into a VHDX container
    VHDX,
}

/// Boot firmware type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FirmwareType {
    BIOS,
    UEFI,
    /// Both BIOS and UEFI
    ALL,
}

/// Boot mode combining firmware + partition layout
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BootMode {
    /// UEFI + GPT partition layout
    UefiGpt,
    /// UEFI + MBR partition layout
    UefiMbr,
    /// Legacy BIOS (non-UEFI)
    NonUefi,
}

/// VHD disk type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VhdType {
    Fixed,
    Expandable,
}

/// Image file type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImageType {
    Wim,
    Esd,
    Iso,
    Vhd,
    Vhdx,
}

/// Write operation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WriteStatus {
    Idle,
    Preparing,
    Partitioning,
    ApplyingImage,
    WritingBootFiles,
    FixingBcd,
    CopyingVhd,
    ApplyingExtras,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

// ============================================================
// Data Structures
// ============================================================

/// Disk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
    pub device: String,
    #[serde(default)]
    pub drive_type: String,
    #[serde(default)]
    pub index: String,
    #[serde(default)]
    pub volume: String,
}

/// Partition configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    pub boot_size: u32,
    pub partition_layout: PartitionLayout,
    /// Extra partition sizes in MB (for multi-partition setups)
    #[serde(default)]
    pub extra_partition_sizes: Vec<u32>,
}

/// VHD configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VhdConfig {
    /// VHD size in MB (0 = auto)
    pub size_mb: u32,
    /// Fixed or expandable
    pub vhd_type: VhdType,
    /// VHD or VHDX
    pub extension: String,
    /// Custom VHD filename (without extension)
    #[serde(default = "default_vhd_name")]
    pub filename: String,
    /// VHD partition type (0=MBR, 1=GPT)
    #[serde(default)]
    pub partition_type: u8,
}

fn default_vhd_name() -> String {
    "win8".to_string()
}

/// Extra features configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtraFeatures {
    /// Install .NET Framework 3.5
    pub install_dotnet35: bool,
    /// Block local disk (SAN policy)
    pub block_local_disk: bool,
    /// Disable Windows Recovery Environment
    pub disable_winre: bool,
    /// Skip OOBE (Out-of-Box Experience)
    pub skip_oobe: bool,
    /// Disable UASP
    pub disable_uasp: bool,
    /// Enable Bitlocker
    pub enable_bitlocker: bool,
    /// Fix drive letter
    pub fix_letter: bool,
    /// Set no default drive letter after write
    pub no_default_drive_letter: bool,
    /// Enable CompactOS
    pub compact_os: bool,
    /// Enable WIMBoot
    pub wimboot: bool,
    /// NTFS UEFI support (for non-UEFI mode with UEFI support)
    pub ntfs_uefi_support: bool,
    /// Don't format the disk (use as-is)
    pub do_not_format: bool,
    /// Re-partition the disk
    pub repartition: bool,
    /// Custom driver injection directory
    pub driver_path: Option<String>,
}

/// Complete WTG write configuration (equivalent to WTGModel.cs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WtgConfig {
    /// Path to the Windows image file (ISO/WIM/ESD/VHD/VHDX)
    pub image_path: String,
    /// The image file type
    pub image_type: ImageType,
    /// Selected WIM index (0 = auto)
    #[serde(default)]
    pub wim_index: String,
    /// Target disk
    pub target_disk: Disk,
    /// Boot mode
    pub boot_mode: BootMode,
    /// Apply mode
    pub apply_mode: ApplyMode,
    /// Partition configuration
    pub partition_config: PartitionConfig,
    /// VHD configuration (used when apply_mode is VHD/VHDX)
    pub vhd_config: Option<VhdConfig>,
    /// Extra features
    #[serde(default)]
    pub extra_features: ExtraFeatures,
    /// EFI partition size in MB
    #[serde(default = "default_efi_size")]
    pub efi_partition_size: String,
    /// Custom EFI partition path (optional)
    pub efi_partition_path: Option<String>,
}

fn default_efi_size() -> String {
    "300".to_string()
}

/// Write progress information emitted to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteProgress {
    pub task_id: String,
    pub status: WriteStatus,
    pub progress: f64,
    pub message: String,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub elapsed_seconds: u64,
    #[serde(default)]
    pub estimated_remaining_seconds: u64,
}

/// Image information from DISM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub index: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub size: u64,
}

/// Backward-compatible simple WriteConfig for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConfig {
    pub image_path: String,
    pub target_disk: String,
    pub partition_config: PartitionConfig,
    pub fast_write: bool,
}
