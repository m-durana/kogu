// Client-side saved (bookmarks) + history, persisted in localStorage. No accounts, no server: an
// installed iOS PWA is exempt from Safari's 7-day script-storage eviction, and navigator.storage
// .persist() upgrades durability further. Items hold enough to render a list row and re-open the page
// without a refetch (id is what openEntry takes; negative ids are char-only codepoint entries).
import type { Variety } from './types'

export interface SavedItem {
  id: number
  headword: string
  reading: string | null
  variety: Variety
  gloss: string | null
  ts: number
  /** a raw SEARCH query (no single word/entry matched, e.g. 中宇大廈) — tapping it re-runs the search
   * rather than opening an entry by id. Entry items omit this. */
  query?: boolean
}

const SAVED = 'kogu:saved'
const HISTORY = 'kogu:history'
const HIST_CAP = 200

function read(key: string): SavedItem[] {
  try {
    const v = JSON.parse(localStorage.getItem(key) || '[]')
    return Array.isArray(v) ? v : []
  } catch {
    return []
  }
}
function write(key: string, items: SavedItem[]) {
  try {
    localStorage.setItem(key, JSON.stringify(items))
  } catch {
    // quota / private mode: fail silently, the feature is best-effort
  }
}

let persistAsked = false
function requestPersist() {
  if (persistAsked) return
  persistAsked = true
  navigator.storage?.persist?.().catch(() => {})
}

export function getSaved(): SavedItem[] {
  return read(SAVED)
}
export function getHistory(): SavedItem[] {
  return read(HISTORY)
}
export function isSaved(id: number): boolean {
  return read(SAVED).some((s) => s.id === id)
}

/** Toggle a bookmark; returns the new saved-state (true = now saved). */
export function toggleSaved(item: SavedItem): boolean {
  requestPersist()
  const list = read(SAVED)
  const i = list.findIndex((s) => s.id === item.id)
  if (i >= 0) {
    list.splice(i, 1)
    write(SAVED, list)
    return false
  }
  list.unshift({ ...item, ts: Date.now() })
  write(SAVED, list)
  return true
}

/** Record a visited page OR a raw search at the front of history (de-duped, newest first, capped). */
export function recordHistory(item: SavedItem) {
  requestPersist()
  let list = read(HISTORY)
  if (item.query) {
    // collapse the live-typing chain: drop any existing query that is a prefix of this one or vice
    // versa (so typing 中 → 中宇 → 中宇大廈 leaves only the final term), and exact duplicates.
    list = list.filter(
      (s) =>
        !(
          s.query &&
          (s.headword === item.headword ||
            s.headword.startsWith(item.headword) ||
            item.headword.startsWith(s.headword))
        ),
    )
  } else {
    list = list.filter((s) => s.query || s.id !== item.id)
  }
  list.unshift({ ...item, ts: Date.now() })
  write(HISTORY, list.slice(0, HIST_CAP))
}

export function clearHistory() {
  write(HISTORY, [])
}
