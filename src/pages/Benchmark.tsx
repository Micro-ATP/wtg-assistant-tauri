import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { benchmarkApi } from '../services/api'
import { usePartitionList } from '../hooks/usePartitionList'
import { useAppStore } from '../services/store'
import { SpinnerIcon, RefreshIcon } from '../components/Icons'
import type { BenchmarkResult as BenchResult, DiskInfo } from '../types'
import type { PartitionInfo } from '../hooks/usePartitionList'
import './Benchmark.css'

type BenchmarkMode = 'quick' | 'multithread' | 'fullwrite' | 'full'

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

function BenchmarkPage() {
  const { t } = useTranslation()
  const { partitions, loading, error, refetch } = usePartitionList()
  const { selectedDisk, setSelectedDisk } = useAppStore()

  const [running, setRunning] = useState(false)
  const [modes, setModes] = useState<BenchmarkMode[]>(['quick'])
  const [results, setResults] = useState<Record<string, BenchResult>>({})
  const [benchError, setBenchError] = useState<string | null>(null)

  const visibleParts = useMemo(
    () => partitions.filter((p) => /^[A-Z]$/i.test((p.drive_letter || '').trim())),
    [partitions],
  )

  const getMediaLabel = (media: string) => {
    const up = (media || '').toUpperCase()
    if (up.includes('SSD') || up === '4') return 'SSD'
    if (up.includes('HDD') || up.includes('ROTATIONAL') || up === '3') return 'HDD'
    return 'HDD'
  }

  const runModesSequential = async (targetPath: string) => {
    const seqResults: Record<string, BenchResult> = { ...results }
    for (const m of modes) {
      const r = await benchmarkApi.run(targetPath, m)
      seqResults[m] = r
      setResults({ ...seqResults })
    }
  }

  const mapPartitionToDiskInfo = (partition: PartitionInfo): DiskInfo => ({
    id: partition.drive_letter,
    name: `${partition.label || 'Volume'} (${partition.drive_letter}:)`,
    size: partition.size,
    free: partition.free,
    removable: false,
    device: `Disk ${partition.disk_number}`,
    drive_type: 'Partition',
    index: String(partition.disk_number),
    volume: partition.drive_letter,
    media_type: partition.media_type,
  })

  const handleStart = async () => {
    if (!selectedDisk || !selectedDisk.volume) {
      setBenchError(t('errors.deviceNotFound') || 'No disk selected')
      return
    }
    const targetPath = `${selectedDisk.volume.replace(':', '')}:\\`
    try {
      setRunning(true)
      setBenchError(null)
      await runModesSequential(targetPath)
    } catch (err: unknown) {
      setBenchError(err instanceof Error ? err.message : String(err))
    } finally {
      setRunning(false)
    }
  }

  const latestMode = modes[modes.length - 1]
  const latest = results[latestMode]

  return (
    <div className="benchmark-page">
      <header className="page-header">
        <h1>{t('benchmark.title')}</h1>
        <p className="sub">{t('benchmark.results') || 'Benchmark Results'}</p>
      </header>

      <section className="config-card">
        <div className="card-header">
          <h2>{t('configure.selectDisk')}</h2>
          <button className="btn-refresh" onClick={() => void refetch()} disabled={loading}>
            {loading ? <SpinnerIcon size={18} /> : <RefreshIcon size={18} />}
          </button>
        </div>
        {error && <div className="error-msg">{error}</div>}
        <div className="disk-list">
          {visibleParts.length === 0 && !loading ? (
            <div className="empty-state">{t('errors.deviceNotFound')}</div>
          ) : (
            visibleParts.map((p) => (
              <div
                key={p.drive_letter}
                className={`disk-item ${selectedDisk?.volume === p.drive_letter ? 'selected' : ''}`}
                onClick={() => setSelectedDisk(mapPartitionToDiskInfo(p))}
              >
                <div className="disk-icon">{getMediaLabel(p.media_type)}</div>
                <div className="disk-info">
                  <div className="disk-name">{p.label || 'Volume'}</div>
                  <div className="disk-details">
                    {p.drive_letter}:\\ - {formatBytes(p.size)}
                    <span className="badge-removable">{getMediaLabel(p.media_type)}</span>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      </section>

      <section className="config-card">
        <div className="mode-selector">
          {(['quick', 'multithread', 'fullwrite', 'full'] as const).map((m) => (
            <label key={m}>
              <input
                type="checkbox"
                checked={modes.includes(m)}
                onChange={(e) => {
                  if (e.target.checked) {
                    setModes([...modes, m])
                  } else {
                    const next = modes.filter((x) => x !== m)
                    setModes(next.length ? next : ['quick'])
                  }
                }}
              />
              {t(`benchmark.${m}`) || m}
            </label>
          ))}
        </div>
        <div className="bench-actions">
          <div>
            <div className="label">{t('configure.selectDisk')}</div>
            <div className="value">{selectedDisk ? `${selectedDisk.volume}:\\` : '--'}</div>
          </div>
          <button className="btn-primary" onClick={handleStart} disabled={running || !selectedDisk || !selectedDisk.volume}>
            {running ? t('messages.loading') : t('benchmark.start')}
          </button>
        </div>
        {benchError && <div className="error-msg">{benchError}</div>}

        {latest && (
          <div className="results-grid">
            <div className="result-card">
              <div className="label">{t('benchmark.sequential') || 'Sequential Write'}</div>
              <div className="value highlight">{latest.write_seq.toFixed(1)} MB/s</div>
            </div>
            <div className="result-card">
              <div className="label">{t('benchmark.random4K') || '4K Random Write'}</div>
              <div className="value highlight">{latest.write_4k.toFixed(1)} MB/s</div>
            </div>
            <div className="result-card muted">
              <div className="label">{t('benchmark.duration') || 'Duration'}</div>
              <div className="value">{(latest.duration_ms / 1000).toFixed(1)} s</div>
            </div>
            <div className="result-card muted">
              <div className="label">{t('benchmark.written') || 'Written data'}</div>
              <div className="value">{latest.full_written_gb.toFixed(1)} GB</div>
            </div>
          </div>
        )}

        {latest?.thread_results?.length ? (
          <div className="thread-chart">
            <div className="chart-header">{t('benchmark.threads') || '4K scaling by threads'}</div>
            <svg viewBox="0 0 400 220" preserveAspectRatio="none">
              <line x1="40" y1="10" x2="40" y2="200" stroke="#ccc" strokeWidth="1" />
              <line x1="40" y1="200" x2="390" y2="200" stroke="#ccc" strokeWidth="1" />
              <text x="12" y="16" fontSize="10" fill="#666" transform="rotate(-90 12 16)">MB/s</text>
              <text x="200" y="214" fontSize="10" fill="#666">{t('benchmark.threads') || 'Threads'}</text>
              {latest.thread_results.map((tr, idx) => {
                const max = Math.max(...latest.thread_results.map((p) => p.mb_s), 1)
                const x = 40 + (idx / Math.max(latest.thread_results.length - 1, 1)) * 350
                const y = 200 - (tr.mb_s / max) * 180
                return (
                  <g key={tr.threads}>
                    <circle cx={x} cy={y} r={4} fill="#0078d4" />
                    <text x={x} y={y - 8} fontSize="10" textAnchor="middle" fill="#333">{tr.mb_s.toFixed(0)}</text>
                    <text x={x} y={214} fontSize="10" textAnchor="middle" fill="#666">{tr.threads}</text>
                  </g>
                )
              })}
              <polyline
                fill="none"
                stroke="#0078d4"
                strokeWidth="2"
                points={latest.thread_results.map((tr, idx) => {
                  const max = Math.max(...latest.thread_results.map((p) => p.mb_s), 1)
                  const x = 40 + (idx / Math.max(latest.thread_results.length - 1, 1)) * 350
                  const y = 200 - (tr.mb_s / max) * 180
                  return `${x},${y}`
                }).join(' ')}
              />
            </svg>
          </div>
        ) : null}

        {latest?.full_seq_samples?.length ? (
          <div className="seq-chart">
            <div className="chart-header">{t('benchmark.seqTrend') || 'Sequential speed over time'}</div>
            <svg viewBox="0 0 400 220" preserveAspectRatio="none">
              <line x1="40" y1="10" x2="40" y2="200" stroke="#ccc" strokeWidth="1" />
              <line x1="40" y1="200" x2="390" y2="200" stroke="#ccc" strokeWidth="1" />
              <text x="12" y="16" fontSize="10" fill="#666" transform="rotate(-90 12 16)">MB/s</text>
              <text x="200" y="214" fontSize="10" fill="#666">{t('benchmark.written') || 'Written (GB)'}</text>
              {(() => {
                const samples = latest.full_seq_samples
                const max = Math.max(...samples.map((s) => s.value), 1)
                const maxX = Math.max(...samples.map((s) => s.x_gb), 1)
                return (
                  <polyline
                    fill="none"
                    stroke="#00b4d8"
                    strokeWidth="2"
                    points={samples.map((s) => {
                      const x = 40 + (s.x_gb / maxX) * 350
                      const y = 200 - (s.value / max) * 180
                      return `${x},${y}`
                    }).join(' ')}
                  />
                )
              })()}
            </svg>
          </div>
        ) : null}
      </section>
    </div>
  )
}

export default BenchmarkPage


