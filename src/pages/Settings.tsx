import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { open } from '@tauri-apps/plugin-shell'
import { useTheme } from '../hooks/useTheme'
import { useAppStore } from '../services/store'
import { systemApi } from '../services/api'
import type { MacosAdminSessionStatus } from '../types'
import { SunIcon, GlobeIcon, CogIcon, CheckIcon, LinkOutIcon, HeartIcon, ChevronDownIcon, SpinnerIcon, RefreshIcon, FolderIcon } from '../components/Icons'
import './Settings.css'

type ThemeValue = 'light' | 'dark' | 'system'
type AppLanguage = 'en' | 'zh-Hans' | 'zh-Hant'

const REPORT_URL = 'https://github.com/Micro-ATP/wtg-assistant-tauri/issues'
const REPO_URL = 'https://github.com/Micro-ATP/wtg-assistant-tauri'
const DONATE_URL = 'https://ifdian.net/a/micro-atp'
const RELEASES_LATEST_API = 'https://api.github.com/repos/Micro-ATP/wtg-assistant-tauri/releases/latest'
const TAGS_LATEST_API = 'https://api.github.com/repos/Micro-ATP/wtg-assistant-tauri/tags?per_page=1'

async function openExternal(url: string) {
  try {
    await open(url)
  } catch {
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

function normalizeVersion(input: string): string {
  const trimmed = input.trim()
  const matched = trimmed.match(/(\d+\.\d+\.\d+(?:\.\d+)?)/)
  if (matched?.[1]) {
    return matched[1]
  }
  return trimmed.replace(/^v/i, '').split('-')[0]
}

function compareVersions(aRaw: string, bRaw: string): number | null {
  const a = normalizeVersion(aRaw)
  const b = normalizeVersion(bRaw)
  const parse = (v: string): number[] =>
    v
      .split('.')
      .map((part) => Number(part))
      .filter((n) => Number.isFinite(n))

  const pa = parse(a)
  const pb = parse(b)
  if (!pa.length || !pb.length) {
    return null
  }

  const len = Math.max(pa.length, pb.length)
  for (let i = 0; i < len; i += 1) {
    const av = pa[i] ?? 0
    const bv = pb[i] ?? 0
    if (av > bv) return 1
    if (av < bv) return -1
  }
  return 0
}

function SettingsPage() {
  const { t, i18n } = useTranslation()
  const { language, setLanguage, systemInfo } = useAppStore()
  const { theme, setTheme } = useTheme()
  const [aboutExpanded, setAboutExpanded] = useState(true)
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false)
  const [latestTag, setLatestTag] = useState<string | null>(null)
  const [updateState, setUpdateState] = useState<'latest' | 'update' | 'beta' | 'error'>('latest')
  const [updateMessage, setUpdateMessage] = useState<string>(t('settingsPage.checkingUpdate') || 'Checking for updates...')
  const [logsDir, setLogsDir] = useState<string>('')
  const [macosAdminStatus, setMacosAdminStatus] = useState<MacosAdminSessionStatus | null>(null)
  const [macosAdminLoading, setMacosAdminLoading] = useState(false)
  const [macosAdminAuthorizing, setMacosAdminAuthorizing] = useState(false)
  const [macosAdminError, setMacosAdminError] = useState<string | null>(null)
  const isMacHost = (systemInfo?.os || '').toLowerCase() === 'macos'

  const currentVersion = useMemo(() => t('common.version') || 'V0.0.5-Alpha', [t])

  const themeOptions: Array<{ value: ThemeValue; label: string }> = [
    { value: 'light', label: t('settingsPage.themeLight') || '明亮' },
    { value: 'dark', label: t('settingsPage.themeDark') || '深色' },
    { value: 'system', label: t('settingsPage.themeSystem') || '跟随系统' },
  ]

  const languageOptions: Array<{ value: AppLanguage; label: string }> = [
    { value: 'zh-Hans', label: '简体中文' },
    { value: 'zh-Hant', label: '繁體中文' },
    { value: 'en', label: 'English' },
  ]

  const handleLanguageChange = (nextLang: AppLanguage) => {
    setLanguage(nextLang)
    void i18n.changeLanguage(nextLang)
  }

  const fetchLatestTag = useCallback(async (): Promise<string | null> => {
    const requestInit: RequestInit = {
      headers: {
        Accept: 'application/vnd.github+json',
      },
      cache: 'no-store',
    }

    try {
      const latestResp = await fetch(RELEASES_LATEST_API, requestInit)
      if (latestResp.ok) {
        const latestData = (await latestResp.json()) as { tag_name?: string }
        if (latestData?.tag_name) {
          return latestData.tag_name
        }
      }
    } catch {
      // ignore and fallback to tags
    }

    try {
      const tagsResp = await fetch(TAGS_LATEST_API, requestInit)
      if (tagsResp.ok) {
        const tags = (await tagsResp.json()) as Array<{ name?: string }>
        const firstTag = tags?.[0]?.name
        if (firstTag) {
          return firstTag
        }
      }
    } catch {
      // ignore and return null
    }

    return null
  }, [])

  const checkForUpdate = useCallback(async () => {
    setIsCheckingUpdate(true)
    setUpdateMessage(t('settingsPage.checkingUpdate') || 'Checking for updates...')
    setUpdateState('latest')

    try {
      const remoteTag = await fetchLatestTag()
      if (!remoteTag) {
        setUpdateState('error')
        setLatestTag(null)
        setUpdateMessage(t('settingsPage.updateCheckFailed') || 'Failed to check updates')
        return
      }

      setLatestTag(remoteTag)
      const cmp = compareVersions(currentVersion, remoteTag)
      if (cmp === null) {
        const same = normalizeVersion(currentVersion).toLowerCase() === normalizeVersion(remoteTag).toLowerCase()
        if (same) {
          setUpdateState('latest')
          setUpdateMessage(t('settingsPage.latestVersion') || 'Already the latest version')
        } else {
          setUpdateState('update')
          setUpdateMessage((t('settingsPage.newVersionFound') || 'New version found: {{version}}').replace('{{version}}', remoteTag))
        }
        return
      }

      if (cmp < 0) {
        setUpdateState('update')
        setUpdateMessage((t('settingsPage.newVersionFound') || 'New version found: {{version}}').replace('{{version}}', remoteTag))
      } else if (cmp > 0) {
        setUpdateState('beta')
        setUpdateMessage(t('settingsPage.innerBuild') || 'Current build is ahead of public release')
      } else {
        setUpdateState('latest')
        setUpdateMessage(t('settingsPage.latestVersion') || 'Already the latest version')
      }
    } catch {
      setUpdateState('error')
      setLatestTag(null)
      setUpdateMessage(t('settingsPage.updateCheckFailed') || 'Failed to check updates')
    } finally {
      setIsCheckingUpdate(false)
    }
  }, [currentVersion, fetchLatestTag, t])

  useEffect(() => {
    void checkForUpdate()
  }, [checkForUpdate])

  useEffect(() => {
    let isMounted = true
    void (async () => {
      try {
        const dir = await systemApi.getLogsDirectory()
        if (isMounted) {
          setLogsDir(dir)
        }
      } catch {
        if (isMounted) {
          setLogsDir('')
        }
      }
    })()

    return () => {
      isMounted = false
    }
  }, [])

  const loadMacosAdminStatus = useCallback(async () => {
    if (!isMacHost) {
      setMacosAdminStatus(null)
      setMacosAdminError(null)
      return
    }
    try {
      setMacosAdminLoading(true)
      setMacosAdminError(null)
      const status = await systemApi.getMacosAdminSessionStatus()
      setMacosAdminStatus(status)
    } catch (error) {
      setMacosAdminError(error instanceof Error ? error.message : String(error))
      setMacosAdminStatus(null)
    } finally {
      setMacosAdminLoading(false)
    }
  }, [isMacHost])

  useEffect(() => {
    void loadMacosAdminStatus()
  }, [loadMacosAdminStatus])

  const handleAuthorizeMacosAdmin = async () => {
    if (!isMacHost || macosAdminAuthorizing) return
    try {
      setMacosAdminAuthorizing(true)
      setMacosAdminError(null)
      const status = await systemApi.authorizeMacosAdminSession()
      setMacosAdminStatus(status)
    } catch (error) {
      setMacosAdminError(error instanceof Error ? error.message : String(error))
      await loadMacosAdminStatus()
    } finally {
      setMacosAdminAuthorizing(false)
    }
  }

  const macosAdminStatusLabel = useMemo(() => {
    if (!isMacHost) return t('settingsPage.macosAdminStatusUnavailable') || '当前非 macOS'
    if (macosAdminLoading) return t('settingsPage.checkingUpdate') || '正在检查...'
    if (macosAdminStatus?.authorized) return t('settingsPage.macosAdminStatusAuthorized') || '已授权'
    return t('settingsPage.macosAdminStatusUnauthorized') || '未授权'
  }, [isMacHost, macosAdminLoading, macosAdminStatus?.authorized, t])

  const handleUpdateAction = async () => {
    if (isCheckingUpdate) return
    if (updateState === 'update' && latestTag) {
      await openExternal(`${REPO_URL}/releases/tag/${latestTag}`)
      return
    }
    await checkForUpdate()
  }

  const handleOpenLogsDirectory = async () => {
    try {
      const openedPath = await systemApi.openLogsDirectory()
      if (openedPath) {
        setLogsDir(openedPath)
      }
    } catch {
      if (logsDir) {
        await openExternal(logsDir)
      }
    }
  }

  return (
    <div className="settings-page">
      <h1 className="settings-title">{t('common.settings')}</h1>

      <section className="settings-cards">
        <div className="settings-card">
          <div className="settings-card-icon">
            <SunIcon size={20} />
          </div>
          <div className="settings-card-main">
            <div className="settings-card-title">{t('settingsPage.themeTitle') || '应用主题'}</div>
            <div className="settings-card-sub">{t('settingsPage.themeDesc') || '切换浅色/深色模式'}</div>
          </div>
          <select
            className="settings-select"
            value={theme}
            onChange={(e) => setTheme(e.target.value as ThemeValue)}
          >
            {themeOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div className="settings-card">
          <div className="settings-card-icon">
            <GlobeIcon size={24} />
          </div>
          <div className="settings-card-main">
            <div className="settings-card-title">{t('settingsPage.languageTitle') || '语言'}</div>
            <div className="settings-card-sub">{t('settingsPage.languageDesc') || '切换软件显示语言'}</div>
          </div>
          <select
            className="settings-select"
            value={language}
            onChange={(e) => handleLanguageChange(e.target.value as AppLanguage)}
          >
            {languageOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div className={`settings-card ${!isMacHost ? 'disabled' : ''}`}>
          <div className="settings-card-icon">
            <CogIcon size={20} />
          </div>
          <div className="settings-card-main">
            <div className="settings-card-title">{t('settingsPage.macosAdminTitle') || 'macOS 管理员权限'}</div>
            <div className="settings-card-sub">
              {macosAdminError || (t('settingsPage.macosAdminDesc') || '用于手动重新触发系统管理员授权弹窗。')}
            </div>
          </div>
          <div className="settings-card-actions">
            <span className={`status-pill ${isMacHost && macosAdminStatus?.authorized ? 'ok' : ''}`}>
              {macosAdminStatusLabel}
            </span>
            <button
              className="settings-btn"
              type="button"
              onClick={() => void handleAuthorizeMacosAdmin()}
              disabled={!isMacHost || macosAdminAuthorizing || macosAdminLoading}
            >
              {macosAdminAuthorizing
                ? (t('settingsPage.macosAdminAuthorizing') || '请求中...')
                : (t('settingsPage.macosAdminAuthorizeBtn') || '立即授权')}
            </button>
          </div>
        </div>
      </section>

      <h2 className="settings-section-title">{t('common.about')}</h2>

      <section className="about-panel">
        <button className="about-head" onClick={() => setAboutExpanded((v) => !v)}>
          <div className="about-head-icon">
            <CogIcon size={22} />
          </div>
          <div className="about-head-main">
            <div className="about-head-title">{t('common.appName')}</div>
            <div className="about-head-sub">© 2026 | Micro-ATP | {t('common.version')}</div>
          </div>
          <ChevronDownIcon className={`about-chevron ${aboutExpanded ? 'open' : ''}`} size={18} />
        </button>

        {aboutExpanded ? (
          <div className="about-rows">
            <button className="about-row link" onClick={() => void handleUpdateAction()}>
              <span>{updateMessage}</span>
              {isCheckingUpdate ? (
                <SpinnerIcon size={16} className="animate-spin" />
              ) : updateState === 'update' ? (
                <LinkOutIcon />
              ) : updateState === 'error' ? (
                <RefreshIcon size={16} />
              ) : (
                <CheckIcon />
              )}
            </button>

            <button className="about-row link" onClick={() => void handleOpenLogsDirectory()}>
              <span className="about-row-main">
                <span>{t('settingsPage.openLogsDir') || '打开日志目录'}</span>
                <span className="about-row-sub">
                  {logsDir || t('settingsPage.logsDirUnknown') || '未获取到日志目录'}
                </span>
              </span>
              <FolderIcon size={18} />
            </button>

            <button className="about-row link" onClick={() => void openExternal(REPORT_URL)}>
              <span>{t('settingsPage.reportFeedback') || '报告错误或提交意见'}</span>
              <LinkOutIcon />
            </button>

            <button className="about-row link" onClick={() => void openExternal(REPO_URL)}>
              <span>{t('settingsPage.viewRepo') || '查看仓库'}</span>
              <LinkOutIcon />
            </button>

            <button className="about-row link" onClick={() => void openExternal(DONATE_URL)}>
              <span>{t('settingsPage.donate') || '我很可爱，请给我钱'}</span>
              <HeartIcon color="#ef4444" />
            </button>
          </div>
        ) : null}
      </section>
    </div>
  )
}

export default SettingsPage
