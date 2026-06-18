<script lang="ts">
  import { search, entry as fetchEntry } from './lib/api'
  import type { Entry, Hit, CharInfo } from './lib/types'
  import { primaryForm, varietyLabel, regionsOf, shortGloss, cleanGloss, langTag, hanFont } from './lib/display'
  import Unified from './lib/Unified.svelte'
  import Pad from './lib/Pad.svelte'
  import Ocr from './lib/Ocr.svelte'
  import { Search, X, Brush, Camera, Bookmark, Clock, Share2, Trash2 } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { getSaved, getHistory, isSaved, toggleSaved, recordHistory, clearHistory, type SavedItem } from './lib/store'

  let q = $state('')
  let results = $state<Hit[]>([])
  let classified = $state('')
  let entry = $state<Entry | null>(null)
  let enrichEntry = $state<Entry | null>(null)
  let enriching = $state(false)
  let unified = $state(false)
  let view = $state<'results' | 'entry' | 'saved' | 'history'>('results')
  // saved (bookmarks) + history lists, loaded from localStorage when their view opens
  let savedList = $state<SavedItem[]>([])
  let historyList = $state<SavedItem[]>([])
  let savedNow = $state(false) // is the currently shown word bookmarked
  let toast = $state('') // transient "Link copied" confirmation for share
  // inline input panel below the search row: 'draw' shows the pad; 'photo' shows the picked image
  let panel = $state<'none' | 'draw' | 'photo'>('none')
  let ocrFile = $state<File | null>(null)
  let fileInput: HTMLInputElement
  let loading = $state(false)
  let err = $state('')
  let searched = $state(false)
  // When a Han query yields no word, we break it into its component characters (char-only entries).
  let breakdown = $state<CharInfo[]>([])
  // true while that per-character breakdown is still being fetched (suppresses a "nothing found" flash)
  let breaking = $state(false)

  const HAN = /\p{Script=Han}/u
  // easter egg: 古古 ("old old") is the app's name, not a real word — shown when looked up (item 8)
  const isEasterEgg = $derived(q.trim() === '古古')

  // ── save / history / share (item 7) ────────────────────────────────────────────────────────────
  // the word currently on screen (a full entry, an enriching unified entry, or the top unified hit)
  const currentItem = $derived.by((): SavedItem | null => {
    const e = entry ?? enrichEntry
    if (e) return { id: e.lexeme_id, headword: e.headword, reading: e.reading, variety: e.variety, gloss: e.senses?.[0]?.gloss_en ?? null, ts: 0 }
    if (unified && results.length) {
      const r = results[0]
      return { id: r.lexeme_id, headword: r.headword, reading: r.reading, variety: r.variety, gloss: r.glosses?.[0] ?? null, ts: 0 }
    }
    return null
  })
  const canSaveShare = $derived(currentItem != null && (view === 'entry' || (unified && results.length > 0)))

  // record each visited word in history, and keep the bookmark toggle in sync with what's shown
  $effect(() => {
    const it = currentItem
    if (it && (view === 'entry' || unified)) {
      recordHistory(it)
      savedNow = isSaved(it.id)
    }
  })

  function toggleSave() {
    if (!currentItem) return
    savedNow = toggleSaved(currentItem)
  }

  // share a direct link: a readable ?q=headword for words, #/entry/<id> for char-only (negative id)
  async function shareCurrent() {
    if (!currentItem) return
    const it = currentItem
    const path = it.id < 0 ? `#/entry/${it.id}` : `?q=${encodeURIComponent(it.headword)}`
    const url = `${location.origin}/${path}`
    try {
      if (navigator.share) await navigator.share({ title: `${it.headword} · Kogu`, url })
      else {
        await navigator.clipboard.writeText(url)
        flash('Link copied')
      }
    } catch (e) {
      if ((e as Error).name !== 'AbortError') {
        try {
          await navigator.clipboard.writeText(url)
          flash('Link copied')
        } catch {
          /* nothing else to do */
        }
      }
    }
  }
  let toastTimer: ReturnType<typeof setTimeout>
  function flash(msg: string) {
    toast = msg
    clearTimeout(toastTimer)
    toastTimer = setTimeout(() => (toast = ''), 1800)
  }

  function openSaved() {
    savedList = getSaved()
    view = 'saved'
    panel = 'none'
    history.pushState({ view: 'saved' }, '', '#/saved')
  }
  function openHistory() {
    historyList = getHistory()
    view = 'history'
    panel = 'none'
    history.pushState({ view: 'history' }, '', '#/history')
  }
  function wipeHistory() {
    clearHistory()
    historyList = []
  }

  // Render typed CJK in the same regional serif as the headword it resolves to (a Japanese word's 誤
  // shouldn't show the Simplified-Chinese glyph in the box while the headword shows the Japanese one).
  // Latin stays Newsreader; the CJK fallback follows the top hit's variety once results arrive.
  const queryLang = $derived(results[0]?.variety ?? 'zh')
  const inputFont = $derived(
    queryLang === 'ja'
      ? '"Newsreader", Georgia, var(--han-ja), serif'
      : queryLang === 'yue'
        ? '"Newsreader", Georgia, var(--han-tc), serif'
        : 'var(--sans)',
  )

  // first language-flagged meaning for a component character, kept short
  function charMeaning(c: CharInfo): string {
    const g = cleanGloss(c.gloss_en ?? '')
    return g.split(';')[0].trim()
  }
  function charLangs(c: CharInfo): string[] {
    const tags: string[] = []
    const has = (k: string) => c.readings.some((r) => r.kind === k && r.value)
    if (has('pinyin')) tags.push('中')
    if (has('jyutping')) tags.push('粵')
    if (has('onyomi') || has('kunyomi')) tags.push('日')
    return tags
  }

  let composing = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let ctrl: AbortController | undefined

  type NavMode = 'push' | 'replace' | 'none'
  const resultsUrl = (t: string) => (t ? `?q=${encodeURIComponent(t)}` : location.pathname)

  async function doSearch(query: string, mode: NavMode = 'push') {
    const term = query.trim()
    // already showing this exact query (e.g. tapped the row/character for the page you're on):
    // do nothing, so the view doesn't blank and reload.
    if (term && term === q.trim() && searched && !loading && (results.length || entry)) return
    q = query
    view = 'results'
    entry = null
    enrichEntry = null
    enriching = false
    unified = false
    breakdown = []
    breaking = false
    // clear prior results so a NEW search shows the skeleton, not stale content that then swaps out
    results = []
    if (!term) {
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
      // Han queries resolve to one word seen across languages - show the unified view directly,
      // no list step. Enrich the lexeme whose form is EXACTLY what was typed (so 氣 enriches the
      // Chinese 氣, not the Japanese 気 that may rank first) - this is the word shown in the field, so
      // its senses / words / origin must come from the same lexeme. Falls back to the top hit.
      // Show the unified word view for Han-script words — INCLUDING mixed kanji+kana words like 入り口
      // (which classify as 'kana' because of the り but are real Han-script words), and for ANY single
      // result (no point making the user click a one-item list — "just show me").
      const hanLike = res.classified_as === 'han' || results.some((r) => HAN.test(r.headword))
      if ((hanLike || results.length === 1) && results.length) {
        unified = true
        enriching = true
        const exact =
          results.find((r) => r.headword === term) ??
          results.find((r) => r.forms.some((f) => f.form === term && f.is_primary)) ??
          results[0]
        const topId = exact.lexeme_id
        // tie the enrich fetch to the same abort controller as the search, so a superseded click
        // cancels its in-flight /entry instead of competing for the rate-limit budget (a cause of the
        // spurious "search failed" when clicking through words quickly).
        fetchEntry(topId, ctrl.signal)
          .then((e) => {
            if (q.trim() === term && unified) enrichEntry = e
          })
          .catch(() => {})
          .finally(() => {
            if (q.trim() === term) enriching = false
          })
      } else if (!results.length && HAN.test(term)) {
        // No word matched, but the query is Han - break it into its component characters so the
        // user still gets per-character meanings and can drill into any one. Char-only entries live
        // at /entry/{-codepoint}. Fetch the unique Han chars in parallel; ignore any that fail.
        const chars = [...new Set([...term].filter((c) => HAN.test(c)))]
        // mark a breakdown as pending so we don't flash "nothing found" before it arrives (item 3)
        breaking = true
        Promise.all(
          chars.map((c) =>
            fetchEntry(-c.codePointAt(0)!)
              .then((e) => e.characters[0] ?? null)
              .catch(() => null),
          ),
        ).then((infos) => {
          // only apply if this is still the active query (guard against a newer search)
          if (q.trim() === term) {
            breakdown = infos.filter((c): c is CharInfo => !!c)
            breaking = false
          }
        })
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
    if (view === 'entry' && entry?.lexeme_id === id) return // already on this entry
    try {
      entry = await fetchEntry(id)
      enriching = false
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
    if (st?.view === 'saved') {
      savedList = getSaved()
      view = 'saved'
    } else if (st?.view === 'history') {
      historyList = getHistory()
      view = 'history'
    } else if (st?.view === 'entry' && st.id != null) {
      openEntry(st.id, 'none')
    } else {
      view = 'results'
      entry = null
      const term = st?.q ?? ''
      if (term && term !== q) doSearch(term, 'none')
      else q = term
    }
  }

  onMount(() => {
    window.addEventListener('popstate', onPop)
    // deep link: a shared #/entry/<id> (id may be negative for a char-only page) reopens that entry
    const m = location.hash.match(/^#\/entry\/(-?\d+)$/)
    const term = new URLSearchParams(location.search).get('q')
    if (m) openEntry(Number(m[1]), 'replace')
    else if (term) doSearch(term, 'replace')
    else history.replaceState({ view: 'results', q: '' }, '', location.pathname)
    return () => window.removeEventListener('popstate', onPop)
  })

  // draw: toggle the inline pad. photo: trigger the OS-native picker (Photo Library / Camera /
  // Files menu on iOS) right here — no separate page. The image opens in an inline panel.
  function toggleDraw() {
    panel = panel === 'draw' ? 'none' : 'draw'
  }
  function openPhoto() {
    if (panel === 'photo') {
      panel = 'none'
      ocrFile = null
    } else {
      fileInput.click()
    }
  }
  function onPhotoFile(e: Event) {
    const f = (e.target as HTMLInputElement).files?.[0]
    if (f) {
      ocrFile = f
      panel = 'photo'
    }
    ;(e.target as HTMLInputElement).value = '' // allow re-picking the same file
  }

  function clearSearch() {
    q = ''
    results = []
    entry = null
    enrichEntry = null
    enriching = false
    unified = false
    searched = false
    breakdown = []
    err = ''
    history.replaceState({ view: 'results', q: '' }, '', location.pathname)
  }

  // tapping the logo resets everything to a clean home
  function goHome() {
    panel = 'none'
    ocrFile = null
    entry = null
    view = 'results'
    clearSearch()
  }

  // a character was chosen from the pad or the photo selection — search it and close the panel
  function fromInput(text: string) {
    panel = 'none'
    ocrFile = null
    doSearch(text)
  }
</script>

<div class="wrap">
  <header class="bar">
    <h1 class="brand">
      <button class="brandbtn" onclick={goHome} aria-label="home"><span class="mark">古古</span> <span class="word">Kogu</span></button>
    </h1>
    <nav class="navbtns">
      <button class="navbtn" class:on={view === 'history'} onclick={openHistory} aria-label="history" title="history"><Clock size={18} /></button>
      <button class="navbtn" class:on={view === 'saved'} onclick={openSaved} aria-label="saved" title="saved"><Bookmark size={18} /></button>
    </nav>
  </header>

  <div class="searchrow">
    <div class="field">
      <span class="searchicon" aria-hidden="true"><Search size={17} /></span>
      <input
        type="text"
        lang={langTag(queryLang)}
        style="font-family:{inputFont}"
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
    <button class="rowbtn" class:on={panel === 'draw'} aria-label="draw a character" aria-pressed={panel === 'draw'} title="draw" onclick={toggleDraw} data-testid="draw-toggle"><Brush size={18} /></button>
    <button class="rowbtn" class:on={panel === 'photo'} aria-label="photo or image" title="photo / image" onclick={openPhoto} data-testid="scan-toggle"><Camera size={18} /></button>
    <input bind:this={fileInput} type="file" accept="image/*" onchange={onPhotoFile} hidden />
  </div>

  {#if panel === 'draw'}
    <section class="inputpanel"><Pad onpick={fromInput} onclose={() => (panel = 'none')} /></section>
  {:else if panel === 'photo' && ocrFile}
    <section class="inputpanel"><Ocr file={ocrFile} onpick={fromInput} /></section>
  {/if}

  {#if err}<div class="err">{err}</div>{/if}

  {#if canSaveShare}
    <!-- per-page actions: bookmark + share a direct link (item 7) -->
    <div class="actions">
      <button class="actbtn" class:on={savedNow} onclick={toggleSave} aria-pressed={savedNow} aria-label={savedNow ? 'remove bookmark' : 'save'} title={savedNow ? 'saved' : 'save'}>
        <Bookmark size={16} fill={savedNow ? 'currentColor' : 'none'} /> <span>{savedNow ? 'saved' : 'save'}</span>
      </button>
      <button class="actbtn" onclick={shareCurrent} aria-label="share" title="share">
        <Share2 size={16} /> <span>share</span>
      </button>
    </div>
  {/if}

  {#snippet savedRow(it: SavedItem)}
    <li>
      <button class="hit" onclick={() => openEntry(it.id)}>
        <span class="hw" lang={langTag(it.variety)} style="font-family:{hanFont(it.variety)}">{it.headword}</span>
        <span class="meta-col">
          <span class="line1">
            {#if it.reading}<span class="rd">{it.reading}</span>{/if}
            <span class="var">{varietyLabel(it.variety)}</span>
          </span>
          {#if it.gloss}<span class="gl">{shortGloss([it.gloss])}</span>{/if}
        </span>
      </button>
    </li>
  {/snippet}

  {#snippet pageSkel()}
    <!-- one whole-page placeholder so results appear all at once, never piecemeal-shifting (item 3) -->
    <div class="pskel" aria-hidden="true" data-testid="page-skeleton">
      <div class="ps-line w40"></div>
      <div class="ps-line w70"></div>
      <div class="ps-gap"></div>
      <div class="ps-line w55"></div>
      <div class="ps-line w85"></div>
      <div class="ps-line w60"></div>
    </div>
  {/snippet}

  {#if view === 'saved'}
    <section class="listview">
      <h2 class="lvh">Saved</h2>
      {#if savedList.length}
        <ul class="results">{#each savedList as it (it.id)}{@render savedRow(it)}{/each}</ul>
      {:else}
        <p class="empty">No saved words yet. Open a word and tap save.</p>
      {/if}
    </section>
  {:else if view === 'history'}
    <section class="listview">
      <h2 class="lvh">History {#if historyList.length}<button class="lvclear" onclick={wipeHistory} aria-label="clear history"><Trash2 size={14} /> clear</button>{/if}</h2>
      {#if historyList.length}
        <ul class="results">{#each historyList as it (it.id)}{@render savedRow(it)}{/each}</ul>
      {:else}
        <p class="empty">No history yet.</p>
      {/if}
    </section>
  {:else if view === 'entry' && entry}
    {#key entry.lexeme_id}
      <Unified entry={entry} anchor={q} onsearch={doSearch} />
    {/key}
  {:else if unified && results.length}
    <Unified hits={results} entry={enrichEntry} {enriching} anchor={q} onsearch={doSearch} />
  {:else if loading}
    {@render pageSkel()}
  {:else}
    {#if searched && !loading && results.length}
      <div class="meta">{results.length} {results.length === 1 ? 'result' : 'results'}</div>
    {/if}
    <ul class="results" data-testid="results">
      {#each results as r (r.lexeme_id)}
        {@const d = primaryForm(r.forms, r.variety, q)}
        <li>
          <button class="hit" onclick={() => doSearch(d?.primary.form ?? r.headword)}>
            <span class="hw" lang={langTag(r.variety)} style="font-family:{hanFont(r.variety)}">
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
      {#if isEasterEgg}
        <!-- 古古 = "old old": there is no such word, but it is the app's name (checked before the
             per-character breakdown, which would otherwise show 古) -->
        <div class="egg">
          <p class="eggh">No known word.</p>
          <p class="eggp">But a rather cool app for reading one character across 中文, 粵語, and 日本語. 😎</p>
        </div>
      {:else if breakdown.length}
        <section class="noword" data-testid="breakdown">
          <div class="nw-head">
            <span class="nw-q">{q}</span>
            <span class="nw-note">no known word</span>
          </div>
          <ul class="nw-list">
            {#each breakdown as c (c.ch)}
              <li>
                <button class="nw-char" onclick={() => doSearch(c.ch)}>
                  <span class="nw-glyph">{c.ch}</span>
                  <span class="nw-col">
                    <span class="nw-tags">
                      {#each charLangs(c) as t}<span class="nw-tag">{t}</span>{/each}
                    </span>
                    {#if charMeaning(c)}<span class="nw-mean">{charMeaning(c)}</span>{/if}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {:else if breaking}
        <!-- breakdown still loading: hold a placeholder rather than flash "nothing found" (item 3) -->
        {@render pageSkel()}
      {:else}
        <div class="empty">nothing for “{q}”.</div>
      {/if}
    {/if}
    {#if !searched && !q && panel === 'none'}
      <!-- "Quiet definition": frame what Kogu is as a dictionary entry in the app's own voice -->
      <div class="intro">
        <p class="introhw"><span class="intromark">古古</span> <span class="introword">Kogu</span></p>
        <p class="intropos"><span class="intropron">/ko.gu/</span> <span class="introtag">noun</span></p>
        <p class="introgloss">A dictionary that reads one Han character or word across <b>中文</b>, <b>粵語</b>, and <b>日本語</b> at once, with the readings and the meaning side by side.</p>
        <p class="introfoot">中 pinyin · 粵 jyutping · 日 kana</p>
      </div>
    {/if}
  {/if}

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}
</div>

<style>
  .wrap {
    max-width: 680px;
    margin: 0 auto;
    padding: calc(1.4rem + env(safe-area-inset-top)) calc(1.35rem + env(safe-area-inset-right))
      calc(4rem + env(safe-area-inset-bottom)) calc(1.35rem + env(safe-area-inset-left));
  }
  .bar { margin-bottom: 1rem; display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  /* item 7: history + saved buttons to the right of the wordmark */
  .navbtns { display: flex; gap: 0.2rem; }
  .navbtn { display: inline-flex; align-items: center; justify-content: center; width: 2.2rem; height: 2.2rem; background: none; border: none; border-radius: var(--r); color: var(--faint); }
  .navbtn:hover { color: var(--text); background: var(--surface); }
  .navbtn.on { color: var(--text); }
  /* per-page save/share actions */
  .actions { display: flex; gap: 0.5rem; margin: 0 0 0.9rem; }
  .actbtn { display: inline-flex; align-items: center; gap: 0.35rem; font-family: var(--mono); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.3rem 0.7rem; }
  .actbtn:hover { color: var(--text); border-color: var(--border-strong); }
  .actbtn.on { color: var(--text); border-color: var(--border-strong); }
  /* saved / history list views */
  .listview { padding-top: 0.2rem; }
  .lvh { display: flex; align-items: center; gap: 0.7rem; font-family: var(--sans); font-size: 1.2rem; font-weight: 500; color: var(--text); margin: 0 0 0.6rem; }
  .lvclear { display: inline-flex; align-items: center; gap: 0.3rem; font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--faint); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.15rem 0.5rem; }
  .lvclear:hover { color: var(--text); border-color: var(--border-strong); }
  /* transient "Link copied" toast for share fallback */
  .toast { position: fixed; left: 50%; bottom: calc(2rem + env(safe-area-inset-bottom)); transform: translateX(-50%); background: var(--surface-2, #1c1c1f); color: var(--text); border: 1px solid var(--border-strong); border-radius: 999px; padding: 0.5rem 1rem; font-size: 0.85rem; z-index: 60; }
  .brand { margin: 0; font-weight: 400; }
  .brandbtn { display: inline-flex; align-items: baseline; gap: 0.45rem; background: none; border: none; padding: 0; }
  .brandbtn:hover { background: none; }
  /* item 6: larger top-left wordmark */
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 2rem; letter-spacing: -0.04em; color: var(--text); }
  .brand .word { font-family: var(--sans); font-size: 1.35rem; letter-spacing: 0.06em; color: var(--muted); }

  .searchrow { display: flex; gap: 0.4rem; align-items: stretch; margin-bottom: 0.7rem; }
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
  .rowbtn.on { color: var(--bg); background: var(--text); border-color: var(--text); }

  /* inline draw pad / photo selection, shown directly under the search row */
  .inputpanel { margin-bottom: 1.2rem; }

  .meta { color: var(--faint); font-size: 0.7rem; margin-bottom: 0.6rem; font-family: var(--mono); text-transform: uppercase; letter-spacing: 0.1em; }
  .err { color: var(--text); margin: 0.5rem 0; }

  /* results - an editorial list: big serif headword, quiet meta column */
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

  /* no-word breakdown: the query shown big (like a headword), a quiet note, then tappable chars */
  .noword { padding: 0.6rem 0; }
  .nw-head { display: flex; align-items: baseline; gap: 0.9rem; margin-bottom: 0.9rem; flex-wrap: wrap; }
  .nw-q { font-family: var(--han); font-size: 2.1rem; line-height: 1.05; color: var(--text); }
  .nw-note { color: var(--faint); font-size: 0.7rem; font-family: var(--mono); text-transform: uppercase; letter-spacing: 0.1em; }
  .nw-list { list-style: none; margin: 0; padding: 0; }
  .nw-list li + li { border-top: 1px solid var(--border); }
  .nw-char {
    display: flex; align-items: center; gap: 0.9rem; width: 100%; text-align: left;
    background: none; border: none; border-radius: var(--r); padding: 0.7rem 0.5rem;
  }
  .nw-char:hover { background: var(--surface); color: var(--text); }
  .nw-glyph { font-family: var(--han); font-size: 1.7rem; line-height: 1.05; flex: none; min-width: 1.4em; }
  .nw-col { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; flex: 1; }
  .nw-tags { display: flex; gap: 0.3rem; }
  .nw-tag { font-family: var(--han); font-size: 0.72rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.25rem; }
  .nw-mean { color: var(--muted); font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* "Quiet definition" home state (item 2): Kogu described as a dictionary entry in its own voice */
  .intro { padding: 1.4rem 0.2rem; max-width: 34ch; }
  .introhw { margin: 0; display: flex; align-items: baseline; gap: 0.5rem; }
  .introhw .intromark { font-family: var(--han); font-weight: 500; font-size: 2.1rem; letter-spacing: -0.04em; color: var(--text); }
  .introhw .introword { font-family: var(--sans); font-size: 1.4rem; letter-spacing: 0.04em; color: var(--muted); }
  .intropos { margin: 0.35rem 0 1rem; display: flex; align-items: baseline; gap: 0.6rem; }
  .intropron { font-family: var(--mono); font-size: 0.95rem; color: var(--faint); }
  .introtag { font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); }
  .introgloss { font-family: var(--sans); font-size: 1.05rem; line-height: 1.7; color: var(--text); margin: 0 0 1rem; }
  .introgloss b { font-family: var(--han); font-weight: 500; }
  .introfoot { font-family: var(--han); font-size: 0.9rem; color: var(--faint); margin: 0; }
  /* easter egg (item 8) */
  /* whole-page loading skeleton (item 3): a quiet shimmer, monochrome */
  .pskel { padding: 0.6rem 0.2rem; }
  .ps-line { height: 0.95rem; border-radius: var(--r); background: var(--surface); margin: 0.55rem 0; overflow: hidden; position: relative; }
  .ps-line::after { content: ''; position: absolute; inset: 0; transform: translateX(-100%);
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.04), transparent); animation: psshimmer 1.3s ease-in-out infinite; }
  .ps-gap { height: 0.8rem; }
  .ps-line.w40 { width: 40%; } .ps-line.w55 { width: 55%; } .ps-line.w60 { width: 60%; }
  .ps-line.w70 { width: 70%; } .ps-line.w85 { width: 85%; }
  @keyframes psshimmer { to { transform: translateX(100%); } }
  @media (prefers-reduced-motion: reduce) { .ps-line::after { animation: none; } }
  .egg { padding: 1.4rem 0.2rem; max-width: 34ch; }
  .eggh { font-family: var(--sans); font-size: 1.25rem; color: var(--text); margin: 0 0 0.5rem; }
  .eggp { font-family: var(--sans); font-size: 1rem; line-height: 1.6; color: var(--muted); margin: 0; }
</style>
