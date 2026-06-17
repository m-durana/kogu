// Pure display logic - fixes the original CJKV Dict's display bugs *by construction* (DESIGN.md §5.3):
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
  // ja / yue: show the language's OWN canonical form, never the cross-script form the user typed.
  // Japanese writes 気 even when you searched the Chinese 氣 (which only appears in the Japanese form
  // list as a kyūjitai variant). The top headword still echoes what you typed; the rows localise.
  const primary = forms.find((f) => f.is_primary) ?? forms.find((f) => f.script !== 'kana') ?? forms[0]
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

/** First gloss, trimmed for the results list (never shows internal placeholders - backend strips them). */
export function shortGloss(glosses: string[], max = 90): string {
  const g = cleanGloss(glosses[0] ?? '')
  return g.length > max ? g.slice(0, max - 1) + '…' : g
}

// numbered-pinyin -> tone-marked, so the language rows match the tone-marked character cards
// (e.g. "shou3 zhi3" -> "shǒu zhǐ"). Non-pinyin input passes through unchanged.
const TONE_MARKS: Record<string, string[]> = {
  a: ['a', 'ā', 'á', 'ǎ', 'à'],
  e: ['e', 'ē', 'é', 'ě', 'è'],
  i: ['i', 'ī', 'í', 'ǐ', 'ì'],
  o: ['o', 'ō', 'ó', 'ǒ', 'ò'],
  u: ['u', 'ū', 'ú', 'ǔ', 'ù'],
  ü: ['ü', 'ǖ', 'ǘ', 'ǚ', 'ǜ'],
}
function markSyllable(syl: string): string {
  const m = syl.match(/^([a-zü]+?)([1-5])$/i)
  if (!m) return syl
  let base = m[1].toLowerCase().replace(/u:|v/g, 'ü')
  const tone = +m[2]
  if (tone === 5 || tone === 0) return base // neutral tone, no mark
  // tone placement: a or e first; else the o in "ou"; else the last vowel
  let target = ''
  if (base.includes('a')) target = 'a'
  else if (base.includes('e')) target = 'e'
  else if (base.includes('ou')) target = 'o'
  else {
    const vowels = base.match(/[aeiouü]/g)
    target = vowels ? vowels[vowels.length - 1] : ''
  }
  if (!target) return base
  const i = base.lastIndexOf(target)
  return base.slice(0, i) + TONE_MARKS[target][tone] + base.slice(i + 1)
}
export function pinyinMarks(reading: string): string {
  if (!reading) return reading
  return reading.split(/\s+/).map(markSyllable).join(' ')
}

/** Split etymology prose so the academic phonological reconstructions can be de-emphasised:
 * parentheticals like "(OC *n̥ˁar)" / "(*ʔɨts)" and slashed forms like "/*ʔɨts/". The narrative
 * stays prominent; the reconstructions render small + faint. */
import type { FormBranch } from './types'

// Short CJK tag(s) for a branch's script. The script may be "+"-joined (学 is both 简 and 日).
// recognized script abbreviations (TC = Traditional Chinese, SC = Simplified Chinese), English until
// localization. JP = Japanese shinjitai, var = z-variant.
const SCRIPT_TAG: Record<string, string> = {
  traditional: 'TC',
  simplified: 'SC',
  shinjitai: 'JP',
  'z-variant': 'var',
}
export function scriptShort(script: string): string {
  return script
    .split('+')
    .map((s) => SCRIPT_TAG[s] ?? 'var')
    .filter((v, i, a) => a.indexOf(v) === i)
    .join(' ')
}

// Tag for a surface_form's script (trad/simp/shinjitai) — used to label both Chinese forms equally.
export function formTag(script: string): string {
  return ({ trad: 'TC', simp: 'SC', shinjitai: 'JP' } as Record<string, string>)[script] ?? ''
}

// Stable display order for the forms strip: traditional → simplified → shinjitai → z-variant.
const SCRIPT_RANK: Record<string, number> = { traditional: 0, simplified: 1, shinjitai: 2, 'z-variant': 3 }
export function orderBranches(branches: FormBranch[]): FormBranch[] {
  return [...branches].sort(
    (a, b) => (SCRIPT_RANK[a.script.split('+')[0]] ?? 9) - (SCRIPT_RANK[b.script.split('+')[0]] ?? 9),
  )
}

export type EtyToken = { t: 'text' | 'recon'; v: string }
export function splitRecon(s: string): EtyToken[] {
  // a parenthetical containing a "*" reconstruction, a tight /…*…/ form, or an (OC …)/(MC …) note.
  // The slashed form must start "/*" so it doesn't swallow trad/simp "X /Y" slash notation.
  const re = /(\([^)]*\*[^)]*\)|\/\*[^/]{0,40}\/|\((?:OC|MC|OJ|PIE|PST|STEDT|Old Chinese|Middle Chinese)[^)]*\))/g
  const out: EtyToken[] = []
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(s)) !== null) {
    if (m.index > last) out.push({ t: 'text', v: s.slice(last, m.index) })
    out.push({ t: 'recon', v: m[0] })
    last = m.index + m[0].length
  }
  if (last < s.length) out.push({ t: 'text', v: s.slice(last) })
  return out
}

/** cjkvi-ids -> clean component list. Strips source tags ("[GTV]") and Ideographic Description
 * Characters (⿰⿱… U+2FF0–2FFF, which many fonts render as tofu) so only the components show:
 * "⿰糸氏[GTV]" -> "糸 氏". (DESIGN.md §6: no placeholder/markup leaks.) */
export function cleanIds(ids: string | null): string {
  if (!ids) return ''
  const s = ids.replace(/\[[A-Z]+\]/g, '').replace(/[⿰-⿿]/g, '')
  return [...s].filter((c) => c.trim()).join(' ')
}

/** Sanitise a CC-CEDICT/JMdict gloss for display: strip classifier clauses, bracketed romanisation,
 * trad|simp pipe pairs, "Taiwan pr." notes, and tidy dangling separators. The raw glosses leak
 * dictionary markup (e.g. "telephone; CL:通[tong1]") that reads as machine junk to users. */
export function cleanGloss(g: string): string {
  if (!g) return ''
  let s = g
  // CC-Canto cross-reference notes: the variety is shown by the 粵語 row label, and the standard
  // form by the "written differently" bridge - so drop them from the definition prose itself.
  s = s.replace(/[;,]?\s*\(?\s*Mandarin equivalent\s*:[^)]*\)?/gi, '') // "(Mandarin equivalent: 沒有…)"
  s = s.replace(/\s*\((?:Cantonese|Mandarin)\)/gi, '') // bare variety tags
  s = s.replace(/\s*\bCL:[^;]*(?=;|$)/g, '') // classifier clauses
  s = s.replace(/\[[A-Za-zÀ-ÿüÜ0-9·,.\s]*\]/g, '') // [hang2 kong1 gang3], [fa3] - before pipes
  s = s.replace(/([^\s;,，|[\]]+)\|([^\s;,，|[\]]+)/g, '$1') // 處|处 -> 處
  s = s.replace(/[,;]?\s*(?:Taiwan|Mainland|also|old|erhua|Cantonese)\s+pr\.\s*/gi, ' ') // pr. notes
  s = s.replace(/\(\s*\)/g, '') // empty parens left behind
  s = s.replace(/\s*;\s*/g, '; ') // normalise sense separators
  s = s.replace(/(?:;\s*)+/g, '; ')
  s = s.replace(/\s{2,}/g, ' ')
  s = s.replace(/\s+([;,.)])/g, '$1')
  s = s.replace(/^[\s;,]+|[\s;,]+$/g, '')
  return s.trim()
}

/** Clean + join a list of glosses for a single line. */
export function glossLine(glosses: string[], max = 4): string {
  return glosses.map(cleanGloss).filter(Boolean).slice(0, max).join('; ')
}

/** One concise gloss line for a comparison row - the leading sense(s), capped so a row stays
 * scannable instead of dumping every sense. Cuts on a clause boundary when it can. */
export function briefGloss(glosses: string[], max = 64): string {
  const g = glossLine(glosses, 2)
  if (g.length <= max) return g
  const cut = g.slice(0, max)
  const sep = Math.max(cut.lastIndexOf('; '), cut.lastIndexOf(', '))
  return (sep > max / 3 ? cut.slice(0, sep) : cut.replace(/\s+\S*$/, '')) + '…'
}

/** A "minor" gloss carries no real meaning for a cross-language comparison - a bare surname,
 * a "variant of"/"used in"/"see" cross-reference, or a radical definition. */
export function isMinorGloss(g: string): boolean {
  const s = cleanGloss(g).toLowerCase()
  if (!s) return true
  return (
    /^(surname\b|old variant of|variant of|used in|see\b|abbr\b|\(bound form\))/.test(s) ||
    s.includes('radical in chinese characters') ||
    s.includes('kangxi radical')
  )
}

/** Count glosses that actually convey meaning (for picking the best lexeme per language). */
export function meaningfulGlossCount(glosses: string[]): number {
  return glosses.filter((g) => !isMinorGloss(g)).length
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
