<script lang="ts">
  import { recognize, type Stroke } from './api'
  import { Eraser, ScanLine } from '@lucide/svelte'

  let { onpick, fill = false }: { onpick: (ch: string) => void; fill?: boolean } = $props()

  const SIZE = 384 // backing resolution; CSS size scales independently
  let canvas: HTMLCanvasElement
  let drawing = $state(false)
  let strokes: Stroke[] = []
  let current: Stroke = []
  let candidates = $state<string[]>([])
  let busy = $state(false)
  let error = $state('')
  let t0 = 0

  const ctx = () => canvas.getContext('2d')!
  function pos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect()
    return [((e.clientX - r.left) / r.width) * SIZE, ((e.clientY - r.top) / r.height) * SIZE]
  }
  function start(e: PointerEvent) {
    e.preventDefault()
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
  }
  function clear() {
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
      if (candidates.length === 0) error = 'no match — try again'
    } catch {
      error = 'recogniser unavailable'
    } finally {
      busy = false
    }
  }
</script>

<div class="pad" class:fill>
  <canvas
    bind:this={canvas}
    width={SIZE}
    height={SIZE}
    onpointerdown={start}
    onpointermove={move}
    onpointerup={end}
    onpointercancel={end}
    oncontextmenu={(e) => e.preventDefault()}
    aria-label="handwriting canvas — draw a character"
  ></canvas>

  {#if candidates.length}
    <div class="cands" data-testid="pad-candidates">
      {#each candidates as ch}
        <button class="cand" onclick={() => onpick(ch)}>{ch}</button>
      {/each}
    </div>
  {:else if error}
    <div class="pad-error">{error}</div>
  {:else}
    <div class="pad-hint">draw one character, then recognise</div>
  {/if}

  <div class="pad-actions">
    <button onclick={clear} data-testid="pad-clear" class="ib"><Eraser size={16} aria-hidden="true" /> clear</button>
    <button onclick={run} disabled={busy} data-testid="pad-recognize" class="ib primary">
      <ScanLine size={16} aria-hidden="true" /> {busy ? '…' : 'recognise'}
    </button>
  </div>
</div>

<style>
  .pad {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }
  .pad.fill { flex: 1; min-height: 0; }
  canvas {
    width: 280px;
    height: 280px;
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
  /* full-screen: the canvas is the hero — a big centred square */
  .pad.fill canvas {
    width: min(88vw, 56vh);
    height: min(88vw, 56vh);
    align-self: center;
    flex: none;
  }
  .pad-actions { display: flex; gap: 0.6rem; }
  .pad.fill .pad-actions { justify-content: center; }
  .ib { display: inline-flex; align-items: center; gap: 0.35rem; padding: 0.6rem 1rem; }
  .ib.primary { background: var(--text); color: var(--bg); border-color: var(--text); }
  .pad-error { color: var(--text); font-size: 0.85rem; text-align: center; }
  .pad-hint { color: var(--faint); font-size: 0.85rem; text-align: center; font-style: italic; }
  .cands { display: flex; flex-wrap: wrap; gap: 0.4rem; justify-content: center; max-height: 22vh; overflow-y: auto; }
  .cand { font-family: var(--han); font-size: 1.7rem; padding: 0.3rem 0.6rem; min-width: 2.8rem; }
</style>
