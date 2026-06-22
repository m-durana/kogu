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
// Stable identity across DB rebuilds: lexeme ids are REASSIGNED on a rebuild, so matching saved/history
// entries by id can show the wrong word as bookmarked and silently unsave an unrelated one. Match by
// content instead — written form + variety + reading (or just the form, for a raw search query).
type ItemKey = Pick<SavedItem, 'headword' | 'variety' | 'reading' | 'query'>
function sameEntry(a: ItemKey, b: ItemKey): boolean {
  if (!!a.query !== !!b.query) return false
  if (a.query) return a.headword === b.headword
  return a.headword === b.headword && a.variety === b.variety && (a.reading ?? '') === (b.reading ?? '')
}

export function isSaved(item: ItemKey): boolean {
  return read(SAVED).some((s) => sameEntry(s, item))
}

/** Toggle a bookmark; returns the new saved-state (true = now saved). */
export function toggleSaved(item: SavedItem): boolean {
  requestPersist()
  const list = read(SAVED)
  const i = list.findIndex((s) => sameEntry(s, item))
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
    // drop an exact duplicate query anywhere…
    list = list.filter((s) => !(s.query && s.headword === item.headword))
    // …and collapse ONLY the consecutive live-typing chain: if the most-recent entry is a prefix of
    // this one (or vice versa) — 中 → 中宇 → 中宇大廈 — drop just it. Older distinct searches survive
    // (a later 中國 no longer wipes an earlier standalone 中).
    const head = list[0]
    if (head?.query && (head.headword.startsWith(item.headword) || item.headword.startsWith(head.headword))) {
      list = list.slice(1)
    }
  } else {
    // dedup visited entries by content, not id (an id reused after a rebuild must not double-record).
    list = list.filter((s) => s.query || !sameEntry(s, item))
  }
  list.unshift({ ...item, ts: Date.now() })
  write(HISTORY, list.slice(0, HIST_CAP))
}

export function clearHistory() {
  write(HISTORY, [])
}
