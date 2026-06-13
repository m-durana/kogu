import type { Entry, SearchResponse } from './types'

const BASE = '/api'

export async function search(q: string, script?: string, signal?: AbortSignal): Promise<SearchResponse> {
  const u = new URL(BASE + '/search', location.origin)
  u.searchParams.set('q', q)
  if (script) u.searchParams.set('script', script)
  const r = await fetch(u, { signal })
  if (!r.ok) throw new Error(`search failed: ${r.status}`)
  return r.json()
}

export async function entry(id: number): Promise<Entry> {
  const r = await fetch(`${BASE}/entry/${id}`)
  if (!r.ok) throw new Error(`entry failed: ${r.status}`)
  return r.json()
}

export type Stroke = [number, number, number][]

export async function recognize(
  width: number,
  height: number,
  strokes: Stroke[],
  languages: string[] = ['zh', 'ja'],
): Promise<{ candidates: string[]; languages: string[] }> {
  const r = await fetch(`${BASE}/recognize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ width, height, strokes, languages }),
  })
  if (!r.ok) throw new Error(`recognize failed: ${r.status}`)
  return r.json()
}
