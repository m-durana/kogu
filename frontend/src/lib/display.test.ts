import { describe, it, expect } from 'vitest'
import { pickForms, primaryForm, matchLabel, regionsOf, shortGloss, varietyLabel, ocrSelectedText, furiganaTokens, pinyinMarks, cleanIds, cleanGloss, glossLine, briefGloss, isMinorGloss, meaningfulGlossCount, splitRecon, scriptShort, orderBranches, formTag, glossParts, linkifyHan, isBoundForm, describeIds, numWord, etymologyTokens, langTag, hanFont, isSoundLoan, soundLoanSource, soundLoanTitle, reformLabel, scriptChangeNote, scriptChangeFromForms, SEARCH_PLACEHOLDERS, placeholderAt, isAlwaysBound, jyutpingToYale } from './display'
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
  it('strips "(meaningless bound form)" too', () => {
    expect(cleanGloss('(old) actor; (meaningless bound form)')).toBe('(old) actor')
    expect(isMinorGloss('(meaningless bound form)')).toBe(true) // nothing left → minor
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

describe('linkifyHan / glossParts - every Han run becomes a tappable link', () => {
  it('splits "variant of X" so the glyph is a link', () => {
    expect(glossParts('variant of 著')).toEqual([
      { v: 'variant of ' },
      { v: '著', link: true },
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
  it('links a Han run anywhere in the gloss, not only after a cue word', () => {
    expect(linkifyHan('ear; cf 耳朵 here')).toEqual([
      { v: 'ear; cf ' },
      { v: '耳朵', link: true },
      { v: ' here' },
    ])
  })
  it('returns a single plain part for an ordinary gloss (no Han)', () => {
    expect(glossParts('to study; to learn')).toEqual([{ v: 'to study; to learn' }])
    expect(glossParts('a variant spelling')).toEqual([{ v: 'a variant spelling' }])
    expect(glossParts('see above')).toEqual([{ v: 'see above' }])
  })
})

describe('describeIds - composition with structure kept (森 = three 木)', () => {
  it('detects a repeated component', () => {
    expect(describeIds('⿱木⿰木木', '森')).toEqual({
      parts: [{ component: '木', count: 3 }],
      arrangement: 'stacked top to bottom',
      idc: '⿱',
      repeated: { component: '木', count: 3 },
    })
  })
  it('two-of-a-kind side by side (林 = two 木)', () => {
    const d = describeIds('⿰木木', '林')!
    expect(d.repeated).toEqual({ component: '木', count: 2 })
    expect(d.arrangement).toBe('side by side')
  })
  it('distinct components, no repetition (好 = 女 + 子)', () => {
    const d = describeIds('⿰女子', '好')!
    expect(d.repeated).toBeNull()
    expect(d.parts).toEqual([{ component: '女', count: 1 }, { component: '子', count: 1 }])
    expect(d.arrangement).toBe('side by side')
  })
  it('strips source tags before parsing', () => {
    expect(describeIds('⿰糸氏[GTV]', '紙')!.parts).toEqual([
      { component: '糸', count: 1 },
      { component: '氏', count: 1 },
    ])
  })
  it('atomic char (ids is just itself or empty) → null', () => {
    expect(describeIds('木', '木')).toBeNull()
    expect(describeIds(null)).toBeNull()
    expect(describeIds('')).toBeNull()
  })
  it('numWord names small counts', () => {
    expect(numWord(2)).toBe('two')
    expect(numWord(3)).toBe('three')
    expect(numWord(12)).toBe('12')
  })
})

describe('etymologyTokens - delineate merged statements + jargon tooltips + Han links', () => {
  it('splits newline-separated statements into separate segments', () => {
    const segs = etymologyTokens('First statement.\nSecond statement.')
    expect(segs.length).toBe(2)
    expect(segs[0].heading).toBeNull()
  })
  it('lifts "Etymology N" markers to headings and drops bare-header text', () => {
    // a header-only etymology (呆: "; Etymology 1\n; Etymology 2") yields no renderable segments
    expect(etymologyTokens('; Etymology 1\n; Etymology 2\n; Etymology 3')).toEqual([])
  })
  it('attaches an "Etymology N" heading to the statement that follows it', () => {
    const segs = etymologyTokens('; Etymology 1\nA pictogram.')
    expect(segs.length).toBe(1)
    expect(segs[0].heading).toBe('Etymology 1')
  })
  it('strips a leading "; " definition-list leak', () => {
    const segs = etymologyTokens(';"not have; not"')
    expect(segs[0].tokens[0]).toEqual({ t: 'text', v: '"not have; not"' })
  })
  it('tags jargon with a plain-English tooltip', () => {
    const toks = etymologyTokens('Phono-semantic compound: semantic part.')[0].tokens
    const abbr = toks.find((t) => t.t === 'abbr')
    expect(abbr).toBeTruthy()
    expect((abbr as { v: string }).v).toBe('Phono-semantic compound')
  })
  it('glosses newly-added technical terms (Tocharian, rendaku)', () => {
    const t1 = etymologyTokens('Borrowed from Tocharian B wänt-.')[0].tokens
    expect(t1.some((t) => t.t === 'abbr' && t.v === 'Tocharian B')).toBe(true)
    const t2 = etymologyTokens('shows rendaku here')[0].tokens
    expect(t2.some((t) => t.t === 'abbr' && t.v === 'rendaku')).toBe(true)
  })
  it('keeps reconstructions faint and Han runs tappable', () => {
    const toks = etymologyTokens('semantic 亻 (OC *maŋ) here')[0].tokens
    expect(toks.some((t) => t.t === 'recon')).toBe(true)
    expect(toks.some((t) => t.t === 'han' && t.v === '亻')).toBe(true)
  })
  it('makes a kanji with a reading into ruby, not a plain link', () => {
    const toks = etymologyTokens('甘(あま)し')[0].tokens
    expect(toks[0]).toEqual({ t: 'ruby', base: '甘', rt: 'あま' })
  })
  // item 19: Wiktionary "#" ordered-list markers (天's four head theories) become 1,2,3… not literal #
  it('numbers consecutive "#" list items and drops the raw marker', () => {
    const segs = etymologyTokens('four head variants:\n# Square block.\n# Elongated neck.\n# Two lines.')
    expect(segs[0].ordinal).toBeNull()
    expect([segs[1].ordinal, segs[2].ordinal, segs[3].ordinal]).toEqual([1, 2, 3])
    const flat = segs[1].tokens.map((t) => ('v' in t ? t.v : '')).join('')
    expect(flat).not.toContain('#')
    expect(flat).toContain('Square block.')
  })
  it('drops "#:" pronunciation-table rows like "*:"', () => {
    const segs = etymologyTokens('A theory.\n#: ipa-table leak')
    expect(segs.length).toBe(1)
  })
  // item 12: 六書 terms (指事…) are tappable Han links, not abbr tooltips; their English twin stays abbr
  it('renders 指事 as a Han hyperlink, not an abbreviation tooltip', () => {
    const toks = etymologyTokens('Ideogram (指事): points at an idea.')[0].tokens
    expect(toks.some((t) => t.t === 'abbr' && t.v === '指事')).toBe(false)
    expect(toks.some((t) => t.t === 'han' && t.v === '指事')).toBe(true)
    expect(toks.some((t) => t.t === 'abbr' && t.v === 'Ideogram')).toBe(true)
  })
  // item 10: a stacked competing theory (古) is flagged so accounts don't read as one run-on
  it('flags a stacked alternative theory as alt', () => {
    const segs = etymologyTokens('A graphic theory of the glyph.\nFrom Proto-Sino-Tibetan *r-ga.')
    expect(segs[0].alt).toBe(false)
    expect(segs[1].alt).toBe(true)
  })
  it('never flags the very first paragraph as alt, even when it opens with "From"', () => {
    expect(etymologyTokens('From Old Chinese root word.')[0].alt).toBe(false)
  })
})

describe('placeholderAt - rotating search placeholder (item 1)', () => {
  it('cycles through the list and wraps around', () => {
    expect(placeholderAt(0)).toBe(SEARCH_PLACEHOLDERS[0])
    expect(placeholderAt(SEARCH_PLACEHOLDERS.length)).toBe(SEARCH_PLACEHOLDERS[0])
    expect(placeholderAt(SEARCH_PLACEHOLDERS.length + 2)).toBe(SEARCH_PLACEHOLDERS[2])
  })
  it('is safe for negative indices', () => {
    expect(placeholderAt(-1)).toBe(SEARCH_PLACEHOLDERS[SEARCH_PLACEHOLDERS.length - 1])
  })
  it('every placeholder is a non-empty example, no em dashes', () => {
    expect(SEARCH_PLACEHOLDERS.length).toBeGreaterThanOrEqual(4)
    for (const p of SEARCH_PLACEHOLDERS) {
      expect(p.length).toBeGreaterThan(0)
      expect(p).not.toContain('—')
    }
  })
})

describe('jyutpingToYale - Cantonese romanization toggle', () => {
  it('low tones get a vowel diacritic plus h', () => {
    expect(jyutpingToYale('nei5')).toBe('néih')
    expect(jyutpingToYale('hai6')).toBe('haih')
    expect(jyutpingToYale('sik6')).toBe('sihk')
    expect(jyutpingToYale('lou5')).toBe('lóuh')
  })
  it('high/rising tones get a diacritic, no h', () => {
    expect(jyutpingToYale('si1')).toBe('sī')
    expect(jyutpingToYale('hou2')).toBe('hóu')
    expect(jyutpingToYale('gwong2')).toBe('gwóng')
  })
  it('converts initials and finals (j→y, jyu→yu, z→j, c→ch, oe→eu)', () => {
    expect(jyutpingToYale('jyu4')).toBe('yùh')
    expect(jyutpingToYale('ngo5')).toBe('ngóh')
  })
  it('handles a multi-syllable reading', () => {
    expect(jyutpingToYale('nei5 hou2')).toBe('néih hóu')
  })
  it('passes through tokens with no tone digit', () => {
    expect(jyutpingToYale('xyz')).toBe('xyz')
  })
})

describe('reformLabel / scriptChangeNote - item 14 script-change explanation', () => {
  it('maps reform ids to plain-language labels', () => {
    expect(reformLabel('opencc')).toBe('PRC simplification')
    expect(reformLabel('prc-1964')).toBe('PRC simplification')
    expect(reformLabel('jp-toyo')).toBe('Tōyō shinjitai reform')
    expect(reformLabel(null)).toBeNull()
    expect(reformLabel('mystery')).toBeNull()
  })
  it('returns null when the glyph has no orthodox parent (nothing changed)', () => {
    expect(scriptChangeNote('山', [])).toBeNull()
  })
  it('explains a simplification with the same-meaning clause and the reform reason', () => {
    const s = scriptChangeNote('汉', [
      { parent: '漢', edge_type: 'simplification', reform: 'opencc', reform_name: 'OpenCC', reform_year: null },
    ])!
    expect(s).toContain('carry the same meaning')
    expect(s).toContain('simplified Chinese form of 漢')
    expect(s).toContain('PRC simplification')
    expect(s).not.toContain('—') // no em dashes
  })
  it('builds the change note from the forms strip for the orthodox glyph (no own variants)', () => {
    const sf = {
      orthodox: '漢',
      is_kokuji: false,
      branches: [
        { form: '漢', script: 'traditional', reform_id: null, reform_label: null, is_orthodox: true },
        { form: '汉', script: 'simplified', reform_id: 'opencc', reform_label: 'PRC simplification', is_orthodox: false },
      ],
    }
    const s = scriptChangeFromForms(sf)!
    expect(s).toContain('carry the same meaning')
    expect(s).toContain('汉 is the simplified Chinese form')
    expect(s).toContain('PRC simplification')
  })
  it('says a dual shinjitai+simplified glyph is BOTH (萬 → 万), not one from the other', () => {
    const sf = {
      orthodox: '萬',
      is_kokuji: false,
      branches: [
        { form: '萬', script: 'traditional', reform_id: null, reform_label: null, is_orthodox: true },
        { form: '万', script: 'shinjitai+simplified', reform_id: null, reform_label: 'Tōyō shinjitai · PRC simplification', is_orthodox: false },
      ],
    }
    const s = scriptChangeFromForms(sf)!
    expect(s).toContain('万 is both the')
    expect(s).toContain('Japanese shinjitai')
    expect(s).toContain('simplified Chinese form')
  })
  it('returns null from the forms strip for a kokuji or a lone orthodox form', () => {
    expect(scriptChangeFromForms({ orthodox: '働', is_kokuji: true, branches: [] })).toBeNull()
    expect(
      scriptChangeFromForms({ orthodox: '山', is_kokuji: false, branches: [{ form: '山', script: 'traditional', reform_id: null, reform_label: null, is_orthodox: true }] }),
    ).toBeNull()
  })
  it('explains a shinjitai change with the reform year', () => {
    const s = scriptChangeNote('広', [
      { parent: '廣', edge_type: 'shinjitai', reform: 'jp-toyo', reform_name: 'Tōyō', reform_year: 1946 },
    ])!
    expect(s).toContain('Japanese shinjitai of 廣')
    expect(s).toContain('(1946)')
  })
})

describe('isBoundForm - detect a bound-morpheme marker across a row\'s glosses', () => {
  it('true when any gloss carries the "(bound form)" marker', () => {
    expect(isBoundForm(['(bound form) up; above'])).toBe(true)
    expect(isBoundForm(['no; not so', '(bound form) not; un-'])).toBe(true) // 不: only later sense bound
    expect(isBoundForm(['(meaningless bound form)'])).toBe(true)
  })
  it('false when no gloss is a bound form', () => {
    expect(isBoundForm(['dragon', 'surname Long'])).toBe(false)
    expect(isBoundForm([])).toBe(false)
    expect(isBoundForm(['the bound copy of a book'])).toBe(false) // "bound" but not the marker
  })
  // item 4: only "always bound" when EVERY sense is bound (日 is bound + free, so not always-bound)
  it('isAlwaysBound: true only when every sense is bound', () => {
    expect(isAlwaysBound(['(bound form) X', '(bound form) Y'])).toBe(true)
    expect(isAlwaysBound(['(bound form) sun', 'day', 'day of the month'])).toBe(false) // 日
    expect(isAlwaysBound(['mountain'])).toBe(false)
    expect(isAlwaysBound([])).toBe(false)
  })
})

describe('describeIds - drops cjkvi-ids placeholder leaves (item 3)', () => {
  it('returns null when the IDS has an unencodable placeholder (華 = ⿱艹⑦)', () => {
    expect(describeIds('⿱艹⑦', '華')).toBeNull()
  })
  it('still decomposes a normal IDS', () => {
    const r = describeIds('⿰木木', '林')
    expect(r?.parts).toEqual([{ component: '木', count: 2 }])
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

// ── #74: IDC operator exposed for the box diagram ─────────────────────────────
describe('describeIds.idc - the top-level Ideographic Description Char for the box diagram', () => {
  it('side-by-side ⿰', () => {
    expect(describeIds('⿰女子', '好')!.idc).toBe('⿰')
  })
  it('stacked ⿱', () => {
    expect(describeIds('⿱木⿰木木', '森')!.idc).toBe('⿱')
  })
  it('enclosure ⿴', () => {
    expect(describeIds('⿴囗玉', '国')!.idc).toBe('⿴')
  })
  it('lower-left wrap ⿺ (the 辶 / 廴 type)', () => {
    expect(describeIds('⿺辶首', '道')!.idc).toBe('⿺')
  })
  it('null when there is no IDC operator (flat/atomic)', () => {
    expect(describeIds('木', '木')).toBeNull()
  })
  it('strips [SOURCE] tags before reading the IDC', () => {
    expect(describeIds('⿰糸氏[GTV]', '紙')!.idc).toBe('⿰')
  })
})

// ── #13 / #85 / #87: Han linkification reaches the Supplementary Ideographic Plane ──
describe('linkifyHan - Ext-B (SIP) glyphs link instead of falling through as tofu', () => {
  it('links 𣥆 (U+23946, the 辵 origin glyph that used to render as plain text)', () => {
    expect(linkifyHan('version is 𣥆.')).toEqual([
      { v: 'version is ' },
      { v: '𣥆', link: true },
      { v: '.' },
    ])
  })
  it('links a run mixing BMP and SIP Han', () => {
    const parts = linkifyHan('害 from 𫲸 and 𥎆')
    expect(parts.filter((p) => p.link).map((p) => p.v)).toEqual(['害', '𫲸', '𥎆'])
  })
  it('still links ordinary BMP Han', () => {
    expect(glossParts('see 你')[1]).toEqual({ v: '你', link: true })
  })
  it('etymologyTokens emits a han token for an Ext-B glyph', () => {
    const toks = etymologyTokens('obsolete form 𣥆 here')[0].tokens
    expect(toks.some((t) => t.t === 'han' && t.v === '𣥆')).toBe(true)
  })
  it('furiganaTokens treats an Ext-B base as ruby', () => {
    expect(furiganaTokens('𣥆(あ)')).toEqual([{ t: 'ruby', base: '𣥆', rt: 'あ' }])
  })
})

// ── #14 / #86: origin sectioning — bullet depth, no fake numbering, marker cleanup ──
describe('etymologyTokens - Wiktionary bullet sub-points + leaked-marker cleanup', () => {
  it('a line led by "*" becomes depth 1 with the marker stripped', () => {
    const segs = etymologyTokens('Two theories:\n* Same source as 仁')
    expect(segs[1].depth).toBe(1)
    expect(segs[1].tokens[0]).toEqual({ t: 'text', v: 'Same source as ' })
  })
  it('"**" becomes depth 2', () => {
    const segs = etymologyTokens('lead\n** deeply nested point')
    expect(segs[1].depth).toBe(2)
  })
  it('top-level lines are depth 0', () => {
    expect(etymologyTokens('A pictogram.')[0].depth).toBe(0)
  })
  it('drops a "*:" pronunciation-table leak line entirely', () => {
    const segs = etymologyTokens('real prose\n*: /pʰ/ leaked IPA')
    expect(segs.length).toBe(1)
    expect(segs[0].tokens[0]).toEqual({ t: 'text', v: 'real prose' })
  })
  it('strips an orphan leading "]" (stripped reference tag)', () => {
    const segs = etymologyTokens(']\nA drawing of a cart')
    expect(segs.length).toBe(1)
    expect(segs[0].tokens[0]).toEqual({ t: 'text', v: 'A drawing of a cart' })
  })
  it('does NOT treat an inline reconstruction "*njin" as a bullet', () => {
    const segs = etymologyTokens('related to *njin forms')
    expect(segs[0].depth).toBe(0)
    expect(segs[0].tokens[0]).toEqual({ t: 'text', v: 'related to *njin forms' })
  })
})

// ── #92: region-correct font + lang per variety ───────────────────────────────
describe('langTag / hanFont - regional Han glyph selection by variety', () => {
  it('Japanese → ja + JP serif', () => {
    expect(langTag('ja')).toBe('ja')
    expect(hanFont('ja')).toBe('var(--han-ja)')
  })
  it('Cantonese → zh-Hant + TC serif', () => {
    expect(langTag('yue')).toBe('zh-Hant')
    expect(hanFont('yue')).toBe('var(--han-tc)')
  })
  it('Mandarin → zh-Hans + default (SC) serif', () => {
    expect(langTag('zh')).toBe('zh-Hans')
    expect(hanFont('zh')).toBe('var(--han)')
  })
  it('every variety maps to a non-empty BCP-47 tag', () => {
    for (const v of ['zh', 'yue', 'ja'] as const) expect(langTag(v).length).toBeGreaterThan(0)
  })
  it('ja and zh resolve to different fonts (the 誤 fix)', () => {
    expect(hanFont('ja')).not.toBe(hanFont('zh'))
  })
})

// ── #99 Fix A: drop trailing borrowed-source notes, keep the meaning ──
describe('cleanGloss - trailing "(from Japanese …)" source note removed', () => {
  it('strips the packaging note source tag', () => {
    expect(cleanGloss('containing (n pieces) (from Japanese 入 "iri")')).toBe('containing (n pieces)')
  })
  it('keeps the real meaning before a (from Japanese) note', () => {
    expect(cleanGloss('idiot; fool (from Japanese 馬鹿)')).toBe('idiot; fool')
  })
  it('handles (from English …)', () => {
    expect(cleanGloss('percent (from English)')).toBe('percent')
  })
  it('only strips a TRAILING source note, not mid-gloss text', () => {
    expect(cleanGloss('to enter; to go into')).toBe('to enter; to go into')
  })
  it('does not touch a normal parenthetical', () => {
    expect(cleanGloss('to conform to (as in 入時)')).toBe('to conform to (as in 入時)')
  })
})

// ── "written for sound" marker: phonetic-loan / transliteration words ──
describe('isSoundLoan - fires only on the phono-semantic-matching badge', () => {
  // 1. psm badge present (沙發 "sofa") → sound loan
  it('true when phono-semantic-matching is present', () => {
    expect(isSoundLoan(['borrowed-from-english', 'phono-semantic-matching'])).toBe(true)
  })
  // 2. psm alone (幽默 "humour") → sound loan
  it('true for psm alone', () => {
    expect(isSoundLoan(['phono-semantic-matching'])).toBe(true)
  })
  // 3. a plain borrowed loan that is NOT psm (a Sino-Japanese loan / wasei-kango) → NOT a sound loan
  it('false for borrowed/calque without psm', () => {
    expect(isSoundLoan(['borrowed', 'borrowed-from-japanese', 'wasei-kango'])).toBe(false)
  })
  // 4. a normal native word (電話) has no badges → NOT a sound loan
  it('false for empty badges', () => {
    expect(isSoundLoan([])).toBe(false)
  })
  // 5. null / undefined safety
  it('false for null/undefined', () => {
    expect(isSoundLoan(null)).toBe(false)
    expect(isSoundLoan(undefined)).toBe(false)
  })
})

describe('soundLoanSource + soundLoanTitle - name the source language when known', () => {
  it('extracts English from borrowed-from-english', () => {
    expect(soundLoanSource(['phono-semantic-matching', 'borrowed-from-english'])).toBe('English')
  })
  it('extracts French', () => {
    expect(soundLoanSource(['borrowed-from-french', 'phono-semantic-matching'])).toBe('French')
  })
  it('null when no borrowed-from-<lang> badge', () => {
    expect(soundLoanSource(['phono-semantic-matching'])).toBeNull()
  })
  it('title names the source when known', () => {
    expect(soundLoanTitle(['phono-semantic-matching', 'borrowed-from-english'])).toContain('from English')
  })
  it('title falls back to generic loanword wording', () => {
    const t = soundLoanTitle(['phono-semantic-matching'])
    expect(t).toContain('sound')
    expect(t).not.toContain('from ')
  })
})

describe('linkifyHan - link Han, never Hangul (item 159)', () => {
  it('links a Han character', () => {
    const parts = linkifyHan('變 from 馬')
    expect(parts.some((p) => p.v === '變' && p.link)).toBe(true)
    expect(parts.some((p) => p.v === '馬' && p.link)).toBe(true)
  })
  it('does NOT link Korean Hangul (말)', () => {
    const parts = linkifyHan('Korean 말 (mal)')
    expect(parts.some((p) => p.link)).toBe(false)
    expect(parts.map((p) => p.v).join('')).toBe('Korean 말 (mal)')
  })
  it('still links a Supplementary-plane ideograph', () => {
    const parts = linkifyHan('component 𣥆 here') // U+23946
    expect(parts.some((p) => p.v === '𣥆' && p.link)).toBe(true)
  })
})

describe('etymologyTokens - residual lines + dead cross-refs (items 153, 159)', () => {
  const flat = (text: string) =>
    etymologyTokens(text).map((s) => s.tokens.map((t: any) => t.v ?? '').join('')).join('\n')
  it('drops a line that is a lone marker char', () => {
    expect(flat('From 馬\n:\nmeaning horse')).toBe('From 馬\nmeaning horse')
  })
  it('drops a lone stray letter line ("h")', () => {
    expect(flat('Pictogram of a horse\nh\nmore prose')).toBe('Pictogram of a horse\nmore prose')
  })
  it('keeps a single CJK character line', () => {
    expect(flat('compare\n馬\nhorse')).toBe('compare\n馬\nhorse')
  })
  it('strips a trailing "More at *márkos." cross-reference', () => {
    const out = flat('From PIE *márkos. More at *márkos.')
    expect(out).not.toContain('More at')
    expect(out).toContain('*márkos')
  })
})

describe('etymology abbreviations - consistent + short historical names (item 158)', () => {
  const abbrs = (text: string) =>
    etymologyTokens(text)
      .flatMap((s) => s.tokens)
      .filter((t: any) => t.t === 'abbr')
  it('tags cognate regardless of case (Cognate / cognate / cognates)', () => {
    expect(abbrs('Cognate with 牛').some((t: any) => t.v === 'Cognate')).toBe(true)
    expect(abbrs('a cognate of X').some((t: any) => t.v === 'cognate')).toBe(true)
    expect(abbrs('these cognates').some((t: any) => t.v === 'cognates')).toBe(true)
  })
  it('shortens "Nihon Shoki of 720 CE" to "Nihon Shoki" with a tooltip', () => {
    const t: any = abbrs('Attested in the Nihon Shoki of 720 CE.').find((x: any) => x.title.includes('Nihon Shoki'))
    expect(t).toBeTruthy()
    expect(t.v).toBe('Nihon Shoki')
    expect(t.title).toContain('720')
  })
  it('keeps all-caps initialisms case-sensitive (OC tagged, lowercase oc not)', () => {
    expect(abbrs('From OC *mraːʔ').some((t: any) => t.v === 'OC')).toBe(true)
    expect(abbrs('the oc shop').some((t: any) => t.v === 'oc')).toBe(false)
  })
})
