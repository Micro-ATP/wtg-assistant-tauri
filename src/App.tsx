import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranslation } from 'react-i18next'
import Sidebar from './components/Sidebar'
import HomePage from './pages/Home'
import ConfigurePage from './pages/Configure'
import WritePage from './pages/Write'
import BenchmarkPage from './pages/Benchmark'
import ToolsPage from './pages/Tools'
import SettingsPage from './pages/Settings'
import { useAppStore } from './services/store'
import type { SystemInfo } from './types'
import './App.css'

function App() {
  const { t } = useTranslation()
  const { currentPage, setSystemInfo } = useAppStore()
  const [showAlphaRiskModal, setShowAlphaRiskModal] = useState(true)

  useEffect(() => {
    const loadSystemInfo = async () => {
      try {
        const info = await invoke<SystemInfo>('get_system_info')
        setSystemInfo(info)
      } catch (error) {
        console.error('Failed to get system info:', error)
      }
    }
    loadSystemInfo()
  }, [setSystemInfo])

  const renderPage = () => {
    switch (currentPage) {
      case 'home':
        return <HomePage />
      case 'configure':
        return <ConfigurePage />
      case 'write':
        return <WritePage />
      case 'benchmark':
        return <BenchmarkPage />
      case 'tools':
        return <ToolsPage />
      case 'settings':
        return <SettingsPage />
      default:
        return <HomePage />
    }
  }

  const handleExitApp = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().close()
    } catch (error) {
      console.error('Failed to close app window:', error)
      window.close()
    }
  }

  return (
    <div className="app-container">
      <Sidebar />
      <main className="main-content">
        {renderPage()}
      </main>

      {showAlphaRiskModal && (
        <div className="safety-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="alpha-risk-title">
          <div className="safety-modal">
            <h2 id="alpha-risk-title">{t('safety.alphaTitle')}</h2>
            <p>{t('safety.alphaBody')}</p>
            <div className="safety-modal-actions">
              <button
                className="btn-danger"
                onClick={() => setShowAlphaRiskModal(false)}
              >
                {t('safety.ackContinue')}
              </button>
              <button
                className="btn-secondary"
                onClick={handleExitApp}
              >
                {t('safety.exitApp')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default App
