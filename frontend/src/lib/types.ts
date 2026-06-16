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
  readings: ReadingKV[]
  variants: VariantEdge[]
  script_forms: ScriptForms | null
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
