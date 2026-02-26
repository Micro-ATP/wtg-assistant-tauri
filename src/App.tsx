import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import Sidebar from './components/Sidebar'
import HomePage from './pages/Home'
import ConfigurePage from './pages/Configure'
import WritePage from './pages/Write'
import BenchmarkPage from './pages/Benchmark'
import ToolsPage from './pages/Tools'
import SettingsPage from './pages/Settings'
import { systemApi } from './services/api'
import { useAppStore } from './services/store'
import type { MacosAdminSessionStatus } from './types'
import './App.css'

function App() {
  const { t } = useTranslation()
  const { currentPage, setSystemInfo, systemInfo } = useAppStore()
  const [showAlphaRiskModal, setShowAlphaRiskModal] = useState(true)
  const [dismissMacosAdminGuide, setDismissMacosAdminGuide] = useState(false)
  const [macosAdminLoading, setMacosAdminLoading] = useState(false)
  const [macosAdminAuthorizing, setMacosAdminAuthorizing] = useState(false)
  const [macosAdminError, setMacosAdminError] = useState<string | null>(null)
  const [macosAdminStatus, setMacosAdminStatus] = useState<MacosAdminSessionStatus | null>(null)
  const isMacHost = (systemInfo?.os || '').toLowerCase() === 'macos'

  const loadMacosAdminSessionStatus = async () => {
    try {
      setMacosAdminLoading(true)
      setMacosAdminError(null)
      const status = await systemApi.getMacosAdminSessionStatus()
      setMacosAdminStatus(status)
    } catch (error) {
      setMacosAdminStatus(null)
      setMacosAdminError(error instanceof Error ? error.message : String(error))
    } finally {
      setMacosAdminLoading(false)
    }
  }

  useEffect(() => {
    const loadSystemInfo = async () => {
      try {
        const info = await systemApi.getSystemInfo()
        setSystemInfo(info)
        if (info.os === 'macos') {
          await loadMacosAdminSessionStatus()
        } else {
          setMacosAdminStatus(null)
          setMacosAdminError(null)
        }
      } catch (error) {
        console.error('Failed to get system info:', error)
      }
    }
    void loadSystemInfo()
    // Initial startup probe.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setSystemInfo])

  const handleAuthorizeMacosAdmin = async () => {
    try {
      setMacosAdminAuthorizing(true)
      setMacosAdminError(null)
      const status = await systemApi.authorizeMacosAdminSession()
      setMacosAdminStatus(status)
    } catch (error) {
      setMacosAdminError(error instanceof Error ? error.message : String(error))
      await loadMacosAdminSessionStatus()
    } finally {
      setMacosAdminAuthorizing(false)
    }
  }

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

      {!showAlphaRiskModal && isMacHost && !dismissMacosAdminGuide && !macosAdminStatus?.authorized && (
        <div className="safety-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="macos-admin-title">
          <div className="safety-modal macos-admin-modal">
            <h2 id="macos-admin-title">{t('safety.macosAdminTitle')}</h2>
            <p>{t('safety.macosAdminBody')}</p>
            <p className="safety-modal-note">{t('safety.macosAdminNote')}</p>
            {macosAdminError ? <div className="safety-modal-error">{macosAdminError}</div> : null}
            <div className="safety-modal-actions">
              <button
                className="btn-danger"
                onClick={() => void handleAuthorizeMacosAdmin()}
                disabled={macosAdminAuthorizing || macosAdminLoading}
              >
                {macosAdminAuthorizing
                  ? t('safety.macosAdminAuthorizing')
                  : t('safety.macosAdminAuthorize')}
              </button>
              <button
                className="btn-secondary"
                onClick={() => setDismissMacosAdminGuide(true)}
                disabled={macosAdminAuthorizing}
              >
                {t('safety.macosAdminLater')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default App
