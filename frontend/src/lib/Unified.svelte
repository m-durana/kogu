<script module lang="ts">
  // Per-word UI state cache: when you open bound / show-more / origin / used-in, click a link, then
  // come back, the panels stay as you left them (cached ~1h). A fresh search of the same term resets
  // it (the component restores from cache only on back/forward; App re-creates state on a new search).
  type UiSnap = {
    expanded: number[]
    showOrigin: boolean
    showWords: boolean
    jaReadOpen: boolean
    yueReadOpen: boolean
    activeTab: string
    boundId: number | null
    ts: number
  }
  const UI_TTL = 60 * 60 * 1000 // ~1 hour
  const uiCache = new Map<string, UiSnap>()
</script>

<script lang="ts">
  import type { CharInfo, Entry, Hit, ReadingKV, Variety } from './types'
  import { primaryForm, varietyLabel, varietyName, headwordGlyphSize, pinyinMarks, cleanGloss, glossLine, briefGloss, meaningfulGlossCount, isMinorGloss, formTag, glossParts, isBoundForm, isAlwaysBound, describeIds, numWord, etymologyTokens, langTag, hanFont, isSoundLoan, soundLoanTitle, scriptShort, scSwitchTarget, scriptChangeNote, scriptChangeFromForms, jyutpingToYale, mcSoundLink, regionTags, expandSenses, formatReading, pitchPattern, moraSplit } from './display'
  import { speakReading, canSpeak } from './speech'
  import { settings } from './settings.svelte'
  import ScriptForms from './ScriptForms.svelte'
  import IdcBox from './IdcBox.svelte'
  import Glyph from './Glyph.svelte'
  import EntryRow from './EntryRow.svelte'
  import { AlertTriangle, Volume2, ArrowLeftRight, Plus, Minus } from '@lucide/svelte'

  // a reading shown in the user's chosen romanisation (shared with the result/saved lists via display.ts)
  function dispReading(variety: string, reading: string): string {
    return formatReading(variety, reading, settings.romanization === 'yale')
  }
  // Japanese pitch-accent contour for a ja kana reading: per-mora cells flagged high/low, with the
  // mora after which the pitch drops (the downstep). Returns null when there's no usable accent, so the
  // plain reading renders unchanged. Monochrome overline + tick is drawn from these cells in the markup.
  type PitchCell = { mora: string; high: boolean; drop: boolean }
  function pitchCells(reading: string, accent: string | null | undefined): PitchCell[] | null {
    if (!settings.pitchAccent) return null // user turned the contour off (Settings)
    const p = pitchPattern(reading, accent)
    if (!p) return null
    const morae = moraSplit(reading)
    // odaka: the drop lands on a FOLLOWING particle (downstep === length) — there is no in-word tick,
    // but the whole word is high; render the tick at the trailing edge of the last mora.
    return morae.map((mora, i) => ({
      mora,
      high: p.highs[i],
      drop: p.downstep !== null && i + 1 === p.downstep,
    }))
  }
  // show the speaker affordance only when the browser can play audio AND the user hasn't turned
  // pronunciation audio off in Settings.
  const speakOn = $derived(canSpeak() && settings.audio)
  // which speaker is currently sounding — lights that one button up until playback finishes (or another
  // tap supersedes it). Keyed by variety+reading so each row's speaker tracks independently.
  let playingKey = $state<string | null>(null)
  function speak(key: string, reading: string | null | undefined, variety: string, form?: string, accent?: string | null) {
    playingKey = key
    speakReading(reading, variety, form, accent).finally(() => {
      if (playingKey === key) playingKey = null
    })
  }
  import { readingRomaji } from './romaji'

  // The unified cross-language view - one Han word, seen across 中 / 粵 / 日 at once.
  // Renders instantly from search hits; enriches (decomposition, origin) when the full entry loads.
  let {
    hits = [],
    entry = null,
    enriching = false,
    anchor = '',
    onsearch,
  }: {
    hits?: Hit[]
    entry?: Entry | null
    enriching?: boolean
    anchor?: string
    onsearch: (q: string) => void
  } = $props()

  const VORDER = ['zh', 'yue', 'ja']
  // English language names — used only by the false-friend band ("written the same in Chinese and
  // Japanese…"), where CJK glyphs would read as the words themselves rather than as labels. The
  // per-language meaning rows use the short 中/粵/日 tags (varietyLabel) instead.
  const langName: Record<string, string> = { zh: 'Chinese', yue: 'Cantonese', ja: 'Japanese' }

  function readingFor(variety: string, readings: ReadingKV[]): string {
    const order =
      variety === 'zh' ? ['pinyin', 'zhuyin'] : variety === 'yue' ? ['jyutping'] : ['kana', 'romaji']
    for (const k of order) {
      const v = readings.filter((r) => r.kind === k).map((r) => r.value)
      if (v.length) return v.join('  ')
    }
    return ''
  }
  // Japanese pitch accent (Kanjium) for the FIRST kana reading — the one readingFor() displays on the
  // ja row. Only ja kana readings carry an accent; everything else returns null (no contour shown).
  function accentFor(variety: string, readings: ReadingKV[]): string | null {
    if (variety !== 'ja') return null
    const kana = readings.find((r) => r.kind === 'kana')
    return kana?.accent ?? null
  }

  const relById = $derived(new Map((entry?.same_form ?? []).map((l) => [l.lexeme_id, l.relation])))
  function relFor(id: number): string {
    if (entry && id === entry.lexeme_id) return 'self'
    return relById.get(id) ?? 'self'
  }

  type Row = {
    id: number
    variety: Variety
    form: string
    alt: string | null
    formScript: string
    altScript: string
    reading: string
    accent?: string | null // Japanese pitch accent (Kanjium) for the displayed ja kana reading
    glosses: string[]
    relation: string
    kind: 'form' | 'equiv' // same characters, vs a meaning-equivalent written differently
    synthetic?: boolean // a Japanese row derived from the character (Kanjidic), not a real ja word-lexeme
  }

  // one row per language word, merged from hits (instant) + entry/same_form (enriched)
  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = []
    const seen = new Set<number>()
    for (const h of hits) {
      if (seen.has(h.lexeme_id)) continue
      seen.add(h.lexeme_id)
      const d = primaryForm(h.forms, h.variety, anchor)
      out.push({
        id: h.lexeme_id,
        variety: h.variety,
        form: d?.primary.form ?? h.headword,
        alt: d?.alternate?.form ?? null,
        formScript: d?.primary.script ?? '',
        altScript: d?.alternate?.script ?? '',
        reading: h.reading ?? '',
        accent: h.accent ?? null,
        glosses: h.glosses,
        relation: relFor(h.lexeme_id),
        kind: 'form',
      })
    }
    if (entry && !seen.has(entry.lexeme_id)) {
      seen.add(entry.lexeme_id)
      const d = primaryForm(entry.forms, entry.variety, anchor)
      out.push({
        id: entry.lexeme_id,
        variety: entry.variety,
        form: d?.primary.form ?? entry.headword,
        alt: d?.alternate?.form ?? null,
        formScript: d?.primary.script ?? '',
        altScript: d?.alternate?.script ?? '',
        reading: readingFor(entry.variety, entry.readings),
        accent: accentFor(entry.variety, entry.readings),
        glosses: entry.senses.map((s) => s.gloss_en),
        relation: 'self',
        kind: 'form',
      })
    }
    if (entry)
      for (const l of entry.same_form) {
        if (seen.has(l.lexeme_id)) continue
        seen.add(l.lexeme_id)
        out.push({
          id: l.lexeme_id,
          variety: l.variety,
          form: l.headword,
          alt: null,
          formScript: '',
          altScript: '',
          reading: l.reading ?? '',
          accent: l.accent ?? null,
          glosses: l.glosses,
          relation: l.relation,
          kind: 'form',
        })
      }
    // Collapse lexemes that are the SAME word - same variety + form + READING - into one row, MERGING
    // their senses so no meaning is dropped (京都 jīng dū = "Kyoto (city in Japan)" + "capital of a
    // country", both kept). Grouping by reading keeps true homographs distinct (行 háng "row" vs xíng
    // "to walk" stay separate rows). Reading match is case/space-insensitive ("Jing1 du1" == "jing1 du1").
    const primary = hits[0]?.lexeme_id ?? entry?.lexeme_id ?? -1
    const readingKey = (s: string | null) => (s ?? '').toLowerCase().replace(/\s+/g, '')
    const groups = new Map<string, Row[]>()
    for (const r of out) {
      const key = `${r.variety}|${r.form}|${readingKey(r.reading)}`
      const arr = groups.get(key)
      if (arr) arr.push(r)
      else groups.set(key, [r])
    }
    let deduped: Row[] = []
    for (const members of groups.values()) {
      // richest member (tie → the looked-up one) supplies id / reading / form / script tags
      const best = members.reduce((a, b) => {
        const ra = meaningfulGlossCount(a.glosses)
        const rb = meaningfulGlossCount(b.glosses)
        if (rb > ra) return b
        if (rb === ra && b.id === primary) return b
        return a
      })
      // merge senses across members, de-duplicating identical glosses (case-insensitive)
      const seenG = new Set<string>()
      const glosses: string[] = []
      for (const m of [best, ...members.filter((m) => m !== best)]) {
        for (const g of m.glosses) {
          const k = cleanGloss(g).toLowerCase()
          if (k && !seenG.has(k)) {
            seenG.add(k)
            glosses.push(g)
          }
        }
      }
      // a false friend only if NO member shares meaning with the other language (every one is one);
      // 京都 has a cognate sense (Kyoto) so it isn't flagged, 手紙 (all false-friend) still is.
      const relation = members.find((m) => m.relation !== 'false-friend')?.relation ?? 'false-friend'
      deduped.push({ ...best, glosses, relation })
    }
    // drop rows whose only content is a surname/variant cross-reference - unless it's the row you
    // looked up, or it's the sole row for its language (so a purely-minor entry still shows).
    const richByVar = new Set(deduped.filter((r) => meaningfulGlossCount(r.glosses) > 0).map((r) => r.variety))
    deduped = deduped.filter(
      (r) =>
        r.id === primary ||
        meaningfulGlossCount(r.glosses) > 0 ||
        !richByVar.has(r.variety),
    )
    // meaning-equivalents: how this meaning is written in a language that DOESN'T share the glyph
    // (機場 ↔ 日 空港; 冇 → 中 沒有). Only TRUSTED explicit equivalence edges (relation 'equivalent':
    // CC-Canto inline + curated cross-language) are shown. The fuzzy English-gloss-pivot synonyms are
    // deliberately NOT bridged - they produced wrong "written differently" rows (騰→開ける, 津→汗) by
    // matching a minor/secondary sense, which undermines trust. Precision over coverage.
    const haveKey = new Set(deduped.map((r) => `${r.variety}|${r.form}`))
    for (const l of entry?.translations ?? []) {
      if (l.relation !== 'equivalent') continue
      const key = `${l.variety}|${l.headword}`
      if (haveKey.has(key)) continue
      haveKey.add(key)
      deduped.push({
        id: l.lexeme_id,
        variety: l.variety,
        form: l.headword,
        alt: null,
        formScript: '',
        altScript: '',
        reading: l.reading ?? '',
        accent: l.accent ?? null,
        glosses: l.glosses,
        relation: l.relation,
        kind: 'equiv',
      })
    }
    return deduped.sort((a, b) => VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety))
  })

  // the headword glyph: what the user looked up
  const head = $derived(anchor || rows[0]?.form || '')
  // the variety the looked-up glyph resolved to — drives the headword's regional font so a Japanese
  // word's 誤 renders with the Japanese glyph, not the Simplified-Chinese one.
  const headVariety = $derived<Variety>((hits[0]?.variety ?? entry?.variety ?? 'zh') as Variety)
  // the searched word split into its glyphs — used to echo the form AS TYPED in the characters
  // breakdown (search the simplified/JP 食虫植物 and 虫 stays 虫, not the trad 蟲 the lexeme is keyed on).
  const headChars = $derived([...head])

  // the (language, form) this page resolved to - marked in the stack. Keyed by form, not lexeme id,
  // because dedupe may keep a richer lexeme of the same written form than the exact top hit.
  const primaryKey = $derived.by(() => {
    if (hits.length) {
      const d = primaryForm(hits[0].forms, hits[0].variety, anchor)
      return `${hits[0].variety}|${d?.primary.form ?? hits[0].headword}`
    }
    if (entry) {
      const d = primaryForm(entry.forms, entry.variety, anchor)
      return `${entry.variety}|${d?.primary.form ?? entry.headword}`
    }
    return ''
  })

  const single = $derived([...head].length === 1)
  const headChar = $derived(entry?.characters?.[0])
  // the headword glyph shrinks for long multi-character words (idioms, kana+kanji verbs like
  // あずかり知る) so the header stays compact and never collides with the save/share buttons (item 3).
  const glyphSize = $derived(headwordGlyphSize(headChars.length))
  const isKana = (s: string) => /[぀-ヿ]/.test(s)
  // languages this word is actually represented in by a REAL word-lexeme (its same-glyph form rows) -
  // gates the structure readings so a 粵-only word shows jyutping, not a nominal Mandarin pinyin.
  const wordVarieties = $derived(new Set(rows.filter((r) => r.kind === 'form').map((r) => r.variety)))

  // Kanjidic kun readings carry okurigana markers — a dot for the okurigana boundary (あ.う = 合 covers
  // あ, う is the trailing kana) and affix hyphens (-あ.う prefix use, あい- suffix use). For a compact
  // reading list we strip both to a plain kana so it reads cleanly instead of "あ.う -あ.う あい-".
  const cleanKanaReading = (v: string): string => v.replace(/[.\-]/g, '')

  // A single Han character can be a genuine word in a language WITHOUT a standalone word-lexeme - e.g.
  // 津 (harbor) is a real kanji (シン/つ) but Japanese only uses it inside compounds, so there is no
  // ja lexeme and it would wrongly show as Chinese-only. Kanjidic kana on/kun is a reliable "used in
  // Japanese" signal, so we synthesize a co-equal 日本語 definition row from the character's own data.
  // (冇 has no on/kun → no synthetic row; its nominal Mandarin pinyin stays suppressed.)
  const synthJaRow = $derived.by<Row | null>(() => {
    // a bound Kangxi radical (彳, 氵) is a component, not a Japanese word — don't synthesize a co-equal
    // 日本語 definition row for it (its okurigana kun reading would otherwise trip the gate below).
    if (!single || !headChar || headChar.is_radical) return null
    const on = headChar.readings.filter((r) => r.kind === 'onyomi' && isKana(r.value)).map((r) => r.value)
    const kun = headChar.readings.filter((r) => r.kind === 'kunyomi' && isKana(r.value)).map((r) => r.value)
    if (!on.length && !kun.length) return null
    // Kanjidic lists readings for MANY Chinese-only kanji (媽 → はは) that aren't actually used in
    // Japanese. Two reliable "really used in Japanese" signals: (a) the kanji appears in a Japanese
    // word, or (b) it has an okurigana kun reading (よ.じる) — a native verb/adjective stem, so the
    // kanji forms a real Japanese word like 攀じる even when JMdict lacks that rare entry. 媽 has
    // neither (はは is a bare nominal reading, 0 ja words) → still suppressed.
    const usedInJa = (entry?.compounds ?? []).some((l) => l.variety === 'ja')
    const hasOkurigana = headChar.readings.some((r) => r.kind === 'kunyomi' && r.value.includes('.'))
    if (!usedInJa && !hasOkurigana) return null
    const gloss = headChar.gloss_ja || headChar.gloss_en || ''
    if (!gloss) return null
    // the Japanese form is the shinjitai if one exists, else the typed glyph itself. We do NOT fall back
    // to the backbone "orthodox" form: this row only fires when the typed glyph HAS Kanjidic on/kun, i.e.
    // it IS a Japanese kanji that Japan writes as-is (合, 電) — so `head` is correct. Using `orthodox`
    // mis-rendered 合 as its spurious Unihan "traditional" 閤 (a kSimplifiedVariant artifact, not a real
    // reform). The PRC-only-simplified case (电→電) never reaches here: 电 has no Kanjidic readings.
    const sf = headChar.script_forms
    const jaForm = sf?.branches.find((b) => b.script.includes('shinjitai'))?.form ?? head
    return {
      id: -(head.codePointAt(0) ?? 1) - 1,
      variety: 'ja',
      form: jaForm,
      alt: null,
      formScript: '',
      altScript: '',
      reading: [on.map(cleanKanaReading).join(' '), kun.map(cleanKanaReading).join(' ')].filter(Boolean).join('    '),
      // Kanjidic packs all senses into one ';'-joined string; split so it enumerates (1. ocean 2. sea
      // …) like every other definition instead of rendering as one semicolon blob.
      glosses: gloss.split(';').map((s) => s.trim()).filter(Boolean),
      relation: 'self',
      kind: 'form',
      synthetic: true,
    }
  })
  // When we synthesize the rich Kanjidic Japanese row for a single kanji (full on/kun + romaji), drop
  // the plain same-glyph ja WORD rows: the full JMdict now has a 機(き)/機(はた) lexeme for almost every
  // kanji, and showing those bare rows hid the consolidated romaji reading list. Different-glyph ja
  // words (cognates/compounds) are unaffected.
  const allRows = $derived.by<Row[]>(() => {
    if (!synthJaRow) return rows
    const dup = rows.filter((r) => !(r.variety === 'ja' && r.kind === 'form' && r.form === synthJaRow.form))
    return [...dup, synthJaRow]
  })

  // === The co-equal cross-language model ===
  // There is NO single privileged headword. A Han glyph that is a real word in two or more languages
  // is the NORM, not the exception - it is the whole point of the app. So the typed glyph's meaning is
  // shown for EVERY language that writes it that way, side by side, co-equally (學 = 中 learn + 日
  // learning; 手紙 = 中 toilet-paper + 日 letter). Then, separately, we show how the SAME meaning is
  // written with a DIFFERENT glyph in another language (機場 ↔ 日 空港) as tappable bridges.

  // Block A - the definition: every language that writes the word exactly as typed. Best-match-first:
  // the language whose form you actually typed leads (it's the top search hit), the rest follow in
  // 中/粵/日 order. Query-driven, not a hard-coded language rank - so no language is privileged by fiat.
  const defRows = $derived(
    allRows
      // same-glyph words + the synthesized Japanese row (same character in Japan's script, e.g. 电→電):
      // all co-equal definitions. The synth row shows Japan's form when it differs from the typed glyph.
      .filter((r) => r.synthetic || (r.kind === 'form' && r.form === head))
      .sort((a, b) => {
        // frequency-led: the top search hit (highest-frequency form) leads, then 中/粵/日.
        const am = `${a.variety}|${a.form}` === primaryKey ? 0 : 1
        const bm = `${b.variety}|${b.form}` === primaryKey ? 0 : 1
        return am !== bm ? am - bm : VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety)
      }),
  )
  const isGlyphSearch = $derived(defRows.length > 0)
  // a component glyph that carries no dictionary sense of its own but appears inside other characters
  const glossless = $derived(
    isGlyphSearch && (entry?.senses?.length ?? 0) === 0 && (entry?.appears_in?.length ?? 0) > 0,
  )
  // "written for sound" marker: a MULTI-character transliteration / phonetic loan (沙發, 幽默, 俱樂部)
  // whose characters were picked for their sound, not meaning. Driven by the entry's origin badges
  // (phono-semantic-matching). Single characters are excluded — they use the component role display.
  const soundLoan = $derived(!single && !!entry && isSoundLoan(entry.origin_badges))
  // shown when the user taps the "written for sound" chip (reuses the term-explainer popup). Prefer the
  // badge's specific note ("Loanword from English: …"); fall back to a generic explanation.
  const soundLoanExplain = $derived(
    (soundLoan && soundLoanTitle(entry!.origin_badges)) ||
      'Written for sound: a loanword whose characters were chosen for how they sound, not for their meaning.',
  )

  // Cantonese shares the Han script: a single character written 中 is almost always written and
  // understood the same in 粵, differing only in pronunciation. So when there's no SEPARATE Cantonese
  // row (which would signal a Cantonese-specific word/meaning, e.g. 係 hai6 / 乜), we surface the
  // character's jyutping right on the Chinese row — 中 ěr · 粵 ji5 — instead of burying it below.
  // ALL the character's Cantonese readings (a polyphonic char has several: 行 hang4/hang6/hong4),
  // customary first (backend orders by `ord`). Each renders with its own speaker, like the JA on/kun.
  const headJyutList = $derived.by<string[]>(() =>
    single && headChar
      ? headChar.readings.filter((r) => r.kind === 'jyutping').map((r) => r.value)
      : [],
  )
  const hasYueDef = $derived(defRows.some((r) => r.variety === 'yue'))
  // single character's composition (what parts make it up, with structure kept): 森 = three 木
  const comp = $derived(single && headChar ? describeIds(headChar.ids, head) : null)
  // recursive "N copies of one base" decomposition from the backend (森 → 木 ×3), preferred over the
  // shallow flat parts when present
  const decomp = $derived(single && headChar ? headChar.decomp : null)
  // component → meaning, so the structure section explains the parts (女 "woman", 木 "tree")
  const compGloss = $derived(new Map((headChar?.components ?? []).map((c) => [c.ch, c.gloss])))
  // at most the first two meanings, split on BOTH ';' and ',' (Unihan glosses like 天 "sky, heaven,
  // day, sky god…" use commas, so a one-sense cap that splits only ';' would leak the whole list).
  function briefMeaning(g: string | null): string {
    const parts = cleanGloss(g ?? '').split(/[;,]/).map((s) => s.trim()).filter(Boolean)
    const major = parts.filter((p) => !isMinorGloss(p))
    return (major.length ? major : parts).slice(0, 2).join(', ')
  }
  const meaningOf = (ch: string) => briefMeaning(compGloss.get(ch) ?? '')
  // phono-semantic roles: when the backend knows which component carries the MEANING vs the SOUND
  // (媽 = 女 semantic + 馬 phonetic) we render the component list with role badges instead of the flat
  // "made of" parts. Only when at least one role is known.
  const roleParts = $derived(single && headChar ? headChar.components : [])
  const hasRoles = $derived(roleParts.some((c) => c.role === 'semantic' || c.role === 'phonetic'))

  // phonological "why": the character's own Middle Chinese reading(s) and, for the phonetic component,
  // whether they shared a sound back then (銅 duwng from 同 duwng). Baxter transcription of the 廣韻.
  const charMc = $derived((headChar?.readings ?? []).filter((r) => r.kind === 'mc').map((r) => r.value))
  // built once per phonetic component: the MC sound-link explanation, or null when MC data is missing.
  function mcLinkFor(c: { ch: string; role: string | null; mc_sound?: string[] }) {
    if (c.role !== 'phonetic') return null
    return mcSoundLink(charMc, c.mc_sound, c.ch)
  }

  // Block B - the bridge: how the same meaning is written DIFFERENTLY in another language (a different
  // glyph or the cross-script form). Everything that isn't a same-glyph definition. Only meaningful
  // once there's a glyph definition to bridge FROM.
  const bridgeRows = $derived(
    isGlyphSearch
      ? allRows
          .filter((r) => !r.synthetic && !(r.kind === 'form' && r.form === head))
          // trusted curated equivalents lead; fuzzy gloss-pivot synonyms follow. Within each, 中/粵/日.
          .sort((a, b) => {
            const ae = a.relation === 'equivalent' ? 0 : 1
            const be = b.relation === 'equivalent' ? 0 : 1
            return ae !== be ? ae - be : VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety)
          })
      : [],
  )

  // English / pinyin search (nothing matches the typed glyph): fall back to a plain results list.
  const listRows = $derived(isGlyphSearch ? [] : allRows)

  // "everyday word": for a single character, the natural multi-character word another language
  // actually writes for this meaning (耳 → 中 耳朵; 朵 → 中 花朵). A Japanese learner sees 耳's bare
  // Chinese gloss but wouldn't know 耳朵 is how it's really said. Backend-derived (relation
  // 'everyday-word'); shown as its own labelled block so it reads as "how you'd actually say it".
  const everydayRows = $derived<Row[]>(
    (entry?.translations ?? [])
      .filter((l) => l.relation === 'everyday-word')
      .map((l) => ({
        id: l.lexeme_id,
        variety: l.variety,
        form: l.headword,
        alt: null,
        formScript: '',
        altScript: '',
        reading: l.reading ?? '',
        glosses: l.glosses,
        relation: 'everyday-word',
        kind: 'equiv' as const,
      }))
      .sort((a, b) => VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety)),
  )

  // significant words in the looked-up entry's OWN meaning — to rank "related" by meaning closeness
  const headGlossWords = $derived.by(() => {
    const s = new Set<string>()
    for (const sn of entry?.senses ?? [])
      for (const w of cleanGloss(sn.gloss_en).toLowerCase().split(/[^a-z]+/)) if (w.length > 2) s.add(w)
    return s
  })
  const glossOverlap = (glosses: string[]): number => {
    let n = 0
    for (const g of glosses)
      for (const w of cleanGloss(g).toLowerCase().split(/[^a-z]+/)) if (w.length > 2 && headGlossWords.has(w)) n++
    return n
  }

  // Block C - "related": same-meaning words in another language linked only by a shared concept
  // (the gloss-pivot + OMW-synset tier, relation 'synonym'). Lower trust than the curated "written
  // differently" bridge, so shown under a clearly hedged heading. Shown for single characters too (駅
  // → 站): the cross-language equivalent word is exactly what a learner wants, even for one glyph.
  // Ordered MEANING-FIRST: the closest-meaning words (most gloss overlap with this entry) lead.
  const relatedRows = $derived<Row[]>(
    entry
      ? (entry?.translations ?? [])
          .filter((l) => l.relation === 'synonym' && l.headword !== head)
          .map((l) => ({
            id: l.lexeme_id,
            variety: l.variety,
            form: l.headword,
            alt: null,
            formScript: '',
            altScript: '',
            reading: l.reading ?? '',
            glosses: l.glosses,
            relation: 'synonym',
            kind: 'equiv' as const,
          }))
          .sort(
            (a, b) =>
              glossOverlap(b.glosses) - glossOverlap(a.glosses) ||
              VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety),
          )
          .slice(0, 12)
      : [],
  )

  // numbered senses for a definition row - the full hit glosses (every sense), cleaned. Identical
  // treatment for every language (no POS on one and not another) so the languages stay co-equal.
  // Collapsed to ~2 lines with a per-row "more" toggle so a long Chinese definition (or a 13-sense
  // kanji) doesn't wall off the page; the toggle only appears when the senses actually overflow 2 lines.
  let expanded = $state(new Set<number>())
  let overflow = $state(new Set<number>())
  // major meanings shown by default. Bare cross-references ("abbr. for 入聲", "variant of X", "used
  // in …") are demoted and hidden behind "show more" — UNLESS a row has no major sense at all (中共 =
  // "abbr. for 中國共產黨" is the only meaning), in which case they ARE the definition and stay visible.
  // expandSenses splits a CC-CEDICT gloss that packs several marker-tagged senses into one ";"-line
  // (加盟 "(of a nation)…; (of an athlete)…") into separate enumerated senses, while leaving plain
  // synonym lists ("I; me; my") as one line.
  function majorSenses(r: Row): string[] {
    const all = expandSenses(r.glosses.map(cleanGloss).filter(Boolean))
    const major = all.filter((g) => !isMinorGloss(g))
    return major.length ? major : all
  }
  function minorSenses(r: Row): string[] {
    const all = expandSenses(r.glosses.map(cleanGloss).filter(Boolean))
    const major = all.filter((g) => !isMinorGloss(g))
    return major.length ? all.filter((g) => isMinorGloss(g)) : []
  }
  function shownSenses(r: Row): string[] {
    return expanded.has(r.id) ? [...majorSenses(r), ...minorSenses(r)] : majorSenses(r)
  }
  function hasMoreSenses(r: Row): boolean {
    return minorSenses(r).length > 0 || overflow.has(r.id)
  }

  function toggleSenses(id: number) {
    const n = new Set(expanded)
    if (n.has(id)) n.delete(id)
    else n.add(id)
    expanded = n
    save()
  }

  // measure whether a senses block exceeds the 2-line clamp (so the "more" toggle shows only when
  // needed). scrollHeight is the full content height even while clamped. We must re-measure after the
  // CJK web font swaps in (Noto loads after first paint and the text can grow past 2 lines) - and the
  // ResizeObserver on the clamped node won't fire then (its box is pinned to max-height), so also hook
  // document.fonts.ready and an rAF. Threshold tracks the CSS max-height (2.9rem) closely so a block
  // that's visibly clipped always gets a toggle (no dead zone).
  // `slack` lets a block run ~1 line past the clamp before we bother hiding anything: clipping just
  // half a second line behind a "show more" is more annoying than the extra line. So we only clamp +
  // fade when content exceeds the clamp by more than slack; up to slack over, it renders in full.
  function clampProbe(node: HTMLElement, opts: { id: number; rem: number; slack?: number }) {
    const { id, rem, slack = 1.5 } = opts
    const measure = () => {
      const limit = parseFloat(getComputedStyle(document.documentElement).fontSize) * (rem + slack)
      const over = node.scrollHeight > limit + 1
      if (over === overflow.has(id)) return
      const n = new Set(overflow)
      if (over) n.add(id)
      else n.delete(id)
      overflow = n
    }
    measure()
    requestAnimationFrame(measure)
    document.fonts?.ready?.then(measure)
    const ro = new ResizeObserver(measure)
    ro.observe(node)
    return { destroy: () => ro.disconnect() }
  }

  // false friends are SAME-glyph words whose meaning diverges (手紙) - they sit co-equally in block A,
  // flagged by a single note. (A different-glyph bridge row is never a false friend - it's just the
  // other language's word.)
  // distinct languages among the same-glyph definition rows (English names: this drives the
  // false-friend sentence below, where 中文/日本語 glyphs would read as words, not labels)
  const defLangs = $derived([...new Set(defRows.map((r) => langName[r.variety]))])
  // flag a false friend only in the clean case: exactly TWO same-glyph rows in TWO languages, with a
  // false-friend relation and NO cognate (shared) meaning. Multi-reading homographs (行 háng/héng/xíng)
  // or words with any shared sense (京都 = Kyoto in both) are not flagged.
  const hasFalseFriend = $derived(
    defRows.length === 2 &&
      defLangs.length === 2 &&
      defRows.some((r) => r.relation === 'false-friend') &&
      !defRows.some((r) => r.relation === 'cognate'),
  )
  const falseFriendLangs = $derived(defLangs.join(' and '))

  // The single character's full Japanese on/kun readings (kana + romaji), shown right on the 日
  // definition row — Chinese readers can't read kana, so each gets its romaji. Capped to JA_CAP with a
  // "+N" toggle so a kanji with a dozen readings doesn't wrap into a wall. (中 pinyin / 粵 jyutping are
  // short and already sit on their rows; there is no separate "readings" section any more.)
  let jaReadOpen = $state(false)
  let yueReadOpen = $state(false)
  // a single character with exactly ONE Japanese row should show the CHARACTER's full on/kun set
  // (志 → シ · こころざ.す · こころざし), not just the one reading of the word-lexeme that happens to
  // exist (こころざし). When a character has several ja words (生: なま/いきる/セイ…) they stay as
  // separate per-word rows instead, so the full list isn't duplicated across them.
  const singleJaRow = $derived(single && defRows.filter((r) => r.variety === 'ja').length === 1)
  const jaReadItems = $derived.by<{ main: string; sub: string; accent: string | null }[]>(() => {
    if (!single || !headChar) return []
    // clean the Kanjidic okurigana markers off the kana, and dedup (あ.う / -あ.う collapse to あう).
    const seen = new Set<string>()
    const mk = (kind: string) =>
      headChar!.readings
        .filter((r) => r.kind === kind && isKana(r.value))
        .map((r) => ({ main: cleanKanaReading(r.value), sub: readingRomaji(kind as 'onyomi' | 'kunyomi', r.value), accent: r.accent ?? null }))
        .filter((it) => it.main !== '' && !seen.has(it.main) && !!seen.add(it.main))
    return [...mk('onyomi'), ...mk('kunyomi')]
  })

  // which languages a character actually belongs to (for the lean breakdown): 中 if it has a Mandarin
  // reading, 粵 jyutping, 日 a kana on/kun reading.
  function charLangs(c: CharInfo): string[] {
    const out: string[] = []
    if (c.readings.some((r) => r.kind === 'pinyin')) out.push('中')
    if (c.readings.some((r) => r.kind === 'jyutping')) out.push('粵')
    if (c.readings.some((r) => (r.kind === 'onyomi' || r.kind === 'kunyomi') && isKana(r.value))) out.push('日')
    return out
  }

  // item 14: a full-sentence explanation of a script change (繁→简 / 旧→新), replacing the bare
  // "PRC simplification" caption. Built from the head character's own variant edges.
  const scriptNote = $derived(
    headChar
      ? scriptChangeNote(head, headChar.variants ?? []) ?? scriptChangeFromForms(headChar.script_forms)
      : null,
  )
  // item 161: the traditional/simplified counterpart of the viewed glyph, if one exists — drives the
  // small two-arrow switch button at the top-right of the header glyph. Tap it to jump to the other
  // script's form (馬 ⇄ 马). Only for a genuine TC/SC pair (not shinjitai-only or z-variants).
  const scCounterpart = $derived(scSwitchTarget(headChar?.script_forms ?? null, head))
  // item: the same TC/SC switch must also work for multi-character WORDS (機場 ⇄ 机场), which have no
  // single-char script_forms. Fall back to the Chinese definition row's own trad/simp pair: if the
  // viewed zh form has a differing counterpart, offer the jump to it. (Single chars use scCounterpart.)
  const switchTarget = $derived.by(() => {
    if (scCounterpart) return scCounterpart
    if (single) return null // a single glyph with no script_forms genuinely has no TC/SC pair
    const zh = defRows.find((r) => r.variety === 'zh')
    if (zh?.alt && zh.form === head && zh.altScript) {
      return { to: zh.alt, label: zh.altScript === 'trad' ? 'traditional' : 'simplified' }
    }
    return null
  })
  // a TRADITIONAL-script Chinese headword must use the TC font stack: the SC-first --han stack leads
  // with Simplified system fonts that can lack traditional-only forms (關), which tofu. The GlyphWiki
  // fallback in Glyph.svelte still backstops genuinely-uncovered glyphs; this just prefers the right
  // native font first. Detect the script from the Chinese definition row for the viewed form.
  const headFont = $derived.by(() => {
    if (headVariety === 'zh') {
      const zh = defRows.find((r) => r.variety === 'zh' && r.form === head)
      if (zh?.formScript === 'trad') return 'var(--han-tc)'
    }
    return hanFont(headVariety)
  })
  // a region-exclusive word (Taiwan-only 計程車, Hong Kong-only …): a small country tag next to the
  // headword, derived from the looked-up lexeme's CC-CEDICT "(Tw)"/"(HK)" markers (primary sense only).
  const regionBadges = $derived(regionTags((entry?.senses ?? []).map((s) => s.gloss_en)))
  // items 17/18: the radical and "rarely used" tags sit on their own line under each language row.
  // "radical" is a character property (bound in every language); "rarely used" is now PER-LANGUAGE,
  // from the per-variety containing-word count (巴 is common in 中 but rare in 日).
  const isRadicalChar = $derived(!!headChar?.is_radical)
  // rarity tag from the MAX word-frequency of words containing this glyph in that language (a real
  // frequency signal; the old containing-word count mislabelled common particles like 嗎/也).
  // Cantonese borrows the Mandarin frequency corpus, so its scores understate core 粵字 — we suppress
  // the tag for 粵 rather than emit wrong labels (until a real Cantonese char-frequency source).
  function rowUsage(variety: Variety): string {
    if (variety === 'yue') return ''
    const f = headChar?.freq_by_variety?.[variety]
    if (f === undefined) return 'rarely used' // no scored word in this language uses the glyph
    if (f < 0.3) return 'rarely used'
    if (f < 0.4) return 'uncommon'
    return ''
  }
  // a single kanji that forms native words via okurigana (乗 → 乗る, 化 → 化ける) is a WORD STEM, not a
  // bound morpheme — its kun readings carry an okurigana split (の.る). Used to avoid mis-tagging the
  // synthetic Japanese row "bound" (item 4: 乗 is effectively a word, unlike 津 which is truly bound).
  const headHasOkurigana = $derived(
    (headChar?.readings ?? []).some((r) => r.kind === 'kunyomi' && r.value.includes('.')),
  )
  // a kanji that is itself a standalone Japanese WORD (本=ほん, 水=みず, 木=き) is NOT a bound morpheme,
  // even with no okurigana — detected by a real same-glyph ja word-lexeme among the rows. Without this,
  // the synthetic Kanjidic row for such a noun-kanji was mislabelled "only in compounds".
  const hasStandaloneJaWord = $derived(
    rows.some((r) => r.variety === 'ja' && r.kind === 'form' && !r.synthetic && r.form === head),
  )
  // bound classification for a row: 'always' (only ever in compounds), 'often' (bound in some senses
  // but free in others, e.g. 日), or null (not bound / a word stem).
  function boundKind(r: Row): 'always' | 'often' | null {
    if (r.synthetic) return headHasOkurigana || hasStandaloneJaWord ? null : 'always'
    if (isAlwaysBound(r.glosses)) return 'always'
    if (isBoundForm(r.glosses)) return 'often'
    return null
  }

  // one compact reading for a component character in the breakdown row — primary pinyin (tone-marked),
  // else jyutping, else the first few kana on/kun. Keeps the row consistent with the word rows.
  function charReading(c: CharInfo): string {
    const p = c.readings.filter((r) => r.kind === 'pinyin').map((r) => r.value)
    if (p.length) return pinyinMarks(p[0])
    const j = c.readings.filter((r) => r.kind === 'jyutping').map((r) => r.value)
    if (j.length) return settings.romanization === 'yale' ? jyutpingToYale(j[0]) : j[0]
    const k = c.readings.filter((r) => (r.kind === 'onyomi' || r.kind === 'kunyomi') && isKana(r.value)).map((r) => r.value)
    return k.slice(0, 3).join('  ')
  }

  // first MEANINGFUL sense only, cleaned - the character breakdown stays lean and never leads with a
  // "surname X" cross-reference when a real meaning exists.
  function firstSense(g: string | null): string {
    const parts = cleanGloss(g ?? '').split(';').map((s) => s.trim()).filter(Boolean)
    return parts.find((p) => !isMinorGloss(p)) ?? parts[0] ?? ''
  }

  // 熟語 - one flat, frequency-ranked list (language tag per row, no per-variety sectioning). Words
  // that use a CROSS-SCRIPT VARIANT of the character (relation 'compound-alt', e.g. 氷-words for 冰)
  // come after the same-glyph words under a "written differently" divider, still frequency-ranked.
  // exclude the character ITSELF from its own "used in" list: a single char (之) can have a standalone
  // word-lexeme in another language that the backend also returns as a "containing word", which read as
  // a confusing self-reference ("only used in compounds" → first compound is the same glyph).
  const compoundList = $derived((entry?.compounds ?? []).filter((l) => l.headword !== head))
  // map the compound LinkLites onto the shared Row shape so the "used in" list renders with the SAME
  // rowItem style as every other entry list (usually-written / written-differently / characters) —
  // one singular row style across the app (item 155). The relation is kept for the variant divider.
  const compoundRows = $derived.by(() => {
    // dedupe by form+variety+reading: the full JMdict ships near-identical sense-lexemes (大陸 "mainland
    // China" + "continent", 大家 ×3) that would otherwise render as duplicate-looking rows.
    const seenc = new Set<string>()
    return compoundList
      .filter((l) => {
        const k = `${l.variety}|${l.headword}|${l.reading ?? ''}`
        if (seenc.has(k)) return false
        seenc.add(k)
        return true
      })
      .map((l) => ({
        id: l.lexeme_id,
        variety: l.variety as Variety,
        form: l.headword,
        alt: null,
        formScript: '',
        altScript: '',
        reading: l.reading ?? '',
        glosses: l.glosses,
        relation: l.relation,
        kind: 'form' as const,
      }))
  })
  let showOrigin = $state(false)
  let showWords = $state(false)
  const wordCount = $derived(compoundList.length)
  // origin: one account per language for the same glyph (中 Sinitic + 日 Japonic, both true). Falls
  // back to the single legacy etymology field if the backend didn't supply per-variety accounts.
  const originAccounts = $derived.by(() => {
    const accs = entry?.origins ?? []
    if (accs.length) return accs
    if (entry?.etymology) return [{ variety: entry.variety, headword: entry.headword, text: entry.etymology }]
    return []
  })

  // everything BELOW the definition is organised into a CJKV-style segmented control: the sections
  // become tabs (only those with content), one panel visible at a time. Applies to single chars and
  // words alike. Order: make-up → related → origin → used-in.
  let activeTab = $state('')
  const tabs = $derived.by<{ key: string; label: string }[]>(() => {
    if (!isGlyphSearch) return []
    const t: { key: string; label: string }[] = []
    // Order: Related → Used in → Origin → Structure (most-used sections first).
    if (everydayRows.length || bridgeRows.length || relatedRows.length)
      t.push({ key: 'related', label: 'Related' })
    if (compoundList.length) t.push({ key: 'words', label: 'Used in' })
    else if (entry?.appears_in.length) t.push({ key: 'words', label: 'Appears in' })
    if (originAccounts.length) t.push({ key: 'origin', label: 'Origin' })
    // a single-character entry always gets Structure; a word gets a "Characters" breakdown only when it
    // has 2+ Han components (one kanji, e.g. あずかり知る → 知, is not a meaningful breakdown).
    if ((single && headChar) || (entry && entry.characters.length >= 2))
      t.push({ key: 'forms', label: single ? 'Structure' : 'Characters' })
    return t
  })
  // keep activeTab valid as content loads (default to the first tab; never stay on a vanished one)
  $effect(() => {
    if (tabs.length && !tabs.some((t) => t.key === activeTab)) activeTab = tabs[0].key
  })
  function setTab(key: string) {
    activeTab = key
    save()
  }
  // re-sort the Related and Used-in lists: 'relevance' = the default order (frequency / relevance),
  // 'language' = grouped 中 → 粵 → 日, 'alphabet' = by reading (A–Z / gojūon). Stable within each key.
  let sortMode = $state<'relevance' | 'language' | 'alphabet'>('relevance')
  // one button cycles the three modes (relevance → language → A–Z → …)
  function cycleSort() {
    sortMode = sortMode === 'relevance' ? 'language' : sortMode === 'language' ? 'alphabet' : 'relevance'
  }
  function sortRows<T extends { variety: Variety }>(rows: T[]): T[] {
    if (sortMode === 'language') {
      return [...rows].sort((a, b) => VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety))
    }
    if (sortMode === 'alphabet') {
      const key = (r: T) =>
        (((r as { reading?: string | null }).reading || (r as { form?: string }).form || '') as string).toLowerCase()
      return [...rows].sort((a, b) => key(a).localeCompare(key(b)))
    }
    return rows
  }

  // which Origin accounts have their deep comparative-cognate paragraphs expanded (keyed by variety)
  let deepOpen = $state<Set<string>>(new Set())
  function toggleDeep(key: string) {
    const n = new Set(deepOpen)
    n.has(key) ? n.delete(key) : n.add(key)
    deepOpen = n
  }

  // Bound form: a morpheme that doesn't stand alone as a word — it only carries meaning inside
  // compounds. CC-CEDICT flags these; instead of leaking the jargon into the prose we show a small
  // tappable "bound" tag whose popup explains it and lists the compounds the character lives in.
  let boundOpen = $state<Row | null>(null)
  // origin jargon (形聲, OC, STEDT…): a plain-English explanation. On desktop it's the button's
  // title= (hover), but hover doesn't exist on touch, so tapping a term opens this small popup too.
  let openTerm = $state<string | null>(null)
  function boundCompounds(r: Row): { lexeme_id: number; headword: string; glosses: string[] }[] {
    // never list the character itself as one of "its" compounds (之 → 之)
    return (entry?.compounds ?? []).filter((l) => l.variety === r.variety && l.headword !== head).slice(0, 30)
  }

  // ── per-word UI state cache (#101): keep panels as left when you click a link and come back ──
  function save() {
    if (!head) return
    uiCache.set(head, {
      expanded: [...expanded],
      showOrigin,
      showWords,
      jaReadOpen,
      yueReadOpen,
      activeTab,
      boundId: boundOpen?.id ?? null,
      ts: Date.now(),
    })
  }
  function setOpen(which: 'origin' | 'words', val: boolean) {
    if (which === 'origin') showOrigin = val
    else showWords = val
    save()
  }
  function openBound(r: Row) {
    boundOpen = r
    save()
  }
  function closeBound() {
    boundOpen = null
    save()
  }
  function toggleJaRead() {
    jaReadOpen = !jaReadOpen
    save()
  }
  function toggleYueRead() {
    yueReadOpen = !yueReadOpen
    save()
  }
  function pivot(q: string) {
    save() // persist the open bound menu so it's still open when we come back
    boundOpen = null
    onsearch(q)
  }
  // restore the cached panel state when this word (re)appears (back/forward nav); a fresh search of a
  // new term gets fresh defaults. Keyed on head so toggling panels doesn't re-trigger a restore.
  let restoredFor = ''
  $effect(() => {
    const h = head
    if (h === restoredFor) return
    restoredFor = h
    // collapse the headword + reading expanders on every word change (per-session UI, not cached)
    headOpen = false
    readOpen = new Set()
    const snap = h ? uiCache.get(h) : undefined
    if (snap && Date.now() - snap.ts < UI_TTL) {
      expanded = new Set(snap.expanded)
      showOrigin = snap.showOrigin
      showWords = snap.showWords
      jaReadOpen = snap.jaReadOpen
      yueReadOpen = snap.yueReadOpen ?? false
      activeTab = snap.activeTab ?? ''
      boundOpen = snap.boundId != null ? allRows.find((r) => r.id === snap.boundId) ?? null : null
    } else {
      expanded = new Set()
      showOrigin = false
      showWords = false
      jaReadOpen = false
      yueReadOpen = false
      activeTab = ''
      boundOpen = null
    }
  })

  // ── readings "show more" (#102): clamp a readings line to ONE line; "+" reveals the rest when it
  // would overflow (and the line stays side-scrollable). Used by BOTH the 日 on/kun list and the 粵
  // jyutping list, so readProbe takes a setter for whichever "over" flag this line drives. ──
  let jaReadOver = $state(false)
  let yueReadOver = $state(false)
  // per-row clamp state for a plain word reading (中 pinyin / 粵 jyutping / 日 word kana): a long
  // multi-syllable reading (中國國民黨革命委員會) would otherwise run off the viewport, so each row's
  // reading clamps to one line with a "+" — keyed by row id since a card has several reading rows.
  let readOver = $state<Set<number>>(new Set())
  let readOpen = $state<Set<number>>(new Set())
  function setReadOver(id: number, v: boolean) {
    if (v === readOver.has(id)) return
    const n = new Set(readOver)
    if (v) n.add(id)
    else n.delete(id)
    readOver = n
  }
  function toggleRead(id: number) {
    const n = new Set(readOpen)
    n.has(id) ? n.delete(id) : n.add(id)
    readOpen = n
  }
  // headword glyph: clamp to one line so a long word (idiom) never grows tall enough to clip into the
  // save/share buttons; a "+" expands it downward instead of shrinking it away (item).
  let headOpen = $state(false)
  let headOver = $state(false)
  function readProbe(node: HTMLElement, setOver: (v: boolean) => void) {
    const measure = () => {
      // only meaningful while clamped (one line); when expanded keep the toggle visible so it can
      // collapse again. Horizontal overflow = the readings would be clipped → show "+".
      if (!node.classList.contains('clamp')) return
      setOver(node.scrollWidth > node.clientWidth + 2)
    }
    measure()
    requestAnimationFrame(measure)
    document.fonts?.ready?.then(measure)
    const ro = new ResizeObserver(measure)
    ro.observe(node)
    return { update: (s: (v: boolean) => void) => { setOver = s }, destroy: () => ro.disconnect() }
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') { if (boundOpen) closeBound(); openTerm = null } }} />

<article class="u">
  <!-- one tappable cross-language row (bridge band + plain results list) -->
  {#snippet rowItem(r: Row)}
    <!-- the ONE shared entry-row style (same as the search results list): glyph left, reading + 中/粵/日
         tag + gloss in the meta column (item: one singular style for every entry list). -->
    <EntryRow
      glyph={r.form}
      font={hanFont(r.variety)}
      lang={langTag(r.variety)}
      reading={dispReading(r.variety, r.reading)}
      tags={[varietyLabel(r.variety)]}
      gloss={briefGloss(r.glosses)}
      onclick={() => onsearch(r.form)}
    />
  {/snippet}

  <!-- the cycling sort control, at the article scope so BOTH the Related and Used-in tabs can render it
       (it was previously nested in the Related block, so Used-in's {@render} threw and showed nothing). -->
  {#snippet sortControl(defLabel: string)}
    <div class="sortrow">
      <button class="sortbtn" onclick={cycleSort}>sort: {sortMode === 'relevance' ? defLabel : sortMode === 'language' ? 'language' : 'A–Z'}</button>
    </div>
  {/snippet}

  <!-- render a row list with a 中/粵/日 divider before each language group (only in language-sort mode,
       where same-variety rows are contiguous). Caller wraps these <li> in <ul class="langs">. -->
  {#snippet langRows(rows: Row[])}
    {#each rows as r, i (r.id)}
      {#if sortMode === 'language' && (i === 0 || rows[i - 1].variety !== r.variety)}<li class="langdiv">{varietyName(r.variety)}</li>{/if}
      {@render rowItem(r)}
    {/each}
  {/snippet}

  {#if isGlyphSearch}
    <!-- Block A - the definition: the typed glyph across every language that writes it, co-equally -->
    <section class="def">
      <div class="glyphrow">
        <h2 class="glyph" class:clamp={!headOpen} style="font-size:{glyphSize}" use:readProbe={(v) => (headOver = v)}><Glyph ch={head} font={headFont} lang={langTag(headVariety)} /></h2>
        {#if headOver}<button class="headmore" onclick={() => (headOpen = !headOpen)} aria-label={headOpen ? 'collapse headword' : 'show full headword'} title={headOpen ? 'collapse' : 'show full word'}>{#if headOpen}<Minus size={18} />{:else}<Plus size={18} />{/if}</button>{/if}
        {#if switchTarget}
          <button class="scswitch" onclick={() => onsearch(switchTarget.to)} title="switch to the {switchTarget.label} form ({switchTarget.to})" aria-label="switch to the {switchTarget.label} form"><ArrowLeftRight size={17} /></button>
        {/if}
        {#if regionBadges.length}
          <!-- small country tag: this word is used only in this region (e.g. Taiwan 計程車) -->
          <span class="regiontags">{#each regionBadges as t}<span class="rtag" title="used mainly in {t}">{t}</span>{/each}</span>
        {/if}
      </div>
      <div class="defs">
        {#each defRows as r (r.id)}
          {@const ss = shownSenses(r)}
          <div class="dl">
            <div class="dlh">
              <span class="dvar">{varietyLabel(r.variety)}</span>
              {#if r.variety === 'zh'}
                <!-- the searched glyph is already the big header above; so the Chinese row shows ONLY
                     the OTHER script's form (search a TC hanzi → "SC 机场"; search SC → "TC 機場"), and
                     nothing at all when the two scripts are identical (山). No repeat, no "TC/SC". -->
                {#if r.alt}<span class="dform"><span class="ftag">{formTag(r.altScript)}</span><Glyph ch={r.alt} font={r.altScript === 'trad' ? 'var(--han-tc)' : 'var(--han)'} lang={r.altScript === 'trad' ? 'zh-Hant' : 'zh-Hans'} /></span>{/if}
              {:else if r.alt}
                <span class="dform"><span class="ftag">{formTag(r.formScript)}</span><Glyph ch={r.form} font={hanFont(r.variety)} lang={langTag(r.variety)} /><span class="fsep">·</span><span class="ftag">{formTag(r.altScript)}</span><Glyph ch={r.alt} font={hanFont(r.variety)} lang={langTag(r.variety)} /></span>
              {:else if r.form !== head}
                <!-- the language writes the same character with a different glyph (Japan: 电 → 電) -->
                <span class="dform"><Glyph ch={r.form} font={hanFont(r.variety)} lang={langTag(r.variety)} /></span>
              {/if}
              <!-- the readings (+ Cantonese + speaker) sit INLINE after the language tag; they grow into
                   the remaining width and wrap there only if genuinely too long (never forced to their
                   own line, which read as the reading "dropping" under the tag). -->
              <span class="drow2">
                {#if r.variety === 'ja' && singleJaRow && jaReadItems.length}
                  <!-- a SYNTHETIC ja row (kanji used only in compounds) shows the character's full on/kun
                       (kana + romaji), clamped to one line with a "+" (and horizontally scrollable). A
                       REAL ja word-row shows its OWN reading instead. The readings are plain text; the
                       single speaker icon plays them — one consistent speech affordance across 中/粵/日. -->
                  <span class="dread dreads" class:clamp={!jaReadOpen} class:faded={jaReadOver && !jaReadOpen} use:readProbe={(v) => (jaReadOver = v)}>{#each jaReadItems as it, i}{@const cells = pitchCells(it.main, it.accent)}{#if i}<span class="rsep">·</span>{/if}<span class="rdg">{#if cells}<span class="pitch" title="pitch accent (Kanjium)">{#each cells as c}<span class="pmora" class:phigh={c.high} class:pdrop={c.drop}>{c.mora}</span>{/each}</span>{:else}{it.main}{/if}{#if it.sub}<span class="rsub">{it.sub}</span>{/if}{#if speakOn}<button class="spk spk-sm" class:speaking={playingKey === 'ja:' + it.main} onclick={() => speak('ja:' + it.main, it.main, 'ja', undefined, it.accent)} aria-label="listen to {it.main}" title="listen"><Volume2 size={13} /></button>{/if}</span>{/each}</span>{#if jaReadOver}<button class="rmore" onclick={toggleJaRead} aria-label={jaReadOpen ? 'show fewer readings' : 'show more readings'}>{#if jaReadOpen}<Minus size={15} />{:else}<Plus size={15} />{/if}</button>{/if}
                {:else if r.reading}
                  {@const cells = r.variety === 'ja' ? pitchCells(r.reading, r.accent) : null}
                  <!-- plain reading text + speaker, wrapped in .rdg so the speaker sits tight to the
                       reading EXACTLY like the 日/粵 per-reading icons (not pushed away by .drow2's gap).
                       For a Japanese kana reading with Kanjium accent data, the reading renders as a
                       monochrome pitch contour (overline over high morae + a downstep tick) instead. -->
                  <span class="dread dreads plainread" class:clamp={!readOpen.has(r.id)} class:faded={readOver.has(r.id) && !readOpen.has(r.id)} use:readProbe={(v) => setReadOver(r.id, v)}><span class="rdg">{#if cells}<span class="pitch" title="pitch accent (Kanjium)">{#each cells as c}<span class="pmora" class:phigh={c.high} class:pdrop={c.drop}>{c.mora}</span>{/each}</span>{:else}{dispReading(r.variety, r.reading)}{/if}{#if speakOn}<button class="spk spk-sm" class:speaking={playingKey === r.variety + ':' + r.reading} onclick={() => speak(r.variety + ':' + r.reading, r.reading, r.variety, r.form, r.accent)} aria-label="listen" title="listen"><Volume2 size={13} /></button>{/if}</span></span>{#if readOver.has(r.id)}<button class="rmore" onclick={() => toggleRead(r.id)} aria-label={readOpen.has(r.id) ? 'collapse reading' : 'show full reading'}>{#if readOpen.has(r.id)}<Minus size={15} />{:else}<Plus size={15} />{/if}</button>{/if}
                {/if}
                {#if r.variety === 'zh' && headJyutList.length && !hasYueDef}<span class="dvar dcanto">粵</span><span class="dread dreads" class:clamp={!yueReadOpen} class:faded={yueReadOver && !yueReadOpen} use:readProbe={(v) => (yueReadOver = v)}>{#each headJyutList as j, i}{#if i}<span class="rsep">·</span>{/if}<span class="rdg">{settings.romanization === 'yale' ? jyutpingToYale(j) : j}{#if speakOn}<button class="spk spk-sm" class:speaking={playingKey === 'yue:' + j} onclick={() => speak('yue:' + j, j, 'yue', r.form)} aria-label="listen to {j}, Cantonese" title="listen (Cantonese)"><Volume2 size={13} /></button>{/if}</span>{/each}</span>{#if yueReadOver}<button class="rmore" onclick={toggleYueRead} aria-label={yueReadOpen ? 'show fewer readings' : 'show more readings'}>{#if yueReadOpen}<Minus size={15} />{:else}<Plus size={15} />{/if}</button>{/if}{/if}
              </span>
            </div>
            {#if boundKind(r) || (soundLoan && r.variety === 'zh') || (single && headChar && (isRadicalChar || rowUsage(r.variety)))}
              <!-- tags (bound, written-for-sound, rarely-used, radical) on their own line, indented under
                   the readings — UNDER the language row, not the header. "rarely used" is for THIS row's
                   language; "radical" is character-wide. The character badges are gated on headChar so
                   they don't FLASH before the entry enriches. -->
              <div class="rtagline">
                {#if boundKind(r) === 'always'}<button class="ltag tappable" onclick={() => openBound(r)} title="only used in compounds, never as a word on its own">only in compounds</button>{:else if boundKind(r) === 'often'}<button class="ltag tappable" onclick={() => openBound(r)} title="bound in some senses; often used in compounds">often in compounds</button>{/if}
                {#if soundLoan && r.variety === 'zh'}<button class="ltag tappable" onclick={() => (openTerm = soundLoanExplain)} title="tap to explain">written for sound</button>{/if}
                {#if single && headChar && isRadicalChar}<span class="ltag rad">radical</span>{/if}
                {#if single && headChar && rowUsage(r.variety)}<span class="ltag">{rowUsage(r.variety)}</span>{/if}
              </div>
            {/if}
            {#if ss.length}
              <ol class="senses" class:clamp={!expanded.has(r.id) && overflow.has(r.id)} use:clampProbe={{ id: r.id, rem: 2.9 }}>
                {#each ss as g}<li><span class="sg">{#each glossParts(g) as p}{#if p.link}<button class="xref" onclick={() => onsearch(p.v)}>{p.v}</button>{:else}{p.v}{/if}{/each}</span></li>{/each}
              </ol>
              {#if hasMoreSenses(r)}
                <button class="more" onclick={() => toggleSenses(r.id)}>{expanded.has(r.id) ? 'show less' : 'show more'}</button>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
      {#if hasFalseFriend}
        <p class="note"><AlertTriangle size={14} /> {head} is written the same in {falseFriendLangs} but means different things.</p>
      {/if}
      {#if glossless}
        <!-- item 17: a component glyph with no dictionary definition of its own (𦘒, 肀) -->
        <p class="cnote">Only used as a component inside other characters, not as a word on its own.</p>
      {/if}
    </section>

    {#if tabs.length}
      <!-- CJKV-style segmented control: everything below the definition lives in these tabs -->
      <div class="seg" role="tablist" aria-label="sections">
        {#each tabs as t, i}{#if i && tabs[i].key !== activeTab && tabs[i - 1].key !== activeTab}<span class="segsep" aria-hidden="true"></span>{/if}<button class="segb" class:on={activeTab === t.key} role="tab" aria-selected={activeTab === t.key} onclick={() => setTab(t.key)}>{t.label}</button>{/each}
      </div>
    {/if}

    {#if activeTab === 'related'}
      {@render sortControl('relevance')}
      {#if sortMode === 'relevance'}
        <!-- band labels explain WHY each group is related; shown only in relevance order (in language /
             A–Z order the rows are globally re-sorted into one list, so the bands no longer hold). -->
        {#if everydayRows.length}
          <!-- the natural everyday word another language writes for this character's meaning (耳 → 耳朵) -->
          <section class="bridge"><div class="blabel">everyday word</div><ul class="langs">{#each everydayRows as r (r.id)}{@render rowItem(r)}{/each}</ul></section>
        {/if}
        {#if bridgeRows.length}
          <!-- the same meaning, written differently elsewhere. Tappable pivots. -->
          <section class="bridge"><div class="blabel">written differently</div><ul class="langs">{#each bridgeRows as r (r.id)}{@render rowItem(r)}{/each}</ul></section>
        {/if}
        {#if relatedRows.length}
          <!-- looser same-concept words in another language (lowest-confidence gloss/synset pivot) -->
          <section class="bridge related"><div class="blabel">related in meaning</div><ul class="langs">{#each relatedRows as r (r.id)}{@render rowItem(r)}{/each}</ul></section>
        {/if}
      {:else}
        <!-- language / A–Z: one globally-sorted list; language order gets 中/粵/日 dividers -->
        <section class="bridge"><ul class="langs">{@render langRows(sortRows([...everydayRows, ...bridgeRows, ...relatedRows]))}</ul></section>
      {/if}
    {/if}

  {:else if listRows.length}
    <!-- English / reading search: a plain results list -->
    <section class="bridge">
      <ul class="langs">
        {#each listRows as r (r.id)}{@render rowItem(r)}{/each}
      </ul>
    </section>
  {/if}

  {#if isGlyphSearch && activeTab === 'forms' && single && headChar}
    <!-- single character: a compact structure line (no repeated glyph), then the words that use it.
         (Readings used to live in their own section here; they're now folded onto the definition rows
         above — 中 pinyin / 粵 jyutping / 日 on·kun — so there's no duplicate "readings" block.) -->
    <section class="struct">
      {#if headChar.is_radical && (headChar.radical_number || headChar.standalone)}
        <!-- a radical's detail (Kangxi number, standalone form). The "radical" badge itself now sits
             left of the language rows above (item 18); here we keep only the explanatory detail. -->
        <p class="radline">
          {#if headChar.radical_number}<span class="dim">Kangxi radical {headChar.radical_number}</span>{/if}
          {#if headChar.standalone}<span class="dim">· written</span> <button class="part" onclick={() => onsearch(headChar.standalone!)} title="look up {headChar.standalone}"><Glyph ch={headChar.standalone} font="var(--han)" /></button> <span class="dim">when standalone</span>{/if}
        </p>
      {/if}
      {#if headChar.script_forms}
        <div class="strip substep">
          <div class="sublabel">across scripts</div>
          <ScriptForms forms={headChar.script_forms} anchor={head} {onsearch} />
          {#if scriptNote}<p class="scriptnote">{scriptNote}</p>{/if}
        </div>
      {/if}
      <!-- only label the composition block when there's ALSO an "across scripts" strip (or radical
           detail) above it; if it's the only thing under Structure, the label is redundant. -->
      {#if (hasRoles || decomp || comp) && (headChar.script_forms || (headChar.is_radical && (headChar.radical_number || headChar.standalone)))}<div class="sublabel substep">what it's made of</div>{/if}
      {#if hasRoles}
        <!-- phono-semantic: which part carries the meaning vs the sound (媽 = 女 meaning + 馬 sound) -->
        <p class="comp">
          {#if comp?.idc}<IdcBox idc={comp.idc} /><span class="dim idcsep">:</span>{/if}
          {#each roleParts as c, i}{#if i}<span class="plus">+</span>{/if}<span class="cpart"><button class="part" onclick={() => onsearch(c.ch)} title="look up {c.ch}"><Glyph ch={c.ch} font="var(--han)" /></button>{#if c.role === 'phonetic' && c.sound}<span class="crole">sound: {pinyinMarks(c.sound)}</span>{:else if c.role === 'semantic' && meaningOf(c.ch)}<span class="crole">meaning: {meaningOf(c.ch)}</span>{:else if meaningOf(c.ch)}<span class="cmean">{meaningOf(c.ch)}</span>{/if}</span>{/each}
        </p>
        <!-- phonological "why": the historical (Middle Chinese) sound link between the character and
             its phonetic component. Stated honestly: a full match, a partial resemblance, or a note
             that the link only holds in the modern reading. -->
        {#each roleParts as c}{@const link = mcLinkFor(c)}{#if link}
          <p class="phonowhy" class:diverged={link.relation === 'diverged'}>
            <span class="mcpair"><span class="mch" title="this character's Middle Chinese reading (廣韻, Baxter)">{head} <b>{charMc[0]}</b></span><span class="dim">·</span><span class="mch" title="{c.ch}'s Middle Chinese reading (廣韻, Baxter)">{c.ch} <b>{link.compMc[0]}</b></span></span>
            <span class="pwnote">{link.note}</span>
          </p>
        {/if}{/each}
      {:else if decomp}
        <p class="comp">
          {#if comp?.idc}<IdcBox idc={comp.idc} /><span class="dim idcsep">:</span>{/if}
          <span class="cpart"><span class="dim">{numWord(decomp.count)} ×</span> <button class="part" onclick={() => onsearch(decomp.base)} title="look up {decomp.base}"><Glyph ch={decomp.base} font="var(--han)" /></button>{#if meaningOf(decomp.base)}<span class="cmean">{meaningOf(decomp.base)}</span>{/if}</span>
        </p>
      {:else if comp}
        <p class="comp">
          {#if comp.idc}<IdcBox idc={comp.idc} /><span class="dim idcsep">:</span>{/if}
          {#each comp.parts as p, i}{#if i}<span class="plus">+</span>{/if}<span class="cpart"><button class="part" onclick={() => onsearch(p.component)} title="look up {p.component}"><Glyph ch={p.component} font="var(--han)" /></button>{#if p.count > 1}<span class="dim">×{p.count}</span>{/if}{#if meaningOf(p.component)}<span class="cmean">{meaningOf(p.component)}</span>{/if}</span>{/each}
        </p>
      {/if}
      {#if headChar.strokes}
        <div class="cln">
          <span class="dim">{headChar.strokes} strokes</span>
        </div>
      {/if}
      {#if headChar.confusables.length}
        <!-- Unihan kSpoofingVariant: glyphs easily MISREAD for this one. A look-alike note, NOT a
             variant/meaning link — kept visually distinct from the "across scripts" strip. -->
        <div class="confus">
          <span class="sublabel">easily confused with</span>
          <div class="confrow">{#each headChar.confusables as cf}<button class="part confbtn" onclick={() => onsearch(cf)} title="look up {cf} (look-alike)"><Glyph ch={cf} font="var(--han)" /></button>{/each}</div>
        </div>
      {/if}
    </section>
  {:else if isGlyphSearch && activeTab === 'forms' && entry && entry.characters.length}
    <!-- jukugo: break the word into its component characters. Same tappable row system as the
         "usually written" / "written differently" bands (one list style across the app), showing the
         languages it lives in, its reading, and one meaning. -->
    <section class="chars">
      <ul class="langs">
        {#each entry.characters as c, i (c.ch)}
          {@const glyph = headChars.length === entry.characters.length ? headChars[i] : c.ch}
          <EntryRow
            glyph={glyph}
            font={hanFont(headVariety)}
            lang={langTag(headVariety)}
            reading={charReading(c)}
            tags={charLangs(c)}
            gloss={firstSense(c.gloss_en)}
            onclick={() => onsearch(glyph)}
          />
        {/each}
      </ul>
    </section>
  {:else if isGlyphSearch && enriching}
    <!-- reserve the structure + words space while the entry loads, so nothing pops in below -->
    <section class="skel" aria-hidden="true">
      <div class="skel-h"></div>
      <div class="skel-line"></div>
      <div class="skel-line w60"></div>
      <div class="skel-h"></div>
      <div class="skel-chips">{#each Array(10) as _}<span class="skel-chip"></span>{/each}</div>
    </section>
  {/if}

  <!-- one etymology paragraph (shared by the core prose and the collapsed deep-cognate block) -->
  {#snippet etySeg(seg: ReturnType<typeof etymologyTokens>[number])}
    <div class="etyseg" class:sub={seg.depth > 0} class:alt={seg.alt} class:ord={seg.ordinal != null} style="--depth:{seg.depth}">
      {#if seg.heading}<div class="etyhead">{seg.heading}</div>{/if}
      <p class="ety">{#if seg.ordinal != null}<span class="etynum">{seg.ordinal}.</span> {/if}{#each seg.tokens as s}{#if s.t === 'ruby'}<ruby><button class="kanji etylink" onclick={() => onsearch(s.base)}>{s.base}</button><rt>{s.rt}</rt></ruby>{:else if s.t === 'recon'}<span class="recon" title={s.title}>{s.v}</span>{:else if s.t === 'abbr'}<button class="term" title={s.title} onclick={() => (openTerm = s.title)}>{s.v}</button>{:else if s.t === 'han'}<button class="kanji etylink" onclick={() => onsearch(s.v)}>{s.v}</button>{:else}{s.v}{/if}{/each}</p>
    </div>
  {/snippet}

  {#snippet etyBody(text: string, key: string)}
    <!-- one account's prose: the core formation first; dense cross-family comparative cognates
         ("STEDT compares…") tuck behind a "show deeper cognates" toggle so the plain origin reads
         first. The deep paragraphs keep their normal brightness (foreign scripts are not dimmed). -->
    {@const segs = etymologyTokens(text)}
    {@const core = segs.filter((s) => !s.deep)}
    {@const deep = segs.filter((s) => s.deep)}
    <div class="etylist">
      {#each core as seg}{@render etySeg(seg)}{/each}
      {#if deep.length}
        <button class="deeptoggle" onclick={() => toggleDeep(key)} aria-expanded={deepOpen.has(key)}>{deepOpen.has(key) ? 'hide deeper cognates' : 'show deeper cognates'}</button>
        {#if deepOpen.has(key)}<div class="etydeep">{#each deep as seg}{@render etySeg(seg)}{/each}</div>{/if}
      {/if}
    </div>
  {/snippet}

  {#if isGlyphSearch && activeTab === 'origin' && originAccounts.length}
    <section class="origin">
      <!-- one account per language: 山's Chinese (Sinitic) AND Japanese (Japonic) origins are both
           true and complementary, so each is labelled by variety instead of showing only one. -->
      {#each originAccounts as acc (acc.variety)}
        <div class="oacc">
          {#if originAccounts.length > 1 || acc.script}
            <div class="olang"><span class="ovar" lang={langTag(acc.variety)} style="font-family:{hanFont(acc.variety)}">{varietyLabel(acc.variety)}</span> <span class="ohw" lang={langTag(acc.variety)} style="font-family:{hanFont(acc.variety)}">{#if acc.script}<span class="ftag">{scriptShort(acc.script)}</span>{/if}{acc.headword}</span></div>
          {/if}
          {#if acc.note}<p class="onote">{acc.note}</p>{/if}
          {@render etyBody(acc.text, acc.variety)}
        </div>
      {/each}
    </section>
  {/if}

  {#if isGlyphSearch && activeTab === 'words' && entry && compoundList.length}
    {@const wlist = sortRows(compoundRows)}
    <section class="words">
      {@render sortControl('frequency')}
      <!-- the SAME rowItem style as every other entry list. In frequency order, cross-script-variant
           words (氷 for 冰) follow under a "written with a variant character" divider. -->
      <ul class="langs">
        {#each wlist as r, i (r.id)}
          {#if sortMode === 'language' && (i === 0 || wlist[i - 1].variety !== r.variety)}
            <li class="langdiv">{varietyName(r.variety)}</li>
          {:else if sortMode === 'relevance' && r.relation === 'compound-alt' && (i === 0 || wlist[i - 1].relation !== 'compound-alt')}
            <li class="wdiv">written with a variant character</li>
          {/if}
          {@render rowItem(r)}
        {/each}
      </ul>
    </section>
  {:else if isGlyphSearch && activeTab === 'words' && entry && entry.appears_in.length}
    <!-- a radical/bound component isn't a morpheme in words; show the CHARACTERS that contain it -->
    <section class="words">
      <div class="chips">
        {#each entry.appears_in as c (c.ch)}
          <!-- rare ext-plane glyphs may render as tofu on devices without the font; the codepoint in
               the tooltip + a subtle marker keep them identifiable instead of a blank box (item 5) -->
          <button class="chip" class:rare={c.rare} onclick={() => onsearch(c.ch)} title={c.rare ? `${c.gloss ?? ''} (U+${c.ch.codePointAt(0)?.toString(16).toUpperCase()})` : (c.gloss ?? '')}><Glyph ch={c.ch} font={hanFont(headVariety)} lang={langTag(headVariety)} /></button>
        {/each}
      </div>
    </section>
  {/if}

  {#if openTerm}
    <!-- term/explainer popup (origin jargon + the "written for sound" chip) — at the component root so
         it opens from any tab, not only the Origin section. -->
    <div class="termpop" role="presentation" onclick={() => (openTerm = null)}>
      <div class="termcard" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
        <p>{openTerm}</p>
        <button class="mclose" onclick={() => (openTerm = null)}>close</button>
      </div>
    </div>
  {/if}

  {#if boundOpen}
    {@const bc = boundCompounds(boundOpen)}
    {@const often = boundKind(boundOpen) === 'often'}
    <div class="mbg" role="presentation" onclick={closeBound}>
      <div class="modal" role="dialog" aria-modal="true" aria-label="bound form" onclick={(e) => e.stopPropagation()}>
        <div class="mh"><span class="mglyph">{boundOpen.form}</span><span class="mtag">{often ? 'often in compounds' : 'bound form'}</span></div>
        <p class="mexp">{often ? 'Used as a word on its own in some senses, but often appears only inside compounds.' : 'Not used as a word on its own; it carries meaning only inside compounds.'}</p>
        {#if bc.length}
          <div class="mlabel">appears in</div>
          <div class="chips">
            {#each bc as l (l.lexeme_id)}<button class="chip" onclick={() => pivot(l.headword)} title={glossLine(l.glosses, 1)}>{l.headword}</button>{/each}
          </div>
        {/if}
        <button class="mclose" onclick={closeBound}>close</button>
      </div>
    </div>
  {/if}
</article>

<style>
  .u { display: flex; flex-direction: column; }

  /* Block A - the definition: the typed glyph (shared form, shown once), then every language that
     writes it, co-equally. No language is the hero; the glyph is. */
  /* matches .bridge so def→next-heading spacing is identical whether a bridge band follows or not
     (margins don't collapse inside the flex column, so keep both sides small + let h3's top margin lead) */
  .def { margin-bottom: 1rem; }
  /* leave a clear gutter on the right so even a wrapped long headword never runs under the
     save/share buttons that overlap the top-right corner of the card (item 3). */
  .glyphrow { display: flex; align-items: flex-start; gap: 0.5rem; padding-right: 5.6rem; }
  .glyph { font-family: var(--han); font-size: clamp(2.8rem, 14vw, 3.8rem); line-height: 1.05; margin: 0 0 1.1rem; font-weight: 500; overflow-wrap: anywhere; min-width: 0; }
  /* clamped: the headword stays ONE line (never grows tall into the save/share buttons); the "+" un-
     clamps it to wrap downward. */
  .glyph.clamp { white-space: nowrap; overflow: hidden; }
  .headmore { align-self: flex-start; margin-top: 0.5rem; flex: none; display: inline-flex; padding: 0.15rem; background: none; border: none; color: var(--muted); }
  .headmore:hover { color: var(--text); background: none; }
  /* tiny two-arrow switch to the TC/SC counterpart, top-right of the header glyph (item 161) */
  /* just the two-arrow icon, no box around it (item) */
  .scswitch { display: inline-flex; align-items: center; justify-content: center; margin-top: 0.45rem; padding: 0.15rem; color: var(--muted); background: none; border: none; }
  .scswitch:hover { color: var(--text); background: none; }
  /* small country tag(s) for a region-exclusive word (Taiwan/Hong Kong) — sits up by the headword */
  .regiontags { display: inline-flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.55rem; }
  .rtag { font-family: var(--mono); font-size: 0.58rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); line-height: 1.5; }
  .defs { display: flex; flex-direction: column; gap: 1.1rem; }
  .dlh { display: flex; align-items: baseline; gap: 0.7rem; flex-wrap: wrap; }
  /* the language leads (it's the heading of the definition); the reading is secondary */
  .dvar { font-family: var(--han); font-size: 1.1rem; color: var(--text); font-weight: 500; letter-spacing: 0.02em; }
  /* Cantonese tag appended to the Chinese row (中 ěr · 粵 ji5) when the glyph is shared */
  .dcanto { margin-left: 0.5rem; color: var(--muted); font-weight: 400; }
  .dform { font-family: var(--han); font-size: 1.15rem; }
  .dform .ftag { font-family: var(--mono); font-size: 0.7rem; color: var(--muted); margin-right: 0.18rem; vertical-align: 0.35em; }
  .dform .fsep { color: var(--faint); margin: 0 0.18rem; }
  /* the reading group sits inline after the language tag and GROWS to fill the rest of the row, so the
     ja on/kun readings line (.dreads, flex:1 1 0) has a bounded width and can clamp to one line + reveal
     its "+" toggle. (Without flex-grow here .drow2 sized to its content, so .dreads never overflowed and
     the multi-reading toggle silently disappeared.) Readings wrap WITHIN this box, never onto their own
     line under the tag. */
  .drow2 { display: inline-flex; align-items: baseline; gap: 0.7rem; min-width: 0; flex: 1 1 0; }
  .dread { font-family: var(--mono); font-size: 0.9rem; color: var(--muted); }
  /* Japanese pitch-accent contour (Kanjium): a subtle MONOCHROME overline over the high morae with a
     downstep tick where the pitch falls. No accent colour — the app is strictly monochrome; the line
     uses currentColor at low opacity so it reads as a quiet annotation, not a second reading. */
  .pitch { display: inline-flex; }
  .pitch .pmora { position: relative; padding-top: 1px; }
  /* high mora: an overline (the "high plateau"); kept thin and faint so it doesn't shout. */
  .pitch .pmora.phigh { box-shadow: inset 0 1px 0 0 color-mix(in srgb, currentColor 55%, transparent); }
  /* downstep: a short vertical tick at the trailing edge of the last high mora (the fall). */
  .pitch .pmora.pdrop::after {
    content: ''; position: absolute; top: 0; right: -0.5px; height: 0.5em; width: 1px;
    background: color-mix(in srgb, currentColor 55%, transparent);
  }
  /* tight reading separator (the old "  ·  " ate too much space) + romaji gloss + "+N more" toggle */
  .dread .rsep { color: var(--faint); margin: 0 0.28rem; }
  /* "+" / "−" toggle, sized to match the readings so it reads as part of the line, not a tiny tag */
  .rmore { background: none; border: none; padding: 0 0.25rem; font-family: var(--mono); font-size: 0.95rem; line-height: 1; color: var(--text); cursor: pointer; flex: none; }
  .rmore:hover { color: var(--hi); background: none; }
  .senses { margin: 0.5rem 0 0; padding: 0; list-style: none; counter-reset: s; display: flex; flex-direction: column; gap: 0.35rem; }
  /* collapsed: clip to ~2 lines and fade the cut, so a long definition doesn't wall off the page */
  .senses.clamp { max-height: 2.9rem; overflow: hidden; -webkit-mask-image: linear-gradient(to bottom, #000 74%, transparent); mask-image: linear-gradient(to bottom, #000 74%, transparent); }
  .senses li { position: relative; padding-left: 1.5rem; font-size: 1rem; line-height: 1.45; color: var(--text); counter-increment: s; }
  /* always number senses — including a single-sense definition — so "1." reads as a definition, not
     as loose text bumping against the language tag. */
  .senses li::before { content: counter(s) '.'; position: absolute; left: 0; top: 0.05rem; font-family: var(--mono); font-size: 0.78rem; color: var(--faint); }
  .more { background: none; border: none; padding: 0.3rem 0; margin-top: 0.1rem; font-family: var(--mono); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }
  /* tappable cross-reference target inside a gloss ("variant of 著" → jump to 著) */
  .xref { font-family: var(--han); color: var(--text); background: none; border: none; padding: 0; font: inherit; text-decoration: underline; text-underline-offset: 2px; cursor: pointer; }
  .xref:hover { color: var(--hi); background: none; }
  .more:hover { color: var(--text); background: none; }

  /* "bound" tag — a bound morpheme (only used in compounds); taps open an explainer + its compounds */
  /* item 16: make the "bound" tag clearly visible (it marks a morpheme that only lives in compounds) */

  /* bound-form popup — minimal monochrome dialog */
  .mbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); backdrop-filter: blur(10px) saturate(1.4); -webkit-backdrop-filter: blur(10px) saturate(1.4); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 50; }
  .modal { width: min(28rem, 100%); max-height: 80vh; overflow-y: auto; background: var(--surface-2); border: 0.5px solid var(--border-strong); border-radius: 16px; box-shadow: 0 12px 40px -12px rgba(0, 0, 0, 0.7); padding: 1.2rem 1.2rem 1rem; }
  .mh { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.6rem; }
  .mglyph { font-family: var(--han); font-size: 1.9rem; line-height: 1; }
  .mtag { font-family: var(--mono); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }
  .mexp { margin: 0 0 0.9rem; font-size: 0.92rem; line-height: 1.5; color: var(--text); }
  .mlabel { font-family: var(--mono); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--faint); margin-bottom: 0.5rem; }
  .modal .chips { margin-bottom: 0.4rem; }
  .mclose { display: block; margin-top: 0.9rem; margin-left: auto; font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); background: none; border: none; padding: 0.3rem 0; cursor: pointer; }
  .mclose:hover { color: var(--text); background: none; }

  /* Block B - the bridge: the same meaning written differently elsewhere. Tappable rows. */
  .bridge { margin-bottom: 0.6rem; }
  /* the bottom "related" band gets breathing room from origin / used-in above it */
  .bridge.related { margin-top: 1.4rem; }
  /* entry lists (bridges, used-in, characters) — the rows themselves are rendered by EntryRow, which
     owns the row layout and the hairline separators; this just resets the <ul>. */
  .langs { list-style: none; margin: 0; padding: 0; }
  .strip { margin-top: 0.5rem; }
  /* item: visually separate the structure sub-sections (forms/simplification vs composition/why) with
     a faint label and a hairline rule, so they don't read as one undifferentiated run. */
  .sublabel { font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin-bottom: 0.45rem; }
  .substep { margin-top: 1rem; padding-top: 0.9rem; border-top: 1px solid var(--border); }
  /* the first sub-section (the forms strip) follows the heading directly — no rule above it */
  .substep:first-of-type { margin-top: 0.6rem; padding-top: 0; border-top: none; }
  .note { color: var(--faint); font-size: 0.82rem; margin: 0.5rem 0 0; line-height: 1.5; display: flex; align-items: flex-start; gap: 0.4rem; }
  /* align the warning triangle with the cap of the first text line (was sitting a touch low) */
  .note :global(svg) { flex: none; margin-top: 0.02rem; color: var(--muted); }

  /* CJKV-style segmented control for the below-definition sections */
  /* Hybrid: underline tabs (not a segmented pill) — a hairline rule with a white indicator. */
  /* overflow-x:auto alone coerces overflow-y to auto too (CSS spec), which let the tab strip scroll
     vertically; pin overflow-y:hidden so it only ever scrolls horizontally when tabs don't fit. */
  .seg { display: flex; align-items: stretch; gap: 1.5rem; border-bottom: 0.5px solid var(--border); margin: 1.5rem 0 0.2rem; overflow-x: auto; overflow-y: hidden; scrollbar-width: none; }
  .seg::-webkit-scrollbar { display: none; }
  .segb { flex: 0 0 auto; font-family: var(--sans); font-size: 0.95rem; color: var(--muted); background: none; border: none; padding: 0.7rem 0; white-space: nowrap; cursor: pointer; position: relative; }
  .segb:hover:not(.on) { color: var(--text); }
  .segb.on { color: var(--text); font-weight: 600; }
  .segb.on::after { content: ""; position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: var(--text); border-radius: 2px; }
  .segsep { display: none; }
  /* a single toggle that re-sorts the Related / Used-in lists (default ⇄ by language) */
  .sortrow { display: flex; align-items: center; justify-content: flex-end; gap: 0.5rem; margin: 0.7rem 0 0.2rem; }
  /* one cycling pill, same outlined-pill style as the app's other small buttons */
  .sortbtn { font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--faint); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.22rem 0.62rem; cursor: pointer; }
  .sortbtn:hover { color: var(--text); border-color: var(--border-strong); }
  /* band label above each Related group ("everyday word" / "written differently" / "related") */
  .blabel { font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin: 0 0 0.4rem; }
  .dim { color: var(--faint); }

  /* jukugo component characters now reuse the shared .langs/.lang row system (see "written
     differently"); no bespoke character-row styles needed. */

  /* romaji gloss under each Japanese reading (kana is opaque to Chinese readers) */
  .rsub { color: var(--faint); margin-left: 0.2rem; font-size: 0.82em; }
  /* 日 readings stay inline with the 日 label (never evict it to its own line); when the on/kun list
     is long they clip to one line and a "+" reveals the rest (expanded → wraps). */
  /* flex-basis 0 so the readings never trigger a wrap of the header (which would push the 日 label
     to its own line); it grows into the space after the label and clips to one line when long. */
  .dreads { flex: 1 1 0; min-width: 0; line-height: 1.5; }
  /* clamped to one line, but horizontally SCROLLABLE — you can either swipe the readings right to see
     more, or tap "+" to expand them onto multiple lines (item: both open and scroll). */
  .dreads.clamp { white-space: nowrap; overflow-x: auto; overflow-y: hidden; -webkit-overflow-scrolling: touch; scrollbar-width: none; }
  .dreads.clamp::-webkit-scrollbar { display: none; }
  /* fade the last renderable character into nothing (like a "show more" cue) instead of a hard cut —
     only when the readings actually overflow */
  .dreads.faded { -webkit-mask-image: linear-gradient(to right, #000 80%, transparent); mask-image: linear-gradient(to right, #000 80%, transparent); }
  /* a single long plain reading (multi-syllable pinyin/jyutping) WRAPS when expanded rather than
     overflowing — its .rdg is otherwise nowrap (which keeps the speaker tight while clamped). */
  .plainread:not(.clamp) .rdg { white-space: normal; overflow-wrap: anywhere; }
  /* radical line (#16) + usage badge (#17) + script-strip caption (#7) */
  .radline { display: flex; align-items: center; flex-wrap: wrap; gap: 0.4rem; margin: 0 0 0.55rem; font-size: 0.9rem; }
  .radline .part { font-size: 1.15rem; }
  /* item 14: the script-change explanation sentence (replaces the bare "PRC simplification" caption) */
  .scriptnote { font-size: 0.85rem; color: var(--muted); line-height: 1.55; margin: 0.5rem 0 0; }
  .cnote { font-size: 0.9rem; color: var(--muted); font-style: italic; line-height: 1.55; margin: 0.6rem 0 0; }
  /* items 17/18: radical / rarely-used / bound tags on their own line below the readings, indented one
     level so they sit under the language kanji rather than crowding the header row. */
  /* speaker (Web Speech API) button on each definition row */
  .spk { display: inline-flex; align-items: center; justify-content: center; background: none; border: none; color: var(--faint); padding: 0.1rem 0.2rem; border-radius: var(--r); align-self: center; }
  .spk:hover { color: var(--text); background: var(--surface); }
  /* lit up while this reading is actually sounding (cleared when playback ends) */
  .spk.speaking { color: var(--text); }
  @media (prefers-reduced-motion: no-preference) { .spk.speaking { animation: spkpulse 0.9s ease-in-out infinite; } }
  @keyframes spkpulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }
  /* a per-reading speaker for the Japanese on/kun list, so each reading can be heard individually */
  .rdg { white-space: nowrap; }
  .spk-sm { padding: 0 0.1rem; vertical-align: -0.15em; margin-left: 0.1rem; }
  .rtagline { display: flex; flex-wrap: wrap; gap: 0.3rem; align-items: center; margin: 0.2rem 0 0; padding-left: 1.6rem; }
  /* one unified style for every small row tag: radical, rarely used, uncommon, only/often in compounds */
  /* soft filled pill — noticeable (uncommon / only-in-compounds shouldn't be missed) without the harsh
     outlined box used on row variety tags */
  .ltag { font-family: var(--mono); font-size: 0.56rem; text-transform: uppercase; letter-spacing: 0.07em; color: var(--muted); background: var(--surface-2); border: none; border-radius: 999px; padding: 0.12rem 0.5rem; line-height: 1.5; }
  .ltag.rad { color: var(--text); }
  .ltag.tappable { cursor: pointer; }
  .ltag.tappable:hover { color: var(--text); background: var(--border-strong); }
  /* per-language origin account label (中 山 / 日 山) */
  .oacc { margin-top: 1rem; }
  .oacc:first-of-type { margin-top: 0; }
  .olang { display: flex; align-items: baseline; gap: 0.4rem; margin-bottom: 0.25rem; }
  /* brighter origin header, but keep a hierarchy (not all one colour): the hanzi being explained is
     full-ink white, the language label + TC/SC tag sit a tier down at muted — both brighter than the
     old dim faint, and visibly separated from the headword glyph. */
  .olang .ovar { font-size: 0.95rem; color: var(--muted); }
  .olang .ohw { font-size: 0.95rem; color: var(--text); }
  /* item 15: traditional/simplified tag + merge-clarifying note on an origin account */
  /* same TC/SC tag the definition rows use, reused before the origin headword (item 152) */
  .olang .ohw .ftag { font-family: var(--mono); font-size: 0.7rem; color: var(--muted); margin-right: 0.18rem; vertical-align: 0.2em; }
  .onote { font-size: 0.85rem; color: var(--faint); font-style: italic; margin: 0 0 0.4rem; line-height: 1.5; }
  /* structure block - composition (what parts make it up, e.g. 森 = three 木) + a quiet stroke count */
  .cln { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; margin-top: 0.5rem; font-size: 0.8rem; }
  /* English label + Chinese glyph kept close in size so the line reads as one phrase (was 0.82rem vs
     1.6rem — too far apart). The component glyph leads only slightly. */
  /* item 11 + wrap fix: flow components as inline text so each glyph stays with its own gloss and
     wraps as a unit (the gloss never drops to its own line right after the kanji). Inline flow also
     baseline-aligns the larger kanji with their latin meanings. */
  /* flex + align-items:center so the IDC box, the colon, and the part glosses sit on one vertical
     centre line — the latin gloss text no longer rides high above the boxes. */
  .comp { display: flex; flex-wrap: wrap; align-items: center; gap: 0.2rem 0.1rem; line-height: 1.6; margin: 0.6rem 0 0; }
  .comp :global(svg) { vertical-align: middle; }
  /* each component (glyph + its gloss + role) flows as one inline unit, so the gloss never drops to a
     line of its own straight after the glyph; the unit only wraps when the line is genuinely full. */
  .cpart { display: inline; }
  .comp .part { white-space: nowrap; }
  .part { font-family: var(--han); color: var(--text); background: none; border: none; padding: 0 0.1rem; font-size: 1.25rem; line-height: 1; }
  .part:hover { color: var(--hi); background: none; }
  /* confusable look-alikes row (kSpoofingVariant) */
  .confus { margin-top: 1rem; padding-top: 0.9rem; border-top: 1px solid var(--border); }
  .confrow { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .confbtn { font-size: 1.5rem; padding: 0.1rem 0.25rem; border: 1px solid var(--border); border-radius: var(--r); color: var(--muted); }
  .confbtn:hover { color: var(--text); border-color: var(--border-strong); }
  .comp .dim { font-size: 0.95rem; }
  .plus { color: var(--faint); font-family: var(--mono); margin: 0 0.35rem; }
  /* a component's meaning, e.g. 木 (tree) — the "explain the parts" layer */
  .cmean { color: var(--muted); font-size: 0.9rem; margin-left: -0.05rem; }
  .cmean::before { content: '('; }
  .cmean::after { content: ')'; }
  /* role-labelled part gloss: "meaning: woman, girl" / "sound: mǎ" as plain text (no chip, no parens) */
  .crole { color: var(--muted); font-size: 0.9rem; margin-left: 0.3rem; }

  /* phonological "why": the historical (Middle Chinese) sound link char ↔ phonetic component */
  .phonowhy { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.3rem 0.7rem; margin: 0.4rem 0 0; font-size: 0.85rem; line-height: 1.55; }
  .mcpair { display: inline-flex; align-items: baseline; gap: 0.5rem; flex-wrap: wrap; }
  .mch { color: var(--muted); }
  .mch b { font-family: var(--mono); font-weight: 600; color: var(--text); letter-spacing: 0.01em; }
  .pwnote { color: var(--muted); }
  .phonowhy.diverged .pwnote { color: var(--faint); }

  /* words: a tab panel; grouped by language with breathing room */
  .words { margin-top: 0.4rem; }
  /* divider before the cross-script-variant words (氷 for 冰), inside the shared .langs list (item 155) */
  .wdiv { list-style: none; font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin: 0.7rem 0 0.3rem; padding-top: 0.5rem; border-top: 1px solid var(--border); }
  .langdiv { list-style: none; font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin: 0.7rem 0 0.3rem; padding-top: 0.5rem; border-top: 1px solid var(--border); }
  .langdiv:first-child { margin-top: 0; padding-top: 0; border-top: none; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.35rem; font-family: var(--han); font-size: 1.05rem; padding: 0.25rem 0.55rem; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r); max-width: 14em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip.rare { border-style: dashed; color: var(--muted); }
  .chip:hover { border-color: var(--border-strong); }

  .origin { margin-top: 0.4rem; }
  .etylist { margin-top: 0.5rem; }
  /* "show deeper cognates" toggle + the collapsed deep block (kept at normal brightness) */
  .deeptoggle { display: inline-flex; background: none; border: none; padding: 0.3rem 0; margin-top: 0.6rem; font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--muted); }
  .deeptoggle:hover { color: var(--text); background: none; }
  .etydeep { margin-top: 0.2rem; padding-top: 0.3rem; border-top: 1px solid var(--border); }
  /* one flowing account: plain stacked paragraphs, no dividing rule, no fake numbering */
  .etyseg { margin-top: 0.7rem; }
  .etyseg:first-child { margin-top: 0; }
  /* a Wiktionary "*"/"**" sub-point: indent by its depth and mark with a quiet bullet */
  .etyseg.sub { margin-top: 0.25rem; padding-left: calc(0.7rem + (var(--depth) - 1) * 0.9rem); }
  .etyseg.sub .ety { position: relative; }
  .etyseg.sub .ety::before { content: '‣'; position: absolute; left: -0.7rem; color: var(--faint); }
  .etyhead { font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin-bottom: 0.2rem; }
  .ety { font-size: 0.95rem; color: var(--muted); line-height: 1.9; margin: 0; }
  .ety ruby { font-family: var(--han); }
  .ety rt { font-size: 0.55em; color: var(--faint); font-family: var(--han); }
  .ety .kanji { background: none; border: none; padding: 0; font: inherit; color: var(--text); font-family: var(--han); }
  .ety .kanji:hover { text-decoration: underline; }
  /* clickable hanzi in origin prose: a thin SOLID underline — distinct from the DOTTED underline that
     marks abbreviation/jargon terms (.term), so the two reading cues don't get confused (item 156). */
  .ety .etylink { text-decoration: underline solid; text-decoration-thickness: 1px; text-underline-offset: 2px; text-decoration-color: var(--border-strong); background: none; padding: 0; }
  .ety .etylink:hover { text-decoration-color: var(--text); color: var(--hi); background: none; }
  /* item 19: numbered ("#") Wiktionary list items */
  .etyseg.ord { margin-top: 0.3rem; }
  .ety .etynum { font-family: var(--mono); font-size: 0.8em; color: var(--faint); }
  /* item 10: a stacked alternative theory is set off so competing accounts don't read as one run-on */
  .etyseg.alt { margin-top: 1.1rem; padding-top: 0.8rem; border-top: 1px solid var(--border); }
  /* jargon term: dotted-underline like a glossary word. Desktop gets the title= hover; tap opens the
     popup below (hover doesn't exist on touch, which is why the tooltip "didn't work on mobile"). */
  .ety .term { color: var(--text); text-decoration: underline dotted; text-underline-offset: 2px; cursor: help; background: none; border: none; padding: 0; font: inherit; }
  .ety .term:hover { color: var(--hi); background: none; }
  /* phonological reconstructions de-emphasised so the narrative reads first */
  .ety .recon { font-size: 0.78em; color: var(--faint); font-family: var(--mono); }
  .ety .recon[title] { cursor: help; }

  /* tapped-term explanation — a small centred card (mobile-friendly; no hover needed) */
  .termpop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); backdrop-filter: blur(10px) saturate(1.4); -webkit-backdrop-filter: blur(10px) saturate(1.4); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 50; }
  .termcard { width: min(24rem, 100%); background: var(--surface-2); border: 0.5px solid var(--border-strong); border-radius: 16px; box-shadow: 0 12px 40px -12px rgba(0, 0, 0, 0.7); padding: 1.1rem 1.1rem 0.9rem; }
  .termcard p { margin: 0; font-size: 0.95rem; line-height: 1.5; color: var(--text); }

  /* loading skeleton - reserves the lower sections' space so they don't pop in */
  .skel { margin-top: 1.4rem; }
  .skel-h { height: 0.7rem; width: 5rem; border-radius: 4px; background: var(--surface-2); margin: 1rem 0 0.7rem; }
  .skel-line { height: 0.95rem; border-radius: 4px; background: var(--surface); margin-bottom: 0.5rem; }
  .skel-line.w60 { width: 60%; }
  .skel-chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .skel-chip { height: 1.8rem; width: 3.4rem; border-radius: var(--r); background: var(--surface); }
  .skel-h, .skel-line, .skel-chip { animation: skelpulse 1.3s ease-in-out infinite; }
  @keyframes skelpulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }
</style>
