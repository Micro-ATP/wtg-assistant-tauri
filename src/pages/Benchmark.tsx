import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { benchmarkApi, diskApi } from '../services/api'
import { useAppStore } from '../services/store'
import { SpinnerIcon, RefreshIcon } from '../components/Icons'
import type { BenchmarkResult as BenchResult, DiskInfo } from '../types'
import './Benchmark.css'

type PrimaryBenchmarkMode = 'quick' | 'multithread' | 'full'
type ExtraBenchmarkMode = 'fullwrite' | 'scenario'
type BenchmarkMode = PrimaryBenchmarkMode | ExtraBenchmarkMode

const PRIMARY_MODE_ORDER: PrimaryBenchmarkMode[] = ['quick', 'multithread', 'full']
const EXTRA_MODE_ORDER: ExtraBenchmarkMode[] = ['fullwrite', 'scenario']
const MODE_BASE_ESTIMATE_SECONDS: Record<Exclude<BenchmarkMode, 'fullwrite'>, number> = {
  quick: 15,
  multithread: 305,
  full: 930,
  scenario: 930,
}

const CHART = {
  width: 860,
  height: 340,
  left: 64,
  right: 20,
  top: 18,
  bottom: 52,
}

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

function niceStep(range: number, targetTicks = 6): number {
  if (!Number.isFinite(range) || range <= 0) return 1
  const rough = range / targetTicks
  const pow = Math.pow(10, Math.floor(Math.log10(rough)))
  const lead = rough / pow
  const unit = lead <= 1 ? 1 : lead <= 2 ? 2 : lead <= 5 ? 5 : 10
  return unit * pow
}

function buildTicks(maxValue: number, targetTicks = 6): number[] {
  const safeMax = Number.isFinite(maxValue) && maxValue > 0 ? maxValue : 1
  const step = niceStep(safeMax, targetTicks)
  const upper = Math.max(step, Math.ceil(safeMax / step) * step)
  const ticks: number[] = []
  for (let v = 0; v <= upper + step * 0.5; v += step) {
    ticks.push(Number(v.toFixed(6)))
  }
  return ticks
}

function formatTick(v: number, compact = false): string {
  if (!Number.isFinite(v)) return '0'
  if (compact && Math.abs(v) >= 1000) return `${(v / 1000).toFixed(1)}k`
  if (Math.abs(v) >= 100) return `${Math.round(v)}`
  if (Math.abs(v) >= 10) return v.toFixed(1)
  return v.toFixed(2)
}

function chartBase() {
  return {
    plotW: CHART.width - CHART.left - CHART.right,
    plotH: CHART.height - CHART.top - CHART.bottom,
  }
}

function buildThreadChart(result?: BenchResult) {
  if (!result?.thread_results?.length) return null
  const { plotW, plotH } = chartBase()
  const list = [...result.thread_results].sort((a, b) => a.threads - b.threads)
  const yTicks = buildTicks(Math.max(...list.map((d) => d.mb_s), 1), 6)
  const yMax = yTicks[yTicks.length - 1] || 1
  const points = list.map((d, i) => {
    const x = CHART.left + (i / Math.max(list.length - 1, 1)) * plotW
    const y = CHART.top + plotH - (d.mb_s / yMax) * plotH
    return { ...d, x, y }
  })
  const linePath = points.map((p) => `${p.x},${p.y}`).join(' ')
  const xTicks = points.map((p) => ({ x: p.x, label: String(p.threads) }))
  return { points, linePath, yTicks, yMax, xTicks, plotW, plotH }
}

function buildSeqChart(samples?: { t_ms: number; value: number; x_gb: number }[]) {
  if (!samples?.length) return null
  const sorted = [...samples].sort((a, b) => a.x_gb - b.x_gb)
  const { plotW, plotH } = chartBase()
  const xTicks = buildTicks(Math.max(...sorted.map((s) => s.x_gb), 1), 6)
  const yTicks = buildTicks(Math.max(...sorted.map((s) => s.value), 1), 6)
  const xMax = xTicks[xTicks.length - 1] || 1
  const yMax = yTicks[yTicks.length - 1] || 1
  const points = sorted.map((s) => {
    const x = CHART.left + (s.x_gb / xMax) * plotW
    const y = CHART.top + plotH - (s.value / yMax) * plotH
    return { ...s, x, y }
  })
  const linePath = points.map((p) => `${p.x},${p.y}`).join(' ')
  const areaPath = `${linePath} ${CHART.left + plotW},${CHART.top + plotH} ${CHART.left},${CHART.top + plotH}`
  return { points, linePath, areaPath, xTicks, yTicks, xMax, yMax, plotW, plotH }
}

function buildTrendChart(samples?: { x: number; y: number }[]) {
  if (!samples?.length) return null
  const sorted = [...samples].sort((a, b) => a.x - b.x)
  const { plotW, plotH } = chartBase()
  const xTicks = buildTicks(Math.max(...sorted.map((p) => p.x), 1), 8)
  const yTicks = buildTicks(Math.max(...sorted.map((p) => p.y), 1), 6)
  const xMax = xTicks[xTicks.length - 1] || 1
  const yMax = yTicks[yTicks.length - 1] || 1
  const points = sorted.map((p) => {
    const x = CHART.left + (p.x / xMax) * plotW
    const y = CHART.top + plotH - (p.y / yMax) * plotH
    return { ...p, px: x, py: y }
  })
  const linePath = points.map((p) => `${p.px},${p.py}`).join(' ')
  const areaPath = `${linePath} ${CHART.left + plotW},${CHART.top + plotH} ${CHART.left},${CHART.top + plotH}`
  return { points, linePath, areaPath, xTicks, yTicks, xMax, yMax, plotW, plotH }
}

function isModeCompleted(mode: BenchmarkMode, result?: BenchResult): boolean {
  if (!result) return false
  if (mode === 'scenario') return !!(result.scenario_samples?.length || result.scenario_total_io)
  if (mode === 'multithread') return !!(result.thread_results?.length || result.write_seq || result.write_4k)
  if (mode === 'fullwrite') return !!(result.full_seq_samples?.length || result.write_seq)
  return !!(result.write_seq || result.write_4k)
}

function getBenchmarkTargetPath(disk: DiskInfo): string {
  const raw = (disk.volume || '').trim()
  if (!raw) return ''
  if (/^[a-z]$/i.test(raw)) return `${raw.toUpperCase()}:\\`
  if (/^[a-z]:$/i.test(raw)) return `${raw.toUpperCase()}\\`
  return raw
}

function formatDiskTarget(disk: DiskInfo | null): string {
  if (!disk) return '--'
  const target = getBenchmarkTargetPath(disk)
  return target || '--'
}

function getDiskProtocolLabel(disk: DiskInfo): string {
  const driveType = (disk.drive_type || '').trim().toUpperCase()
  if (driveType.includes('NVME')) return 'NVMe'
  if (driveType.includes('SATA')) return 'SATA'
  if (driveType.includes('USB')) return 'USB'
  if (driveType.includes('SAS')) return 'SAS'
  if (driveType.includes('RAID')) return 'RAID'
  if (driveType.includes('ATA')) return 'ATA'
  if (driveType.includes('SCSI')) return 'SCSI'
  if (driveType.includes('ATAPI')) return 'ATAPI'
  if (driveType.includes('SD')) return 'SD'
  if (driveType.includes('MMC')) return 'MMC'

  const media = (disk.media_type || '').toUpperCase()
  if (media.includes('SSD') || media.includes('NVME') || media === '4') return 'SSD'
  if (media.includes('HDD') || media.includes('ROTATIONAL') || media === '3') return 'HDD'
  return 'Disk'
}

function BenchmarkPage() {
  const { t } = useTranslation()
  const { disks, setDisks, selectedDisk, setSelectedDisk } = useAppStore()

  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [running, setRunning] = useState(false)
  const [primaryMode, setPrimaryMode] = useState<PrimaryBenchmarkMode>('quick')
  const [extraModes, setExtraModes] = useState<Record<ExtraBenchmarkMode, boolean>>({
    fullwrite: false,
    scenario: false,
  })
  const [results, setResults] = useState<Record<string, BenchResult>>({})
  const [benchError, setBenchError] = useState<string | null>(null)
  const [currentMode, setCurrentMode] = useState<BenchmarkMode | null>(null)
  const [currentModeStartedAt, setCurrentModeStartedAt] = useState<number | null>(null)
  const [progressNowMs, setProgressNowMs] = useState<number>(Date.now())
  const [canceling, setCanceling] = useState(false)

  const loadDisks = useCallback(async () => {
    try {
      setLoading(true)
      setError(null)
      const result = await diskApi.listDisks()
      setDisks(result)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [setDisks])

  useEffect(() => {
    void loadDisks()
  }, [loadDisks])

  const visibleDisks = useMemo(() => {
    const mounted = disks.filter((d) => getBenchmarkTargetPath(d).length > 0)
    const removableMounted = mounted.filter((d) => d.removable)
    return removableMounted.length > 0 ? removableMounted : mounted
  }, [disks])

  useEffect(() => {
    if (visibleDisks.length === 0) {
      return
    }
    const current = selectedDisk
      ? visibleDisks.find((d) => d.id === selectedDisk.id)
      : undefined
    const currentTarget = current ? getBenchmarkTargetPath(current) : ''
    if (!current || !currentTarget) {
      setSelectedDisk(visibleDisks[0])
    }
  }, [visibleDisks, selectedDisk, setSelectedDisk])

  useEffect(() => {
    if (!running) return
    const id = window.setInterval(() => setProgressNowMs(Date.now()), 300)
    return () => window.clearInterval(id)
  }, [running])

  const selectedModes = useMemo<BenchmarkMode[]>(() => {
    const queue: BenchmarkMode[] = [primaryMode]
    for (const mode of EXTRA_MODE_ORDER) {
      if (extraModes[mode]) {
        queue.push(mode)
      }
    }
    return queue
  }, [primaryMode, extraModes])

  const estimateModeSeconds = useCallback((mode: BenchmarkMode): number => {
    if (mode !== 'fullwrite') return MODE_BASE_ESTIMATE_SECONDS[mode]
    const freeBytes = Math.max(0, selectedDisk?.free || 0)
    const media = (selectedDisk?.media_type || '').toUpperCase()
    const assumedMbS = media.includes('HDD') || media.includes('ROTATIONAL') ? 120 : 280
    const estimated = freeBytes / 1024 / 1024 / assumedMbS
    return Math.max(60, Math.min(2 * 3600, Math.round(estimated)))
  }, [selectedDisk?.free, selectedDisk?.media_type])

  const progressPercent = useMemo(() => {
    if (!running) return 100
    const totalEstimate = Math.max(1, selectedModes.reduce((sum, mode) => sum + estimateModeSeconds(mode), 0))
    const doneEstimate = selectedModes.reduce((sum, mode) => sum + (results[mode] ? estimateModeSeconds(mode) : 0), 0)
    const currentEstimate = currentMode ? estimateModeSeconds(currentMode) : 0
    const elapsedSec = currentModeStartedAt ? Math.max(0, (progressNowMs - currentModeStartedAt) / 1000) : 0

    // Keep a small headroom in each stage so progress does not appear "finished" too early.
    const runningContribution = Math.min(elapsedSec, currentEstimate * 0.92)
    const raw = ((doneEstimate + runningContribution) / totalEstimate) * 80
    return Math.max(1, Math.min(80, raw))
  }, [running, selectedModes, estimateModeSeconds, results, currentMode, currentModeStartedAt, progressNowMs])

  const runModesSequential = async (targetPath: string) => {
    const queue = [...selectedModes]
    setResults({})
    for (const m of queue) {
      setCurrentMode(m)
      setCurrentModeStartedAt(Date.now())
      const r = await benchmarkApi.run(targetPath, m)
      setResults((prev) => ({ ...prev, [m]: r }))
    }
    setCurrentMode(null)
    setCurrentModeStartedAt(null)
  }

  const handleStart = async () => {
    const fallbackDisk = selectedDisk && getBenchmarkTargetPath(selectedDisk)
      ? selectedDisk
      : visibleDisks[0] || null
    const targetPath = fallbackDisk ? getBenchmarkTargetPath(fallbackDisk) : ''
    if (!fallbackDisk || !targetPath) {
      setBenchError(t('errors.deviceNotFound') || 'No disk selected')
      return
    }
    if (!selectedDisk || selectedDisk.id !== fallbackDisk.id) {
      setSelectedDisk(fallbackDisk)
    }
    try {
      setRunning(true)
      setProgressNowMs(Date.now())
      setBenchError(null)
      setCurrentMode(null)
      setCurrentModeStartedAt(null)
      await runModesSequential(targetPath)
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err)
      if (/cancel/i.test(message)) {
        setBenchError(t('benchmark.cancelledHint') || 'Benchmark cancelled')
      } else {
        setBenchError(message)
      }
    } finally {
      setCurrentMode(null)
      setCurrentModeStartedAt(null)
      setRunning(false)
      setCanceling(false)
    }
  }

  const handleCancel = async () => {
    if (!running || canceling) return
    try {
      setCanceling(true)
      await benchmarkApi.cancel()
    } catch (err: unknown) {
      setBenchError(err instanceof Error ? err.message : String(err))
      setCanceling(false)
    }
  }

  const displayedModes = useMemo(
    () => selectedModes.filter((mode) => results[mode] || (running && currentMode === mode)),
    [selectedModes, results, running, currentMode],
  )

  return (
    <div className="benchmark-page">
      <header className="page-header">
        <h1>{t('benchmark.title')}</h1>
        <p className="sub">{t('benchmark.results') || 'Benchmark Results'}</p>
      </header>

      <section className="config-card">
        <div className="card-header">
          <h2>{t('configure.selectDisk')}</h2>
          <button className="btn-refresh" onClick={() => void loadDisks()} disabled={loading}>
            {loading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
          </button>
        </div>
        {error && <div className="error-msg">{error}</div>}
        <div className="disk-list">
          {visibleDisks.length === 0 && !loading ? (
            <div className="empty-state">{t('errors.deviceNotFound')}</div>
          ) : (
            visibleDisks.map((d) => (
              <div
                key={d.id}
                className={`disk-item ${selectedDisk?.id === d.id ? 'selected' : ''}`}
                onClick={() => setSelectedDisk(d)}
              >
                <div className="disk-icon">{getDiskProtocolLabel(d)}</div>
                <div className="disk-info">
                  <div className="disk-name">{d.name || d.id}</div>
                  <div className="disk-details">
                    {formatDiskTarget(d)} - {formatBytes(d.size)}
                    <span className="badge-removable">{getDiskProtocolLabel(d)}</span>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      </section>

      <section className="config-card">
        <div className="mode-selector">
          <div className="mode-group">
            <div className="mode-group-label">{t('benchmark.primaryModes') || '主测试（三选一）'}</div>
            {PRIMARY_MODE_ORDER.map((m) => (
              <label key={m}>
                <input
                  type="radio"
                  name="benchmark-primary-mode"
                  checked={primaryMode === m}
                  disabled={running}
                  onChange={() => setPrimaryMode(m)}
                />
                {t(`benchmark.${m}`) || m}
              </label>
            ))}
          </div>
          <div className="mode-group">
            <div className="mode-group-label">{t('benchmark.extraModes') || '附加测试（可叠加）'}</div>
            {EXTRA_MODE_ORDER.map((m) => (
              <label key={m}>
                <input
                  type="checkbox"
                  checked={extraModes[m]}
                  disabled={running}
                  onChange={(e) =>
                    setExtraModes((prev) => ({
                      ...prev,
                      [m]: e.target.checked,
                    }))
                  }
                />
                {t(`benchmark.${m}`) || m}
              </label>
            ))}
          </div>
        </div>
        <div className="bench-actions">
          <div>
            <div className="label">{t('configure.selectDisk')}</div>
            <div className="value">{formatDiskTarget(selectedDisk)}</div>
          </div>
          <div className="bench-buttons">
            <button className="btn-primary" onClick={handleStart} disabled={running || !selectedDisk || !getBenchmarkTargetPath(selectedDisk)}>
              {running ? t('benchmark.running') || t('messages.loading') : t('benchmark.start')}
            </button>
            {running ? (
              <button className="btn-cancel-bench" onClick={handleCancel} disabled={canceling}>
                {canceling ? t('messages.loading') : t('benchmark.cancel') || 'Cancel'}
              </button>
            ) : null}
          </div>
        </div>
        {running ? (
          <div className="bench-progress-wrap">
            <div className="bench-progress-head">
              <span>{t('benchmark.progress') || 'Progress'}</span>
              <span>{Math.round(progressPercent)}%</span>
            </div>
            <div className="bench-progress-track">
              <div className="bench-progress-fill" style={{ width: `${progressPercent}%` }} />
            </div>
          </div>
        ) : null}
        {benchError && <div className="error-msg">{benchError}</div>}

        {displayedModes.length === 0 && !running ? (
          <div className="empty-state">{t('benchmark.realtimeHint') || 'Select mode(s) and start benchmark.'}</div>
        ) : null}

        {displayedModes.map((mode) => {
          const result = results[mode]
          const done = isModeCompleted(mode, result)
          const isRunningMode = running && currentMode === mode && !done
          const threadChart = buildThreadChart(result)
          const seqChart = buildSeqChart(result?.full_seq_samples)
          const random4kChart = buildTrendChart(result?.write_4k_samples)
          const scenarioChart = buildTrendChart(result?.scenario_samples)
          const threadGradId = `threadLineGrad-${mode}`
          const seqAreaId = `seqAreaGrad-${mode}`
          const r4kAreaId = `r4kAreaGrad-${mode}`
          const sceAreaId = `sceAreaGrad-${mode}`

          return (
            <div className="mode-result-section" key={mode}>
              <div className="mode-result-header">
                <h3>{t(`benchmark.${mode}`) || mode}</h3>
                <span className={`mode-badge ${isRunningMode ? 'running' : done ? 'done' : 'pending'}`}>
                  {isRunningMode ? t('benchmark.running') || 'Running' : done ? t('benchmark.completed') || 'Completed' : '--'}
                </span>
              </div>

              {!result ? (
                <div className="mode-pending">
                  <SpinnerIcon size={18} />
                  <span>{t('messages.loading') || 'Loading...'}</span>
                </div>
              ) : (
                <>
                  <div className="results-grid">
                    {result.write_seq > 0 ? (
                      <div className="result-card">
                        <div className="label">{t('benchmark.sequential') || 'Sequential Write'}</div>
                        <div className="value highlight">{result.write_seq.toFixed(1)} MB/s</div>
                      </div>
                    ) : null}

                    {result.write_4k > 0 ? (
                      <div className="result-card">
                        <div className="label">{t('benchmark.random4K') || '4K Random Write'}</div>
                        <div className="value highlight">{result.write_4k.toFixed(1)} MB/s</div>
                      </div>
                    ) : null}

                    {typeof result.write_4k_raw === 'number' ? (
                      <div className="result-card muted">
                        <div className="label">{t('benchmark.raw4k') || '4K Raw'}</div>
                        <div className="value">{result.write_4k_raw.toFixed(1)} MB/s</div>
                      </div>
                    ) : null}

                    {typeof result.write_4k_adjusted === 'number' ? (
                      <div className="result-card muted">
                        <div className="label">{t('benchmark.adjusted4k') || '4K Adjusted'}</div>
                        <div className="value">{result.write_4k_adjusted.toFixed(1)} MB/s</div>
                      </div>
                    ) : null}

                    {typeof result.score === 'number' ? (
                      <div className="result-card">
                        <div className="label">{t('benchmark.score') || 'Score'}</div>
                        <div className="value">{result.score.toFixed(1)}</div>
                      </div>
                    ) : null}

                    {result.grade ? (
                      <div className="result-card">
                        <div className="label">{t('benchmark.grade') || 'Grade'}</div>
                        <div className="value">{result.grade}</div>
                      </div>
                    ) : null}

                    {typeof result.scenario_score === 'number' ? (
                      <div className="result-card">
                        <div className="label">{t('benchmark.scenarioScore') || 'Scenario Score'}</div>
                        <div className="value">{result.scenario_score.toFixed(1)}</div>
                      </div>
                    ) : null}

                    {typeof result.scenario_total_io === 'number' ? (
                      <div className="result-card muted">
                        <div className="label">{t('benchmark.scenarioTotalIo') || 'Scenario Total IO'}</div>
                        <div className="value">{Math.round(result.scenario_total_io)}</div>
                      </div>
                    ) : null}

                    <div className="result-card muted">
                      <div className="label">{t('benchmark.duration') || 'Duration'}</div>
                      <div className="value">{(result.duration_ms / 1000).toFixed(1)} s</div>
                    </div>

                    <div className="result-card muted">
                      <div className="label">{t('benchmark.written') || 'Written data'}</div>
                      <div className="value">{result.full_written_gb.toFixed(1)} GB</div>
                    </div>
                  </div>

                  {random4kChart ? (
                    <div className="chart-panel">
                      <div className="chart-header">{t('benchmark.random4kTrend') || '4K random write trend'}</div>
                      <svg viewBox={`0 0 ${CHART.width} ${CHART.height}`}>
                        <defs>
                          <linearGradient id={r4kAreaId} x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#2b7fff" stopOpacity="0.35" />
                            <stop offset="100%" stopColor="#2b7fff" stopOpacity="0.06" />
                          </linearGradient>
                        </defs>

                        {random4kChart.yTicks.map((tick) => {
                          const y = CHART.top + random4kChart.plotH - (tick / random4kChart.yMax) * random4kChart.plotH
                          return (
                            <g key={`r4k-y-${mode}-${tick}`}>
                              <line x1={CHART.left} y1={y} x2={CHART.left + random4kChart.plotW} y2={y} className="chart-grid" />
                              <text x={CHART.left - 8} y={y + 4} className="axis-text y-axis">{formatTick(tick, true)}</text>
                            </g>
                          )
                        })}

                        {random4kChart.xTicks.map((tick) => {
                          const x = CHART.left + (tick / random4kChart.xMax) * random4kChart.plotW
                          return (
                            <g key={`r4k-x-${mode}-${tick}`}>
                              <line x1={x} y1={CHART.top} x2={x} y2={CHART.top + random4kChart.plotH} className="chart-grid chart-grid-v" />
                              <text x={x} y={CHART.top + random4kChart.plotH + 18} className="axis-text" textAnchor="middle">
                                {formatTick(tick)}
                              </text>
                            </g>
                          )
                        })}

                        <line x1={CHART.left} y1={CHART.top + random4kChart.plotH} x2={CHART.left + random4kChart.plotW} y2={CHART.top + random4kChart.plotH} className="chart-axis" />
                        <line x1={CHART.left} y1={CHART.top} x2={CHART.left} y2={CHART.top + random4kChart.plotH} className="chart-axis" />

                        <polygon points={random4kChart.areaPath} fill={`url(#${r4kAreaId})`} />
                        <polyline fill="none" stroke="#2b7fff" strokeWidth="2.5" points={random4kChart.linePath} />

                        {random4kChart.points.map((p, idx) => (
                          <circle key={`r4k-p-${mode}-${idx}`} cx={p.px} cy={p.py} r={2.8} className="chart-point">
                            <title>{`t=${p.x.toFixed(2)} s, ${p.y.toFixed(2)} MB/s`}</title>
                          </circle>
                        ))}

                        <text x={CHART.left + random4kChart.plotW / 2} y={CHART.height - 8} className="axis-title" textAnchor="middle">
                          {t('benchmark.seconds') || 'Seconds'}
                        </text>
                        <text
                          x={16}
                          y={CHART.top + random4kChart.plotH / 2}
                          className="axis-title"
                          textAnchor="middle"
                          transform={`rotate(-90 16 ${CHART.top + random4kChart.plotH / 2})`}
                        >
                          MB/s
                        </text>
                      </svg>
                    </div>
                  ) : null}

                  {threadChart ? (
                    <div className="chart-panel">
                      <div className="chart-header">{t('benchmark.threads') || '4K scaling by threads'}</div>
                      <svg viewBox={`0 0 ${CHART.width} ${CHART.height}`}>
                        <defs>
                          <linearGradient id={threadGradId} x1="0" y1="0" x2="1" y2="0">
                            <stop offset="0%" stopColor="#2b7fff" />
                            <stop offset="100%" stopColor="#00b4d8" />
                          </linearGradient>
                        </defs>

                        {threadChart.yTicks.map((tick) => {
                          const y = CHART.top + threadChart.plotH - (tick / threadChart.yMax) * threadChart.plotH
                          return (
                            <g key={`mt-y-${mode}-${tick}`}>
                              <line x1={CHART.left} y1={y} x2={CHART.left + threadChart.plotW} y2={y} className="chart-grid" />
                              <text x={CHART.left - 8} y={y + 4} className="axis-text y-axis">{formatTick(tick, true)}</text>
                            </g>
                          )
                        })}

                        {threadChart.xTicks.map((tick) => (
                          <g key={`mt-x-${mode}-${tick.label}`}>
                            <line x1={tick.x} y1={CHART.top} x2={tick.x} y2={CHART.top + threadChart.plotH} className="chart-grid chart-grid-v" />
                            <text x={tick.x} y={CHART.top + threadChart.plotH + 18} className="axis-text" textAnchor="middle">
                              {tick.label}
                            </text>
                          </g>
                        ))}

                        <line x1={CHART.left} y1={CHART.top + threadChart.plotH} x2={CHART.left + threadChart.plotW} y2={CHART.top + threadChart.plotH} className="chart-axis" />
                        <line x1={CHART.left} y1={CHART.top} x2={CHART.left} y2={CHART.top + threadChart.plotH} className="chart-axis" />

                        <polyline fill="none" stroke={`url(#${threadGradId})`} strokeWidth="2.5" points={threadChart.linePath} />

                        {threadChart.points.map((p) => (
                          <g key={`mt-p-${mode}-${p.threads}`}>
                            <circle cx={p.x} cy={p.y} r={4} className="chart-point">
                              <title>{`${p.threads} threads, ${p.mb_s.toFixed(2)} MB/s`}</title>
                            </circle>
                            <text x={p.x} y={p.y - 8} className="axis-text" textAnchor="middle">{p.mb_s.toFixed(1)}</text>
                          </g>
                        ))}

                        <text x={CHART.left + threadChart.plotW / 2} y={CHART.height - 8} className="axis-title" textAnchor="middle">
                          {t('benchmark.threads') || 'Threads'}
                        </text>
                        <text
                          x={16}
                          y={CHART.top + threadChart.plotH / 2}
                          className="axis-title"
                          textAnchor="middle"
                          transform={`rotate(-90 16 ${CHART.top + threadChart.plotH / 2})`}
                        >
                          MB/s
                        </text>
                      </svg>
                    </div>
                  ) : null}

                  {seqChart ? (
                    <div className="chart-panel">
                      <div className="chart-header">{t('benchmark.seqTrend') || 'Sequential speed over written data'}</div>
                      <svg viewBox={`0 0 ${CHART.width} ${CHART.height}`}>
                        <defs>
                          <linearGradient id={seqAreaId} x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#00b4d8" stopOpacity="0.35" />
                            <stop offset="100%" stopColor="#00b4d8" stopOpacity="0.05" />
                          </linearGradient>
                        </defs>

                        {seqChart.yTicks.map((tick) => {
                          const y = CHART.top + seqChart.plotH - (tick / seqChart.yMax) * seqChart.plotH
                          return (
                            <g key={`seq-y-${mode}-${tick}`}>
                              <line x1={CHART.left} y1={y} x2={CHART.left + seqChart.plotW} y2={y} className="chart-grid" />
                              <text x={CHART.left - 8} y={y + 4} className="axis-text y-axis">{formatTick(tick, true)}</text>
                            </g>
                          )
                        })}

                        {seqChart.xTicks.map((tick) => {
                          const x = CHART.left + (tick / seqChart.xMax) * seqChart.plotW
                          return (
                            <g key={`seq-x-${mode}-${tick}`}>
                              <line x1={x} y1={CHART.top} x2={x} y2={CHART.top + seqChart.plotH} className="chart-grid chart-grid-v" />
                              <text x={x} y={CHART.top + seqChart.plotH + 18} className="axis-text" textAnchor="middle">
                                {formatTick(tick)}
                              </text>
                            </g>
                          )
                        })}

                        <line x1={CHART.left} y1={CHART.top + seqChart.plotH} x2={CHART.left + seqChart.plotW} y2={CHART.top + seqChart.plotH} className="chart-axis" />
                        <line x1={CHART.left} y1={CHART.top} x2={CHART.left} y2={CHART.top + seqChart.plotH} className="chart-axis" />

                        <polygon points={seqChart.areaPath} fill={`url(#${seqAreaId})`} />
                        <polyline fill="none" stroke="#00b4d8" strokeWidth="2.5" points={seqChart.linePath} />

                        {seqChart.points
                          .filter((_, idx) => idx % Math.max(Math.floor(seqChart.points.length / 20), 1) === 0)
                          .map((p, idx) => (
                            <circle key={`seq-p-${mode}-${idx}`} cx={p.x} cy={p.y} r={2.8} className="chart-point chart-point-alt">
                              <title>{`written=${p.x_gb.toFixed(2)} GB, ${p.value.toFixed(2)} MB/s`}</title>
                            </circle>
                          ))}

                        <text x={CHART.left + seqChart.plotW / 2} y={CHART.height - 8} className="axis-title" textAnchor="middle">
                          {t('benchmark.writtenCapacity') || 'Written Capacity (GB)'}
                        </text>
                        <text
                          x={16}
                          y={CHART.top + seqChart.plotH / 2}
                          className="axis-title"
                          textAnchor="middle"
                          transform={`rotate(-90 16 ${CHART.top + seqChart.plotH / 2})`}
                        >
                          MB/s
                        </text>
                      </svg>
                    </div>
                  ) : null}

                  {scenarioChart ? (
                    <div className="chart-panel">
                      <div className="chart-header">{t('benchmark.scenarioTrend') || 'Scenario workload trend'}</div>
                      <svg viewBox={`0 0 ${CHART.width} ${CHART.height}`}>
                        <defs>
                          <linearGradient id={sceAreaId} x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#f59e0b" stopOpacity="0.33" />
                            <stop offset="100%" stopColor="#f59e0b" stopOpacity="0.05" />
                          </linearGradient>
                        </defs>

                        {scenarioChart.yTicks.map((tick) => {
                          const y = CHART.top + scenarioChart.plotH - (tick / scenarioChart.yMax) * scenarioChart.plotH
                          return (
                            <g key={`sce-y-${mode}-${tick}`}>
                              <line x1={CHART.left} y1={y} x2={CHART.left + scenarioChart.plotW} y2={y} className="chart-grid" />
                              <text x={CHART.left - 8} y={y + 4} className="axis-text y-axis">{formatTick(tick, true)}</text>
                            </g>
                          )
                        })}

                        {scenarioChart.xTicks.map((tick) => {
                          const x = CHART.left + (tick / scenarioChart.xMax) * scenarioChart.plotW
                          return (
                            <g key={`sce-x-${mode}-${tick}`}>
                              <line x1={x} y1={CHART.top} x2={x} y2={CHART.top + scenarioChart.plotH} className="chart-grid chart-grid-v" />
                              <text x={x} y={CHART.top + scenarioChart.plotH + 18} className="axis-text" textAnchor="middle">
                                {formatTick(tick)}
                              </text>
                            </g>
                          )
                        })}

                        <line x1={CHART.left} y1={CHART.top + scenarioChart.plotH} x2={CHART.left + scenarioChart.plotW} y2={CHART.top + scenarioChart.plotH} className="chart-axis" />
                        <line x1={CHART.left} y1={CHART.top} x2={CHART.left} y2={CHART.top + scenarioChart.plotH} className="chart-axis" />

                        <polygon points={scenarioChart.areaPath} fill={`url(#${sceAreaId})`} />
                        <polyline fill="none" stroke="#f59e0b" strokeWidth="2.5" points={scenarioChart.linePath} />

                        {scenarioChart.points.map((p, idx) => (
                          <circle key={`sce-p-${mode}-${idx}`} cx={p.px} cy={p.py} r={2.8} className="chart-point chart-point-warm">
                            <title>{`t=${p.x.toFixed(2)} s, ${p.y.toFixed(2)} ops/s`}</title>
                          </circle>
                        ))}

                        <text x={CHART.left + scenarioChart.plotW / 2} y={CHART.height - 8} className="axis-title" textAnchor="middle">
                          {t('benchmark.seconds') || 'Seconds'}
                        </text>
                        <text
                          x={16}
                          y={CHART.top + scenarioChart.plotH / 2}
                          className="axis-title"
                          textAnchor="middle"
                          transform={`rotate(-90 16 ${CHART.top + scenarioChart.plotH / 2})`}
                        >
                          {t('benchmark.opsPerSecond') || 'Ops/s'}
                        </text>
                      </svg>
                    </div>
                  ) : null}
                </>
              )}
            </div>
          )
        })}
      </section>
    </div>
  )
}

export default BenchmarkPage
