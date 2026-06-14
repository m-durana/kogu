<script lang="ts">
  import type { ConceptGroup } from './types'
  import { varietyLabel } from './display'

  let { groups, onopen }: { groups: ConceptGroup[]; onopen: (id: number) => void } = $props()
</script>

<section class="concepts">
  {#each groups as g}
    <div class="group">
      <div class="concept">{g.concept}</div>
      <div class="members">
        {#each g.members as m}
          <button class="member" onclick={() => onopen(m.lexeme_id)}>
            <span class="var v-{m.variety}">{varietyLabel(m.variety)}</span>
            <span class="mhead">{m.headword}</span>
            {#if m.reading}<span class="mread">{m.reading}</span>{/if}
          </button>
        {/each}
      </div>
    </div>
  {/each}
</section>

<style>
  .concepts { display: flex; flex-direction: column; gap: 1rem; margin-bottom: 1.2rem; }
  .group { display: flex; flex-direction: column; gap: 0.5rem; }
  .concept {
    font-family: var(--serif); font-size: 1.3rem; color: var(--text);
    border-bottom: 1px solid var(--border); padding-bottom: 0.3rem;
  }
  .members { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  .member {
    display: inline-flex; align-items: center; gap: 0.4rem;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r);
    padding: 0.4rem 0.7rem;
  }
  .member:hover { border-color: var(--border-strong); background: var(--surface-2); }
  .var { font-family: var(--han); font-size: 0.72rem; color: var(--faint); border: 1px solid var(--border); border-radius: 4px; padding: 0 0.2rem; }
  .v-zh, .v-ja, .v-yue { color: var(--muted); }
  .mhead { font-family: var(--han); font-size: 1.35rem; }
  .mread { font-family: var(--mono); color: var(--muted); font-size: 0.75rem; }
</style>
