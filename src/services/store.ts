import { create } from 'zustand'
import type {
  DiskInfo,
  SystemInfo,
  ApplyMode,
  BootMode,
  ExtraFeatures,
  WriteProgress,
  ImageInfo,
} from '../types'
import { defaultExtraFeatures } from '../types'

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

  // Boot & Apply mode
  bootMode: BootMode
  setBootMode: (mode: BootMode) => void
  applyMode: ApplyMode
  setApplyMode: (mode: ApplyMode) => void

  // VHD settings
  vhdSizeMb: number
  setVhdSizeMb: (size: number) => void
  vhdType: 'fixed' | 'expandable'
  setVhdType: (type: 'fixed' | 'expandable') => void
  vhdExtension: 'vhd' | 'vhdx'
  setVhdExtension: (ext: 'vhd' | 'vhdx') => void

  // EFI partition
  efiPartitionSize: string
  setEfiPartitionSize: (size: string) => void

  // Extra features
  extraFeatures: ExtraFeatures
  setExtraFeatures: (features: ExtraFeatures) => void
  toggleExtraFeature: (key: keyof ExtraFeatures) => void

  // Image info
  imageInfoList: ImageInfo[]
  setImageInfoList: (list: ImageInfo[]) => void
  selectedWimIndex: string
  setSelectedWimIndex: (index: string) => void

  // Write state
  isWriting: boolean
  writeProgress: WriteProgress | null
  setWriting: (writing: boolean) => void
  setWriteProgress: (progress: WriteProgress | null) => void

  // Language
  language: 'en' | 'zh-Hans' | 'zh-Hant'
  setLanguage: (lang: 'en' | 'zh-Hans' | 'zh-Hant') => void

  // Navigation
  currentPage: string
  setCurrentPage: (page: string) => void

  // Error handling
  error: string | null
  setError: (error: string | null) => void
}

export const useAppStore = create<AppState>((set) => ({
  systemInfo: null,
  setSystemInfo: (info) => set({ systemInfo: info }),

  disks: [],
  selectedDisk: null,
  setDisks: (disks) => set({ disks }),
  setSelectedDisk: (disk) => set({ selectedDisk: disk }),

  imagePath: '',
  setImagePath: (path) => set({ imagePath: path }),

  bootMode: 'uefi_gpt',
  setBootMode: (mode) => set({ bootMode: mode }),
  applyMode: 'legacy',
  setApplyMode: (mode) => set({ applyMode: mode }),

  vhdSizeMb: 40960,
  setVhdSizeMb: (size) => set({ vhdSizeMb: size }),
  vhdType: 'expandable',
  setVhdType: (type) => set({ vhdType: type }),
  vhdExtension: 'vhdx',
  setVhdExtension: (ext) => set({ vhdExtension: ext }),

  efiPartitionSize: '300',
  setEfiPartitionSize: (size) => set({ efiPartitionSize: size }),

  extraFeatures: defaultExtraFeatures(),
  setExtraFeatures: (features) => set({ extraFeatures: features }),
  toggleExtraFeature: (key) =>
    set((state) => ({
      extraFeatures: {
        ...state.extraFeatures,
        [key]: !state.extraFeatures[key],
      },
    })),

  imageInfoList: [],
  setImageInfoList: (list) => set({ imageInfoList: list }),
  selectedWimIndex: '0',
  setSelectedWimIndex: (index) => set({ selectedWimIndex: index }),

  isWriting: false,
  writeProgress: null,
  setWriting: (writing) => set({ isWriting: writing }),
  setWriteProgress: (progress) => set({ writeProgress: progress }),

  language: 'zh-Hans',
  setLanguage: (lang) => set({ language: lang }),

  currentPage: 'home',
  setCurrentPage: (page) => set({ currentPage: page }),

  error: null,
  setError: (error) => set({ error }),
}))
