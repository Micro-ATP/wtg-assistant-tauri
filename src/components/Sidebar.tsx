import { useTranslation } from 'react-i18next'
import { useAppStore } from '../services/store'
import './Sidebar.css'

function Sidebar() {
  const { t, i18n } = useTranslation()
  const { currentPage, setCurrentPage, language, setLanguage } = useAppStore()

  const navItems = [
    { id: 'home', label: t('common.home') },
    { id: 'configure', label: t('common.configure') },
    { id: 'write', label: t('common.write') },
    { id: 'benchmark', label: t('common.benchmark') },
  ]

  const handleLanguageChange = (lang: 'en' | 'zh-Hans' | 'zh-Hant') => {
    setLanguage(lang)
    i18n.changeLanguage(lang)
  }

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h2>{t('common.appName')}</h2>
      </div>
      <nav className="sidebar-nav">
        <ul>
          {navItems.map((item) => (
            <li key={item.id}>
              <a
                href={`#${item.id}`}
                className={`nav-link ${currentPage === item.id ? 'active' : ''}`}
                onClick={(e) => {
                  e.preventDefault()
                  setCurrentPage(item.id)
                }}
              >
                {item.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>
      <div className="sidebar-lang">
        <select
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
    </aside>
  )
}

export default Sidebar
