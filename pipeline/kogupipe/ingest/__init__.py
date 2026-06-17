"""Ingest steps, run in order by build.py.

Each step is ``(name, fn)`` where ``fn(conn)`` reads from ``sources/`` and writes into the DB.
Order matters: backbone characters before lexemes (FKs / readings reference characters).
"""
from . import backbone, cantonese, components, concepts, equivalents, etymology, frequency, lexemes, romaji

INGEST_STEPS = [
    ("character backbone (Unihan + cjkvi-ids + OpenCC)", backbone.ingest),
    ("phono-semantic component roles (Wiktionary Han-compound)", components.ingest),
    ("lexemes (CC-CEDICT + JMdict + Kanjidic)", lexemes.ingest),
    ("Cantonese (CC-Canto: jyutping + 粵字)", cantonese.ingest),
    ("concept layer (gloss pivot)", concepts.ingest),
    ("explicit equivalence edges (inline + curated)", equivalents.ingest),
    ("etymology + origin badges (Wiktionary passthrough)", etymology.ingest),
    ("word frequency (OpenSubtitles zh/ja)", frequency.ingest),
    ("romaji reading index", romaji.ingest),
]
