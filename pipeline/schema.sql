-- kogu canonical database schema
-- Three layers over one character backbone (DESIGN.md §2). Built offline by build.py,
-- served read-only by the Rust backend. All heavy conversion is precomputed here.
--
-- Conventions:
--   * Han characters are stored as their UTF-8 text in `char` columns AND as the integer
--     Unicode codepoint in `cp` columns (codepoint is the stable join key; text is for display).
--   * `variety` ∈ ('zh','yue','ja')  -- Mandarin/written-Chinese, Cantonese, Japanese.
--   * `region` is a free-ish tag from the controlled set in `region` table (core four at launch).
--   * `script` ∈ ('trad','simp','shinjitai','kana','mixed','other').

PRAGMA foreign_keys = ON;

-- ============================================================================
-- 0. Reference / controlled vocabularies
-- ============================================================================

-- Regions are a first-class tag (DESIGN.md §0). Launch set = core four; more rows added later
-- without re-architecting. `folds_into` lets Macau→HK, SG/MY→mainland later.
CREATE TABLE region (
    code        TEXT PRIMARY KEY,         -- 'CN','TW','HK','JP', later 'MO','SG','MY'
    name        TEXT NOT NULL,
    script      TEXT NOT NULL,            -- default script for the region
    folds_into  TEXT REFERENCES region(code),
    launch      INTEGER NOT NULL DEFAULT 0  -- 1 = in the core-four launch set
);

-- Reform events that produced glyph edges (the "which reform / when" for the orthographic why).
CREATE TABLE reform (
    id          TEXT PRIMARY KEY,         -- 'prc-1956','prc-1964','jp-toyo','jp-joyo','hk-std','tw-std','unihan-variant'
    name        TEXT NOT NULL,
    year        INTEGER,
    note        TEXT
);

-- ============================================================================
-- 1. Character backbone (DESIGN.md §2.1)
-- ============================================================================

-- Every character node (orthodox or living glyph). The orthodox set (traditional / kyūjitai)
-- is the canonical key; living glyphs carry edges to their orthodox parent.
CREATE TABLE character (
    cp           INTEGER PRIMARY KEY,     -- Unicode codepoint
    char         TEXT NOT NULL UNIQUE,    -- the glyph itself (UTF-8)
    is_orthodox  INTEGER NOT NULL DEFAULT 0,
    strokes      INTEGER,                 -- total stroke count (kTotalStrokes / kanjidic)
    radical      INTEGER,                 -- Kangxi radical number
    ids          TEXT,                    -- Ideographic Description Sequence (cjkvi-ids), may hold multiple
    gloss_en     TEXT,                    -- short Unihan English gloss (kDefinition, Chinese-centric)
    gloss_ja     TEXT                     -- Kanjidic English meaning (Japanese perspective: 津 → "haven; port; harbor")
);
CREATE INDEX idx_character_orthodox ON character(is_orthodox);

-- Per-character readings across varieties.
CREATE TABLE char_reading (
    cp      INTEGER NOT NULL REFERENCES character(cp),
    kind    TEXT NOT NULL,               -- 'pinyin','jyutping','onyomi','kunyomi','zhuyin','mc'(middle chinese, phase 4)
    value   TEXT NOT NULL,
    region  TEXT REFERENCES region(code),-- optional region-specific reading
    ord     INTEGER NOT NULL DEFAULT 0,  -- position within (cp,kind); 0 = the customary reading first
    PRIMARY KEY (cp, kind, value)
) WITHOUT ROWID;
CREATE INDEX idx_char_reading_value ON char_reading(kind, value);

-- Phono-semantic composition: which component carries the MEANING vs the SOUND (媽 = 女 semantic +
-- 馬 phonetic). From Wiktionary's structured `Han compound` template (extract_components.py); used to
-- badge the structure section. Distinct from the IDS decomposition (which has no role information).
CREATE TABLE char_component (
    cp        INTEGER NOT NULL REFERENCES character(cp),
    ord       INTEGER NOT NULL,            -- order within the character (left→right / top→bottom)
    component TEXT NOT NULL,               -- the component glyph
    role      TEXT,                        -- 'semantic' | 'phonetic' | 'form' | 'iconic' | NULL
    gloss     TEXT,                        -- optional component meaning from the template
    PRIMARY KEY (cp, ord)
) WITHOUT ROWID;

-- The variant graph. Directed edge: child glyph -> orthodox parent.
-- type ∈ ('simplification','shinjitai','z-variant','semantic-variant','region-standard').
-- identity-class types (simplification, shinjitai, z-variant) auto-expand at query time with a
-- bounded transitive closure; 'semantic-variant' is suggestion-only (never silent expansion).
CREATE TABLE glyph_edge (
    child_cp    INTEGER NOT NULL REFERENCES character(cp),
    parent_cp   INTEGER NOT NULL REFERENCES character(cp),
    type        TEXT NOT NULL,
    reform_id   TEXT REFERENCES reform(id),
    confidence  REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (child_cp, parent_cp, type)
) WITHOUT ROWID;
CREATE INDEX idx_glyph_edge_child  ON glyph_edge(child_cp);
CREATE INDEX idx_glyph_edge_parent ON glyph_edge(parent_cp);
CREATE INDEX idx_glyph_edge_type   ON glyph_edge(type);

-- Confusable look-alikes (Unihan kSpoofingVariant): homoglyphs that are easily MISREAD for each other
-- (日/曰, 未/末, 土/士). This is a visual-confusability signal ONLY — not identity or shared meaning —
-- so it is deliberately kept OUT of glyph_edge and the variant graph, and surfaced as a quiet note.
-- kSpoofingVariant is symmetric; both directions are stored.
CREATE TABLE char_confusable (
    cp             INTEGER NOT NULL REFERENCES character(cp),
    confusable_cp  INTEGER NOT NULL REFERENCES character(cp),
    PRIMARY KEY (cp, confusable_cp)
) WITHOUT ROWID;
CREATE INDEX idx_char_confusable_cp ON char_confusable(cp);

-- ============================================================================
-- 2. Lexeme layer (DESIGN.md §2.2)
-- ============================================================================

-- A word in one variety. Chinese = one lexeme with two skins (trad/simp surface forms,
-- one Mandarin reading). Japanese = separate lexemes (never merged into Chinese).
CREATE TABLE lexeme (
    id          INTEGER PRIMARY KEY,
    variety     TEXT NOT NULL,           -- 'zh' | 'yue' | 'ja'
    headword    TEXT NOT NULL,           -- canonical display form (script chosen per variety)
    reading     TEXT,                    -- primary reading (pinyin / jyutping / kana)
    freq        REAL,                    -- corpus frequency (normalised); higher = commoner
    freq_source TEXT
);
CREATE INDEX idx_lexeme_variety  ON lexeme(variety);
CREATE INDEX idx_lexeme_headword ON lexeme(headword);

-- Every surface form of a lexeme, region- and script-tagged. The many-to-one merges
-- (发 ← 髮 and 發) are represented as several trad forms sharing one simp form here.
CREATE TABLE surface_form (
    id          INTEGER PRIMARY KEY,
    lexeme_id   INTEGER NOT NULL REFERENCES lexeme(id),
    form        TEXT NOT NULL,
    script      TEXT NOT NULL,           -- 'trad','simp','shinjitai','kana','mixed','other'
    region      TEXT REFERENCES region(code),
    is_primary  INTEGER NOT NULL DEFAULT 0,
    rare        INTEGER NOT NULL DEFAULT 0  -- JMdict rK/iK/oK/sK: searchable, but not shown as a normal form
);
CREATE INDEX idx_surface_form_lexeme ON surface_form(lexeme_id);
CREATE INDEX idx_surface_form_form   ON surface_form(form);

-- Additional readings (ambiguous / regional / kun vs on at the word level).
CREATE TABLE lexeme_reading (
    lexeme_id   INTEGER NOT NULL REFERENCES lexeme(id),
    kind        TEXT NOT NULL,           -- 'pinyin','jyutping','kana','romaji','zhuyin'
    value       TEXT NOT NULL,
    -- Japanese pitch accent (Kanjium accents.txt, CC BY-SA 4.0) on the ja kind='kana' rows: the
    -- downstep mora index as a string ("0"=heiban/no drop, "1"=atamadaka, n=drop after mora n).
    -- Multi-accent words keep the full comma list ("2,1"); the serving layer reads the first. NULL =
    -- no Kanjium entry. Only ever set for ja kana rows; the column is meaningless for other kinds.
    accent      TEXT,
    PRIMARY KEY (lexeme_id, kind, value)
) WITHOUT ROWID;
CREATE INDEX idx_lexeme_reading_value ON lexeme_reading(kind, value);

-- A sense = a gloss group on a lexeme. The unit that links to a concept.
CREATE TABLE sense (
    id          INTEGER PRIMARY KEY,
    lexeme_id   INTEGER NOT NULL REFERENCES lexeme(id),
    pos         TEXT,                    -- part of speech (where available)
    gloss_en    TEXT NOT NULL,          -- English gloss text (semicolon-joined)
    sense_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sense_lexeme ON sense(lexeme_id);

-- ============================================================================
-- 3. Concept layer (DESIGN.md §2.3) -- populated in Phase 2
-- ============================================================================

-- A language-independent sense. Gathers words across all four systems that express it.
CREATE TABLE concept (
    id           INTEGER PRIMARY KEY,
    label_en     TEXT,                   -- short English label
    definition   TEXT,
    source       TEXT,                   -- 'omw','wiktionary','gloss-pivot','curated'
    member_count INTEGER NOT NULL DEFAULT 0  -- distinct lexemes (specificity: smaller = tighter)
);

-- sense ↔ concept, many-to-many (DESIGN.md §2: word↔concept is sense-level).
CREATE TABLE sense_concept (
    sense_id    INTEGER NOT NULL REFERENCES sense(id),
    concept_id  INTEGER NOT NULL REFERENCES concept(id),
    confidence  REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (sense_id, concept_id)
) WITHOUT ROWID;
CREATE INDEX idx_sense_concept_concept ON sense_concept(concept_id);

-- Explicit cross-variety equivalence: a precise lexicographer/curated statement that a word in one
-- variety is normally written with a DIFFERENT word in another (colloquial Cantonese 冇 → standard
-- Chinese 沒有; ja 空港 → zh 機場). Stronger and cleaner than the fuzzy English-gloss-pivot concept
-- layer, so it drives the "written differently" bridge directly. Directed; the serving layer reads
-- it in both directions.
CREATE TABLE lexeme_equivalent (
    src_lexeme_id INTEGER NOT NULL REFERENCES lexeme(id),
    dst_lexeme_id INTEGER NOT NULL REFERENCES lexeme(id),
    relation      TEXT NOT NULL,   -- 'colloquial-standard' (粵→中) | 'cross-lang'
    source        TEXT NOT NULL,   -- 'cc-canto-inline' | 'curated'
    PRIMARY KEY (src_lexeme_id, dst_lexeme_id, relation)
) WITHOUT ROWID;
CREATE INDEX idx_lex_equiv_src ON lexeme_equivalent(src_lexeme_id);
CREATE INDEX idx_lex_equiv_dst ON lexeme_equivalent(dst_lexeme_id);

-- ============================================================================
-- 4. "Why" payloads (Phases 3-4) -- origin badges + etymology passthrough, phonology notes
-- ============================================================================

CREATE TABLE origin_badge (
    lexeme_id   INTEGER NOT NULL REFERENCES lexeme(id),
    badge       TEXT NOT NULL,           -- 'wasei-kango','borrowed-from-japanese','psm','calque',...
    PRIMARY KEY (lexeme_id, badge)
) WITHOUT ROWID;

CREATE TABLE etymology (
    lexeme_id   INTEGER NOT NULL REFERENCES lexeme(id),
    text        TEXT NOT NULL,           -- Wiktionary free-text etymology, passthrough (no LLM)
    source      TEXT NOT NULL DEFAULT 'wiktionary',
    PRIMARY KEY (lexeme_id)
) WITHOUT ROWID;

-- ============================================================================
-- 5. Full-text search (FTS5) -- English gloss / translation pivot
-- ============================================================================

-- External-content FTS over sense glosses. Rebuilt by build.py after sense load.
-- porter stemming so a query for "ears"/"loved"/"cats" still matches a gloss "ear"/"love"/"cat"
-- (the single biggest English-search defect was inflected queries matching nothing).
CREATE VIRTUAL TABLE gloss_fts USING fts5(
    gloss_en,
    content='sense',
    content_rowid='id',
    tokenize='porter unicode61'
);

-- ============================================================================
-- 6. Build metadata
-- ============================================================================

CREATE TABLE build_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- e.g. ('schema_version','1'), ('built_at', <iso8601 passed in>), source versions from the lockfile.

-- ----------------------------------------------------------------------------
-- Seed: core-four regions + the reform events Phase 1.1 needs.
-- ----------------------------------------------------------------------------
INSERT INTO region(code,name,script,folds_into,launch) VALUES
    ('CN','Mainland China','simp',NULL,1),
    ('TW','Taiwan','trad',NULL,1),
    ('HK','Hong Kong','trad',NULL,1),
    ('JP','Japan','shinjitai',NULL,1);

INSERT INTO reform(id,name,year,note) VALUES
    ('prc-1956','PRC First Simplification Scheme',1956,NULL),
    ('prc-1964','PRC Complete List of Simplified Characters',1964,NULL),
    ('jp-toyo','Japanese Tōyō kanji (shinjitai)',1946,NULL),
    ('jp-joyo','Japanese Jōyō kanji',1981,NULL),
    ('hk-std','Hong Kong standard form',NULL,NULL),
    ('tw-std','Taiwan standard form',NULL,NULL),
    ('unihan-variant','Unihan variant field',NULL,'z-variant / semantic-variant source'),
    ('opencc','OpenCC conversion mapping',NULL,NULL);

INSERT INTO build_meta(key,value) VALUES ('schema_version','1');
