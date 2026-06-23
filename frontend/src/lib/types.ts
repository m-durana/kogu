export type Variety = 'zh' | 'yue' | 'ja'
export type Script = 'trad' | 'simp' | 'shinjitai' | 'kana' | 'mixed' | 'other'
export type PrefScript = 'trad' | 'simp'

export interface Form {
  form: string
  script: Script
  region: string | null
  is_primary: boolean
}

export interface Hit {
  lexeme_id: number
  variety: Variety
  headword: string
  reading: string | null
  forms: Form[]
  glosses: string[]
  match_type: string
  score: number
}

export interface SearchResponse {
  query: string
  classified_as: string
  results: Hit[]
}

export interface ReadingKV {
  kind: string
  value: string
  /** Japanese pitch accent (Kanjium) on a ja kind='kana' reading: the downstep mora index as a string
   * ("0"=heiban, "1"=atamadaka, n=drop after mora n; a multi-accent word keeps the comma list "2,1").
   * Absent for every reading without Kanjium accent data. */
  accent?: string | null
}
export interface VariantEdge {
  parent: string
  edge_type: string
  reform: string | null
  reform_name: string | null
  reform_year: number | null
}
export interface FormBranch {
  form: string
  script: string // "traditional" | "simplified" | "shinjitai" | "z-variant" (may be "+"-joined)
  reform_id: string | null
  reform_label: string | null
  is_orthodox: boolean
}
export interface ScriptForms {
  orthodox: string
  is_kokuji: boolean
  branches: FormBranch[]
}
export interface CharInfo {
  ch: string
  is_orthodox: boolean
  strokes: number | null
  radical: number | null
  ids: string | null
  gloss_en: string | null
  gloss_ja: string | null
  readings: ReadingKV[]
  variants: VariantEdge[]
  script_forms: ScriptForms | null
  /** set when the char is N copies of one base glyph (森 → {base:'木', count:3}); else null */
  decomp: { base: string; count: number } | null
  /** distinct components with meanings (好 → 女 "woman", 子 "child"); radical forms glossed via parent.
   * role: 'semantic' carries the meaning, 'phonetic' carries the sound (媽 = 女 semantic + 馬 phonetic).
   * mc_sound: a phonetic component's Middle Chinese (廣韻 / Baxter) reading(s) — the historical sound
   * it lent (同 → ["duwng"]); absent/empty for non-phonetic components or when no MC data exists. */
  components: { ch: string; gloss: string | null; role: string | null; sound: string | null; mc_sound?: string[] }[]
  /** the glyph is primarily a Kangxi radical / bound component (彳, 辵, 氵…), not a standalone word */
  is_radical: boolean
  /** Kangxi radical number when known */
  radical_number: number | null
  /** standalone character a radical-variant form stands for (氵→水), when it differs */
  standalone: string | null
  /** how many lexemes contain this character — global usage signal (0 = archaic, large = core) */
  used_count: number
  /** per-language containing-word counts ({zh,yue,ja}) for a language-specific rarity tag */
  used_by_variety: Record<string, number>
  /** per-language MAX word-frequency (0..1) among words containing this glyph; drives the rarity tag */
  freq_by_variety: Record<string, number>
}

export interface OriginAccount {
  variety: Variety
  headword: string
  text: string
  /** "traditional" | "simplified" when the glyph diverges across scripts; null when identical. */
  script?: string | null
  /** clarifying note when the glyph doubles as the simplified form of a distinct character. */
  note?: string | null
}
export interface CharLite {
  ch: string
  gloss: string | null
  /** rare extension-plane glyph (cp ≥ U+20000) the device font likely renders as tofu. */
  rare?: boolean
}
export interface Sense {
  pos: string | null
  gloss_en: string
}
export interface LinkLite {
  lexeme_id: number
  variety: Variety
  headword: string
  reading: string | null
  glosses: string[]
  relation: string // 'cognate' | 'false-friend' | 'synonym'
  concept: string | null
}
export interface Entry {
  lexeme_id: number
  variety: Variety
  headword: string
  reading: string | null
  freq: number | null
  forms: Form[]
  readings: ReadingKV[]
  senses: Sense[]
  characters: CharInfo[]
  same_form: LinkLite[]
  translations: LinkLite[]
  compounds: LinkLite[]
  origin_badges: string[]
  etymology: string | null
  /** per-language origin accounts (中 Sinitic, 日 Japonic) for the same glyph */
  origins: OriginAccount[]
  /** for a radical/bound-component entry: characters that contain it (replaces word "used in") */
  appears_in: CharLite[]
}

export interface ConceptGroup {
  concept: string
  members: LinkLite[]
}

export type Box = [number, number, number, number] // x, y, w, h in image px
export interface OcrChar {
  ch: string
  box: Box
}
export interface OcrLine {
  text: string
  confidence: number
  box: Box
  chars: OcrChar[]
}
export interface OcrResponse {
  width: number
  height: number
  lines: OcrLine[]
}
export interface TranslateResponse {
  query: string
  concepts: ConceptGroup[]
}
