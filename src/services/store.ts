import { create } from 'zustand'
import type { DiskInfo, WriteConfig, SystemInfo } from '@/types'

interface AppState {
  // System
  systemInfo: SystemInfo | null
  setSystemInfo: (info: SystemInfo) => void

  // Disks
  disks: DiskInfo[]
  selectedDisk: DiskInfo | null
  setDisks: (disks: DiskInfo[]) => void
  setSelectedDisk: (disk: DiskInfo | null) => void

  // Configuration
  imagePath: string
  setImagePath: (path: string) => void

  // Write state
  isWriting: boolean
  writeProgress: number
  writeSpeed: number
  estimatedTime: number
  setWriting: (writing: boolean) => void
  setWriteProgress: (progress: number) => void
  setWriteSpeed: (speed: number) => void
  setEstimatedTime: (time: number) => void

  // Language
  language: 'en' | 'zh-Hans' | 'zh-Hant'
  setLanguage: (lang: 'en' | 'zh-Hans' | 'zh-Hant') => void

  // Error handling
  error: string | null
  setError: (error: string | null) => void
}

export const useAppStore = create<AppState>((set) => ({
  // System
  systemInfo: null,
  setSystemInfo: (info) => set({ systemInfo: info }),

  // Disks
  disks: [],
  selectedDisk: null,
  setDisks: (disks) => set({ disks }),
  setSelectedDisk: (disk) => set({ selectedDisk: disk }),

  // Configuration
  imagePath: '',
  setImagePath: (path) => set({ imagePath: path }),

  // Write state
  isWriting: false,
  writeProgress: 0,
  writeSpeed: 0,
  estimatedTime: 0,
  setWriting: (writing) => set({ isWriting: writing }),
  setWriteProgress: (progress) => set({ writeProgress: progress }),
  setWriteSpeed: (speed) => set({ writeSpeed: speed }),
  setEstimatedTime: (time) => set({ estimatedTime: time }),

  // Language
  language: 'en',
  setLanguage: (lang) => set({ language: lang }),

  // Error handling
  error: null,
  setError: (error) => set({ error }),
}))
