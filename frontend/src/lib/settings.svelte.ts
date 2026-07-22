// Reactive user settings, persisted in localStorage. Imported anywhere; reads are reactive (Svelte 5
// module-level $state).
export type Romanization = 'jyutping' | 'yale'

function loadRom(): Romanization {
  try {
    return localStorage.getItem('kogu:rom') === 'yale' ? 'yale' : 'jyutping'
  } catch {
    return 'jyutping'
  }
}

function loadBool(key: string, dflt: boolean): boolean {
  try {
    const v = localStorage.getItem(key)
    return v === null ? dflt : v === '1'
  } catch {
    return dflt
  }
}

export const settings = $state({
  romanization: loadRom() as Romanization,
  // show the Japanese pitch-accent contour (overline + downstep tick) on kana readings
  pitchAccent: loadBool('kogu:pitch', true),
  // play readings aloud (tap the speaker): when off, the speaker buttons are hidden
  audio: loadBool('kogu:audio', true),
  // show Japanese readings as rōmaji (Hepburn) instead of kana. Default off (kana).
  jaRomaji: loadBool('kogu:jaromaji', false),
})

export function setRomanization(v: Romanization): void {
  settings.romanization = v
  try {
    localStorage.setItem('kogu:rom', v)
  } catch {
    /* private mode: in-memory only */
  }
}

export function setPitchAccent(v: boolean): void {
  settings.pitchAccent = v
  try {
    localStorage.setItem('kogu:pitch', v ? '1' : '0')
  } catch {
    /* private mode: in-memory only */
  }
}

export function setAudio(v: boolean): void {
  settings.audio = v
  try {
    localStorage.setItem('kogu:audio', v ? '1' : '0')
  } catch {
    /* private mode: in-memory only */
  }
}

export function setJaRomaji(v: boolean): void {
  settings.jaRomaji = v
  try {
    localStorage.setItem('kogu:jaromaji', v ? '1' : '0')
  } catch {
    /* private mode: in-memory only */
  }
}
