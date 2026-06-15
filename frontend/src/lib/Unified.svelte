<script lang="ts">
  import type { CharInfo, Entry, Hit, ReadingKV, Variety } from './types'
  import { primaryForm, varietyLabel, furiganaTokens, pinyinMarks, cleanIds, cleanGloss, glossLine, meaningfulGlossCount } from './display'

  // The unified cross-language view — one Han word, seen across 中 / 粵 / 日 at once.
  // Renders instantly from search hits; enriches (decomposition, origin) when the full entry loads.
  let {
    hits = [],
    entry = null,
    anchor = '',
    onsearch,
  }: {
    hits?: Hit[]
    entry?: Entry | null
    anchor?: string
    onsearch: (q: string) => void
  } = $props()

  const VORDER = ['zh', 'yue', 'ja']
  const fullName: Record<string, string> = { zh: 'Chinese', yue: 'Cantonese', ja: 'Japanese' }

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
    reading: string
    glosses: string[]
    relation: string
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
        reading: h.reading ?? '',
        glosses: h.glosses,
        relation: relFor(h.lexeme_id),
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
        reading: readingFor(entry.variety, entry.readings),
        glosses: entry.senses.map((s) => s.gloss_en),
        relation: 'self',
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
          reading: l.reading ?? '',
          glosses: l.glosses,
          relation: l.relation,
        })
      }
    // collapse to one row per (variety, form), keeping the MOST MEANINGFUL lexeme (tie → the one
    // you looked up). This drops minor duplicates — e.g. the bare "surname Long" / "radical 广"
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
    // drop rows whose only content is a surname/variant cross-reference — unless it's the row you
    // looked up, or it's the sole row for its language (so a purely-minor entry still shows).
    const richByVar = new Set(deduped.filter((r) => meaningfulGlossCount(r.glosses) > 0).map((r) => r.variety))
    deduped = deduped.filter(
      (r) =>
        r.id === primary ||
        meaningfulGlossCount(r.glosses) > 0 ||
        !richByVar.has(r.variety),
    )
    return deduped.sort((a, b) => VORDER.indexOf(a.variety) - VORDER.indexOf(b.variety))
  })

  // the headword glyph: what the user looked up
  const head = $derived(anchor || rows[0]?.form || '')

  // the (language, form) this page resolved to — marked in the stack. Keyed by form, not lexeme id,
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

  // a "different meaning" flag only makes sense as a contrast: another language entry whose meaning
  // differs from the one you looked up. So it needs ≥2 rows and applies to non-primary rows only.
  function isFalseFriendRow(r: Row): boolean {
    return rows.length > 1 && r.relation === 'false-friend' && `${r.variety}|${r.form}` !== primaryKey
  }
  const hasFalseFriend = $derived(rows.some(isFalseFriendRow))

  const READING_ORDER: [string, string][] = [
    ['pinyin', '拼'],
    ['jyutping', '粵'],
    ['onyomi', '音'],
    ['kunyomi', '訓'],
  ]
  const isKana = (s: string) => /[぀-ヿ]/.test(s)
  function charReadings(c: CharInfo) {
    const out: { label: string; value: string }[] = []
    for (const [kind, label] of READING_ORDER) {
      // Japanese on/kun readings must be kana — drop corrupt values like "K0"
      let v = c.readings.filter((r) => r.kind === kind).map((r) => r.value)
      if (kind === 'onyomi' || kind === 'kunyomi') v = v.filter(isKana)
      if (v.length) out.push({ label, value: v.join(' ') })
    }
    return out
  }

  // languages actually present (for the subtitle — hidden when there's only one)
  const varieties = $derived([...new Set(rows.map((r) => r.variety))])

  // single character vs jukugo (compound word) — they get purpose-built layouts:
  // a character page (readings + structure + the words that use it) vs a word page
  // (meaning across languages + its component characters).
  const single = $derived([...head].length === 1)
  const headChar = $derived(entry?.characters?.[0])

  let showOrigin = $state(false)
</script>

<article class="u">
  <header class="head">
    {#if varieties.length > 1}
      <p class="sub">{varieties.map((v) => varietyLabel(v)).join(' · ')}</p>
    {/if}
    <h2 class="glyph">{head}</h2>
  </header>

  <!-- the core: this word, read across every language at once -->
  <ul class="langs">
    {#each rows as r (r.id)}
      <li class:cur={`${r.variety}|${r.form}` === primaryKey}>
        <button class="lang" onclick={() => onsearch(r.form)} title="look up {r.form}">
          <span class="v">{varietyLabel(r.variety)}</span>
          <span class="body">
            <span class="top">
              <span class="form">{r.form}{#if r.alt}<span class="alt">{r.alt}</span>{/if}</span>
              {#if r.reading}<span class="read">{r.variety === 'zh' ? pinyinMarks(r.reading) : r.reading}</span>{/if}
              {#if isFalseFriendRow(r)}<span class="ff">different meaning</span>{/if}
            </span>
            {#if glossLine(r.glosses)}<span class="gloss">{glossLine(r.glosses)}</span>{/if}
          </span>
        </button>
      </li>
    {/each}
  </ul>

  {#if hasFalseFriend}
    <p class="note">同字 — same characters, but the meaning differs by language.</p>
  {/if}

  {#if entry && single && headChar}
    <!-- single character: a compact structure line (no repeated glyph), then the words that use it -->
    <section class="struct">
      <h3>structure <span class="dim">字源</span></h3>
      {#if charReadings(headChar).length}
        <div class="crd">
          {#each charReadings(headChar) as r}<span class="rd"><span class="rl">{r.label}</span> {r.value}</span>{/each}
        </div>
      {/if}
      <div class="cln">
        {#if headChar.strokes}<span class="dim">{headChar.strokes}画</span>{/if}
        {#if headChar.radical}<span class="dim">radical {headChar.radical}</span>{/if}
        {#if cleanIds(headChar.ids)}<span class="ids">{cleanIds(headChar.ids)}</span>{/if}
      </div>
      {#each headChar.variants as v}
        <div class="vedge">→ <b>{v.parent}</b> <span class="dim">{v.edge_type}{#if v.reform_name} · {v.reform_name}{/if}</span></div>
      {/each}
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
              {#if c.strokes}<span class="dim">{c.strokes}画</span>{/if}
              {#if c.radical}<span class="dim">rad {c.radical}</span>{/if}
              {#if cleanIds(c.ids)}<span class="ids">{cleanIds(c.ids)}</span>{/if}
              {#each c.variants as v}<span class="dim">→ <b>{v.parent}</b> {v.edge_type}{#if v.reform_name} · {v.reform_name}{/if}</span>{/each}
            </div>
          </div>
        </div>
      {/each}
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

  {#if entry && !single && entry.translations.length}
    <section class="also">
      <h3>same meaning <span class="dim">同義</span></h3>
      <div class="chips">
        {#each entry.translations.slice(0, 6) as l}
          <button class="chip" onclick={() => onsearch(l.headword)}>
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
          {#each furiganaTokens(entry.etymology) as tok}{#if tok.t === 'ruby'}<ruby><button class="kanji" onclick={() => onsearch(tok.base)}>{tok.base}</button><rt>{tok.rt}</rt></ruby>{:else}{tok.v}{/if}{/each}
        </p>
      {/if}
    </section>
  {/if}
</article>

<style>
  .u { display: flex; flex-direction: column; }
  .head { margin-bottom: 1rem; }
  .glyph { font-family: var(--han); font-size: clamp(3rem, 16vw, 4.5rem); line-height: 1; margin: 0; font-weight: 500; }
  .sub { font-family: var(--han); color: var(--faint); font-size: 0.85rem; margin: 0 0 0.4rem; letter-spacing: 0.1em; }

  /* the cross-language stack — the heart of the app */
  .langs { list-style: none; margin: 0 0 0.4rem; padding: 0; border-top: 1px solid var(--border); }
  .langs li { border-bottom: 1px solid var(--border); }
  /* the row this page resolved to — marked so you know which entry you're on */
  .langs li.cur { background: var(--surface); box-shadow: inset 2px 0 0 var(--text); }
  .lang { display: flex; gap: 0.8rem; align-items: flex-start; width: 100%; text-align: left; background: none; border: none; border-radius: 0; padding: 0.85rem 0.3rem; }
  .lang:hover { background: var(--surface); }
  .v { font-family: var(--han); font-size: 1rem; color: var(--muted); flex: none;
       display: inline-flex; align-items: center; justify-content: center; width: 1.5rem; height: 1.5rem; }
  .langs li.cur .v { background: var(--text); color: var(--bg); border-radius: 5px; }
  .body { display: flex; flex-direction: column; gap: 0.25rem; min-width: 0; flex: 1; }
  .top { display: flex; align-items: baseline; gap: 0.6rem; flex-wrap: wrap; }
  .form { font-family: var(--han); font-size: 1.5rem; line-height: 1.1; }
  .form .alt { color: var(--faint); font-size: 0.62em; margin-left: 0.25rem; }
  .read { font-family: var(--mono); color: var(--muted); font-size: 0.9rem; }
  .ff { font-size: 0.6rem; line-height: 1; color: var(--faint); border: 1px dashed var(--border-strong); border-radius: 4px; padding: 0.2rem 0.35rem; align-self: center; display: inline-flex; align-items: center; }
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
  .crd .rl { font-family: var(--han); color: var(--faint); margin-right: 0.15rem; }
  .cgl { font-size: 0.92rem; color: var(--muted); margin-top: 0.25rem; }
  .cln { display: flex; gap: 0.7rem; align-items: center; flex-wrap: wrap; margin-top: 0.3rem; font-size: 0.8rem; }
  .ids { font-family: var(--han); color: var(--muted); }
  .cln b { font-family: var(--han); }

  /* single-character structure block — readings + decomposition, no repeated glyph */
  .struct .crd { font-size: 0.9rem; gap: 1rem; }
  .struct .cln { margin-top: 0.5rem; }
  .vedge { font-size: 0.9rem; margin-top: 0.35rem; }
  .vedge b { font-family: var(--han); }

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
</style>
