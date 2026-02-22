export interface Disk {
  id: string
  name: string
  size: number
  removable: boolean
  device: string
  drive_type?: string
  index?: string
  volume?: string
}

export interface DiskInfo {
  id: string
  name: string
  size: number
  removable: boolean
  device: string
  drive_type?: string
  index?: string
  volume?: string
}

export type PartitionLayout = 'mbr' | 'gpt'
export type ApplyMode = 'legacy' | 'vhd' | 'vhdx'
export type BootMode = 'uefi_gpt' | 'uefi_mbr' | 'non_uefi'
export type VhdType = 'fixed' | 'expandable'
export type ImageType = 'wim' | 'esd' | 'iso' | 'vhd' | 'vhdx'
export type FirmwareType = 'bios' | 'uefi' | 'all'
export type WriteStatus =
  | 'idle'
  | 'preparing'
  | 'partitioning'
  | 'applyingimage'
  | 'writingbootfiles'
  | 'fixingbcd'
  | 'copyingvhd'
  | 'applyingextras'
  | 'verifying'
  | 'completed'
  | 'failed'
  | 'cancelled'

export interface PartitionConfig {
  boot_size: number
  partition_layout: PartitionLayout
  extra_partition_sizes?: number[]
}

export interface VhdConfig {
  size_mb: number
  vhd_type: VhdType
  extension: string
  filename?: string
  partition_type?: number
}

export interface ExtraFeatures {
  install_dotnet35: boolean
  block_local_disk: boolean
  disable_winre: boolean
  skip_oobe: boolean
  disable_uasp: boolean
  enable_bitlocker: boolean
  fix_letter: boolean
  no_default_drive_letter: boolean
  compact_os: boolean
  wimboot: boolean
  ntfs_uefi_support: boolean
  do_not_format: boolean
  repartition: boolean
  driver_path?: string
}

export interface WtgConfig {
  image_path: string
  image_type: ImageType
  wim_index?: string
  target_disk: Disk
  boot_mode: BootMode
  apply_mode: ApplyMode
  partition_config: PartitionConfig
  vhd_config?: VhdConfig
  extra_features: ExtraFeatures
  efi_partition_size?: string
  efi_partition_path?: string
}

export interface WriteProgress {
  task_id: string
  status: WriteStatus
  progress: number
  message: string
  speed?: number
  elapsed_seconds?: number
  estimated_remaining_seconds?: number
}

export interface ImageInfo {
  index: number
  name: string
  description?: string
  size?: number
}

export interface WriteConfig {
  image_path: string
  target_disk: string
  partition_config: PartitionConfig
  fast_write: boolean
}

export interface SystemInfo {
  os: string
  arch: string
  version: string
  total_memory: number
  available_memory: number
  cpu_count: number
}

export interface UsbDevice {
  id: string
  name: string
  vendor: string
  product: string
  size: number
}

export type UsbEventType = 'connected' | 'disconnected'

export interface UsbEvent {
  event_type: UsbEventType
  device: UsbDevice
}

export function defaultExtraFeatures(): ExtraFeatures {
  return {
    install_dotnet35: false,
    block_local_disk: false,
    disable_winre: false,
    skip_oobe: false,
    disable_uasp: false,
    enable_bitlocker: false,
    fix_letter: true,
    no_default_drive_letter: false,
    compact_os: false,
    wimboot: false,
    ntfs_uefi_support: false,
    do_not_format: false,
    repartition: false,
  }
}
