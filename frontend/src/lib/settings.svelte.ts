// Reactive user settings, persisted in localStorage. Imported anywhere; reads are reactive (Svelte 5
// module-level $state). Currently just the Cantonese romanisation system (jyutping vs Yale).
export type Romanization = 'jyutping' | 'yale'

function load(): Romanization {
  try {
    return localStorage.getItem('kogu:rom') === 'yale' ? 'yale' : 'jyutping'
  } catch {
    return 'jyutping'
  }
}

export const settings = $state({ romanization: load() as Romanization })

export function setRomanization(v: Romanization): void {
  settings.romanization = v
  try {
    localStorage.setItem('kogu:rom', v)
  } catch {
    /* private mode: in-memory only */
  }
}
