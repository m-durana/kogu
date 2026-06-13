"""Ingest steps, run in order by build.py.

Each step is ``(name, fn)`` where ``fn(conn)`` reads from ``sources/`` and writes into the DB.
Order matters: backbone characters before lexemes (FKs / readings reference characters).
"""
from . import backbone, concepts, lexemes

INGEST_STEPS = [
    ("character backbone (Unihan + cjkvi-ids + OpenCC)", backbone.ingest),
    ("lexemes (CC-CEDICT + JMdict + Kanjidic)", lexemes.ingest),
    ("concept layer (gloss pivot)", concepts.ingest),
]
