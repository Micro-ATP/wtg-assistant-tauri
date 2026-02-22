import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import './Home.css'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

function HomePage() {
  const { t } = useTranslation()
  const { systemInfo } = useAppStore()

  return (
    <div className="home-page">
      <header className="page-header">
        <h1>{t('home.title')}</h1>
        <p className="subtitle">{t('home.subtitle')}</p>
      </header>

      <section className="system-info-card">
        <h2>{t('home.systemInfo')}</h2>
        {systemInfo ? (
          <div className="info-grid">
            <div className="info-item">
              <span className="info-label">{t('home.os')}</span>
              <span className="info-value">{systemInfo.os}</span>
            </div>
            <div className="info-item">
              <span className="info-label">{t('home.architecture')}</span>
              <span className="info-value">{systemInfo.arch}</span>
            </div>
            <div className="info-item">
              <span className="info-label">{t('home.cpuCount')}</span>
              <span className="info-value">{systemInfo.cpu_count}</span>
            </div>
            <div className="info-item">
              <span className="info-label">{t('home.totalMemory')}</span>
              <span className="info-value">{formatBytes(systemInfo.total_memory)}</span>
            </div>
            <div className="info-item">
              <span className="info-label">{t('home.availableMemory')}</span>
              <span className="info-value">{formatBytes(systemInfo.available_memory)}</span>
            </div>
          </div>
        ) : (
          <div className="loading">{t('messages.loading')}</div>
        )}
      </section>
    </div>
  )
}

export default HomePage
