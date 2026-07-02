// Candidate queries for "did you mean" when a search matched nothing.
// /suggest is prefix-based, so a typo in the MIDDLE of a romanized reading ("xuexaio") never
// matches any prefix of the real word. Adjacent-letter transposition is the dominant class of
// that typo, and undoing it restores a real prefix ("xuexiao"), so we try each swap first,
// then fall back to progressively shorter prefixes (recovers trailing junk: "mountainz").
// Latin-only: transposing Han glyphs or kana just makes a different (or broken) word.

const LATIN = /^[a-z0-9\s'-]+$/i

/** adjacent transpositions of `t`, deduplicated, original excluded */
export function transpositions(t: string): string[] {
  if (!LATIN.test(t)) return []
  const out: string[] = []
  const seen = new Set<string>([t])
  for (let i = 0; i + 1 < t.length; i++) {
    if (t[i] === t[i + 1]) continue
    const v = t.slice(0, i) + t[i + 1] + t[i] + t.slice(i + 2)
    if (!seen.has(v)) {
      seen.add(v)
      out.push(v)
    }
  }
  return out
}

/** ordered candidate list for the did-you-mean lookup, capped so we don't hammer /suggest */
export function typoCandidates(term: string, cap = 8): string[] {
  const t = term.trim()
  if ([...t].length < 2) return []
  const out: string[] = [t, ...transpositions(t)]
  // shorter prefixes after the swaps (a swap that also has trailing junk is out of scope)
  for (let p = t.slice(0, -1); [...p].length >= 2 && out.length < cap + 4; p = p.slice(0, -1))
    out.push(p)
  return out.slice(0, cap)
}
