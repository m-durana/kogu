// Kana → Hepburn romaji for displaying Japanese on'yomi / kun'yomi to non-Japanese readers.
// Ported from pipeline/kogupipe/ingest/romaji.py (the search-fold table), plus: on'yomi long-vowel
// macrons (コウ → kō), KANJIDIC marker stripping (leading/trailing '-'), and okurigana boundary
// handling ('.' is dropped in the romaji; the kana keeps it). Pure, no deps.

const BASE: Record<string, string> = {
  あ: 'a', い: 'i', う: 'u', え: 'e', お: 'o',
  か: 'ka', き: 'ki', く: 'ku', け: 'ke', こ: 'ko',
  が: 'ga', ぎ: 'gi', ぐ: 'gu', げ: 'ge', ご: 'go',
  さ: 'sa', し: 'shi', す: 'su', せ: 'se', そ: 'so',
  ざ: 'za', じ: 'ji', ず: 'zu', ぜ: 'ze', ぞ: 'zo',
  た: 'ta', ち: 'chi', つ: 'tsu', て: 'te', と: 'to',
  だ: 'da', ぢ: 'ji', づ: 'zu', で: 'de', ど: 'do',
  な: 'na', に: 'ni', ぬ: 'nu', ね: 'ne', の: 'no',
  は: 'ha', ひ: 'hi', ふ: 'fu', へ: 'he', ほ: 'ho',
  ば: 'ba', び: 'bi', ぶ: 'bu', べ: 'be', ぼ: 'bo',
  ぱ: 'pa', ぴ: 'pi', ぷ: 'pu', ぺ: 'pe', ぽ: 'po',
  ま: 'ma', み: 'mi', む: 'mu', め: 'me', も: 'mo',
  や: 'ya', ゆ: 'yu', よ: 'yo',
  ら: 'ra', り: 'ri', る: 'ru', れ: 're', ろ: 'ro',
  わ: 'wa', ゐ: 'i', ゑ: 'e', を: 'o', ん: 'n',
  ぁ: 'a', ぃ: 'i', ぅ: 'u', ぇ: 'e', ぉ: 'o',
  ゔ: 'vu',
}
const YOON: Record<string, string> = { ゃ: 'ya', ゅ: 'yu', ょ: 'yo' }
const VOWELS = new Set(['a', 'e', 'i', 'o', 'u'])

function kanaToRomaji(input: string): string {
  // normalise katakana → hiragana
  const s = [...input].map((c) => (c >= 'ァ' && c <= 'ヶ' ? String.fromCharCode(c.charCodeAt(0) - 0x60) : c)).join('')
  const out: string[] = []
  let i = 0
  while (i < s.length) {
    const c = s[i]
    if (c === 'っ') {
      const r = BASE[s[i + 1]] ?? ''
      if (r && !VOWELS.has(r[0])) out.push(r[0])
      i += 1
      continue
    }
    if (c === 'ー' || c === '～') {
      const last = out[out.length - 1]
      if (last && VOWELS.has(last[last.length - 1])) out.push(last[last.length - 1])
      i += 1
      continue
    }
    if (i + 1 < s.length && YOON[s[i + 1]]) {
      const base = BASE[c] ?? ''
      if (base.endsWith('i') && base !== 'i') {
        const stem = base.slice(0, -1)
        out.push(/(sh|ch|j)$/.test(stem) ? stem + YOON[s[i + 1]].slice(1) : stem + YOON[s[i + 1]])
        i += 2
        continue
      }
    }
    out.push(BASE[c] ?? '')
    i += 1
  }
  return out.join('')
}

// on'yomi are single morphemes: fold long vowels to macrons (kou → kō, shuu → shū). Leave 'ei' as-is.
function macronize(r: string): string {
  return r
    .replace(/ou|oo/g, 'ō')
    .replace(/uu/g, 'ū')
    .replace(/aa/g, 'ā')
    .replace(/ii/g, 'ī')
}

/** Display romaji for a KANJIDIC on/kun reading. on'yomi gets Hepburn macrons; the okurigana '.' is
 * dropped (kana keeps it); KANJIDIC prefix/suffix '-' markers are stripped. */
export function readingRomaji(kind: 'onyomi' | 'kunyomi', value: string): string {
  const cleaned = value.replace(/^-+|-+$/g, '')
  const r = cleaned
    .split('.')
    .map(kanaToRomaji)
    .join('')
  return kind === 'onyomi' ? macronize(r) : r
}
