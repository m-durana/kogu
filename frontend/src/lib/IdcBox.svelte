<script lang="ts">
  // A tiny monochrome schematic of a character's top-level composition — the "box thing" that
  // replaces spelling out "side by side" / "stacked top to bottom". Driven purely by the Ideographic
  // Description Character (⿰⿱⿴…). Component slots are filled boxes; an enclosure draws its frame.
  let { idc, size = 18 }: { idc: string | null; size?: number } = $props()

  type Rect = { x: number; y: number; w: number; h: number }
  type Layout = { enclose: boolean; slots: Rect[] }

  // 24×24 grid, 2px margin, 2px gaps. Enclosures draw the outer frame + an inner slot positioned to
  // show where the frame opens.
  const LAYOUTS: Record<string, Layout> = {
    '⿰': { enclose: false, slots: [r(2, 2, 9, 20), r(13, 2, 9, 20)] },
    '⿱': { enclose: false, slots: [r(2, 2, 20, 9), r(2, 13, 20, 9)] },
    '⿲': { enclose: false, slots: [r(2, 2, 6, 20), r(9, 2, 6, 20), r(16, 2, 6, 20)] },
    '⿳': { enclose: false, slots: [r(2, 2, 20, 6), r(2, 9, 20, 6), r(2, 16, 20, 6)] },
    '⿴': { enclose: true, slots: [r(8, 8, 8, 8)] },
    '⿵': { enclose: true, slots: [r(8, 11, 8, 9)] }, // open below
    '⿶': { enclose: true, slots: [r(8, 2, 8, 9)] }, // open above
    '⿷': { enclose: true, slots: [r(11, 8, 9, 8)] }, // open right
    '⿸': { enclose: true, slots: [r(11, 11, 9, 9)] }, // open lower-right
    '⿹': { enclose: true, slots: [r(2, 11, 9, 9)] }, // open lower-left
    '⿺': { enclose: true, slots: [r(11, 2, 9, 9)] }, // open upper-right (辶 廴 type)
    '⿻': { enclose: false, slots: [r(2, 2, 15, 15), r(7, 7, 15, 15)] }, // overlapping
  }
  function r(x: number, y: number, w: number, h: number): Rect {
    return { x, y, w, h }
  }
  const layout = $derived(idc ? LAYOUTS[idc] ?? null : null)
</script>

{#if layout}
  <svg class="idc" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    {#if layout.enclose}<rect class="frame" x="2" y="2" width="20" height="20" rx="1.5" />{/if}
    {#each layout.slots as s}<rect class="slot" x={s.x} y={s.y} width={s.w} height={s.h} rx="1" />{/each}
  </svg>
{/if}

<style>
  .idc { display: inline-block; vertical-align: -0.25em; }
  .frame { fill: none; stroke: var(--muted); stroke-width: 1.4; }
  .slot { fill: var(--muted); opacity: 0.32; stroke: var(--muted); stroke-width: 1; }
</style>
