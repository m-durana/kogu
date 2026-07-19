import type { Entry, SearchResponse, Variety } from './types'

const BASE = '/api'

// One retry on TRANSIENT failures (nginx rate-limit 503, gateway 502/504, 429, or a dropped
// connection). Every click fires /search AND /entry back-to-back, so a flurry of taps can briefly trip
// the search rate-limit and surface a spurious "search failed"; a single short retry absorbs that.
// 4xx (bad query/id) fails fast; an aborted request (superseded by a newer search) rethrows AbortError
// so the caller ignores it rather than showing an error.
const TRANSIENT = new Set([429, 502, 503, 504])
const RETRY_BACKOFF = [250, 700] // ms before retry 1, retry 2 (3 attempts total)
// Per-attempt deadline: without one, a black-holed connection (mobile radio dropping mid-request)
// hangs the fetch for the OS's 30-60s: the load bar animates forever and the user can't tell
// "slow" from "stuck". 10s is generous for a <50ms API; after it, the attempt aborts and the retry
// (or the final error strip, which offers a way forward) takes over.
const ATTEMPT_TIMEOUT_MS = 10_000
function withDeadline(signal: AbortSignal | undefined): AbortSignal {
  const t = AbortSignal.timeout(ATTEMPT_TIMEOUT_MS)
  return signal ? AbortSignal.any([signal, t]) : t
}
async function fetchRetry(input: URL | string, init: RequestInit, label: string): Promise<Response> {
  let lastErr: unknown
  const callerSignal = init.signal ?? undefined
  for (let attempt = 0; attempt < 3; attempt++) {
    if (attempt) await new Promise((r) => setTimeout(r, RETRY_BACKOFF[attempt - 1]))
    try {
      const r = await fetch(input, { ...init, signal: withDeadline(callerSignal as AbortSignal | undefined) })
      if (r.ok) return r
      const err = new Error(`${label} failed: ${r.status}`) as Error & { fatal?: boolean }
      // fail fast on 4xx etc. — TAGGED, because this throw lands in the catch below, which must
      // not swallow it into the retry loop (it used to: every 400/404/500 got 3 attempts)
      if (!TRANSIENT.has(r.status)) {
        err.fatal = true
        throw err
      }
      lastErr = err
    } catch (e) {
      if ((e as { fatal?: boolean }).fatal) throw e
      // the CALLER's abort (superseded search) rethrows; a deadline abort just tries again
      if ((e as Error).name === 'AbortError' && callerSignal?.aborted) throw e
      if ((e as Error).name === 'TimeoutError' && callerSignal?.aborted) throw e
      lastErr = e
    }
  }
  throw lastErr
}

export async function search(q: string, script?: string, signal?: AbortSignal): Promise<SearchResponse> {
  const u = new URL(BASE + '/search', location.origin)
  u.searchParams.set('q', q)
  if (script) u.searchParams.set('script', script)
  const r = await fetchRetry(u, { signal }, 'search')
  return r.json()
}

export interface Suggestion {
  headword: string
  reading: string | null
  variety: string
}
// Lightweight autocomplete (no retry: a stale keystroke is simply superseded by the next one).
export async function suggest(q: string, signal?: AbortSignal): Promise<Suggestion[]> {
  const u = new URL(BASE + '/suggest', location.origin)
  u.searchParams.set('q', q)
  const r = await fetch(u, { signal })
  if (!r.ok) return []
  const d = await r.json()
  return d.suggestions ?? []
}

export async function entry(id: number, signal?: AbortSignal): Promise<Entry> {
  const r = await fetchRetry(`${BASE}/entry/${id}`, { signal }, 'entry')
  return r.json()
}

export interface InterestingItem {
  lexeme_id: number
  variety: Variety
  headword: string
  reading: string | null
  gloss: string | null
  why: string
  category: string
}
// Homepage showcase: a fresh-random handful of noteworthy entries. Purely decorative, so it fails
// soft (returns []) rather than retrying or surfacing an error.
export async function interesting(limit = 8, signal?: AbortSignal): Promise<InterestingItem[]> {
  const u = new URL(BASE + '/interesting', location.origin)
  u.searchParams.set('limit', String(limit))
  try {
    const r = await fetch(u, { signal })
    if (!r.ok) return []
    const d = await r.json()
    return d.items ?? []
  } catch {
    return []
  }
}

export async function translate(q: string, signal?: AbortSignal): Promise<import('./types').TranslateResponse> {
  const u = new URL(BASE + '/translate', location.origin)
  u.searchParams.set('q', q)
  const r = await fetchRetry(u, { signal }, 'translate')
  return r.json()
}

export interface SegmentPart {
  form: string
  gloss: string
  lexeme_id?: number
}
// Greedily split an unrecognized Han query into the longest known sub-words for the "literally" hint.
export async function segment(q: string, signal?: AbortSignal): Promise<{ query: string; segments: SegmentPart[] }> {
  const u = new URL(BASE + '/segment', location.origin)
  u.searchParams.set('q', q)
  const r = await fetchRetry(u, { signal }, 'segment')
  return r.json()
}

export async function ocr(blob: Blob): Promise<import('./types').OcrResponse> {
  const r = await fetch(`${BASE}/ocr`, {
    method: 'POST',
    headers: { 'Content-Type': blob.type || 'image/jpeg' },
    body: blob,
  })
  if (r.status === 503) throw new Error('ocr_unavailable')
  if (!r.ok) throw new Error(`ocr failed: ${r.status}`)
  return r.json()
}

export type Stroke = [number, number, number][]

export async function recognize(
  width: number,
  height: number,
  strokes: Stroke[],
  languages: string[] = ['zh', 'ja'],
): Promise<{ candidates: string[]; languages: string[] }> {
  const body = JSON.stringify({ width, height, strokes, languages })
  // Retry once: the first request after a cold PWA launch (radio/TLS waking) often fails.
  let lastErr: unknown
  for (let attempt = 0; attempt < 2; attempt++) {
    if (attempt) await new Promise((res) => setTimeout(res, 400))
    try {
      const r = await fetch(`${BASE}/recognize`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      })
      if (!r.ok) throw new Error(`recognize failed: ${r.status}`)
      return await r.json()
    } catch (e) {
      lastErr = e
    }
  }
  throw lastErr
}
