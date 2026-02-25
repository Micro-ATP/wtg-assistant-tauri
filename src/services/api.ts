import { invoke } from '@tauri-apps/api/core'
import type {
  DiskInfo,
  SystemInfo,
  WtgConfig,
  WriteProgress,
  ImageInfo,
  BenchmarkResult,
  DiskDiagnostics,
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
      const info = await invoke<DiskInfo>('get_disk_info', { diskId })
      return info
    } catch (error) {
      console.error('Failed to get disk info:', error)
      throw error
    }
  },

  listDiskDiagnostics: async (): Promise<DiskDiagnostics[]> => {
    try {
      const diagnostics = await invoke<DiskDiagnostics[]>('list_disk_diagnostics')
      return diagnostics
    } catch (error) {
      console.error('Failed to get disk diagnostics:', error)
      throw error
    }
  },
}

/**
 * USB Operations API
 */
export const usbApi = {
  startMonitoring: async (appHandle: unknown): Promise<string> => {
    try {
      const monitorId = await invoke<string>('start_usb_monitoring', {
        appHandle,
      })
      return monitorId
    } catch (error) {
      console.error('Failed to start USB monitoring:', error)
      throw error
    }
  },

  stopMonitoring: async (monitorId: string): Promise<void> => {
    try {
      await invoke('stop_usb_monitoring', { monitorId })
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
 * Image Operations API
 */
export const imageApi = {
  getImageInfo: async (imagePath: string): Promise<ImageInfo[]> => {
    try {
      const info = await invoke<ImageInfo[]>('get_image_info', {
        imagePath,
      })
      return info
    } catch (error) {
      console.error('Failed to get image info:', error)
      throw error
    }
  },
}

/**
 * Write Operations API
 */
export const writeApi = {
  startWrite: async (config: WtgConfig): Promise<WriteProgress> => {
    try {
      const progress = await invoke<WriteProgress>('start_write', { config })
      return progress
    } catch (error) {
      console.error('Failed to start write operation:', error)
      throw error
    }
  },

  cancelWrite: async (taskId: string): Promise<void> => {
    try {
      await invoke('cancel_write', { taskId })
    } catch (error) {
      console.error('Failed to cancel write operation:', error)
      throw error
    }
  },

  verifySystemFiles: async (targetDisk: string): Promise<boolean> => {
    try {
      const result = await invoke<boolean>('verify_system_files', {
        targetDisk,
      })
      return result
    } catch (error) {
      console.error('Failed to verify system files:', error)
      throw error
    }
  },
}

/**
 * Benchmark API
 */
export const benchmarkApi = {
  run: async (
    targetPath: string,
    mode: 'quick' | 'multithread' | 'fullwrite' | 'full' | 'scenario' = 'quick',
  ): Promise<BenchmarkResult> => {
    try {
      const result = await invoke<BenchmarkResult>('run_benchmark', { targetPath, mode })
      return result
    } catch (error) {
      console.error('Failed to run benchmark:', error)
      throw error
    }
  },
  cancel: async (): Promise<void> => {
    try {
      await invoke('cancel_benchmark')
    } catch (error) {
      console.error('Failed to cancel benchmark:', error)
      throw error
    }
  },
}
