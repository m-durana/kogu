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

/** Choose the headword form by echoing what the user typed: if the query equals one of the surface
 * forms, lead with that (search 机场 → 机场 leads, 機場 → 機場 leads); otherwise the canonical form.
 * The differing alternate is always shown bracketed. No script toggle needed. */
export function primaryForm(forms: Form[], variety: Variety, query = ''): DisplayForms | null {
  if (!forms || forms.length === 0) return null
  const q = query.trim()
  const matched = q ? forms.find((f) => f.form === q) : undefined
  if (variety === 'zh') {
    const primary = matched ?? forms.find((f) => f.is_primary) ?? forms[0]
    const alt = forms.find((f) => f.form !== primary.form) ?? null
    return { primary, alternate: alt }
  }
  const primary = matched ?? forms.find((f) => f.is_primary) ?? forms.find((f) => f.script !== 'kana') ?? forms[0]
  return { primary, alternate: null }
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

export type FuriToken = { t: 'text'; v: string } | { t: 'ruby'; base: string; rt: string }

/** Turn inline readings into real furigana tokens: 甘(あま)し -> ruby[甘|あま] + "し".
 * A (reading) in parens right after a Han run becomes ruby on that run (kana or romaji); the rest
 * stays plain text. Rendered with <ruby>/<rt> so the reading sits ON the character. */
export function furiganaTokens(text: string): FuriToken[] {
  const out: FuriToken[] = []
  const han = '[\u3400-\u9FFF\uF900-\uFAFF\u3005\u3006]+'
  const reading = "[\u3040-\u30FF \u30FC\u30FBA-Za-z\u0100-\u017F\u00E0-\u00FC'\u0304\u0301\u0300\u030C-]+"
  const re = new RegExp('(' + han + ')\\((' + reading + ')\\)', 'g')
  let last = 0
  let m
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push({ t: 'text', v: text.slice(last, m.index) })
    out.push({ t: 'ruby', base: m[1], rt: m[2].trim() })
    last = m.index + m[0].length
  }
  if (last < text.length) out.push({ t: 'text', v: text.slice(last) })
  return out
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
