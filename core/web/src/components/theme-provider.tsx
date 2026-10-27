import * as React from "react"

export type Theme = "dark" | "light" | "system"

// core/web/public/theme.js applies the same choice before the first paint; keep the key in step.
const STORAGE_KEY = "thoric-theme"
const query = "(prefers-color-scheme: dark)"

type ThemeState = { theme: Theme; resolved: "dark" | "light"; setTheme: (theme: Theme) => void }

const ThemeContext = React.createContext<ThemeState | undefined>(undefined)

function stored(): Theme {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    if (value === "light" || value === "dark" || value === "system") return value
  } catch {
    // Storage can be blocked; the default still works.
  }
  return "dark"
}

const subscribeSystem = (listener: () => void) => {
  const media = window.matchMedia(query)
  media.addEventListener("change", listener)
  return () => media.removeEventListener("change", listener)
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = React.useState<Theme>(stored)
  const systemDark = React.useSyncExternalStore(
    subscribeSystem,
    () => window.matchMedia(query).matches
  )
  const resolved = theme === "system" ? (systemDark ? "dark" : "light") : theme

  React.useEffect(() => {
    const root = document.documentElement
    root.classList.toggle("dark", resolved === "dark")
    root.style.colorScheme = resolved
  }, [resolved])

  const setTheme = React.useCallback((next: Theme) => {
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // The choice still applies to this page.
    }
    setThemeState(next)
  }, [])

  const value = React.useMemo(() => ({ theme, resolved, setTheme }), [theme, resolved, setTheme])
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme() {
  const context = React.useContext(ThemeContext)
  if (!context) throw new Error("useTheme must be used within a ThemeProvider")
  return context
}
