import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import { imageApi } from '../services/api'
import type { DiskInfo, BootMode, ApplyMode } from '../types'
import './Configure.css'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

function ConfigurePage() {
  const { t } = useTranslation()
  const {
    disks,
    setDisks,
    selectedDisk,
    setSelectedDisk,
    imagePath,
    setImagePath,
    bootMode,
    setBootMode,
    applyMode,
    setApplyMode,
    vhdSizeMb,
    setVhdSizeMb,
    vhdType,
    setVhdType,
    efiPartitionSize,
    setEfiPartitionSize,
    extraFeatures,
    toggleExtraFeature,
    imageInfoList,
    setImageInfoList,
    selectedWimIndex,
    setSelectedWimIndex,
    setCurrentPage,
  } = useAppStore()

  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadDisks = async () => {
    try {
      setLoading(true)
      setError(null)
      const result = await invoke<DiskInfo[]>('list_disks')
      setDisks(result)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadDisks()
  }, [])

  const handleSelectImage = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'Windows Image', extensions: ['iso', 'wim', 'esd', 'vhd', 'vhdx'] },
        ],
      })
      if (selected) {
        const path = selected as string
        setImagePath(path)

        // Auto-detect image type and get WIM info
        const ext = path.split('.').pop()?.toLowerCase() || ''
        if (ext === 'wim' || ext === 'esd') {
          try {
            const info = await imageApi.getImageInfo(path)
            setImageInfoList(info)
          } catch {
            setImageInfoList([])
          }
        } else if (ext === 'vhd' || ext === 'vhdx') {
          setApplyMode(ext as ApplyMode)
          setImageInfoList([])
        } else {
          setImageInfoList([])
        }
      }
    } catch (err) {
      console.error('Failed to open file dialog:', err)
    }
  }

  const isVhdMode = applyMode === 'vhd' || applyMode === 'vhdx'
  const isUefiMode = bootMode === 'uefi_gpt' || bootMode === 'uefi_mbr'

  return (
    <div className="configure-page">
      <header className="page-header">
        <h1>{t('configure.title')}</h1>
      </header>

      {/* Image Selection */}
      <section className="config-card">
        <h2>{t('configure.selectImage')}</h2>
        <div className="image-selector">
          <input
            type="text"
            readOnly
            value={imagePath}
            placeholder={t('configure.selectImage')}
            className="image-path-input"
          />
          <button onClick={handleSelectImage} className="btn-select">
            {t('configure.selectImage')}
          </button>
        </div>

        {/* WIM Index Selection */}
        {imageInfoList.length > 0 && (
          <div className="wim-index-selector">
            <label>{t('configure.wimIndex') || 'WIM Index'}</label>
            <select
              value={selectedWimIndex}
              onChange={(e) => setSelectedWimIndex(e.target.value)}
              className="select-input"
            >
              <option value="0">{t('configure.autoDetect') || 'Auto'}</option>
              {imageInfoList.map((img) => (
                <option key={img.index} value={String(img.index)}>
                  {img.index}: {img.name}
                </option>
              ))}
            </select>
          </div>
        )}
      </section>

      {/* Disk Selection */}
      <section className="config-card">
        <div className="card-header">
          <h2>{t('configure.selectDisk')}</h2>
          <button onClick={loadDisks} className="btn-refresh" disabled={loading}>
            {loading ? t('messages.loading') : '\u21BB'}
          </button>
        </div>

        {error && <div className="error-msg">{error}</div>}

        <div className="disk-list">
          {disks.length === 0 && !loading ? (
            <div className="empty-state">{t('errors.deviceNotFound')}</div>
          ) : (
            disks.map((disk) => (
              <div
                key={disk.id}
                className={`disk-item ${selectedDisk?.id === disk.id ? 'selected' : ''}`}
                onClick={() => setSelectedDisk(disk)}
              >
                <div className="disk-icon">{disk.removable ? 'USB' : 'HDD'}</div>
                <div className="disk-info">
                  <div className="disk-name">{disk.name}</div>
                  <div className="disk-details">
                    {disk.device} - {formatBytes(disk.size)}
                    {disk.removable && <span className="badge-removable">USB</span>}
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      </section>

      {/* Boot Mode */}
      <section className="config-card">
        <h2>{t('configure.bootMode') || 'Boot Mode'}</h2>
        <div className="radio-group">
          <label className="radio-option">
            <input
              type="radio"
              name="bootMode"
              value="uefi_gpt"
              checked={bootMode === 'uefi_gpt'}
              onChange={(e) => setBootMode(e.target.value as BootMode)}
            />
            <span>UEFI + GPT</span>
          </label>
          <label className="radio-option">
            <input
              type="radio"
              name="bootMode"
              value="uefi_mbr"
              checked={bootMode === 'uefi_mbr'}
              onChange={(e) => setBootMode(e.target.value as BootMode)}
            />
            <span>UEFI + MBR</span>
          </label>
          <label className="radio-option">
            <input
              type="radio"
              name="bootMode"
              value="non_uefi"
              checked={bootMode === 'non_uefi'}
              onChange={(e) => setBootMode(e.target.value as BootMode)}
            />
            <span>Legacy (Non-UEFI)</span>
          </label>
        </div>
      </section>

      {/* Apply Mode */}
      <section className="config-card">
        <h2>{t('configure.applyMode') || 'Apply Mode'}</h2>
        <div className="radio-group">
          <label className="radio-option">
            <input
              type="radio"
              name="applyMode"
              value="legacy"
              checked={applyMode === 'legacy'}
              onChange={(e) => setApplyMode(e.target.value as ApplyMode)}
            />
            <span>{t('configure.typicalMode') || 'Typical (Direct)'}</span>
          </label>
          <label className="radio-option">
            <input
              type="radio"
              name="applyMode"
              value="vhd"
              checked={applyMode === 'vhd'}
              onChange={(e) => setApplyMode(e.target.value as ApplyMode)}
            />
            <span>VHD</span>
          </label>
          <label className="radio-option">
            <input
              type="radio"
              name="applyMode"
              value="vhdx"
              checked={applyMode === 'vhdx'}
              onChange={(e) => setApplyMode(e.target.value as ApplyMode)}
            />
            <span>VHDX</span>
          </label>
        </div>
      </section>

      {/* VHD Settings (conditional) */}
      {isVhdMode && (
        <section className="config-card">
          <h2>{t('configure.vhdSettings') || 'VHD Settings'}</h2>
          <div className="settings-grid">
            <div className="setting-item">
              <label>{t('configure.vhdSize') || 'VHD Size (MB)'}</label>
              <input
                type="number"
                value={vhdSizeMb}
                onChange={(e) => setVhdSizeMb(Number(e.target.value))}
                className="number-input"
                min={10240}
                max={524288}
              />
            </div>
            <div className="setting-item">
              <label>{t('configure.vhdType') || 'VHD Type'}</label>
              <select
                value={vhdType}
                onChange={(e) => setVhdType(e.target.value as 'fixed' | 'expandable')}
                className="select-input"
              >
                <option value="expandable">{t('configure.expandable') || 'Expandable'}</option>
                <option value="fixed">{t('configure.fixed') || 'Fixed'}</option>
              </select>
            </div>
          </div>
        </section>
      )}

      {/* EFI Partition Size (conditional) */}
      {isUefiMode && (
        <section className="config-card">
          <h2>{t('configure.efiSettings') || 'EFI Partition'}</h2>
          <div className="setting-item">
            <label>{t('configure.efiSize') || 'EFI Size (MB)'}</label>
            <input
              type="number"
              value={efiPartitionSize}
              onChange={(e) => setEfiPartitionSize(e.target.value)}
              className="number-input"
              min={100}
              max={1024}
            />
          </div>
        </section>
      )}

      {/* Extra Features */}
      <section className="config-card">
        <h2>{t('configure.extraFeatures') || 'Extra Features'}</h2>
        <div className="checkbox-grid">
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.block_local_disk}
              onChange={() => toggleExtraFeature('block_local_disk')}
            />
            <span>{t('configure.blockLocalDisk') || 'Block Local Disk (SAN Policy)'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.disable_winre}
              onChange={() => toggleExtraFeature('disable_winre')}
            />
            <span>{t('configure.disableWinre') || 'Disable WinRE'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.skip_oobe}
              onChange={() => toggleExtraFeature('skip_oobe')}
            />
            <span>{t('configure.skipOobe') || 'Skip OOBE'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.disable_uasp}
              onChange={() => toggleExtraFeature('disable_uasp')}
            />
            <span>{t('configure.disableUasp') || 'Disable UASP'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.compact_os}
              onChange={() => toggleExtraFeature('compact_os')}
            />
            <span>{t('configure.compactOs') || 'CompactOS'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.install_dotnet35}
              onChange={() => toggleExtraFeature('install_dotnet35')}
            />
            <span>{t('configure.dotnet35') || '.NET Framework 3.5'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.no_default_drive_letter}
              onChange={() => toggleExtraFeature('no_default_drive_letter')}
            />
            <span>{t('configure.noDefaultLetter') || 'No Default Drive Letter'}</span>
          </label>
          <label className="checkbox-option">
            <input
              type="checkbox"
              checked={extraFeatures.repartition}
              onChange={() => toggleExtraFeature('repartition')}
            />
            <span>{t('configure.repartition') || 'Repartition Disk'}</span>
          </label>
        </div>
      </section>

      {/* Actions */}
      <div className="actions">
        <button
          className="btn-primary"
          disabled={!selectedDisk || !imagePath}
          onClick={() => setCurrentPage('write')}
        >
          {t('configure.next')}
        </button>
      </div>
    </div>
  )
}

export default ConfigurePage
