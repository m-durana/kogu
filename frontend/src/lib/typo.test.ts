import { describe, expect, it } from 'vitest'
import { transpositions, typoCandidates } from './typo'

describe('transpositions', () => {
  it('generates each adjacent swap once', () => {
    expect(transpositions('abc')).toEqual(['bac', 'acb'])
  })

  it('recovers the canonical pinyin from a swapped vowel pair', () => {
    expect(transpositions('xuexaio')).toContain('xuexiao')
  })

  it('skips identical-letter swaps and deduplicates', () => {
    expect(transpositions('aab')).toEqual(['aba'])
  })

  it('returns nothing for non-Latin input (swapping Han glyphs makes a different word)', () => {
    expect(transpositions('学校')).toEqual([])
    expect(transpositions('がっこう')).toEqual([])
  })
})

describe('typoCandidates', () => {
  it('orders: exact term, swaps, then shorter prefixes', () => {
    const c = typoCandidates('gakko')
    expect(c[0]).toBe('gakko')
    expect(c).toContain('agkko')
    expect(c[c.length - 1].length).toBeLessThan('gakko'.length)
  })

  it('caps the candidate count', () => {
    expect(typoCandidates('internationalization').length).toBeLessThanOrEqual(8)
  })

  it('still offers prefixes for non-Latin queries', () => {
    expect(typoCandidates('発展xx')).toEqual(['発展xx', '発展x', '発展'])
  })

  it('gives up on single characters', () => {
    expect(typoCandidates('x')).toEqual([])
    expect(typoCandidates(' 学 ')).toEqual([])
  })
})
