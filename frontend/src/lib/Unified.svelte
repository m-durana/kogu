<script lang="ts">
  import type { CharInfo, Entry, Hit, ReadingKV, Variety } from './types'
  import { primaryForm, varietyLabel, pinyinMarks, cleanGloss, glossLine, briefGloss, meaningfulGlossCount, isMinorGloss, formTag, glossParts, isBoundForm, describeIds, numWord, etymologyTokens } from './display'
  import ScriptForms from './ScriptForms.svelte'
  import { AlertTriangle } from '@lucide/svelte'
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
        glosses: l.glosses,
        relation: l.relation,
        kind: 'equiv',
      })
    }
    return deduped.sort((a, b) => VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety))
  })

  // the headword glyph: what the user looked up
  const head = $derived(anchor || rows[0]?.form || '')

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
  const isKana = (s: string) => /[぀-ヿ]/.test(s)
  // languages this word is actually represented in by a REAL word-lexeme (its same-glyph form rows) -
  // gates the structure readings so a 粵-only word shows jyutping, not a nominal Mandarin pinyin.
  const wordVarieties = $derived(new Set(rows.filter((r) => r.kind === 'form').map((r) => r.variety)))

  // A single Han character can be a genuine word in a language WITHOUT a standalone word-lexeme - e.g.
  // 津 (harbor) is a real kanji (シン/つ) but Japanese only uses it inside compounds, so there is no
  // ja lexeme and it would wrongly show as Chinese-only. Kanjidic kana on/kun is a reliable "used in
  // Japanese" signal, so we synthesize a co-equal 日本語 definition row from the character's own data.
  // (冇 has no on/kun → no synthetic row; its nominal Mandarin pinyin stays suppressed.)
  const synthJaRow = $derived.by<Row | null>(() => {
    if (!single || !headChar) return null
    const on = headChar.readings.filter((r) => r.kind === 'onyomi' && isKana(r.value)).map((r) => r.value)
    const kun = headChar.readings.filter((r) => r.kind === 'kunyomi' && isKana(r.value)).map((r) => r.value)
    if (!on.length && !kun.length) return null
    const gloss = headChar.gloss_ja || headChar.gloss_en || ''
    if (!gloss) return null
    // the Japanese form is the shinjitai if one exists, else the orthodox (traditional) glyph - NOT
    // necessarily what was typed: Japan writes 電 / 陝, never the PRC 电 / 陕. It's still the SAME
    // character, so this row is a co-equal block-A definition (it just displays Japan's glyph).
    const sf = headChar.script_forms
    const jaForm = sf?.branches.find((b) => b.script.includes('shinjitai'))?.form ?? sf?.orthodox ?? head
    // only skip if Japan already has a real lexeme for the SAME glyph (don't be fooled by a
    // different-glyph ja cognate in same_form, e.g. 電's 稲妻 - that's a bridge, not 電 itself).
    if (rows.some((r) => r.kind === 'form' && r.variety === 'ja' && r.form === jaForm)) return null
    return {
      id: -(head.codePointAt(0) ?? 1) - 1,
      variety: 'ja',
      form: jaForm,
      alt: null,
      formScript: '',
      altScript: '',
      reading: [on.join(' '), kun.join(' ')].filter(Boolean).join('    '),
      glosses: [gloss],
      relation: 'self',
      kind: 'form',
      synthetic: true,
    }
  })
  const allRows = $derived(synthJaRow ? [...rows, synthJaRow] : rows)

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

  // Cantonese shares the Han script: a single character written 中 is almost always written and
  // understood the same in 粵, differing only in pronunciation. So when there's no SEPARATE Cantonese
  // row (which would signal a Cantonese-specific word/meaning, e.g. 係 hai6 / 乜), we surface the
  // character's jyutping right on the Chinese row — 中 ěr · 粵 ji5 — instead of burying it below.
  const headJyut = $derived.by(() =>
    single && headChar
      ? headChar.readings.filter((r) => r.kind === 'jyutping').map((r) => r.value).join('  ')
      : '',
  )
  const hasYueDef = $derived(defRows.some((r) => r.variety === 'yue'))
  // single character's composition (what parts make it up, with structure kept): 森 = three 木
  const comp = $derived(single && headChar ? describeIds(headChar.ids, head) : null)
  // recursive "N copies of one base" decomposition from the backend (森 → 木 ×3), preferred over the
  // shallow flat parts when present
  const decomp = $derived(single && headChar ? headChar.decomp : null)
  // component → meaning, so the structure section explains the parts (女 "woman", 木 "tree")
  const compGloss = $derived(new Map((headChar?.components ?? []).map((c) => [c.ch, c.gloss])))
  const meaningOf = (ch: string) => firstSense(compGloss.get(ch) ?? '')

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

  // numbered senses for a definition row - the full hit glosses (every sense), cleaned. Identical
  // treatment for every language (no POS on one and not another) so the languages stay co-equal.
  // Collapsed to ~2 lines with a per-row "more" toggle so a long Chinese definition (or a 13-sense
  // kanji) doesn't wall off the page; the toggle only appears when the senses actually overflow 2 lines.
  let expanded = $state(new Set<number>())
  let overflow = $state(new Set<number>())
  function senseList(r: Row): string[] {
    const all = r.glosses.map(cleanGloss).filter(Boolean)
    // real meanings lead; "surname X" / "variant of" / "used in" cross-refs sink to the end (stable)
    return [...all.filter((g) => !isMinorGloss(g)), ...all.filter((g) => isMinorGloss(g))]
  }


  function toggleSenses(id: number) {
    const n = new Set(expanded)
    if (n.has(id)) n.delete(id)
    else n.add(id)
    expanded = n
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

  // The character's readings, grouped by language for the readings line: 中 pinyin, 粵 jyutping (kept
  // per request), 日 on'yomi then kun'yomi each as kana + romaji (Chinese readers can't read kana).
  // Shows ALL the character's readings - it's the pronunciation reference, kept on the entry.
  type RItem = { main: string; sub?: string }
  type RGroup = { vh: string; items: RItem[] }
  function charReadingGroups(c: CharInfo): RGroup[] {
    const g: RGroup[] = []
    const pinyin = c.readings.filter((r) => r.kind === 'pinyin').map((r) => r.value)
    if (pinyin.length) g.push({ vh: '中', items: pinyin.map((v) => ({ main: v })) })
    const jyut = c.readings.filter((r) => r.kind === 'jyutping').map((r) => r.value)
    if (jyut.length) g.push({ vh: '粵', items: jyut.map((v) => ({ main: v })) })
    const on = c.readings.filter((r) => r.kind === 'onyomi').map((r) => r.value).filter(isKana)
    const kun = c.readings.filter((r) => r.kind === 'kunyomi').map((r) => r.value).filter(isKana)
    const ja: RItem[] = [
      ...on.map((v) => ({ main: v, sub: readingRomaji('onyomi', v) })),
      ...kun.map((v) => ({ main: v, sub: readingRomaji('kunyomi', v) })),
    ]
    if (ja.length) g.push({ vh: '日', items: ja })
    return g
  }
  const READINGS_ID = -1 // clamp/expand key for the readings line (distinct from any lexeme id)

  // which languages a character actually belongs to (for the lean breakdown): 中 if it has a Mandarin
  // reading, 粵 jyutping, 日 a kana on/kun reading.
  function charLangs(c: CharInfo): string[] {
    const out: string[] = []
    if (c.readings.some((r) => r.kind === 'pinyin')) out.push('中')
    if (c.readings.some((r) => r.kind === 'jyutping')) out.push('粵')
    if (c.readings.some((r) => (r.kind === 'onyomi' || r.kind === 'kunyomi') && isKana(r.value))) out.push('日')
    return out
  }

  // first MEANINGFUL sense only, cleaned - the character breakdown stays lean and never leads with a
  // "surname X" cross-reference when a real meaning exists.
  function firstSense(g: string | null): string {
    const parts = cleanGloss(g ?? '').split(';').map((s) => s.trim()).filter(Boolean)
    return parts.find((p) => !isMinorGloss(p)) ?? parts[0] ?? ''
  }

  // Chinese always carries a trad/simp clarifier. When the two scripts differ we show 繁 X · 简 Y; when
  // they're identical we show one glyph tagged 繁简 (same in both). Other varieties don't get this.
  function zhPair(r: Row): { trad: string; simp: string; same: boolean } {
    if (r.alt) {
      const trad = r.formScript === 'simp' ? r.alt : r.form
      const simp = r.formScript === 'simp' ? r.form : r.alt
      return { trad, simp, same: trad === simp }
    }
    return { trad: r.form, simp: r.form, same: true }
  }

  // 熟語 grouped by language - the character is a morpheme in several languages, so the words that use
  // it are shown per language (中 / 粵 / 日) rather than as one misleading single-language list.
  const wordGroups = $derived(
    VORDER.map((v) => ({
      variety: v as Variety,
      items: (entry?.compounds ?? []).filter((l) => l.variety === v).slice(0, 16),
    })).filter((g) => g.items.length),
  )

  let showOrigin = $state(false)
  let showWords = $state(false)
  const wordCount = $derived(wordGroups.reduce((n, g) => n + g.items.length, 0))
  // origin split into delineated segments (drops bare "Etymology N" headers with no body, so a
  // header-only etymology renders nothing rather than an empty toggle)
  const etySegs = $derived(entry?.etymology ? etymologyTokens(entry.etymology) : [])

  // Bound form: a morpheme that doesn't stand alone as a word — it only carries meaning inside
  // compounds. CC-CEDICT flags these; instead of leaking the jargon into the prose we show a small
  // tappable "bound" tag whose popup explains it and lists the compounds the character lives in.
  let boundOpen = $state<Row | null>(null)
  function boundCompounds(r: Row): { lexeme_id: number; headword: string; glosses: string[] }[] {
    return (entry?.compounds ?? []).filter((l) => l.variety === r.variety).slice(0, 30)
  }
  function pivot(q: string) {
    boundOpen = null
    onsearch(q)
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && boundOpen) boundOpen = null }} />

<article class="u">
  <!-- one tappable cross-language row (bridge band + plain results list) -->
  {#snippet rowItem(r: Row)}
    <li>
      <button class="lang" onclick={() => onsearch(r.form)} title="look up {r.form}">
        <span class="body">
          <span class="top">
            <span class="lvar"><span class="vh">{varietyLabel(r.variety)}</span></span>
            <span class="form">{#if r.alt}<span class="ftag">{formTag(r.formScript)}</span>{r.form}<span class="fsep">·</span><span class="ftag">{formTag(r.altScript)}</span>{r.alt}{:else}{r.form}{/if}</span>
            {#if r.reading}<span class="read">{r.variety === 'zh' ? pinyinMarks(r.reading) : r.reading}</span>{/if}
          </span>
          {#if briefGloss(r.glosses)}<span class="gloss">{briefGloss(r.glosses)}</span>{/if}
        </span>
      </button>
    </li>
  {/snippet}

  {#if isGlyphSearch}
    <!-- Block A - the definition: the typed glyph across every language that writes it, co-equally -->
    <section class="def">
      <h2 class="glyph">{head}</h2>
      <div class="defs">
        {#each defRows as r (r.id)}
          {@const ss = senseList(r)}
          <div class="dl">
            <div class="dlh">
              <span class="dvar">{varietyLabel(r.variety)}</span>
              {#if r.variety === 'zh'}
                {@const zp = zhPair(r)}
                {#if zp.same}<span class="dform"><span class="ftag">TC/SC</span>{zp.trad}</span>{:else}<span class="dform"><span class="ftag">TC</span>{zp.trad}<span class="fsep">·</span><span class="ftag">SC</span>{zp.simp}</span>{/if}
              {:else if r.alt}
                <span class="dform"><span class="ftag">{formTag(r.formScript)}</span>{r.form}<span class="fsep">·</span><span class="ftag">{formTag(r.altScript)}</span>{r.alt}</span>
              {:else if r.form !== head}
                <!-- the language writes the same character with a different glyph (Japan: 电 → 電) -->
                <span class="dform">{r.form}</span>
              {/if}
              {#if r.reading}<span class="dread">{r.variety === 'zh' ? pinyinMarks(r.reading) : r.reading}</span>{/if}
              {#if r.variety === 'zh' && headJyut && !hasYueDef}<span class="dvar dcanto">粵</span><span class="dread">{headJyut}</span>{/if}
              {#if isBoundForm(r.glosses)}<button class="btag" onclick={() => (boundOpen = r)} title="bound form — only used in compounds">bound</button>{/if}
            </div>
            {#if ss.length}
              <ol class="senses" class:clamp={!expanded.has(r.id) && overflow.has(r.id)} use:clampProbe={{ id: r.id, rem: 2.9 }}>
                {#each ss as g}<li><span class="sg">{#each glossParts(g) as p}{#if p.link}<button class="xref" onclick={() => onsearch(p.v)}>{p.v}</button>{:else}{p.v}{/if}{/each}</span></li>{/each}
              </ol>
              {#if overflow.has(r.id)}
                <button class="more" onclick={() => toggleSenses(r.id)}>{expanded.has(r.id) ? 'show less' : 'show more'}</button>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
      {#if hasFalseFriend}
        <p class="note"><AlertTriangle size={14} /> {head} is written the same in {falseFriendLangs} but means different things.</p>
      {/if}
    </section>

    {#if everydayRows.length}
      <!-- the natural everyday word another language writes for this character's meaning (耳 → 耳朵) -->
      <section class="bridge">
        <h3>usually written</h3>
        <ul class="langs">
          {#each everydayRows as r (r.id)}{@render rowItem(r)}{/each}
        </ul>
      </section>
    {/if}

    {#if bridgeRows.length}
      <!-- Block B - the bridge: the same meaning, written differently elsewhere. Tappable pivots. -->
      <section class="bridge">
        <h3>written differently</h3>
        <ul class="langs">
          {#each bridgeRows as r (r.id)}{@render rowItem(r)}{/each}
        </ul>
      </section>
    {/if}
  {:else if listRows.length}
    <!-- English / reading search: a plain results list -->
    <section class="bridge">
      <ul class="langs">
        {#each listRows as r (r.id)}{@render rowItem(r)}{/each}
      </ul>
    </section>
  {/if}

  {#if entry && single && headChar}
    <!-- single character: a compact structure line (no repeated glyph), then the words that use it -->
    {#if charReadingGroups(headChar).length}
      <!-- readings: the character's pronunciations, kept on the entry. 中 pinyin · 粵 jyutping · 日
           on'yomi/kun'yomi as kana + romaji. Collapsed to ~3 lines with a toggle. -->
      <section class="readings">
        <h3>readings</h3>
        <div class="rds" class:clamp={!expanded.has(READINGS_ID) && overflow.has(READINGS_ID)} use:clampProbe={{ id: READINGS_ID, rem: 4.4 }}>
          {#each charReadingGroups(headChar) as g}
            <div class="rgrp"><span class="rvh">{g.vh}</span><span class="rtext">{#each g.items as it, i}{i ? '  ·  ' : ''}{it.main}{#if it.sub}<span class="rsub">{it.sub}</span>{/if}{/each}</span></div>
          {/each}
        </div>
        {#if overflow.has(READINGS_ID)}
          <button class="more" onclick={() => toggleSenses(READINGS_ID)}>{expanded.has(READINGS_ID) ? 'show less' : 'show more'}</button>
        {/if}
      </section>
    {/if}
    <section class="struct">
      <h3>structure</h3>
      {#if headChar.script_forms}
        <div class="strip"><ScriptForms forms={headChar.script_forms} anchor={head} {onsearch} /></div>
      {/if}
      {#if decomp}
        <p class="comp">
          <span class="dim">{numWord(decomp.count)} ×</span>
          <button class="part" onclick={() => onsearch(decomp.base)} title="look up {decomp.base}">{decomp.base}</button>
          {#if meaningOf(decomp.base)}<span class="cmean">{meaningOf(decomp.base)}</span>{/if}
          {#if comp?.arrangement}<span class="arr">{comp.arrangement}</span>{/if}
        </p>
      {:else if comp}
        <p class="comp">
          <span class="dim">made of</span>
          {#each comp.parts as p, i}{#if i}<span class="plus">+</span>{/if}<button class="part" onclick={() => onsearch(p.component)} title="look up {p.component}">{p.component}</button>{#if p.count > 1}<span class="dim">×{p.count}</span>{/if}{#if meaningOf(p.component)}<span class="cmean">{meaningOf(p.component)}</span>{/if}{/each}
          {#if comp.arrangement}<span class="arr">{comp.arrangement}</span>{/if}
        </p>
      {/if}
      {#if headChar.strokes}<div class="cln"><span class="dim">{headChar.strokes} strokes</span></div>{/if}
    </section>
  {:else if entry && entry.characters.length}
    <!-- jukugo: break the word into its component characters. Lean: which languages it lives in + one
         meaning. Tap to open the full character page. -->
    <section class="chars">
      <h3>characters</h3>
      {#each entry.characters as c}
        <button class="char" onclick={() => onsearch(c.ch)} title="look up {c.ch}">
          <span class="cg">{c.ch}</span>
          <span class="cmeta">
            <span class="clangs">{#each charLangs(c) as l}<span class="clang">{l}</span>{/each}</span>
            {#if firstSense(c.gloss_en)}<span class="cgl">{firstSense(c.gloss_en)}</span>{/if}
          </span>
        </button>
      {/each}
    </section>
  {:else if enriching}
    <!-- reserve the structure + words space while the entry loads, so nothing pops in below -->
    <section class="skel" aria-hidden="true">
      <div class="skel-h"></div>
      <div class="skel-line"></div>
      <div class="skel-line w60"></div>
      <div class="skel-h"></div>
      <div class="skel-chips">{#each Array(10) as _}<span class="skel-chip"></span>{/each}</div>
    </section>
  {/if}

  {#if etySegs.length}
    <section class="origin">
      <button class="oh" aria-expanded={showOrigin} onclick={() => (showOrigin = !showOrigin)}>
        origin <span class="chev">{showOrigin ? '−' : '+'}</span>
      </button>
      {#if showOrigin}
        <!-- multiple merged statements are shown as separate, delineated paragraphs (with their
             "Etymology N" label when the source numbers them) so it's clear which is which -->
        <div class="etylist" class:multi={etySegs.length > 1}>
          {#each etySegs as seg, i}
            <div class="etyseg">
              {#if seg.heading}<div class="etyhead">{seg.heading}</div>{:else if etySegs.length > 1}<div class="etyhead">{i + 1}</div>{/if}
              <p class="ety">{#each seg.tokens as s}{#if s.t === 'ruby'}<ruby><button class="kanji" onclick={() => onsearch(s.base)}>{s.base}</button><rt>{s.rt}</rt></ruby>{:else if s.t === 'recon'}<span class="recon" title={s.title}>{s.v}</span>{:else if s.t === 'abbr'}<abbr class="term" title={s.title}>{s.v}</abbr>{:else if s.t === 'han'}<button class="kanji" onclick={() => onsearch(s.v)}>{s.v}</button>{:else}{s.v}{/if}{/each}</p>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  {#if entry && wordGroups.length}
    <section class="words">
      <button class="oh" aria-expanded={showWords} onclick={() => (showWords = !showWords)}>
        used in <span class="count">{wordCount}</span> <span class="chev">{showWords ? '−' : '+'}</span>
      </button>
      {#if showWords}
        {#each wordGroups as wg (wg.variety)}
          <div class="wgroup">
            <div class="wglabel">{varietyLabel(wg.variety)}</div>
            <div class="chips">
              {#each wg.items as l (l.lexeme_id)}
                <button class="chip" onclick={() => onsearch(l.headword)} title={glossLine(l.glosses, 1)}>{l.headword}</button>
              {/each}
            </div>
          </div>
        {/each}
      {/if}
    </section>
  {/if}

  {#if boundOpen}
    {@const bc = boundCompounds(boundOpen)}
    <div class="mbg" role="presentation" onclick={() => (boundOpen = null)}>
      <div class="modal" role="dialog" aria-modal="true" aria-label="bound form" onclick={(e) => e.stopPropagation()}>
        <div class="mh"><span class="mglyph">{boundOpen.form}</span><span class="mtag">bound form</span></div>
        <p class="mexp">Not used as a word on its own — it carries meaning only inside compounds.</p>
        {#if bc.length}
          <div class="mlabel">appears in</div>
          <div class="chips">
            {#each bc as l (l.lexeme_id)}<button class="chip" onclick={() => pivot(l.headword)} title={glossLine(l.glosses, 1)}>{l.headword}</button>{/each}
          </div>
        {/if}
        <button class="mclose" onclick={() => (boundOpen = null)}>close</button>
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
  .def { margin-bottom: 0.6rem; }
  .glyph { font-family: var(--han); font-size: clamp(3rem, 16vw, 4.5rem); line-height: 1; margin: 0 0 1.1rem; font-weight: 500; }
  .defs { display: flex; flex-direction: column; gap: 1.2rem; }
  .dlh { display: flex; align-items: baseline; gap: 0.7rem; flex-wrap: wrap; }
  /* the language leads (it's the heading of the definition); the reading is secondary */
  .dvar { font-family: var(--han); font-size: 1.1rem; color: var(--text); font-weight: 500; letter-spacing: 0.02em; }
  /* Cantonese tag appended to the Chinese row (中 ěr · 粵 ji5) when the glyph is shared */
  .dcanto { margin-left: 0.5rem; color: var(--muted); font-weight: 400; }
  .dform { font-family: var(--han); font-size: 1.15rem; }
  .dform .ftag { font-family: var(--mono); font-size: 0.7rem; color: var(--muted); margin-right: 0.18rem; vertical-align: 0.35em; }
  .dform .fsep { color: var(--faint); margin: 0 0.35rem; }
  .dread { font-family: var(--mono); font-size: 0.9rem; color: var(--muted); }
  .senses { margin: 0.5rem 0 0; padding: 0; list-style: none; counter-reset: s; display: flex; flex-direction: column; gap: 0.35rem; }
  /* collapsed: clip to ~2 lines and fade the cut, so a long definition doesn't wall off the page */
  .senses.clamp { max-height: 2.9rem; overflow: hidden; -webkit-mask-image: linear-gradient(to bottom, #000 74%, transparent); mask-image: linear-gradient(to bottom, #000 74%, transparent); }
  .senses li { position: relative; padding-left: 1.5rem; font-size: 1rem; line-height: 1.45; color: var(--text); counter-increment: s; }
  .senses li::before { content: counter(s); position: absolute; left: 0; top: 0.05rem; font-family: var(--mono); font-size: 0.78rem; color: var(--muted); }
  .more { background: none; border: none; padding: 0.3rem 0; margin-top: 0.1rem; font-family: var(--mono); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }
  /* tappable cross-reference target inside a gloss ("variant of 著" → jump to 著) */
  .xref { font-family: var(--han); color: var(--text); background: none; border: none; padding: 0; font: inherit; text-decoration: underline; text-underline-offset: 2px; cursor: pointer; }
  .xref:hover { color: #fff; background: none; }
  .more:hover { color: var(--text); background: none; }

  /* "bound" tag — a bound morpheme (only used in compounds); taps open an explainer + its compounds */
  .btag { font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.07em; color: var(--muted); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.06rem 0.42rem; line-height: 1.4; cursor: pointer; align-self: center; }
  .btag:hover { color: var(--text); border-color: var(--border-strong); background: none; }

  /* bound-form popup — minimal monochrome dialog */
  .mbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 50; }
  .modal { width: min(28rem, 100%); max-height: 80vh; overflow-y: auto; background: var(--surface-2); border: 1px solid var(--border-strong); border-radius: calc(var(--r) * 1.5); padding: 1.2rem 1.2rem 1rem; }
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
  .langs { list-style: none; margin: 0; padding: 0; border-top: 1px solid var(--border); }
  .langs li { border-bottom: 1px solid var(--border); }
  .lang { display: flex; gap: 0.8rem; align-items: flex-start; width: 100%; text-align: left; background: none; border: none; border-radius: 0; padding: 0.7rem 0.5rem; }
  .lang:hover { background: var(--surface); }
  .body { display: flex; flex-direction: column; gap: 0.2rem; min-width: 0; flex: 1; }
  .top { display: flex; align-items: baseline; gap: 0.6rem; flex-wrap: wrap; }
  .lvar { align-self: center; }
  .lvar .vh { font-family: var(--han); font-size: 0.95rem; color: var(--muted); }
  .form { font-family: var(--han); font-size: 1.5rem; line-height: 1.1; }
  /* trad + simp shown as equal peers (no demoting bracket), each with a small 繁/简 tag */
  .form .ftag { font-family: var(--mono); font-size: 0.68rem; color: var(--muted); margin-right: 0.2rem; vertical-align: 0.3em; }
  .form .fsep { color: var(--faint); margin: 0 0.4rem; }
  .strip { margin-top: 0.5rem; }
  .read { font-family: var(--mono); color: var(--muted); font-size: 0.9rem; }
  .gloss { color: var(--text); font-size: 0.98rem; line-height: 1.4; }

  .note { color: var(--faint); font-size: 0.82rem; margin: 0.5rem 0 0; line-height: 1.5; display: flex; align-items: flex-start; gap: 0.4rem; }
  .note :global(svg) { flex: none; margin-top: 0.15rem; color: var(--muted); }

  h3 { font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin: 1.9rem 0 0.8rem; }
  .dim { color: var(--faint); }

  /* jukugo component characters - the WHOLE row is tappable (glyph + tags + meaning) */
  .char { display: flex; gap: 0.9rem; align-items: center; width: 100%; text-align: left; padding: 0.7rem 0.3rem; background: none; border: none; border-top: 1px solid var(--border); }
  .char:hover { background: var(--surface); }
  .cg { font-family: var(--han); font-size: 2.4rem; line-height: 1; padding: 0 0.3rem; flex: none; }
  .cmeta { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .clangs { display: flex; gap: 0.3rem; margin-bottom: 0.25rem; }
  .clang { font-family: var(--han); font-size: 0.8rem; color: var(--muted); border: 1px solid var(--border); border-radius: 4px; padding: 0.05rem 0.32rem; }
  .cgl { font-size: 0.95rem; color: var(--text); }

  /* readings line - 中/粵/日 grouped, on/kun shown as kana + romaji; collapses to ~3 lines */
  .rds { display: flex; flex-direction: column; gap: 0.4rem; }
  .rds.clamp { max-height: 4.4rem; overflow: hidden; -webkit-mask-image: linear-gradient(to bottom, #000 78%, transparent); mask-image: linear-gradient(to bottom, #000 78%, transparent); }
  .rgrp { display: flex; gap: 0.7rem; align-items: baseline; font-family: var(--mono); font-size: 0.95rem; }
  .rvh { font-family: var(--han); color: var(--muted); font-size: 1rem; flex: none; min-width: 1.2em; }
  .rtext { color: var(--text); }
  .rsub { color: var(--muted); margin-left: 0.25rem; font-size: 0.86em; }
  /* structure block - composition (what parts make it up, e.g. 森 = three 木) + a quiet stroke count */
  .cln { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; margin-top: 0.5rem; font-size: 0.8rem; }
  .comp { display: flex; flex-wrap: wrap; align-items: center; gap: 0.4rem; margin: 0.6rem 0 0; }
  .part { font-family: var(--han); color: var(--text); background: none; border: none; padding: 0 0.1rem; font-size: 1.6rem; line-height: 1; }
  .part:hover { color: #fff; background: none; }
  .comp .dim { font-size: 0.82rem; }
  .plus { color: var(--faint); font-family: var(--mono); }
  /* a component's meaning, e.g. 木 (tree) — the "explain the parts" layer */
  .cmean { color: var(--muted); font-size: 0.82rem; margin-left: -0.1rem; }
  .cmean::before { content: '('; }
  .cmean::after { content: ')'; }
  .arr { color: var(--muted); font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.06em; margin-left: 0.3rem; }

  /* words: collapsible (toggle header like origin), grouped by language with breathing room */
  .words { margin-top: 1.6rem; }
  .words .count { color: var(--faint); }
  .wgroup { margin-top: 1rem; }
  .wglabel { font-family: var(--han); font-size: 0.9rem; color: var(--muted); margin: 0 0 0.5rem; letter-spacing: 0.02em; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.35rem; font-family: var(--han); font-size: 1.05rem; padding: 0.25rem 0.55rem; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r); }
  .chip:hover { border-color: var(--border-strong); }
  .cv { font-size: 0.7rem; color: var(--faint); }

  .origin { margin-top: 1.2rem; }
  .oh { display: inline-flex; align-items: center; gap: 0.4rem; background: none; border: none; padding: 0.2rem 0; font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); }
  .oh:hover { color: var(--text); background: none; }
  .oh .chev { font-family: var(--mono); }
  .etylist { margin-top: 0.5rem; }
  /* when several statements are merged, give each its own delineated block */
  .etylist.multi .etyseg { padding-left: 0.9rem; border-left: 1px solid var(--border); margin-top: 0.9rem; }
  .etylist.multi .etyseg:first-child { margin-top: 0; }
  .etyhead { font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); margin-bottom: 0.2rem; }
  .ety { font-size: 0.95rem; color: var(--muted); line-height: 1.9; margin: 0; }
  .ety ruby { font-family: var(--han); }
  .ety rt { font-size: 0.55em; color: var(--faint); font-family: var(--han); }
  .ety .kanji { background: none; border: none; padding: 0; font: inherit; color: var(--text); font-family: var(--han); }
  .ety .kanji:hover { text-decoration: underline; }
  /* jargon term with a plain-English tooltip (tap/hover) */
  .ety .term { color: var(--text); text-decoration: underline dotted; text-underline-offset: 2px; cursor: help; }
  /* phonological reconstructions de-emphasised so the narrative reads first */
  .ety .recon { font-size: 0.78em; color: var(--faint); font-family: var(--mono); }
  .ety .recon[title] { cursor: help; }

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
