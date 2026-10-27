import { createContext, useContext, useLayoutEffect, useMemo, useState, type ReactNode } from 'react'

export type Theme = 'dark' | 'light'
const THEME_STORAGE_KEY = 'arg.theme'
type ThemeContextValue = { theme: Theme; setTheme: (theme: Theme) => void; toggleTheme: () => void }
const ThemeContext = createContext<ThemeContextValue | null>(null)

function readInitialTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY)
    if (stored === 'light' || stored === 'dark') return stored
  } catch { /* Storage can be disabled; use the ARG dark default. */ }
  return 'dark'
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(readInitialTheme)

  useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme
    const themeColor = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')
    themeColor?.setAttribute('content', theme === 'dark' ? '#0c1219' : '#f6f7fb')
    try { window.localStorage.setItem(THEME_STORAGE_KEY, theme) } catch { /* The current tab still follows the chosen theme. */ }
  }, [theme])

  useLayoutEffect(() => {
    const syncFromOtherTab = (event: StorageEvent) => {
      if (event.key !== THEME_STORAGE_KEY || (event.newValue !== 'light' && event.newValue !== 'dark')) return
      setTheme(event.newValue)
    }
    window.addEventListener('storage', syncFromOtherTab)
    return () => window.removeEventListener('storage', syncFromOtherTab)
  }, [])

  const value = useMemo<ThemeContextValue>(() => ({
    theme,
    setTheme,
    toggleTheme: () => setTheme((current) => current === 'dark' ? 'light' : 'dark'),
  }), [theme])

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme() {
  const context = useContext(ThemeContext)
  if (!context) throw new Error('useTheme must be used inside ThemeProvider')
  return context
}
