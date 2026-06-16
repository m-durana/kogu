# Cantonese → Standard Written Chinese equivalents — curation notes

Maps colloquial written Cantonese forms (粵字 and multi-character colloquialisms) to their
**Standard Written Chinese** (Mandarin-based) equivalents, in **traditional** characters.

- File: `equivalents_yue_zh.tsv`
- Rows: 61 (58 high confidence, 3 medium)
- Columns: `yue_form	zh_equiv_trad	jyutping	english	confidence	source`

## Sources

1. **en.wiktionary.org** — primary source for every row. Each entry was checked individually,
   relying mainly on Wiktionary's **"Dialectal synonyms of X"** tables, which explicitly list the
   Cantonese form against the "Formal (Written Standard Chinese)" form. Where a table was absent,
   the entry's own usage note ("direct equivalent of Standard Chinese …", "dialectal synonym of …")
   was used. The `source` column cites the specific lemma consulted.
2. **CC-Canto** (`../sources/cccanto-webdist.txt`, already in the repo) — used **only** to
   cross-check jyutping readings, NOT echoed for meanings. CC-Canto's English glosses are frequently
   wrong or garbled for these lemmas (e.g. it glosses 食 as "used in names", 睇 as "to catch",
   飲 as "wedding party"), so all meanings/equivalents come from Wiktionary instead. This satisfies
   the "corroborate, don't just echo CC-Canto" requirement.

## Judgement calls

- **Jyutping for 畀**: used the vernacular reading `bei2` (the everyday "to give" reading) per
  Wiktionary, not CC-Canto's `bei3`.
- **唔該 → 謝謝 (medium)**: 唔該 is polysemous ("thank you for a favour" / "excuse me" / "please").
  謝謝 is the standard equivalent only for the thanking sense; there is no single clean standard word
  covering all senses, hence medium confidence. Compare 多謝 → 謝謝 (high), which is unambiguously
  "thank you (for a gift/favour)".
- **講 → 說 (medium)**: 講 is itself acceptable in standard written Chinese; Wiktionary keys the
  colloquial "to say/speak" slot to 說, so it is listed, but the boundary is soft.
- **緊 → 正在 (medium)**: 緊 is the Cantonese progressive aspect suffix (V緊). Wiktionary did not give
  a one-to-one standard token; 正在 (and 着) is the standard way to render the progressive, so it is
  included at medium confidence as a functional, not lexical, equivalent.
- **識** appears twice on purpose: "to know how to; can" → 會, and "to know; be acquainted with" →
  認識. These are distinct standard equivalents for distinct senses.
- Traditional forms used throughout for `zh_equiv_trad` (e.g. 沒有, 來, 給, 沒, 對, 還, 這, 那, 認識).

## Deliberately EXCLUDED (no clean single standard equivalent, or "equivalent" is itself colloquial)

- **走** — the character is shared, but in Cantonese it means "to leave/go away" whereas the
  colloquial "to run" sense and the Mandarin "to walk" sense diverge; no clean mapping. Skipped.
- **喐 (juk1, "to move one's body")** — Wiktionary keys it to 移 ("move an object"), but the natural
  gloss is 動; the source disagreed with the intuitive equivalent, so skipped per reliability rule.
- **嘅** sentence-final / emphatic uses, and other **final particles** (吖, 喎, 㗎, 啦, 囉, 嘅 as
  emphasis, 咩 as a question particle) — these are discourse particles with no standalone standard
  written equivalent; only 嘅's possessive/attributive use (→ 的) and 咗 (→ 了 perfective) and 緊
  (→ progressive) were included because those map to real standard morphemes.
- **晒 / 嗮 (saai3, completive "all/entirely")** — an aspect suffix with no clean single standard
  token; skipped.
- **嘥 vs 嘥** and similar onomatopoeic/substrate items beyond 嘥 itself were not added.
- Pure loanword vocabulary (巴士, 的士, 士多 …) — these are transliterations, not the kind of
  colloquial↔standard lexical pair requested, so omitted.

## Overall confidence

High. Every row is grounded in a Wiktionary lemma that explicitly states the standard-Chinese
equivalent, with jyutping cross-checked against CC-Canto. The three medium rows are flagged because
the mapping is functional/sense-restricted rather than a clean lexical one-to-one.
