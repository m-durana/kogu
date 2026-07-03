/**
 * Deep-run layer C: cross-skin consistency sweep over the REAL Unified component.
 *
 * For every character variant pair (桥/橋 class) and a large sample of two-skin words, this
 * renders the actual unified word card for BOTH skins (replicating App.svelte's search→enrich
 * pick exactly) and asserts the rendered semantics agree: same languages, same readings, same
 * glosses, same bound/usage tags, same resolved Japanese form. Also checks every "only in
 * compounds" tag against the DB truth (does a standalone ja word exist?) and scans for junk
 * text. Findings go to a JSONL file for triage; the single vitest assertion only fails on
 * harness breakage, not on findings.
 *
 * Run: python3 ../pipeline/... generates pairs_chars.json/pairs_words.json/ja_standalone.json into
 * SWEEP_DIR first (see the deep-run notes), then:
 *   SWEEP_DIR=/tmp/kogu-sweep SWEEP_SHARD=0 SWEEP_TOTAL=6 pnpm vitest run -c vitest.sweep.config.ts
 */
import { test, expect } from 'vitest'
import { render, cleanup } from '@testing-library/svelte'
import fs from 'node:fs'
import Unified from '../src/lib/Unified.svelte'

const DIR = process.env.SWEEP_DIR ?? '/tmp/kogu-sweep'
const BASE = process.env.SWEEP_BASE ?? 'http://127.0.0.1:4100'
const SHARD = Number(process.env.SWEEP_SHARD ?? 0)
const TOTAL = Number(process.env.SWEEP_TOTAL ?? 1)

// jsdom lacks ResizeObserver (used by the read/clamp probes): a no-op is fine headless
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
;(globalThis as any).ResizeObserver = RO
window.scrollTo = () => {}

type Hit = any
type FindRec = Record<string, unknown>

// crash-resilient: findings/latencies APPEND as they happen, and a done-file lets a rerun skip
// pairs already processed (the harness kept dying mid-run and losing everything)
const FIND_PATH = `${DIR}/findings_frontend_${SHARD}.jsonl`
const DONE_PATH = `${DIR}/done_${SHARD}.txt`
const LAT_PATH = `${DIR}/latency_${SHARD}.jsonl`
const done = new Set(
  fs.existsSync(DONE_PATH) ? fs.readFileSync(DONE_PATH, 'utf8').split('\n').filter(Boolean) : [],
)
let findingCount = 0

function fin(rec: FindRec) {
  fs.appendFileSync(FIND_PATH, JSON.stringify(rec) + '\n')
  findingCount++
}

let latBuf: string[] = []
async function api(path: string): Promise<any> {
  const t0 = performance.now()
  const r = await fetch(BASE + path)
  const j = r.ok ? await r.json() : null
  latBuf.push(JSON.stringify({ ep: path.split('?')[0].split('/').slice(0, 2).join('/'), ms: Math.round(performance.now() - t0) }))
  if (latBuf.length >= 100) {
    fs.appendFileSync(LAT_PATH, latBuf.join('\n') + '\n')
    latBuf = []
  }
  if (!r.ok) throw new Error(`${path} -> ${r.status}`)
  return j
}

// replicate App.svelte doSearch: which lexeme gets enriched for a Han query
async function fetchView(q: string): Promise<{ hits: Hit[]; entry: any } | null> {
  const s = await api('/search?q=' + encodeURIComponent(q))
  const results: Hit[] = s.results ?? []
  if (!results.length || results[0].match_type === 'partial') return null
  const exact =
    results.find((r) => r.headword === q) ??
    results.find((r) => r.forms.some((f: any) => f.form === q && f.is_primary)) ??
    results[0]
  const entry = await api('/entry/' + exact.lexeme_id)
  return { hits: results, entry }
}

const strip = (s: string) => s.replace(/\s+/g, ' ').trim()
// glyph text of a .dform with the TC/SC/JP tag letters and separators removed
const formGlyphs = (s: string) => s.replace(/[A-Za-z·\s]/g, '')

type RowSum = {
  variety: string
  forms: string[]
  readings: string[]
  tags: string[]
  glosses: string[]
}
type ViewSum = {
  head: string
  rows: RowSum[]
  note: boolean
  cnote: boolean
  tabs: string[]
  junk: string[]
}

function extract(anchor: string): ViewSum {
  const rows: RowSum[] = []
  for (const dl of Array.from(document.querySelectorAll('.defs .dl'))) {
    rows.push({
      variety: strip(dl.querySelector('.dvar:not(.dcanto)')?.textContent ?? ''),
      forms: Array.from(dl.querySelectorAll('.dlh .dform')).map((e) => formGlyphs(e.textContent ?? '')),
      readings: Array.from(dl.querySelectorAll('.dread')).map((e) => strip(e.textContent ?? '')),
      tags: Array.from(dl.querySelectorAll('.rtagline .ltag')).map((e) => strip(e.textContent ?? '')),
      glosses: Array.from(dl.querySelectorAll('.senses li')).map((e) => strip(e.textContent ?? '')).slice(0, 6),
    })
  }
  const body = document.body.textContent ?? ''
  const junk: string[] = []
  for (const j of ['undefined', '[object Object]', 'null,', '??']) if (body.includes(j)) junk.push(j)
  if (/\bNaN\b/.test(body)) junk.push('NaN')
  return {
    head: strip(document.querySelector('.glyph')?.textContent ?? ''),
    rows,
    note: !!document.querySelector('.note'),
    cnote: !!document.querySelector('.cnote'),
    tabs: Array.from(document.querySelectorAll('.segb')).map((e) => strip(e.textContent ?? '')),
    junk,
  }
}

async function renderView(q: string): Promise<ViewSum | null> {
  const v = await fetchView(q)
  if (!v) return null
  render(Unified, { props: { hits: v.hits, entry: v.entry, anchor: q, onsearch: () => {} } })
  // flush microtasks so $derived/$effect settle
  await new Promise((r) => setTimeout(r, 0))
  const sum = extract(q)
  cleanup()
  document.body.innerHTML = ''
  return sum
}

// compare the two skins' rendered semantics; anchor-dependent differences are whitelisted:
// each row's FORM inventory is compared as (shown forms ∪ {anchor}) since form===head renders nothing
function comparePair(kind: string, a: string, b: string, va: ViewSum, vb: ViewSum) {
  const id = `${a}/${b}`
  if (va.junk.length) fin({ check: 'junk-text', kind, pair: id, view: a, junk: va.junk })
  if (vb.junk.length) fin({ check: 'junk-text', kind, pair: id, view: b, junk: vb.junk })
  const seqA = va.rows.map((r) => r.variety).join(',')
  const seqB = vb.rows.map((r) => r.variety).join(',')
  if (seqA !== seqB) {
    fin({ check: 'row-seq-differ', kind, pair: id, a: seqA, b: seqB, aRows: va.rows, bRows: vb.rows })
    return
  }
  for (let i = 0; i < va.rows.length; i++) {
    const ra = va.rows[i]
    const rb = vb.rows[i]
    if (ra.tags.join('|') !== rb.tags.join('|'))
      fin({ check: 'tags-differ', kind, pair: id, variety: ra.variety, a: ra.tags, b: rb.tags })
    if (ra.readings.join('|') !== rb.readings.join('|'))
      fin({ check: 'readings-differ', kind, pair: id, variety: ra.variety, a: ra.readings, b: rb.readings })
    if (ra.glosses.join('|') !== rb.glosses.join('|'))
      fin({ check: 'glosses-differ', kind, pair: id, variety: ra.variety, a: ra.glosses, b: rb.glosses })
    const fa = new Set([...ra.forms.flatMap((f) => [...f]), ...(ra.forms.length ? [] : [...a])])
    const fb = new Set([...rb.forms.flatMap((f) => [...f]), ...(rb.forms.length ? [] : [...b])])
    // a row that renders NO form means "form === the page's head"; resolve it to the anchor
    const setEq = fa.size === fb.size && [...fa].every((x) => fb.has(x))
    if (!setEq && ra.variety === vb.rows[i].variety && ra.variety === '日')
      fin({ check: 'ja-form-differ', kind, pair: id, a: [...fa], b: [...fb] })
  }
  if (va.note !== vb.note) fin({ check: 'falsefriend-differ', kind, pair: id, a: va.note, b: vb.note })
  if (va.cnote !== vb.cnote) fin({ check: 'cnote-differ', kind, pair: id, a: va.cnote, b: vb.cnote })
  if (va.tabs.join('|') !== vb.tabs.join('|'))
    fin({ check: 'tabs-differ', kind, pair: id, a: va.tabs, b: vb.tabs })
}

const jaStandalone: Record<string, number> = JSON.parse(fs.readFileSync(`${DIR}/ja_standalone.json`, 'utf8'))

function boundTruth(kind: string, q: string, v: ViewSum) {
  for (const r of v.rows) {
    if (r.variety !== '日') continue
    const tag = r.tags.find((t) => t.includes('in compounds'))
    if (!tag) continue
    const jaForm = r.forms.length ? r.forms[0] : v.head
    if (jaForm.length === 1 && jaStandalone[jaForm] === 0)
      fin({ check: 'bound-tag-vs-db', kind, q, jaForm, tag, note: 'standalone ja word exists in DB' })
  }
}

test('cross-skin consistency sweep', { timeout: 1000 * 60 * 60 * 3 }, async () => {
  const charPairs = JSON.parse(fs.readFileSync(`${DIR}/pairs_chars.json`, 'utf8'))
  const wordPairs = JSON.parse(fs.readFileSync(`${DIR}/pairs_words.json`, 'utf8'))
  const mine = <T,>(list: T[]) => list.filter((_, i) => i % TOTAL === SHARD)
  let n = 0
  const work: { kind: string; a: string; b: string }[] = [
    ...mine(charPairs).map((p: any) => ({ kind: 'char:' + p.type, a: p.a, b: p.b })),
    ...mine(wordPairs).map((p: any) => ({ kind: 'word', a: p.a, b: p.b })),
  ]
  for (const w of work) {
    const key = `${w.kind}:${w.a}/${w.b}`
    if (done.has(key)) {
      n++
      continue
    }
    try {
      const va = await renderView(w.a)
      const vb = await renderView(w.b)
      if (!va || !vb) {
        // one skin resolves to a word card, the other doesn't = itself a finding
        if (!!va !== !!vb) fin({ check: 'card-only-one-skin', ...w, aCard: !!va, bCard: !!vb })
      } else {
        comparePair(w.kind, w.a, w.b, va, vb)
        boundTruth(w.kind, w.a, va)
        boundTruth(w.kind, w.b, vb)
      }
    } catch (e) {
      const cause = (e as any)?.cause
      fin({ check: 'harness-error', ...w, err: String(e) + (cause ? ` / ${String(cause)}` : '') })
    }
    fs.appendFileSync(DONE_PATH, key + '\n')
    if (++n % 200 === 0) console.log(`shard ${SHARD}: ${n}/${work.length} (${findingCount} findings)`)
  }
  if (latBuf.length) fs.appendFileSync(LAT_PATH, latBuf.join('\n') + '\n')
  console.log(`shard ${SHARD} done: ${n} pairs, ${findingCount} findings`)
  expect(n).toBeGreaterThan(0)
})
