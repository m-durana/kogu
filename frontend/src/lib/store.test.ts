import { beforeEach, describe, expect, it } from 'vitest'
import { getSaved, getHistory, isSaved, toggleSaved, recordHistory, clearHistory, type SavedItem } from './store'

const mk = (id: number, headword = 'x'): SavedItem => ({ id, headword, reading: null, variety: 'zh', gloss: null, ts: 0 })

describe('store - saved bookmarks + history (item 7)', () => {
  beforeEach(() => localStorage.clear())

  it('toggleSaved adds then removes, isSaved reflects state', () => {
    expect(isSaved(1)).toBe(false)
    expect(toggleSaved(mk(1))).toBe(true)
    expect(isSaved(1)).toBe(true)
    expect(getSaved().length).toBe(1)
    expect(toggleSaved(mk(1))).toBe(false)
    expect(isSaved(1)).toBe(false)
    expect(getSaved().length).toBe(0)
  })

  it('saved list is newest-first', () => {
    toggleSaved(mk(1, 'a'))
    toggleSaved(mk(2, 'b'))
    expect(getSaved().map((s) => s.id)).toEqual([2, 1])
  })

  it('recordHistory de-dupes and moves a revisit to the front', () => {
    recordHistory(mk(1))
    recordHistory(mk(2))
    recordHistory(mk(1))
    expect(getHistory().map((s) => s.id)).toEqual([1, 2])
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
    expect(isSaved(-22909)).toBe(true)
    expect(getSaved()[0].headword).toBe('好')
  })
})
