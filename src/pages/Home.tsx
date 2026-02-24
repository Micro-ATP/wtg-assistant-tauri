import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import { SettingsIcon, BenchmarkIcon } from '../components/Icons'
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
  const { systemInfo, setCurrentPage } = useAppStore()

  const featureCards = [
    {
      id: 'configure',
      page: 'configure',
      title: t('home.deploy'),
      description: t('home.deployDesc'),
      icon: SettingsIcon,
    },
    {
      id: 'benchmark',
      page: 'benchmark',
      title: t('home.benchmark'),
      description: t('home.benchmarkDesc'),
      icon: BenchmarkIcon,
    },
  ]

  const handleCardClick = (page: string) => {
    setCurrentPage(page as any)
  }

  return (
    <div className="home-page">
      {/* Banner Section */}
      <div className="home-banner" style={{ backgroundImage: 'url(/images/banner.png)' }}>
        <div className="banner-overlay"></div>
        <div className="banner-content">
          <div className="banner-text">
            <h1>{t('home.title')}</h1>
            <p>{t('home.subtitle')}</p>
          </div>
        </div>
        <div className="banner-version">{t('common.version')}</div>
      </div>

      {/* System Information Card */}
      <section className="system-info-card">
        <div className="card-header">
          <h2>{t('home.systemInfo')}</h2>
        </div>
        {systemInfo ? (
          <div className="info-table">
            <div className="info-row">
              <span className="info-label">{t('home.version')}</span>
              <span className="info-value">{systemInfo.version}</span>
            </div>
            <div className="info-row">
              <span className="info-label">{t('home.architecture')}</span>
              <span className="info-value">{systemInfo.arch}</span>
            </div>
            <div className="info-row">
              <span className="info-label">{t('home.cpuModel')}</span>
              <span className="info-value">{systemInfo.cpu_model}</span>
            </div>
            <div className="info-row">
              <span className="info-label">{t('home.memoryCapacity')}</span>
              <span className="info-value">{formatBytes(systemInfo.total_memory)}</span>
            </div>
          </div>
        ) : (
          <div className="loading">{t('messages.loading')}</div>
        )}
      </section>

      {/* Feature Cards Grid */}
      <div className="features-grid">
        {featureCards.map((card) => {
          const Icon = card.icon
          return (
            <div
              key={card.id}
              className="feature-card"
              onClick={() => handleCardClick(card.page)}
            >
              <div className="card-icon">
                <Icon size={24} color="currentColor" />
              </div>
              <div className="card-content">
                <h3>{card.title}</h3>
                <p>{card.description}</p>
              </div>
              <div className="card-arrow">›</div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export default HomePage
