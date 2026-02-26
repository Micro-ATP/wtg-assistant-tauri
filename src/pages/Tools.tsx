import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { listen } from '@tauri-apps/api/event'
import { RefreshIcon, SpinnerIcon } from '../components/Icons'
import { diskApi, toolsApi } from '../services/api'
import { useAppStore } from '../services/store'
import type {
  BootRepairFirmware,
  DiskDiagnostics,
  HardwareOverview,
  MacosPluginInstallEvent,
  MacosPluginItem,
  PartitionInfo,
} from '../types'
import './Tools.css'

type ToolKey = 'hardwareInfo' | 'diskInfo' | 'bootRepair' | 'capacityCalc' | 'macosPlugins'
type CapacityUnitKey = 'B' | 'KB' | 'MB' | 'GB' | 'TB' | 'KiB' | 'MiB' | 'GiB' | 'TiB'

const CAPACITY_UNITS: Array<{ key: CapacityUnitKey; bytes: number }> = [
  { key: 'B', bytes: 1 },
  { key: 'KB', bytes: 1000 },
  { key: 'MB', bytes: 1000 ** 2 },
  { key: 'GB', bytes: 1000 ** 3 },
  { key: 'TB', bytes: 1000 ** 4 },
  { key: 'KiB', bytes: 1024 },
  { key: 'MiB', bytes: 1024 ** 2 },
  { key: 'GiB', bytes: 1024 ** 3 },
  { key: 'TiB', bytes: 1024 ** 4 },
]

const CAPACITY_PRESETS: Array<{ label: string; value: number; unit: CapacityUnitKey }> = [
  { label: '32 GB', value: 32, unit: 'GB' },
  { label: '64 GB', value: 64, unit: 'GB' },
  { label: '128 GB', value: 128, unit: 'GB' },
  { label: '256 GB', value: 256, unit: 'GB' },
  { label: '512 GB', value: 512, unit: 'GB' },
  { label: '1 TB', value: 1, unit: 'TB' },
  { label: '2 TB', value: 2, unit: 'TB' },
  { label: '4 TB', value: 4, unit: 'TB' },
  { label: '8 TB', value: 8, unit: 'TB' },
  { label: '14 TB', value: 14, unit: 'TB' },
]

const MACOS_PLUGIN_PLACEHOLDERS: MacosPluginItem[] = [
  {
    id: 'homebrew',
    name: 'Homebrew',
    description: '',
    installed: false,
  },
  {
    id: 'macfuse',
    name: 'macFUSE',
    description: '',
    installed: false,
  },
  {
    id: 'gromgit-homebrew-fuse',
    name: 'gromgit/homebrew-fuse tap',
    description: '',
    installed: false,
  },
  {
    id: 'ntfs-3g-mac',
    name: 'ntfs-3g-mac',
    description: '',
    installed: false,
  },
  {
    id: 'smartmontools',
    name: 'smartmontools',
    description: '',
    installed: false,
  },
  {
    id: 'wimlib',
    name: 'wimlib',
    description: '',
    installed: false,
  },
]

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const k = 1024
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`
}

function formatMediaHealth(percentageUsed: number | null | undefined): string {
  if (percentageUsed == null || Number.isNaN(percentageUsed)) return '--'
  const used = Math.min(100, Math.max(0, percentageUsed))
  const health = Math.max(0, 100 - used)
  const rounded = Math.abs(health - Math.round(health)) < 0.05 ? `${Math.round(health)}` : health.toFixed(1)
  return `${rounded}%`
}

function asFiniteNumber(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string') {
    const n = Number(value)
    return Number.isFinite(n) ? n : null
  }
  return null
}

function pickObjectNumber(obj: unknown, key: string): number | null {
  if (!obj || typeof obj !== 'object') return null
  return asFiniteNumber((obj as Record<string, unknown>)[key])
}

function estimateHostIoGb(diag: DiskDiagnostics, kind: 'read' | 'write', rawValue: number): number | null {
  const rel = diag.reliability
  const unitsKeys =
    kind === 'read'
      ? ['NvmeIoctl.DataUnitsRead', 'Nvme.DataUnitsRead']
      : ['NvmeIoctl.DataUnitsWritten', 'Nvme.DataUnitsWritten']

  for (const key of unitsKeys) {
    const units = pickObjectNumber(rel, key)
    if (units != null && units > 0) {
      return (units * 512000) / 1_000_000_000
    }
  }

  const source = (diag.smart_data_source || '').toUpperCase()
  if (source.includes('NVME') && rawValue > 0) {
    return (rawValue * 512000) / 1_000_000_000
  }

  const attrId = kind === 'read' ? 242 : 241
  const attrRaw = diag.smart_attributes?.find((a) => a.id === attrId)?.raw
  if (attrRaw != null && attrRaw > 0) {
    return (attrRaw * 512) / 1_000_000_000
  }

  if (source.includes('STORAGE_RELIABILITY') && rawValue > 0) {
    return rawValue / 1_000_000_000
  }

  if (rawValue >= 1_000_000_000) {
    return rawValue / 1_000_000_000
  }

  return null
}

function formatAdaptiveCapacityFromGb(gb: number): string {
  if (!Number.isFinite(gb) || gb <= 0) return '0 GB'

  if (gb < 1) {
    const mb = gb * 1000
    if (mb >= 1) return `${mb.toFixed(mb >= 100 ? 0 : 1)} MB`
    const kb = mb * 1000
    return `${kb.toFixed(kb >= 100 ? 0 : 1)} KB`
  }

  const units = ['GB', 'TB', 'PB', 'EB']
  let value = gb
  let unitIndex = 0
  while (value >= 1000 && unitIndex < units.length - 1) {
    value /= 1000
    unitIndex += 1
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unitIndex]}`
}

function formatHostIoWithGb(diag: DiskDiagnostics, value: number | null | undefined, kind: 'read' | 'write'): string {
  if (value == null || !Number.isFinite(value)) return '--'
  const gb = estimateHostIoGb(diag, kind, value)
  if (gb == null || !Number.isFinite(gb) || gb <= 0) return `${value}`
  return `${value} (${formatAdaptiveCapacityFromGb(gb)})`
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '--'
  if (Array.isArray(value)) return value.map((item) => String(item)).join(', ')
  if (typeof value === 'number') return String(value)
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  return String(value)
}

function formatConversion(value: number): string {
  if (!Number.isFinite(value)) return '--'

  const toPlainString = (num: number): string => {
    const raw = String(num)
    if (!/[eE]/.test(raw)) return raw

    const [mantissaPart, exponentPart] = raw.toLowerCase().split('e')
    const exponent = Number(exponentPart)
    if (!Number.isFinite(exponent)) return raw

    const sign = mantissaPart.startsWith('-') ? '-' : ''
    const mantissa = mantissaPart.replace('-', '')
    const dotIndex = mantissa.indexOf('.')
    const digits = mantissa.replace('.', '')
    const fractionDigits = dotIndex >= 0 ? mantissa.length - dotIndex - 1 : 0
    const decimalShift = exponent - fractionDigits

    if (decimalShift >= 0) {
      return `${sign}${digits}${'0'.repeat(decimalShift)}`
    }

    const pointPos = digits.length + decimalShift
    if (pointPos > 0) {
      return `${sign}${digits.slice(0, pointPos)}.${digits.slice(pointPos)}`
    }
    return `${sign}0.${'0'.repeat(Math.abs(pointPos))}${digits}`
  }

  const withGrouping = (plain: string): string => {
    const sign = plain.startsWith('-') ? '-' : ''
    const unsigned = sign ? plain.slice(1) : plain
    const [intPart, fracPart = ''] = unsigned.split('.')
    const groupedInt = intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    const trimmedFrac = fracPart.replace(/0+$/, '')
    return trimmedFrac ? `${sign}${groupedInt}.${trimmedFrac}` : `${sign}${groupedInt}`
  }

  return withGrouping(toPlainString(value))
}

function wrapHardwareText(input: string, maxLen = 56): string[] {
  const text = (input || '').trim()
  if (!text) return ['--']
  if (text.length <= maxLen) return [text]

  const normalized = text.replace(/\s+/g, ' ').trim()
  if (!normalized.includes(' ')) {
    const chunks: string[] = []
    for (let i = 0; i < normalized.length; i += maxLen) {
      chunks.push(normalized.slice(i, i + maxLen))
    }
    return chunks
  }

  const words = normalized.split(' ')
  const lines: string[] = []
  let current = ''
  for (const word of words) {
    if (!current) {
      current = word
      continue
    }
    if ((current.length + 1 + word.length) <= maxLen) {
      current += ` ${word}`
    } else {
      lines.push(current)
      current = word
    }
  }
  if (current) lines.push(current)
  return lines.length ? lines : [normalized]
}

function partitionOptionLabel(partition: PartitionInfo): string {
  const osName = partition.windows_name?.trim() || 'Windows'
  return `${partition.drive_letter}:\\  ${osName}`
}

function macosPluginDescriptionKey(pluginId: string): string {
  switch (pluginId) {
    case 'homebrew':
      return 'tools.macosPluginDesc.homebrew'
    case 'macfuse':
      return 'tools.macosPluginDesc.macfuse'
    case 'gromgit-homebrew-fuse':
      return 'tools.macosPluginDesc.gromgit_homebrew_fuse'
    case 'ntfs-3g-mac':
      return 'tools.macosPluginDesc.ntfs_3g_mac'
    case 'smartmontools':
      return 'tools.macosPluginDesc.smartmontools'
    case 'wimlib':
      return 'tools.macosPluginDesc.wimlib'
    default:
      return ''
  }
}

function ToolsPage() {
  const { t } = useTranslation()
  const tr = (key: string, fallback: string): string => {
    const value = t(key)
    return value === key ? fallback : value
  }

  const { selectedDisk } = useAppStore()
  const yesLabel = tr('tools.yes', 'Yes')
  const noLabel = tr('tools.no', 'No')
  const [activeTool, setActiveTool] = useState<ToolKey>('diskInfo')

  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [diagnostics, setDiagnostics] = useState<DiskDiagnostics[]>([])
  const [selectedDiagId, setSelectedDiagId] = useState<string>('')
  const [hardwareLoading, setHardwareLoading] = useState(false)
  const [hardwareError, setHardwareError] = useState<string | null>(null)
  const [hardware, setHardware] = useState<HardwareOverview | null>(null)

  const [partitions, setPartitions] = useState<PartitionInfo[]>([])
  const [partitionsLoading, setPartitionsLoading] = useState(false)
  const [partitionsError, setPartitionsError] = useState<string | null>(null)
  const [bootTarget, setBootTarget] = useState('')
  const [bootFirmware, setBootFirmware] = useState<BootRepairFirmware>('all')
  const [bootRunning, setBootRunning] = useState(false)
  const [bootMessage, setBootMessage] = useState<string | null>(null)
  const [bootError, setBootError] = useState<string | null>(null)

  const [capacityInput, setCapacityInput] = useState('64')
  const [capacityFrom, setCapacityFrom] = useState<CapacityUnitKey>('GB')
  const [capacityTo, setCapacityTo] = useState<CapacityUnitKey>('GiB')
  const [macosPlugins, setMacosPlugins] = useState<MacosPluginItem[]>([])
  const [macosPluginsLoading, setMacosPluginsLoading] = useState(false)
  const [macosPluginsError, setMacosPluginsError] = useState<string | null>(null)
  const [selectedMacosPluginId, setSelectedMacosPluginId] = useState('')
  const [macosInstallRunning, setMacosInstallRunning] = useState(false)
  const [macosInstallStarting, setMacosInstallStarting] = useState(false)
  const [macosInstallActivePluginId, setMacosInstallActivePluginId] = useState('')
  const [macosInstallMessage, setMacosInstallMessage] = useState<string | null>(null)
  const [macosInstallLogs, setMacosInstallLogs] = useState<string[]>([])
  const terminalRef = useRef<HTMLDivElement | null>(null)
  const installStartLockRef = useRef(false)
  const isMacPlatform = typeof navigator !== 'undefined' && /mac/i.test(navigator.userAgent)

  // Keep tool order stable: newer tools should be appended at the end.
  const cards: Array<{ key: ToolKey; title: string; description: string }> = [
    {
      key: 'diskInfo',
      title: tr('tools.diskInfo', '磁盘信息查看'),
      description: tr('tools.diskInfoDesc', '显示当前所选磁盘的容量与介质类型。'),
    },
    {
      key: 'bootRepair',
      title: tr('tools.bootRepair', '引导修复'),
      description: tr('tools.bootRepairDesc', '用于修复目标磁盘启动项与引导配置。'),
    },
    {
      key: 'capacityCalc',
      title: tr('tools.capacityCalc', '容量换算'),
      description: tr('tools.capacityCalcDesc', '在 GB / GiB 单位之间进行容量换算。'),
    },
    {
      key: 'hardwareInfo',
      title: tr('tools.hardwareInfo', '硬件信息'),
      description: tr('tools.hardwareInfoDesc', '查看处理器、主板、内存、显卡、磁盘与网卡等硬件概览。'),
    },
    {
      key: 'macosPlugins',
      title: tr('tools.macosPlugins', 'macOS 插件'),
      description: tr('tools.macosPluginsDesc', '安装 Homebrew 及相关依赖，并实时查看安装终端输出。'),
    },
  ]

  const selectedDiag = useMemo(
    () => diagnostics.find((d) => d.id === selectedDiagId) ?? diagnostics[0] ?? null,
    [diagnostics, selectedDiagId],
  )

  const reliabilityRows = useMemo(() => {
    if (!selectedDiag?.reliability || typeof selectedDiag.reliability !== 'object') return []
    return Object.entries(selectedDiag.reliability).filter(([, value]) => value !== null && value !== undefined)
  }, [selectedDiag])

  const capacityInputValue = useMemo(() => {
    const value = Number(capacityInput)
    return Number.isFinite(value) ? value : null
  }, [capacityInput])

  const convertedCapacity = useMemo(() => {
    if (capacityInputValue == null) return null
    const from = CAPACITY_UNITS.find((item) => item.key === capacityFrom)
    const to = CAPACITY_UNITS.find((item) => item.key === capacityTo)
    if (!from || !to || from.bytes <= 0 || to.bytes <= 0) return null
    return (capacityInputValue * from.bytes) / to.bytes
  }, [capacityFrom, capacityInputValue, capacityTo])

  const localizeReliabilityKey = (key: string): string => {
    const map: Record<string, string> = {
      Temperature: 'temperature',
      Wear: 'wear',
      PowerOnHours: 'powerOnHours',
      PowerCycleCount: 'powerCycleCount',
      ReadErrorsTotal: 'readErrorsTotal',
      WriteErrorsTotal: 'writeErrorsTotal',
      HostReadsTotal: 'hostReadsTotal',
      HostWritesTotal: 'hostWritesTotal',
      ReadErrorsUncorrected: 'readErrorsUncorrected',
      WriteErrorsUncorrected: 'writeErrorsUncorrected',
      'Smartctl.Device': 'smartctlDevice',
      'Smartctl.DeviceType': 'smartctlDeviceType',
      'Smartctl.Protocol': 'smartctlProtocol',
      'Smartctl.ExitStatus': 'smartctlExitStatus',
      'Smartctl.RotationRate': 'smartctlRotationRate',
      'Smartctl.UserCapacityBytes': 'smartctlUserCapacityBytes',
      'Smartctl.AtaAttributeCount': 'smartctlAtaAttributeCount',
      'Nvme.CriticalWarning': 'nvmeCriticalWarning',
      'Nvme.AvailableSpare': 'nvmeAvailableSpare',
      'Nvme.AvailableSpareThreshold': 'nvmeAvailableSpareThreshold',
      'Nvme.PercentageUsed': 'nvmePercentageUsed',
      'Nvme.DataUnitsRead': 'nvmeDataUnitsRead',
      'Nvme.DataUnitsWritten': 'nvmeDataUnitsWritten',
      'Nvme.HostReads': 'nvmeHostReads',
      'Nvme.HostWrites': 'nvmeHostWrites',
      'Nvme.ControllerBusyTime': 'nvmeControllerBusyTime',
      'Nvme.PowerCycles': 'nvmePowerCycles',
      'Nvme.PowerOnHours': 'nvmePowerOnHours',
      'Nvme.UnsafeShutdowns': 'nvmeUnsafeShutdowns',
      'Nvme.MediaErrors': 'nvmeMediaErrors',
      'Nvme.ErrorLogEntries': 'nvmeErrorLogEntries',
      'NvmeIoctl.CriticalWarning': 'nvmeCriticalWarning',
      'NvmeIoctl.AvailableSpare': 'nvmeAvailableSpare',
      'NvmeIoctl.AvailableSpareThreshold': 'nvmeAvailableSpareThreshold',
      'NvmeIoctl.PercentageUsed': 'nvmePercentageUsed',
      'NvmeIoctl.DataUnitsRead': 'nvmeDataUnitsRead',
      'NvmeIoctl.DataUnitsWritten': 'nvmeDataUnitsWritten',
      'NvmeIoctl.HostReadCommands': 'nvmeHostReadCommands',
      'NvmeIoctl.HostWriteCommands': 'nvmeHostWriteCommands',
      'NvmeIoctl.ControllerBusyTime': 'nvmeControllerBusyTime',
      'NvmeIoctl.PowerCycles': 'nvmePowerCycles',
      'NvmeIoctl.PowerOnHours': 'nvmePowerOnHours',
      'NvmeIoctl.UnsafeShutdowns': 'nvmeUnsafeShutdowns',
      'NvmeIoctl.MediaErrors': 'nvmeMediaErrors',
      'NvmeIoctl.ErrorLogEntries': 'nvmeErrorLogEntries',
      'NvmeIoctl.WarningTempTime': 'nvmeWarningTempTime',
      'NvmeIoctl.CriticalTempTime': 'nvmeCriticalTempTime',
      'NvmeIoctl.TemperatureSensors': 'nvmeTemperatureSensors',
    }
    const mapped = map[key]
    return mapped ? t(`tools.reliabilityKey.${mapped}`) : key
  }

  const localizeSmartAttrName = (id: number, fallback: string): string => {
    const localized = t(`tools.smartAttrName.${id}`)
    return localized && localized !== `tools.smartAttrName.${id}` ? localized : fallback
  }

  const localizeSmartSource = (source: string): string => {
    if (!source) return '--'
    const parts = source.split('+').map((part) => part.trim()).filter(Boolean)
    if (!parts.length) return source
    return parts
      .map((part) => {
        const localized = t(`tools.smartSourceCode.${part}`)
        return localized && localized !== `tools.smartSourceCode.${part}` ? localized : part
      })
      .join(' + ')
  }

  const localizeNote = (note: string): string => {
    const map: Record<string, string> = {
      'Using ATA SMART attributes and Storage Reliability counters.': 'ataAndReliability',
      'Using ATA SMART attribute table.': 'ataOnly',
      'ATA SMART attribute table unavailable; using Storage Reliability counters.': 'reliabilityOnly',
      'SMART/reliability counters unavailable for this device path (common with some USB bridges/RAID drivers).': 'noSmartCounters',
      'Serial number appears masked by controller/driver; another identifier may be required for exact model matching.': 'serialMasked',
      'No usable serial number returned by current Windows APIs for this device.': 'serialMissing',
      'Some counters were derived from ATA SMART attributes because Storage Reliability counters were incomplete.': 'derivedFromAta',
      'USB bridge may block pass-through SMART commands on this enclosure.': 'usbBridgeBlocked',
      'NVMe SMART data read directly via Windows Storage Query API.': 'nvmeIoctl',
      'ATA SMART data read directly via Windows IOCTL (native API).': 'ataIoctl',
      'ATA SMART data read via legacy SMART DFP command path.': 'ataDfp',
      'ATA SMART data read via SAT bridge fallback (SCSI pass-through).': 'ataSat',
      'SMART threshold table was not returned by device/bridge; threshold values are unavailable.': 'thresholdMissing',
      'smartctl not found in PATH; install smartmontools to enable extended SMART details.': 'smartctlMissing',
      'smartctl not found in bundled resources or PATH; include smartmontools to enable extended SMART details.': 'smartctlMissing',
      'Extended SMART details were enhanced via smartctl.': 'smartctlEnhanced',
    }
    const mapped = map[note]
    return mapped ? t(`tools.note.${mapped}`) : note
  }

  const loadDiagnostics = async () => {
    try {
      setLoading(true)
      setError(null)
      const list = await diskApi.listDiskDiagnostics()
      setDiagnostics(list)

      if (!list.length) {
        setSelectedDiagId('')
        return
      }

      if (selectedDisk?.id && list.some((d) => d.id === selectedDisk.id)) {
        setSelectedDiagId(selectedDisk.id)
        return
      }

      if (selectedDisk?.index) {
        const idByIndex = `disk${selectedDisk.index}`
        if (list.some((d) => d.id === idByIndex)) {
          setSelectedDiagId(idByIndex)
          return
        }
      }

      setSelectedDiagId(list[0].id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setDiagnostics([])
      setSelectedDiagId('')
    } finally {
      setLoading(false)
    }
  }

  const loadPartitions = async () => {
    try {
      setPartitionsLoading(true)
      setPartitionsError(null)
      setBootError(null)
      setBootMessage(null)
      const list = await toolsApi.listPartitions()
      const windowsOnly = list.filter((p) => p.has_windows)
      const sorted = [...windowsOnly].sort((a, b) => a.drive_letter.localeCompare(b.drive_letter))
      setPartitions(sorted)
      if (!sorted.length) {
        setBootTarget('')
        return
      }
      setBootTarget((prev) => {
        if (prev && sorted.some((partition) => partition.drive_letter === prev)) {
          return prev
        }
        return sorted[0].drive_letter
      })
    } catch (err) {
      setPartitions([])
      setBootTarget('')
      setPartitionsError(err instanceof Error ? err.message : String(err))
    } finally {
      setPartitionsLoading(false)
    }
  }

  const loadHardwareOverview = async () => {
    try {
      setHardwareLoading(true)
      setHardwareError(null)
      const result = await toolsApi.getHardwareOverview()
      setHardware(result)
    } catch (err) {
      setHardwareError(err instanceof Error ? err.message : String(err))
      setHardware(null)
    } finally {
      setHardwareLoading(false)
    }
  }

  const handleRepairBoot = async () => {
    if (!bootTarget) {
      setBootError(tr('tools.bootRepairTargetRequired', '请选择目标盘符后再执行引导修复。'))
      return
    }

    try {
      setBootRunning(true)
      setBootError(null)
      setBootMessage(null)
      const message = await toolsApi.repairBoot(bootTarget, bootFirmware)
      setBootMessage(message)
    } catch (err) {
      setBootError(err instanceof Error ? err.message : String(err))
    } finally {
      setBootRunning(false)
    }
  }

  const appendInstallLog = (line: string) => {
    setMacosInstallLogs((prev) => {
      const next = [...prev, line]
      if (next.length > 800) {
        return next.slice(next.length - 800)
      }
      return next
    })
  }

  const loadMacosPlugins = async () => {
    if (!isMacPlatform) {
      setMacosPlugins(MACOS_PLUGIN_PLACEHOLDERS)
      setSelectedMacosPluginId((prev) => {
        if (prev && MACOS_PLUGIN_PLACEHOLDERS.some((plugin) => plugin.id === prev)) return prev
        return MACOS_PLUGIN_PLACEHOLDERS[0]?.id || ''
      })
      setMacosInstallRunning(false)
      setMacosInstallStarting(false)
      setMacosInstallActivePluginId('')
      setMacosPluginsError(null)
      setMacosPluginsLoading(false)
      return
    }

    try {
      setMacosPluginsLoading(true)
      setMacosPluginsError(null)
      const [plugins, status] = await Promise.all([
        toolsApi.listMacosPlugins(),
        toolsApi.getMacosPluginInstallStatus(),
      ])
      setMacosPlugins(plugins)
      setMacosInstallRunning(status.running)
      setMacosInstallStarting(false)
      setMacosInstallActivePluginId(status.plugin_id || '')
      setSelectedMacosPluginId((prev) => {
        if (prev && plugins.some((plugin) => plugin.id === prev)) return prev
        return plugins[0]?.id || ''
      })
    } catch (err) {
      setMacosPlugins([])
      setMacosPluginsError(err instanceof Error ? err.message : String(err))
    } finally {
      setMacosPluginsLoading(false)
    }
  }

  const handleStartMacosPluginInstall = async () => {
    if (!selectedMacosPluginId || macosInstallRunning || macosInstallStarting || !isMacPlatform) return
    if (installStartLockRef.current) return
    installStartLockRef.current = true
    try {
      setMacosPluginsError(null)
      setMacosInstallMessage(null)
      setMacosInstallStarting(true)
      setMacosInstallRunning(true)
      setMacosInstallActivePluginId(selectedMacosPluginId)
      const selectedName = macosPlugins.find((plugin) => plugin.id === selectedMacosPluginId)?.name || selectedMacosPluginId
      appendInstallLog(`$ install ${selectedName}`)
      const message = await toolsApi.startMacosPluginInstall(selectedMacosPluginId)
      setMacosInstallMessage(message)
    } catch (err) {
      setMacosInstallRunning(false)
      setMacosInstallStarting(false)
      setMacosInstallActivePluginId('')
      setMacosPluginsError(err instanceof Error ? err.message : String(err))
    } finally {
      installStartLockRef.current = false
    }
  }

  useEffect(() => {
    if (activeTool === 'hardwareInfo') {
      void loadHardwareOverview()
    }
    if (activeTool === 'diskInfo') {
      void loadDiagnostics()
    }
    if (activeTool === 'bootRepair') {
      void loadPartitions()
    }
    if (activeTool === 'macosPlugins') {
      void loadMacosPlugins()
    }
    // Tool switch should lazy-load related data
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTool])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | null = null

    const bindEvents = async () => {
      const un = await listen<MacosPluginInstallEvent>('macos-plugin-install-log', (event) => {
        const payload = event.payload
        const stream = payload.stream || 'system'
        const phase = payload.phase || ''
        const prefix = stream === 'stdout' ? '[out]' : stream === 'stderr' ? '[err]' : '[sys]'
        appendInstallLog(`${prefix} ${payload.line}`)

        if (phase === 'started') {
          setMacosInstallStarting(false)
          setMacosInstallRunning(true)
          setMacosInstallActivePluginId(payload.plugin_id || '')
          setMacosInstallMessage(null)
          return
        }

        if (phase === 'finished') {
          setMacosInstallStarting(false)
          setMacosInstallRunning(false)
          setMacosInstallActivePluginId('')
          if (payload.success) {
            setMacosInstallMessage(payload.line || 'Install completed')
            setMacosPluginsError(null)
          } else {
            setMacosPluginsError(payload.line || 'Install failed')
          }
          void loadMacosPlugins()
        }
      })
      if (disposed) {
        un()
      } else {
        unlisten = un
      }
    }

    void bindEvents()
    return () => {
      disposed = true
      if (unlisten) unlisten()
    }
    // Keep single listener for the whole page lifecycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const el = terminalRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
  }, [macosInstallLogs])

  return (
    <div className="tools-page">
      <header className="page-header">
        <h1>{tr('tools.title', '小工具')}</h1>
        <p className="sub">{tr('tools.subtitle', '实用工具集合')}</p>
      </header>

      <section className="tools-panel">
        <div className="tools-grid">
          {cards.map((card) => (
            <button
              key={card.key}
              className={`tool-card ${activeTool === card.key ? 'active' : ''}`}
              onClick={() => setActiveTool(card.key)}
              type="button"
            >
              <div className="tool-name">{card.title}</div>
              <div className="tool-desc">{card.description}</div>
            </button>
          ))}
        </div>
        <p className="tools-hint">{tr('tools.hint', '该板块为扩展区，后续会持续新增实用工具。')}</p>
      </section>

      {activeTool === 'hardwareInfo' ? (
        <section className="tools-panel">
          <div className="tool-panel-header">
            <div>
              <h2>{tr('tools.hardwareInfoTitle', '硬件信息')}</h2>
              <p>{tr('tools.hardwareInfoSubtitle', '快速查看当前设备的核心硬件清单。')}</p>
            </div>
            <button className="btn-refresh" onClick={() => void loadHardwareOverview()} disabled={hardwareLoading} type="button">
              {hardwareLoading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
            </button>
          </div>

          {hardwareError ? <div className="error-msg">{hardwareError}</div> : null}

          {hardwareLoading && !hardware ? (
            <div className="tool-loading">
              <SpinnerIcon size={20} />
              <span>{t('messages.loading')}</span>
            </div>
          ) : null}

          {hardware ? (
            <div className="hardware-overview">
              <div className="hardware-row">
                <span>{tr('tools.hw.processor', '处理器')}</span>
                <div>
                  {(hardware.processors?.length ? hardware.processors : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`cpu-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.motherboard', '主板')}</span>
                <div>{wrapHardwareText(hardware.motherboard || '--', 54).map((item, idx) => <p key={`mb-${idx}-${item}`}>{item}</p>)}</div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.memory', '内存')}</span>
                <div>{wrapHardwareText(hardware.memory_summary || '--', 54).map((item, idx) => <p key={`mem-${idx}-${item}`}>{item}</p>)}</div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.graphics', '显卡')}</span>
                <div>
                  {(hardware.graphics?.length ? hardware.graphics : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`gpu-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.monitor', '显示器')}</span>
                <div>
                  {(hardware.monitors?.length ? hardware.monitors : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`monitor-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.disk', '磁盘')}</span>
                <div>
                  {(hardware.disks?.length ? hardware.disks : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`disk-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.audio', '声卡')}</span>
                <div>
                  {(hardware.audio_devices?.length ? hardware.audio_devices : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`audio-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
              <div className="hardware-row">
                <span>{tr('tools.hw.network', '网卡')}</span>
                <div>
                  {(hardware.network_adapters?.length ? hardware.network_adapters : ['--']).flatMap((item) => wrapHardwareText(item, 54)).map((item, idx) => <p key={`nic-${idx}-${item}`}>{item}</p>)}
                </div>
              </div>
            </div>
          ) : null}
        </section>
      ) : null}

      {activeTool === 'diskInfo' ? (
        <section className="tools-panel disk-info-panel">
          <div className="tool-panel-header">
            <div>
              <h2>{tr('tools.diskInfoTitle', '磁盘信息与 SMART 诊断')}</h2>
              <p>{tr('tools.diskInfoSubtitle', '查看序列号、固件、接口、健康状态与 SMART 详细属性。')}</p>
            </div>
            <button className="btn-refresh" onClick={() => void loadDiagnostics()} disabled={loading} type="button">
              {loading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
            </button>
          </div>

          {error && <div className="error-msg">{error}</div>}

          {loading && !diagnostics.length ? (
            <div className="tool-loading">
              <SpinnerIcon size={20} />
              <span>{t('messages.loading')}</span>
            </div>
          ) : null}

          {!loading && !error && !diagnostics.length ? (
            <div className="empty-state">{t('errors.deviceNotFound')}</div>
          ) : null}

          {selectedDiag ? (
            <div className="disk-diag-layout">
              <aside className="disk-list-panel">
                {diagnostics.map((d) => (
                  <button
                    key={d.id}
                    type="button"
                    className={`disk-row ${selectedDiag.id === d.id ? 'selected' : ''}`}
                    onClick={() => setSelectedDiagId(d.id)}
                  >
                    <div className="disk-row-main">
                      <strong>{d.model || d.friendly_name || `Disk ${d.disk_number}`}</strong>
                      <span>{d.serial_number || d.id}</span>
                    </div>
                    <div className={`health-badge ${String(d.health_status || '').toLowerCase()}`}>
                      {d.health_status || 'Unknown'}
                    </div>
                  </button>
                ))}
              </aside>

              <div className="disk-detail-panel">
                <div className="disk-hero">
                  <div>
                    <h3>{selectedDiag.model || selectedDiag.friendly_name || `Disk ${selectedDiag.disk_number}`}</h3>
                    <p>
                      {tr('tools.serialNumber', '序列号')}: <code>{selectedDiag.serial_number || '--'}</code>
                    </p>
                  </div>
                  <div className="hero-firmware">
                    <span>{tr('tools.firmware', '固件版本')}</span>
                    <strong>{selectedDiag.firmware_version || '--'}</strong>
                  </div>
                </div>

                <div className="metrics-grid">
                  <div className="metric-card">
                    <span>{tr('tools.temperature', '温度')}</span>
                    <strong>{selectedDiag.temperature_c != null ? `${selectedDiag.temperature_c}°C` : '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{tr('tools.powerOnHours', '通电时长')}</span>
                    <strong>{selectedDiag.power_on_hours != null ? `${selectedDiag.power_on_hours} h` : '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{tr('tools.powerCycleCount', '通电次数')}</span>
                    <strong>{selectedDiag.power_cycle_count ?? '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{tr('tools.mediaHealth', '介质健康')}</span>
                    <strong>{formatMediaHealth(selectedDiag.percentage_used)}</strong>
                  </div>
                </div>

                <div className="detail-grid">
                  <div className="detail-item"><span>{tr('tools.diskId', '磁盘 ID')}</span><strong>{selectedDiag.id}</strong></div>
                  <div className="detail-item"><span>{tr('tools.transportType', '传输类型')}</span><strong>{selectedDiag.transport_type || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.usbDevice', 'USB 设备')}</span><strong>{selectedDiag.is_usb ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{tr('tools.interfaceType', '接口类型')}</span><strong>{selectedDiag.interface_type || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.busType', '总线类型')}</span><strong>{selectedDiag.bus_type || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.mediaType', '介质类型')}</span><strong>{selectedDiag.media_type || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.capacity', '容量')}</span><strong>{formatBytes(selectedDiag.size_bytes)}</strong></div>
                  <div className="detail-item"><span>{tr('tools.smartSource', 'SMART 数据来源')}</span><strong>{localizeSmartSource(selectedDiag.smart_data_source)}</strong></div>
                  <div className="detail-item"><span>{tr('tools.smartSupported', 'SMART 支持')}</span><strong>{selectedDiag.smart_supported ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{tr('tools.smartEnabled', 'SMART 已启用')}</span><strong>{selectedDiag.smart_enabled ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{tr('tools.isSystemDisk', '系统盘')}</span><strong>{selectedDiag.is_system ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{tr('tools.readErrors', '读取错误')}</span><strong>{selectedDiag.read_errors_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.writeErrors', '写入错误')}</span><strong>{selectedDiag.write_errors_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.hostReads', '主机读取总量')}</span><strong>{formatHostIoWithGb(selectedDiag, selectedDiag.host_reads_total, 'read')}</strong></div>
                  <div className="detail-item"><span>{tr('tools.hostWrites', '主机写入总量')}</span><strong>{formatHostIoWithGb(selectedDiag, selectedDiag.host_writes_total, 'write')}</strong></div>
                  <div className="detail-item"><span>{tr('tools.uniqueId', '唯一标识')}</span><strong>{selectedDiag.unique_id || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.pnpId', 'PNP 标识')}</span><strong>{selectedDiag.pnp_device_id || '--'}</strong></div>
                  <div className="detail-item"><span>{tr('tools.usbVidPid', 'USB VID:PID')}</span><strong>{selectedDiag.usb_vendor_id && selectedDiag.usb_product_id ? `${selectedDiag.usb_vendor_id}:${selectedDiag.usb_product_id}` : '--'}</strong></div>
                </div>

                {selectedDiag.smart_attributes?.length ? (
                  <div className="smart-table-wrap">
                    <h4>{tr('tools.smartAttributes', 'SMART 属性明细')}</h4>
                    <table className="smart-table">
                      <thead>
                        <tr>
                          <th>ID</th>
                          <th>{tr('tools.attribute', '属性')}</th>
                          <th>{tr('tools.current', '当前值')}</th>
                          <th>{tr('tools.worst', '最差值')}</th>
                          <th>{tr('tools.threshold', '阈值')}</th>
                          <th>RAW</th>
                        </tr>
                      </thead>
                      <tbody>
                        {selectedDiag.smart_attributes.map((attr) => (
                          <tr key={`${attr.id}-${attr.name}`}>
                            <td>{attr.id}</td>
                            <td>{localizeSmartAttrName(attr.id, attr.name)}</td>
                            <td>{attr.current ?? '--'}</td>
                            <td>{attr.worst ?? '--'}</td>
                            <td>{attr.threshold ?? '--'}</td>
                            <td>{attr.raw_hex || attr.raw || '--'}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : !reliabilityRows.length ? (
                  <div className="empty-state">
                    {tr('tools.smartAttrUnavailable', '当前设备未返回 ATA SMART 属性表（NVMe 设备通常如此）。')}
                  </div>
                ) : null}

                {reliabilityRows.length ? (
                  <div className="smart-table-wrap">
                    <h4>{tr('tools.reliabilityCounters', '可靠性计数器')}</h4>
                    <div className="reliability-grid">
                      {reliabilityRows.map(([key, value]) => (
                        <div className="reliability-item" key={key}>
                          <span>{localizeReliabilityKey(key)}</span>
                          <strong>{renderValue(value)}</strong>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}

                {selectedDiag.notes?.length ? (
                  <div className="notes-box">
                    {selectedDiag.notes.map((note) => (
                      <p key={note}>{localizeNote(note)}</p>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
          ) : null}
        </section>
      ) : null}

      {activeTool === 'bootRepair' ? (
        <section className="tools-panel">
          <div className="tool-panel-header">
            <div>
              <h2>{tr('tools.bootRepairTitle', '引导修复')}</h2>
              <p>{tr('tools.bootRepairSubtitle', '选择目标盘符后执行 bcdboot 与 bcdedit 修复流程。')}</p>
            </div>
            <button className="btn-refresh" onClick={() => void loadPartitions()} disabled={partitionsLoading || bootRunning} type="button">
              {partitionsLoading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
            </button>
          </div>

          {partitionsError ? <div className="error-msg">{partitionsError}</div> : null}
          {bootError ? <div className="error-msg">{bootError}</div> : null}
          {bootMessage ? <div className="success-msg">{bootMessage}</div> : null}

          {partitionsLoading && !partitions.length ? (
            <div className="tool-loading">
              <SpinnerIcon size={20} />
              <span>{t('messages.loading')}</span>
            </div>
          ) : null}

          {!partitionsLoading && !partitions.length ? (
            <div className="empty-state">{tr('tools.bootRepairNoPartition', '未检测到含 Windows 系统的分区。')}</div>
          ) : null}

          {partitions.length ? (
            <div className="tool-form">
              <div className="tool-form-grid">
                <label className="form-field">
                  <span>{tr('tools.bootRepairTarget', '目标盘符')}</span>
                  <select value={bootTarget} onChange={(event) => setBootTarget(event.target.value)} disabled={bootRunning}>
                    {partitions.map((partition) => (
                      <option key={partition.drive_letter} value={partition.drive_letter}>
                        {partitionOptionLabel(partition)}
                      </option>
                    ))}
                  </select>
                </label>

                <label className="form-field">
                  <span>{tr('tools.bootRepairFirmware', '固件模式')}</span>
                  <select
                    value={bootFirmware}
                    onChange={(event) => setBootFirmware(event.target.value as BootRepairFirmware)}
                    disabled={bootRunning}
                  >
                    <option value="all">{tr('tools.bootRepairFirmwareAll', 'ALL（UEFI + BIOS）')}</option>
                    <option value="uefi">{tr('tools.bootRepairFirmwareUefi', 'UEFI')}</option>
                    <option value="bios">{tr('tools.bootRepairFirmwareBios', 'BIOS')}</option>
                  </select>
                </label>
              </div>

              <p className="tool-hint">
                {tr(
                  'tools.bootRepairHint',
                  '目标分区需包含 Windows 目录，且程序需要管理员权限运行。',
                )}
              </p>

              <div className="tool-actions">
                <button className="btn-primary" onClick={() => void handleRepairBoot()} disabled={bootRunning} type="button">
                  {bootRunning ? tr('tools.bootRepairRunning', '修复中...') : tr('tools.bootRepairStart', '开始修复')}
                </button>
              </div>
            </div>
          ) : null}
        </section>
      ) : null}

      {activeTool === 'macosPlugins' ? (
        <section className={`tools-panel plugin-install-panel ${!isMacPlatform ? 'is-disabled' : ''}`}>
          <div className="tool-panel-header">
            <div>
              <h2>{tr('tools.macosPluginsTitle', 'macOS 插件安装')}</h2>
              <p>{tr('tools.macosPluginsSubtitle', '每次安装一个插件，安装输出会实时显示在下方终端窗口。')}</p>
            </div>
            <button className="btn-refresh" onClick={() => void loadMacosPlugins()} disabled={!isMacPlatform || macosPluginsLoading || macosInstallRunning || macosInstallStarting} type="button">
              {macosPluginsLoading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
            </button>
          </div>

          {!isMacPlatform ? (
            <div className="tool-hint disabled-hint">
              {tr('tools.macosPluginsDisabledHint', '当前不是 macOS 环境，此工具仅展示布局，所有安装项均不可操作。')}
            </div>
          ) : null}

          {macosPluginsError ? <div className="error-msg">{macosPluginsError}</div> : null}
          {macosInstallMessage ? <div className="success-msg">{macosInstallMessage}</div> : null}

          {isMacPlatform && macosPluginsLoading && !macosPlugins.length ? (
            <div className="tool-loading">
              <SpinnerIcon size={20} />
              <span>{t('messages.loading')}</span>
            </div>
          ) : null}

          <div className="plugin-list">
            {macosPlugins.length ? macosPlugins.map((plugin) => {
              const descKey = macosPluginDescriptionKey(plugin.id)
              const localizedDesc = descKey ? tr(descKey, plugin.description || '') : (plugin.description || '')
              return (
                <label
                  key={plugin.id}
                  className={`plugin-item ${selectedMacosPluginId === plugin.id ? 'selected' : ''} ${plugin.installed ? 'installed' : ''}`}
                >
                  <div className="plugin-item-main">
                    <input
                      type="radio"
                      name="macos-plugin"
                      checked={selectedMacosPluginId === plugin.id}
                      onChange={() => setSelectedMacosPluginId(plugin.id)}
                      disabled={!isMacPlatform || macosInstallRunning || macosInstallStarting}
                    />
                    <div>
                      <strong>{plugin.name}</strong>
                      <p>{localizedDesc || plugin.description}</p>
                    </div>
                  </div>
                  <span className={`plugin-state ${plugin.installed ? 'installed' : 'pending'}`}>
                    {plugin.installed ? tr('tools.pluginInstalled', '已安装') : tr('tools.pluginNotInstalled', '未安装')}
                  </span>
                </label>
              )
            }) : (
              <div className="empty-state">
                {isMacPlatform
                  ? tr('tools.macosPluginsEmpty', '暂无可用插件列表。')
                  : tr('tools.macosPluginsWinEmpty', '该平台不提供 macOS 插件安装功能。')}
              </div>
            )}
          </div>

          <div className="tool-actions">
            <button
              className="btn-primary"
              onClick={() => void handleStartMacosPluginInstall()}
              disabled={!isMacPlatform || !selectedMacosPluginId || macosInstallRunning || macosInstallStarting || !macosPlugins.length}
              type="button"
            >
              {(macosInstallRunning || macosInstallStarting)
                ? tr('tools.pluginInstalling', '安装中...')
                : tr('tools.pluginInstallStart', '开始安装')}
            </button>
            {(macosInstallRunning || macosInstallStarting) && macosInstallActivePluginId ? (
              <span className="tool-hint">
                {tr('tools.pluginInstallingNow', '当前安装')}: {macosInstallActivePluginId}
              </span>
            ) : null}
          </div>

          <div className="plugin-terminal-wrap">
            <div className="plugin-terminal-header">{tr('tools.pluginTerminal', '终端输出')}</div>
            <div className="plugin-terminal" ref={terminalRef}>
              {macosInstallLogs.length ? (
                macosInstallLogs.map((line, index) => <div key={`${index}-${line}`}>{line}</div>)
              ) : (
                <div className="plugin-terminal-placeholder">
                  {tr('tools.pluginTerminalHint', '等待安装输出...')}
                </div>
              )}
            </div>
          </div>
        </section>
      ) : null}

      {activeTool === 'capacityCalc' ? (
        <section className="tools-panel">
          <div className="tool-panel-header">
            <div>
              <h2>{tr('tools.capacityTitle', '容量换算')}</h2>
              <p>{tr('tools.capacitySubtitle', '支持十进制与二进制单位间换算。')}</p>
            </div>
          </div>

          <div className="tool-form">
            <div className="tool-form-grid">
              <label className="form-field">
                <span>{tr('tools.capacityInput', '输入值')}</span>
                <input
                  value={capacityInput}
                  onChange={(event) => setCapacityInput(event.target.value)}
                  type="number"
                  min="0"
                  step="any"
                />
              </label>

              <label className="form-field">
                <span>{tr('tools.capacityFrom', '从')}</span>
                <select value={capacityFrom} onChange={(event) => setCapacityFrom(event.target.value as CapacityUnitKey)}>
                  {CAPACITY_UNITS.map((unit) => (
                    <option key={unit.key} value={unit.key}>{unit.key}</option>
                  ))}
                </select>
              </label>

              <label className="form-field">
                <span>{tr('tools.capacityTo', '到')}</span>
                <select value={capacityTo} onChange={(event) => setCapacityTo(event.target.value as CapacityUnitKey)}>
                  {CAPACITY_UNITS.map((unit) => (
                    <option key={unit.key} value={unit.key}>{unit.key}</option>
                  ))}
                </select>
              </label>
            </div>

            <div className="capacity-result-card">
              <span>{tr('tools.capacityResult', '换算结果')}</span>
              <strong>
                {convertedCapacity == null
                  ? tr('tools.capacityInvalid', '请输入有效数字')
                  : `${formatConversion(convertedCapacity)} ${capacityTo}`}
              </strong>
            </div>

            <div className="preset-row">
              {CAPACITY_PRESETS.map((preset) => (
                <button
                  key={preset.label}
                  type="button"
                  className={`preset-btn ${capacityInput === String(preset.value) && capacityFrom === preset.unit ? 'active' : ''}`}
                  onClick={() => {
                    setCapacityInput(String(preset.value))
                    setCapacityFrom(preset.unit)
                  }}
                >
                  {preset.label}
                </button>
              ))}
            </div>

            <p className="tool-hint">{tr('tools.capacityHint', 'KB/MB/GB/TB 为十进制（1000），KiB/MiB/GiB/TiB 为二进制（1024）。')}</p>
          </div>
        </section>
      ) : null}
    </div>
  )
}

export default ToolsPage
