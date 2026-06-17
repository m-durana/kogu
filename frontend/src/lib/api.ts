import type { Entry, SearchResponse } from './types'

const BASE = '/api'

// One retry on TRANSIENT failures (nginx rate-limit 503, gateway 502/504, 429, or a dropped
// connection). Every click fires /search AND /entry back-to-back, so a flurry of taps can briefly trip
// the search rate-limit and surface a spurious "search failed"; a single short retry absorbs that.
// 4xx (bad query/id) fails fast; an aborted request (superseded by a newer search) rethrows AbortError
// so the caller ignores it rather than showing an error.
const TRANSIENT = new Set([429, 502, 503, 504])
const RETRY_BACKOFF = [250, 700] // ms before retry 1, retry 2 (3 attempts total)
async function fetchRetry(input: URL | string, init: RequestInit, label: string): Promise<Response> {
  let lastErr: unknown
  for (let attempt = 0; attempt < 3; attempt++) {
    if (attempt) await new Promise((r) => setTimeout(r, RETRY_BACKOFF[attempt - 1]))
    try {
      const r = await fetch(input, init)
      if (r.ok) return r
      const err = new Error(`${label} failed: ${r.status}`)
      if (!TRANSIENT.has(r.status)) throw err // fail fast on 4xx etc.
      lastErr = err
    } catch (e) {
      if ((e as Error).name === 'AbortError') throw e
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

export async function entry(id: number, signal?: AbortSignal): Promise<Entry> {
  const r = await fetchRetry(`${BASE}/entry/${id}`, { signal }, 'entry')
  return r.json()
}

export async function translate(q: string, signal?: AbortSignal): Promise<import('./types').TranslateResponse> {
  const u = new URL(BASE + '/translate', location.origin)
  u.searchParams.set('q', q)
  const r = await fetchRetry(u, { signal }, 'translate')
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
