<script lang="ts">
  import { search, entry as fetchEntry } from './lib/api'
  import type { Entry, Hit } from './lib/types'
  import { primaryForm, varietyLabel, regionsOf, shortGloss } from './lib/display'
  import InputSheet from './lib/InputSheet.svelte'
  import Unified from './lib/Unified.svelte'
  import { Search, X, Brush, Camera } from '@lucide/svelte'
  import { onMount } from 'svelte'

  let q = $state('')
  let results = $state<Hit[]>([])
  let classified = $state('')
  let entry = $state<Entry | null>(null)
  let enrichEntry = $state<Entry | null>(null)
  let unified = $state(false)
  let view = $state<'results' | 'entry'>('results')
  let inputOpen = $state(false)
  let inputMode = $state<'draw' | 'photo'>('draw')
  let loading = $state(false)
  let err = $state('')
  let searched = $state(false)

  let composing = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let ctrl: AbortController | undefined

  type NavMode = 'push' | 'replace' | 'none'
  const resultsUrl = (t: string) => (t ? `?q=${encodeURIComponent(t)}` : location.pathname)

  async function doSearch(query: string, mode: NavMode = 'push') {
    const term = query.trim()
    q = query
    view = 'results'
    entry = null
    enrichEntry = null
    unified = false
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
      const res = await search(term, undefined, ctrl.signal)
      results = res.results
      classified = res.classified_as
      searched = true
      // Han queries resolve to one word seen across languages — show the unified view directly,
      // no list step. Enrich it with the top lexeme's decomposition + origin in the background.
      if (res.classified_as === 'han' && results.length) {
        unified = true
        const topId = results[0].lexeme_id
        fetchEntry(topId)
          .then((e) => {
            if (q.trim() === term && unified) enrichEntry = e
          })
          .catch(() => {})
      }
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
    timer = setTimeout(() => doSearch(v, 'replace'), 180)
  }

  async function openEntry(id: number, mode: NavMode = 'push') {
    loading = true
    err = ''
    try {
      entry = await fetchEntry(id)
      unified = false
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

  function openInput(m: 'draw' | 'photo') {
    inputMode = m
    inputOpen = true
  }

  function clearSearch() {
    q = ''
    results = []
    entry = null
    enrichEntry = null
    unified = false
    searched = false
    err = ''
    history.replaceState({ view: 'results', q: '' }, '', location.pathname)
  }

  // tapping the logo resets everything to a clean home (and drops the back button)
  function goHome() {
    inputOpen = false
    entry = null
    view = 'results'
    clearSearch()
  }

  function fromInput(text: string) {
    inputOpen = false
    doSearch(text)
  }
</script>

<div class="wrap">
  <header class="bar">
    <h1 class="brand">
      <button class="brandbtn" onclick={goHome} aria-label="home"><span class="mark">古古</span> <span class="word">Kogu</span></button>
    </h1>
  </header>

  <div class="searchrow">
    <div class="field">
      <span class="searchicon" aria-hidden="true"><Search size={17} /></span>
      <input
        type="text"
        aria-label="Search by hanzi, kanji, pinyin, jyutping, kana, or English"
        placeholder="character · reading · meaning"
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
      {#if q}
        <button class="clearbtn" aria-label="clear search" onclick={clearSearch} data-testid="clear"><X size={17} /></button>
      {/if}
    </div>
    <button class="rowbtn" aria-label="draw a character" title="draw" onclick={() => openInput('draw')} data-testid="draw-toggle"><Brush size={18} /></button>
    <button class="rowbtn" aria-label="photo or image" title="photo / image" onclick={() => openInput('photo')} data-testid="scan-toggle"><Camera size={18} /></button>
  </div>

  {#if inputOpen}
    <InputSheet mode={inputMode} onpick={fromInput} onclose={() => (inputOpen = false)} />
  {/if}

  {#if err}<div class="err">{err}</div>{/if}

  {#if view === 'entry' && entry}
    {#key entry.lexeme_id}
      <Unified entry={entry} anchor={q} onsearch={doSearch} />
    {/key}
  {:else if unified && results.length}
    <Unified hits={results} entry={enrichEntry} anchor={q} onsearch={doSearch} />
  {:else}
    {#if searched && !loading && results.length}
      <div class="meta">{results.length} {results.length === 1 ? 'result' : 'results'}</div>
    {/if}
    <ul class="results" data-testid="results">
      {#each results as r (r.lexeme_id)}
        {@const d = primaryForm(r.forms, r.variety, q)}
        <li>
          <button class="hit" onclick={() => openEntry(r.lexeme_id)}>
            <span class="hw">
              {d?.primary.form ?? r.headword}{#if d?.alternate}<span class="alt">{d.alternate.form}</span>{/if}
            </span>
            <span class="meta-col">
              <span class="line1">
                {#if r.reading}<span class="rd">{r.reading}</span>{/if}
                <span class="var">{varietyLabel(r.variety)}</span>
                {#each regionsOf(r) as rg}<span class="rg">{rg}</span>{/each}
              </span>
              <span class="gl">{shortGloss(r.glosses)}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    {#if searched && !loading && results.length === 0}
      <div class="empty">nothing for “{q}”.</div>
    {/if}
    {#if !searched && !q}
      <div class="intro">
        <p class="tag">One word, read across <b>中文</b> · <b>粵語</b> · <b>日本語</b>.</p>
        <p class="tag2">Type a character, a reading (pinyin · jyutping · kana), or English — or draw or photograph one.</p>
      </div>
    {/if}
  {/if}
</div>

<style>
  .wrap {
    max-width: 680px;
    margin: 0 auto;
    padding: calc(1.4rem + env(safe-area-inset-top)) calc(1.15rem + env(safe-area-inset-right))
      calc(4rem + env(safe-area-inset-bottom)) calc(1.15rem + env(safe-area-inset-left));
  }
  .bar { margin-bottom: 1rem; }
  .brand { margin: 0; font-weight: 400; }
  .brandbtn { display: inline-flex; align-items: baseline; gap: 0.45rem; background: none; border: none; padding: 0; }
  .brandbtn:hover { background: none; }
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 1.4rem; letter-spacing: -0.04em; color: var(--text); }
  .brand .word { font-family: var(--sans); font-size: 1.05rem; letter-spacing: 0.06em; color: var(--muted); }

  .searchrow { display: flex; gap: 0.4rem; align-items: stretch; margin-bottom: 1.5rem; }
  .field { position: relative; flex: 1; min-width: 0; display: flex; }
  .searchicon { position: absolute; left: 0.8rem; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; display: flex; }
  .field input {
    width: 100%; padding: 0.6rem 2.4rem 0.6rem 2.4rem; font-size: 1.05rem; font-family: var(--sans);
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg);
  }
  .field input:focus { border-color: var(--border-strong); background: var(--surface-2); }
  .field input::placeholder { color: var(--faint); }
  .clearbtn {
    position: absolute; right: 0.45rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
  }
  .clearbtn:hover { color: #fff; background: var(--surface-2); }
  .rowbtn {
    flex: none; display: inline-flex; align-items: center; justify-content: center; padding: 0 0.75rem;
    color: var(--muted); background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg);
  }
  .rowbtn:hover { color: #fff; border-color: var(--border-strong); background: var(--surface-2); }

  .meta { color: var(--faint); font-size: 0.7rem; margin-bottom: 0.6rem; font-family: var(--mono); text-transform: uppercase; letter-spacing: 0.1em; }
  .err { color: var(--text); margin: 0.5rem 0; }

  /* results — an editorial list: big serif headword, quiet meta column */
  .results { list-style: none; margin: 0; padding: 0; }
  .results li + li { border-top: 1px solid var(--border); }
  .hit {
    display: flex; align-items: center; justify-content: flex-start; gap: 0.9rem; width: 100%; text-align: left;
    background: none; border: none; border-radius: var(--r); padding: 0.7rem 0.5rem;
  }
  .hit:hover { background: var(--surface); color: var(--text); }
  .hw { font-family: var(--han); font-size: 1.7rem; line-height: 1.05; flex: none; min-width: 2.2em; }
  .hw .alt { color: var(--faint); font-size: 0.95rem; margin-left: 0.35rem; }
  .meta-col { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; flex: 1; }
  .line1 { display: flex; align-items: baseline; gap: 0.5rem; }
  .rd { font-family: var(--mono); color: var(--text); font-size: 0.8rem; }
  .var { font-family: var(--han); font-size: 0.72rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.25rem; }
  .rg { font-size: 0.6rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.2rem; font-family: var(--mono); }
  .gl { color: var(--muted); font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { color: var(--faint); padding: 1.2rem 0; }
  .intro { padding: 1.5rem 0.2rem; }
  .tag { font-family: var(--sans); font-size: 1.35rem; line-height: 1.5; color: var(--text); margin: 0 0 0.8rem; }
  .tag b { font-family: var(--han); font-weight: 500; }
  .tag2 { color: var(--faint); font-size: 0.95rem; line-height: 1.6; margin: 0; max-width: 32ch; }
</style>
