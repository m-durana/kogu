<script lang="ts">
  import { search, entry as fetchEntry, randomEntry, segment as fetchSegment, suggest as fetchSuggest, interesting, type SegmentPart, type Suggestion, type InterestingItem, type SearchScope, type SearchLang } from './lib/api'
  import { dialogFocus } from './lib/modal'
  import type { Entry, Hit, CharInfo } from './lib/types'
  import { primaryForm, varietyLabel, regionsOf, shortGloss, cleanGloss, langTag, hanFont, placeholderAt, formatReading } from './lib/display'
  import { typoCandidates } from './lib/typo'
  import Unified from './lib/Unified.svelte'
  import EntryRow from './lib/EntryRow.svelte'
  import LookupPanel from './lib/LookupPanel.svelte'
  import Pad from './lib/Pad.svelte'
  import Ocr from './lib/Ocr.svelte'
  import { Search, X, Brush, Camera, Bookmark, Clock, Share, Share2, Trash2, ArrowRight, Download, Settings, SquarePlus, ExternalLink, ChevronDown, Dices } from '@lucide/svelte'
  import { settings, setRomanization, setPitchAccent, setAudio, setJaRomaji } from './lib/settings.svelte'
  import { onMount } from 'svelte'
  import { getSaved, getHistory, isSaved, toggleSaved, recordHistory, clearHistory, type SavedItem } from './lib/store'

  let q = $state('')
  let results = $state<Hit[]>([])
  let classified = $state('')
  let entry = $state<Entry | null>(null)
  let enrichEntry = $state<Entry | null>(null)
  let enriching = $state(false)
  // the /entry enrich failed after the search succeeded: the card renders from hits alone (no
  // structure/origin/words tabs): say so and offer a retry instead of silently showing less
  let enrichFailed = $state(false)
  let enrichFailedId = 0
  let unified = $state(false)
  let view = $state<'results' | 'entry' | 'saved' | 'history'>('results')
  // saved (bookmarks) + history lists, loaded from localStorage when their view opens
  let savedList = $state<SavedItem[]>([])
  let historyList = $state<SavedItem[]>([])
  // homepage showcase: a fresh-random handful of noteworthy entries (kokuji, false friends, …)
  let interestingList = $state<InterestingItem[]>([])
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
  // greedy longest-word segmentation of an unknown query for the "literally" line (紅出口 → red · exit);
  // when present it supersedes the strict character-by-character composite.
  let segments = $state<SegmentPart[]>([])
  // "did you mean …": closest real entries when a search finds nothing (a typo/partial query).
  let didYouMean = $state<Suggestion[]>([])

  const HAN = /\p{Script=Han}/u
  // easter egg: 古古 ("old old") is the app's name, not a real word: shown when looked up (item 8)
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
  // save/share belong only on an actual word page: NOT on the History or Saved list views (where a
  // stale `unified` from a prior search would otherwise keep them visible). Gate on the current view.
  const onWordPage = $derived(view === 'entry' || (view === 'results' && unified && results.length > 0))
  const canSaveShare = $derived(currentItem != null && onWordPage)

  // record each visited word in history, and keep the bookmark toggle in sync with what's shown.
  // Wait until enrichment settles: while /entry is in flight, currentItem is the top SEARCH hit
  // (學校, zh) and flips to the enriched lexeme (学校, ja) when it lands: recording both wrote two
  // history rows for one visit.
  $effect(() => {
    const it = currentItem
    if (it && onWordPage && !enriching) {
      recordHistory(it)
      savedNow = isSaved(it)
    }
  })

  // Settings: "clear cache": wipe everything Kogu stores on this device (history, bookmarks,
  // preferences, the offline app shell and cached audio) and reload fresh. Two-tap confirm so a
  // stray tap can't erase the user's saved words.
  let clearArmed = $state(false)
  async function clearAllData() {
    if (!clearArmed) {
      clearArmed = true
      setTimeout(() => (clearArmed = false), 4000)
      return
    }
    for (const k of Object.keys(localStorage)) {
      if (k.startsWith('kogu:')) localStorage.removeItem(k)
    }
    try {
      if ('caches' in window) {
        for (const k of await caches.keys()) await caches.delete(k)
      }
      const regs = (await navigator.serviceWorker?.getRegistrations?.()) ?? []
      for (const r of regs) await r.unregister()
    } catch {
      // storage APIs can throw in private windows; the reload still gives a clean state
    }
    location.href = location.pathname // full reload, no hash/query, re-registers the SW
  }

  function toggleSave() {
    if (!currentItem) return
    savedNow = toggleSaved(currentItem)
  }

  // share a direct link: a readable ?q=headword for words, #/entry/<id> for char-only (negative id)
  async function shareCurrent() {
    if (!currentItem) return
    const it = currentItem
    // share the page as TYPED: a 桥 lookup must link (and title) 桥, not the trad headword 橋
    const term = view === 'results' && q.trim() ? q.trim() : it.headword
    const path = it.id < 0 ? `#/entry/${it.id}` : `?q=${encodeURIComponent(term)}`
    const url = `${location.origin}/${path}`
    try {
      if (navigator.share) await navigator.share({ title: `${term} · Kogu`, url })
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
    history.pushState({ view: 'saved' }, '', `${location.pathname}#/saved`)
  }
  function openHistory() {
    if (view === 'history') return closePanelView()
    historyList = getHistory()
    view = 'history'
    panel = 'none'
    history.pushState({ view: 'history' }, '', `${location.pathname}#/history`)
  }
  function wipeHistory() {
    clearHistory()
    historyList = []
  }

  // Render typed CJK in the same regional serif as the headword it resolves to (a Japanese word's 誤
  // shouldn't show the Simplified-Chinese glyph in the box while the headword shows the Japanese one).
  // Latin is system sans; the CJK fallback follows the top hit's variety once results arrive.
  const queryLang = $derived(results[0]?.variety ?? 'zh')
  // sans Latin (matching the UI), but keep the script-correct Han face so a typed kanji/粵字 renders right
  const inputFont = $derived(
    queryLang === 'ja'
      ? '-apple-system, system-ui, var(--han-ja), sans-serif'
      : queryLang === 'yue'
        ? '-apple-system, system-ui, var(--han-tc), sans-serif'
        : 'var(--sans)',
  )
  // CJK glyphs sit high in the box: their ink rides above the Latin baseline the input's line box is
  // placed by (measured ~3px high at 16px). Nudge the padding down 2px, but only while the text
  // actually contains Han/kana: Latin text (and the placeholder) is already centered.
  const inputHan = $derived(
    /[\u{2e80}-\u{9fff}\u{3040}-\u{30ff}\u{31f0}-\u{31ff}\u{f900}-\u{faff}\u{20000}-\u{3ffff}]/u.test(q),
  )

  // first language-flagged meaning for a component character, kept short
  function charMeaning(c: CharInfo): string {
    const g = cleanGloss(c.gloss_en ?? '')
    return g.split(';')[0].trim()
  }
  // a LITERAL gloss chain for a query with no whole-word match, clearly labelled "literally" and never
  // a fabricated whole-phrase definition. Prefer the backend's greedy longest-sub-word segmentation
  // (紅出口 → "red · exit"); fall back to the strict character-by-character chain (中宇大度 → "central ·
  // roof · big · degree") when segmentation is unavailable.
  const compositeMeaning = $derived.by(() => {
    const segGloss = segments.map((s) => s.gloss.trim()).filter(Boolean)
    if (segments.length >= 2 && segGloss.length >= 2) return segGloss.join(' · ')
    if (breakdown.length >= 2)
      return breakdown.map((c) => charMeaning(c).split(',')[0].trim()).filter(Boolean).join(' · ')
    return ''
  })
  // in-app lookup panel (Translate + Wiktionary) for the typed term: works whether or not Kogu has
  // the word, useful for names, neologisms, and partial phrases.
  let lookupOpen = $state(false)
  // source-language hint for the translate proxy: a Han query uses its result variety; otherwise auto.
  const lookupSl = $derived(
    !HAN.test(q) ? 'auto' : queryLang === 'ja' ? 'ja' : queryLang === 'yue' ? 'yue' : 'zh-CN',
  )
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

  // the term whose results are currently shown: used to skip a redundant re-search (e.g. tapping the
  // word you're already on). NOT `q`: onInput overwrites q with the NEW typed value before doSearch
  // runs, so comparing to q made every edit look like "same query" and silently skipped the search.
  let lastSearched = ''
  // Force scope for an ambiguous romanized query: `you` can be the WORD (你) or a SOUND (有/又/よ).
  // The toggle only appears for romanized queries (Latin letters, no Han/kana) since CJK input is
  // unambiguous; a non-roman query always searches in auto so a stale toggle can't silently filter it.
  let searchScope = $state<SearchScope>('auto')
  // language filter (中/粵/日 pill): restrict results to one variety. 'all' = every language.
  let searchLang = $state<SearchLang>('all')
  function isRoman(s: string): boolean {
    const t = s.trim()
    return t.length > 0 && /[a-zA-Z]/.test(t) && !HAN.test(t) && !/[぀-ヿ]/.test(t)
  }
  const romanQuery = $derived(isRoman(q))
  function setScope(s: SearchScope) {
    if (s === searchScope) return
    searchScope = s
    rerunSearch()
  }
  function setLang(l: SearchLang) {
    if (l === searchLang) return
    searchLang = l
    rerunSearch()
  }
  // re-run the current search under the new lens/filter (bypass the "same term" guard in doSearch)
  function rerunSearch() {
    if (q.trim() && view === 'results') {
      lastSearched = ''
      doSearch(q, 'replace')
    }
  }

  async function doSearch(query: string, mode: NavMode = 'push') {
    const term = query.trim()
    // already showing this exact query ON THE RESULTS VIEW (e.g. tapped the row/character for the page
    // you're on): do nothing, so the view doesn't blank and reload. Must check view: tapping a search
    // from the History/Saved view needs to actually switch back to results even for the same term.
    if (term && term === lastSearched && searched && !loading && view === 'results' && (results.length || entry)) return
    lastSearched = term
    q = query
    view = 'results'
    entry = null
    enrichEntry = null
    enriching = false
    enrichFailed = false
    unified = false
    breakdown = []
    breaking = false
    segments = []
    didYouMean = []
    // clear prior results so a NEW search shows the skeleton, not stale content that then swaps out
    results = []
    if (!term) {
      searched = false
      if (mode !== 'none') history.replaceState({ view: 'results', q: '' }, '', location.pathname)
      return
    }
    if (mode === 'push') {
      history.pushState({ view: 'results', q: term }, '', resultsUrl(term))
      window.scrollTo(0, 0) // a freshly navigated word/list starts at the top (back restores the rest)
    } else if (mode === 'replace') history.replaceState({ view: 'results', q: term }, '', resultsUrl(term))
    ctrl?.abort()
    ctrl = new AbortController()
    loading = true
    err = ''
    try {
      // a non-roman query is unambiguous, so it always searches in auto (a stale toggle can't filter it)
      const res = await search(term, undefined, isRoman(term) ? searchScope : 'auto', searchLang, ctrl.signal)
      results = res.results
      classified = res.classified_as
      searched = true
      // Only a query the user typed with Han GLYPHS resolves directly to the unified word card
      // (incl. mixed kanji+kana like 入り口). Searches by sound or meaning: kana (パレスチナ), romaji
      // pinyin/jyutping, or English: stay a plain list; they're lookups, not "this exact word", so
      // they get no word card, no character breakdown, and no save/share (the user drills in to get
      // those). Enrich the lexeme whose form is EXACTLY what was typed, falling back to the top hit.
      // a wildcard query (你* / *場) is a browse/filter with no single headword: always a list.
      // a "partial" top hit means the query didn't resolve to a whole word (a name glued to a common
      // word): show the contained words as a LIST, not a unified card for one of them.
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
          .catch((e) => {
            if ((e as Error).name !== 'AbortError' && q.trim() === term && unified) {
              enrichFailed = true
              enrichFailedId = topId
            }
          })
          .finally(() => {
            if (q.trim() === term) enriching = false
          })
      } else if (HAN.test(term) && !isWildcard) {
        // The query is Han but did NOT resolve to a single word card (no match, or only a PARTIAL
        // word was caught inside it, e.g. 中宇大度 → only 大度). Break the WHOLE query into its
        // component characters so every character is shown with its meaning no matter what, beneath
        // any partial-word results. Char-only entries live at /entry/{-codepoint}; fetch in parallel.
        const chars = [...new Set([...term].filter((c) => HAN.test(c)))]
        // longest-known-sub-word segmentation for the "literally" hint (紅出口 → red · exit). Falls back
        // to the per-character composite if it returns nothing or fails.
        fetchSegment(term)
          .then((s) => {
            if (q.trim() === term) segments = s.segments
          })
          .catch(() => {})
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
      // nothing matched and there's no character breakdown to show (a non-Han typo/partial like
      // "moutain" or "fei1ji"): offer the closest real entries as "did you mean …".
      if (results.length === 0 && !breaking) loadDidYouMean(term)
      // record the SEARCH itself in history when it did NOT resolve to a single word card/entry (a
      // list, a partial match, or a no-word query like 中宇大廈): those have no entry to record via
      // the visited-page effect, so they'd otherwise never appear in history. (Skip back/forward nav.)
      if (mode !== 'none' && !unified) {
        recordHistory({
          id: 0,
          headword: term,
          reading: null,
          variety: (results[0]?.variety ?? 'zh') as Hit['variety'],
          gloss: results[0]?.glosses?.[0] ? cleanGloss(results[0].glosses[0]) : null,
          ts: 0,
          query: true,
        })
      }
    } catch (e) {
      if ((e as Error).name !== 'AbortError') err = 'search failed'
    } finally {
      loading = false
    }
  }

  // closest real entries for a query that matched nothing. /suggest is prefix-based, so we try the
  // term, its adjacent-letter transpositions ("xuexaio" → "xuexiao"), then progressively shorter
  // prefixes ("mountainz" → "mountain"): see typoCandidates for the reasoning and ordering.
  async function loadDidYouMean(term: string) {
    for (const t of typoCandidates(term.trim())) {
      if (q.trim() !== term) return // superseded by a newer query
      try {
        const s = await fetchSuggest(t)
        const hits = s.filter((x) => x.headword !== term)
        if (hits.length) {
          if (q.trim() === term) didYouMean = hits.slice(0, 6)
          return
        }
      } catch {
        /* a stale/aborted suggest is fine: leave didYouMean empty */
      }
    }
  }

  // live search: each keystroke updates the results list on the page directly (debounced), like
  // hitting Enter after every character. No autocomplete dropdown.
  function onInput(e: Event) {
    const v = (e.target as HTMLInputElement).value
    q = v
    if (composing) return
    clearTimeout(timer)
    // emptying the field keeps the current page (results/entry stay) until a new character is typed :
    // don't run an empty search that would blank back to the home screen.
    if (!v.trim()) return
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

  // re-fetch a failed enrich for the word still on screen (its own controller: the search's
  // controller may already be aborted)
  function retryEnrich() {
    if (!enrichFailedId) return
    const id = enrichFailedId
    const term = q.trim()
    enrichFailed = false
    enriching = true
    fetchEntry(id, new AbortController().signal)
      .then((e) => {
        if (q.trim() === term && unified) enrichEntry = e
      })
      .catch(() => {
        if (q.trim() === term && unified) enrichFailed = true
      })
      .finally(() => {
        if (q.trim() === term) enriching = false
      })
  }

  // "Feeling lucky": jump to a random reasonably-common word. Disabled while a roll is in flight so a
  // double-tap doesn't fire two navigations.
  let rolling = $state(false)
  async function feelingLucky() {
    if (rolling) return
    rolling = true
    try {
      const id = await randomEntry()
      await openEntry(id, 'push')
    } catch {
      err = 'Could not fetch a random word. Try again.'
    } finally {
      rolling = false
    }
  }

  async function openEntry(id: number, mode: NavMode = 'push', anchor = '') {
    if (view === 'entry' && entry?.lexeme_id === id) return // already on this entry (before loading!)
    loading = true
    err = ''
    // reached not via the search bar (saved/history/link/character tap): the `anchor` is the exact
    // form the user tapped (a saved/history headword, a result-row glyph). Seed q with it so the
    // headword echoes THAT script (tapping a traditional 機場 keeps 機場, not the simplified default)
    // instead of letting primaryForm fall back to Simplified. Empty anchor (deep link) → simp default.
    q = anchor
    lastSearched = '' // leaving the results view: a later identical typed query should re-search
    results = []
    unified = false
    searched = false
    try {
      const e = await fetchEntry(id)
      // Stale-id guard: a saved/history id stored before a DB rebuild can now resolve to a DIFFERENT
      // word (lexeme ids are reassigned on rebuild). If the loaded entry doesn't contain the form the
      // user actually tapped, re-resolve by searching that form instead of showing the wrong entry.
      if (anchor && e.headword !== anchor && !e.forms?.some((f) => f.form === anchor)) {
        loading = false
        await doSearch(anchor)
        return
      }
      entry = e
      enriching = false
      view = 'entry'
      // store the headword in history state so Back/Forward to this entry can re-resolve if the id
      // went stale after a DB rebuild (see the openEntry stale-id guard above).
      // pathname-anchored so a leftover ?q= from the search view can't ride along in the URL
      // (a bare "#/entry/…" pushState kept it: shared links looked like "#/entry/71976?q=犬")
      if (mode === 'push') {
        history.pushState({ view: 'entry', id, hw: e.headword }, '', `${location.pathname}#/entry/${id}`)
        window.scrollTo(0, 0) // a freshly opened entry starts at the top
      } else if (mode === 'replace') {
        // a deep link must ALSO stamp its state: without it, navigating away and pressing Back
        // pops a null-state #/entry URL and the entry never comes back (stale card stays on screen)
        history.replaceState({ view: 'entry', id, hw: e.headword }, '', `${location.pathname}#/entry/${id}`)
      }
    } catch {
      // a saved/history id that no longer exists (reassigned/removed by a DB rebuild): if we know the
      // tapped form, recover by searching it instead of showing an error.
      if (anchor) {
        loading = false
        await doSearch(anchor)
        return
      }
      err = 'could not load entry'
    } finally {
      loading = false
    }
  }

  // ── scroll restoration (arrow back/forward) ──────────────────────────────────────────────────
  // Word/entry pages remember where you'd scrolled to, so going back/forward with the arrows lands you
  // where you left off (not pinned to the top). The home page always resets to the top. Content loads
  // async, so on restore we re-apply the saved offset over a few frames as the page settles.
  const scrollPos = new Map<string, number>()
  const locKey = () => location.pathname + location.search + location.hash
  const isHome = () => !location.search && (!location.hash || location.hash === '#/')
  let rafScroll = 0
  function onScroll() {
    if (rafScroll) return
    rafScroll = requestAnimationFrame(() => {
      rafScroll = 0
      scrollPos.set(locKey(), window.scrollY)
    })
  }
  function restoreScroll(key: string) {
    const y = isHome() ? 0 : scrollPos.get(key) ?? 0
    const apply = () => window.scrollTo(0, y)
    requestAnimationFrame(() => {
      apply()
      requestAnimationFrame(apply)
    })
    setTimeout(apply, 90)
    setTimeout(apply, 220)
  }

  async function onPop(e: PopStateEvent) {
    const st = e.state as { view?: string; id?: number; q?: string; hw?: string } | null
    const key = locKey()
    if (st?.view === 'saved') {
      savedList = getSaved()
      view = 'saved'
    } else if (st?.view === 'history') {
      historyList = getHistory()
      view = 'history'
    } else if (st?.view === 'entry' && st.id != null) {
      await openEntry(st.id, 'none', st.hw ?? '')
    } else {
      // a null-state pop whose URL is still an entry deep link (stamped before the replaceState
      // fix, or by an external navigation): re-resolve from the URL instead of blanking the view
      const m = !st && location.hash.match(/^#\/entry\/(-?\d+)$/)
      if (m) {
        await openEntry(Number(m[1]), 'none')
      } else {
        view = 'results'
        entry = null
        const term = st?.q ?? ''
        // re-search when the term changed OR when the results view lost its content along the
        // way: opening an entry (from History, a row, a character tap) clears results and flips
        // searched/unified off, and a stale enrichEntry can't render without them. Without this,
        // Back landed on a blank page even though q still held the term.
        const intact = searched && (results.length > 0 || enrichEntry != null)
        if (term && (term !== q || !intact)) await doSearch(term, 'none')
        else q = term
      }
    }
    restoreScroll(key)
  }

  onMount(() => {
    // take over scroll restoration: the browser's 'auto' restores against the OLD (pre-render) layout
    // of this SPA and lands in the wrong place; we restore manually once async content has settled.
    if ('scrollRestoration' in history) history.scrollRestoration = 'manual'
    window.addEventListener('scroll', onScroll, { passive: true })
    window.addEventListener('popstate', onPop)
    // fresh-random homepage showcase (fails soft to []): loaded once on mount
    interesting(6).then((items) => (interestingList = items))
    // deep link: a shared #/entry/<id> (id may be negative for a char-only page) reopens that entry
    const m = location.hash.match(/^#\/entry\/(-?\d+)$/)
    const term = new URLSearchParams(location.search).get('q')
    if (m) openEntry(Number(m[1]), 'replace')
    else if (location.hash.startsWith('#/entry/')) {
      // malformed deep link (#/entry/abc): say so instead of silently showing the landing page
      err = 'could not load entry'
      history.replaceState({ view: 'results', q: '' }, '', location.pathname)
    } else if (location.hash === '#/saved') openSaved()
    else if (location.hash === '#/history') openHistory()
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
      window.removeEventListener('scroll', onScroll)
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
  // Files menu on iOS) right here: no separate page. The image opens in an inline panel.
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

  // the X button just EMPTIES the text: it keeps whatever page is shown (results / entry / card)
  // until you type a new character, so you can clear and retype without it blanking to the home
  // screen. (The logo, goHome, is the full reset.) lastSearched is left intact so retyping the same
  // term doesn't reload the identical page.
  function clearSearch() {
    q = ''
    err = ''
    inputEl?.focus()
  }

  // tapping the logo resets everything to a clean home
  function goHome() {
    panel = 'none'
    lookupOpen = false
    showInstallHelp = false
    showSettings = false
    ocrFile = null
    entry = null
    enrichEntry = null
    enriching = false
    unified = false
    searched = false
    results = []
    breakdown = []
    breaking = false
    segments = []
    didYouMean = []
    q = ''
    lastSearched = ''
    err = ''
    view = 'results'
    history.replaceState({ view: 'results', q: '' }, '', location.pathname)
    window.scrollTo(0, 0) // home always resets to the top
  }

  // a character was chosen from the photo selection: search it and close the panel
  function fromInput(text: string) {
    panel = 'none'
    ocrFile = null
    doSearch(text)
  }
  // a stroke-recognised character from the draw pad. replace=true swaps the last (provisional) char
  // for the new guess (Google-Translate style: the top guess auto-enters as you draw, and picking a
  // different candidate replaces it); replace=false appends a fresh character. Live-updates results.
  function fromDraw(ch: string, replace: boolean) {
    const cur = [...q]
    q = (replace && cur.length ? cur.slice(0, -1).join('') : q) + ch
    clearTimeout(timer)
    doSearch(q, 'replace')
  }

  // Escape closes the topmost overlay (keyboard path for the click-to-dismiss backdrops)
  function onEscape(e: KeyboardEvent) {
    if (e.key !== 'Escape') return
    if (showInstallHelp) showInstallHelp = false
    else if (showSettings) showSettings = false
    else if (lookupOpen) lookupOpen = false
    else if (panel !== 'none') panel = 'none' // the draw/photo dock dismisses like every other overlay
  }
</script>

<svelte:window onkeydown={onEscape} />

<div class="wrap" class:drawing={panel === 'draw'} class:wide={onWordPage}>
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
        class:hanq={inputHan}
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
        <button type="button" class="clearbtn" aria-label="clear search" onmousedown={(e) => e.preventDefault()} onclick={clearSearch} data-testid="clear"><X size={22} /></button>
      {:else}
        <!-- empty field only: a dice that opens a random word. It hides the moment you type (the clear
             button takes its place), so it never gets in the way of a real search. -->
        <button type="button" class="luckybtn" class:rolling aria-label="random word" title="I'm feeling lucky" onmousedown={(e) => e.preventDefault()} onclick={feelingLucky} data-testid="lucky"><Dices size={20} /></button>
      {/if}
      <button type="submit" class="searchbtn" aria-label="search" title="search" data-testid="search-go"><ArrowRight size={22} /></button>
      {#if loading}<span class="loadbar" aria-hidden="true"></span>{/if}
    </form>
    <button class="rowbtn" class:on={panel === 'draw'} aria-label="draw a character" aria-pressed={panel === 'draw'} title="draw" onclick={toggleDraw} data-testid="draw-toggle"><Brush size={18} /></button>
    <button class="rowbtn" class:on={panel === 'photo'} aria-label="photo or image" title="photo / image" onclick={openPhoto} data-testid="scan-toggle"><Camera size={18} /></button>
    <input bind:this={fileInput} type="file" accept="image/*" onchange={onPhotoFile} hidden />
  </div>

  {#if romanQuery || (view === 'results' && q.trim())}
    <!-- filter row: a language pill (restrict to 中/粵/日), plus - for a romanized query, which is
         ambiguous between a SOUND and a MEANING - a "read as" lens. CJK queries only get the language
         pill (they're unambiguous). -->
    <div class="scoperow" data-testid="scope">
      <span class="scopelbl">show</span>
      <div class="seg langseg" role="radiogroup" aria-label="filter by language">
        <button role="radio" aria-checked={searchLang === 'all'} class:on={searchLang === 'all'} onclick={() => setLang('all')} title="all languages">All</button>
        <button role="radio" aria-checked={searchLang === 'zh'} class:on={searchLang === 'zh'} onclick={() => setLang('zh')} title="Mandarin only">中</button>
        <button role="radio" aria-checked={searchLang === 'yue'} class:on={searchLang === 'yue'} onclick={() => setLang('yue')} title="Cantonese only">粵</button>
        <button role="radio" aria-checked={searchLang === 'ja'} class:on={searchLang === 'ja'} onclick={() => setLang('ja')} title="Japanese only">日</button>
      </div>
      {#if romanQuery}
        <span class="scopelbl scopelbl-2">read as</span>
        <div class="seg scopeseg" role="radiogroup" aria-label="how to read this query">
          <button role="radio" aria-checked={searchScope === 'auto'} class:on={searchScope === 'auto'} onclick={() => setScope('auto')} title="blend sound and meaning">Auto</button>
          <button role="radio" aria-checked={searchScope === 'sound'} class:on={searchScope === 'sound'} onclick={() => setScope('sound')} title="only words pronounced like this">Sound</button>
          <button role="radio" aria-checked={searchScope === 'meaning'} class:on={searchScope === 'meaning'} onclick={() => setScope('meaning')} title="only words that mean this in English">Meaning</button>
        </div>
      {/if}
    </div>
  {/if}

  {#if panel === 'photo' && ocrFile}
    <section class="inputpanel"><Ocr file={ocrFile} onpick={fromInput} /></section>
  {/if}

  {#if panel === 'draw'}
    <!-- inline draw pad, directly under the search row (top of the page) so it sits where you type -->
    <div class="drawpanel">
      <Pad onpick={fromDraw} onclose={() => (panel = 'none')} />
    </div>
  {/if}

  {#if err}
    <div class="err">
      {err}
      {#if view === 'results' && q.trim()}<button class="retry" onclick={() => doSearch(q, 'replace')}>retry</button>{/if}
    </div>
  {/if}

  {#if canSaveShare}
    <!-- per-page actions: icon-only save + share, on the right edge level with the big character -->
    <div class="actions">
      <button class="actbtn" class:on={savedNow} onclick={toggleSave} aria-pressed={savedNow} aria-label={savedNow ? 'remove bookmark' : 'save'} title={savedNow ? 'saved' : 'save'}>
        <Bookmark size={22} fill={savedNow ? 'currentColor' : 'none'} />
      </button>
      <button class="actbtn" onclick={shareCurrent} aria-label="share" title="share">
        {#if isIOS}<Share size={22} />{:else}<Share2 size={22} />{/if}
      </button>
    </div>
  {/if}

  {#snippet savedRow(it: SavedItem)}
    <!-- no variety chip for an ASCII text search ("dog"): it isn't a word OF one language -->
    <EntryRow
      glyph={it.headword}
      font={hanFont(it.variety)}
      lang={langTag(it.variety)}
      reading={formatReading(it.variety, it.reading, settings.romanization === 'yale', settings.jaRomaji)}
      tags={/^[\x20-\x7e]+$/.test(it.headword) ? [] : [varietyLabel(it.variety)]}
      gloss={it.gloss ? shortGloss([it.gloss]) : ''}
      onclick={() => (it.query ? doSearch(it.headword) : openEntry(it.id, 'push', it.headword))}
    />
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
        <ul class="results">{#each savedList as it (it.id + '|' + it.headword)}{@render savedRow(it)}{/each}</ul>
      {:else}
        <p class="empty">No saved words yet. Open a word and tap save.</p>
      {/if}
    </section>
  {:else if view === 'history'}
    <section class="listview">
      <h2 class="lvh">History {#if historyList.length}<button class="lvclear" onclick={wipeHistory} aria-label="clear history"><Trash2 size={14} /> clear</button>{/if}</h2>
      {#if historyList.length}
        <ul class="results">{#each historyList as it (it.id + '|' + it.headword)}{@render savedRow(it)}{/each}</ul>
      {:else}
        <p class="empty">No history yet.</p>
      {/if}
    </section>
  {:else if view === 'entry' && entry}
    {#key entry.lexeme_id}
      <Unified entry={entry} anchor={q} onsearch={doSearch} onopen={(id, g) => openEntry(id, 'push', g)} />
    {/key}
  {:else if unified && enrichEntry}
    <!-- the full entry has arrived: definition + structure + origin + used-in + bridges all rendered. -->
    <Unified hits={results} entry={enrichEntry} anchor={q} onsearch={doSearch} onopen={(id, g) => openEntry(id, 'push', g)} />
  {:else if unified && enriching}
    <!-- enriching: show the DEFINITION immediately from the search hits, with the lower sections'
         scaffolding/skeleton in place; the structure/origin/used-in content fills in when /entry
         arrives in the background. The user gets the main page at once instead of a blank skeleton. -->
    <Unified hits={results} entry={null} enriching={true} anchor={q} onsearch={doSearch} onopen={(id, g) => openEntry(id, 'push', g)} />
  {:else if unified && results.length}
    <!-- enrich finished but no entry came back (fetch failed): still show the card from search hits,
         and SAY the rest is missing: a silently reduced page reads as "that's all there is" -->
    <Unified hits={results} entry={null} anchor={q} onsearch={doSearch} onopen={(id, g) => openEntry(id, 'push', g)} />
    {#if enrichFailed}
      <div class="err">
        couldn't load the full entry
        <button class="retry" onclick={retryEnrich}>retry</button>
      </div>
    {/if}
  {:else if loading}
    {@render pageSkel()}
  {:else}
    {#if searched && !loading && results.length}
      <div class="meta">{results.length} {results.length === 1 ? 'result' : 'results'}</div>
    {/if}
    <ul class="results" data-testid="results">
      {#each results as r (r.lexeme_id)}
        {@const d = primaryForm(r.forms, r.variety, q)}
        <!-- open the tapped result by id (not a re-search): a kana/loanword row like トイレ has no Han
             glyph, so re-searching its headword just re-lists; opening by id always shows the focused
             entry. Pass the displayed form as anchor so the headword keeps its script. Only the
             INFORMATIVE region tags (TW/HK) are shown; CN/JP are redundant with the 中/日 tag. -->
        <EntryRow
          glyph={d?.primary.form ?? r.headword}
          font={hanFont(r.variety)}
          lang={langTag(r.variety)}
          alt={d?.alternate?.form ?? null}
          reading={formatReading(r.variety, r.reading, settings.romanization === 'yale', settings.jaRomaji)}
          tags={[varietyLabel(r.variety)]}
          regions={regionsOf(r).filter((rg) => rg === 'TW' || rg === 'HK')}
          gloss={shortGloss(r.glosses)}
          onclick={() => openEntry(r.lexeme_id, 'push', d?.primary.form ?? r.headword)}
        />
      {/each}
    </ul>
    {#if searched && !loading}
      {#if breakdown.length}
        <!-- every character of the query, with its meaning: shown whether the query matched no word
             OR only a partial word (so all characters always appear). When a partial word WAS caught,
             this is a "characters" breakdown under the results; otherwise it's the no-word page. -->
        <section class="noword" data-testid="breakdown">
          <div class="nw-head">
            <span class="nw-q">{q}</span>
            <!-- 古古 ("old old") is the app's name, not a real word: same page, just a cheekier note -->
            <span class="nw-note">{results.length ? 'characters' : isEasterEgg ? 'no known word, but a super cool app 😎' : 'no known word'}</span>
          </div>
          {#if compositeMeaning}<p class="nw-lit"><span class="nw-lit-k">literally</span> {compositeMeaning}</p>{/if}
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
          <button class="lookup" onclick={() => (lookupOpen = true)}><ExternalLink size={14} /> look “{q}” up</button>
        </section>
      {:else if breaking && results.length === 0}
        <!-- breakdown still loading: hold a placeholder rather than flash "nothing found" (item 3) -->
        {@render pageSkel()}
      {:else if results.length === 0}
        <div class="empty">nothing for “{q}”.</div>
        {#if didYouMean.length}
          <section class="dym" data-testid="did-you-mean">
            <span class="dym-k">did you mean</span>
            <ul class="dym-list">
              {#each didYouMean as s (s.headword + '|' + s.variety)}
                <li><button class="dym-item" onclick={() => doSearch(s.headword)}>
                  <span class="dym-hw" lang={langTag(s.variety as Hit['variety'])} style="font-family:{hanFont(s.variety as Hit['variety'])}">{s.headword}</span>
                  {#if s.reading}<span class="dym-rd">{formatReading(s.variety as Hit['variety'], s.reading, settings.romanization === 'yale', settings.jaRomaji)}</span>{/if}
                  <span class="dym-var">{varietyLabel(s.variety as Hit['variety'])}</span>
                </button></li>
              {/each}
            </ul>
          </section>
        {/if}
        <button class="lookup" onclick={() => (lookupOpen = true)}><ExternalLink size={14} /> look “{q}” up</button>
      {/if}
    {/if}
    {#if !searched && !q}
      <!-- About page: what Kogu is, what each section of an entry means, and where the data comes from.
           Stays visible when the draw/photo panel is open (the pad is docked at the bottom over it). -->
      <div class="about">
        <p class="introhw">
          <span class="intromark">古古</span> <span class="introword">Kogu</span>
          {#if canInstall}<button class="installbtn" onclick={installApp} aria-label="install as web app"><Download size={14} /> Install</button>{/if}
        </p>
        <p class="intropos"><span class="intropron">/ko.gu/</span> <span class="introtag">noun</span></p>

        <p class="introgloss">A dictionary for the Han script: one word across <b>中文</b> (Mandarin), <b>粵語</b> (Cantonese), and <b>日本語</b> (Japanese) at once, and why the written forms differ.</p>

        <h2 class="abh">On each page</h2>
        <dl class="ablist">
          <div class="abitem"><dt>Readings</dt><dd>How the word sounds in each language: <b>中</b> pinyin, <b>粵</b> jyutping, <b>日</b> kana (on and kun) with the pitch accent, the meaning beside each.</dd></div>
          <div class="abitem"><dt>Related</dt><dd>Words that carry the same meaning, including cross-language equivalents, cognates, and false friends (same writing, different meaning).</dd></div>
          <div class="abitem"><dt>Used in</dt><dd>Common words that contain the character, grouped by language.</dd></div>
          <div class="abitem"><dt>Origin</dt><dd>The etymology, kept per language since the Chinese and Japanese accounts of the same glyph can both be true.</dd></div>
          <div class="abitem"><dt>Structure</dt><dd>What a character is built from (its parts, and which carries the meaning vs the sound), and its forms across scripts: traditional, simplified, and Japanese shinjitai, with the reform that split them.</dd></div>
        </dl>

        {#if interestingList.length}
          <!-- homepage showcase: a fresh-random pick, one per category, six in all (kokuji, 日/中 false
               friends, 和製漢語, 粵字, simplified merges, English false friends). Reuses the canonical
               EntryRow; the "why" rides its optional note caption. Tap opens the full entry. -->
          <section class="showcase" data-testid="interesting">
            <h2 class="abh">Worth exploring</h2>
            <ul class="results">
              {#each interestingList as it (it.category + '|' + it.lexeme_id)}
                <EntryRow
                  glyph={it.headword}
                  font={hanFont(it.variety)}
                  lang={langTag(it.variety)}
                  reading={formatReading(it.variety, it.reading, settings.romanization === 'yale', settings.jaRomaji)}
                  tags={[varietyLabel(it.variety)]}
                  gloss={it.gloss ? shortGloss([it.gloss]) : ''}
                  note={it.why}
                  notePrimary
                  onclick={() => openEntry(it.lexeme_id, 'push', it.headword)}
                />
              {/each}
            </ul>
          </section>
        {/if}

        <h2 class="abh">Where the data comes from</h2>
        <ul class="absrc">
          <li><b><a href="https://cc-cedict.org/" target="_blank" rel="noopener noreferrer external">CC-CEDICT</a></b> and <b><a href="https://cantonese.org/" target="_blank" rel="noopener noreferrer external">CC-Canto</a></b>: Mandarin and Cantonese words and readings</li>
          <li><b><a href="https://www.edrdg.org/jmdict/j_jmdict.html" target="_blank" rel="noopener noreferrer external">JMdict</a></b> and <b><a href="https://www.edrdg.org/wiki/index.php/KANJIDIC_Project" target="_blank" rel="noopener noreferrer external">KANJIDIC</a></b>: Japanese words and kanji readings</li>
          <li><b><a href="https://github.com/mifunetoshiro/kanjium" target="_blank" rel="noopener noreferrer external">Kanjium</a></b> (CC BY-SA 4.0): Japanese pitch accent</li>
          <li><b><a href="https://www.unicode.org/charts/unihan.html" target="_blank" rel="noopener noreferrer external">Unihan</a></b> and <b><a href="https://github.com/cjkvi/cjkvi-ids" target="_blank" rel="noopener noreferrer external">cjkvi-ids</a></b>: characters, stroke data, and how they decompose</li>
          <li><b><a href="https://github.com/nk2028/tshet-uinh-data" target="_blank" rel="noopener noreferrer external">Tshet-uinh</a></b> (廣韻, Baxter): Middle Chinese readings behind the phonological notes</li>
          <li><b><a href="https://www.wiktionary.org/" target="_blank" rel="noopener noreferrer external">Wiktionary</a></b>: etymologies and phono-semantic component roles</li>
          <li><b><a href="https://github.com/BYVoid/OpenCC" target="_blank" rel="noopener noreferrer external">OpenCC</a></b>: traditional / simplified / shinjitai conversion tables behind the variant graph</li>
          <li><b><a href="https://github.com/rspeer/wordfreq" target="_blank" rel="noopener noreferrer external">wordfreq</a></b> and <b><a href="https://github.com/hermitdave/FrequencyWords" target="_blank" rel="noopener noreferrer external">FrequencyWords</a></b>: word frequency</li>
          <li><b><a href="https://omwn.org/" target="_blank" rel="noopener noreferrer external">Open Multilingual Wordnet</a></b>: part of the cross-language concept links</li>
          <li>Pronunciation clips: <b><a href="https://github.com/davinfifield/mp3-chinese-pinyin-sound" target="_blank" rel="noopener noreferrer external">mp3-chinese-pinyin-sound</a></b> (Mandarin) and <b><a href="https://jyutping.org/" target="_blank" rel="noopener noreferrer external">jyutping.org</a></b> (Cantonese); Japanese is synthesized locally with <b><a href="https://open-jtalk.sourceforge.net/" target="_blank" rel="noopener noreferrer external">Open JTalk</a></b></li>
        </ul>
        <h2 class="abh">API</h2>
        <p class="abapi">Everything on this site is served by a free, open JSON API: see the <a href="/api-docs/" target="_blank" rel="noopener">API reference</a>.</p>

        <p class="abnote">Every entry is compiled directly from these open datasets. Kogu is <a href="https://github.com/m-durana/kogu" target="_blank" rel="noopener noreferrer external">open source</a> (code MIT, data licences in NOTICE.md), and was inspired by <b><a href="https://cjkvdict.com/" target="_blank" rel="noopener noreferrer external">CJKV Dict</a></b>.</p>
      </div>
    {/if}
  {/if}

  {#if showInstallHelp}
    <!-- guided add-to-home-screen (iOS has no install API; Android uses the native prompt instead) -->
    <!-- svelte-ignore a11y_click_events_have_key_events -- backdrop dismiss; Escape (svelte:window) is the keyboard path -->
    <div class="instbg" role="presentation" onclick={() => (showInstallHelp = false)}>
      <div class="instcard" role="dialog" aria-modal="true" aria-label="install instructions" tabindex="-1" use:dialogFocus onclick={(e) => e.stopPropagation()}>
        <p class="insth">Add Kogu to your Home Screen</p>
        <ol class="inststeps">
          <!-- the li is a flex row: keep the whole sentence ONE flex item (bare text nodes become
               separate items and wrap into broken columns) -->
          <li><span class="instep">{#if isIOS}<Share size={18} />{:else}<Share2 size={18} />{/if}</span><span>Tap the <b>Share</b> button {isIOS ? 'in the toolbar below' : 'in your browser menu'}</span></li>
          <li><span class="instep"><SquarePlus size={18} /></span><span>Choose <b>Add to Home Screen</b></span></li>
        </ol>
        <button class="instok" onclick={() => (showInstallHelp = false)}>got it</button>
      </div>
      {#if isIOS}<div class="instpoint" aria-hidden="true"><ChevronDown size={18} /></div>{/if}
    </div>
  {/if}

  {#if lookupOpen && q.trim()}<LookupPanel term={q.trim()} sl={lookupSl} onclose={() => (lookupOpen = false)} />{/if}

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}

  {#if showSettings}
    <!-- svelte-ignore a11y_click_events_have_key_events -- backdrop dismiss; Escape (svelte:window) is the keyboard path -->
    <div class="setbg" role="presentation" onclick={() => (showSettings = false)}>
      <div class="setcard" role="dialog" aria-modal="true" aria-label="settings" tabindex="-1" use:dialogFocus onclick={(e) => e.stopPropagation()}>
        <div class="sethrow">
          <h2 class="seth">Settings</h2>
          <button class="setx" onclick={() => (showSettings = false)} aria-label="close"><X size={20} /></button>
        </div>
        <div class="setrow">
          <span class="setlabel">Cantonese romanization</span>
          <div class="seg">
            <button class:on={settings.romanization === 'jyutping'} onclick={() => setRomanization('jyutping')}>Jyutping</button>
            <button class:on={settings.romanization === 'yale'} onclick={() => setRomanization('yale')}>Yale</button>
          </div>
        </div>
        <div class="setrow">
          <span class="setlabel">Japanese readings</span>
          <span class="setsub">Show readings as kana (はいかい) or rōmaji (haikai).</span>
          <div class="seg">
            <button class:on={!settings.jaRomaji} onclick={() => setJaRomaji(false)}>Kana</button>
            <button class:on={settings.jaRomaji} onclick={() => setJaRomaji(true)}>Rōmaji</button>
          </div>
        </div>
        <div class="setrow">
          <span class="setlabel">Japanese pitch accent</span>
          <span class="setsub">Show the pitch-accent contour over kana readings.</span>
          <div class="seg">
            <button class:on={settings.pitchAccent} onclick={() => setPitchAccent(true)}>Show</button>
            <button class:on={!settings.pitchAccent} onclick={() => setPitchAccent(false)}>Hide</button>
          </div>
        </div>
        <div class="setrow">
          <span class="setlabel">Pronunciation audio</span>
          <span class="setsub">Tap the speaker on a reading to hear it.</span>
          <div class="seg">
            <button class:on={settings.audio} onclick={() => setAudio(true)}>On</button>
            <button class:on={!settings.audio} onclick={() => setAudio(false)}>Off</button>
          </div>
        </div>
        <div class="setrow">
          <span class="setlabel">Stored data</span>
          <span class="setsub">Removes history, saved words, preferences, and everything cached for offline use.</span>
          <button class="setclear" class:armed={clearArmed} onclick={clearAllData}>
            {clearArmed ? 'tap again to clear everything' : 'clear cache'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    max-width: 680px;
    margin: 0 auto;
  }
  /* leave room so the bottom-docked handwriting panel doesn't cover the last results */
  .wrap.drawing { padding-bottom: 52dvh; }
  .wrap {
    padding: calc(1.7rem + env(safe-area-inset-top)) calc(1.5rem + env(safe-area-inset-right))
      calc(4rem + env(safe-area-inset-bottom)) calc(1.5rem + env(safe-area-inset-left));
  }
  .bar { margin-bottom: 1rem; display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  /* item 7: history + saved buttons to the right of the wordmark (enlarged) */
  .navbtns { display: flex; gap: 0.3rem; }
  .navbtn { display: inline-flex; align-items: center; justify-content: center; width: 2.9rem; height: 2.9rem; background: none; border: none; border-radius: 50%; color: var(--muted); }
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
  .lvclear { display: inline-flex; align-items: center; gap: 0.3rem; font-family: var(--mono); font-size: 0.7rem; letter-spacing: 0.02em; color: var(--faint); background: none; border: 1px solid var(--border); border-radius: 999px; padding: 0.15rem 0.5rem; }
  .lvclear:hover { color: var(--text); border-color: var(--border-strong); }
  /* transient "Link copied" toast for share fallback */
  .toast { position: fixed; left: 50%; bottom: calc(2rem + env(safe-area-inset-bottom)); transform: translateX(-50%); background: var(--surface-2, #1c1c1f); color: var(--text); border: 1px solid var(--border-strong); border-radius: 999px; padding: 0.5rem 1rem; font-size: 0.85rem; z-index: 60; }
  /* settings panel */
  .setbg { position: fixed; inset: 0; background: rgba(0,0,0,0.5); backdrop-filter: blur(10px) saturate(1.4); -webkit-backdrop-filter: blur(10px) saturate(1.4); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 70; }
  .setcard { width: min(22rem, 100%); background: var(--bg); border: 1px solid var(--border-strong); border-radius: 16px; box-shadow: 0 12px 40px -12px rgba(0,0,0,0.7); padding: 1.1rem 1.1rem 0.9rem; }
  .sethrow { display: flex; align-items: center; justify-content: space-between; margin: 0 0 1.1rem; }
  .seth { font-family: var(--mono); font-size: 0.76rem; font-weight: 400; letter-spacing: 0.02em; color: var(--muted); margin: 0; }
  .setx { display: inline-flex; background: none; border: none; color: var(--muted); padding: 0.2rem; border-radius: var(--r); }
  .setx:hover { color: var(--text); background: var(--surface); }
  .setrow { display: flex; flex-direction: column; gap: 0.45rem; padding: 0.9rem 0; border-top: 0.5px solid var(--border); }
  .setrow:first-of-type { border-top: none; padding-top: 0; }
  .setlabel { font-family: var(--sans); font-size: 0.95rem; color: var(--text); }
  .setsub { font-size: 0.8rem; color: var(--faint); line-height: 1.4; margin-top: -0.15rem; }
  /* quiet outline button on the flat-black card; arming fills it so the second tap reads as deliberate */
  .setclear { align-self: start; font-family: var(--mono); font-size: 0.78rem; letter-spacing: 0.02em; color: var(--muted); background: none; border: 1px solid var(--border-strong); border-radius: 999px; padding: 0.45rem 0.95rem; }
  .setclear:hover { color: var(--text); }
  .setclear.armed { background: var(--text); color: var(--bg); border-color: var(--text); }
  /* track-style segmented control: a quiet rounded track, only the SELECTED segment is filled. No
     per-segment border/divider, so the unselected side has no stray outline of the selector (item 7). */
  .seg { display: inline-flex; gap: 2px; padding: 2px; background: var(--surface); border-radius: 999px; align-self: start; margin-top: 0.15rem; }
  /* 0.6rem vertical padding keeps each toggle ~34px tall: the old 0.38rem gave 25px targets,
     well under a comfortable touch size on phones */
  .seg button { font-family: var(--mono); font-size: 0.78rem; letter-spacing: 0.02em; color: var(--muted); background: none; border: none; border-radius: 999px; padding: 0.6rem 0.95rem; }
  .seg button:hover { color: var(--text); background: none; }
  .seg button.on { background: var(--text); color: var(--bg); }
  .seg button.on:hover { color: var(--bg); }
  .brand { margin: 0; font-weight: 400; }
  .brandbtn { display: inline-flex; align-items: baseline; gap: 0.45rem; background: none; border: none; padding: 0; }
  .brandbtn:hover { background: none; }
  /* item 6: larger top-left wordmark */
  .brand .mark { font-family: var(--han); font-weight: 500; font-size: 1.6rem; letter-spacing: -0.04em; color: var(--text); }
  .brand .word { font-family: var(--sans); font-size: 1.15rem; letter-spacing: 0.04em; color: var(--muted); }

  /* scope lens row under the search field (romanized queries only) */
  .scoperow { display: flex; align-items: center; gap: 0.6rem; margin: -0.2rem 0 0.9rem; flex-wrap: wrap; }
  .scopelbl { font-family: var(--mono); font-size: 0.68rem; letter-spacing: 0.02em; color: var(--faint); }
  .scopelbl-2 { margin-left: 0.5rem; }
  /* a slightly more compact take on the settings segmented control */
  .scopeseg, .langseg { margin-top: 0; }
  .scopeseg button, .langseg button { padding: 0.32rem 0.72rem; font-size: 0.72rem; }
  /* the 中/粵/日 pills use the Han font so the glyphs match the rest of the UI */
  .langseg button { font-family: var(--han); }
  .langseg button:first-child { font-family: var(--sans); }

  .searchrow { display: flex; align-items: stretch; margin-bottom: 0.7rem; }
  .field { position: relative; flex: 1; min-width: 0; display: flex; }
  .searchicon { position: absolute; left: 0.8rem; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; display: flex; }
  /* matches the Hybrid mockup's .search: 0.6rem block padding + 18px radius (same height→same roundness) */
  .field input {
    width: 100%; padding: 0.6rem 5.2rem 0.6rem 2.4rem; font-size: 1.02rem; line-height: 1.15;
    font-family: var(--sans); color: var(--text); -webkit-appearance: none; appearance: none;
    background: var(--surface); border: 1px solid transparent; border-radius: 18px;
  }
  /* optical correction: CJK ink is top-heavy relative to the Latin baseline (see inputHan) */
  .field input.hanq { padding-top: calc(0.6rem + 2px); padding-bottom: calc(0.6rem - 2px); }
  .field input::-webkit-search-decoration, .field input::-webkit-search-cancel-button { -webkit-appearance: none; appearance: none; }
  .field input:focus { border-color: transparent; background: var(--surface-2); }
  .field input::placeholder { color: var(--faint); }
  /* loading indicator: a thin sliding bar along the bottom of the field while a search is in flight */
  .loadbar { position: absolute; left: 1px; right: 1px; bottom: 1px; height: 2px; overflow: hidden; border-radius: 0 0 18px 18px; pointer-events: none; }
  .loadbar::after { content: ''; position: absolute; inset: 0; width: 40%; background: var(--muted); border-radius: 2px; animation: loadslide 0.9s ease-in-out infinite; }
  @keyframes loadslide { 0% { transform: translateX(-110%); } 100% { transform: translateX(360%); } }
  @media (prefers-reduced-motion: reduce) { .loadbar::after { animation-duration: 2s; } }
  /* item 1: monochrome selection so highlighting typed text doesn't look odd */
  .field input::selection { background: var(--muted); color: var(--bg); }
  .clearbtn {
    position: absolute; right: 2.9rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
  }
  .clearbtn:hover { color: var(--hi); background: var(--surface-2); }
  /* "feeling lucky" dice: shares the clear button's slot (only one shows at a time — clear when there's
     a query, dice when the field is empty) */
  .luckybtn {
    position: absolute; right: 2.9rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--faint); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
    transition: color 0.15s;
  }
  .luckybtn:hover { color: var(--hi); background: var(--surface-2); }
  .luckybtn.rolling { color: var(--accent); animation: luckyroll 0.5s ease; }
  @keyframes luckyroll { from { transform: translateY(-50%) rotate(0); } to { transform: translateY(-50%) rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .luckybtn.rolling { animation: none; } }
  /* item 1: search button to the right of the X */
  .searchbtn {
    position: absolute; right: 0.4rem; top: 50%; transform: translateY(-50%);
    border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex;
  }
  .searchbtn:hover { color: var(--text); background: var(--surface-2); }
  /* circular icon buttons, matching the round nav buttons */
  .rowbtn {
    flex: none; display: inline-flex; align-items: center; justify-content: center; align-self: center;
    width: 2.9rem; height: 2.9rem; margin-left: 0.4rem;
    color: var(--muted); background: var(--surface); border: none; border-radius: 50%;
    overflow: hidden;
    transition: width 0.22s ease, opacity 0.18s ease, margin 0.22s ease, padding 0.22s ease;
  }
  .rowbtn:hover { color: var(--hi); background: var(--surface-2); }
  .rowbtn.on { color: var(--bg); background: var(--text); }
  /* item 1: focusing the field expands it to FULL width; draw + camera slide away. Must also zero the
     padding/border: with box-sizing:border-box a width:0 button still can't shrink below its 24px
     horizontal padding, which left the field ~48px short of full width (item: search bar only went
     halfway). Collapsing padding+border too lets it reach 0 and the field fills the row. */
  /* visibility (not just opacity) also removes the collapsed buttons from the Tab order:
     focus used to land on a fully invisible 0-width button two Tabs after the input */
  .searchrow.focused .rowbtn { width: 0; padding: 0; border: 0; margin-left: 0; opacity: 0; pointer-events: none; visibility: hidden; }
  @media (prefers-reduced-motion: reduce) { .rowbtn { transition: none; } }
  /* draw pad: a FLOATING panel just under the search row. position:absolute with no offset keeps it at
     its natural place in flow but lifts it OUT of flow, so the results list / about text render full
     height behind it and simply continue past: the pad overlays them instead of pushing them down. */
  /* docked handwriting panel (Google Translate / PLECO style): fixed to the bottom of the screen,
     full width, content scrolls behind it. Not a floating window. */
  .drawpanel {
    position: fixed;
    left: 0; right: 0; bottom: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    /* default: a comfortable docked strip (candidate row + canvas), like Google Translate's collapsed
       handwriting pad. The expand button on the candidate strip grows it to (near) full screen. */
    height: min(46dvh, 460px);
    background: var(--surface-2);
    border-top: 1px solid var(--border-strong);
    box-shadow: 0 -8px 24px -12px rgba(0, 0, 0, 0.6);
    padding: 0.6rem calc(0.8rem + env(safe-area-inset-right)) calc(0.6rem + env(safe-area-inset-bottom)) calc(0.8rem + env(safe-area-inset-left));
  }
  /* the pad fills the dock full width (edge to edge), not a centered box */
  .drawpanel :global(.pad) { flex: 1; min-height: 0; width: 100%; }

  /* inline photo selection, shown directly under the search row */
  .inputpanel { margin-bottom: 1.2rem; }

  .meta { color: var(--faint); font-size: 0.76rem; margin-bottom: 0.6rem; font-family: var(--mono); letter-spacing: 0.02em; }
  .err { color: var(--text); margin: 0.5rem 0; }
  .err .retry {
    background: none; border: none; color: var(--muted); text-decoration: underline;
    text-underline-offset: 0.2em; padding: 0 0 0 0.35rem; font-size: inherit; cursor: pointer;
  }
  .err .retry:hover { color: var(--text); }

  /* results - an editorial list of EntryRow rows (the one shared row style); the <ul> just resets. */
  .results { list-style: none; margin: 0; padding: 0; }
  .empty { color: var(--faint); padding: 1.2rem 0; }

  /* no-word breakdown: the query shown big (like a headword), a quiet note, then tappable chars */
  .noword { padding: 0.6rem 0; }
  .nw-head { display: flex; align-items: baseline; gap: 0.9rem; margin-bottom: 0.9rem; flex-wrap: wrap; }
  .nw-q { font-family: var(--han); font-size: 2.1rem; line-height: 1.05; color: var(--text); }
  .nw-note { color: var(--faint); font-size: 0.76rem; font-family: var(--mono); letter-spacing: 0.02em; }
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
  /* a literal character-by-character gloss chain for an unmatched query (honest hint, not a definition) */
  .nw-lit { margin: 0 0 0.9rem; font-size: 0.95rem; line-height: 1.5; color: var(--muted); }
  .nw-lit-k { font-family: var(--mono); font-size: 0.68rem; letter-spacing: 0.02em; color: var(--faint); margin-right: 0.5rem; }
  /* "did you mean …": closest real entries for a query that matched nothing */
  .dym { margin: 0.9rem 0 0.2rem; }
  .dym-k { font-family: var(--mono); font-size: 0.68rem; letter-spacing: 0.02em; color: var(--faint); }
  .dym-list { list-style: none; margin: 0.4rem 0 0; padding: 0; }
  .dym-list li + li { border-top: 1px solid var(--border); }
  .dym-item { display: flex; align-items: baseline; gap: 0.6rem; width: 100%; text-align: left; background: none; border: none; border-radius: var(--r); padding: 0.55rem 0.5rem; }
  .dym-item:hover { background: var(--surface); color: var(--text); }
  .dym-hw { font-family: var(--han); font-size: 1.3rem; line-height: 1.1; color: var(--text); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: none; max-width: 9em; }
  .dym-rd { font-family: var(--mono); font-size: 0.78rem; color: var(--muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dym-var { font-family: var(--han); font-size: 0.78rem; color: var(--faint); margin-left: auto; flex: none; }
  /* "look it up on the web": an external lookup that works regardless of whether Kogu has the word */
  .lookup { display: inline-flex; align-items: center; gap: 0.4rem; margin-top: 1rem; font-family: var(--mono); font-size: 0.74rem; letter-spacing: 0.02em; color: var(--muted); background: none; border: 1px solid var(--border); border-radius: var(--r); padding: 0.4rem 0.7rem; }
  .lookup:hover { color: var(--text); border-color: var(--border-strong); background: var(--surface); }
  /* About page (item 2): what Kogu is, what each section means, and the data sources */
  .about { padding: 1rem 0.2rem 2rem; max-width: 58ch; }
  .introhw { margin: 0; display: flex; align-items: baseline; gap: 0.5rem; flex-wrap: wrap; }
  .introhw .intromark { font-family: var(--han); font-weight: 500; font-size: 2.1rem; letter-spacing: -0.04em; color: var(--text); }
  .introhw .introword { font-family: var(--sans); font-size: 1.4rem; letter-spacing: 0.04em; color: var(--muted); }
  .intropos { margin: 0.35rem 0 1rem; display: flex; align-items: baseline; gap: 0.6rem; }
  .intropron { font-family: var(--mono); font-size: 0.95rem; color: var(--faint); }
  .introtag { font-family: var(--mono); font-size: 0.76rem; letter-spacing: 0.02em; color: var(--faint); }
  /* install-as-web-app button (item 2) */
  /* install button sits to the right of the 古古 Kogu wordmark (item 139) */
  .installbtn { display: inline-flex; align-items: center; gap: 0.35rem; margin-left: auto; align-self: center; font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.02em; color: var(--text); background: none; border: 1px solid var(--border-strong); border-radius: var(--r); padding: 0.3rem 0.6rem; }
  .installbtn:hover { background: var(--surface); }
  /* guided add-to-home-screen overlay */
  .instbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px) saturate(1.4); -webkit-backdrop-filter: blur(10px) saturate(1.4); z-index: 80; display: flex; align-items: center; justify-content: center; padding: 1.2rem; }
  .instcard { width: min(22rem, 100%); background: var(--bg); border: 1px solid var(--border-strong); border-radius: 16px; box-shadow: 0 12px 40px -12px rgba(0,0,0,0.7); padding: 1.1rem; }
  .insth { font-family: var(--mono); font-size: 0.76rem; letter-spacing: 0.02em; color: var(--muted); margin: 0 0 0.9rem; }
  .inststeps { margin: 0 0 1rem; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 0.7rem; }
  .inststeps li { display: flex; align-items: center; gap: 0.6rem; font-size: 0.95rem; line-height: 1.4; color: var(--muted); }
  .inststeps b { color: var(--text); font-weight: 500; }
  .instep { display: inline-flex; align-items: center; justify-content: center; width: 2rem; height: 2rem; flex: none; border: 1px solid var(--border-strong); border-radius: var(--r); color: var(--text); }
  .instpoint { position: fixed; left: 50%; bottom: calc(0.5rem + env(safe-area-inset-bottom)); transform: translateX(-50%); color: var(--hi); font-size: 2rem; animation: instbob 1.1s ease-in-out infinite; }
  @keyframes instbob { 0%,100% { transform: translate(-50%, 0); } 50% { transform: translate(-50%, 0.4rem); } }
  @media (prefers-reduced-motion: reduce) { .instpoint { animation: none; } }
  .introgloss { font-family: var(--sans); font-size: 1.05rem; line-height: 1.7; color: var(--text); margin: 0 0 1.6rem; }
  .introgloss b, .ablist b, .absrc b { font-family: var(--han); font-weight: 500; }
  .abh { font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.02em; color: var(--faint); margin: 1.6rem 0 0.6rem; }
  .ablist { margin: 0; }
  .ablist dt { font-family: var(--sans); font-size: 0.98rem; color: var(--text); font-weight: 500; margin-top: 0.7rem; }
  .ablist dd { margin: 0.1rem 0 0; font-size: 0.92rem; line-height: 1.6; color: var(--muted); }
  .ablist b { font-weight: 500; }
  .absrc { list-style: none; margin: 0; padding: 0; }
  .absrc li { font-size: 0.92rem; line-height: 1.55; color: var(--muted); padding: 0.28rem 0; border-top: 1px solid var(--border); }
  .absrc li:first-child { border-top: none; }
  .absrc b { color: var(--text); font-family: var(--sans); font-weight: 500; }
  .abnote, .abapi { font-size: 0.88rem; line-height: 1.6; color: var(--faint); margin: 1.3rem 0 0; }
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

  /* popup close language (matches the bound-form modal's mono text close) */
  .instok { display: block; margin-left: auto; font-family: var(--mono); font-size: 0.76rem; letter-spacing: 0.02em; color: var(--muted); background: none; border: none; padding: 0.3rem 0; }
  .instok:hover { color: var(--text); background: none; border: none; }

  /* ── desktop ──────────────────────────────────────────────────────────────────────────────────
     the phone-first column reads lost on a large screen: give the content a wider measure and pin
     the handwriting dock to the column instead of the full viewport edge. */
  @media (min-width: 1100px) {
    /* ONE wrap width for every view, so the wordmark + search bar never shift when you open an entry
       (they used to jump because entries were 1128px wide and everything else 880px). Entry pages
       split into two columns WITHIN this width; the About page and lists fill or sit inside it. */
    .wrap { max-width: 1200px; }
    /* header is identical on every view: the search field fills the content width, left-aligned, so it
       never re-centres or resizes between pages (no shift) and leaves no dead space to its right. */
    .searchrow { max-width: none; margin-left: 0; margin-right: 0; }
    /* result / list surfaces: one readable column, left-aligned (a full-width row would fling the glyph
       and its gloss to opposite edges), so these stay at a scannable measure under the wide search bar */
    .results, .empty, .dym, .noword { max-width: 840px; }
    /* the About page fills the full width. Its reference list flows as balanced multi-column: each
       .abitem (dt+dd) is break-inside:avoid so a heading never orphans from its text, and columns pack
       top-to-bottom instead of a 2-col GRID that row-aligns and left a dead void under the short
       column (5 items split 3/2 -> empty cell under "Origin"). */
    .about { max-width: none; }
    /* the intro spans the content width (was capped at 62ch, which left a wide void to its right) */
    .about .introgloss { max-width: none; }
    .ablist { columns: 2; column-gap: 3rem; margin-top: 0.2rem; }
    .abitem { break-inside: avoid; }
    /* the first heading in each column shouldn't carry the inter-item top margin (it reads as a gap
       above the column); the grid's row-1 items used to sit flush with the "On each page" label. */
    .ablist .abitem:first-child dt { margin-top: 0; }
    .absrc { columns: 2; column-gap: 3rem; }
    .absrc li { break-inside: avoid; }
    .absrc li:first-child { border-top: none; }
    .showcase .results { display: grid; grid-template-columns: 1fr 1fr; column-gap: 3rem; max-width: none; }
    /* save/share sit to the RIGHT OF THE HEADER GLYPH, not at the far page edge: cap the row to the
       entry's left column width (grid is 5fr/7fr with a 3.2rem gap) and right-align inside it, so the
       icons hug the headword column instead of floating above the right column and leaving a gap. */
    .actions { margin: 1.3rem 0 -3.05rem; max-width: calc((100% - 3.2rem) * 5 / 12); }
    .drawpanel {
      /* floor matches the wrap's own 1.5rem padding edge so the dock stays flush with the column */
      left: max(calc(50vw - 600px + 1.5rem), 1.5rem); right: auto;
      width: min(880px, calc(100vw - 3rem));
      border: 1px solid var(--border-strong); border-bottom: none;
      border-radius: 16px 16px 0 0;
    }
  }
</style>
