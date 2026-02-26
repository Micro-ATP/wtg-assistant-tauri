import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import { SettingsIcon, BenchmarkIcon, ToolsIcon } from '../components/Icons'
import './Home.css'

type HomeTargetPage = 'configure' | 'benchmark' | 'tools'

function formatMemoryCapacity(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '--'

  const gib = bytes / (1024 ** 3)
  const roundedGb = Math.round(gib)

  // Only force integer when the measured value is already very close.
  if (Math.abs(gib - roundedGb) <= 0.08) {
    return `${roundedGb} GB`
  }

  return `${gib.toFixed(1)} GB`
}

function HomePage() {
  const { t } = useTranslation()
  const { systemInfo, setCurrentPage } = useAppStore()

  const featureCards: Array<{
    id: string
    page: HomeTargetPage
    title: string
    description: string
    icon: typeof SettingsIcon
  }> = [
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
    {
      id: 'tools',
      page: 'tools',
      title: t('home.tools') || t('common.tools'),
      description: t('home.toolsDesc') || t('tools.subtitle'),
      icon: ToolsIcon,
    },
  ]

  const handleCardClick = (page: HomeTargetPage) => {
    setCurrentPage(page)
  }

  const infoRows = systemInfo
    ? [
        { key: 'version', label: t('home.version'), value: systemInfo.version },
        { key: 'arch', label: t('home.architecture'), value: systemInfo.arch },
        { key: 'cpu', label: t('home.cpuModel'), value: systemInfo.cpu_model },
        { key: 'memory', label: t('home.memoryCapacity'), value: formatMemoryCapacity(systemInfo.total_memory) },
      ].filter((row) => String(row.label ?? '').trim() && String(row.value ?? '').trim())
    : []

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
            {infoRows.map((row) => (
              <div className="info-row" key={row.key}>
                <span className="info-label">{row.label}</span>
                <span className="info-value">{row.value}</span>
              </div>
            ))}
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
