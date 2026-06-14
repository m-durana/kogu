// Pure display logic — fixes the original CJKV Dict's display bugs *by construction* (DESIGN.md §5.3):
//  - user-selectable primary script (never hard-code traditional)
//  - principled bracketing: show the alternate form IFF it differs (no inverted logic)
//  - region tags surfaced on forms
// Kept framework-free so it can be unit-tested directly.

import type { Form, Hit, PrefScript, Variety } from './types'

export interface DisplayForms {
  primary: Form
  /** the differing alternate form to show in brackets, or null when there's nothing to add */
  alternate: Form | null
}

/** Choose the headword form and its bracketed alternate, honouring the user's script preference. */
export function pickForms(forms: Form[], variety: Variety, pref: PrefScript): DisplayForms | null {
  if (!forms || forms.length === 0) return null

  if (variety === 'zh') {
    const byScript = (s: string) => forms.find((f) => f.script === s)
    const primary = byScript(pref) ?? forms.find((f) => f.is_primary) ?? forms[0]
    const opposite: PrefScript = pref === 'trad' ? 'simp' : 'trad'
    // prefer the opposite-script form as the alternate; fall back to any differing form
    const altCandidate =
      byScript(opposite) ?? forms.find((f) => f.form !== primary.form) ?? null
    const alternate = altCandidate && altCandidate.form !== primary.form ? altCandidate : null
    return { primary, alternate }
  }

  // ja / yue: the kana reading is shown separately, not as a bracketed form
  const primary = forms.find((f) => f.is_primary) ?? forms.find((f) => f.script !== 'kana') ?? forms[0]
  return { primary, alternate: null }
}

/** Short human label + css class for how a hit matched (Phase-2 will add 同字/同義/cognate/false-friend). */
export function matchLabel(matchType: string): { label: string; cls: string } {
  switch (matchType) {
    case 'exact':
      return { label: 'exact', cls: 'm-exact' }
    case 'variant':
      return { label: 'variant', cls: 'm-variant' }
    case 'reading':
      return { label: 'reading', cls: 'm-reading' }
    case 'english':
      return { label: 'gloss', cls: 'm-english' }
    default:
      return { label: matchType, cls: 'm-other' }
  }
}

export function varietyLabel(v: Variety): string {
  return v === 'zh' ? '中' : v === 'yue' ? '粵' : '日'
}

/** Region codes present across a hit's forms, in a stable order (core four). */
export function regionsOf(hit: Hit): string[] {
  const order = ['CN', 'TW', 'HK', 'JP']
  const present = new Set(hit.forms.map((f) => f.region).filter((r): r is string => !!r))
  return order.filter((r) => present.has(r))
}

/** First gloss, trimmed for the results list (never shows internal placeholders — backend strips them). */
export function shortGloss(glosses: string[], max = 90): string {
  const g = glosses[0] ?? ''
  return g.length > max ? g.slice(0, max - 1) + '…' : g
}

/** OCR selection -> text, always in document (line, char) order regardless of tap order. */
export function ocrSelectedText(
  lines: { chars: { ch: string }[] }[],
  selected: Set<string>,
): string {
  return lines
    .flatMap((l, li) => l.chars.filter((_, ci) => selected.has(`${li}-${ci}`)).map((c) => c.ch))
    .join('')
}
