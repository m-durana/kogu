<script lang="ts">
  import { ocr } from './api'
  import type { OcrResponse } from './types'
  import { ocrSelectedText } from './display'
  import { Camera, ImagePlus, X } from '@lucide/svelte'

  let { onpick }: { onpick: (text: string) => void } = $props()

  let imageUrl = $state<string | null>(null)
  let resp = $state<OcrResponse | null>(null)
  let busy = $state(false)
  let error = $state('')
  // selection keyed by "lineIdx-charIdx"
  let selected = $state<Set<string>>(new Set())

  // downscale a captured image to <=MAX px and return a JPEG blob (smaller upload, better OCR)
  const MAX = 1600
  async function toBlob(file: File): Promise<Blob> {
    const bmp = await createImageBitmap(file)
    const scale = Math.min(1, MAX / Math.max(bmp.width, bmp.height))
    const w = Math.round(bmp.width * scale)
    const h = Math.round(bmp.height * scale)
    const canvas = document.createElement('canvas')
    canvas.width = w
    canvas.height = h
    canvas.getContext('2d')!.drawImage(bmp, 0, 0, w, h)
    return new Promise((res) => canvas.toBlob((b) => res(b!), 'image/jpeg', 0.85))
  }

  async function onFile(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0]
    if (!file) return
    error = ''
    resp = null
    selected = new Set()
    busy = true
    try {
      const blob = await toBlob(file)
      if (imageUrl) URL.revokeObjectURL(imageUrl)
      imageUrl = URL.createObjectURL(blob)
      resp = await ocr(blob)
      if (resp.lines.length === 0) error = 'no text found'
    } catch (err) {
      error = (err as Error).message === 'ocr_unavailable' ? 'OCR is unavailable' : 'recognition failed'
    } finally {
      busy = false
    }
  }

  function toggle(li: number, ci: number) {
    const key = `${li}-${ci}`
    const next = new Set(selected)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    selected = next
  }
  function selectLine(li: number) {
    const next = new Set(selected)
    const line = resp!.lines[li]
    const allSel = line.chars.every((_, ci) => next.has(`${li}-${ci}`))
    line.chars.forEach((_, ci) => (allSel ? next.delete(`${li}-${ci}`) : next.add(`${li}-${ci}`)))
    selected = next
  }

  // selected text in document order
  const selectedText = $derived(ocrSelectedText(resp?.lines ?? [], selected))

  function pct(v: number, total: number) {
    return `${(v / total) * 100}%`
  }
</script>

<div class="ocr">
  {#if !imageUrl}
    <div class="choices">
      <label class="capture">
        <Camera size={18} aria-hidden="true" /> take photo
        <input type="file" accept="image/*" capture="environment" onchange={onFile} hidden />
      </label>
      <label class="capture">
        <ImagePlus size={18} aria-hidden="true" /> attach image
        <input type="file" accept="image/*" onchange={onFile} hidden />
      </label>
    </div>
    <p class="hint">snap a sign, page, or screenshot — then tap the characters to look up.</p>
  {:else}
    <div class="stage">
      <img src={imageUrl} alt="captured" />
      {#if resp}
        <div class="overlay">
          {#each resp.lines as line, li}
            {#each line.chars as c, ci}
              <button
                class="cbox"
                class:sel={selected.has(`${li}-${ci}`)}
                style="left:{pct(c.box[0], resp.width)};top:{pct(c.box[1], resp.height)};width:{pct(c.box[2], resp.width)};height:{pct(c.box[3], resp.height)}"
                title={c.ch}
                aria-label="select {c.ch}"
                onclick={() => toggle(li, ci)}
              ></button>
            {/each}
            <button
              class="linetag"
              style="left:{pct(line.box[0], resp.width)};top:{pct(line.box[1], resp.height)}"
              onclick={() => selectLine(li)}
              title="select whole line">{line.text.length}</button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if busy}<div class="status">recognising…</div>{/if}
  {#if error}<div class="status err">{error}</div>{/if}

  {#if imageUrl}
    <div class="bar">
      <label class="retake"><X size={14} aria-hidden="true" /> new
        <input type="file" accept="image/*" capture="environment" onchange={onFile} hidden /></label>
      {#if selectedText}
        <span class="sel-text">{selectedText}</span>
        <button class="lookup" onclick={() => onpick(selectedText)}>look up</button>
      {:else if resp && resp.lines.length}
        <span class="hint2">tap characters to select</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .ocr { display: flex; flex-direction: column; gap: 0.7rem; }
  .choices { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .capture, .retake {
    display: inline-flex; align-items: center; gap: 0.4rem; cursor: pointer;
    background: var(--surface); border: 1px solid var(--border-strong); border-radius: var(--r);
    padding: 0.6rem 0.9rem; font-size: 0.9rem; align-self: flex-start;
  }
  .capture:hover, .retake:hover { border-color: var(--text); color: #fff; }
  .hint { color: var(--faint); font-size: 0.85rem; margin: 0; }
  .stage { position: relative; display: inline-block; max-width: 100%; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .stage img { display: block; max-width: 100%; height: auto; }
  .overlay { position: absolute; inset: 0; }
  .cbox {
    position: absolute; padding: 0; margin: 0; background: transparent;
    border: 1px solid rgba(244, 244, 242, 0.35); border-radius: 2px; cursor: pointer; min-width: 0;
  }
  .cbox:hover { border-color: #fff; background: rgba(244, 244, 242, 0.12); }
  .cbox.sel { background: rgba(244, 244, 242, 0.55); border-color: #fff; }
  .linetag {
    position: absolute; transform: translate(-50%, -100%); font-size: 0.6rem; padding: 0 0.25rem;
    background: var(--bg); border: 1px solid var(--border-strong); color: var(--faint); opacity: 0.7;
  }
  .status { font-size: 0.85rem; color: var(--muted); }
  .status.err { color: var(--text); }
  .bar { display: flex; align-items: center; gap: 0.7rem; flex-wrap: wrap; }
  .sel-text { font-family: var(--han); font-size: 1.5rem; }
  .hint2 { color: var(--faint); font-size: 0.85rem; }
  .lookup { background: var(--text); color: var(--bg); border: none; padding: 0.5rem 0.9rem; font-size: 0.9rem; }
</style>
