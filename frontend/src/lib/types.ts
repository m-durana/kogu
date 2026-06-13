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
}
export interface Sense {
  pos: string | null
  gloss_en: string
}
export interface LinkLite {
  lexeme_id: number
  variety: Variety
  headword: string
  glosses: string[]
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
}
