<script lang="ts">
  import { recognize, type Stroke } from './api'
  import { Eraser, ScanLine } from '@lucide/svelte'

  let { onpick }: { onpick: (ch: string) => void } = $props()

  const SIZE = 280
  let canvas: HTMLCanvasElement
  let drawing = $state(false)
  let strokes: Stroke[] = []
  let current: Stroke = []
  let candidates = $state<string[]>([])
  let busy = $state(false)
  let error = $state('')
  let t0 = 0

  function ctx() {
    return canvas.getContext('2d')!
  }
  function pos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect()
    return [((e.clientX - r.left) / r.width) * SIZE, ((e.clientY - r.top) / r.height) * SIZE]
  }
  function start(e: PointerEvent) {
    e.preventDefault()
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
    const [x, y] = pos(e)
    current.push([x, y, performance.now() - t0])
    const c = ctx()
    c.lineTo(x, y)
    c.strokeStyle = '#ededeb'
    c.lineWidth = 8
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
      if (candidates.length === 0) error = 'no candidates'
    } catch (e) {
      error = 'recogniser unavailable'
    } finally {
      busy = false
    }
  }
</script>

<div class="pad">
  <canvas
    bind:this={canvas}
    width={SIZE}
    height={SIZE}
    onpointerdown={start}
    onpointermove={move}
    onpointerup={end}
    onpointerleave={end}
    aria-label="handwriting canvas"
  ></canvas>
  <div class="pad-actions">
    <button onclick={clear} data-testid="pad-clear" class="ib"><Eraser size={15} /> clear</button>
    <button onclick={run} disabled={busy} data-testid="pad-recognize" class="ib">
      <ScanLine size={15} /> {busy ? '…' : 'recognise'}
    </button>
  </div>
  {#if error}<div class="pad-error">{error}</div>{/if}
  {#if candidates.length}
    <div class="cands" data-testid="pad-candidates">
      {#each candidates as ch}
        <button class="cand" onclick={() => onpick(ch)}>{ch}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pad { display: flex; flex-direction: column; gap: 0.6rem; }
  canvas {
    width: 280px;
    height: 280px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    touch-action: none;
    cursor: crosshair;
  }
  .pad-actions { display: flex; gap: 0.5rem; }
  .ib { display: inline-flex; align-items: center; gap: 0.3rem; }
  .pad-error { color: var(--accent); font-size: 0.8rem; }
  .cands { display: flex; flex-wrap: wrap; gap: 0.4rem; max-width: 280px; }
  .cand {
    font-family: var(--han);
    font-size: 1.5rem;
    padding: 0.3rem 0.5rem;
    min-width: 2.6rem;
  }
</style>
