<script lang="ts">
  import { recognize, type Stroke } from './api'
  import { X } from '@lucide/svelte'

  let { onpick, onclose }: { onpick: (ch: string) => void; onclose?: () => void } = $props()

  const SIZE = 384 // backing resolution; CSS size scales independently
  let canvas: HTMLCanvasElement
  let drawing = $state(false)
  let strokes: Stroke[] = []
  let current: Stroke = []
  let candidates = $state<string[]>([])
  let busy = $state(false)
  let error = $state('')
  let t0 = 0
  let timer: ReturnType<typeof setTimeout> | undefined

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

  const ctx = () => canvas.getContext('2d')!
  function pos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect()
    return [((e.clientX - r.left) / r.width) * SIZE, ((e.clientY - r.top) / r.height) * SIZE]
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
    c.strokeStyle = '#ededeb'
    c.lineWidth = 11
    c.lineCap = 'round'
    c.lineJoin = 'round'
    c.stroke()
  }
  function end() {
    if (!drawing) return
    drawing = false
    if (current.length) strokes.push(current)
    current = []
    // auto-recognise once the pen has rested for a moment
    clearTimeout(timer)
    timer = setTimeout(run, 1000)
  }
  function clear() {
    // nothing drawn yet → the X doubles as "close the draw box"
    if (strokes.length === 0 && current.length === 0 && candidates.length === 0) {
      onclose?.()
      return
    }
    clearTimeout(timer)
    strokes = []
    current = []
    candidates = []
    error = ''
    ctx().clearRect(0, 0, SIZE, SIZE)
  }
  async function run() {
    if (strokes.length === 0) return
    busy = true
    error = ''
    try {
      const res = await recognize(SIZE, SIZE, strokes, ['zh', 'ja'])
      candidates = res.candidates
      if (candidates.length === 0) error = 'no match, try again'
    } catch {
      error = 'recogniser unavailable'
    } finally {
      busy = false
    }
  }
</script>

<div class="pad">
  <div class="canvas-wrap">
    <canvas
      bind:this={canvas}
      width={SIZE}
      height={SIZE}
      onpointerdown={start}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={end}
      oncontextmenu={(e) => e.preventDefault()}
      aria-label="handwriting canvas, draw a character"
    ></canvas>
    <button class="clearx" onclick={clear} data-testid="pad-clear" aria-label="clear"><X size={18} /></button>
  </div>

  {#if candidates.length}
    <div class="cands" data-testid="pad-candidates">
      {#each candidates as ch}
        <button class="cand" onclick={() => onpick(ch)}>{ch}</button>
      {/each}
    </div>
  {:else if busy}
    <div class="pad-status">recognising…</div>
  {:else if error}
    <div class="pad-status">{error}</div>
  {/if}
</div>

<style>
  .pad {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }
  .canvas-wrap { position: relative; width: 100%; }
  canvas {
    display: block;
    width: 100%;
    aspect-ratio: 1 / 1;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    -webkit-tap-highlight-color: transparent;
    cursor: crosshair;
  }
  .clearx {
    position: absolute; top: 0.5rem; right: 0.5rem;
    display: inline-flex; align-items: center; justify-content: center;
    padding: 0.4rem; color: var(--muted);
    background: var(--bg); border: 1px solid var(--border); border-radius: var(--r);
  }
  .clearx:hover { color: #fff; border-color: var(--border-strong); }
  .pad-status { color: var(--faint); font-size: 0.85rem; text-align: center; }
  .cands { display: flex; flex-wrap: wrap; gap: 0.4rem; justify-content: center; }
  .cand { font-family: var(--han); font-size: 1.7rem; padding: 0.3rem 0.6rem; min-width: 2.8rem; }
</style>
