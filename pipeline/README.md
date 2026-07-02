# Kogu ingestion pipeline

Build-time only. Compiles the open upstream sources into **one normalised SQLite database**
(`../data/kogu.sqlite`, with FTS5) that the Rust backend serves. Runs rarely. Language doesn't
matter here (Python/JS allowed): **none of this is in the serving path**.

All heavy conversions (OpenCC simp/trad/shinjitai, Middle Chinese tagging, variant-graph closure,
concept clustering) are **precomputed here** so the runtime does only index hits.

## Sources (licences + attribution in `../NOTICE.md`)

| Layer                         | Source                                                        |
|-------------------------------|---------------------------------------------------------------|
| Characters/variants/readings  | Unihan (incl. kSpoofingVariant confusables)                   |
| Decomposition (IDS)           | cjkvi-ids                                                     |
| Simp/Trad/shinjitai conversion| OpenCC tables (ST/TS/TW/HK/JPShinjitai)                       |
| Chinese words                 | CC-CEDICT                                                     |
| Cantonese words/jyutping      | CC-Canto + CC-CEDICT Cantonese readings (two files)           |
| Japanese words/kanji          | JMdict + KANJIDIC via jmdict-simplified                       |
| Japanese pitch accent         | Kanjium `accents.txt`                                         |
| Etymology/components/bridges  | Wiktionary via kaikki.org dumps (streamed, never stored raw)  |
| Concepts/synsets              | Open Multilingual Wordnet (`wn` package: omw-ja, omw-cmn)     |
| Middle Chinese (廣韻/Baxter)   | nk2028 tshet-uinh (CC0), via `scripts/gen_mc.mjs`             |
| Frequency                     | wordfreq (primary); hermitdave FrequencyWords 50k (fallback)  |

Downloaded sources land in `sources/` (gitignored). The built DB lands in `../data/` (gitignored).

## Layout

```
pipeline/
├── kogupipe/           The package: fetch, build, ingest steps, extractors, apply_* refreshers
│   └── ingest/         One module per ingest step (see INGEST_STEPS in ingest/__init__.py)
├── scripts/            gen_mc.mjs (Middle Chinese), fetch_translations.py (kaikki stream)
├── curated/            Hand-maintained equivalence/override data
├── schema.sql          Canonical DB schema
├── sources.lock.json   Pinned URL + sha256 + size per fetched source
├── refresh_*.py        Standalone live-DB refreshers (see below)
└── tests/              pytest suite
```

## Build order

Everything below runs from `pipeline/` with the venv
(`python3 -m venv .venv && .venv/bin/pip install -r requirements.txt`).

**1. Fetch the pinned sources**

```
.venv/bin/python -m kogupipe.fetch          # all; or a subset: fetch unihan cedict
```

Downloads into `sources/` and records URL + sha256 + size in `sources.lock.json`.
Caveat: the lockfile currently covers only the directly-downloaded files. The derived inputs
below (kaikki extracts, `char_mc.json`, `wiktionary_translations.tsv`, the vendored OMW LMF
files) are produced by their own scripts and are not yet pinned there.

**2. Generate the side-channel inputs** (independent of the DB, any order):

- **Middle Chinese**: `node scripts/gen_mc.mjs` writes `sources/char_mc.json` from the
  tshet-uinh npm packages. Install them first with `npm --prefix "$MC_PREFIX" install ...`
  (exact command in the script header). `MC_PREFIX` defaults to a server-local path; override
  it via the environment.
- **Translation bridges**: `.venv/bin/python scripts/fetch_translations.py` streams the kaikki
  English dump over HTTP and writes only the compact zh/yue/ja TSV
  (`sources/wiktionary_translations.tsv`); the ~20 GB dump never lands on disk.
- **Component roles**: `.venv/bin/python -m kogupipe.extract_components` streams the kaikki
  zh/ja dumps for the structured `Han compound` templates, writing `sources/components.jsonl`.
- **Wordnet**: the OMW LMF files (`sources/wn/omw-ja.xml.xz`, `omw-cmn.xml.xz`) are vendored
  for offline builds; alternatively `wn.download()` them via the `wn` package.

**3. Build**

```
.venv/bin/python -m kogupipe.build          # or --out /path/to.sqlite
```

Runs the ingest steps in order (`kogupipe/ingest/__init__.py::INGEST_STEPS`): character
backbone (Unihan + cjkvi-ids + OpenCC), confusables, component roles, Middle Chinese readings,
lexemes (CC-CEDICT + JMdict + Kanjidic), Cantonese, concept layer (gloss pivot, then OMW),
equivalence edges (inline + curated), Wiktionary translation bridges, etymology + origin
badges, JMdict loanword origins, word frequency, romaji index, pitch accents. Then it rebuilds
FTS and **fails loudly** if the build-time invariants (DESIGN.md) are violated. Steps whose
side inputs are missing skip with a note, so a first build works before step 2/4 have run.

**4. Etymology extraction (needs a built DB, then re-apply)**

`.venv/bin/python -m kogupipe.extract_etymology` streams the kaikki zh/ja dumps and keeps only
entries whose surface form is **already in the DB**, writing `sources/etymology.{zh,ja}.jsonl`.
Bootstrapping is therefore two-pass: build once, extract, then either rebuild or run
`kogupipe.apply_translations`-style patching (here: rebuild, or a fresh `kogupipe.build` picks
the extracts up in step 3).

**5. Live-DB refreshers (optional, in-place)**

A fresh full build already includes everything above. The `apply_*` modules
(`kogupipe.apply_accents`, `apply_cantonese`, `apply_components`, `apply_confusables`,
`apply_equivalents`, `apply_frequency`, `apply_gloss_clean`, `apply_kanji_ja`,
`apply_loanwords`, `apply_mc`, `apply_translations`) and the standalone
`refresh_char_readings.py`, `refresh_concepts.py`, `refresh_omw.py`, `migrate_canto_senses.py`
patch an **existing** DB idempotently without a full rebuild; they exist because the live DB
predates several ingest fixes. Each module's docstring states what it needs and whether a
service restart is required afterwards. They are independent of each other except that
`refresh_concepts.py` should run before `refresh_omw.py` (it rebuilds the gloss-pivot concepts
that OMW concepts sit alongside); beyond that, no strict ordering is documented, so when in
doubt run the one you need and check `kogupipe.qa_audit`.

**6. QA**

`.venv/bin/python -m kogupipe.qa_audit` flags entries a human would instantly see as wrong
(DB-only heuristics). `pytest` runs the pipeline test suite.

## Server-specific defaults

Paths that default to this VPS's layout are all env-overridable: `KOGU_DB` (DB path for every
apply/refresh script), `MC_PREFIX` (npm prefix for `gen_mc.mjs`), and the TTS sidecar's venv +
cache paths (see `../tts/README.md` and `../tts/kogu-tts.service`).
