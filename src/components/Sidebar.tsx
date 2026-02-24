import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import { useTheme } from '../hooks/useTheme'
import { HomeIcon, SettingsIcon, WriteIcon, BenchmarkIcon, MenuIcon, CloseIcon, SunIcon, MoonIcon, WindowIcon } from './Icons'
import './Sidebar.css'

function Sidebar() {
  const { t, i18n } = useTranslation()
  const { currentPage, setCurrentPage, language, setLanguage } = useAppStore()
  const { resolvedTheme, toggleTheme } = useTheme()
  const [isCollapsed, setIsCollapsed] = useState(false)

  const navItems = [
    { id: 'home', label: t('common.home'), icon: HomeIcon },
    { id: 'configure', label: t('common.configure'), icon: SettingsIcon },
    { id: 'write', label: t('common.write'), icon: WriteIcon },
    { id: 'benchmark', label: t('common.benchmark'), icon: BenchmarkIcon },
  ]

  const handleLanguageChange = (lang: 'en' | 'zh-Hans' | 'zh-Hant') => {
    setLanguage(lang)
    i18n.changeLanguage(lang)
  }

  return (
    <aside className={`sidebar ${isCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header">
        {!isCollapsed && (
          <div className="app-title-container">
            <div className="app-icon">
              <img src="/icons/WTGA.ico" alt="WTG Logo" />
            </div>
            <div className="app-title-text">
              <h2>Windows To Go</h2>
              <p>助手</p>
            </div>
          </div>
        )}
        <button
          className="sidebar-toggle"
          onClick={() => setIsCollapsed(!isCollapsed)}
          title={isCollapsed ? 'Expand' : 'Collapse'}
        >
          {isCollapsed ? <MenuIcon size={20} /> : <CloseIcon size={20} />}
        </button>
      </div>

      <nav className="sidebar-nav">
        <ul>
          {navItems.map((item) => {
            const Icon = item.icon
            return (
              <li key={item.id}>
                <a
                  href={`#${item.id}`}
                  className={`nav-link ${currentPage === item.id ? 'active' : ''}`}
                  onClick={(e) => {
                    e.preventDefault()
                    setCurrentPage(item.id)
                  }}
                  title={isCollapsed ? item.label : ''}
                >
                  <span className="nav-icon">
                    <Icon size={20} />
                  </span>
                  {!isCollapsed && <span className="nav-label">{item.label}</span>}
                </a>
              </li>
            )
          })}
        </ul>
      </nav>

      {!isCollapsed && (
        <>
          <div className="sidebar-controls">
            <button
              className="theme-toggle"
              onClick={toggleTheme}
              title="Toggle theme"
            >
              {resolvedTheme === 'light' ? <MoonIcon size={20} /> : <SunIcon size={20} />}
            </button>
          </div>

          <div className="sidebar-lang">
            <label htmlFor="lang-select">Language</label>
            <select
              id="lang-select"
              value={language}
              onChange={(e) => handleLanguageChange(e.target.value as any)}
            >
              <option value="zh-Hans">简体中文</option>
              <option value="zh-Hant">繁體中文</option>
              <option value="en">English</option>
            </select>
          </div>

          <div className="sidebar-footer">
            <p>{t('common.version')}</p>
          </div>
        </>
      )}
    </aside>
  )
}

export default Sidebar
