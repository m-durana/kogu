<script lang="ts">
  import { search, entry as fetchEntry } from './lib/api'
  import type { Entry, Hit, CharInfo } from './lib/types'
  import { primaryForm, varietyLabel, regionsOf, shortGloss, cleanGloss, langTag, hanFont, placeholderAt } from './lib/display'
  import Unified from './lib/Unified.svelte'
  import Pad from './lib/Pad.svelte'
  import Ocr from './lib/Ocr.svelte'
  import { Search, X, Brush, Camera, Bookmark, Clock, Share2, Trash2, ArrowRight, Download, Settings, SquarePlus } from '@lucide/svelte'
  import { settings, setRomanization } from './lib/settings.svelte'
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

  // tapping the active nav button again closes the view (back to the search/results)
  function closePanelView() {
    view = 'results'
    history.replaceState({ view: 'results', q }, '', resultsUrl(q))
  }
  function openSaved() {
    if (view === 'saved') return closePanelView()
    savedList = getSaved()
    view = 'saved'
    panel = 'none'
    history.pushState({ view: 'saved' }, '', '#/saved')
  }
  function openHistory() {
    if (view === 'history') return closePanelView()
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

  // ── search bar (item 1) ──────────────────────────────────────────────────────────────────────
  let inputEl: HTMLInputElement
  let focused = $state(false) // drives the focus-expand (field grows, draw/camera hide)
  // rotating placeholder: a different example every 2s while the field is empty and unfocused
  let phIndex = $state(0)
  const placeholder = $derived(placeholderAt(phIndex))
  // install-as-web-app
  let deferredPrompt = $state<any>(null)
  let isStandalone = $state(false)
  let isIOS = $state(false)
  let isMobile = $state(false)
  let showInstallHelp = $state(false)
  let showSettings = $state(false)
  // mobile-only: never offer "install" on desktop (item 139)
  const canInstall = $derived(isMobile && !isStandalone && (deferredPrompt !== null || isIOS))

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
      // Only a query the user typed with Han GLYPHS resolves directly to the unified word card
      // (incl. mixed kanji+kana like 入り口). Searches by sound or meaning — kana (パレスチナ), romaji
      // pinyin/jyutping, or English — stay a plain list; they're lookups, not "this exact word", so
      // they get no word card, no character breakdown, and no save/share (the user drills in to get
      // those). Enrich the lexeme whose form is EXACTLY what was typed, falling back to the top hit.
      // a wildcard query (你* / *場) is a browse/filter with no single headword — always a list.
      // a "partial" top hit means the query didn't resolve to a whole word (a name glued to a common
      // word) — show the contained words as a LIST, not a unified card for one of them.
      const isWildcard = /[*?＊？]/.test(term)
      const queryHasHan = HAN.test(term) && !isWildcard
      if (queryHasHan && results.length && results[0].match_type !== 'partial') {
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
      } else if (!results.length && HAN.test(term) && !isWildcard) {
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

  // live search: each keystroke updates the results list on the page directly (debounced), like
  // hitting Enter after every character. No autocomplete dropdown.
  function onInput(e: Event) {
    const v = (e.target as HTMLInputElement).value
    q = v
    if (composing) return
    clearTimeout(timer)
    timer = setTimeout(() => doSearch(v, 'replace'), 160)
  }
  // commit immediately (Enter / search button): also close the draw / photo panel
  function submitSearch() {
    clearTimeout(timer)
    focused = false
    panel = 'none'
    ocrFile = null
    inputEl?.blur()
    doSearch(q)
  }
  function onFocus() {
    focused = true
    // caret to the very end when focusing a field that already holds a word
    const len = inputEl?.value.length ?? 0
    if (len) requestAnimationFrame(() => inputEl?.setSelectionRange(len, len))
  }
  function onBlur() {
    setTimeout(() => (focused = false), 150)
  }

  async function openEntry(id: number, mode: NavMode = 'push') {
    if (view === 'entry' && entry?.lexeme_id === id) return // already on this entry (before loading!)
    loading = true
    err = ''
    // reached not via the search bar (saved/history/link/character tap): clear the stale query so the
    // search field empties and the headword/glyph come from the ENTRY, not a leftover search term.
    q = ''
    results = []
    unified = false
    searched = false
    try {
      entry = await fetchEntry(id)
      enriching = false
      view = 'entry'
      if (mode === 'push') history.pushState({ view: 'entry', id }, '', `#/entry/${id}`)
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
    // rotate the placeholder every 2s (only matters while the field is empty); honour reduced-motion
    const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    let ph: ReturnType<typeof setInterval> | undefined
    if (!reduce) ph = setInterval(() => (phIndex += 1), 2000)
    // install-as-web-app (item 2): Android/Chrome fire beforeinstallprompt; iOS has no API (instruct).
    isStandalone =
      window.matchMedia?.('(display-mode: standalone)').matches || (navigator as any).standalone === true
    isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent) && !(window as any).MSStream
    isMobile = isIOS || /Android/i.test(navigator.userAgent) || (window.matchMedia?.('(pointer: coarse)').matches && window.matchMedia?.('(max-width: 860px)').matches)
    const onBip = (e: Event) => {
      e.preventDefault()
      deferredPrompt = e
    }
    window.addEventListener('beforeinstallprompt', onBip)
    return () => {
      window.removeEventListener('popstate', onPop)
      window.removeEventListener('beforeinstallprompt', onBip)
      if (ph) clearInterval(ph)
    }
  })

  async function installApp() {
    if (deferredPrompt) {
      // Android/Chrome: fire the native install dialog directly
      deferredPrompt.prompt()
      try {
        await deferredPrompt.userChoice
      } catch {
        /* dismissed */
      }
      deferredPrompt = null
    } else {
      // iOS (no install API): guide the user to Share → Add to Home Screen
      showInstallHelp = true
    }
  }

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

  // a character was chosen from the photo selection — search it and close the panel
  function fromInput(text: string) {
    panel = 'none'
    ocrFile = null
    doSearch(text)
  }
  // a stroke-recognised character from the draw pad: APPEND it to the field, live-update the results
  // behind the floating pad (= Enter after each character), and keep the pad open for the next one.
  function fromDraw(ch: string) {
    q = q + ch
    clearTimeout(timer)
    doSearch(q, 'replace')
  }
</script>

<div class="wrap">
  <header class="bar">
    <h1 class="brand">
      <button class="brandbtn" onclick={goHome} aria-label="home"><span class="mark">古古</span> <span class="word">Kogu</span></button>
    </h1>
    <nav class="navbtns">
      <button class="navbtn" class:on={view === 'history'} onclick={openHistory} aria-label="history" title="history"><Clock size={24} /></button>
      <button class="navbtn" class:on={view === 'saved'} onclick={openSaved} aria-label="saved" title="saved"><Bookmark size={24} /></button>
      <button class="navbtn" onclick={() => (showSettings = true)} aria-label="settings" title="settings"><Settings size={24} /></button>
    </nav>
  </header>

  <div class="searchrow" class:focused>
    <!-- a real <form> so the iOS keyboard shows a Go/Search key that reliably submits (item 7) -->
    <form class="field" onsubmit={(e) => { e.preventDefault(); submitSearch() }}>
      <span class="searchicon" aria-hidden="true"><Search size={17} /></span>
      <input
        bind:this={inputEl}
        type="text"
        enterkeyhint="search"
        lang={langTag(queryLang)}
        style="font-family:{inputFont}"
        aria-label="Search by hanzi, kanji, pinyin, jyutping, kana, or English"
        placeholder={placeholder}
        value={q}
        oninput={onInput}
        onfocus={onFocus}
        onblur={onBlur}
        oncompositionstart={() => (composing = true)}
        oncompositionend={(e) => {
          composing = false
          onInput(e)
        }}
        onkeydown={(e) => { if (e.key === 'Escape') inputEl?.blur() }}
        data-testid="search-input"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
      />
      {#if q}
        <button type="button" class="clearbtn" aria-label="clear search" onmousedown={(e) => e.preventDefault()} onclick={clearSearch} data-testid="clear"><X size={17} /></button>
      {/if}
      <button type="submit" class="searchbtn" aria-label="search" title="search" data-testid="search-go"><ArrowRight size={18} /></button>
      {#if loading}<span class="loadbar" aria-hidden="true"></span>{/if}
    </form>
    <button class="rowbtn" class:on={panel === 'draw'} aria-label="draw a character" aria-pressed={panel === 'draw'} title="draw" onclick={toggleDraw} data-testid="draw-toggle"><Brush size={18} /></button>
    <button class="rowbtn" class:on={panel === 'photo'} aria-label="photo or image" title="photo / image" onclick={openPhoto} data-testid="scan-toggle"><Camera size={18} /></button>
    <input bind:this={fileInput} type="file" accept="image/*" onchange={onPhotoFile} hidden />
  </div>

  {#if panel === 'photo' && ocrFile}
    <section class="inputpanel"><Ocr file={ocrFile} onpick={fromInput} /></section>
  {/if}

  {#if panel === 'draw'}
    <!-- inline draw pad, directly under the search row (top of the page) so it sits where you type -->
    <div class="drawpanel">
      <Pad onpick={fromDraw} onclose={() => (panel = 'none')} />
    </div>
  {/if}

  {#if err}<div class="err">{err}</div>{/if}

  {#if canSaveShare}
    <!-- per-page actions: icon-only save + share, on the right edge level with the big character -->
    <div class="actions">
      <button class="actbtn" class:on={savedNow} onclick={toggleSave} aria-pressed={savedNow} aria-label={savedNow ? 'remove bookmark' : 'save'} title={savedNow ? 'saved' : 'save'}>
        <Bookmark size={22} fill={savedNow ? 'currentColor' : 'none'} />
      </button>
      <button class="actbtn" onclick={shareCurrent} aria-label="share" title="share">
        <Share2 size={22} />
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
  {:else if unified && enrichEntry}
    <!-- render the WHOLE card at once, from the full entry: definition + structure + origin + used-in +
         bridges all appear together in one paint, so nothing pops in after (collapsed panel bodies
         still render lazily on expand). The /entry fetch is ~10-16ms warm, hidden behind the skeleton. -->
    <Unified hits={results} entry={enrichEntry} anchor={q} onsearch={doSearch} />
  {:else if unified && enriching}
    <!-- enriching: hold the whole-page skeleton rather than a partial def that then grows (no pop-in) -->
    {@render pageSkel()}
  {:else if unified && results.length}
    <!-- enrich finished but no entry came back (fetch failed): still show the card from search hits -->
    <Unified hits={results} entry={null} anchor={q} onsearch={doSearch} />
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
                <!-- only the INFORMATIVE region tags: CN/JP are redundant with the 中/日 variety tag,
                     so drop them; keep the rarer TW/HK that actually distinguish a regional word. -->
                {#each regionsOf(r).filter((rg) => rg === 'TW' || rg === 'HK') as rg}<span class="rg">{rg}</span>{/each}
              </span>
              <span class="gl">{shortGloss(r.glosses)}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    {#if searched && !loading && results.length === 0}
      {#if breakdown.length}
        <section class="noword" data-testid="breakdown">
          <div class="nw-head">
            <span class="nw-q">{q}</span>
            <!-- 古古 ("old old") is the app's name, not a real word: same page, just a cheekier note -->
            <span class="nw-note">{isEasterEgg ? 'no known word, but a super cool app 😎' : 'no known word'}</span>
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
      <!-- About page: what Kogu is, what each section of an entry means, and where the data comes from -->
      <div class="about">
        <p class="introhw">
          <span class="intromark">古古</span> <span class="introword">Kogu</span>
          {#if canInstall}<button class="installbtn" onclick={installApp} aria-label="install as web app"><Download size={14} /> Install</button>{/if}
        </p>
        <p class="intropos"><span class="intropron">/ko.gu/</span> <span class="introtag">noun</span></p>
        <p class="introgloss">A dictionary for the living Han script. One character or word is shown across <b>中文</b> (Mandarin), <b>粵語</b> (Cantonese), and <b>日本語</b> (Japanese) at once, so you can see how the same writing is read and used in each, and how the reforms pulled the forms apart.</p>

        <h2 class="abh">On each page</h2>
        <dl class="ablist">
          <dt>Readings</dt><dd>How the word sounds in each language: <b>中</b> pinyin, <b>粵</b> jyutping, <b>日</b> kana (on and kun), with the meaning beside it.</dd>
          <dt>Structure</dt><dd>What a character is built from (its parts, and which carries the meaning vs the sound), plus its forms across scripts: traditional, simplified, and Japanese shinjitai, with the reform that split them.</dd>
          <dt>Origin</dt><dd>The character or word's etymology, kept per language since the Chinese and Japanese accounts of the same glyph can both be true.</dd>
          <dt>Used in</dt><dd>Common words that contain the character, grouped by language.</dd>
          <dt>Related</dt><dd>Other words that carry the same meaning, including cross-language equivalents, cognates, and false friends (same writing, different meaning).</dd>
        </dl>

        <h2 class="abh">Where the data comes from</h2>
        <ul class="absrc">
          <li><b>CC-CEDICT</b> and <b>CC-Canto</b>: Mandarin and Cantonese words and readings</li>
          <li><b>JMdict</b> and <b>Kanjidic</b>: Japanese words and kanji readings</li>
          <li><b>Unihan</b> and <b>cjkvi-ids</b>: characters, stroke data, and how they decompose</li>
          <li><b>Wiktionary</b>: etymologies and phono-semantic component roles</li>
        </ul>
        <p class="abnote">Everything is passed through from these open datasets directly. Nothing here is written by an AI. Kogu is open source.</p>
      </div>
    {/if}
  {/if}

  {#if showInstallHelp}
    <!-- guided add-to-home-screen (iOS has no install API; Android uses the native prompt instead) -->
    <div class="instbg" role="presentation" onclick={() => (showInstallHelp = false)}>
      <div class="instcard" role="dialog" aria-modal="true" aria-label="install instructions" onclick={(e) => e.stopPropagation()}>
        <p class="insth">Add Kogu to your Home Screen</p>
        <ol class="inststeps">
          <li><span class="instep"><Share2 size={18} /></span> Tap the <b>Share</b> button {isIOS ? 'in the toolbar below' : 'in your browser menu'}</li>
          <li><span class="instep"><SquarePlus size={18} /></span> Choose <b>Add to Home Screen</b></li>
        </ol>
        <button class="setclose" onclick={() => (showInstallHelp = false)}>got it</button>
      </div>
      {#if isIOS}<div class="instpoint" aria-hidden="true">▾</div>{/if}
    </div>
  {/if}

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}

  {#if showSettings}
    <div class="setbg" role="presentation" onclick={() => (showSettings = false)}>
      <div class="setcard" role="dialog" aria-modal="true" aria-label="settings" onclick={(e) => e.stopPropagation()}>
        <h2 class="seth">Settings</h2>
        <div class="setrow">
          <span class="setlabel">Cantonese romanization</span>
          <div class="seg">
            <button class:on={settings.romanization === 'jyutping'} onclick={() => setRomanization('jyutping')}>Jyutping</button>
            <button class:on={settings.romanization === 'yale'} onclick={() => setRomanization('yale')}>Yale</button>
          </div>
        </div>
        <button class="setclose" onclick={() => (showSettings = false)}>close</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .wrap {
    max-width: 680px;
    margin: 0 auto;
    padding: calc(1.4rem + env(safe-area-inset-top)) calc(1.35rem + env(safe-area-inset-right))
      calc(4rem + env(safe-area-inset-bottom)) calc(1.35rem + env(safe-area-inset-left));
  }
  .bar { margin-bottom: 1rem; display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  /* item 7: history + saved buttons to the right of the wordmark (enlarged) */
  .navbtns { display: flex; gap: 0.3rem; }
  .navbtn { display: inline-flex; align-items: center; justify-content: center; width: 2.9rem; height: 2.9rem; background: none; border: none; border-radius: var(--r); color: var(--muted); }
  .navbtn:hover { color: var(--text); background: var(--surface); }
  .navbtn.on { color: var(--text); }
  /* per-page save/share: icon-only, on the right edge, pulled up level with the big character */
  .actions { display: flex; justify-content: flex-end; gap: 0.2rem; margin: 0 0 -2.6rem; position: relative; z-index: 3; pointer-events: none; }
  .actbtn { display: inline-flex; align-items: center; justify-content: center; width: 2.6rem; height: 2.6rem; color: var(--muted); background: none; border: none; border-radius: var(--r); pointer-events: auto; }
  .actbtn:hover { color: var(--text); background: var(--surface); }
  .actbtn.on { color: var(--text); }
  /* saved / history list views */
  .listview { padding-top: 0.2rem; }
  .lvh { display: flex; align-items: center; gap: 0.7rem; font-family: var(--sans); font-size: 1.2rem; font-weight: 500; color: var(--text); margin: 0 0 0.6rem; }
  .lvclear { display: inline-flex; align-items: center; gap: 0.3rem; font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--faint); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.15rem 0.5rem; }
  .lvclear:hover { color: var(--text); border-color: var(--border-strong); }
  /* transient "Link copied" toast for share fallback */
  .toast { position: fixed; left: 50%; bottom: calc(2rem + env(safe-area-inset-bottom)); transform: translateX(-50%); background: var(--surface-2, #1c1c1f); color: var(--text); border: 1px solid var(--border-strong); border-radius: 999px; padding: 0.5rem 1rem; font-size: 0.85rem; z-index: 60; }
  /* settings panel */
  .setbg { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 70; }
  .setcard { width: min(22rem, 100%); background: var(--surface-2, #1c1c1f); border: 1px solid var(--border-strong); border-radius: var(--r-lg); padding: 1.1rem 1.1rem 0.9rem; }
  .seth { font-family: var(--sans); font-size: 1.1rem; font-weight: 500; color: var(--text); margin: 0 0 0.9rem; }
  .setrow { display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 1rem; }
  .setlabel { font-size: 0.9rem; color: var(--muted); }
  .seg { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--r); overflow: hidden; align-self: start; }
  .seg button { font-family: var(--mono); font-size: 0.78rem; color: var(--muted); background: none; border: none; padding: 0.35rem 0.8rem; }
  .seg button + button { border-left: 1px solid var(--border-strong); }
  .seg button.on { background: var(--text); color: var(--bg); }
  .setclose { font-family: var(--mono); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--faint); background: none; border: 1px solid var(--border); border-radius: var(--r); padding: 0.3rem 0.7rem; }
  .setclose:hover { color: var(--text); border-color: var(--border-strong); }
  .brand { margin: 0; font-weight: 400; }
  .brandbtn { display: inline-flex; align-items: baseline; gap: 0.45rem; background: none; border: none; padding: 0; }
  .brandbtn:hover { background: none; }
  /* item 6: larger top-left wordmark */
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 2rem; letter-spacing: -0.04em; color: var(--text); }
  .brand .word { font-family: var(--sans); font-size: 1.35rem; letter-spacing: 0.06em; color: var(--muted); }

  .searchrow { display: flex; align-items: stretch; margin-bottom: 0.7rem; }
  .field { position: relative; flex: 1; min-width: 0; display: flex; }
  .searchicon { position: absolute; left: 0.8rem; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; display: flex; }
  .field input {
    width: 100%; padding: 0.72rem 4.4rem 0.72rem 2.4rem; font-size: 1.05rem; line-height: 1.15;
    font-family: var(--sans); color: var(--text); -webkit-appearance: none; appearance: none;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg);
  }
  .field input::-webkit-search-decoration, .field input::-webkit-search-cancel-button { -webkit-appearance: none; appearance: none; }
  .field input:focus { border-color: var(--border-strong); background: var(--surface-2); }
  .field input::placeholder { color: var(--faint); }
  /* loading indicator: a thin sliding bar along the bottom of the field while a search is in flight */
  .loadbar { position: absolute; left: 1px; right: 1px; bottom: 1px; height: 2px; overflow: hidden; border-radius: 0 0 var(--r-lg) var(--r-lg); pointer-events: none; }
  .loadbar::after { content: ''; position: absolute; inset: 0; width: 40%; background: var(--muted); border-radius: 2px; animation: loadslide 0.9s ease-in-out infinite; }
  @keyframes loadslide { 0% { transform: translateX(-110%); } 100% { transform: translateX(360%); } }
  @media (prefers-reduced-motion: reduce) { .loadbar::after { animation-duration: 2s; } }
  /* item 1: monochrome selection so highlighting typed text doesn't look odd */
  .field input::selection { background: var(--muted); color: var(--bg); }
  .clearbtn {
    position: absolute; right: 2.5rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
  }
  .clearbtn:hover { color: #fff; background: var(--surface-2); }
  /* item 1: search button to the right of the X */
  .searchbtn {
    position: absolute; right: 0.4rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
  }
  .searchbtn:hover { color: var(--text); background: var(--surface-2); }
  .rowbtn {
    flex: none; display: inline-flex; align-items: center; justify-content: center; padding: 0 0.75rem; margin-left: 0.4rem;
    color: var(--muted); background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg);
    max-width: 4rem; overflow: hidden;
    transition: max-width 0.22s ease, opacity 0.18s ease, margin 0.22s ease, padding 0.22s ease;
  }
  .rowbtn:hover { color: #fff; border-color: var(--border-strong); background: var(--surface-2); }
  .rowbtn.on { color: var(--bg); background: var(--text); border-color: var(--text); }
  /* item 1: focusing the field expands it to full width; draw + camera slide away */
  .searchrow.focused .rowbtn { max-width: 0; margin-left: 0; padding: 0; opacity: 0; border-width: 0; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .rowbtn { transition: none; } }
  /* inline draw pad at the top (under the search row); minimal frame, no heavy floating chrome */
  .drawpanel {
    width: min(20rem, 100%);
    margin: 0 0 1.2rem;
    background: var(--surface-2); border: 1px solid var(--border); border-radius: 3px;
    padding: 0.6rem;
  }

  /* inline photo selection, shown directly under the search row */
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
  /* About page (item 2): what Kogu is, what each section means, and the data sources */
  .about { padding: 1rem 0.2rem 2rem; max-width: 40ch; }
  .introhw { margin: 0; display: flex; align-items: baseline; gap: 0.5rem; flex-wrap: wrap; }
  .introhw .intromark { font-family: var(--han); font-weight: 500; font-size: 2.1rem; letter-spacing: -0.04em; color: var(--text); }
  .introhw .introword { font-family: var(--sans); font-size: 1.4rem; letter-spacing: 0.04em; color: var(--muted); }
  .intropos { margin: 0.35rem 0 1rem; display: flex; align-items: baseline; gap: 0.6rem; }
  .intropron { font-family: var(--mono); font-size: 0.95rem; color: var(--faint); }
  .introtag { font-family: var(--mono); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--faint); }
  /* install-as-web-app button (item 2) */
  /* install button sits to the right of the 古古 Kogu wordmark (item 139) */
  .installbtn { display: inline-flex; align-items: center; gap: 0.35rem; margin-left: auto; align-self: center; font-family: var(--mono); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text); background: none; border: 1px solid var(--border-strong); border-radius: var(--r); padding: 0.3rem 0.6rem; }
  .installbtn:hover { background: var(--surface); }
  /* guided add-to-home-screen overlay */
  .instbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.72); z-index: 80; display: flex; align-items: center; justify-content: center; padding: 1.2rem; }
  .instcard { width: min(22rem, 100%); background: var(--surface-2, #1c1c1f); border: 1px solid var(--border-strong); border-radius: var(--r-lg); padding: 1.1rem; }
  .insth { font-family: var(--sans); font-size: 1.1rem; font-weight: 500; color: var(--text); margin: 0 0 0.8rem; }
  .inststeps { margin: 0 0 1rem; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 0.7rem; }
  .inststeps li { display: flex; align-items: center; gap: 0.6rem; font-size: 0.95rem; line-height: 1.4; color: var(--muted); }
  .inststeps b { color: var(--text); font-weight: 500; }
  .instep { display: inline-flex; align-items: center; justify-content: center; width: 2rem; height: 2rem; flex: none; border: 1px solid var(--border-strong); border-radius: var(--r); color: var(--text); }
  .instpoint { position: fixed; left: 50%; bottom: calc(0.5rem + env(safe-area-inset-bottom)); transform: translateX(-50%); color: #fff; font-size: 2rem; animation: instbob 1.1s ease-in-out infinite; }
  @keyframes instbob { 0%,100% { transform: translate(-50%, 0); } 50% { transform: translate(-50%, 0.4rem); } }
  @media (prefers-reduced-motion: reduce) { .instpoint { animation: none; } }
  .introgloss { font-family: var(--sans); font-size: 1.05rem; line-height: 1.7; color: var(--text); margin: 0 0 1.6rem; }
  .introgloss b, .ablist b, .absrc b { font-family: var(--han); font-weight: 500; }
  .abh { font-family: var(--mono); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--faint); margin: 1.6rem 0 0.6rem; }
  .ablist { margin: 0; }
  .ablist dt { font-family: var(--sans); font-size: 0.98rem; color: var(--text); font-weight: 500; margin-top: 0.7rem; }
  .ablist dd { margin: 0.1rem 0 0; font-size: 0.92rem; line-height: 1.6; color: var(--muted); }
  .ablist b { font-weight: 500; }
  .absrc { list-style: none; margin: 0; padding: 0; }
  .absrc li { font-size: 0.92rem; line-height: 1.55; color: var(--muted); padding: 0.28rem 0; border-top: 1px solid var(--border); }
  .absrc li:first-child { border-top: none; }
  .absrc b { color: var(--text); font-family: var(--sans); font-weight: 500; }
  .abnote { font-size: 0.88rem; line-height: 1.6; color: var(--faint); margin: 1.3rem 0 0; }
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
</style>
