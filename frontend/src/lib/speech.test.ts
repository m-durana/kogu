import { describe, it, expect } from 'vitest'
import { zhFile, yueFile } from './speech'

describe('zhFile - numbered pinyin to davinfifield clip name', () => {
  it('keeps a plain syllable with its tone digit', () => {
    expect(zhFile('chang3')).toBe('chang3')
    expect(zhFile('xue2')).toBe('xue2')
  })
  it('maps CC-CEDICT ü ("u:") to doubled uu', () => {
    expect(zhFile('nu:3')).toBe('nuu3')
    expect(zhFile('lu:4')).toBe('luu4')
  })
  it('maps a literal ü and a v to uu too', () => {
    expect(zhFile('nü3')).toBe('nuu3')
    expect(zhFile('lv4')).toBe('luu4')
  })
  it('accepts neutral tone 5', () => {
    expect(zhFile('de5')).toBe('de5')
  })
  it('lowercases', () => {
    expect(zhFile('Qu1')).toBe('qu1')
  })
  it('rejects a non-syllable token (no tone, stray letter/number)', () => {
    expect(zhFile('C')).toBeNull()
    expect(zhFile('11')).toBeNull()
    expect(zhFile('chang')).toBeNull()
  })
})

describe('yueFile - jyutping to jyutping.org clip name', () => {
  it('keeps a jyutping syllable with tone 1-6', () => {
    expect(yueFile('mat1')).toBe('mat1')
    expect(yueFile('keoi5')).toBe('keoi5')
    expect(yueFile('sik6')).toBe('sik6')
  })
  it('lowercases', () => {
    expect(yueFile('Mou5')).toBe('mou5')
  })
  it('rejects a token without a tone digit', () => {
    expect(yueFile('mat')).toBeNull()
    expect(yueFile('')).toBeNull()
  })
})
