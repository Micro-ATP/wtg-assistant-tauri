import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import type { Disk, WtgConfig, ImageType, WriteProgress, MacosTargetWritableCheck } from '../types'
import { writeApi } from '../services/api'
import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import './Write.css'

function getImageType(path: string): ImageType {
  const ext = path.split('.').pop()?.toLowerCase() || ''
  switch (ext) {
    case 'wim':
      return 'wim'
    case 'esd':
      return 'esd'
    case 'iso':
      return 'iso'
    case 'vhd':
      return 'vhd'
    case 'vhdx':
      return 'vhdx'
    default:
      return 'wim'
  }
}

function formatTime(seconds: number): string {
  if (seconds <= 0) return '--:--'
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`
}

function getStatusText(status: string, t: (key: string) => string): string {
  switch (status) {
    case 'preparing':
      return t('writeStatus.preparing')
    case 'partitioning':
      return t('writeStatus.partitioning')
    case 'applyingimage':
      return t('writeStatus.applyingImage')
    case 'writingbootfiles':
      return t('writeStatus.writingBootFiles')
    case 'fixingbcd':
      return t('writeStatus.fixingBcd')
    case 'copyingvhd':
      return t('writeStatus.copyingVhd')
    case 'applyingextras':
      return t('writeStatus.applyingExtras')
    case 'verifying':
      return t('writeStatus.verifying')
    case 'completed':
      return t('write.completed')
    case 'failed':
      return t('write.failed')
    case 'cancelled':
      return t('write.cancel')
    default:
      return status
  }
}

function WritePage() {
  const { t } = useTranslation()
  const {
    systemInfo,
    imagePath,
    selectedDisk,
    bootMode,
    applyMode,
    extraFeatures,
    efiPartitionSize,
    vhdSizeMb,
    vhdType,
    vhdExtension,
    selectedWimIndex,
    isWriting,
    writeProgress,
    setWriting,
    setWriteProgress,
    setCurrentPage,
    setError,
  } = useAppStore()
  const [showEraseConfirmModal, setShowEraseConfirmModal] = useState(false)
  const [showNtfsSafetyModal, setShowNtfsSafetyModal] = useState(false)
  const [eraseConfirmCountdown, setEraseConfirmCountdown] = useState(0)
  const [macosWritableCheck, setMacosWritableCheck] = useState<MacosTargetWritableCheck | null>(null)
  const [macosPrecheckRunning, setMacosPrecheckRunning] = useState(false)
  const [ntfsRemountRunning, setNtfsRemountRunning] = useState(false)
  const isMacHost = (systemInfo?.os || '').toLowerCase() === 'macos'

  const buildTargetDisk = (): Disk | null => {
    if (!selectedDisk) return null
    return {
      id: selectedDisk.id,
      name: selectedDisk.name,
      size: selectedDisk.size,
      removable: selectedDisk.removable,
      device: selectedDisk.device,
      drive_type: selectedDisk.drive_type || '',
      index: selectedDisk.index || '0',
      volume: selectedDisk.volume || '',
    }
  }

  // Set up event listener for real-time progress updates
  useEffect(() => {
    let unlisten: (() => void) | null = null

    const setupListener = async () => {
      unlisten = await listen<WriteProgress>('write-progress', (event) => {
        setWriteProgress(event.payload)
      })
    }

    setupListener()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [setWriteProgress])

  useEffect(() => {
    if (!showEraseConfirmModal) {
      return
    }

    setEraseConfirmCountdown(2)
    const timer = setInterval(() => {
      setEraseConfirmCountdown((prev) => {
        if (prev <= 1) {
          clearInterval(timer)
          return 0
        }
        return prev - 1
      })
    }, 1000)

    return () => clearInterval(timer)
  }, [showEraseConfirmModal])

  const startWrite = async () => {
    if (!selectedDisk || !imagePath) return
    if (isMacHost && (applyMode === 'vhd' || applyMode === 'vhdx')) {
      setError(t('configure.macVhdUnsupported') || 'VHD/VHDX apply mode is currently unavailable on macOS.')
      return
    }

    const targetDisk = buildTargetDisk()
    if (!targetDisk) return

    const imageType = getImageType(imagePath)

    const config: WtgConfig = {
      image_path: imagePath,
      image_type: imageType,
      wim_index: selectedWimIndex,
      target_disk: targetDisk,
      boot_mode: bootMode,
      apply_mode: applyMode,
      partition_config: {
        boot_size: 300,
        partition_layout: bootMode === 'non_uefi' ? 'mbr' : 'gpt',
      },
      extra_features: extraFeatures,
      efi_partition_size: efiPartitionSize,
    }

    // Add VHD config if applicable
    if (applyMode === 'vhd' || applyMode === 'vhdx') {
      config.vhd_config = {
        size_mb: vhdSizeMb,
        vhd_type: vhdType,
        extension: vhdExtension,
        filename: 'win8',
        partition_type: 0,
      }
    }

    try {
      setWriting(true)
      setWriteProgress(null)
      const result = await writeApi.startWrite(config)
      setWriteProgress(result)
    } catch (err) {
      setError(String(err))
      setWriteProgress({
        task_id: '',
        status: 'failed',
        progress: 0,
        message: String(err),
      })
    } finally {
      setWriting(false)
    }
  }

  const handleStartWriteClick = async () => {
    if (!selectedDisk || !imagePath) return
    if (!isMacHost) {
      setShowEraseConfirmModal(true)
      return
    }

    const willRebuildPartitionTable = extraFeatures.repartition || !extraFeatures.do_not_format
    if (willRebuildPartitionTable) {
      setMacosWritableCheck(null)
      setShowNtfsSafetyModal(false)
      setShowEraseConfirmModal(true)
      return
    }

    const targetDisk = buildTargetDisk()
    if (!targetDisk) return

    try {
      setMacosPrecheckRunning(true)
      setError(null)
      const check = await writeApi.checkMacosTargetWritable(targetDisk)
      setMacosWritableCheck(check)
      if (check.needs_ntfs_remount) {
        setShowNtfsSafetyModal(true)
        return
      }
    } catch (err) {
      setError(String(err))
      return
    } finally {
      setMacosPrecheckRunning(false)
    }

    setShowEraseConfirmModal(true)
  }

  const handleConfirmNtfsRemount = async () => {
    const targetDisk = buildTargetDisk()
    if (!targetDisk) return

    try {
      setNtfsRemountRunning(true)
      setError(null)
      const check = await writeApi.remountMacosTargetNtfsWritable(targetDisk)
      setMacosWritableCheck(check)
      setShowNtfsSafetyModal(false)
      setShowEraseConfirmModal(true)
    } catch (err) {
      setError(String(err))
    } finally {
      setNtfsRemountRunning(false)
    }
  }

  const handleConfirmStartWrite = async () => {
    if (eraseConfirmCountdown > 0) return
    setShowEraseConfirmModal(false)
    await startWrite()
  }

  const handleCancel = async () => {
    try {
      await writeApi.cancelWrite(writeProgress?.task_id || '')
    } catch (err) {
      console.error('Cancel failed:', err)
    }
  }

  const isCompleted = writeProgress?.status === 'completed'
  const isFailed = writeProgress?.status === 'failed'

  return (
    <div className="write-page">
      <header className="page-header">
        <h1>{t('write.title')}</h1>
      </header>

      {/* Summary card */}
      <section className="config-summary">
        <h2>{t('write.summary') || 'Summary'}</h2>
        <div className="summary-grid">
          <div className="summary-item">
            <span className="summary-label">{t('configure.selectImage')}</span>
            <span className="summary-value">{imagePath || '--'}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('configure.selectDisk')}</span>
            <span className="summary-value">
              {selectedDisk ? `${selectedDisk.name} (${selectedDisk.device})` : '--'}
            </span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('configure.bootMode') || 'Boot Mode'}</span>
            <span className="summary-value">{bootMode.replace('_', ' + ').toUpperCase()}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">{t('configure.applyMode') || 'Apply Mode'}</span>
            <span className="summary-value">{applyMode.toUpperCase()}</span>
          </div>
        </div>
      </section>

      {/* Progress */}
      {(isWriting || writeProgress) && (
        <section className="write-progress-card">
          <h2>{t('write.progress')}</h2>

          <div className="progress-bar-container">
            <div
              className={`progress-bar ${isCompleted ? 'completed' : ''} ${isFailed ? 'failed' : ''}`}
              style={{ width: `${writeProgress?.progress || 0}%` }}
            />
          </div>
          <div className="progress-text">
            {writeProgress ? `${writeProgress.progress.toFixed(1)}%` : '0%'}
          </div>

          {writeProgress && (
            <div className="status-info">
              <div className="status-item">
                <span className="status-label">{t('write.status') || 'Status'}</span>
                <span className={`status-value ${writeProgress.status}`}>
                  {getStatusText(writeProgress.status, t)}
                </span>
              </div>
              {writeProgress.message && (
                <div className="status-item">
                  <span className="status-label">{t('messages.info')}</span>
                  <span className="status-value">{writeProgress.message}</span>
                </div>
              )}
              {writeProgress.elapsed_seconds !== undefined && writeProgress.elapsed_seconds > 0 && (
                <div className="status-item">
                  <span className="status-label">{t('write.elapsed') || 'Elapsed'}</span>
                  <span className="status-value">{formatTime(writeProgress.elapsed_seconds)}</span>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Actions */}
      <div className="actions">
        <button
          className="btn-secondary"
          onClick={() => setCurrentPage('configure')}
          disabled={isWriting}
        >
          {isWriting ? t('configure.cancel') : t('write.back') || 'Back'}
        </button>

        {!isWriting && !isCompleted && (
          <button
            className="btn-primary btn-start"
            onClick={() => void handleStartWriteClick()}
            disabled={!selectedDisk || !imagePath || macosPrecheckRunning || ntfsRemountRunning}
          >
            {macosPrecheckRunning ? t('safety.ntfsPreWriteChecking') : t('write.startWrite')}
          </button>
        )}

        {isWriting && (
          <button className="btn-danger" onClick={handleCancel}>
            {t('write.stopWrite') || 'Stop Write'}
          </button>
        )}

        {isCompleted && (
          <button
            className="btn-primary"
            onClick={() => setCurrentPage('home')}
          >
            {t('write.completed')}
          </button>
        )}
      </div>

      {showEraseConfirmModal && (
        <div className="safety-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="erase-confirm-title">
          <div className="safety-modal">
            <h2 id="erase-confirm-title">{t('safety.eraseTitle')}</h2>
            <p>{t('safety.eraseBody')}</p>
            <div className="safety-modal-actions">
              <button
                className="btn-danger"
                onClick={handleConfirmStartWrite}
                disabled={eraseConfirmCountdown > 0}
              >
                {eraseConfirmCountdown > 0
                  ? `${t('safety.continueWrite')} (${eraseConfirmCountdown}s)`
                  : t('safety.continueWrite')}
              </button>
              <button
                className="btn-secondary"
                onClick={() => setShowEraseConfirmModal(false)}
              >
                {t('safety.cancel')}
              </button>
            </div>
          </div>
        </div>
      )}

      {showNtfsSafetyModal && (
        <div className="safety-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="ntfs-prewrite-title">
          <div className="safety-modal">
            <h2 id="ntfs-prewrite-title">{t('safety.ntfsPreWriteTitle')}</h2>
            <p>{t('safety.ntfsPreWriteBody')}</p>
            <p className="safety-modal-note">{t('safety.ntfsPreWriteImpact')}</p>
            {macosWritableCheck?.mount_point ? (
              <p className="safety-modal-note">
                {t('safety.ntfsPreWriteTarget')}: {macosWritableCheck.mount_point}
              </p>
            ) : null}
            <div className="safety-modal-actions">
              <button
                className="btn-danger"
                onClick={() => void handleConfirmNtfsRemount()}
                disabled={ntfsRemountRunning}
              >
                {ntfsRemountRunning
                  ? t('safety.ntfsPreWriteAuthorizing')
                  : t('safety.ntfsPreWriteContinue')}
              </button>
              <button
                className="btn-secondary"
                onClick={() => setShowNtfsSafetyModal(false)}
                disabled={ntfsRemountRunning}
              >
                {t('safety.cancel')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default WritePage
