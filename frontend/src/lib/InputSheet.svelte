<script lang="ts">
  import Pad from './Pad.svelte'
  import Ocr from './Ocr.svelte'
  import { X } from '@lucide/svelte'

  let {
    onpick,
    onclose,
    mode: initialMode = 'draw',
  }: { onpick: (text: string) => void; onclose: () => void; mode?: 'draw' | 'photo' } = $props()

  // the two non-keyboard input modes, full-screen and focused — one at a time.
  // seeded from the prop (the sheet is re-created on each open) — initial capture is intended.
  // svelte-ignore state_referenced_locally
  let mode = $state<'draw' | 'photo'>(initialMode)
</script>

<div class="overlay" role="dialog" aria-modal="true" aria-label="character input">
  <div class="top">
    <div class="seg" role="group" aria-label="input method">
      <button aria-pressed={mode === 'draw'} onclick={() => (mode = 'draw')}>draw</button>
      <button aria-pressed={mode === 'photo'} onclick={() => (mode = 'photo')}>photo</button>
    </div>
    <button class="close" onclick={onclose} aria-label="close input"><X size={20} /></button>
  </div>

  <div class="body">
    {#if mode === 'draw'}
      <Pad {onpick} fill />
    {:else}
      <Ocr {onpick} />
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    padding: calc(0.8rem + env(safe-area-inset-top)) calc(0.9rem + env(safe-area-inset-right))
      calc(1rem + env(safe-area-inset-bottom)) calc(0.9rem + env(safe-area-inset-left));
    gap: 0.9rem;
  }
  .top { display: flex; justify-content: space-between; align-items: center; }
  .seg { display: inline-flex; gap: 2px; padding: 3px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); }
  .seg button { border: none; background: transparent; color: var(--muted); padding: 0.4rem 1rem; border-radius: calc(var(--r-lg) - 5px); font-size: 0.85rem; }
  .seg button:hover { color: #fff; background: var(--surface-2); }
  .seg button[aria-pressed='true'] { background: var(--border-strong); color: #fff; }
  .close { border: none; background: transparent; color: var(--muted); padding: 0.4rem; border-radius: var(--r); display: inline-flex; }
  .close:hover { color: #fff; background: var(--surface-2); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
