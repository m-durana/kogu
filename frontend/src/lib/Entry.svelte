<script lang="ts">
  import type { CharInfo, Entry } from './types'
  import { primaryForm, varietyLabel } from './display'

  let {
    entry,
    anchor = '',
    onsearch,
  }: { entry: Entry; anchor?: string; onsearch: (q: string) => void } = $props()

  const disp = $derived(primaryForm(entry.forms, entry.variety, anchor))

  let tab = $state<'meaning' | 'why' | 'characters'>('meaning')

  // headword readings (hide internal normalisation kinds; backend already excludes them)
  const headReadings = $derived(
    entry.readings.filter((r) => ['pinyin', 'jyutping', 'kana', 'romaji', 'zhuyin'].includes(r.kind)),
  )

  // phonological "why": per-character readings across varieties, side by side
  const READING_ORDER: [string, string][] = [
    ['pinyin', '拼'],
    ['jyutping', '粵'],
    ['onyomi', '音'],
    ['kunyomi', '訓'],
  ]
  function readingLine(c: CharInfo) {
    const out: { label: string; value: string }[] = []
    for (const [kind, label] of READING_ORDER) {
      const v = c.readings.filter((r) => r.kind === kind).map((r) => r.value)
      if (v.length) out.push({ label, value: v.join(' ') })
    }
    return out
  }

  const reformName: Record<string, string> = {
    'prc-1956': 'PRC 1956', 'prc-1964': 'PRC 1964', 'jp-toyo': 'Tōyō', 'jp-joyo': 'Jōyō',
    opencc: 'OpenCC', 'unihan-variant': 'Unihan', 'hk-std': 'HK', 'tw-std': 'TW',
  }

  const charsWithWhy = $derived(entry.characters.filter((c) => c.variants.length || readingLine(c).length))
  const hasWhy = $derived(
    entry.origin_badges.length > 0 || !!entry.etymology || charsWithWhy.length > 0,
  )
</script>

<article class="entry">
  <header>
    <div class="hero">
      <span class="var v-{entry.variety}">{varietyLabel(entry.variety)}</span>
      <h2 class="head">
        {disp?.primary.form ?? entry.headword}{#if disp?.alternate}<span class="alt">{disp.alternate.form}</span>{/if}
      </h2>
    </div>
    {#if headReadings.length}
      <div class="readings">
        {#each headReadings as r}<span class="rk">{r.value}</span>{/each}
      </div>
    {/if}
  </header>

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === 'meaning'} onclick={() => (tab = 'meaning')}>meaning</button>
    {#if hasWhy}<button role="tab" aria-selected={tab === 'why'} onclick={() => (tab = 'why')}>why</button>{/if}
    <button role="tab" aria-selected={tab === 'characters'} onclick={() => (tab = 'characters')}>characters</button>
  </div>

  {#if tab === 'meaning'}
    <section class="pane">
      <ol class="senses">
        {#each entry.senses as s}
          <li>{#if s.pos}<span class="pos">{s.pos}</span>{/if}{s.gloss_en}</li>
        {/each}
      </ol>

      {#if entry.same_form.length}
        <h3>same form <span class="dim">同字</span></h3>
        {#each entry.same_form as l}
          <button class="link" onclick={() => onsearch(l.headword)}>
            <span class="var v-{l.variety}">{varietyLabel(l.variety)}</span>
            <span class="lhead">{l.headword}</span>
            <span class="rel {l.relation === 'false-friend' ? 'ff' : 'cog'}">{l.relation === 'false-friend' ? 'false friend' : 'cognate'}</span>
            <span class="dim lg">{l.glosses[0] ?? ''}</span>
          </button>
        {/each}
      {/if}

      {#if entry.translations.length}
        <h3>same meaning <span class="dim">同義</span></h3>
        {#each entry.translations as l}
          <button class="link" onclick={() => onsearch(l.headword)}>
            <span class="var v-{l.variety}">{varietyLabel(l.variety)}</span>
            <span class="lhead">{l.headword}</span>
            {#if l.reading}<span class="lread">{l.reading}</span>{/if}
            {#if l.concept}<span class="dim lg">{l.concept}</span>{/if}
          </button>
        {/each}
      {/if}
    </section>
  {:else if tab === 'why'}
    <section class="pane">
      {#if entry.origin_badges.length || entry.etymology}
        <h3>origin <span class="dim">lexical</span></h3>
        {#if entry.origin_badges.length}
          <div class="badges">{#each entry.origin_badges as b}<span class="obadge">{b.replace(/-/g, ' ')}</span>{/each}</div>
        {/if}
        {#if entry.etymology}<p class="ety">{entry.etymology}</p>{/if}
      {/if}

      {#each charsWithWhy as c}
        <div class="whychar">
          <button class="glyph sm" onclick={() => onsearch(c.ch)}>{c.ch}</button>
          <div class="whymeta">
            {#if c.variants.length}
              {#each c.variants as v}
                <div class="vedge">{c.ch} → <b>{v.parent}</b> <span class="dim">{v.edge_type}{#if v.reform_name} · {v.reform_name}{#if v.reform_year} {v.reform_year}{/if}{/if}</span></div>
              {/each}
            {/if}
            {#if readingLine(c).length}
              <div class="creadings">{#each readingLine(c) as r}<span class="rd"><span class="rl">{r.label}</span> {r.value}</span>{/each}</div>
            {/if}
          </div>
        </div>
      {/each}
    </section>
  {:else}
    <section class="pane">
      {#each entry.characters as c}
        <div class="char">
          <button class="glyph" onclick={() => onsearch(c.ch)} title="look up {c.ch}">{c.ch}</button>
          <div class="cmeta">
            <div class="cline">
              <span class="badge {c.is_orthodox ? 'b-orth' : 'b-deriv'}">{c.is_orthodox ? 'orthodox' : 'derived'}</span>
              {#if c.strokes}<span class="dim">{c.strokes}画</span>{/if}
              {#if c.radical}<span class="dim">rad {c.radical}</span>{/if}
              {#if c.ids}<span class="ids">{c.ids}</span>{/if}
            </div>
            {#if c.gloss_en}<div class="cgloss">{c.gloss_en}</div>{/if}
          </div>
        </div>
      {/each}
    </section>
  {/if}
</article>

<style>
  .entry { display: flex; flex-direction: column; }
  header { padding-bottom: 1rem; }
  .hero { display: flex; align-items: flex-start; gap: 0.6rem; }
  .var { font-family: var(--han); font-size: 0.8rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0.1rem 0.3rem; margin-top: 0.6rem; flex: none; }
  .v-ja, .v-zh, .v-yue { color: var(--muted); }
  .head { font-family: var(--han); font-size: clamp(2.8rem, 13vw, 4.2rem); margin: 0; font-weight: 500; line-height: 1; }
  .head .alt { color: var(--faint); font-size: 0.4em; margin-left: 0.3rem; font-weight: 400; }
  .readings { display: flex; flex-wrap: wrap; gap: 1rem; margin-top: 0.7rem; font-family: var(--mono); color: var(--text); font-size: 0.95rem; }

  .tabs { display: flex; gap: 0.4rem; border-bottom: 1px solid var(--border); margin-bottom: 1.1rem; }
  .tabs button {
    border: none; background: none; color: var(--faint); padding: 0.6rem 0.2rem; margin-right: 0.8rem;
    font-family: var(--mono); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.12em;
    border-bottom: 2px solid transparent; border-radius: 0;
  }
  .tabs button:hover { color: var(--text); background: none; }
  .tabs button[aria-selected='true'] { color: var(--text); border-bottom-color: var(--text); }

  .pane { display: flex; flex-direction: column; gap: 0.3rem; animation: fade 0.2s ease; }
  @keyframes fade { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; } }
  h3 { font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin: 1.2rem 0 0.4rem; }
  h3 .dim { font-family: var(--han); }
  .dim { color: var(--faint); }

  .senses { margin: 0; padding-left: 1.3rem; }
  .senses li { margin: 0.3rem 0; font-size: 1.05rem; line-height: 1.5; }
  .pos { color: var(--faint); font-size: 0.75rem; margin-right: 0.4rem; font-family: var(--mono); }

  .link { display: flex; gap: 0.6rem; align-items: baseline; text-align: left; padding: 0.5rem 0.4rem; border: none; background: none; border-radius: var(--r); }
  .link:hover { background: var(--surface); }
  .lhead { font-family: var(--han); font-size: 1.3rem; }
  .lread { font-family: var(--mono); color: var(--muted); font-size: 0.8rem; }
  .lg { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.85rem; }
  .rel { font-size: 0.6rem; padding: 0.1rem 0.35rem; border-radius: 4px; flex: none; text-transform: uppercase; letter-spacing: 0.04em; }
  .rel.cog { border: 1px solid var(--border-strong); color: var(--muted); }
  .rel.ff { background: var(--text); color: var(--bg); font-weight: 700; }

  .badges { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .obadge { font-size: 0.68rem; padding: 0.12rem 0.45rem; border: 1px solid var(--border-strong); border-radius: var(--r); text-transform: uppercase; letter-spacing: 0.04em; }
  .ety { font-size: 0.95rem; color: var(--muted); line-height: 1.6; margin: 0.5rem 0 0; }

  .whychar, .char { display: flex; gap: 0.9rem; padding: 0.7rem 0; border-top: 1px solid var(--border); }
  .glyph { font-family: var(--han); font-size: 2.6rem; padding: 0 0.4rem; line-height: 1; background: none; border: none; }
  .glyph.sm { font-size: 1.9rem; }
  .glyph:hover { background: var(--surface); }
  .whymeta { flex: 1; display: flex; flex-direction: column; gap: 0.35rem; justify-content: center; }
  .vedge { font-size: 0.9rem; }
  .vedge b { font-family: var(--han); }
  .creadings { display: flex; flex-wrap: wrap; gap: 0.8rem; font-family: var(--mono); font-size: 0.8rem; }
  .creadings .rl { font-family: var(--han); color: var(--faint); margin-right: 0.15rem; }
  .cmeta { flex: 1; }
  .cline { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
  .badge { font-size: 0.68rem; padding: 0.05rem 0.3rem; border: 1px solid var(--border-strong); border-radius: 4px; }
  .b-orth { color: #fff; }
  .b-deriv { color: var(--muted); border-style: dashed; }
  .ids { font-family: var(--han); color: var(--muted); }
  .cgloss { font-size: 0.9rem; color: var(--muted); margin-top: 0.25rem; }
</style>
