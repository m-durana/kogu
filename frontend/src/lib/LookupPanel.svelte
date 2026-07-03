<script lang="ts">
  // In-app lookup for a term Kogu doesn't have a full entry for (names, neologisms, partial phrases),
  // or any term the user wants more on: a Translate tab (our /mt proxy → Google, MyMemory fallback) and
  // a Definition tab (Wiktionary's CORS REST API). When Wiktionary has no page for a multi-character
  // phrase, it falls back to the per-character entries so something useful always shows.
  import { X, ExternalLink } from '@lucide/svelte'
  import { untrack } from 'svelte'

  let { term, sl = 'auto', onclose }: { term: string; sl?: string; onclose: () => void } = $props()

  type WiktSense = { pos: string; lang: string; defs: string[] }
  let tab = $state<'translate' | 'define'>('translate')
  let trLoading = $state(false)
  let translation = $state<string | null>(null)
  let trError = $state(false)
  let wkLoading = $state(false)
  let wkSenses = $state<WiktSense[]>([])
  let wkError = $state(false)
  let trDone = false
  let wkDone = false

  const stripTags = (h: string) => h.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim()
  const LANG = (c: string) => ({ zh: 'Chinese', ja: 'Japanese', yue: 'Cantonese', en: 'English', ko: 'Korean', vi: 'Vietnamese' } as Record<string, string>)[c] ?? c

  async function runTranslate() {
    if (trDone || trLoading) return // mark done only on SUCCESS, so a failed fetch can be retried
    trLoading = true
    trError = false
    try {
      const r = await fetch(`/api/mt?q=${encodeURIComponent(term)}&sl=${encodeURIComponent(sl)}`)
      const d = await r.json()
      if (d.translation) {
        translation = d.translation
        trDone = true
      } else trError = true
    } catch {
      trError = true
    } finally {
      trLoading = false
    }
  }

  // one Wiktionary REST definition lookup → flattened senses (CJK languages first)
  async function wiktFor(word: string): Promise<WiktSense[]> {
    const r = await fetch(`https://en.wiktionary.org/api/rest_v1/page/definition/${encodeURIComponent(word)}`)
    if (!r.ok) return []
    const d = await r.json()
    const out: WiktSense[] = []
    for (const code of ['zh', 'ja', 'yue', 'ko', 'vi', 'en']) {
      for (const block of d[code] ?? []) {
        const defs = (block.definitions ?? []).map((x: any) => stripTags(x.definition || '')).filter(Boolean)
        if (defs.length) out.push({ pos: block.partOfSpeech || '', lang: code, defs: defs.slice(0, 4) })
      }
    }
    return out
  }

  async function runDefine() {
    if (wkDone || wkLoading) return // mark done only on SUCCESS, so a failed lookup can be retried
    wkLoading = true
    wkError = false
    try {
      // a single Wiktionary lookup for the whole term. No per-character fallback: if the user wants a
      // component character's meaning, they can just search it in Kogu.
      const senses = await wiktFor(term)
      if (senses.length) {
        wkSenses = senses
        wkDone = true
      } else wkError = true
    } catch {
      wkError = true
    } finally {
      wkLoading = false
    }
  }

  // Run the active tab's loader when the tab changes. untrack the call so the loaders' internal reads
  // (wkLoading/wkDone/trDone guards) don't become effect dependencies: otherwise toggling wkLoading
  // re-fires this effect, and a no-result Dictionary lookup (which never sets wkDone) loops forever.
  $effect(() => {
    const t = tab
    untrack(() => (t === 'translate' ? runTranslate() : runDefine()))
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -- backdrop dismiss; Escape (svelte:window) is the keyboard path -->
<div class="lpbg" role="presentation" onclick={onclose}>
  <div class="lp" role="dialog" aria-modal="true" aria-label="look up {term}" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <div class="lph">
      <span class="lpterm">{term}</span>
      <button class="lpx" onclick={onclose} aria-label="close"><X size={18} /></button>
    </div>
    <div class="lptabs">
      <button class:on={tab === 'translate'} onclick={() => (tab = 'translate')}>Translate</button>
      <button class:on={tab === 'define'} onclick={() => (tab = 'define')}>Dictionary</button>
    </div>

    <div class="lpbody">
      {#if tab === 'translate'}
        {#if trLoading}<p class="lpdim">translating…</p>
        {:else if translation}<p class="lptr">{translation}</p><p class="lpsrc">machine translation · English</p>
        {:else if trError}<p class="lpdim">couldn't translate this.</p>{/if}
      {:else if wkLoading}<p class="lpdim">looking up…</p>
      {:else if wkSenses.length}
        {#each wkSenses as s}
          <div class="lpsense">
            <span class="lppos">{LANG(s.lang) + (s.pos ? ' · ' + s.pos : '')}</span>
            <ol>{#each s.defs as d}<li>{d}</li>{/each}</ol>
          </div>
        {/each}
        <p class="lpsrc">Wiktionary</p>
      {:else if wkError}<p class="lpdim">No Wiktionary entry found.</p>{/if}
    </div>

    <!-- rel="external" hints the OS to hand the URL to the SYSTEM browser instead of opening an in-app
         tab inside the installed PWA (best-effort: there's no guaranteed web API to break out). -->
    <a class="lpweb" href={`https://www.google.com/search?q=${encodeURIComponent(term)}`} target="_blank" rel="noopener noreferrer external">
      <ExternalLink size={13} /> search the web
    </a>
  </div>
</div>

<style>
  .lpbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); backdrop-filter: blur(10px) saturate(1.4); -webkit-backdrop-filter: blur(10px) saturate(1.4); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 60; }
  .lp { width: min(30rem, 100%); max-height: 80vh; overflow-y: auto; background: var(--bg); border: 1px solid var(--border-strong); border-radius: 16px; box-shadow: 0 12px 40px -12px rgba(0, 0, 0, 0.7); padding: 1rem 1.1rem 0.9rem; }
  .lph { display: flex; align-items: center; justify-content: space-between; gap: 0.6rem; margin-bottom: 0.7rem; }
  .lpterm { font-family: var(--han); font-size: 1.6rem; line-height: 1.1; color: var(--text); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lpx { display: inline-flex; background: none; border: none; color: var(--muted); padding: 0.2rem; border-radius: var(--r); }
  .lpx:hover { color: var(--text); background: var(--surface); }
  /* track-style segmented control (matches Settings): only the selected tab is filled, no per-cell
     outline on the unselected one (item 7/8). */
  .lptabs { display: inline-flex; gap: 2px; padding: 2px; background: var(--surface); border-radius: 999px; margin-bottom: 0.9rem; }
  .lptabs button { font-family: var(--mono); font-size: 0.78rem; letter-spacing: 0.02em; color: var(--muted); background: none; border: none; border-radius: 999px; padding: 0.38rem 0.95rem; }
  .lptabs button:hover { color: var(--text); background: none; }
  .lptabs button.on { background: var(--text); color: var(--bg); }
  .lptabs button.on:hover { color: var(--bg); }
  .lpbody { min-height: 3rem; }
  .lpdim { color: var(--faint); font-size: 0.92rem; }
  .lptr { font-size: 1.3rem; line-height: 1.4; color: var(--text); margin: 0; font-family: var(--sans); }
  .lpsrc { font-family: var(--mono); font-size: 0.68rem; letter-spacing: 0.02em; color: var(--faint); margin: 0.4rem 0 0; }
  .lpsense { margin-bottom: 0.8rem; }
  .lppos { font-family: var(--mono); font-size: 0.7rem; letter-spacing: 0.02em; color: var(--muted); }
  .lpsense ol { margin: 0.3rem 0 0; padding-left: 1.2rem; }
  .lpsense li { font-size: 0.95rem; line-height: 1.5; color: var(--text); margin-bottom: 0.2rem; }
  .lpweb { display: inline-flex; align-items: center; gap: 0.4rem; margin-top: 0.6rem; font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.02em; color: var(--muted); text-decoration: none; }
  .lpweb:hover { color: var(--text); }
</style>
