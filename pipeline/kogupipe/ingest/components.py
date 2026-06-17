"""Phase 3.3 - load phono-semantic component roles into char_component.

Reads sources/components.jsonl (produced offline by `python -m kogupipe.extract_components`, which
streams the kaikki dumps for the structured `Han compound` templates) and records which component of
each character carries the MEANING vs the SOUND (媽 = 女 semantic + 馬 phonetic). Distinct from the IDS
decomposition, which has no role information. Runs after the character backbone (FK on character.cp).
"""
from __future__ import annotations

import json

from ..db import SOURCES_DIR

SRC = SOURCES_DIR / "components.jsonl"


def ingest(conn) -> None:
    if not SRC.exists():
        print("      components.jsonl missing - skipping (run kogupipe.extract_components)")
        return
    have = {cp for (cp,) in conn.execute("SELECT cp FROM character")}
    rows = []
    with open(SRC, encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            cp = ord(rec["char"])
            if cp not in have:
                continue
            for i, c in enumerate(rec["components"]):
                rows.append((cp, i, c["ch"], c.get("role"), c.get("gloss")))
    conn.executemany(
        "INSERT OR REPLACE INTO char_component(cp,ord,component,role,gloss) VALUES (?,?,?,?,?)", rows
    )
    print(f"      char_component rows={len(rows)} across {len({r[0] for r in rows})} characters")
