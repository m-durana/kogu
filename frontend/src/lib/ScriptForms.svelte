<script lang="ts">
  import type { ScriptForms } from './types'
  import { scriptShort, orderBranches } from './display'

  // The character's script family (繁 → 简 · 日), anchored on the orthodox glyph, with the reform
  // that caused each divergence. The looked-up form is highlighted; kokuji are labelled as such.
  let {
    forms,
    anchor = '',
    onsearch,
    compact = false,
  }: {
    forms: ScriptForms | null
    anchor?: string
    onsearch: (q: string) => void
    compact?: boolean
  } = $props()

  const ordered = $derived(forms ? orderBranches(forms.branches) : [])
</script>

{#if forms}
  {#if forms.is_kokuji}
    <div class="sf" class:compact>
      <span class="tag">日</span><span class="g">{forms.orthodox}</span>
      <span class="cap">Japanese-coined · no Chinese form</span>
    </div>
  {:else}
    <div class="sf" class:compact>
      {#each ordered as b, i}
        {#if i === 1}<span class="arrow" aria-hidden="true">→</span>{/if}
        <span class="branch">
          <button class="b" class:cur={b.form === anchor} onclick={() => onsearch(b.form)} title="look up {b.form}">
            <span class="tag">{scriptShort(b.script)}</span><span class="g">{b.form}</span>
          </button>
          {#if b.reform_label}<span class="cap">{b.reform_label}</span>{/if}
        </span>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .sf { display: flex; flex-wrap: wrap; align-items: flex-start; gap: 0.5rem 0.7rem; }
  .branch { display: inline-flex; flex-direction: column; align-items: flex-start; gap: 0.15rem; }
  .b {
    display: inline-flex; align-items: baseline; gap: 0.3rem; background: none; border: none;
    padding: 0.1rem 0.2rem; border-radius: var(--r); border: 1px solid transparent;
  }
  .b:hover { background: var(--surface); }
  /* the looked-up form used to get a boxed ring (border + fill) — it read as a stray outline,
     especially on a simplified glyph. Mark "current" with a quiet underline on the glyph instead. */
  .b.cur { background: none; }
  .b.cur .g { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--border-strong); }
  .tag { font-family: var(--mono); font-size: 0.62rem; color: var(--faint); }
  .g { font-family: var(--han); font-size: 1.7rem; line-height: 1; color: var(--text); }
  .arrow { color: var(--faint); align-self: center; }
  .cap { font-family: var(--mono); font-size: 0.6rem; color: var(--faint); letter-spacing: 0.02em; padding-left: 0.2rem; }

  .sf.compact .g { font-size: 1.25rem; }
  .sf.compact { gap: 0.3rem 0.5rem; }
</style>
