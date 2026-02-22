import { invoke } from '@tauri-apps/api/tauri'
import type {
  Disk,
  DiskInfo,
  SystemInfo,
  UsbDevice,
  UsbEvent,
  WriteConfig,
} from '@/types'

/**
 * Disk Operations API
 */
export const diskApi = {
  listDisks: async (): Promise<DiskInfo[]> => {
    try {
      const disks = await invoke<DiskInfo[]>('list_disks')
      return disks
    } catch (error) {
      console.error('Failed to list disks:', error)
      throw error
    }
  },

  getDiskInfo: async (diskId: string): Promise<DiskInfo> => {
    try {
      const info = await invoke<DiskInfo>('get_disk_info', { disk_id: diskId })
      return info
    } catch (error) {
      console.error('Failed to get disk info:', error)
      throw error
    }
  },
}

/**
 * USB Operations API
 */
export const usbApi = {
  startMonitoring: async (appHandle: any): Promise<string> => {
    try {
      const monitorId = await invoke<string>('start_usb_monitoring', {
        app_handle: appHandle,
      })
      return monitorId
    } catch (error) {
      console.error('Failed to start USB monitoring:', error)
      throw error
    }
  },

  stopMonitoring: async (monitorId: string): Promise<void> => {
    try {
      await invoke('stop_usb_monitoring', { monitor_id: monitorId })
    } catch (error) {
      console.error('Failed to stop USB monitoring:', error)
      throw error
    }
  },
}

/**
 * System Operations API
 */
export const systemApi = {
  getSystemInfo: async (): Promise<SystemInfo> => {
    try {
      const info = await invoke<SystemInfo>('get_system_info')
      return info
    } catch (error) {
      console.error('Failed to get system info:', error)
      throw error
    }
  },
}

/**
 * Write Operations API
 */
export const writeApi = {
  startWrite: async (config: WriteConfig): Promise<string> => {
    try {
      const taskId = await invoke<string>('start_write', { config })
      return taskId
    } catch (error) {
      console.error('Failed to start write operation:', error)
      throw error
    }
  },

  pauseWrite: async (taskId: string): Promise<void> => {
    try {
      await invoke('pause_write', { task_id: taskId })
    } catch (error) {
      console.error('Failed to pause write operation:', error)
      throw error
    }
  },

  resumeWrite: async (taskId: string): Promise<void> => {
    try {
      await invoke('resume_write', { task_id: taskId })
    } catch (error) {
      console.error('Failed to resume write operation:', error)
      throw error
    }
  },

  cancelWrite: async (taskId: string): Promise<void> => {
    try {
      await invoke('cancel_write', { task_id: taskId })
    } catch (error) {
      console.error('Failed to cancel write operation:', error)
      throw error
    }
  },
}
