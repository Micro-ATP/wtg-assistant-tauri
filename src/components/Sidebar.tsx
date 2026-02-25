import { useState } from 'react'
import type React from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import { HomeIcon, SettingsIcon, WriteIcon, BenchmarkIcon, ToolsIcon, MenuIcon, CogIcon } from './Icons'
import './Sidebar.css'

function Sidebar() {
  const { t } = useTranslation()
  const { currentPage, setCurrentPage } = useAppStore()
  const [isCollapsed, setIsCollapsed] = useState(true)

  const navItems = [
    { id: 'home', label: t('common.home'), icon: HomeIcon },
    { id: 'configure', label: t('common.configure'), icon: SettingsIcon },
    { id: 'write', label: t('common.write'), icon: WriteIcon },
    { id: 'benchmark', label: t('common.benchmark'), icon: BenchmarkIcon },
    { id: 'tools', label: t('common.tools'), icon: ToolsIcon },
  ] as const

  const footerItem = { id: 'settings', label: t('common.settings'), icon: CogIcon } as const

  const renderNavItem = (
    item: { id: string; label: string; icon: React.ComponentType<{ size?: number; color?: string; className?: string }> },
    extraClass = '',
  ) => {
    const Icon = item.icon
    return (
      <a
        href={`#${item.id}`}
        className={`nav-link ${currentPage === item.id ? 'active' : ''} ${extraClass}`.trim()}
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
    )
  }

  return (
    <aside className={`sidebar ${isCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header">
        <button
          className="sidebar-toggle"
          onClick={() => setIsCollapsed((v) => !v)}
          title={isCollapsed ? 'Expand' : 'Collapse'}
          aria-label={isCollapsed ? 'Expand navigation' : 'Collapse navigation'}
        >
          <MenuIcon size={22} />
        </button>

        {!isCollapsed ? (
          <div className="app-mini-brand">
            <div className="app-mini-icon">
              <img src="/icons/WTGA.ico" alt="WTG Logo" />
            </div>
            <span>{t('common.appName')}</span>
          </div>
        ) : null}
      </div>

      <nav className="sidebar-nav">
        <ul>
          {navItems.map((item) => (
            <li key={item.id}>{renderNavItem(item)}</li>
          ))}
        </ul>
      </nav>

      <div className="sidebar-footer-nav">
        {renderNavItem(footerItem, 'footer-nav-link')}
      </div>
    </aside>
  )
}

export default Sidebar
