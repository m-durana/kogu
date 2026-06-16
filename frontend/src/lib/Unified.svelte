<script lang="ts">
  import type { CharInfo, Entry, Hit, ReadingKV, Variety } from './types'
  import { primaryForm, varietyLabel, furiganaTokens, pinyinMarks, cleanIds, cleanGloss, glossLine, briefGloss, meaningfulGlossCount, splitRecon, formTag } from './display'
  import ScriptForms from './ScriptForms.svelte'

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
  const sectionName: Record<string, string> = { zh: '中文', yue: '粵語', ja: '日本語' }

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
    // collapse to one row per (variety, form), keeping the MOST MEANINGFUL lexeme (tie → the one
    // you looked up). This drops minor duplicates - e.g. the bare "surname Long" / "radical 广"
    // entries that share a form with the real word (dragon / wide).
    const primary = hits[0]?.lexeme_id ?? entry?.lexeme_id ?? -1
    const best = new Map<string, Row>()
    for (const r of out) {
      const key = `${r.variety}|${r.form}`
      const prev = best.get(key)
      if (!prev) {
        best.set(key, r)
        continue
      }
      const rr = meaningfulGlossCount(r.glosses)
      const pr = meaningfulGlossCount(prev.glosses)
      if (rr > pr || (rr === pr && r.id === primary)) best.set(key, r)
    }
    let deduped = [...best.values()]
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
    // (機場 ↔ 日 空港). Explicit equivalence edges (relation 'equivalent': 冇→沒有, curated cross-lang)
    // are precise statements and ALWAYS show. Fuzzy gloss-pivot synonyms only fill GAP languages -
    // never padding a language that already has a same-glyph row - and are capped, to stay tight.
    const haveForm = new Set(deduped.map((r) => r.variety))
    const haveKey = new Set(deduped.map((r) => `${r.variety}|${r.form}`))
    // a curated equivalent for a language is the trusted answer - it suppresses that language's fuzzy
    // gloss-pivot synonyms (so 自行車 shows 日 自転車, not also the noisy バイク / 夜車 matches).
    const haveEquivVar = new Set(
      (entry?.translations ?? []).filter((l) => l.relation === 'equivalent').map((l) => l.variety),
    )
    const equivCount: Record<string, number> = {}
    for (const l of entry?.translations ?? []) {
      const explicit = l.relation === 'equivalent'
      if (!explicit && haveForm.has(l.variety)) continue // language already represented by its own glyph
      if (!explicit && haveEquivVar.has(l.variety)) continue // a curated equivalent already covers it
      const key = `${l.variety}|${l.headword}`
      if (haveKey.has(key)) continue
      if (!explicit && (equivCount[l.variety] ?? 0) >= 1) continue // tight cap on fuzzy synonyms
      haveKey.add(key)
      if (!explicit) equivCount[l.variety] = (equivCount[l.variety] ?? 0) + 1
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
    rows
      .filter((r) => r.kind === 'form' && r.form === head)
      .sort((a, b) => {
        const am = `${a.variety}|${a.form}` === primaryKey ? 0 : 1
        const bm = `${b.variety}|${b.form}` === primaryKey ? 0 : 1
        return am !== bm ? am - bm : VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety)
      }),
  )
  const isGlyphSearch = $derived(defRows.length > 0)

  // Block B - the bridge: how the same meaning is written DIFFERENTLY in another language (a different
  // glyph or the cross-script form). Everything that isn't a same-glyph definition. Only meaningful
  // once there's a glyph definition to bridge FROM.
  const bridgeRows = $derived(
    isGlyphSearch
      ? rows
          .filter((r) => !(r.kind === 'form' && r.form === head))
          // trusted curated equivalents lead; fuzzy gloss-pivot synonyms follow. Within each, 中/粵/日.
          .sort((a, b) => {
            const ae = a.relation === 'equivalent' ? 0 : 1
            const be = b.relation === 'equivalent' ? 0 : 1
            return ae !== be ? ae - be : VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety)
          })
      : [],
  )

  // English / pinyin search (nothing matches the typed glyph): fall back to a plain results list.
  const listRows = $derived(isGlyphSearch ? [] : rows)

  // numbered senses for a definition row - the full hit glosses (every sense), cleaned. Identical
  // treatment for every language (no POS on one and not another) so the languages stay co-equal.
  // Capped with a per-row toggle so a 13-sense kanji doesn't wall off the bridge below.
  const SENSE_CAP = 6
  let expanded = $state(new Set<number>())
  function senseList(r: Row): string[] {
    return r.glosses.map(cleanGloss).filter(Boolean)
  }
  function toggleSenses(id: number) {
    const n = new Set(expanded)
    if (n.has(id)) n.delete(id)
    else n.add(id)
    expanded = n
  }

  // false friends are SAME-glyph words whose meaning diverges (手紙) - they sit co-equally in block A,
  // flagged by a single note. (A different-glyph bridge row is never a false friend - it's just the
  // other language's word.)
  const hasFalseFriend = $derived(defRows.length > 1 && defRows.some((r) => r.relation === 'false-friend'))

  // reading systems, labelled in plain words (中 Mandarin pinyin, 粵 Cantonese jyutping,
  // 日 Japanese on'yomi / kun'yomi) so it's obvious which language each reading is. Each maps to the
  // language that uses it, so we can hide readings for languages this word isn't actually used in.
  const READING_ORDER: [string, string, Variety][] = [
    ['pinyin', 'pinyin', 'zh'],
    ['jyutping', 'jyutping', 'yue'],
    ['onyomi', "on'yomi", 'ja'],
    ['kunyomi', "kun'yomi", 'ja'],
  ]
  const isKana = (s: string) => /[぀-ヿ]/.test(s)
  // `allow`: if given, hide pinyin / on'yomi / kun'yomi for languages this word isn't used in. 冇
  // carries a nominal Unihan pinyin (mǎo) but is a Cantonese-only word - showing it contradicts the
  // 粵 label above. Jyutping is NEVER gated: Cantonese is written with essentially all Han
  // characters, so a jyutping reading is legitimate even without a separate 粵 lexeme (e.g. 氣 hei3).
  function charReadings(c: CharInfo, allow?: Set<string>) {
    const out: { label: string; value: string }[] = []
    for (const [kind, label, variety] of READING_ORDER) {
      if (allow && variety !== 'yue' && !allow.has(variety)) continue
      // Japanese on/kun readings must be kana - drop corrupt values like "K0"
      let v = c.readings.filter((r) => r.kind === kind).map((r) => r.value)
      if (kind === 'onyomi' || kind === 'kunyomi') v = v.filter(isKana)
      if (v.length) out.push({ label, value: v.join(' ') })
    }
    return out
  }

  // languages this word is actually represented in (its same-glyph rows) - used to gate the
  // single-character structure readings so e.g. a 粵-only word shows jyutping, not Mandarin pinyin.
  const wordVarieties = $derived(new Set(rows.filter((r) => r.kind === 'form').map((r) => r.variety)))

  // single character vs jukugo (compound word) - they get purpose-built layouts:
  // a character page (readings + structure + the words that use it) vs a word page
  // (meaning across languages + its component characters).
  const single = $derived([...head].length === 1)
  const headChar = $derived(entry?.characters?.[0])

  let showOrigin = $state(false)
</script>

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
              <span class="dvar">{sectionName[r.variety]}</span>
              {#if r.alt}<span class="dform"><span class="ftag">{formTag(r.formScript)}</span>{r.form}<span class="fsep">·</span><span class="ftag">{formTag(r.altScript)}</span>{r.alt}</span>{/if}
              {#if r.reading}<span class="dread">{r.variety === 'zh' ? pinyinMarks(r.reading) : r.reading}</span>{/if}
            </div>
            {#if ss.length}
              <ol class="senses">
                {#each (expanded.has(r.id) ? ss : ss.slice(0, SENSE_CAP)) as g}<li><span class="sg">{g}</span></li>{/each}
              </ol>
              {#if ss.length > SENSE_CAP}
                <button class="more" onclick={() => toggleSenses(r.id)}>{expanded.has(r.id) ? 'show less' : `show all ${ss.length}`}</button>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
      {#if hasFalseFriend}
        <p class="note">同字 · same characters, different meaning by language.</p>
      {/if}
    </section>

    {#if bridgeRows.length}
      <!-- Block B - the bridge: the same meaning, written differently elsewhere. Tappable pivots. -->
      <section class="bridge">
        <h3>written differently <span class="dim">異形</span></h3>
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
    <section class="struct">
      <h3>structure <span class="dim">字源</span></h3>
      {#if charReadings(headChar, wordVarieties).length}
        <div class="crd">
          {#each charReadings(headChar, wordVarieties) as r}<span class="rd"><span class="rl">{r.label}</span> {r.value}</span>{/each}
        </div>
      {/if}
      {#if headChar.script_forms}
        <div class="strip"><ScriptForms forms={headChar.script_forms} anchor={head} {onsearch} /></div>
      {/if}
      <div class="cln">
        {#if headChar.strokes}<span class="dim">{headChar.strokes} strokes</span>{/if}
        {#if cleanIds(headChar.ids)}<span class="dim">parts</span> <span class="ids">{cleanIds(headChar.ids)}</span>{/if}
      </div>
    </section>
  {:else if entry && entry.characters.length}
    <!-- jukugo: break the word into its component characters, each tappable -->
    <section class="chars">
      <h3>characters <span class="dim">字</span></h3>
      {#each entry.characters as c}
        <div class="char">
          <button class="cg" onclick={() => onsearch(c.ch)} title="look up {c.ch}">{c.ch}</button>
          <div class="cmeta">
            {#if charReadings(c).length}
              <div class="crd">
                {#each charReadings(c) as r}<span class="rd"><span class="rl">{r.label}</span> {r.value}</span>{/each}
              </div>
            {/if}
            {#if cleanGloss(c.gloss_en ?? '')}<div class="cgl">{cleanGloss(c.gloss_en ?? '')}</div>{/if}
            <div class="cln">
              {#if c.strokes}<span class="dim">{c.strokes} strokes</span>{/if}
              {#if cleanIds(c.ids)}<span class="ids">{cleanIds(c.ids)}</span>{/if}
            </div>
            {#if c.script_forms}
              <div class="strip"><ScriptForms forms={c.script_forms} anchor={c.ch} {onsearch} compact /></div>
            {/if}
          </div>
        </div>
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

  {#if entry && entry.compounds.length}
    <section class="words">
      <h3>words <span class="dim">熟語</span></h3>
      <div class="chips">
        {#each entry.compounds.slice(0, 24) as l}
          <button class="chip" onclick={() => onsearch(l.headword)} title={glossLine(l.glosses, 1)}>
            <span class="cv">{varietyLabel(l.variety)}</span>{l.headword}
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if entry && entry.etymology}
    <section class="origin">
      <button class="oh" aria-expanded={showOrigin} onclick={() => (showOrigin = !showOrigin)}>
        origin <span class="dim">語源</span> <span class="chev">{showOrigin ? '−' : '+'}</span>
      </button>
      {#if showOrigin}
        <p class="ety">
          {#each furiganaTokens(entry.etymology) as tok}{#if tok.t === 'ruby'}<ruby><button class="kanji" onclick={() => onsearch(tok.base)}>{tok.base}</button><rt>{tok.rt}</rt></ruby>{:else}{#each splitRecon(tok.v) as s}{#if s.t === 'recon'}<span class="recon">{s.v}</span>{:else}{s.v}{/if}{/each}{/if}{/each}
        </p>
      {/if}
    </section>
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
  .dvar { font-family: var(--han); font-size: 0.95rem; color: var(--muted); letter-spacing: 0.03em; }
  .dform { font-family: var(--han); font-size: 1.2rem; }
  .dform .ftag { font-family: var(--mono); font-size: 0.7rem; color: var(--muted); margin-right: 0.18rem; vertical-align: 0.35em; }
  .dform .fsep { color: var(--faint); margin: 0 0.35rem; }
  .dread { font-family: var(--mono); font-size: 1rem; color: var(--text); }
  .senses { margin: 0.5rem 0 0; padding: 0; list-style: none; counter-reset: s; display: flex; flex-direction: column; gap: 0.35rem; }
  .senses li { position: relative; padding-left: 1.5rem; font-size: 1rem; line-height: 1.45; color: var(--text); counter-increment: s; }
  .senses li::before { content: counter(s); position: absolute; left: 0; top: 0.05rem; font-family: var(--mono); font-size: 0.78rem; color: var(--muted); }
  .more { background: none; border: none; padding: 0.3rem 0; margin-top: 0.1rem; font-family: var(--mono); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }
  .more:hover { color: var(--text); background: none; }

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

  .note { color: var(--faint); font-size: 0.8rem; margin: 0.3rem 0 0; }

  h3 { font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin: 1.4rem 0 0.5rem; }
  h3 .dim { font-family: var(--han); }
  .dim { color: var(--faint); }

  .char { display: flex; gap: 0.9rem; padding: 0.7rem 0; border-top: 1px solid var(--border); }
  .cg { font-family: var(--han); font-size: 2.4rem; line-height: 1; padding: 0 0.3rem; background: none; border: none; }
  .cg:hover { background: var(--surface); }
  .cmeta { flex: 1; min-width: 0; }
  .crd { display: flex; flex-wrap: wrap; gap: 0.8rem; font-family: var(--mono); font-size: 0.8rem; }
  .crd .rl { font-family: var(--mono); font-size: 0.85em; color: var(--muted); margin-right: 0.2rem; }
  .cgl { font-size: 0.92rem; color: var(--muted); margin-top: 0.25rem; }
  .cln { display: flex; gap: 0.7rem; align-items: center; flex-wrap: wrap; margin-top: 0.3rem; font-size: 0.8rem; }
  .ids { font-family: var(--han); color: var(--muted); }

  /* single-character structure block - readings + decomposition, no repeated glyph */
  .struct .crd { font-size: 0.9rem; gap: 1rem; }
  .struct .cln { margin-top: 0.5rem; }

  .chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.35rem; font-family: var(--han); font-size: 1.05rem; padding: 0.25rem 0.55rem; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r); }
  .chip:hover { border-color: var(--border-strong); }
  .cv { font-size: 0.7rem; color: var(--faint); }

  .origin { margin-top: 1.2rem; }
  .oh { display: inline-flex; align-items: center; gap: 0.4rem; background: none; border: none; padding: 0.2rem 0; font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); }
  .oh:hover { color: var(--text); background: none; }
  .oh .chev { font-family: var(--mono); }
  .ety { font-size: 0.95rem; color: var(--muted); line-height: 1.9; margin: 0.5rem 0 0; }
  .ety ruby { font-family: var(--han); }
  .ety rt { font-size: 0.55em; color: var(--faint); font-family: var(--han); }
  .ety .kanji { background: none; border: none; padding: 0; font: inherit; color: var(--text); }
  .ety .kanji:hover { text-decoration: underline; }
  /* phonological reconstructions de-emphasised so the narrative reads first */
  .ety .recon { font-size: 0.78em; color: var(--faint); font-family: var(--mono); }

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
