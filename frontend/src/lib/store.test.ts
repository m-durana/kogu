import { beforeEach, describe, expect, it } from 'vitest'
import { getSaved, getHistory, isSaved, toggleSaved, recordHistory, clearHistory, type SavedItem } from './store'

// distinct id ⇒ distinct content by default (each id gets its own headword), so the content-based
// identity (form+variety+reading, robust to lexeme-id reassignment on a DB rebuild) treats them apart.
const mk = (id: number, headword = 'x' + id): SavedItem => ({ id, headword, reading: null, variety: 'zh', gloss: null, ts: 0 })

describe('store - saved bookmarks + history (item 7)', () => {
  beforeEach(() => localStorage.clear())

  it('toggleSaved adds then removes, isSaved reflects state', () => {
    expect(isSaved(mk(1))).toBe(false)
    expect(toggleSaved(mk(1))).toBe(true)
    expect(isSaved(mk(1))).toBe(true)
    expect(getSaved().length).toBe(1)
    expect(toggleSaved(mk(1))).toBe(false)
    expect(isSaved(mk(1))).toBe(false)
    expect(getSaved().length).toBe(0)
  })

  it('saved list is newest-first', () => {
    toggleSaved(mk(1, 'a'))
    toggleSaved(mk(2, 'b'))
    expect(getSaved().map((s) => s.id)).toEqual([2, 1])
  })

  it('identity is content-based, not id-based (survives a lexeme-id rebuild)', () => {
    toggleSaved(mk(5000, '好'))
    // same word, different id (ids reassigned by a rebuild) → still saved
    expect(isSaved({ id: 12345, headword: '好', reading: null, variety: 'zh', query: undefined } as SavedItem)).toBe(true)
    // different word that reused the old id → NOT saved (no false bookmark / wrong unsave)
    expect(isSaved({ id: 5000, headword: '惡', reading: null, variety: 'zh', query: undefined } as SavedItem)).toBe(false)
  })

  it('recordHistory de-dupes and moves a revisit to the front', () => {
    recordHistory(mk(1))
    recordHistory(mk(2))
    recordHistory(mk(1))
    expect(getHistory().map((s) => s.id)).toEqual([1, 2])
  })

  it('history query dedup collapses only the consecutive live-typing chain', () => {
    const q = (headword: string): SavedItem => ({ id: 0, headword, reading: null, variety: 'zh', gloss: null, ts: 0, query: true })
    recordHistory(q('中')) // a standalone earlier search
    recordHistory(q('機')) // an unrelated search in between
    recordHistory(q('中國')) // prefix of nothing at the head (機) → 中 must survive
    expect(getHistory().map((s) => s.headword)).toEqual(['中國', '機', '中'])
    // consecutive typing chain DOES collapse: 大 then 大學 (head is 大, a prefix) → only 大學 kept
    recordHistory(q('大'))
    recordHistory(q('大學'))
    expect(getHistory().map((s) => s.headword)).toEqual(['大學', '中國', '機', '中'])
  })

  it('history is capped (no unbounded growth)', () => {
    for (let i = 0; i < 250; i++) recordHistory(mk(i))
    expect(getHistory().length).toBeLessThanOrEqual(200)
    expect(getHistory()[0].id).toBe(249) // newest kept
  })

  it('clearHistory empties history but not saved', () => {
    toggleSaved(mk(1))
    recordHistory(mk(2))
    clearHistory()
    expect(getHistory().length).toBe(0)
    expect(getSaved().length).toBe(1)
  })

  it('corrupt storage degrades to an empty list', () => {
    localStorage.setItem('kogu:saved', '{not json')
    expect(getSaved()).toEqual([])
  })

  it('char-only entries (negative ids) round-trip', () => {
    toggleSaved(mk(-22909, '好'))
    expect(isSaved(mk(-22909, '好'))).toBe(true)
    expect(getSaved()[0].headword).toBe('好')
  })
})
