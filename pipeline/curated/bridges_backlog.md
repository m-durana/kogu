# Cross-language bridges — backlog & decisions

Status as of 2026-06-16. Companion to `bridges_assessment.md` (the original audit).

## Where bridges come from today (shipped)

The "written differently" bridge band + the `translations`/`same_form` entry fields are fed by:

1. **`same_form`** ← variant graph (`glyph_edge`): cross-script siblings (氣/気). Reliable.
2. **`concept` / `sense_concept`** (gloss-pivot): fuzzy synonyms. Broad but noisy; capped to 1 per
   gap language in the UI and made to sit *below* explicit equivalents.
3. **`lexeme_equivalent`** (NEW, the trusted layer): explicit edges from
   - CC-Canto inline "Mandarin equivalent" notes (14),
   - curated `equivalents_yue_zh.tsv` (粵→中 colloquial, ~42 resolved),
   - curated `bridges_crosslang.tsv` (zh/yue/ja, ~124 edges).
   Loaded by `ingest/equivalents.py`; live-DB refresh via `python -m kogupipe.apply_equivalents`.

## Decision: wiktextract ingest is DEFERRED (not necessary now)

Reasoning: the **core value (co-equal definitions) is already complete** from search hits; bridges are
the secondary layer, and the curated+inline set already covers the high-value everyday vocabulary
(airport, bicycle, phone, fridge, common 粵字). Wiktextract mainly buys the long tail at the cost of
auto-generated precision risk + ongoing maintenance (deprecated URLs, 2.6 GB dumps). Revisit only if
"this should bridge but doesn't" becomes a recurring complaint. Full scope preserved below so we don't
re-research.

## Better next step IF we push bridges further: mint `yue` lexemes for shared-char colloquial Cantonese

The one real gap found: everyday Cantonese written with **shared** Han chars — 食 eat, 飲 drink, 睇 look,
講 say, 行 walk, 係 be, 凍/平/細/返/仲/呢/識 … (~25 in `equivalents_yue_zh.tsv` that don't resolve).
They exist only as `zh` lexemes, so there's no 粵 side to attach a bridge to. The retag
(`ingest/cantonese.py`) only minted `yue` lexemes for *exclusively*-Cantonese 粵字 (冇/嘅/喺).
Fix = mint `yue` lexemes (jyutping from `char_reading`, Cantonese sense) for high-frequency colloquial
uses of shared chars. This is smaller/lower-risk than wiktextract AND is a **prerequisite** for any
future Cantonese bridge source (wiktextract included) to resolve.

## Wiktextract scope (shelved — for when we build it)

Mine the **English edition** of Wiktionary via kaikki.org (English glosses align with our `sense.gloss_en`).
Two bridge surfaces:

- **(A) lect-tagged `synonyms` in Chinese entries → zh↔yue.** Cantonese = `tags:["Cantonese"]` on a
  synonym (not a separate `lang_code`), with an English `sense` label. e.g. 車 → `汽車 {tags:[Cantonese]}`,
  `聽日 {tags:[Cantonese]}` (=明日). Biggest volume of colloquial-Cantonese bridges.
- **(B) sense-labelled `translations` in English entries → zh/yue/ja cross-lang.** e.g. *refrigerator* →
  zh 冰箱 / yue 雪櫃 / ja 冷蔵庫 under sense "appliance". Cleanest; auto-reproduces `bridges_crosslang.tsv`.
  Caveat: `word` bundles form+romanization (`"雪櫃 /雪柜 (syut³ gwai⁶)"`) → parse out Han, split trad/simp.

**Files (kaikki.org, English edition, verified live 2026-06-16; per-language URLs are DEPRECATED but live):**
- Chinese section ~1.17 GB: `https://kaikki.org/dictionary/Chinese/kaikki.org-dictionary-Chinese.jsonl`
- Japanese section ~372 MB: `https://kaikki.org/dictionary/Japanese/kaikki.org-dictionary-Japanese.jsonl`
- English section ~2.9 GB (has the `translations` tables): `https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl`
- Fallback (non-deprecated): raw dump `https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz`
  (~2.6 GB gz), filter `lang_code in {zh, ja, en}`. NOTE: `downloads/{zh,ja}-extract.jsonl.gz` are the
  *native-language* editions (Chinese/Japanese glosses) — NOT a substitute.
- License: **CC BY-SA** (+ GFDL) — compatible with our sources; needs Wiktionary/kaikki attribution.

**Design:** mirror `extract_etymology.py` (stream-filter over HTTP, keep only forms we have) → emit
compact `sources/wikt.{syn,xlang}.jsonl` → fold two new sources into `ingest/equivalents.py`
(`source` = `wiktextract-syn` / `wiktextract-xlang`, `INSERT OR IGNORE`, curated stays authoritative).
Reuse `_resolver` / `_first_form`. Same guards already in place: skip-and-count unresolved, no
same-form edges, lect whitelist (Cantonese→yue, Mandarin/written-vernacular→zh; drop Hokkien/Min/Wu/
Hakka/literary), sense-gating via `sense` / `_dis1`.

**Validate:** the curated TSVs are the gold set (recall/precision); dry-run on a DB copy; spot-check
雪櫃↔冰箱, 聽日↔明日, 空港↔機場, 冷蔵庫↔冰箱.

**Effort ~1–2 days. Phases:** 1) translations (cleanest, do first) → 2) Cantonese synonyms (needs the
yue-lexeme mint above to resolve) → 3) optional descendants/origin + zh-CN/zh-TW region splits (needs a
schema change — one `zh` node can't hold both regions today).

**Open decisions (recommended defaults):** English edition + raw-dump fallback coded from day one;
fold into `equivalents.py` (one table owner); curated authoritative on conflict + add a `confidence`
tier to `lexeme_equivalent`; Cantonese+Mandarin lects only; defer region splits.
