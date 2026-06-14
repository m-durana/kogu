<script lang="ts">
  import { search, entry as fetchEntry } from './lib/api'
  import type { Entry, Hit } from './lib/types'
  import { primaryForm, varietyLabel, regionsOf, shortGloss } from './lib/display'
  import InputSheet from './lib/InputSheet.svelte'
  import EntryView from './lib/Entry.svelte'
  import { Search, Plus, X, ArrowLeft } from '@lucide/svelte'
  import { onMount } from 'svelte'

  let q = $state('')
  let results = $state<Hit[]>([])
  let classified = $state('')
  let entry = $state<Entry | null>(null)
  let view = $state<'results' | 'entry'>('results')
  let inputOpen = $state(false)
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

  const goBack = () => history.back()

  function fromInput(text: string) {
    inputOpen = false
    doSearch(text)
  }
</script>

<div class="wrap">
  <header class="bar">
    <h1 class="brand"><span class="mark">古古</span> <span class="word">Kogu</span></h1>
  </header>

  <div class="searchrow" class:open={inputOpen}>
    <span class="searchicon" aria-hidden="true"><Search size={18} /></span>
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
    <button
      class="inputbtn"
      aria-label={inputOpen ? 'close input methods' : 'draw or photograph a character'}
      aria-pressed={inputOpen}
      title="draw / photo"
      onclick={() => (inputOpen = !inputOpen)}
      data-testid="input-toggle"
    >
      {#if inputOpen}<X size={18} />{:else}<Plus size={18} />{/if}
    </button>
  </div>

  {#if inputOpen}
    <InputSheet onpick={fromInput} onclose={() => (inputOpen = false)} />
  {/if}

  {#if err}<div class="err">{err}</div>{/if}

  {#if view === 'entry' && entry}
    <button class="back" onclick={goBack} data-testid="back"><ArrowLeft size={15} aria-hidden="true" /> back</button>
    <EntryView {entry} anchor={q} onsearch={doSearch} />
  {:else}
    {#if searched && !loading}
      <div class="meta">{results.length} · {classified}</div>
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
  {/if}
</div>

<style>
  .wrap {
    max-width: 680px;
    margin: 0 auto;
    padding: calc(1.4rem + env(safe-area-inset-top)) calc(1.15rem + env(safe-area-inset-right))
      calc(4rem + env(safe-area-inset-bottom)) calc(1.15rem + env(safe-area-inset-left));
  }
  .bar { margin-bottom: 1.1rem; }
  .brand { margin: 0; font-weight: 400; display: flex; align-items: baseline; gap: 0.45rem; }
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 1.4rem; letter-spacing: -0.04em; }
  .brand .word { font-family: var(--sans); font-size: 1.05rem; letter-spacing: 0.06em; color: var(--muted); }

  .searchrow { position: relative; margin-bottom: 1.6rem; }
  .searchicon { position: absolute; left: 1rem; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; display: flex; }
  .searchrow input {
    padding: 0.95rem 3.2rem 0.95rem 3rem; font-size: 1.3rem; font-family: var(--sans);
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg);
  }
  .searchrow input:focus { border-color: var(--border-strong); background: var(--surface-2); }
  .searchrow input::placeholder { font-style: italic; color: var(--faint); }
  .inputbtn {
    position: absolute; right: 0.5rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.45rem; border-radius: var(--r);
  }
  .inputbtn:hover { color: #fff; background: var(--surface-2); }
  .inputbtn[aria-pressed='true'] { color: #fff; }

  .meta { color: var(--faint); font-size: 0.7rem; margin-bottom: 0.6rem; font-family: var(--mono); text-transform: uppercase; letter-spacing: 0.1em; }
  .err { color: var(--text); margin: 0.5rem 0; }

  /* results — an editorial list: big serif headword, quiet meta column */
  .results { list-style: none; margin: 0; padding: 0; }
  .results li + li { border-top: 1px solid var(--border); }
  .hit {
    display: flex; align-items: baseline; gap: 1rem; width: 100%; text-align: left;
    background: none; border: none; border-radius: var(--r); padding: 0.85rem 0.5rem;
  }
  .hit:hover { background: var(--surface); color: var(--text); }
  .hw { font-family: var(--han); font-size: 1.75rem; line-height: 1.05; flex: none; min-width: 2.4em; }
  .hw .alt { color: var(--faint); font-size: 0.95rem; margin-left: 0.35rem; }
  .meta-col { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; flex: 1; }
  .line1 { display: flex; align-items: baseline; gap: 0.5rem; }
  .rd { font-family: var(--mono); color: var(--text); font-size: 0.8rem; }
  .var { font-family: var(--han); font-size: 0.72rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.25rem; }
  .rg { font-size: 0.6rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.2rem; font-family: var(--mono); }
  .gl { color: var(--muted); font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .back { display: inline-flex; align-items: center; gap: 0.3rem; margin-bottom: 1rem; background: none; border: none; color: var(--muted); padding: 0.3rem 0; }
  .back:hover { color: #fff; background: none; }
  .empty { color: var(--faint); padding: 1.2rem 0; font-style: italic; }
</style>
