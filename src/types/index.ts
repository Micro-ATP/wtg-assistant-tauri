export interface Disk {
  id: string
  name: string
  size: number
  removable: boolean
  device: string
}

export interface DiskInfo {
  id: string
  name: string
  size: number
  removable: boolean
  device: string
}

export interface PartitionConfig {
  boot_size: number
  partition_layout: 'mbr' | 'gpt'
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
