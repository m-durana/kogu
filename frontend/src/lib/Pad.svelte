<script lang="ts">
  import { recognize, type Stroke } from './api'
  import { X, ChevronDown, ChevronUp } from '@lucide/svelte'

  // onpick(ch, replace): replace=true swaps the provisional character already in the field for `ch`
  // (Google-Translate style — the top guess auto-enters, picking another replaces it).
  let { onpick, onclose }: { onpick: (ch: string, replace: boolean) => void; onclose?: () => void } = $props()

  // The candidate strip is normally a single side-scrolling row. The expand button grows it into a
  // wrapped grid that fills the dock, covering the canvas; retracting returns to the side-scroll row.
  let optionsOpen = $state(false)

  let canvas: HTMLCanvasElement
  let drawing = $state(false)
  let strokes: Stroke[] = []
  let current: Stroke = []
  let candidates = $state<string[]>([])
  // a provisional (auto-entered) character is currently in the field for the strokes being drawn
  let live = false
  let busy = $state(false)
  let error = $state('')
  let t0 = 0
  let timer: ReturnType<typeof setTimeout> | undefined

  // Strokes are stored in CSS pixel coordinates (same units for x and y), so the canvas can be any
  // aspect ratio without distortion. cw/ch track the current CSS-pixel size of the canvas, which is
  // also what gets passed to recognize() so the stroke coordinates and the bounds agree.
  let cw = 0
  let ch = 0

  // While the pad is open, drawing strokes that stray off the canvas would otherwise drag-select the
  // page text behind it. Disable text selection document-wide for as long as the pad is mounted, and
  // restore the previous value on close.
  $effect(() => {
    const body = document.body
    const prev = body.style.userSelect
    const prevWebkit = body.style.getPropertyValue('-webkit-user-select')
    body.style.userSelect = 'none'
    body.style.setProperty('-webkit-user-select', 'none')
    return () => {
      body.style.userSelect = prev
      body.style.setProperty('-webkit-user-select', prevWebkit)
    }
  })

  // Size the canvas backing store to its actual displayed pixel size (× dpr for crispness) and keep
  // it in sync as the dock resizes / rotates. The 2d context is scaled by dpr so all drawing is done
  // in CSS pixels, matching the stroke coordinates. Existing strokes are redrawn after every resize
  // so a resize mid-character doesn't wipe what's already there.
  $effect(() => {
    const dpr = window.devicePixelRatio || 1
    const fit = () => {
      const w = canvas.clientWidth
      const h = canvas.clientHeight
      if (!w || !h) return
      cw = w
      ch = h
      canvas.width = Math.round(w * dpr)
      canvas.height = Math.round(h * dpr)
      const c = ctx()
      c.setTransform(dpr, 0, 0, dpr, 0, 0)
      redraw()
    }
    fit()
    const ro = new ResizeObserver(fit)
    ro.observe(canvas)
    return () => ro.disconnect()
  })

  const ctx = () => canvas.getContext('2d')!
  function strokeWidth(): number {
    return Math.max(6, ch / 35)
  }
  function applyStrokeStyle(c: CanvasRenderingContext2D) {
    c.strokeStyle = '#ededeb'
    c.lineWidth = strokeWidth()
    c.lineCap = 'round'
    c.lineJoin = 'round'
  }
  // Repaint all completed strokes (used after a resize, which clears the backing store).
  function redraw() {
    const c = ctx()
    c.clearRect(0, 0, cw, ch)
    applyStrokeStyle(c)
    for (const s of strokes) {
      if (!s.length) continue
      c.beginPath()
      c.moveTo(s[0][0], s[0][1])
      for (let i = 1; i < s.length; i++) c.lineTo(s[i][0], s[i][1])
      c.stroke()
    }
  }
  function pos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect()
    // CSS pixels, same units for x and y — no aspect distortion.
    return [e.clientX - r.left, e.clientY - r.top]
  }
  function start(e: PointerEvent) {
    e.preventDefault()
    clearTimeout(timer)
    try {
      canvas.setPointerCapture(e.pointerId)
    } catch {}
    drawing = true
    if (strokes.length === 0 && current.length === 0) t0 = performance.now()
    const [x, y] = pos(e)
    current = [[x, y, performance.now() - t0]]
    const c = ctx()
    c.beginPath()
    c.moveTo(x, y)
  }
  function move(e: PointerEvent) {
    if (!drawing) return
    e.preventDefault()
    const [x, y] = pos(e)
    current.push([x, y, performance.now() - t0])
    const c = ctx()
    c.lineTo(x, y)
    applyStrokeStyle(c)
    c.stroke()
  }
  function end() {
    if (!drawing) return
    drawing = false
    if (current.length) strokes.push(current)
    current = []
    // re-recognise on every stroke (lightly debounced so a fast multi-stroke character doesn't spam
    // /recognize); the candidate list refreshes as you add strokes instead of waiting a full second.
    clearTimeout(timer)
    timer = setTimeout(run, 160)
  }
  // clear the drawing/candidates (for the next character), without closing the dock
  function reset() {
    clearTimeout(timer)
    strokes = []
    current = []
    candidates = []
    error = ''
    optionsOpen = false
    ctx().clearRect(0, 0, cw, ch)
  }
  // tapping a candidate commits THIS character (replacing the auto-entered provisional) and advances:
  // the canvas clears so the next strokes are a fresh character.
  function pick(ch: string) {
    onpick(ch, live)
    live = false
    reset()
  }
  function clear() {
    // nothing drawn yet → the X doubles as "close the draw box"
    if (strokes.length === 0 && current.length === 0 && candidates.length === 0) {
      onclose?.()
      return
    }
    reset()
    live = false
  }
  async function run() {
    if (strokes.length === 0) return
    busy = true
    error = ''
    try {
      // pass the real CSS-pixel bounds, matching the stroke coordinates
      const res = await recognize(cw, ch, strokes, ['zh', 'ja'])
      candidates = res.candidates
      if (candidates.length === 0) {
        error = 'no match, try again'
      } else {
        // auto-enter the top guess (replacing the previous provisional as more strokes refine it)
        onpick(candidates[0], live)
        live = true
      }
    } catch {
      error = 'recogniser unavailable'
    } finally {
      busy = false
    }
  }
</script>

<div class="pad" class:optionsopen={optionsOpen}>
  <!-- candidate strip on top (Google Translate / PLECO docked style). The candidates side-scroll in
       their own track; the expand + clear buttons are pinned to the right and never scroll away. -->
  <div class="candstrip">
    <div class="candscroll" data-testid="pad-candidates">
      {#if candidates.length}
        {#each candidates as ch, i}
          {#if i}<span class="csep" aria-hidden="true">│</span>{/if}
          <button class="cand" onclick={() => pick(ch)}>{ch}</button>
        {/each}
      {:else if busy}
        <span class="pad-status">recognising…</span>
      {:else if error}
        <span class="pad-status">{error}</span>
      {:else}
        <span class="pad-status">draw a character below</span>
      {/if}
    </div>
    <div class="padbtns">
      <button
        class="padbtn"
        onclick={() => (optionsOpen = !optionsOpen)}
        disabled={!candidates.length}
        data-testid="pad-expand"
        aria-label={optionsOpen ? 'collapse options' : 'expand options'}
        aria-pressed={optionsOpen}
      >
        {#if optionsOpen}<ChevronUp size={18} />{:else}<ChevronDown size={18} />{/if}
      </button>
      <button class="padbtn" onclick={clear} data-testid="pad-clear" aria-label="clear / close"><X size={18} /></button>
    </div>
  </div>

  <div class="canvas-wrap">
    <canvas
      bind:this={canvas}
      onpointerdown={start}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={end}
      oncontextmenu={(e) => e.preventDefault()}
      aria-label="handwriting canvas, draw a character"
    ></canvas>
  </div>
</div>

<style>
  .pad {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    /* fill the dock vertically so the canvas can grow into the available space */
    flex: 1;
    min-height: 0;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }
  /* candidate strip: a side-scrolling track of candidates + a pinned button group on the right */
  .candstrip { display: flex; align-items: center; gap: 0.3rem; flex: none; }
  /* the candidates scroll horizontally inside their own track; the buttons sit outside it (pinned) */
  .candscroll { display: flex; align-items: center; gap: 0.15rem; overflow-x: auto; scrollbar-width: none; min-height: 2.4rem; flex: 1; min-width: 0; }
  .candscroll::-webkit-scrollbar { display: none; }
  /* expanded: the options grow DOWN into a wrapped grid that fills the dock and covers the canvas */
  .pad.optionsopen .candstrip { flex: 1; min-height: 0; align-items: flex-start; }
  .pad.optionsopen .candscroll {
    flex-wrap: wrap; align-content: flex-start;
    overflow-x: hidden; overflow-y: auto;
    min-height: 0; height: 100%; padding-top: 0.15rem;
  }
  .pad.optionsopen .canvas-wrap { display: none; }
  /* keep the vertical separators between candidates even in the expanded grid (item: "add the vertical
     lines even if you expand the menu"); give candidates room to be tapped */
  .pad.optionsopen .cand { padding: 0.35rem 0.7rem; }
  /* the wrapper grows to fill remaining dock height; the canvas fills the wrapper */
  .canvas-wrap { position: relative; display: flex; flex: 1; min-height: 160px; }
  canvas {
    display: block;
    /* the whole dock IS the drawing surface: full-bleed, no box/border. Sits directly on the dock
       background so there is no inner frame — you write across the entire area. */
    width: 100%;
    height: 100%;
    min-height: 160px;
    background: transparent;
    border: none;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    -webkit-tap-highlight-color: transparent;
    cursor: crosshair;
  }
  /* expand + clear/close, grouped at the very right of the candidate strip */
  .padbtns { margin-left: auto; flex: none; display: flex; gap: 0.25rem; }
  .padbtn {
    display: inline-flex; align-items: center; justify-content: center;
    padding: 0.4rem; color: var(--muted);
    background: none; border: 1px solid var(--border); border-radius: var(--r);
  }
  .padbtn:hover { color: var(--hi); border-color: var(--border-strong); }
  .padbtn[aria-pressed='true'] { color: var(--hi); border-color: var(--border-strong); }
  .pad-status { color: var(--faint); font-size: 0.85rem; }
  .csep { color: var(--border-strong); flex: none; }
  .cand { font-family: var(--han); font-size: 1.7rem; padding: 0.1rem 0.5rem; background: none; border: none; color: var(--text); border-radius: var(--r); flex: none; }
  .cand:hover { background: var(--surface); }
</style>
