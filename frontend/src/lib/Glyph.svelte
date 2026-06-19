<script module lang="ts">
  // Memoised font-coverage probes, keyed by `${codepoint}|${font}` — the same glyphs recur constantly
  // across an entry, so we measure each (codepoint, font) pair at most once per session.
  const probeCache = new Map<string, boolean>()
</script>

<script lang="ts">
  import { glyphWikiUrl } from './display'
  // A Han glyph that falls back to a GlyphWiki SVG when the device font can't render it. This covers
  // BOTH the rare supplementary-plane ideographs (U+20000+, missing from almost every installed font)
  // AND ordinary BMP characters the loaded fonts happen to lack — e.g. a traditional-only form like 關
  // on a device whose Simplified-first font stack doesn't carry it. Without the BMP case those showed
  // as a permanent tofu box (□) with no fallback.
  let {
    ch,
    font,
    lang = undefined,
    cls = '',
  }: { ch: string; font: string; lang?: string | undefined; cls?: string } = $props()

  const cp = $derived(ch && [...ch].length === 1 ? (ch.codePointAt(0) ?? 0) : 0)
  const gwUrl = $derived(glyphWikiUrl(ch) ?? '')
  // worth probing: any CJK ideograph (Ext-A onward). BMP CJK starts at U+3400.
  const probable = $derived(cp >= 0x3400 && !!gwUrl)

  // Probe result: null until the canvas measurement runs, then true/false for "device font has it".
  let fontHas = $state<boolean | null>(null)
  // Render decision:
  //  • Supplementary plane (≥U+20000): assume ABSENT until proven present, so the SVG shows on the
  //    first paint (these are almost never in a font; avoids a tofu→image pop-in).
  //  • BMP (U+3400–U+1FFFF): assume PRESENT (the common case) and only swap to the SVG once the probe
  //    CONFIRMS the fonts lack it — so common characters never trigger a needless network image.
  const missing = $derived(
    probable &&
      (cp >= 0x20000 ? fontHas !== true : fontHas === false),
  )

  $effect(() => {
    if (!probable || typeof document === 'undefined') {
      fontHas = null
      return
    }
    const key = `${cp}|${font}`
    const cached = probeCache.get(key)
    if (cached !== undefined) {
      fontHas = cached
      return
    }
    fontHas = null
    const measure = () => {
      try {
        const ctx = document.createElement('canvas').getContext('2d')
        if (!ctx) return
        ctx.font = `48px ${font}`
        const w = ctx.measureText(ch).width
        const tofu = ctx.measureText('\u{10FFFF}').width // noncharacter: always the font's .notdef box
        const has = !(w === 0 || Math.abs(w - tofu) < 0.5)
        probeCache.set(key, has)
        fontHas = has
      } catch {
        fontHas = true // can't probe → trust the font, show the glyph rather than a network image
      }
    }
    // measure AFTER the web fonts settle: probing mid-swap would falsely report a font-less glyph.
    if (document.fonts?.ready) document.fonts.ready.then(measure)
    else measure()
  })
</script>

{#if missing && gwUrl}
  <img class="gw {cls}" src={gwUrl} alt={ch} {lang} />
{:else}
  <span class={cls} {lang} style="font-family:{font}">{ch}</span>
{/if}

<style>
  /* size to the surrounding text (1em) and invert GlyphWiki's black artwork for the dark theme */
  .gw {
    display: inline-block;
    height: 1em;
    width: 1em;
    vertical-align: -0.15em;
    filter: invert(1);
  }
</style>
