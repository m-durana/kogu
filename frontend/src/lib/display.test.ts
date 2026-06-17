import { describe, it, expect } from 'vitest'
import { pickForms, primaryForm, matchLabel, regionsOf, shortGloss, varietyLabel, ocrSelectedText, furiganaTokens, pinyinMarks, cleanIds, cleanGloss, glossLine, briefGloss, isMinorGloss, meaningfulGlossCount, splitRecon, scriptShort, orderBranches, formTag, glossParts } from './display'
import type { Form, Hit } from './types'

const f = (form: string, script: Form['script'], region: string | null = null, is_primary = false): Form =>
  ({ form, script, region, is_primary })

const zhHit = (forms: Form[]): Hit => ({
  lexeme_id: 1, variety: 'zh', headword: forms[0].form, reading: null,
  forms, glosses: [], match_type: 'exact', score: 1,
})

describe('pickForms - principled bracketing + script toggle', () => {
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

describe('primaryForm - echo the typed form (no toggle)', () => {
  const ts = [f('機場', 'trad', null, true), f('机场', 'simp', 'CN')]
  it('echoes the simplified query', () => {
    const d = primaryForm(ts, 'zh', '机场')!
    expect(d.primary.form).toBe('机场')
    expect(d.alternate?.form).toBe('機場')
  })
  it('echoes the traditional query', () => {
    const d = primaryForm(ts, 'zh', '機場')!
    expect(d.primary.form).toBe('機場')
    expect(d.alternate?.form).toBe('机场')
  })
  it('no/!match query falls back to canonical, still brackets the other', () => {
    const d = primaryForm(ts, 'zh', 'airport')!
    expect(d.primary.form).toBe('機場')
    expect(d.alternate?.form).toBe('机场')
  })
  it('identical forms across scripts -> no bracket', () => {
    const d = primaryForm([f('山', 'trad', null, true), f('山', 'simp', 'CN')], 'zh', '')!
    expect(d.alternate).toBeNull()
  })
  it('japanese: no bracketed alternate', () => {
    const d = primaryForm([f('会社', 'shinjitai', 'JP', true), f('かいしゃ', 'kana', 'JP')], 'ja', '会社')!
    expect(d.primary.form).toBe('会社')
    expect(d.alternate).toBeNull()
  })
  it('empty forms -> null', () => {
    expect(primaryForm([], 'zh', 'x')).toBeNull()
  })
})

describe('furiganaTokens - readings become ruby on the kanji', () => {
  it('kana reading after a kanji', () => {
    expect(furiganaTokens('甘(あま)し')).toEqual([
      { t: 'ruby', base: '甘', rt: 'あま' },
      { t: 'text', v: 'し' },
    ])
  })
  it('consecutive per-kanji readings', () => {
    expect(furiganaTokens('止(し)形(けい)')).toEqual([
      { t: 'ruby', base: '止', rt: 'し' },
      { t: 'ruby', base: '形', rt: 'けい' },
    ])
  })
  it('romaji reading also rubies', () => {
    expect(furiganaTokens('字(zi)')).toEqual([{ t: 'ruby', base: '字', rt: 'zi' }])
  })
  it('multi-char base run keeps one ruby', () => {
    expect(furiganaTokens('漢字(かんじ)')).toEqual([{ t: 'ruby', base: '漢字', rt: 'かんじ' }])
  })
  it('reading not after a kanji stays plain text', () => {
    expect(furiganaTokens('first (abc)')).toEqual([{ t: 'text', v: 'first (abc)' }])
  })
  it('plain text passes through', () => {
    expect(furiganaTokens('no readings here')).toEqual([{ t: 'text', v: 'no readings here' }])
  })
})

describe('pinyinMarks - numbered pinyin to tone marks', () => {
  it('places tone on a/e first', () => {
    expect(pinyinMarks('xue2')).toBe('xué')
    expect(pinyinMarks('hao3')).toBe('hǎo')
  })
  it('o in ou gets the mark', () => {
    expect(pinyinMarks('shou3 zhi3')).toBe('shǒu zhǐ')
  })
  it('last vowel when no a/e/ou', () => {
    expect(pinyinMarks('gui4')).toBe('guì')
  })
  it('neutral tone (5) drops the digit, no mark', () => {
    expect(pinyinMarks('ma5')).toBe('ma')
  })
  it('ü via v', () => {
    expect(pinyinMarks('lv4')).toBe('lǜ')
  })
  it('multi-syllable joins with spaces', () => {
    expect(pinyinMarks('ji1 chang3')).toBe('jī chǎng')
  })
  it('already-marked / non-pinyin passes through', () => {
    expect(pinyinMarks('xué')).toBe('xué')
    expect(pinyinMarks('')).toBe('')
  })
})

describe('cleanIds - strip source tags + IDC operators, keep components', () => {
  it('removes [GTV] tags and the ⿰ operator', () => {
    expect(cleanIds('⿰糸氏[GTV]')).toBe('糸 氏')
  })
  it('removes interleaved source tags', () => {
    expect(cleanIds('⿰亻[G]木[TV]')).toBe('亻 木')
  })
  it('strips the IDC operator from a plain IDS', () => {
    expect(cleanIds('⿱艹心')).toBe('艹 心')
  })
  it('nested operators all stripped', () => {
    expect(cleanIds('⿱⿰木木子')).toBe('木 木 子')
  })
  it('null/empty -> empty string', () => {
    expect(cleanIds(null)).toBe('')
    expect(cleanIds('')).toBe('')
  })
})

describe('cleanGloss - strip CC-CEDICT markup', () => {
  it('removes CL classifier clauses', () => {
    expect(cleanGloss('telephone; CL:通[tong1]; phone number')).toBe('telephone; phone number')
  })
  it('removes bracketed romanisation', () => {
    expect(cleanGloss('airport (abbr. for 航空港[hang2 kong1 gang3])')).toBe('airport (abbr. for 航空港)')
  })
  it('collapses trad|simp pipe pairs', () => {
    expect(cleanGloss('variant of 繫|系[xi4]')).toBe('variant of 繫')
  })
  it('drops Taiwan pr. notes and trailing tags', () => {
    expect(cleanGloss('hair; Taiwan pr. [fa3]')).toBe('hair')
  })
  it('trims dangling separators', () => {
    expect(cleanGloss('to love; ')).toBe('to love')
    expect(cleanGloss('; people; bunch; gang;')).toBe('people; bunch; gang')
  })
  it('plain gloss untouched', () => {
    expect(cleanGloss('to study; to learn')).toBe('to study; to learn')
  })
  it('bracket removed before pipe collapse (no leftover tail)', () => {
    expect(cleanGloss('used in 自個兒|自个儿[zi4 ge3 r5]')).toBe('used in 自個兒')
  })
  it('strips "(bound form)" grammatical jargon', () => {
    expect(cleanGloss('(bound form) row; line; (bound form) trade')).toBe('row; line; trade')
    // a real sense starting with (bound form) is no longer treated as minor
    expect(isMinorGloss('(bound form) row; line')).toBe(false)
  })
  it('strips CC-Canto (Cantonese) tag and Mandarin-equivalent note', () => {
    // the 粵語 row label + the "written differently" bridge now carry this info structurally
    expect(cleanGloss('to not have (Cantonese) (Mandarin equivalent: 沒有|没有[mei2 you3])')).toBe('to not have')
    expect(cleanGloss('so (Cantonese); Mandarin equivalent: 這樣|这样[zhe4 yang4]')).toBe('so')
    expect(cleanGloss('(Cantonese) he, she, it')).toBe('he, she, it')
  })
  it('glossLine cleans, filters empties, caps count', () => {
    expect(glossLine(['a', 'CL:个[ge4]', 'b', 'c', 'd', 'e'], 4)).toBe('a; b; c; d')
  })
  it('briefGloss keeps short glosses whole', () => {
    expect(briefGloss(['to study; to learn'])).toBe('to study; to learn')
  })
  it('briefGloss caps long glosses on a clause boundary', () => {
    const out = briefGloss(['airport; airfield; service provider for circumventing censorship online'])
    expect(out.endsWith('…')).toBe(true)
    expect(out.length).toBeLessThanOrEqual(66)
    expect(out.startsWith('airport; airfield')).toBe(true)
  })
})

describe('script-forms helpers', () => {
  it('scriptShort maps single + joined scripts to English tags', () => {
    expect(scriptShort('traditional')).toBe('TC')
    expect(scriptShort('simplified')).toBe('SC')
    expect(scriptShort('shinjitai')).toBe('JP')
    expect(scriptShort('simplified+shinjitai')).toBe('SC JP')
  })
  it('formTag maps surface-form scripts', () => {
    expect(formTag('trad')).toBe('TC')
    expect(formTag('simp')).toBe('SC')
    expect(formTag('kana')).toBe('')
  })
  it('orderBranches sorts traditional → simplified → shinjitai', () => {
    const b = (script: string) => ({ form: 'x', script, reform_id: null, reform_label: null, is_orthodox: false })
    const out = orderBranches([b('shinjitai'), b('simplified'), b('traditional')])
    expect(out.map((x) => x.script)).toEqual(['traditional', 'simplified', 'shinjitai'])
  })
})

describe('splitRecon - de-emphasise phonological reconstructions', () => {
  it('splits an OC parenthetical', () => {
    expect(splitRecon('漢 (OC *n̥ˁar): water')).toEqual([
      { t: 'text', v: '漢 ' },
      { t: 'recon', v: '(OC *n̥ˁar)' },
      { t: 'text', v: ': water' },
    ])
  })
  it('splits a slashed reconstruction', () => {
    expect(splitRecon('from /*ʔɨts/ in')).toEqual([
      { t: 'text', v: 'from ' },
      { t: 'recon', v: '/*ʔɨts/' },
      { t: 'text', v: ' in' },
    ])
  })
  it('plain prose stays one text token', () => {
    expect(splitRecon('a calque of English')).toEqual([{ t: 'text', v: 'a calque of English' }])
  })
})

describe('isMinorGloss / meaningfulGlossCount', () => {
  it('flags surnames, variants, radicals as minor', () => {
    expect(isMinorGloss('surname Long')).toBe(true)
    expect(isMinorGloss('variant of 痴[chi1]')).toBe(true)
    expect(isMinorGloss('used in 乜斜')).toBe(true)
    expect(isMinorGloss('"house on a cliff" radical in Chinese characters')).toBe(true)
    expect(isMinorGloss('')).toBe(true)
  })
  it('keeps real meanings', () => {
    expect(isMinorGloss('dragon')).toBe(false)
    expect(isMinorGloss('to study; to learn')).toBe(false)
  })
  it('meaningfulGlossCount ignores minor glosses', () => {
    expect(meaningfulGlossCount(['surname Mu', 'tree; wood'])).toBe(1)
    expect(meaningfulGlossCount(['surname Shui'])).toBe(0)
  })
})

describe('glossParts - tappable cross-reference target', () => {
  it('splits "variant of X" so the glyph is a link', () => {
    expect(glossParts('variant of 著')).toEqual([
      { v: 'variant of ' },
      { v: '著', link: true },
      { v: '' },
    ])
  })
  it('handles "old variant of", "used in", "see", "see also"', () => {
    expect(glossParts('old variant of 群')[1]).toEqual({ v: '群', link: true })
    expect(glossParts('used in 乜斜')[1]).toEqual({ v: '乜斜', link: true })
    expect(glossParts('see 你')[1]).toEqual({ v: '你', link: true })
    expect(glossParts('see also 妳')[1]).toEqual({ v: '妳', link: true })
  })
  it('keeps trailing prose after the glyph as a separate text part', () => {
    expect(glossParts('variant of 着 (to wear)')).toEqual([
      { v: 'variant of ' },
      { v: '着', link: true },
      { v: ' (to wear)' },
    ])
  })
  it('returns a single plain part for an ordinary gloss (no link)', () => {
    expect(glossParts('to study; to learn')).toEqual([{ v: 'to study; to learn' }])
    // "variant" without a following Han glyph is not a cross-reference
    expect(glossParts('a variant spelling')).toEqual([{ v: 'a variant spelling' }])
  })
  it('does not match a target that is not a Han glyph', () => {
    expect(glossParts('see above')).toEqual([{ v: 'see above' }])
  })
})

describe('ocrSelectedText - OCR character selection', () => {
  const lines = [
    { chars: [{ ch: '機' }, { ch: '場' }] },
    { chars: [{ ch: '空' }, { ch: '港' }] },
  ]
  it('empty selection -> empty string', () => {
    expect(ocrSelectedText(lines, new Set())).toBe('')
  })
  it('single character', () => {
    expect(ocrSelectedText(lines, new Set(['0-1']))).toBe('場')
  })
  it('keeps document order regardless of tap order', () => {
    // select 港(line1) before 機(line0) - output must be document order 機…港
    expect(ocrSelectedText(lines, new Set(['1-1', '0-0']))).toBe('機港')
  })
  it('whole line', () => {
    expect(ocrSelectedText(lines, new Set(['1-0', '1-1']))).toBe('空港')
  })
  it('all selected', () => {
    expect(ocrSelectedText(lines, new Set(['0-0', '0-1', '1-0', '1-1']))).toBe('機場空港')
  })
  it('ignores out-of-range keys', () => {
    expect(ocrSelectedText(lines, new Set(['9-9', '0-0']))).toBe('機')
  })
})
