<script lang="ts">
  import Glyph from './Glyph.svelte'

  // The ONE row style used for every entry list in the app — the results list, saved/history, the
  // cross-language bridges (usually written / written differently / related), the "used in" words,
  // and a jukugo's component characters. Big serif headword on the left, a quiet meta column on the
  // right (reading + language tag(s) + region, then the gloss). Keeping it in one component is what
  // makes the lists genuinely identical instead of three near-duplicate layouts.
  let {
    glyph,
    font = 'var(--han)',
    lang = undefined,
    alt = null,
    reading = '',
    tags = [],
    regions = [],
    gloss = '',
    onclick,
    title = '',
  }: {
    glyph: string
    font?: string
    lang?: string | undefined
    alt?: string | null
    reading?: string
    tags?: string[]
    regions?: string[]
    gloss?: string
    onclick: () => void
    title?: string
  } = $props()
</script>

<li>
  <button class="hit" {onclick} title={title || `look up ${glyph}`}>
    <span class="hw" {lang} style="font-family:{font}"><Glyph ch={glyph} {font} {lang} />{#if alt}<span class="alt">{alt}</span>{/if}</span>
    <span class="meta-col">
      <span class="line1">
        {#if reading}<span class="rd">{reading}</span>{/if}
        {#if tags.length || regions.length}<span class="tags">{#each tags as t}<span class="var">{t}</span>{/each}{#each regions as rg}<span class="rg">{rg}</span>{/each}</span>{/if}
      </span>
      {#if gloss}<span class="gl">{gloss}</span>{/if}
    </span>
  </button>
</li>

<style>
  /* a hairline rule BETWEEN rows (none above the first), matching the search results list exactly */
  li { border-top: 1px solid var(--border); }
  li:first-child { border-top: none; }
  .hit {
    display: flex; align-items: center; justify-content: flex-start; gap: 0.9rem; width: 100%; text-align: left;
    background: none; border: none; border-radius: var(--r); padding: 0.7rem 0.5rem;
  }
  .hit:hover { background: var(--surface); color: var(--text); }
  /* headword: reserve a 2.2em column for short glyphs, but cap long headwords (idioms like
     親の意見と茄子の花は千に一つも無駄はない) and ellipsise them so a row never widens the layout. */
  .hw { font-family: var(--han); font-size: 1.7rem; line-height: 1.05; flex: none; min-width: 2.2em; max-width: 8.5em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hw .alt { color: var(--faint); font-size: 0.95rem; margin-left: 0.35rem; }
  .meta-col { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; flex: 1; }
  .line1 { display: flex; align-items: baseline; gap: 0.5rem; flex-wrap: wrap; }
  .rd { font-family: var(--mono); color: var(--muted); font-size: 0.78rem; }
  /* Hybrid: variety/region as plain labels after a hairline divider, not bordered chips */
  .tags { display: inline-flex; align-items: baseline; gap: 0.35rem; }
  .rd + .tags { border-left: 1px solid var(--border-strong); padding-left: 0.5rem; margin-left: 0.05rem; }
  .var { font-family: var(--han); font-size: 0.78rem; color: var(--faint); }
  .rg { font-size: 0.62rem; color: var(--faint); font-family: var(--mono); }
  .gl { color: var(--muted); font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
