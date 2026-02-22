import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranslation } from 'react-i18next'
import Sidebar from './components/Sidebar'
import HomePage from './pages/Home'
import ConfigurePage from './pages/Configure'
import WritePage from './pages/Write'
import { useAppStore } from './services/store'
import type { SystemInfo } from './types'
import './App.css'

function App() {
  const { t } = useTranslation()
  const { currentPage, setSystemInfo } = useAppStore()

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
      default:
        return <HomePage />
    }
  }

  return (
    <div className="app-container">
      <Sidebar />
      <main className="main-content">
        {renderPage()}
      </main>
    </div>
  )
}

export default App
