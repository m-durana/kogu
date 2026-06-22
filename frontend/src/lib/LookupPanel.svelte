<script lang="ts">
  // In-app lookup for a term Kogu doesn't have a full entry for (names, neologisms, partial phrases),
  // or any term the user wants more on: a Translate tab (our /mt proxy → Google, MyMemory fallback) and
  // a Definition tab (Wiktionary's CORS REST API). When Wiktionary has no page for a multi-character
  // phrase, it falls back to the per-character entries so something useful always shows.
  import { X, ExternalLink } from '@lucide/svelte'

  let { term, sl = 'auto', onclose }: { term: string; sl?: string; onclose: () => void } = $props()

  type WiktSense = { pos: string; lang: string; defs: string[] }
  let tab = $state<'translate' | 'define'>('translate')
  let trLoading = $state(false)
  let translation = $state<string | null>(null)
  let trError = $state(false)
  let wkLoading = $state(false)
  let wkSenses = $state<WiktSense[]>([])
  let wkPerChar = $state(false)
  let wkError = $state(false)
  let trDone = false
  let wkDone = false

  const stripTags = (h: string) => h.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim()
  const LANG = (c: string) => ({ zh: 'Chinese', ja: 'Japanese', yue: 'Cantonese', en: 'English', ko: 'Korean', vi: 'Vietnamese' } as Record<string, string>)[c] ?? c

  async function runTranslate() {
    if (trDone) return
    trDone = true
    trLoading = true
    try {
      const r = await fetch(`/api/mt?q=${encodeURIComponent(term)}&sl=${encodeURIComponent(sl)}`)
      const d = await r.json()
      if (d.translation) translation = d.translation
      else trError = true
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
    if (wkDone) return
    wkDone = true
    wkLoading = true
    try {
      let senses = await wiktFor(term)
      if (!senses.length && [...term].length > 1) {
        // no page for the whole phrase — show each character's entry instead
        wkPerChar = true
        for (const ch of [...term]) {
          const s = await wiktFor(ch)
          if (s.length) senses.push({ pos: ch, lang: '', defs: s.flatMap((x) => x.defs).slice(0, 2) })
        }
      }
      if (senses.length) wkSenses = senses
      else wkError = true
    } catch {
      wkError = true
    } finally {
      wkLoading = false
    }
  }

  $effect(() => {
    if (tab === 'translate') runTranslate()
    else runDefine()
  })
</script>

<div class="lpbg" role="presentation" onclick={onclose}>
  <div class="lp" role="dialog" aria-modal="true" aria-label="look up {term}" onclick={(e) => e.stopPropagation()}>
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
        {#if wkPerChar}<p class="lpnote">No Wiktionary entry for the whole word — showing each character.</p>{/if}
        {#each wkSenses as s}
          <div class="lpsense">
            <span class="lppos">{wkPerChar ? s.pos : (LANG(s.lang) + (s.pos ? ' · ' + s.pos : ''))}</span>
            <ol>{#each s.defs as d}<li>{d}</li>{/each}</ol>
          </div>
        {/each}
      {:else if wkError}<p class="lpdim">No Wiktionary entry found.</p>{/if}
    </div>

    <a class="lpweb" href={`https://www.google.com/search?q=${encodeURIComponent(term)}`} target="_blank" rel="noopener">
      <ExternalLink size={13} /> search the web
    </a>
  </div>
</div>

<style>
  .lpbg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; padding: 1.2rem; z-index: 60; }
  .lp { width: min(30rem, 100%); max-height: 80vh; overflow-y: auto; background: var(--surface-2); border: 1px solid var(--border-strong); border-radius: var(--r-lg); padding: 1rem 1.1rem 0.9rem; }
  .lph { display: flex; align-items: center; justify-content: space-between; gap: 0.6rem; margin-bottom: 0.7rem; }
  .lpterm { font-family: var(--han); font-size: 1.6rem; line-height: 1.1; color: var(--text); }
  .lpx { display: inline-flex; background: none; border: none; color: var(--muted); padding: 0.2rem; border-radius: var(--r); }
  .lpx:hover { color: var(--text); background: var(--surface); }
  .lptabs { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--r); overflow: hidden; margin-bottom: 0.9rem; }
  .lptabs button { font-family: var(--mono); font-size: 0.74rem; color: var(--muted); background: none; border: none; padding: 0.35rem 0.9rem; }
  .lptabs button + button { border-left: 1px solid var(--border-strong); }
  .lptabs button.on { background: var(--text); color: var(--bg); }
  .lpbody { min-height: 3rem; }
  .lpdim { color: var(--faint); font-size: 0.92rem; }
  .lptr { font-size: 1.3rem; line-height: 1.4; color: var(--text); margin: 0; font-family: var(--sans); }
  .lpsrc { font-family: var(--mono); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--faint); margin: 0.4rem 0 0; }
  .lpnote { color: var(--faint); font-size: 0.82rem; font-style: italic; margin: 0 0 0.7rem; }
  .lpsense { margin-bottom: 0.8rem; }
  .lppos { font-family: var(--mono); font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }
  .lpsense ol { margin: 0.3rem 0 0; padding-left: 1.2rem; }
  .lpsense li { font-size: 0.95rem; line-height: 1.5; color: var(--text); margin-bottom: 0.2rem; }
  .lpweb { display: inline-flex; align-items: center; gap: 0.4rem; margin-top: 0.6rem; font-family: var(--mono); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); text-decoration: none; }
  .lpweb:hover { color: var(--text); }
</style>
