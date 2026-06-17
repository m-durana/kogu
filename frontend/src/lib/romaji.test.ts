import { describe, it, expect } from 'vitest'
import { readingRomaji } from './romaji'

describe('readingRomaji - kana on/kun to Hepburn for non-Japanese readers', () => {
  it('romanizes katakana on\'yomi', () => {
    expect(readingRomaji('onyomi', 'デン')).toBe('den')
    expect(readingRomaji('onyomi', 'ガク')).toBe('gaku')
  })
  it('applies long-vowel macrons to on\'yomi', () => {
    expect(readingRomaji('onyomi', 'キョウ')).toBe('kyō') // 京 - yōon + long o
    expect(readingRomaji('onyomi', 'コウ')).toBe('kō')
    expect(readingRomaji('onyomi', 'シュウ')).toBe('shū')
  })
  it('romanizes hiragana kun\'yomi without macrons', () => {
    expect(readingRomaji('kunyomi', 'みやこ')).toBe('miyako') // 京
    expect(readingRomaji('kunyomi', 'いなずま')).toBe('inazuma') // 稲妻
  })
  it('drops the okurigana boundary "." in the romaji (kana keeps it)', () => {
    expect(readingRomaji('kunyomi', 'さけ.ぶ')).toBe('sakebu') // 叫 - saké.bu -> sakebu
  })
  it('strips KANJIDIC prefix/suffix "-" markers', () => {
    expect(readingRomaji('kunyomi', '-づ.く')).toBe('zuku')
    expect(readingRomaji('kunyomi', 'お-')).toBe('o')
  })
  it('handles sokuon (small tsu) gemination', () => {
    expect(readingRomaji('onyomi', 'ガッ')).toBe('ga') // trailing っ has nothing to double
    expect(readingRomaji('onyomi', 'ガッコウ')).toBe('gakkō') // doubled k + long o (on'yomi macron)
    expect(readingRomaji('kunyomi', 'がっこう')).toBe('gakkou') // kun'yomi: no macron folding
  })
})
