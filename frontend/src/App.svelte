<script lang="ts">
  import { search, entry as fetchEntry } from './lib/api'
  import type { Entry, Hit, PrefScript } from './lib/types'
  import { pickForms, matchLabel, varietyLabel, regionsOf, shortGloss } from './lib/display'
  import Pad from './lib/Pad.svelte'
  import Ocr from './lib/Ocr.svelte'
  import EntryView from './lib/Entry.svelte'
  import { Search, Brush, Camera, ArrowLeft } from '@lucide/svelte'
  import { onMount } from 'svelte'

  let q = $state('')
  let results = $state<Hit[]>([])
  let classified = $state('')
  let entry = $state<Entry | null>(null)
  let view = $state<'results' | 'entry'>('results')
  let pref = $state<PrefScript>('trad')
  let pad = $state(false)
  let scan = $state(false)
  let loading = $state(false)
  let err = $state('')
  let searched = $state(false)

  let composing = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let ctrl: AbortController | undefined

  // --- history-based navigation: makes the browser / PWA / device back button work ---
  type NavMode = 'push' | 'replace' | 'none'

  function resultsUrl(term: string) {
    return term ? `?q=${encodeURIComponent(term)}` : location.pathname
  }

  async function doSearch(query: string, mode: NavMode = 'push') {
    const term = query.trim()
    q = query
    view = 'results'
    entry = null
    if (!term) {
      results = []
      searched = false
      if (mode !== 'none') history.replaceState({ view: 'results', q: '' }, '', location.pathname)
      return
    }
    if (mode === 'push') history.pushState({ view: 'results', q: term }, '', resultsUrl(term))
    else if (mode === 'replace') history.replaceState({ view: 'results', q: term }, '', resultsUrl(term))
    ctrl?.abort()
    ctrl = new AbortController()
    loading = true
    err = ''
    try {
      const res = await search(term, pref, ctrl.signal)
      results = res.results
      classified = res.classified_as
      searched = true
    } catch (e) {
      if ((e as Error).name !== 'AbortError') err = 'search failed'
    } finally {
      loading = false
    }
  }

  function onInput(e: Event) {
    const v = (e.target as HTMLInputElement).value
    q = v
    if (composing) return
    clearTimeout(timer)
    // live typing updates the current history entry instead of stacking new ones
    timer = setTimeout(() => doSearch(v, 'replace'), 180)
  }

  async function openEntry(id: number, mode: NavMode = 'push') {
    loading = true
    err = ''
    try {
      entry = await fetchEntry(id)
      view = 'entry'
      if (mode === 'push') history.pushState({ view: 'entry', id, q }, '', `#/entry/${id}`)
    } catch {
      err = 'could not load entry'
    } finally {
      loading = false
    }
  }

  function onPop(e: PopStateEvent) {
    const st = e.state as { view?: string; id?: number; q?: string } | null
    if (!st || st.view === 'results') {
      view = 'results'
      entry = null
      const term = st?.q ?? ''
      if (term && term !== q) doSearch(term, 'none')
      else q = term
    } else if (st.view === 'entry' && st.id != null) {
      openEntry(st.id, 'none')
    }
  }

  onMount(() => {
    window.addEventListener('popstate', onPop)
    const term = new URLSearchParams(location.search).get('q')
    if (term) doSearch(term, 'replace')
    else history.replaceState({ view: 'results', q: '' }, '', location.pathname)
    return () => window.removeEventListener('popstate', onPop)
  })

  function goBack() {
    history.back()
  }

  function fromPad(ch: string) {
    pad = false
    doSearch(ch)
  }

  function fromOcr(text: string) {
    scan = false
    doSearch(text)
  }
</script>

<div class="wrap">
  <header class="bar">
    <h1 class="brand">
      <span class="mark">古古</span>
      <span class="word">Kogu</span>
    </h1>
    <div class="controls">
      <div class="toggle" role="group" aria-label="Chinese script">
        <button aria-pressed={pref === 'trad'} title="Traditional Chinese" onclick={() => (pref = 'trad')}>
          <span class="cjk">繁</span> Trad
        </button>
        <button aria-pressed={pref === 'simp'} title="Simplified Chinese" onclick={() => (pref = 'simp')}>
          <span class="cjk">简</span> Simp
        </button>
      </div>
      <button aria-pressed={scan} aria-label="scan an image" title="scan an image" onclick={() => (scan = !scan)} data-testid="scan-toggle" class="ic">
        <Camera size={18} aria-hidden="true" />
      </button>
      <button aria-pressed={pad} aria-label="draw a character" title="draw a character" onclick={() => (pad = !pad)} data-testid="draw-toggle" class="ic">
        <Brush size={18} aria-hidden="true" />
      </button>
    </div>
  </header>

  <div class="searchrow">
    <span class="searchicon" aria-hidden="true"><Search size={18} /></span>
    <input
      type="text"
      aria-label="Search by hanzi, kanji, pinyin, jyutping, kana, or English"
      placeholder="hanzi · kanji · pinyin · jyutping · kana · english"
      value={q}
      oninput={onInput}
      oncompositionstart={() => (composing = true)}
      oncompositionend={(e) => {
        composing = false
        doSearch((e.target as HTMLInputElement).value, 'replace')
      }}
      onkeydown={(e) => e.key === 'Enter' && doSearch(q)}
      data-testid="search-input"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
    />
  </div>

  {#if scan}
    <div class="padwrap"><Ocr onpick={fromOcr} /></div>
  {/if}

  {#if pad}
    <div class="padwrap"><Pad onpick={fromPad} /></div>
  {/if}

  {#if err}<div class="err">{err}</div>{/if}

  {#if view === 'entry' && entry}
    <button class="back iconbtn" onclick={goBack} data-testid="back">
      <ArrowLeft size={15} aria-hidden="true" /> results
    </button>
    <EntryView {entry} {pref} onsearch={doSearch} />
  {:else}
    {#if searched && !loading}
      <div class="meta">{results.length} results · {classified}</div>
    {/if}
    <ul class="results" data-testid="results">
      {#each results as r (r.lexeme_id)}
        {@const d = pickForms(r.forms, r.variety, pref)}
        {@const m = matchLabel(r.match_type)}
        <li>
          <button class="hit" onclick={() => openEntry(r.lexeme_id)}>
            <span class="var v-{r.variety}">{varietyLabel(r.variety)}</span>
            <span class="hw">
              {d?.primary.form ?? r.headword}{#if d?.alternate}<span class="alt">［{d.alternate.form}］</span>{/if}
            </span>
            {#if r.reading}<span class="rd">{r.reading}</span>{/if}
            <span class="gl">{shortGloss(r.glosses)}</span>
            <span class="tags">
              {#each regionsOf(r) as rg}<span class="rg">{rg}</span>{/each}
              <span class="mt {m.cls}">{m.label}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    {#if searched && !loading && results.length === 0}
      <div class="empty">no matches for “{q}”.</div>
    {/if}
  {/if}
</div>

<style>
  .wrap {
    max-width: 760px;
    margin: 0 auto;
    /* respect the notch / status bar / home indicator so it doesn't sit under them */
    padding: calc(1.2rem + env(safe-area-inset-top)) calc(1rem + env(safe-area-inset-right)) calc(4rem + env(safe-area-inset-bottom)) calc(1rem + env(safe-area-inset-left));
  }
  .bar { display: flex; justify-content: space-between; align-items: center; gap: 0.6rem 0.8rem; flex-wrap: wrap; margin-bottom: 1.2rem; }
  .brand { display: flex; align-items: baseline; gap: 0.5rem; margin: 0; font-weight: 400; flex-shrink: 0; }
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 1.5rem; letter-spacing: -0.04em; line-height: 1; white-space: nowrap; }
  .brand .word { font-family: var(--sans); font-size: 1.15rem; letter-spacing: 0.02em; color: var(--muted); }
  .controls { display: flex; gap: 0.4rem; align-items: center; }
  .ic { display: inline-flex; align-items: center; justify-content: center; padding: 0.5rem 0.6rem; }
  .iconbtn { display: inline-flex; align-items: center; gap: 0.3rem; }
  .searchrow { position: relative; margin-bottom: 1.4rem; }
  .searchicon { position: absolute; left: 0.95rem; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; display: flex; }
  .searchrow input { padding: 0.85rem 0.9rem 0.85rem 2.9rem; font-size: 1.35rem; background: var(--surface); border: 1px solid var(--border); }
  .searchrow input:focus { border-color: var(--border-strong); background: var(--surface-2); }
  .toggle { display: inline-flex; gap: 2px; padding: 3px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); }
  .toggle button { border: none; background: transparent; color: var(--muted); font-family: var(--sans); font-size: 0.8rem; padding: 0.35rem 0.6rem; border-radius: calc(var(--r-lg) - 5px); }
  .toggle button .cjk { font-family: var(--han); font-size: 1.05rem; }
  .toggle button:hover { background: var(--surface-2); color: #fff; }
  .toggle button[aria-pressed='true'] { background: var(--border-strong); color: #fff; }
  .padwrap { margin-bottom: 1.2rem; }
  .meta { color: var(--faint); font-size: 0.75rem; margin-bottom: 0.5rem; font-family: var(--mono); text-transform: uppercase; }
  .err { color: var(--accent); margin: 0.5rem 0; }
  .results { list-style: none; margin: 0; padding: 0; }
  .results li { border-bottom: 1px solid var(--border); }
  .hit { display: flex; align-items: baseline; gap: 0.7rem; width: 100%; text-align: left; background: none; border: none; border-radius: var(--r); padding: 0.6rem 0.5rem; }
  .hit:hover { background: var(--surface); border: none; color: var(--text); }
  .var { font-size: 0.7rem; padding: 0.05rem 0.3rem; border: 1px solid var(--border-strong); border-radius: 5px; color: var(--muted); flex: none; }
  .v-zh { color: var(--zh); border-color: var(--accent-dim); }
  .v-ja { color: var(--ja); }
  .v-yue { color: var(--yue); }
  .hw { font-family: var(--han); font-size: 1.4rem; flex: none; }
  .alt { color: var(--faint); font-size: 0.95rem; }
  .rd { font-family: var(--mono); color: var(--accent); font-size: 0.85rem; flex: none; }
  .gl { color: var(--muted); font-size: 0.85rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .tags { display: flex; gap: 0.3rem; flex: none; }
  .rg { font-size: 0.65rem; color: var(--faint); border: 1px solid var(--border); border-radius: 5px; padding: 0 0.25rem; font-family: var(--mono); }
  .mt { font-size: 0.65rem; padding: 0 0.25rem; border: 1px solid var(--border); border-radius: 5px; font-family: var(--mono); color: var(--faint); }
  .m-exact { color: var(--text); border-color: var(--border-strong); }
  .m-english { color: var(--muted); }
  .back { margin-bottom: 0.8rem; }
  .empty { color: var(--faint); padding: 1rem 0; }
</style>
