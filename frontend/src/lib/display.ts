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

/** Describe a character's composition from its IDS, KEEPING the structural information cleanIds throws
 * away: which components it's built from, how many of each (so 森 = three 木, 淼 = three 水), and how
 * they're arranged (the top-level Ideographic Description Character). This is the "background on the
 * character" — what radicals/parts make it up — not just a flat component list. */
export type IdsPart = { component: string; count: number }
export interface IdsInfo {
  parts: IdsPart[]
  arrangement: string | null
  /** set when the character is one component repeated (森 → {木, 3}); the headline insight */
  repeated: IdsPart | null
}
// Ideographic Description Characters → a plain-English arrangement of the TOP-LEVEL split.
const ARRANGEMENT: Record<string, string> = {
  '⿰': 'side by side',
  '⿱': 'stacked top to bottom',
  '⿲': 'three side by side',
  '⿳': 'three stacked',
  '⿴': 'one enclosing another',
  '⿵': 'enclosed from above',
  '⿶': 'enclosed from below',
  '⿷': 'enclosed from the left',
  '⿸': 'enclosed from the upper-left',
  '⿹': 'enclosed from the upper-right',
  '⿺': 'enclosed from the lower-left',
  '⿻': 'overlapping',
}
const IDC_RE = /[⿰-⿿]/
export function describeIds(ids: string | null, self = ''): IdsInfo | null {
  if (!ids) return null
  const clean = ids.replace(/\[[A-Z]+\]/g, '')
  const firstIdc = [...clean].find((c) => IDC_RE.test(c))
  const arrangement = firstIdc ? ARRANGEMENT[firstIdc] ?? null : null
  // leaf components = everything that isn't an IDC operator or whitespace
  const leaves = [...clean].filter((c) => c.trim() && !IDC_RE.test(c))
  // atomic / undecomposable (ids is just the character itself or empty) → nothing to explain
  if (!leaves.length || (leaves.length === 1 && leaves[0] === self)) return null
  const counts = new Map<string, number>()
  for (const c of leaves) counts.set(c, (counts.get(c) ?? 0) + 1)
  const parts = [...counts.entries()].map(([component, count]) => ({ component, count }))
  const repeated = parts.length === 1 && parts[0].count >= 2 ? parts[0] : null
  return { parts, arrangement, repeated }
}

const NUM_WORD = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine']
export function numWord(n: number): string {
  return NUM_WORD[n] ?? String(n)
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
  s = s.replace(/\(\s*(?:meaningless\s+)?bound form\s*\)\s*/gi, '') // grammatical jargon → surfaced as a "bound" tag instead
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
    /^(surname\b|old variant of|variant of|used in|see\b|abbr\b)/.test(s) ||
    s.includes('radical in chinese characters') ||
    s.includes('kangxi radical')
  )
}

/** Count glosses that actually convey meaning (for picking the best lexeme per language). */
export function meaningfulGlossCount(glosses: string[]): number {
  return glosses.filter((g) => !isMinorGloss(g)).length
}

/** Split a cross-reference gloss so its target glyph is tappable: "variant of 著" → the 著 becomes a
 * link that pivots to the real entry, so a dead-end glyph still gets you to the meaning. Matches the
 * leading "variant of / used in / see / see also" cue followed by a Han run; everything else is one
 * plain text part. (CJK range, not \p{Han}: the build-time regex parser rejects script-name escapes.) */
/** CC-CEDICT marks morphemes that never stand alone as words with "(bound form)" (occasionally
 * "(meaningless bound form)"). True if any of a row's raw glosses carries that marker — the prose has
 * it stripped (see cleanGloss), so detect it here to show a small tappable "bound" tag instead. */
const BOUND_FORM_RE = /\(\s*(?:meaningless\s+)?bound form\s*\)/i
export function isBoundForm(glosses: string[]): boolean {
  return glosses.some((g) => BOUND_FORM_RE.test(g))
}

export type GlossPart = { v: string; link?: boolean }
// Han run: CJK Unified (incl. ext-A) + compat ideographs + iteration mark 々. Explicit ranges, not
// \p{Han}: the build-time regex parser rejects Unicode script-name escapes.
const HAN_RUN = /[㐀-鿿豈-﫿々]+/g
/** Split a string so every Han run becomes a tappable link and the rest stays plain text — used in
 * glosses ("variant of 著" → 著 links; "ear; handle 耳" → 耳 links) and origin prose. */
export function linkifyHan(s: string): GlossPart[] {
  const out: GlossPart[] = []
  let last = 0
  let m: RegExpExecArray | null
  HAN_RUN.lastIndex = 0
  while ((m = HAN_RUN.exec(s)) !== null) {
    if (m.index > last) out.push({ v: s.slice(last, m.index) })
    out.push({ v: m[0], link: true })
    last = m.index + m[0].length
  }
  if (last < s.length) out.push({ v: s.slice(last) })
  return out.length ? out : [{ v: s }]
}
export const glossParts = linkifyHan

// === Origin (etymology) rendering ===
// Etymology arrives as one Wiktionary string that often MERGES several statements (newline-separated)
// and sometimes numbered "Etymology 1/2" sections, peppered with academic jargon (形聲, OC, STEDT,
// Proto-Sino-Tibetan…). We (1) split it into clearly-delineated segments, and within each segment
// (2) keep phonological reconstructions faint, (3) attach plain-English tooltips to the jargon, and
// (4) make every Han run tappable — composed as ordered passes so they don't fight each other.

// Plain-English glossary for the jargon. Longest keys first so phrases beat their abbreviations and
// CJK terms match before single chars. `word: true` adds \b boundaries (so "OC" ≠ inside "OCt").
type GlossEntry = { term: string; title: string; word?: boolean }
const ETY_GLOSSARY: GlossEntry[] = [
  { term: 'Phono-semantic compound', title: 'A character that pairs a meaning part with a sound part.' },
  { term: 'Phono-semantic matching', title: 'A foreign word borrowed with characters picked to fit both its sound and meaning.' },
  { term: 'Ideogrammic compound', title: 'A character whose parts together picture its meaning.' },
  { term: 'Simple ideogram', title: 'A character that points at an abstract idea directly.' },
  { term: 'Pictogram', title: 'A character that began as a drawing of the thing it names.' },
  { term: 'Ideogram', title: 'A character representing an idea directly, not a picture of an object.' },
  { term: 'Initialism', title: 'A word formed from the initial letters of other words.' },
  { term: 'Old Chinese', title: 'The reconstructed pronunciation of Chinese around 1000 BCE.' },
  { term: 'Middle Chinese', title: 'The pronunciation of Chinese around 600 CE.' },
  { term: 'Old Japanese', title: 'The earliest written Japanese, 8th century.' },
  { term: 'Proto-Sino-Tibetan', title: 'The reconstructed common ancestor of Chinese, Tibetan, Burmese and related languages.' },
  { term: 'Proto-Indo-European', title: 'The reconstructed ancestor of most European and South-Asian languages.' },
  { term: 'STEDT', title: 'Sino-Tibetan Etymological Dictionary and Thesaurus — a comparative reconstruction project.', word: true },
  { term: '形聲', title: 'Phono-semantic compound: a meaning part plus a sound part.' },
  { term: '形声', title: 'Phono-semantic compound: a meaning part plus a sound part.' },
  { term: '會意', title: 'Ideogrammic compound: parts that combine to suggest the meaning.' },
  { term: '会意', title: 'Ideogrammic compound: parts that combine to suggest the meaning.' },
  { term: '象形', title: 'Pictogram: originally a picture of the thing it names.' },
  { term: '指事', title: 'Simple ideogram: points at an abstract idea.' },
  { term: '假借', title: 'Phonetic loan: a character borrowed for its sound.' },
  { term: 'calque', title: 'A word translated piece by piece from another language.', word: true },
  { term: 'cognate', title: 'A word sharing a common ancestor with another.', word: true },
  { term: 'OC', title: 'Old Chinese (~1000 BCE).', word: true },
  { term: 'MC', title: 'Middle Chinese (~600 CE).', word: true },
  { term: 'OJ', title: 'Old Japanese (8th century).', word: true },
  { term: 'PST', title: 'Proto-Sino-Tibetan.', word: true },
  { term: 'PIE', title: 'Proto-Indo-European.', word: true },
]
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const ETY_GLOSS_RE = new RegExp(
  ETY_GLOSSARY.map((e) => (e.word ? `\\b${esc(e.term)}\\b` : esc(e.term))).join('|'),
  'g',
)
const ETY_GLOSS_TITLE = new Map(ETY_GLOSSARY.map((e) => [e.term, e.title]))

export type EtyInline =
  | { t: 'ruby'; base: string; rt: string }
  | { t: 'recon'; v: string; title?: string }
  | { t: 'abbr'; v: string; title: string }
  | { t: 'han'; v: string }
  | { t: 'text'; v: string }
export interface EtySegment {
  /** numbered-section label ("Etymology 2") when the source delineates several, else null */
  heading: string | null
  tokens: EtyInline[]
}

// run a text-splitter over only the 'text' tokens of a list, leaving other token kinds intact
function expandText(toks: EtyInline[], split: (s: string) => EtyInline[]): EtyInline[] {
  return toks.flatMap((t) => (t.t === 'text' ? split(t.v) : [t]))
}
// generic regex tokenizer: matched spans → make(), gaps → text
function tokenizeBy(s: string, re: RegExp, make: (m: string) => EtyInline): EtyInline[] {
  const out: EtyInline[] = []
  let last = 0
  let m: RegExpExecArray | null
  re.lastIndex = 0
  while ((m = re.exec(s)) !== null) {
    if (m.index > last) out.push({ t: 'text', v: s.slice(last, m.index) })
    out.push(make(m[0]))
    last = m.index + m[0].length
    if (m[0].length === 0) re.lastIndex++ // guard against zero-width matches
  }
  if (last < s.length) out.push({ t: 'text', v: s.slice(last) })
  return out
}
const RECON_RE = /(\([^)]*\*[^)]*\)|\/\*[^/]{0,40}\/|\((?:OC|MC|OJ|PIE|PST|STEDT|Old Chinese|Middle Chinese)[^)]*\))/g
const REGISTER_RE = /\b(OC|MC|OJ|PIE|PST|Old Chinese|Middle Chinese)\b/
function inlineEty(s: string): EtyInline[] {
  // 1. furigana ruby (Han + (kana/romaji reading)) — must win over plain Han-linkify
  let toks: EtyInline[] = furiganaTokens(s).map((t) =>
    t.t === 'ruby' ? { t: 'ruby', base: t.base, rt: t.rt } : { t: 'text', v: t.v },
  )
  // 2. reconstructions → faint; tag with a tooltip when they carry a register marker (OC/MC…)
  toks = expandText(toks, (x) =>
    tokenizeBy(x, RECON_RE, (v) => {
      const reg = v.match(REGISTER_RE)
      const title = reg ? ETY_GLOSS_TITLE.get(reg[1]) : undefined
      return { t: 'recon', v, title }
    }),
  )
  // 3. jargon → plain-English tooltips
  toks = expandText(toks, (x) =>
    tokenizeBy(x, ETY_GLOSS_RE, (v) => ({ t: 'abbr', v, title: ETY_GLOSS_TITLE.get(v) ?? '' })),
  )
  // 4. remaining Han runs → tappable
  toks = expandText(toks, (x) =>
    tokenizeBy(x, HAN_RUN, (v) => ({ t: 'han', v })),
  )
  return toks
}

/** Split merged etymology prose into delineated segments (one per newline-separated statement),
 * lifting "Etymology N" markers to segment headings and stripping Wiktionary "; " list leaks. */
export function etymologyTokens(text: string): EtySegment[] {
  const segs: EtySegment[] = []
  let heading: string | null = null
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (!line) continue
    const hm = line.match(/^;?\s*(Etymology\s+\d+)\s*$/i)
    if (hm) {
      heading = hm[1]
      continue
    }
    const body = line.replace(/^;\s*/, '').trim() // drop a leading "; " definition-list marker
    if (!body) continue
    segs.push({ heading, tokens: inlineEty(body) })
    heading = null // a heading labels only its first following statement
  }
  return segs
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
