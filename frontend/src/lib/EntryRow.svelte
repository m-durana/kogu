<script lang="ts">
  import Glyph from './Glyph.svelte'

  // The ONE row style used for every entry list in the app: the results list, saved/history, the
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
    note = '',
    notePrimary = false,
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
    // optional caption under the gloss (the homepage "interesting" showcase uses it for the "why");
    // empty by default, so every other list renders exactly as before.
    note?: string
    // when true (the showcase), the note IS the point: render it as clear primary text (up to two
    // lines) and let the reading recede, since WHY an entry is shown matters more than how it sounds.
    notePrimary?: boolean
  } = $props()

  // long headwords (idioms, katakana loanwords) step the type down so the row still shows the
  // reading and gloss instead of one giant ellipsised glyph column
  const hwSize = $derived([...glyph].length > 12 ? '1.05rem' : [...glyph].length > 6 ? '1.3rem' : '1.7rem')
</script>

<li>
  <button class="hit" {onclick} title={title || `look up ${glyph}`}>
    <span class="hw" {lang} style="font-family:{font};font-size:{hwSize}"><Glyph ch={glyph} {font} {lang} />{#if alt}<span class="alt">{alt}</span>{/if}</span>
    <span class="meta-col" class:np={notePrimary}>
      <span class="line1">
        {#if reading && reading !== glyph}<span class="rd">{reading}</span>{/if}
        {#if tags.length || regions.length}<span class="tags">{#each tags as t}<span class="var">{t}</span>{/each}{#each regions as rg}<span class="rg">{rg}</span>{/each}</span>{/if}
      </span>
      {#if gloss}<span class="gl">{gloss}</span>{/if}
      {#if note}<span class="note" class:primary={notePrimary}>{note}</span>{/if}
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
  .hw { font-family: var(--han); line-height: 1.05; flex: none; min-width: 2.2em; max-width: 8.5em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* the phone cap is far too tight for the 780px desktop list: コントラクトブリッジ was ellipsised
     with half the row empty. Let long headwords breathe where there is room. */
  @media (min-width: 1100px) { .hw { max-width: 16em; } }
  .hw .alt { color: var(--faint); font-size: 0.95rem; margin-left: 0.35rem; }
  .meta-col { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; flex: 1; }
  /* keep the reading line to ONE line: a long reading ellipsises rather than wrapping the row to a
     second line (no entry longer than one line). */
  .line1 { display: flex; align-items: baseline; gap: 0.5rem; flex-wrap: nowrap; min-width: 0; }
  .rd { font-family: var(--mono); color: var(--muted); font-size: 0.78rem; min-width: 0; flex: 0 1 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Hybrid: variety/region as plain labels after a hairline divider, not bordered chips */
  .tags { display: inline-flex; align-items: baseline; gap: 0.35rem; flex: none; }
  .rd + .tags { border-left: 1px solid var(--border-strong); padding-left: 0.5rem; margin-left: 0.05rem; }
  .var { font-family: var(--han); font-size: 0.78rem; color: var(--faint); }
  .rg { font-size: 0.62rem; color: var(--faint); font-family: var(--mono); }
  .gl { color: var(--muted); font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* the "why this is interesting" caption (homepage showcase only); quiet + italic so it reads as a note */
  .note { color: var(--faint); font-size: 0.72rem; font-style: italic; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* showcase "why": the reason an entry is noteworthy is the point, so make it clear primary text
     (readable colour, upright, wraps to two lines) and let the reading fade back below it. */
  .note.primary { color: var(--text); font-size: 0.82rem; font-style: normal; line-height: 1.35; white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .meta-col.np { gap: 0.25rem; }
  .meta-col.np .rd { color: var(--faint); }
  .meta-col.np .gl { font-size: 0.82rem; color: var(--faint); }
</style>
