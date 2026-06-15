# kogu ingestion pipeline

Build-time only. Compiles all open upstream sources into **one normalised SQLite database**
(`../data/kogu.sqlite`, with FTS5) that the Rust backend serves. Runs rarely. Language doesn't
matter here (Python/JS allowed) - **none of this is in the serving path**.

All heavy conversions (OpenCC simp/trad/shinjitai, Middle Chinese tagging, variant-graph closure,
concept clustering) are **precomputed here** so the runtime does only index hits.

## Sources (all open - see `../docs/PLAN.md` for licences & exact roles)

| Layer                    | Source                                                     |
|--------------------------|-----------------------------------------------------------|
| Characters/variants/readings | Unihan, cjkvi-variants                                |
| Decomposition (IDS)      | cjkvi-ids                                                  |
| Simp/Trad/shinjitai conv | OpenCC (+ nk2028/opencc-data)                             |
| Chinese words            | CC-CEDICT                                                  |
| Cantonese words/jyutping | CC-Canto, words.hk, rime-cantonese                        |
| Japanese words/kanji     | jmdict-simplified (JMdict/JMnedict/Kanjidic/KRADFILE)     |
| Etymology/translations   | Wiktionary via kaikki.org / wiktextract                   |
| Concepts/synsets         | Open Multilingual Wordnet (COW + wnja)                     |
| Readings across varieties| MCPDict                                                    |
| Middle Chinese/phonology | nk2028 (tshet-uinh, ToMiddleChinese), Guangyun, Baxter–Sagart |
| Char-origin (optional)   | Shuowen Jiezi (cjkvi-dict) - **GPLv2, separate module**   |
| Frequency                | SUBTLEX-CH, JP freq list, Cantonese list                  |

Downloaded sources land in `sources/` (gitignored). The built DB lands in `../data/` (gitignored).

## Layout (to be filled in per phase)

```
pipeline/
├── sources/      Downloaded raw upstream data (gitignored)
├── fetch.py      Download + pin upstream sources
├── build.py      Orchestrate: sources → kogu.sqlite
└── schema.sql    Canonical DB schema (character backbone / lexeme / concept layers)
```

Not yet implemented - Phase 0.1 onward in `../docs/PLAN.md`.
