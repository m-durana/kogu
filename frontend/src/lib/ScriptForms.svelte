<script lang="ts">
  import type { ScriptForms } from './types'
  import { scriptShort, orderBranches } from './display'
  import Glyph from './Glyph.svelte'

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
  // the reform arrow sits between the traditional form(s) and what they became: with merged
  // simplifications there can be SEVERAL traditional parents before it (乾 幹 榦 → 干)
  const arrowAt = $derived(ordered.findIndex((b) => !b.script.split('+').includes('traditional')))
</script>

{#if forms}
  {#if forms.is_kokuji}
    <div class="sf" class:compact>
      <span class="tag">日</span><span class="g"><Glyph ch={forms.orthodox} font="var(--han-ja)" /></span>
      <span class="cap">Japanese-coined · no Chinese form</span>
    </div>
  {:else}
    <div class="sf" class:compact>
      {#each ordered as b, i}
        {#if i === arrowAt && i > 0}<span class="arrow" aria-hidden="true">→</span>{/if}
        <span class="branch">
          <!-- the reform behind each change is explained as a full sentence below the strip (item 14),
               so no bare "PRC simplification" caption hangs under the glyph here. -->
          <button class="b" class:cur={b.form === anchor} onclick={() => onsearch(b.form)} title="look up {b.form}">
            <span class="tag">{scriptShort(b.script)}</span><span class="g"><Glyph ch={b.form} font={b.script.includes('shinjitai') ? 'var(--han-ja)' : b.script.includes('simplified') ? 'var(--han)' : 'var(--han-tc)'} /></span>
          </button>
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
  /* the looked-up form is marked by a stronger tag colour only: no ring, no underline (item 14) */
  .b.cur { background: none; }
  .b.cur .tag { color: var(--text); font-weight: 600; }
  /* muted, not faint: TC/SC/JP here tell you WHICH form you're looking at (the def rows' .ftag
     uses the same tier), and faint (#4b4b4e on black) is below readable contrast */
  .tag { font-family: var(--mono); font-size: 0.62rem; color: var(--muted); }
  .g { font-family: var(--han); font-size: 1.7rem; line-height: 1; color: var(--text); }
  .arrow { color: var(--faint); align-self: center; }
  .cap { font-family: var(--mono); font-size: 0.6rem; color: var(--faint); letter-spacing: 0.02em; padding-left: 0.2rem; }

  .sf.compact .g { font-size: 1.25rem; }
  .sf.compact { gap: 0.3rem 0.5rem; }
</style>
