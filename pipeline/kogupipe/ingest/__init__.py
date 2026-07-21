"""Ingest steps, run in order by build.py.

Each step is ``(name, fn)`` where ``fn(conn)`` reads from ``sources/`` and writes into the DB.
Order matters: backbone characters before lexemes (FKs / readings reference characters).
"""
from . import (
    accents,
    backbone,
    cantonese,
    chengyu,
    components,
    confusables,
    concepts,
    equivalents,
    etymology,
    frequency,
    howell,
    lexemes,
    loanwords,
    middle_chinese,
    omw,
    romaji,
    translations,
)

INGEST_STEPS = [
    ("character backbone (Unihan + cjkvi-ids + OpenCC)", backbone.ingest),
    ("confusable look-alikes (Unihan kSpoofingVariant)", confusables.ingest),
    ("phono-semantic component roles (Wiktionary Han-compound)", components.ingest),
    ("Middle Chinese readings (廣韻 / Baxter, nk2028)", middle_chinese.ingest),
    ("lexemes (CC-CEDICT + JMdict + Kanjidic)", lexemes.ingest),
    ("Cantonese (CC-Canto: jyutping + 粵字)", cantonese.ingest),
    ("concept layer (gloss pivot)", concepts.ingest),
    ("concept layer (Open Multilingual Wordnet synsets)", omw.ingest),
    ("explicit equivalence edges (inline + curated)", equivalents.ingest),
    ("cross-language bridges (Wiktionary translation tables)", translations.ingest),
    ("etymology + origin badges (Wiktionary passthrough)", etymology.ingest),
    ("loanword origins (JMdict lsource: gairaigo / wasei-eigo)", loanwords.ingest),
    ("character etymology (Howell, MIT: phono-semantic gap-fill)", howell.ingest),
    ("idiom etymology (chinese-xinhua, MIT: 成語 出處)", chengyu.ingest),
    ("word frequency (OpenSubtitles zh/ja)", frequency.ingest),
    ("romaji reading index", romaji.ingest),
    ("Japanese pitch accent (Kanjium, CC BY-SA 4.0)", accents.ingest),
]
