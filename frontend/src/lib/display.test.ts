import { describe, it, expect } from 'vitest'
import { pickForms, matchLabel, regionsOf, shortGloss, varietyLabel } from './display'
import type { Form, Hit } from './types'

const f = (form: string, script: Form['script'], region: string | null = null, is_primary = false): Form =>
  ({ form, script, region, is_primary })

const zhHit = (forms: Form[]): Hit => ({
  lexeme_id: 1, variety: 'zh', headword: forms[0].form, reading: null,
  forms, glosses: [], match_type: 'exact', score: 1,
})

describe('pickForms — principled bracketing + script toggle', () => {
  const tradSimp = [f('機場', 'trad', null, true), f('机场', 'simp', 'CN')]

  // 1. prefer traditional -> trad primary, simp bracketed
  it('zh pref trad', () => {
    const d = pickForms(tradSimp, 'zh', 'trad')!
    expect(d.primary.form).toBe('機場')
    expect(d.alternate?.form).toBe('机场')
  })

  // 2. prefer simplified -> simp primary, trad bracketed (NOT inverted)
  it('zh pref simp', () => {
    const d = pickForms(tradSimp, 'zh', 'simp')!
    expect(d.primary.form).toBe('机场')
    expect(d.alternate?.form).toBe('機場')
  })

  // 3. identical across scripts -> NO bracket
  it('zh same form has no alternate', () => {
    const d = pickForms([f('山', 'trad', null, true), f('山', 'simp', 'CN')], 'zh', 'trad')!
    expect(d.primary.form).toBe('山')
    expect(d.alternate).toBeNull()
  })

  // 4. single form -> no alternate
  it('zh single form', () => {
    const d = pickForms([f('的士', 'trad', 'HK', true)], 'zh', 'trad')!
    expect(d.alternate).toBeNull()
  })

  // 5. japanese -> primary shinjitai, kana not bracketed
  it('ja primary shinjitai, no bracket', () => {
    const d = pickForms([f('会社', 'shinjitai', 'JP', true), f('かいしゃ', 'kana', 'JP')], 'ja', 'trad')!
    expect(d.primary.form).toBe('会社')
    expect(d.alternate).toBeNull()
  })

  // --- edge cases ---
  // E1. empty forms -> null
  it('empty forms', () => {
    expect(pickForms([], 'zh', 'trad')).toBeNull()
  })

  // E2. preferred script absent -> fall back to primary/first, still no spurious bracket
  it('zh pref absent falls back', () => {
    const d = pickForms([f('出租車', 'trad', 'CN', true)], 'zh', 'simp')!
    expect(d.primary.form).toBe('出租車')
    expect(d.alternate).toBeNull()
  })
})

describe('labels & helpers', () => {
  it('matchLabel maps all match types', () => {
    expect(matchLabel('exact').label).toBe('exact')
    expect(matchLabel('english').label).toBe('gloss')
    expect(matchLabel('weird').label).toBe('weird')
  })
  it('varietyLabel', () => {
    expect(varietyLabel('zh')).toBe('中')
    expect(varietyLabel('ja')).toBe('日')
    expect(varietyLabel('yue')).toBe('粵')
  })
  it('regionsOf keeps core-four order', () => {
    const hit = zhHit([f('x', 'trad', 'HK'), f('y', 'simp', 'CN'), f('z', 'shinjitai', 'JP')])
    expect(regionsOf(hit)).toEqual(['CN', 'HK', 'JP'])
  })
  it('shortGloss truncates and tolerates empty', () => {
    expect(shortGloss([])).toBe('')
    expect(shortGloss(['a'.repeat(200)]).endsWith('…')).toBe(true)
    expect(shortGloss(['airport'])).toBe('airport')
  })
})
