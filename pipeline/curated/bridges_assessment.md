# Cross-language bridge assessment

Companion to `bridges_crosslang.tsv` (62 curated divergent-vocabulary bridges across
zh-trad / yue-trad / ja). Goal: catalogue everyday concepts where the CJK varieties use
**different words** (not just different glyphs), which an English-gloss-pivot concept layer
tends to miss or merge incorrectly.

## 1. Categories a gloss-pivot system most commonly misses (and why)

A gloss-pivot ("two words are linked iff their English glosses overlap") fails on the
following systematically:

1. **Loanword-vs-Sino divergence (biggest gap).** Japanese very often expresses an everyday
   concept with a katakana foreign loan while Chinese uses a Han compound:
   bread パン / 麵包, beer ビール / 啤酒, taxi タクシー / 計程車, juice ジュース / 果汁,
   door ドア / 門. The pivot *can* catch these if both glosses say "bread", but in practice
   the katakana entry often has a thin or missing English gloss in JMdict, and the systems
   never see them as the same node. These are also exactly the entries Kogu most wants to
   surface, because the writing systems share zero characters.

2. **Wasei-kango / Sino-Japanese reshuffles.** Japan and China independently coined different
   Han compounds for the same thing: airport 空港 / 機場, hospital 病院 / 醫院,
   newspaper 新聞 / 報紙, mobile phone 携帯電話 / 手機, refrigerator 冷蔵庫 / 冰箱,
   airplane 飛行機 / 飛機, post office 郵便局 / 郵局. A pivot misses these whenever the two
   sides' English glosses are phrased differently ("medical institution" vs "hospital"), or
   when one side's word is multi-sense and the dominant gloss is something else.

3. **False friends (same/near-same characters, different meaning).** These are the *most
   dangerous* for a naive pivot because the characters look identical and a glyph-similarity
   heuristic would WRONGLY bridge them, while the gloss pivot correctly separates them — but
   then never records the *real* bridge. Examples: 新聞 (ja newspaper / zh news),
   勉強 (ja study / zh force-reluctantly), 走 (ja/classical & yue "walk-ish" vs zh "walk",
   ja run = 走る), 手紙 (ja letter / zh toilet paper), 先生 (ja teacher / zh Mr.),
   机 (ja desk / zh machine), 貴 (ja noble / zh expensive). The correct bridge for "letter"
   is 信 ↔ 手紙, which a pivot rarely finds.

4. **Native wago vs Sino-Chinese.** When Japanese uses a kun'yomi native word the English
   gloss often still matches, but the lemma forms diverge totally and frequency/register
   differ: vegetable 野菜 / 蔬菜, fruit 果物 / 水果, money お金 / 錢, dog 犬 / 狗,
   shoes 靴 / 鞋, look 見る / 看, run 走る / 跑, delicious 美味しい / 好吃.

5. **Cantonese colloquial divergence from written/Mandarin.** Cantonese everyday speech uses
   distinct lexemes: refrigerator 雪櫃, umbrella 遮, key 鎖匙, soap 番梘, taxi 的士,
   bicycle 單車, eat 食, drink 飲, sleep 瞓, knee 膝頭哥. A pivot built on CC-CEDICT/standard
   written Chinese under-represents these because many appear only in CC-Canto / words.hk /
   rime-cantonese, not in the Mandarin-centric gloss layer.

6. **Register / regional splits within one variety** (zh-CN vs zh-TW): taxi 出租車 vs 計程車,
   bus 公共汽車 vs 公車, bicycle 自行車 vs 腳踏車. A single "zh" concept node hides these.

7. **Honorific/orthographic-prefix forms** (お金, ご飯, お茶) — the bare gloss is "money/rice/
   tea" so a pivot may link to the Han 錢/飯/茶 only sometimes, and the as-written normal
   Japanese form (with お/ご) is lost.

The common thread: gloss-pivot precision is hostage to **how rich, consistent, and
disambiguated the English glosses are on both sides**. JMdict glosses are good; CC-CEDICT
glosses are terse and often list many senses; Cantonese sources are sparse. So the pivot is
strongest on rare technical terms (well-glossed, one sense) and *weakest exactly on common
everyday words* (many senses, terse glosses, loanwords, colloquialisms) — the opposite of
what a learner-facing dictionary needs.

## 2. Authoritative datasets/sources to ingest for systematic coverage

Ranked by value for cross-language bridges specifically:

1. **Wiktionary (kaikki.org / wiktextract dump)** — *highest value, and Kogu already pulls a
   slice of it for etymology.* Each lemma is tagged by language AND lects (e.g.
   `Cantonese`, `Hokkien`, `Taiwanese Mandarin`), and crucially carries **per-sense
   "Synonyms" and "Descendants/Translations" tables that explicitly name the equivalent in
   other varieties** (the 飲/喝, 雪櫃/冰箱, 計程車/的士/出租車 tables that powered most of
   this TSV came straight from there). Mining the structured `synonyms`, `translations`, and
   `related` fields in the wiktextract JSON gives variety-tagged, sense-aligned equivalence
   sets far more reliable than an English pivot. License: CC BY-SA 3.0 / GFDL (attribution +
   share-alike). URL: https://kaikki.org/dictionary/ (per-language JSONL),
   source https://github.com/tatuylonen/wiktextract.

2. **CC-Canto + words.hk + rime-cantonese** (already in the source list) — essential to close
   the Cantonese-colloquial gap (食/飲/瞓/雪櫃/番梘/遮/鎖匙). words.hk is the most
   authoritative modern Cantonese lexicon. Licenses: CC-Canto CC BY-SA;
   words.hk has a non-commercial-ish custom license (verify before redistribution —
   https://words.hk/base/hoifong/); rime-cantonese CC BY 4.0
   (https://github.com/rime/rime-cantonese).

3. **JMdict (jmdict-simplified)** (already used) — the katakana-loanword layer (パン, ビール,
   タクシー, ドア) lives here; ensure katakana entries are NOT dropped and that their English
   glosses are used for pivoting. License: JMdict is CC BY-SA 4.0 (EDRDG);
   jmdict-simplified MIT-wraps the data — https://github.com/scriptin/jmdict-simplified.

4. **CJKV "false friends" / divergent-vocab curated lists** — for the dangerous same-glyph /
   different-meaning cases that no automated overlap will catch correctly:
   - sci.lang.japan FAQ, Chinese-Japanese false friends:
     https://www.sljfaq.org/afaq/cj-false-friends.html
   - Wikipedia "Wasei-kango": https://en.wikipedia.org/wiki/Wasei-kango (and
     "Sino-Japanese vocabulary" https://en.wikipedia.org/wiki/Sino-Japanese_vocabulary).
   These are small enough to ingest as a curated seed list (like this TSV) rather than parse.

5. **Open Multilingual Wordnet (wnja / COW)** (already used) — keep, but treat as *recall*
   not precision: it is itself essentially the gloss/synset pivot, so it should be the source
   the bridge layer is *augmenting*, not the authority.

6. **CLICS / Concepticon (cross-linguistic colexification)** — research-grade concept set
   with stable concept IDs; useful as a controlled `concept_en` vocabulary so bridges anchor
   to a canonical concept rather than free-text English. https://concepticon.clld.org/
   (CC BY 4.0). Lower priority — infrastructure, not data.

### Single highest-value source to ingest next
**Wiktionary via wiktextract (kaikki.org), mining the variety-tagged `synonyms` and
`translations` fields.** It uniquely provides sense-level, language/lect-labelled equivalence
sets for exactly the everyday concepts the gloss pivot drops, it covers all three varieties
(including Cantonese lects) in one dataset, Kogu already has wiktextract plumbing, and the
license (CC BY-SA) is compatible with the rest of the corpus.

## 3. Confidence and caveats

- **62 rows: 43 high / 19 medium.** "high" = the divergence is the normal, current,
  well-attested way each listed variety expresses the concept, cross-checked on Wiktionary or
  a standard reference. "medium" = the divergence is real but softer: the loan/native form
  competes with a Sino synonym (e.g. ja レストラン vs 食堂; お金 vs 金), or the bridge is
  register/colloquial (yue 較 for lift, 嘆 for "enjoy").
- **Empty cells are deliberate.** A blank `yue` usually means Cantonese shares the Chinese
  written form (e.g. it also writes 機場); a blank means "no *distinct* lexeme worth a
  separate bridge", NOT "unknown". This keeps the file a list of genuine divergences.
- **Glyph-only differences were excluded.** Pairs like 学校/學校, 経済/經濟, 天気/天氣,
  猫/貓, 来る/來 differ only by shinjitai vs traditional glyph of the *same* word — these are
  a character-variant relation Kogu should handle in its variant-graph layer, not as a
  cross-language vocabulary bridge, so they are not rows here.
- **Japanese forms are given as normally written** (kana/okurigana/honorific included:
  走る, 見る, ご飯, お金, 歯ブラシ). Chinese/Cantonese are traditional Han.
- **zh-CN vs zh-TW collapse.** The single `zh_trad` column can't hold both regional Mandarin
  variants; where they differ (taxi, bus, bicycle) the most representative form is in the
  cell and the alternative is noted in `source`. A production schema should split these.
- **Verification depth varies.** ~15 entries were directly fetched and confirmed on
  Wiktionary during this pass (空港, 手提電話, 雪櫃, 計程車, 便當, 飲, 番梘, 鎖匙, 遮, 嘆,
  膝頭哥, etc.); the remainder are standard, widely-documented divergences asserted from the
  Wasei-kango / Sino-Japanese-vocabulary / CJK-false-friend literature. Before ingestion,
  every row should be machine-checked against the wiktextract per-lemma language tags to
  confirm each form is attested for its labelled variety.
- This is a **seed/curation list**, not a complete inventory. It is meant to (a) immediately
  add high-value bridges the pivot misses and (b) serve as a gold set to evaluate the
  automated Wiktionary-synonyms extraction proposed in section 2.
