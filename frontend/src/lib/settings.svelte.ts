// Reactive user settings, persisted in localStorage. Imported anywhere; reads are reactive (Svelte 5
// module-level $state).
export type Romanization = 'jyutping' | 'yale'

// design mockups to compare (set on <html data-theme>; CSS in app.css overrides the palette/fonts/radii)
export type Theme = 'sharp' | 'ios' | 'paper' | 'editorial' | 'terminal'
export const THEMES: { id: Theme; label: string }[] = [
  { id: 'sharp', label: 'Sharp' },
  { id: 'ios', label: 'iOS' },
  { id: 'paper', label: 'Paper' },
  { id: 'editorial', label: 'Editorial' },
  { id: 'terminal', label: 'Terminal' },
]

function load(): Romanization {
  try {
    return localStorage.getItem('kogu:rom') === 'yale' ? 'yale' : 'jyutping'
  } catch {
    return 'jyutping'
  }
}

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem('kogu:theme')
    return THEMES.some((x) => x.id === t) ? (t as Theme) : 'sharp'
  } catch {
    return 'sharp'
  }
}

export const settings = $state({ romanization: load() as Romanization, theme: loadTheme() })

export function setRomanization(v: Romanization): void {
  settings.romanization = v
  try {
    localStorage.setItem('kogu:rom', v)
  } catch {
    /* private mode: in-memory only */
  }
}

/** Apply a theme to <html data-theme> and persist it. Call applyTheme() once on startup too. */
export function setTheme(v: Theme): void {
  settings.theme = v
  if (typeof document !== 'undefined') document.documentElement.dataset.theme = v
  try {
    localStorage.setItem('kogu:theme', v)
  } catch {
    /* private mode: in-memory only */
  }
}

export function applyTheme(): void {
  if (typeof document !== 'undefined') document.documentElement.dataset.theme = settings.theme
}
