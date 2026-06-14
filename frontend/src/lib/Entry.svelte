<script lang="ts">
  import type { Entry, PrefScript } from './types'
  import type { CharInfo } from './types'
  import { pickForms, varietyLabel } from './display'
  import { ArrowRight } from '@lucide/svelte'

  let {
    entry,
    pref,
    onsearch,
  }: { entry: Entry; pref: PrefScript; onsearch: (q: string) => void } = $props()

  const disp = $derived(pickForms(entry.forms, entry.variety, pref))

  // phonological "why": readings across varieties, side by side
  const READING_ORDER: [string, string][] = [
    ['pinyin', '拼'],
    ['jyutping', '粵'],
    ['onyomi', '音'],
    ['kunyomi', '訓'],
    ['zhuyin', 'ㄅ'],
  ]
  function readingLine(c: CharInfo) {
    const out: { label: string; value: string }[] = []
    for (const [kind, label] of READING_ORDER) {
      const vals = c.readings.filter((r) => r.kind === kind).map((r) => r.value)
      if (vals.length) out.push({ label, value: vals.join(' ') })
    }
    return out
  }
</script>

<article class="entry">
  <header>
    <span class="var v-{entry.variety}">{varietyLabel(entry.variety)}</span>
    <h2 class="head">
      {disp?.primary.form ?? entry.headword}{#if disp?.alternate}<span class="alt">［{disp.alternate.form}］</span>{/if}
    </h2>
    {#if entry.reading}<div class="reading">{entry.reading}</div>{/if}
  </header>

  {#if entry.readings.length}
    <div class="readings">
      {#each entry.readings as r}
        <span class="rk"><b>{r.kind}</b> {r.value}</span>
      {/each}
    </div>
  {/if}

  <ol class="senses">
    {#each entry.senses as s}
      <li>{#if s.pos}<span class="pos">{s.pos}</span>{/if}{s.gloss_en}</li>
    {/each}
  </ol>

  {#if entry.origin_badges.length || entry.etymology}
    <section class="origin">
      <h3>origin <span class="dim">why this word</span></h3>
      {#if entry.origin_badges.length}
        <div class="badges">
          {#each entry.origin_badges as b}<span class="obadge">{b.replace(/-/g, ' ')}</span>{/each}
        </div>
      {/if}
      {#if entry.etymology}<p class="ety">{entry.etymology}</p>{/if}
    </section>
  {/if}

  <section class="chars">
    <h3>characters</h3>
    {#each entry.characters as c}
      <div class="char">
        <button class="glyph" onclick={() => onsearch(c.ch)} title="look up {c.ch}">{c.ch}</button>
        <div class="cmeta">
          <div class="cline">
            <span class="badge {c.is_orthodox ? 'b-orth' : 'b-deriv'}">
              {c.is_orthodox ? 'orthodox' : 'derived'}
            </span>
            {#if c.strokes}<span class="dim">{c.strokes}画</span>{/if}
            {#if c.ids}<span class="ids">{c.ids}</span>{/if}
          </div>
          {#if c.gloss_en}<div class="cgloss">{c.gloss_en}</div>{/if}
          {#if readingLine(c).length}
            <div class="creadings">
              {#each readingLine(c) as r}
                <span class="rd"><span class="rl">{r.label}</span> {r.value}</span>
              {/each}
            </div>
          {/if}
          {#if c.variants.length}
            <div class="variants">
              {#each c.variants as v}
                <span class="vedge">
                  <ArrowRight size={13} /> <b>{v.parent}</b>
                  <span class="dim">{v.edge_type}{#if v.reform_name} · {v.reform_name}{#if v.reform_year} ({v.reform_year}){/if}{/if}</span>
                </span>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/each}
  </section>

  {#if entry.same_form.length}
    <section class="links">
      <h3>同字 <span class="dim">same form</span></h3>
      {#each entry.same_form as l}
        <button class="link" onclick={() => onsearch(l.headword)}>
          <span class="var v-{l.variety}">{varietyLabel(l.variety)}</span>
          <span class="lhead">{l.headword}</span>
          {#if l.reading}<span class="lread">{l.reading}</span>{/if}
          <span class="rel {l.relation === 'false-friend' ? 'ff' : 'cog'}">
            {l.relation === 'false-friend' ? 'false friend' : 'cognate'}
          </span>
          <span class="dim lg">{l.glosses[0] ?? ''}</span>
        </button>
      {/each}
    </section>
  {/if}

  {#if entry.translations.length}
    <section class="links">
      <h3>同義 <span class="dim">same meaning</span></h3>
      {#each entry.translations as l}
        <button class="link" onclick={() => onsearch(l.headword)}>
          <span class="var v-{l.variety}">{varietyLabel(l.variety)}</span>
          <span class="lhead">{l.headword}</span>
          {#if l.reading}<span class="lread">{l.reading}</span>{/if}
          {#if l.concept}<span class="concept">{l.concept}</span>{/if}
        </button>
      {/each}
    </section>
  {/if}
</article>

<style>
  .entry { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: 1.2rem; }
  header { border-bottom: 1px solid var(--border); padding-bottom: 0.8rem; margin-bottom: 0.8rem; }
  .var { font-size: 0.75rem; padding: 0.1rem 0.35rem; border: 1px solid var(--border-strong); color: var(--muted); }
  .v-zh { color: var(--zh); border-color: var(--accent-dim); }
  .v-ja { color: var(--ja); }
  .v-yue { color: var(--yue); }
  .head { font-family: var(--han); font-size: 3rem; margin: 0.4rem 0 0.2rem; font-weight: 600; }
  .alt { color: var(--muted); font-size: 1.6rem; font-weight: 400; }
  .reading { font-family: var(--mono); color: var(--accent); font-size: 1.1rem; }
  .readings { display: flex; flex-wrap: wrap; gap: 0.8rem; margin-bottom: 0.8rem; font-size: 0.85rem; }
  .rk b { color: var(--muted); font-weight: 600; margin-right: 0.2rem; text-transform: uppercase; font-size: 0.7rem; }
  .rk { font-family: var(--mono); }
  .senses { margin: 0 0 1rem; padding-left: 1.2rem; }
  .senses li { margin: 0.25rem 0; }
  .pos { color: var(--faint); font-size: 0.75rem; margin-right: 0.4rem; font-family: var(--mono); }
  h3 { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); border-top: 1px solid var(--border); padding-top: 0.8rem; }
  .dim { color: var(--faint); font-weight: 400; }
  .char { display: flex; gap: 0.8rem; padding: 0.5rem 0; border-bottom: 1px solid var(--border); }
  .glyph { font-family: var(--han); font-size: 2.4rem; padding: 0.2rem 0.5rem; line-height: 1; }
  .cmeta { flex: 1; }
  .cline { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
  .badge { font-size: 0.7rem; padding: 0.05rem 0.3rem; border: 1px solid var(--border-strong); border-radius: 5px; }
  .b-orth { color: #fff; border-color: var(--border-strong); }
  .b-deriv { color: var(--muted); border-style: dashed; }
  .ids { font-family: var(--han); color: var(--muted); }
  .cgloss { font-size: 0.85rem; color: var(--muted); margin-top: 0.2rem; }
  .creadings { display: flex; flex-wrap: wrap; gap: 0.7rem; margin-top: 0.3rem; font-family: var(--mono); font-size: 0.8rem; }
  .creadings .rd { color: var(--text); }
  .creadings .rl { font-family: var(--han); color: var(--faint); margin-right: 0.15rem; }
  .variants { margin-top: 0.3rem; display: flex; flex-direction: column; gap: 0.15rem; font-size: 0.8rem; }
  .vedge { display: inline-flex; align-items: center; gap: 0.2rem; }
  .vedge b { font-family: var(--han); }
  .badges { display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.2rem 0 0.5rem; }
  .obadge { font-size: 0.7rem; padding: 0.1rem 0.45rem; border: 1px solid var(--border-strong); border-radius: var(--r); color: var(--text); text-transform: uppercase; letter-spacing: 0.04em; }
  .ety { font-size: 0.9rem; color: var(--muted); line-height: 1.55; margin: 0.2rem 0 0; }
  .links { display: flex; flex-direction: column; gap: 0.2rem; }
  .link { display: flex; gap: 0.6rem; align-items: baseline; text-align: left; padding: 0.35rem 0.4rem; }
  .link:hover { background: var(--surface-2); border: none; }
  .lhead { font-family: var(--han); font-size: 1.15rem; flex: none; }
  .lread { font-family: var(--mono); color: var(--muted); font-size: 0.8rem; flex: none; }
  .lg { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rel { font-size: 0.65rem; padding: 0.05rem 0.35rem; border-radius: 5px; flex: none; text-transform: uppercase; letter-spacing: 0.04em; }
  .rel.cog { border: 1px solid var(--border-strong); color: var(--muted); }
  /* false friend stands out (monochrome): inverted chip */
  .rel.ff { background: var(--text); color: var(--bg); font-weight: 700; }
  .concept { font-size: 0.7rem; color: var(--faint); border: 1px solid var(--border); border-radius: 5px; padding: 0 0.3rem; flex: none; }
</style>
