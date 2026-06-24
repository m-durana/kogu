// Pure display logic - fixes the original CJKV Dict's display bugs *by construction* (DESIGN.md §5.3):
//  - user-selectable primary script (never hard-code traditional)
//  - principled bracketing: show the alternate form IFF it differs (no inverted logic)
//  - region tags surfaced on forms
// Kept framework-free so it can be unit-tested directly.

import type { Form, Hit, PrefScript, Variety, VariantEdge } from './types'

export interface DisplayForms {
  primary: Form
  /** the differing alternate form to show in brackets, or null when there's nothing to add */
  alternate: Form | null
}

/** Choose the headword form by echoing what the user typed: if the query equals one of the surface
 * forms, lead with that (search 机场 → 机场 leads, 機場 → 機場 leads). When the query gives no script
 * signal (an English / reading lookup, a saved item), fall back to the SIMPLIFIED form as the main one
 * (it's what most readers expect, simply because there are more Simplified readers). The differing
 * alternate is always shown small beside it. No script toggle needed. */
export function primaryForm(forms: Form[], variety: Variety, query = ''): DisplayForms | null {
  if (!forms || forms.length === 0) return null
  const q = query.trim()
  const matched = q ? forms.find((f) => f.form === q) : undefined
  if (variety === 'zh') {
    // searched form wins; else Simplified; else the primary (trad); else whatever exists.
    const primary =
      matched ?? forms.find((f) => f.script === 'simp') ?? forms.find((f) => f.is_primary) ?? forms[0]
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

/** Full language name for a variety — used for section/divider headings (e.g. the language-sorted
 * Related / Used-in lists, where each 中/粵/日 group gets a labelled divider). */
export function varietyName(v: Variety): string {
  return v === 'zh' ? 'Mandarin' : v === 'yue' ? 'Cantonese' : 'Japanese'
}

/** Headword glyph font-size for an entry: a 1–2 char word stays huge, but a long word (an idiom, or a
 * kana+kanji verb like あずかり知る) shrinks so the header stays compact and never grows tall enough to
 * collide with the save/share buttons overlapping the top-right of the card. `len` is the headword's
 * Unicode code-point count ([...head].length). */
export function headwordGlyphSize(len: number): string {
  if (len <= 2) return 'clamp(2.8rem, 14vw, 3.8rem)'
  if (len <= 4) return 'clamp(2.2rem, 10vw, 2.9rem)'
  if (len <= 7) return 'clamp(1.7rem, 7vw, 2.1rem)'
  return 'clamp(1.35rem, 5.5vw, 1.7rem)'
}

/** BCP-47 lang tag for a variety, stamped on glyph elements so screen readers / text selection know
 * the language, and so the matching regional Han font is chosen. */
export function langTag(v: Variety): string {
  return v === 'ja' ? 'ja' : v === 'yue' ? 'zh-Hant' : 'zh-Hans'
}

/** Region-correct Han serif for a variety — applied inline so it beats component-scoped styles. U+8AA4
 * 誤 (and many Han-unified chars) render with different shapes per region; a single Simplified cut drew
 * the Chinese 誤 even for a Japanese word, so Japanese/Cantonese get their own cut. */
export function hanFont(v: Variety): string {
  return v === 'ja' ? 'var(--han-ja)' : v === 'yue' ? 'var(--han-tc)' : 'var(--han)'
}

/** A region-EXCLUSIVE word — one used only in a particular region, written with its own characters
 * (taxi: Taiwan 計程車 vs mainland 出租車). CC-CEDICT marks these per sense with "(Tw)" / "(HK)". A word
 * earns a region badge only when its PRIMARY (first non-minor) sense carries the marker, so a general
 * word that merely has one regional sense is NOT tagged: 計程車 "(Tw) taxi" → ['Taiwan']; 出租車 "taxi" +
 * "(Tw) rental car" → [] (its main meaning is general). Mainland is the unmarked default, so no badge.
 * Runs on RAW glosses (before cleanGloss strips the marker). */
export function regionTags(glosses: string[]): string[] {
  const meaningful = glosses.filter((g) => g && g.trim())
  const primary = meaningful.find((g) => !isMinorGloss(g)) ?? meaningful[0] ?? ''
  if (/\(\s*Tw\s*\)/i.test(primary)) return ['Taiwan']
  if (/\(\s*HK\s*\)/i.test(primary)) return ['Hong Kong']
  return []
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
import type { FormBranch, ScriptForms } from './types'

// Short CJK tag(s) for a branch's script. The script may be "+"-joined (学 is both 简 and 日).
// Jyutping → Yale romanisation (deterministic). Yale marks tone with a vowel diacritic plus an 'h'
// after the vowel for the low tones (4/5/6): nei5→néih, sik6→sihk, jyu4→yùh, gwong2→gwóng.
const J_TONE_MARK: Record<string, string> = {
  '1': '̄', // macron (high)
  '2': '́', // acute (mid-rising)
  '3': '',
  '4': '̀', // grave + h (low-falling)
  '5': '́', // acute + h (low-rising)
  '6': '', // + h (low)
}
const J_LOW = new Set(['4', '5', '6'])
function jyutpingSyllableToYale(syl: string): string {
  const m = syl.match(/^([a-zA-Z]+)([1-6])$/)
  if (!m) return syl
  let body = m[1].toLowerCase()
  const tone = m[2]
  // initials: an INITIAL j (semivowel) merges into a following yu (jyu→yu); else j→y, z→j, c→ch
  if (body.startsWith('jyu')) body = body.slice(1)
  else if (body[0] === 'j') body = 'y' + body.slice(1)
  else if (body[0] === 'z') body = 'j' + body.slice(1)
  else if (body[0] === 'c') body = 'ch' + body.slice(1)
  // finals: jyutping oe/eo → Yale eu (goek→geuk, deoi→deui)
  body = body.replace(/oe/g, 'eu').replace(/eo/g, 'eu')
  const vowels = 'aeiou'
  const firstV = [...body].findIndex((c) => vowels.includes(c))
  if (J_LOW.has(tone)) {
    let lastV = -1
    for (let i = 0; i < body.length; i++) if (vowels.includes(body[i])) lastV = i
    body = lastV >= 0 ? body.slice(0, lastV + 1) + 'h' + body.slice(lastV + 1) : body + 'h'
  }
  const mark = J_TONE_MARK[tone]
  if (mark) {
    const at = firstV >= 0 ? firstV : 0
    body = (body.slice(0, at + 1) + mark + body.slice(at + 1)).normalize('NFC')
  }
  return body
}
export function jyutpingToYale(reading: string): string {
  return reading.split(/\s+/).map(jyutpingSyllableToYale).join(' ')
}

// A reading shown in the user's chosen romanisation, for ANY list row or entry: pinyin tone-marks for
// 中, jyutping or Yale for 粵 (per the setting), kana left as-is for 日. The single source so result/
// saved/history rows and the entry's Related/Used-in/Characters rows all match the headword.
export function formatReading(variety: string, reading: string | null | undefined, yale: boolean): string {
  if (!reading) return ''
  if (variety === 'zh') return pinyinMarks(reading)
  if (variety === 'yue') return yale ? jyutpingToYale(reading) : reading
  return reading
}

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

// GlyphWiki vector URL for a single codepoint (item 148): GlyphWiki names a glyph "u" + lowercase
// hex. Returns null for empty / multi-codepoint input. Used as the tofu fallback for rare ideographs.
export function glyphWikiUrl(ch: string): string | null {
  if (!ch || [...ch].length !== 1) return null
  const cp = ch.codePointAt(0)
  if (!cp) return null
  return `https://glyphwiki.org/glyph/u${cp.toString(16)}.svg`
}

// The traditional/simplified counterpart to jump to from the header glyph (item 161): given the
// character's script family and the viewed form, return the OTHER Chinese script's form, or null when
// there is no genuine TC/SC pair (identical forms, kokuji, shinjitai-only, z-variants).
export function scSwitchTarget(sf: ScriptForms | null, head: string): { to: string; label: string } | null {
  if (!sf || sf.is_kokuji) return null
  const trad = sf.branches.find((b) => b.script.split('+').includes('traditional'))?.form ?? sf.orthodox
  const simp = sf.branches.find((b) => b.script.split('+').includes('simplified'))?.form
  if (!simp || !trad || trad === simp) return null
  if (head === simp) return { to: trad, label: 'traditional' }
  if (head === trad) return { to: simp, label: 'simplified' }
  return null
}

// Tag for a surface_form's script (trad/simp/shinjitai) — used to label both Chinese forms equally.
export function formTag(script: string): string {
  return ({ trad: 'TC', simp: 'SC', shinjitai: 'JP' } as Record<string, string>)[script] ?? ''
}

// Rotating search-field placeholders (item 1): a different example of what you can type, every 2s.
export const SEARCH_PLACEHOLDERS = [
  'a character',
  'a reading',
  'an English word',
  'pinyin',
  'jyutping',
  'kana',
  'a kanji',
]
export function placeholderAt(index: number): string {
  const n = SEARCH_PLACEHOLDERS.length
  return SEARCH_PLACEHOLDERS[((index % n) + n) % n] // safe for negative / overflowing indices
}

// Plain-language name for the reform behind a script change (mirrors the backend reform_label).
const REFORM_LABEL: Record<string, string> = {
  opencc: 'PRC simplification',
  'prc-1956': 'PRC simplification',
  'prc-1964': 'PRC simplification',
  'jp-toyo': 'Tōyō shinjitai reform',
  'jp-joyo': 'Jōyō kanji reform',
  'hk-std': 'Hong Kong standard',
  'tw-std': 'Taiwan standard',
}
export function reformLabel(id: string | null | undefined): string | null {
  return id ? REFORM_LABEL[id] ?? null : null
}

const EDGE_KIND: Record<string, string> = {
  shinjitai: 'Japanese shinjitai',
  simplification: 'simplified Chinese form',
  'z-variant': 'variant form',
}
// A full-sentence explanation of a script change for the structure section (item 14): the two forms
// carry the same meaning, and the reason for the divergence (reform + year). A glyph can be BOTH a
// Japanese shinjitai AND a PRC simplification of the same parent (萬 → 万) — say it's both, so it never
// reads as if Chinese took the form from Japanese. Returns null when there's no orthodox parent.
export function scriptChangeNote(head: string, variants: VariantEdge[]): string | null {
  if (!variants.length) return null
  const parent = variants[0].parent
  const same = variants.filter((v) => v.parent === parent)
  const kinds = [...new Set(same.map((v) => EDGE_KIND[v.edge_type] ?? 'variant form'))]
  const reforms = [
    ...new Set(
      same
        .map((v) => {
          const l = reformLabel(v.reform)
          return l ? `${l}${v.reform_year ? ` (${v.reform_year})` : ''}` : null
        })
        .filter((x): x is string => !!x),
    ),
  ]
  const which = kinds.length > 1 ? `both the ${kinds.join(' and the ')}` : `the ${kinds[0]}`
  const reason = reforms.length ? `, from the ${reforms.join(' and the ')}` : ''
  return `${parent} and ${head} carry the same meaning; ${head} is ${which} of ${parent}${reason}.`
}

const BRANCH_KIND: Record<string, string> = {
  simplified: 'simplified Chinese form',
  shinjitai: 'Japanese shinjitai',
  'z-variant': 'variant form',
}
// Same explanation as scriptChangeNote, but built from the forms strip so it also appears when the
// ORTHODOX glyph is the one on screen (searching 汉 resolves to the traditional 漢 lexeme, whose own
// variant-edges are empty; its simplified/shinjitai children live on the strip instead).
export function scriptChangeFromForms(sf: ScriptForms | null): string | null {
  if (!sf || sf.is_kokuji) return null
  const others = sf.branches.filter((b: FormBranch) => !b.is_orthodox)
  if (!others.length) return null
  const clauses = others.map((b) => {
    // a glyph can be BOTH a shinjitai AND a PRC simplification of the same parent (萬 → 万). Say it's
    // both, so it never reads as if one script borrowed the form from the other.
    const kinds = b.script.split('+').map((s) => BRANCH_KIND[s] ?? 'variant form')
    const which = kinds.length > 1 ? `both the ${kinds.join(' and the ')}` : `the ${kinds[0]}`
    return `${b.form} is ${which}${b.reform_label ? ` (${b.reform_label})` : ''}`
  })
  return `${sf.orthodox} and ${others.length > 1 ? 'its variants' : others[0].form} carry the same meaning; ${clauses.join('; ')}.`
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
  /** the top-level Ideographic Description Character itself (⿰⿱⿴…), so the UI can draw the layout
   * as a small box diagram instead of spelling it out in prose. null when there's no IDC operator. */
  idc: string | null
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
// a real component leaf is a Han glyph or a CJK radical; cjkvi-ids also uses placeholder symbols
// (circled numbers ①-⑩ U+2460-24FF, etc.) for components it can't encode (華 = ⿱艹⑦). Those must
// not render as "⑦".
const COMPONENT_LEAF = /\p{Script=Han}|[⺀-⻿⼀-⿟々〇]/u
export function describeIds(ids: string | null, self = ''): IdsInfo | null {
  if (!ids) return null
  const clean = ids.replace(/\[[A-Z]+\]/g, '')
  const firstIdc = [...clean].find((c) => IDC_RE.test(c))
  const arrangement = firstIdc ? ARRANGEMENT[firstIdc] ?? null : null
  // leaf components = everything that isn't an IDC operator or whitespace
  const rawLeaves = [...clean].filter((c) => c.trim() && !IDC_RE.test(c))
  const leaves = rawLeaves.filter((c) => COMPONENT_LEAF.test(c))
  // a placeholder leaf was dropped → the decomposition is incomplete; don't show a misleading partial
  if (leaves.length < rawLeaves.length) return null
  // atomic / undecomposable (ids is just the character itself or empty) → nothing to explain
  if (!leaves.length || (leaves.length === 1 && leaves[0] === self)) return null
  const counts = new Map<string, number>()
  for (const c of leaves) counts.set(c, (counts.get(c) ?? 0) + 1)
  const parts = [...counts.entries()].map(([component, count]) => ({ component, count }))
  const repeated = parts.length === 1 && parts[0].count >= 2 ? parts[0] : null
  return { parts, arrangement, idc: firstIdc ?? null, repeated }
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
  s = s.replace(/\(\s*(?:Tw|HK)\s*\)\s*/g, '') // region markers (Tw)/(HK) → surfaced as a small badge
  s = s.replace(/\(\s*(?:meaningless\s+)?bound form\s*\)\s*/gi, '') // grammatical jargon → surfaced as a "bound" tag instead
  s = s.replace(/\s*\(?\s*\bCL:[^;)]*\)?/g, '') // classifier clauses, incl. when wrapped in (…): "fish (CL:條,尾)" → "fish"
  s = s.replace(/\[[A-Za-zÀ-ÿüÜ0-9·,.\s]*\]/g, '') // [hang2 kong1 gang3], [fa3] - before pipes
  s = s.replace(/([^\s;,，|[\]]+)\|([^\s;,，|[\]]+)/g, '$1') // 處|处 -> 處
  s = s.replace(/[,;]?\s*(?:Taiwan|Mainland|also|old|erhua|Cantonese)\s+pr\.\s*/gi, ' ') // pr. notes
  // trailing borrowed-source note ("…(from Japanese 入 "iri")") — metadata, not meaning; drop it but
  // keep the actual sense before it (馬鹿 "idiot (from Japanese)" → "idiot").
  s = s.replace(/[,;]?\s*\(from (?:Japanese|English|French|German|Latin|Korean|Chinese|Sanskrit|Mongolian|Manchu)\b[^)]*\)\s*$/i, '')
  // metadata tags / radical-number boilerplate are not meanings: 働 "…; (kokuji)" → drop tag;
  // 氵 "water; radical number 85" → "water"; 彳 "…; rad. no 60" → drop; "going man radical (no. 60)".
  s = s.replace(/[;,]?\s*\(?\s*kokuji\s*\)?/gi, '')
  // paren-wrapped radical note ("(Kangxi radical 60)", "(radical 60)", "(no. 60)") — keyword-anchored
  // so it never eats an ordinary numeric parenthetical like "(5)".
  s = s.replace(/\s*[;,]?\s*\(\s*(?:kangxi\s+)?(?:radical|rad\.?|no\.?)\s*(?:number|no\.?)?\s*\d+\s*\)/gi, '')
  // bare radical-number boilerplate ("radical number 85", "Kangxi radical 144", "rad. no 60")
  s = s.replace(/[;,]?\s*(?:kangxi\s+)?(?:radical|rad\.?)\s*(?:number|no\.?)?\s*\d+/gi, '')
  s = s.replace(/\(\s*\)/g, '') // empty parens left behind
  s = s.replace(/\s*;\s*/g, '; ') // normalise sense separators
  s = s.replace(/(?:;\s*)+/g, '; ')
  s = s.replace(/\s{2,}/g, ' ')
  s = s.replace(/\s+([;,.)])/g, '$1')
  s = s.replace(/^[\s;,]+|[\s;,]+$/g, '')
  return s.trim()
}

// CC-CEDICT separates SENSES with "/" (each becomes its own row) but within one sense uses ";" for
// BOTH synonyms ("I; me; my" — one meaning) AND, sometimes, genuinely distinct senses each carrying a
// scope marker ("(of a nation) to join an alliance; (of an athlete) to join a sports team; …"). The
// first must stay on one line; the second should enumerate. The reliable distinct-sense signal is a
// leading scope marker — "(…)", "lit.", "fig.", "(idiom)" — on the parts. So we split a single gloss
// into multiple senses ONLY when ≥2 of its ";"-parts carry such a marker; plain synonym lists are left
// as one line (so 我 stays "I; me; my", not "1. I 2. me 3. my").
const SENSE_MARKER = /^\(|^lit\.|^fig\.|^\s*idiom\b/i
export function expandSenses(glosses: string[]): string[] {
  const out: string[] = []
  for (const g of glosses) {
    const parts = g.split(';').map((s) => s.trim()).filter(Boolean)
    const marked = parts.filter((p) => SENSE_MARKER.test(p)).length
    if (parts.length >= 2 && marked >= 2) out.push(...parts)
    else out.push(g)
  }
  return out
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
    s.includes('kangxi radical') ||
    /radical\s*(?:number|no\b)/.test(s) ||
    /\(no\.\s*\d+\)/.test(s) ||
    s === 'kokuji'
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
/** True only when EVERY meaningful sense is bound — a genuinely always-bound morpheme. 日 has both
 * bound senses ("(bound form) sun") and free senses ("day"), so it is NOT always-bound; 的/號 are. */
export function isAlwaysBound(glosses: string[]): boolean {
  const real = glosses.filter(Boolean)
  return real.length > 0 && real.every((g) => BOUND_FORM_RE.test(g))
}

export type GlossPart = { v: string; link?: boolean }
// Han run: CJK Unified (incl. ext-A, U+3400–9FFF) + compat ideographs (U+F900–FAFF) + iteration
// mark 々, AND the Supplementary Ideographic Plane (Ext B–F, U+20000–3FFFF) matched via its surrogate
// pairs (high D840–D8BF, low DC00–DFFF). The SIP range is why 辵's 𣥆 (U+23946) and ~830 other origin
// glyphs were rendering as plain unlinked tofu. Explicit ranges + a surrogate alternation, not
// \p{Han}/the `u` flag: the build-time regex parser rejects Unicode script-name escapes.
// Use \u escapes, not literal chars: the literal lead of the compat-ideograph range was mis-typed as
// 豈 U+8C48 (a UNIFIED ideograph) instead of U+F900, so the class spanned U+8C48–U+FAFF and swallowed
// the entire Hangul Syllables block — Korean 말 was being turned into a (dead) link. Ranges: CJK Ext A
// + Unified (3400–9FFF), iteration marks 々〆 (3005–3006), CJK Compat Ideographs (F900–FAFF), and the
// Supplementary Ideographic Plane via surrogate pairs (item 159).
const HAN_RUN = /(?:[\u3400-\u9FFF\u3005\u3006\uF900-\uFAFF]|[\uD840-\uD8BF][\uDC00-\uDFFF])+/g
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

// === "Written for sound" marker (phonetic-loan / transliteration words) ===
// For a CHARACTER the app shows which component is semantic vs phonetic (媽 = 女 meaning + 馬 sound).
// For most multi-character WORDS that doesn't apply — words are semantic compounds. The exception is
// transliterations / phonetic loans, where the characters were chosen for their SOUND, not meaning
// (沙發 shāfā "sofa", 幽默 yōumò "humour", 俱樂部 "club"). We surface those with a small marker, driven
// ENTIRELY by the existing origin badges — no classifier. The strongest, precise signal is wiktextract's
// `phono-semantic-matching` (psm) badge: a foreign word written with sound-fitting characters. A plain
// `borrowed-*` badge alone is NOT enough (a Sino-Japanese loan / wasei-kango is borrowed but is still a
// meaning-compound), so we require the psm badge. Single characters are excluded by the caller (they use
// the component role display instead).
export function isSoundLoan(badges: string[] | null | undefined): boolean {
  return !!badges && badges.includes('phono-semantic-matching')
}

// === Middle Chinese "phonological why" ===
// A phono-semantic compound borrows its phonetic component's SOUND (媽 = 女 meaning + 馬 sound). The
// modern pinyin shows the present-day link; the Middle Chinese (廣韻 / Baxter) reading shows the
// HISTORICAL one, which is often closer (and sometimes diverged: 媽 muX in 廣韻 ≠ 馬 maeX, a modern-only
// re-analysis). These helpers split a Baxter reading so the UI can say WHETHER the sound link holds in
// Middle Chinese — honestly, never asserting a match that the readings don't support.

// Strip Baxter tone marks (final X = rising 上, H = departing 去; level/entering carry none).
function mcToneless(r: string): string {
  return r.replace(/[XH]$/, '')
}
// Baxter initial = the leading consonant letters before the first vowel (maeX → "m", duwng → "d",
// tshjeng → "tsh"). Used to test a shared initial consonant between a char and its phonetic part.
function mcInitial(r: string): string {
  const m = /^[^aeiou]*/.exec(mcToneless(r))
  return m ? m[0] : ''
}
// Baxter rhyme = everything from the first vowel onward, tone stripped (maeX → "ae", duwng → "uwng").
function mcRhyme(r: string): string {
  const t = mcToneless(r)
  const m = /[aeiou].*$/.exec(t)
  return m ? m[0] : t
}

export interface McLink {
  /** the character's own Middle Chinese reading(s), Baxter (媽 → ["muX"]) */
  charMc: string[]
  /** its phonetic component's Middle Chinese reading(s) (馬 → ["maeX"]) */
  compMc: string[]
  /** the phonetic component glyph (for the note: "shared a sound with 同") */
  comp: string
  /** how the two relate in Middle Chinese:
   *  - 'same'    identical reading (strong, certain link: 銅 duwng = 同 duwng)
   *  - 'related' some shared sound (same rhyme and/or same initial consonant)
   *  - 'diverged' no shared initial or rhyme (the series only works in the modern reading) */
  relation: 'same' | 'related' | 'diverged'
  /** a short plain-English sentence describing the historical sound link — phrased cautiously so a
   * partial resemblance (shared initial only) is never overstated as a full sound match */
  note: string
}

/** Build the Middle Chinese sound-link explanation for a phonetic component, or null when there isn't
 * enough data (one side lacks an MC reading). `comp` is the phonetic component glyph.
 *
 * Comparison is deliberately conservative on a Baxter string: an exact toneless match is a confident
 * link; a shared rhyme or shared initial is reported as a partial resemblance (never asserted as a
 * full match, since e.g. 媽 muX / 馬 maeX share only the m- initial); no overlap is reported as
 * divergence (the phonetic-series logic then holds only in the modern reading). */
export function mcSoundLink(
  charMc: string[] | undefined,
  compMc: string[] | undefined,
  comp: string,
): McLink | null {
  const a = (charMc ?? []).filter(Boolean)
  const b = (compMc ?? []).filter(Boolean)
  if (!a.length || !b.length) return null

  const exact = a.some((x) => b.some((y) => mcToneless(x) === mcToneless(y)))
  const sharedInitial = a.some((x) => b.some((y) => mcInitial(x) !== '' && mcInitial(x) === mcInitial(y)))
  const sharedRhyme = a.some((x) => b.some((y) => mcRhyme(x) === mcRhyme(y)))

  let relation: McLink['relation']
  let note: string
  if (exact) {
    relation = 'same'
    note = `In Middle Chinese both read the same (${mcToneless(a[0])}), so ${comp} clearly lent the sound.`
  } else if (sharedRhyme) {
    relation = 'related'
    note = sharedInitial
      ? `In Middle Chinese they shared the same initial and rhyme, marking ${comp} as the sound component.`
      : `In Middle Chinese they shared the same rhyme, marking ${comp} as the sound component.`
  } else if (sharedInitial) {
    relation = 'related'
    note = `In Middle Chinese they shared only the initial consonant, a partial sound link to ${comp}.`
  } else {
    relation = 'diverged'
    note = `Their Middle Chinese readings differ; ${comp} marks the sound in the modern reading, not the older one.`
  }
  return { charMc: a, compMc: b, comp, relation, note }
}

// The label + tooltip for the sound-loan marker. `borrowed-from-<lang>` (if present) names the source
// language, so we can say "loanword from English" rather than just "loanword".
const LANG_NAMES: Record<string, string> = {
  english: 'English', french: 'French', german: 'German', japanese: 'Japanese',
  sanskrit: 'Sanskrit', chinese: 'Chinese', dutch: 'Dutch', portuguese: 'Portuguese',
}
export function soundLoanSource(badges: string[] | null | undefined): string | null {
  if (!badges) return null
  for (const b of badges) {
    const m = /^borrowed-from-(.+)$/.exec(b)
    if (m && LANG_NAMES[m[1]]) return LANG_NAMES[m[1]]
  }
  return null
}
export function soundLoanTitle(badges: string[] | null | undefined): string {
  const src = soundLoanSource(badges)
  return src
    ? `Loanword from ${src}: the characters were chosen for their sound, not their meaning.`
    : 'Loanword: the characters were chosen for their sound, not their meaning.'
}

// === Origin (etymology) rendering ===
// Etymology arrives as one Wiktionary string that often MERGES several statements (newline-separated)
// and sometimes numbered "Etymology 1/2" sections, peppered with academic jargon (形聲, OC, STEDT,
// Proto-Sino-Tibetan…). We (1) split it into clearly-delineated segments, and within each segment
// (2) keep phonological reconstructions faint, (3) attach plain-English tooltips to the jargon, and
// (4) make every Han run tappable — composed as ordered passes so they don't fight each other.

// Plain-English glossary for the jargon. Longest keys first so phrases beat their abbreviations and
// CJK terms match before single chars. `word: true` adds \b boundaries (so "OC" ≠ inside "OCt").
// `abbr`: a short form to DISPLAY in place of a long term (the full term stays in the tooltip), e.g.
// "Nihon Shoki of 720 CE" → shows "Nihon Shoki". `cs`: match case-sensitively (for all-caps
// initialisms like OC/MC, so a lowercase "oc" in prose isn't wrongly tagged). Everything else matches
// case-insensitively with an optional trailing plural "s" (item 158).
type GlossEntry = { term: string; title: string; word?: boolean; abbr?: string; cs?: boolean }
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
  { term: 'STEDT', title: 'Sino-Tibetan Etymological Dictionary and Thesaurus, a comparative reconstruction project.', word: true },
  // Systematic widening (data-driven: terms occurring across our etymologies). Longest / most-specific
  // phrases first so they match before their bare-language forms. word:true keeps \b boundaries.
  { term: 'Proto-Tibeto-Burman', title: 'The reconstructed ancestor of Tibetan, Burmese and related languages.', word: true },
  { term: 'Proto-Austronesian', title: 'The reconstructed ancestor of the Austronesian (Pacific/Taiwan/SE-Asia) languages.', word: true },
  { term: 'Proto-Hmong-Mien', title: 'The reconstructed ancestor of the Hmong-Mien languages of south China.', word: true },
  { term: 'Proto-Mon-Khmer', title: 'The reconstructed ancestor of the Mon-Khmer (mainland SE-Asia) languages.', word: true },
  { term: 'Proto-Loloish', title: 'The reconstructed ancestor of the Loloish (Yi) languages of southwest China.', word: true },
  { term: 'Proto-Japonic', title: 'The reconstructed common ancestor of Japanese and the Ryukyuan languages.', word: true },
  { term: 'Proto-Vietic', title: 'The reconstructed ancestor of Vietnamese and its close relatives.', word: true },
  { term: 'Proto-Turkic', title: 'The reconstructed common ancestor of the Turkic languages.', word: true },
  { term: 'Proto-Tani', title: 'The reconstructed ancestor of the Tani languages of northeast India.', word: true },
  { term: 'Proto-Tai', title: 'The reconstructed common ancestor of the Tai languages (Thai, Lao, Zhuang).', word: true },
  { term: 'Middle Japanese', title: 'Japanese as spoken roughly 800-1600 CE.', word: true },
  { term: 'Middle Korean', title: 'Korean of roughly the 10th-16th centuries, the oldest well-recorded stage.', word: true },
  { term: 'Tibeto-Burman', title: 'The Sino-Tibetan branch including Tibetan, Burmese and many Himalayan languages.', word: true },
  { term: 'Sino-Tibetan', title: 'The language family that includes Chinese, Tibetan and Burmese.', word: true },
  { term: 'Austroasiatic', title: 'A Southeast-Asian language family including Vietnamese and Khmer.', word: true },
  { term: 'Austronesian', title: 'A vast family spanning Taiwan, the Pacific and Southeast Asia.', word: true },
  { term: 'Hmong-Mien', title: 'A language family of southern China and SE Asia (also called Miao-Yao).', word: true },
  { term: 'Mon-Khmer', title: 'The main branch of Austroasiatic, including Khmer and Mon.', word: true },
  { term: 'Kra-Dai', title: 'A SE-Asian language family including Thai, Lao and Zhuang (also Tai-Kadai).', word: true },
  { term: 'Tocharian B', title: 'An extinct Indo-European language of the Tarim Basin (western China), ~6th c. CE.', word: true },
  { term: 'Tocharian A', title: 'An extinct Indo-European language of the Tarim Basin (western China), ~6th c. CE.', word: true },
  { term: 'Tocharian', title: 'An extinct Indo-European language once spoken in western China.', word: true },
  { term: 'Japonic', title: 'The small family made up of Japanese and the Ryukyuan languages.', word: true },
  { term: 'Sinitic', title: 'The Chinese branch of Sino-Tibetan (all the Chinese languages/dialects).', word: true },
  { term: 'Tungusic', title: 'A language family of Manchuria and Siberia, including Manchu.', word: true },
  { term: 'Mongolic', title: 'The language family that includes Mongolian.', word: true },
  { term: 'Turkic', title: 'The language family that includes Turkish, Uyghur and Kazakh.', word: true },
  { term: 'Tangut', title: 'An extinct Sino-Tibetan language of the Western Xia kingdom (~11th-13th c.).', word: true },
  { term: 'Jurchen', title: 'An extinct Tungusic language of the Jin dynasty, ancestor of Manchu.', word: true },
  { term: 'Manchu', title: 'A Tungusic language of northeast China, the Qing dynasty court language.', word: true },
  { term: 'Sogdian', title: 'An extinct Iranian language of the Silk Road, ~4th-9th c. CE.', word: true },
  { term: 'Bactrian', title: 'An extinct Iranian language of ancient Afghanistan.', word: true },
  { term: 'Khotanese', title: 'An extinct Iranian language of the Silk Road oasis of Khotan.', word: true },
  { term: 'Prakrit', title: 'Everyday spoken descendants of Sanskrit in ancient India.', word: true },
  { term: 'Pali', title: 'An ancient Indian language, used for Buddhist scriptures.', word: true },
  { term: 'Ainu', title: 'The indigenous language of Hokkaido and northern Japan.', word: true },
  { term: "man'yōgana", title: 'Early use of Chinese characters purely for their sound to write Japanese.', word: true },
  { term: 'jukujikun', title: 'A whole Japanese word written with kanji chosen for meaning, not sound.', word: true },
  { term: 'kokuji', title: 'A character invented in Japan rather than borrowed from China.', word: true },
  { term: 'ateji', title: 'Kanji used for their sound to spell a word, ignoring their meaning.', word: true },
  { term: 'kango', title: 'A Japanese word built from Chinese-derived roots.', word: true },
  { term: 'rendaku', title: 'In Japanese, a voicing change to the start of a second compound element.', word: true },
  { term: 'Baxter-Sagart', title: 'A widely-used 2014 reconstruction of Old Chinese pronunciation.', word: true },
  { term: 'Schuessler', title: 'Axel Schuessler, author of a standard Old Chinese etymological dictionary.', word: true },
  { term: 'Zhengzhang', title: 'Zhengzhang Shangfang, author of a major Old Chinese reconstruction.', word: true },
  { term: 'Pulleyblank', title: 'E. G. Pulleyblank, a scholar of Middle Chinese reconstruction.', word: true },
  { term: 'Karlgren', title: 'Bernhard Karlgren, pioneer of Chinese historical phonology.', word: true },
  { term: 'phonetic component', title: 'The part of a character that hints at its pronunciation.', word: true },
  { term: 'semantic component', title: 'The part of a character that hints at its meaning.', word: true },
  { term: 'phonetic series', title: 'A set of characters sharing one sound-hinting part.', word: true },
  { term: 'folk etymology', title: 'A popular but historically wrong story of a word’s origin.', word: true },
  { term: 'semantic shift', title: 'A change in a word’s meaning over time.', word: true },
  { term: 'internationalism', title: 'A word borrowed in similar form across many languages.', word: true },
  { term: 'transliteration', title: 'Spelling a foreign word using another script’s letters or characters.', word: true },
  { term: 'reduplication', title: 'Repeating a word or syllable to form a new word.', word: true },
  { term: 'palatalization', title: 'A sound shifting toward a ‘y’-like position in the mouth.', word: true },
  { term: 'intransitive', title: 'A verb that takes no direct object.', word: true },
  { term: 'transitive', title: 'A verb that takes a direct object.', word: true },
  { term: 'ideogrammic', title: 'Of a character whose parts together picture its meaning.', word: true },
  { term: 'onomatopoeia', title: 'A word that imitates the sound it names.', word: true },
  { term: 'onomatopoeic', title: 'Imitating the sound a thing makes.', word: true },
  { term: 'wanderwort', title: 'A word that has spread by borrowing across many languages.', word: true },
  { term: 'substratum', title: 'An older local language that left traces in the one that replaced it.', word: true },
  { term: 'substrate', title: 'An older local language that left traces in the one that replaced it.', word: true },
  { term: 'metathesis', title: 'Swapping the order of sounds in a word.', word: true },
  { term: 'gemination', title: 'Lengthening a consonant by doubling it.', word: true },
  { term: 'lenition', title: 'A consonant softening or weakening over time.', word: true },
  { term: 'causative', title: 'A form meaning ‘to make someone do’ the action.', word: true },
  { term: 'diminutive', title: 'A form marking something as small or endearing.', word: true },
  { term: 'honorific', title: 'A respectful or polite form used for status.', word: true },
  { term: 'classifier', title: 'A counting word used with numbers (like ‘two SHEETS of paper’).', word: true },
  { term: 'vernacular', title: 'The everyday spoken form of a language.', word: true },
  { term: 'ideophone', title: 'A vivid word evoking a sound, look or feeling.', word: true },
  { term: 'doublet', title: 'Two words in one language descended from the same source.', word: true },
  { term: 'clipping', title: 'A word made by shortening a longer one.', word: true },
  { term: 'ablaut', title: 'A meaningful vowel change inside a word (like sing/sang/sung).', word: true },
  { term: 'sandhi', title: 'Sound changes where words or syllables meet.', word: true },
  { term: 'attested', title: 'Actually recorded in surviving texts (not just reconstructed).', word: true },
  { term: 'reflex', title: 'A later form descended from an earlier word or sound.', word: true },
  { term: 'etymon', title: 'The earlier word a later word came from.', word: true },
  { term: 'areal', title: 'Shared among neighbouring languages through contact, not ancestry.', word: true },
  { term: 'rebus', title: 'Using a sign for a like-sounding word (sound, not meaning).', word: true },
  // The 六書 classification terms (形聲/會意/象形/指事/假借…) are deliberately NOT glossed here: they
  // are real words with their own entries, so they fall through to the Han-linkify pass and become
  // tappable hyperlinks (item 12) instead of tooltip pop-ups. Their English twins above (Pictogram,
  // Ideogram…) still carry the plain-language tooltip for readers who don't tap through.
  { term: 'calque', title: 'A word translated piece by piece from another language.', word: true },
  { term: 'cognate', title: 'A word sharing a common ancestor with another.', word: true },
  { term: 'OC', title: 'Old Chinese (~1000 BCE).', word: true, cs: true },
  { term: 'MC', title: 'Middle Chinese (~600 CE).', word: true, cs: true },
  { term: 'OJ', title: 'Old Japanese (8th century).', word: true, cs: true },
  { term: 'PST', title: 'Proto-Sino-Tibetan.', word: true, cs: true },
  { term: 'PIE', title: 'Proto-Indo-European.', word: true, cs: true },
  // Historical sources & script stages (item 158). Listed longest-variant first so the dated forms
  // ("… of 720 CE") match and collapse to the work name before the bare name is tried.
  { term: 'Nihon Shoki of 720 CE', abbr: 'Nihon Shoki', title: 'Nihon Shoki — Japan’s oldest official chronicle, compiled 720 CE.', word: true },
  { term: 'Kojiki of 712 CE', abbr: 'Kojiki', title: 'Kojiki — Japan’s oldest extant chronicle, compiled 712 CE.', word: true },
  { term: "Man'yōshū of 759 CE", abbr: "Man'yōshū", title: 'Man’yōshū — the oldest Japanese poetry anthology, compiled after 759 CE.', word: true },
  { term: 'Shuowen Jiezi', title: 'The first Chinese character dictionary, ~100 CE.', word: true },
  { term: 'Classic of Poetry', title: 'China’s oldest poetry collection (~1000-600 BCE), the Shijing.', word: true },
  { term: 'Kangxi dictionary', title: 'The imperial Chinese character dictionary of 1716.', word: true },
  { term: 'oracle bone script', title: 'The earliest Chinese writing, carved on bone and shell (~1200 BCE).', word: true },
  { term: 'bronze inscriptions', title: 'Chinese writing cast on ritual bronzes (Shang–Zhou).', word: true },
  { term: 'bronze inscription', title: 'Chinese writing cast on ritual bronzes (Shang–Zhou).', word: true },
  { term: 'clerical script', title: 'The Han-era Chinese script that turned seal curves into flat strokes.', word: true },
  { term: 'seal script', title: 'The formal Chinese script standardised under the Qin (~3rd c. BCE).', word: true },
  { term: 'Nihon Shoki', title: 'Japan’s oldest official chronicle, 720 CE.', word: true },
  { term: 'Nihongi', title: 'Alternative name for the Nihon Shoki, Japan’s oldest official chronicle (720 CE).', word: true },
  { term: 'Kojiki', title: 'Japan’s oldest extant chronicle, 712 CE.', word: true },
  // proper nouns ending in a macron vowel (ū/ō) can't use word:true: the ASCII \b boundary fails after
  // a non-ASCII letter, so they'd never match. They're distinctive enough to match as substrings.
  { term: "Man'yōshū", title: 'The oldest Japanese poetry anthology (after 759 CE).' },
  { term: 'Shuowen', title: 'The Shuowen Jiezi, the first Chinese character dictionary (~100 CE).', word: true },
  { term: 'Guangyun', title: 'A 1008 CE Chinese rhyme dictionary, a key source for Middle Chinese.', word: true },
  { term: 'Guangya', title: 'A 3rd-c. CE Chinese dictionary expanding the Erya.', word: true },
  { term: 'Fangyan', title: 'Yang Xiong’s ~1st-c. BCE dictionary of regional Chinese words.', word: true },
  { term: 'Jiyun', title: 'An expanded 1037 CE Chinese rhyme dictionary.', word: true },
  { term: 'Shijing', title: 'The Classic of Poetry, China’s oldest poetry collection (~1000-600 BCE).', word: true },
  { term: 'Erya', title: 'The oldest Chinese dictionary/thesaurus, ~3rd c. BCE.', word: true },
  { term: 'Wamyō Ruijushō', title: 'A ~931-938 CE Japanese dictionary, the oldest arranged by meaning.' },
  { term: 'Wamyōshū', title: 'The Wamyō Ruijushō, a ~931-938 CE Japanese classified dictionary.' },
  { term: 'Liji', title: 'The Book of Rites, a Confucian classic on ritual and conduct (~1st c. BCE).', word: true },
  { term: 'Book of Rites', title: 'The Liji, a Confucian classic on ritual and conduct (~1st c. BCE).', word: true },
]
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
// Two passes: all-caps initialisms (cs) stay case-sensitive; everything else matches case-insensitively
// with an optional plural "s" so "Cognate"/"cognate"/"cognates" are treated identically (item 158).
const CI_ENTRIES = ETY_GLOSSARY.filter((e) => !e.cs)
const CS_ENTRIES = ETY_GLOSSARY.filter((e) => e.cs)
const glossSrc = (es: GlossEntry[], plural: boolean) =>
  es.map((e) => (e.word ? `\\b${esc(e.term)}${plural ? '(?:es|s)?' : ''}\\b` : esc(e.term))).join('|')
const ETY_GLOSS_RE_CI = new RegExp(glossSrc(CI_ENTRIES, true), 'gi')
const ETY_GLOSS_RE_CS = new RegExp(glossSrc(CS_ENTRIES, false), 'g')
// exact-keyed (used by the reconstruction register lookup, e.g. "(OC *…)" → OC's plain-English note)
const ETY_GLOSS_TITLE = new Map(ETY_GLOSSARY.map((e) => [e.term, e.title]))
const CI_BY_KEY = new Map(CI_ENTRIES.map((e) => [e.term.toLowerCase(), e]))
const CS_BY_KEY = new Map(CS_ENTRIES.map((e) => [e.term, e]))
// resolve a matched surface form → {display, title}: case/plural-insensitive, applying any abbr.
function ciGloss(v: string): { v: string; title: string } {
  const k = v.toLowerCase()
  const e = CI_BY_KEY.get(k) ?? CI_BY_KEY.get(k.replace(/s$/, '')) ?? CI_BY_KEY.get(k.replace(/es$/, ''))
  return e ? { v: e.abbr ?? v, title: e.title } : { v, title: '' }
}

export type EtyInline =
  | { t: 'ruby'; base: string; rt: string }
  | { t: 'recon'; v: string; title?: string }
  | { t: 'abbr'; v: string; title: string }
  | { t: 'han'; v: string }
  | { t: 'text'; v: string }
export interface EtySegment {
  /** numbered-section label ("Etymology 2") when the source delineates several, else null */
  heading: string | null
  /** indent depth for Wiktionary bullet sub-points (a line led by * / **). 0 = top-level paragraph.
   * These are nested points under a lead-in ("…proposes two etymologies: * X ** if so… * Y"); the
   * raw "*" used to leak as an unexplained character, so we lift it to real indentation instead. */
  depth: number
  /** 1-based position when the line is a numbered ("#") Wiktionary list item, else null. Wiktionary
   * stores ordered alternatives with "#" markers (天's four head-shape theories); without this they
   * leaked as a literal "#". A run of consecutive "#" lines numbers 1,2,3…; any other line resets it. */
  ordinal: number | null
  /** true when this is the first paragraph of an ALTERNATIVE account stacked after another (a line
   * that opens a competing theory: "From …", "Alternatively…", "Author (YEAR)…"). The UI sets it off
   * so three unrelated theories don't read as one run-on origin (item 10, e.g. 古). */
  alt: boolean
  /** true for a supplementary deep comparative-linguistics paragraph (cross-family cognates /
   * reconstructions: "STEDT compares…", "Cognate with…", "According to Schuessler (2007)…"). The core
   * account of the character's own formation is never marked deep; the UI tucks the deep ones behind a
   * "show deeper cognates" toggle so the everyday reader sees the plain origin first. */
  deep: boolean
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
  // 3. jargon → plain-English tooltips (and short forms for long historical-source names). Case-
  //    sensitive initialisms first, then the case-insensitive prose terms.
  toks = expandText(toks, (x) =>
    tokenizeBy(x, ETY_GLOSS_RE_CS, (v) => {
      const e = CS_BY_KEY.get(v)
      return { t: 'abbr', v: e?.abbr ?? v, title: e?.title ?? '' }
    }),
  )
  toks = expandText(toks, (x) =>
    tokenizeBy(x, ETY_GLOSS_RE_CI, (v) => {
      const r = ciGloss(v)
      return { t: 'abbr', v: r.v, title: r.title }
    }),
  )
  // 4. remaining Han runs → tappable
  toks = expandText(toks, (x) =>
    tokenizeBy(x, HAN_RUN, (v) => ({ t: 'han', v })),
  )
  return toks
}

/** Split merged etymology prose into delineated segments (one per newline-separated statement),
 * lifting "Etymology N" markers to segment headings and stripping Wiktionary "; " list leaks. */
// The horizontal rule before a stacked paragraph marks a COMPETING / alternative origin theory, so it
// must only fire on lines that genuinely open a new account ("From …", "Alternatively…", a dated
// "Author (1998)…" reconstruction). "Compare …" and "Cognate with …" are supplementary
// cross-references, not rival theories — they were drawing a divider mid-account, so they're excluded.
const ALT_LEADIN = /^(From |Possibly |Perhaps |Alternatively\b|[A-Z][a-zA-Z]+ \(\d{4}\))/

// A supplementary DEEP comparative-linguistics paragraph: cross-family cognates and reconstructions,
// not the character's own formation. These read as dense ("STEDT compares 發 to Proto-Sino-Tibetan
// *m-p(r)ats…"), so the UI hides them behind a toggle. Never applied to the FIRST paragraph (the core
// account). Kept deliberately narrow so a plain "From X" / "Pictogram" lead is never swept in.
const DEEP_LEADIN =
  /^(Cognate|Cognates|Compare\b|Cf\.|STEDT\b|Possibly cognate|Probably cognate|Related to\b|According to [A-Z][a-zA-Z]+ \(\d{4}\)|This is (?:an? )?area word|Sino-Tibetan\b)/i

export function etymologyTokens(text: string): EtySegment[] {
  const segs: EtySegment[] = []
  let heading: string | null = null
  let ordCounter = 0 // running number for a run of consecutive "#" ordered-list items
  let topLevelSeen = 0 // how many depth-0 paragraphs we've emitted (to detect stacked theories)
  let deepRun = false // a deep depth-0 paragraph's nested sub-points inherit its deep flag
  for (const raw of text.split('\n')) {
    let line = raw.trim()
    if (!line) continue
    const hm = line.match(/^;?\s*(Etymology\s+\d+)\s*$/i)
    if (hm) {
      heading = hm[1]
      ordCounter = 0
      continue
    }
    // drop a leading "; " definition-list marker, and an orphan "]" left when a [reference] tag was
    // stripped upstream (a few entries, e.g. 車, literally start "]\nPictogram…").
    line = line.replace(/^[;\]]\s*/, '').trim()
    // Wiktionary list markers: "*"/"**" are bullet sub-points; "#"/"##" are NUMBERED sub-points
    // (天's four head-shape theories). "*:" is a leaked pronunciation/IPA-table row, not prose. Lift
    // the marker to a real indent depth; "#" runs get a 1,2,3… ordinal; drop "*:" leaks entirely.
    let depth = 0
    let ordinal: number | null = null
    const bm = line.match(/^([*#]+)(:+)?[ \t]+/)
    if (bm) {
      if (bm[2]) continue // "*: …" / "#: …" pronunciation table — not etymology
      depth = bm[1].length
      if (bm[1][0] === '#') {
        ordCounter += 1
        ordinal = ordCounter
      } else {
        ordCounter = 0 // a bullet breaks an ordered run
      }
      line = line.slice(bm[0].length).trim()
    } else {
      ordCounter = 0 // any non-list line ends an ordered run
    }
    if (!line) continue
    // a line that is now just ONE stray ASCII char (":", "*", ".", "]", "+", "#", "-" or a lone
    // laryngeal "h"/letter left over from a wiki template) is upstream markup noise, never prose —
    // skip it. Single-codepoint CJK lines are kept (a character can legitimately stand alone). (153)
    if ([...line].length === 1 && /^[\x21-\x7e▲]$/.test(line)) continue
    // "More at *márkos." is Wiktionary's cross-reference to a fuller (proto-form) entry that Kogu does
    // not have — a dead link. Drop the trailing "More at …." sentence. (item 159)
    line = line.replace(/\s*\bMore at [^.]*\.\s*$/i, '').trim()
    // a raw "|" leaks from Wiktionary as an alternate-reading separator ("MC 'jij lje|lejH") or a
    // trad|simp pair; it renders as a bare vertical line with no meaning. Show a clear " / " instead.
    line = line.replace(/\s*\|\s*/g, ' / ')
    if (!line) continue
    // a competing theory stacked as a fresh top-level paragraph after the first (古: graphic theory,
    // then 苦 theory, then the Sino-Tibetan word origin) is flagged so the UI separates them.
    const alt = depth === 0 && topLevelSeen > 0 && ALT_LEADIN.test(line)
    // classify deep comparative paragraphs (never the first); nested points inherit the run.
    let deep: boolean
    if (depth === 0) {
      deep = topLevelSeen > 0 && DEEP_LEADIN.test(line)
      deepRun = deep
    } else {
      deep = deepRun
    }
    segs.push({ heading, depth, ordinal, alt, deep, tokens: inlineEty(line) })
    if (depth === 0) topLevelSeen += 1
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
  const han = '(?:[\u3400-\u9FFF\uF900-\uFAFF\u3005\u3006]|[\uD840-\uD8BF][\uDC00-\uDFFF])+'
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

// === Japanese pitch accent (Kanjium) ===
// The backend carries the downstep mora index ("accent") on a ja kana reading. Tokyo-dialect pitch is
// a binary high/low contour determined entirely by that one number and the mora count:
//   0  heiban   — mora 1 low, all the rest high, no drop (a following particle stays high)
//   1  atamadaka — mora 1 high, all the rest low
//   n (1<n<len)  nakadaka — rises after mora 1, stays high through mora n, then drops
//   n == len     odaka   — rises after mora 1, high to the end, drops onto the FOLLOWING particle
// These helpers are pure so they can be unit-tested directly and rendered as a monochrome overline
// with a downstep tick in the UI.

// Small ya/yu/yo (and the small vowels) bind to the PREVIOUS kana into one mora; the long-vowel mark
// ー, the sokuon っ and the moraic ん each count as their own mora (standard pitch-accent counting).
const SMALL_KANA = new Set([
  'ゃ', 'ゅ', 'ょ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ',
  'ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ', 'ゎ', 'ヮ',
])

/** Split a kana string into morae (small ya/yu/yo merge into the preceding mora). */
export function moraSplit(kana: string): string[] {
  const out: string[] = []
  for (const ch of kana) {
    if (SMALL_KANA.has(ch) && out.length) out[out.length - 1] += ch
    else out.push(ch)
  }
  return out
}

export interface PitchPattern {
  /** one entry per mora: true = high, false = low */
  highs: boolean[]
  /** 0-based index of the mora AFTER which the pitch drops, or null when there is no drop (heiban).
   * Equal to highs.length for odaka (the drop lands on a following particle, outside the word). */
  downstep: number | null
  /** the parsed accent class, for labelling / tooltips */
  kind: 'heiban' | 'atamadaka' | 'nakadaka' | 'odaka'
}

/** Per-mora high/low contour for a kana reading under a Kanjium downstep index. Returns null when
 * there's no kana, no accent value, or the accent isn't a usable non-negative integer. A multi-accent
 * string ("2,1") uses its FIRST value (the commonest). Follows standard Tokyo pitch rules. */
export function pitchPattern(kana: string, accent: string | number | null | undefined): PitchPattern | null {
  if (!kana) return null
  if (accent === null || accent === undefined || accent === '') return null
  const first = String(accent).split(',')[0].trim()
  if (!/^\d+$/.test(first)) return null
  const n = parseInt(first, 10)
  const morae = moraSplit(kana)
  const len = morae.length
  if (len === 0) return null
  if (n > len) return null // malformed: downstep past the end of the word

  const highs = new Array<boolean>(len)
  if (n === 0) {
    // heiban: mora 1 low, rest high, no drop
    for (let i = 0; i < len; i++) highs[i] = i !== 0
    return { highs, downstep: null, kind: 'heiban' }
  }
  if (n === 1) {
    // atamadaka: mora 1 high, rest low
    for (let i = 0; i < len; i++) highs[i] = i === 0
    return { highs, downstep: 1, kind: 'atamadaka' }
  }
  // nakadaka / odaka: low on mora 1, high from mora 2 through mora n, low after
  for (let i = 0; i < len; i++) highs[i] = i !== 0 && i < n
  const kind = n === len ? 'odaka' : 'nakadaka'
  return { highs, downstep: n, kind }
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
