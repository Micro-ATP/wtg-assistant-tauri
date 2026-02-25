import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RefreshIcon, SpinnerIcon } from '../components/Icons'
import { diskApi } from '../services/api'
import { useAppStore } from '../services/store'
import type { DiskDiagnostics } from '../types'
import './Tools.css'

type ToolKey = 'diskInfo' | 'bootRepair' | 'capacityCalc'

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const k = 1024
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '--'
  if (typeof value === 'number') return String(value)
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  return String(value)
}

function ToolsPage() {
  const { t } = useTranslation()
  const { selectedDisk } = useAppStore()
  const yesLabel = t('tools.yes') || 'Yes'
  const noLabel = t('tools.no') || 'No'
  const [activeTool, setActiveTool] = useState<ToolKey>('diskInfo')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [diagnostics, setDiagnostics] = useState<DiskDiagnostics[]>([])
  const [selectedDiagId, setSelectedDiagId] = useState<string>('')

  const cards: Array<{ key: ToolKey; title: string; description: string }> = [
    {
      key: 'diskInfo',
      title: t('tools.diskInfo') || '磁盘信息查看',
      description: t('tools.diskInfoDesc') || '显示当前所选磁盘的容量与介质类型。',
    },
    {
      key: 'bootRepair',
      title: t('tools.bootRepair') || '引导修复',
      description: t('tools.bootRepairDesc') || '用于修复目标磁盘启动项与引导配置。',
    },
    {
      key: 'capacityCalc',
      title: t('tools.capacityCalc') || '容量换算',
      description: t('tools.capacityCalcDesc') || '在 GB / GiB 单位之间进行容量换算。',
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

  useEffect(() => {
    if (activeTool === 'diskInfo') {
      void loadDiagnostics()
    }
    // activeTool changes should re-check data once when entering disk info
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTool])

  return (
    <div className="tools-page">
      <header className="page-header">
        <h1>{t('tools.title') || '小工具'}</h1>
        <p className="sub">{t('tools.subtitle') || '实用工具集合'}</p>
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
        <p className="tools-hint">{t('tools.hint') || '该板块为扩展区，后续会持续新增实用工具。'}</p>
      </section>

      {activeTool === 'diskInfo' ? (
        <section className="tools-panel disk-info-panel">
          <div className="tool-panel-header">
            <div>
              <h2>{t('tools.diskInfoTitle') || '磁盘信息与 SMART 诊断'}</h2>
              <p>{t('tools.diskInfoSubtitle') || '查看序列号、固件、接口、健康状态与 SMART 详细属性。'}</p>
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
                      {t('tools.serialNumber') || '序列号'}: <code>{selectedDiag.serial_number || '--'}</code>
                    </p>
                  </div>
                  <div className="hero-firmware">
                    <span>{t('tools.firmware') || '固件版本'}</span>
                    <strong>{selectedDiag.firmware_version || '--'}</strong>
                  </div>
                </div>

                <div className="metrics-grid">
                  <div className="metric-card">
                    <span>{t('tools.temperature') || '温度'}</span>
                    <strong>{selectedDiag.temperature_c != null ? `${selectedDiag.temperature_c}°C` : '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{t('tools.powerOnHours') || '通电时长'}</span>
                    <strong>{selectedDiag.power_on_hours != null ? `${selectedDiag.power_on_hours} h` : '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{t('tools.powerCycleCount') || '通电次数'}</span>
                    <strong>{selectedDiag.power_cycle_count ?? '--'}</strong>
                  </div>
                  <div className="metric-card">
                    <span>{t('tools.mediaHealth') || '介质健康'}</span>
                    <strong>{selectedDiag.percentage_used != null ? `${selectedDiag.percentage_used}%` : '--'}</strong>
                  </div>
                </div>

                <div className="detail-grid">
                  <div className="detail-item"><span>ID</span><strong>{selectedDiag.id}</strong></div>
                  <div className="detail-item"><span>{t('tools.transportType') || '传输类型'}</span><strong>{selectedDiag.transport_type || '--'}</strong></div>
                  <div className="detail-item"><span>USB</span><strong>{selectedDiag.is_usb ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{t('tools.interfaceType') || '接口类型'}</span><strong>{selectedDiag.interface_type || '--'}</strong></div>
                  <div className="detail-item"><span>Bus</span><strong>{selectedDiag.bus_type || '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.mediaType') || '介质类型'}</span><strong>{selectedDiag.media_type || '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.capacity') || '容量'}</span><strong>{formatBytes(selectedDiag.size_bytes)}</strong></div>
                  <div className="detail-item"><span>{t('tools.smartSource') || 'SMART 数据来源'}</span><strong>{selectedDiag.smart_data_source || '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.smartSupported') || 'SMART 支持'}</span><strong>{selectedDiag.smart_supported ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{t('tools.smartEnabled') || 'SMART 已启用'}</span><strong>{selectedDiag.smart_enabled ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{t('tools.isSystemDisk') || '系统盘'}</span><strong>{selectedDiag.is_system ? yesLabel : noLabel}</strong></div>
                  <div className="detail-item"><span>{t('tools.readErrors') || '读取错误'}</span><strong>{selectedDiag.read_errors_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.writeErrors') || '写入错误'}</span><strong>{selectedDiag.write_errors_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.hostReads') || '主机读取总量'}</span><strong>{selectedDiag.host_reads_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.hostWrites') || '主机写入总量'}</span><strong>{selectedDiag.host_writes_total ?? '--'}</strong></div>
                  <div className="detail-item"><span>{t('tools.uniqueId') || '唯一标识'}</span><strong>{selectedDiag.unique_id || '--'}</strong></div>
                </div>

                {selectedDiag.smart_attributes?.length ? (
                  <div className="smart-table-wrap">
                    <h4>{t('tools.smartAttributes') || 'SMART 属性明细'}</h4>
                    <table className="smart-table">
                      <thead>
                        <tr>
                          <th>ID</th>
                          <th>{t('tools.attribute') || '属性'}</th>
                          <th>{t('tools.current') || '当前值'}</th>
                          <th>{t('tools.worst') || '最差值'}</th>
                          <th>{t('tools.threshold') || '阈值'}</th>
                          <th>RAW</th>
                        </tr>
                      </thead>
                      <tbody>
                        {selectedDiag.smart_attributes.map((attr) => (
                          <tr key={`${attr.id}-${attr.name}`}>
                            <td>{attr.id}</td>
                            <td>{attr.name}</td>
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
                    {t('tools.smartAttrUnavailable') || '当前设备未返回 ATA SMART 属性表（NVMe 设备通常如此）。'}
                  </div>
                ) : null}

                {reliabilityRows.length ? (
                  <div className="smart-table-wrap">
                    <h4>{t('tools.reliabilityCounters') || '可靠性计数器'}</h4>
                    <div className="reliability-grid">
                      {reliabilityRows.map(([key, value]) => (
                        <div className="reliability-item" key={key}>
                          <span>{key}</span>
                          <strong>{renderValue(value)}</strong>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}

                {selectedDiag.notes?.length ? (
                  <div className="notes-box">
                    {selectedDiag.notes.map((note) => (
                      <p key={note}>{note}</p>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
          ) : null}
        </section>
      ) : (
        <section className="tools-panel tool-placeholder">
          <h2>{t('tools.inProgressTitle') || '功能开发中'}</h2>
          <p>{t('tools.inProgressBody') || '该工具将在后续版本中实现，当前优先完成磁盘信息与 SMART 诊断能力。'}</p>
        </section>
      )}
    </div>
  )
}

export default ToolsPage
