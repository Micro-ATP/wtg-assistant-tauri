import { useState, useEffect } from 'react'

type Theme = 'light' | 'dark' | 'system'

export const useTheme = () => {
  const [theme, setThemeState] = useState<Theme>(() => {
    // 从 localStorage 恢复主题设置
    const saved = localStorage.getItem('app-theme') as Theme | null
    return saved || 'system'
  })

  const [resolvedTheme, setResolvedTheme] = useState<'light' | 'dark'>('light')

  // 应用主题到 DOM
  useEffect(() => {
    const applyTheme = (themeValue: 'light' | 'dark') => {
      document.documentElement.setAttribute('data-theme', themeValue)
      setResolvedTheme(themeValue)
    }

    if (theme === 'system') {
      // 检测系统偏好
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
      const systemTheme = mediaQuery.matches ? 'dark' : 'light'
      applyTheme(systemTheme)

      // 监听系统主题变化
      const handler = (e: MediaQueryListEvent) => {
        applyTheme(e.matches ? 'dark' : 'light')
      }

      mediaQuery.addEventListener('change', handler)
      return () => mediaQuery.removeEventListener('change', handler)
    } else {
      applyTheme(theme)
    }
  }, [theme])

  const setTheme = (newTheme: Theme) => {
    setThemeState(newTheme)
    localStorage.setItem('app-theme', newTheme)
  }

  const toggleTheme = () => {
    if (theme === 'system') {
      setTheme(resolvedTheme === 'light' ? 'dark' : 'light')
    } else {
      setTheme(theme === 'light' ? 'dark' : 'light')
    }
  }

  return {
    theme,
    resolvedTheme,
    setTheme,
    toggleTheme,
  }
}
