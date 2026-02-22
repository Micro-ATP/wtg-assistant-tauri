import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import Sidebar from './components/Sidebar'
import './App.css'

function App() {
  const [systemInfo, setSystemInfo] = useState<any>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const getSystemInfo = async () => {
      try {
        const info = await invoke('get_system_info')
        setSystemInfo(info)
      } catch (error) {
        console.error('Failed to get system info:', error)
      } finally {
        setLoading(false)
      }
    }

    getSystemInfo()
  }, [])

  return (
    <div className="app-container">
      <Sidebar />
      <main className="main-content">
        <header className="app-header">
          <h1>Windows To Go Assistant</h1>
          <p className="subtitle">Cross-platform WTG deployment utility</p>
        </header>

        {loading ? (
          <div className="loading">Loading...</div>
        ) : (
          <div className="system-info">
            <h2>System Information</h2>
            {systemInfo && (
              <div className="info-grid">
                <div><strong>OS:</strong> {systemInfo.os}</div>
                <div><strong>Architecture:</strong> {systemInfo.arch}</div>
                <div><strong>CPU Count:</strong> {systemInfo.cpu_count}</div>
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  )
}

export default App
