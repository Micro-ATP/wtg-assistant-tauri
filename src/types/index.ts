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
  media_type?: string
  index?: string
  volume?: string
  free?: number
  is_system?: boolean
}

export interface PartitionInfo {
  drive_letter: string
  label: string
  filesystem: string
  size: number
  free: number
  disk_number: number
  protocol: string
  media_type: string
  has_windows: boolean
  windows_name: string
}

export type PartitionLayout = 'mbr' | 'gpt'
export type ApplyMode = 'legacy' | 'vhd' | 'vhdx'
export type BootMode = 'uefi_gpt' | 'uefi_mbr' | 'non_uefi'
export type VhdType = 'fixed' | 'expandable'
export type ImageType = 'wim' | 'esd' | 'iso' | 'vhd' | 'vhdx'
export type FirmwareType = 'bios' | 'uefi' | 'all'
export type BootRepairFirmware = 'bios' | 'uefi' | 'all'
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
  cpu_model: string
  total_memory: number
  available_memory: number
  cpu_count: number
}

export interface MacosAdminSessionStatus {
  supported: boolean
  authorized: boolean
  authorized_at_unix?: number | null
  last_error?: string | null
}

export interface MacosTargetWritableCheck {
  supported: boolean
  disk_id: string
  partition_id?: string | null
  mount_point?: string | null
  filesystem: string
  writable_volume: boolean
  dir_writable: boolean
  writable: boolean
  needs_ntfs_remount: boolean
  reason?: string | null
}

export interface BenchmarkResult {
  write_seq: number
  write_4k: number
  write_4k_raw?: number
  write_4k_adjusted?: number
  write_4k_samples: { x: number; y: number }[]
  duration_ms: number
  mode: string
  thread_results: { threads: number; mb_s: number }[]
  full_seq_samples: { t_ms: number; value: number; x_gb: number }[]
  scenario_samples: { x: number; y: number }[]
  scenario_total_io?: number
  scenario_score?: number
  score?: number
  grade?: string
  full_written_gb: number
}

export interface SmartAttribute {
  id: number
  name: string
  current?: number
  worst?: number
  threshold?: number
  raw?: number
  raw_hex: string
}

export interface DiskDiagnostics {
  id: string
  disk_number: number
  model: string
  friendly_name: string
  serial_number: string
  firmware_version: string
  interface_type: string
  pnp_device_id: string
  usb_vendor_id: string
  usb_product_id: string
  transport_type: string
  is_usb: boolean
  bus_type: string
  unique_id: string
  media_type: string
  size_bytes: number
  is_system: boolean
  health_status: string
  smart_supported: boolean
  smart_enabled: boolean
  smart_data_source: string
  ata_smart_available: boolean
  reliability_available: boolean
  temperature_c?: number
  power_on_hours?: number
  power_cycle_count?: number
  percentage_used?: number
  read_errors_total?: number
  write_errors_total?: number
  host_reads_total?: number
  host_writes_total?: number
  smart_attributes: SmartAttribute[]
  reliability: Record<string, unknown> | null
  notes: string[]
}

export interface HardwareOverview {
  processors: string[]
  motherboard: string
  memory_summary: string
  graphics: string[]
  monitors: string[]
  disks: string[]
  audio_devices: string[]
  network_adapters: string[]
}

export interface MacosPluginItem {
  id: string
  name: string
  description: string
  installed: boolean
}

export interface MacosPluginInstallStatus {
  running: boolean
  plugin_id?: string | null
}

export interface MacosPluginInstallEvent {
  phase: string
  plugin_id: string
  plugin_name: string
  stream: string
  line: string
  exit_code?: number | null
  success?: boolean | null
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
    block_local_disk: true,
    disable_winre: true,
    skip_oobe: false,
    disable_uasp: false,
    enable_bitlocker: false,
    fix_letter: false,
    no_default_drive_letter: false,
    compact_os: false,
    wimboot: false,
    ntfs_uefi_support: false,
    do_not_format: false,
    repartition: false,
  }
}
