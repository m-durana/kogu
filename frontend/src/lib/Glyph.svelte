<script lang="ts">
  import { glyphWikiUrl } from './display'
  // A Han glyph that falls back to a GlyphWiki SVG when the device font can't render it. Rare CJK
  // Ext-B/C/G ideographs (U+20000+) are missing from most installed fonts and show as tofu (□); for
  // those we swap in glyphwiki.org's vector for the exact codepoint so nothing renders blank.
  let {
    ch,
    font,
    lang = undefined,
    cls = '',
  }: { ch: string; font: string; lang?: string | undefined; cls?: string } = $props()

  let missing = $state(false)
  const cp = $derived(ch && [...ch].length === 1 ? (ch.codePointAt(0) ?? 0) : 0)
  const gwUrl = $derived(glyphWikiUrl(ch) ?? '')

  $effect(() => {
    missing = false
    // BMP CJK (U+3400–U+9FFF) is universally covered; only the supplementary planes are worth probing.
    if (!cp || cp < 0x20000 || typeof document === 'undefined') return
    try {
      const ctx = document.createElement('canvas').getContext('2d')
      if (!ctx) return
      ctx.font = `48px ${font}`
      const w = ctx.measureText(ch).width
      const tofu = ctx.measureText('\u{10FFFF}').width // a noncharacter: always the font's .notdef box
      missing = w === 0 || Math.abs(w - tofu) < 0.5
    } catch {
      missing = false
    }
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
