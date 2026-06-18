"""Phase 4 - load Middle Chinese (廣韻) readings into char_reading as kind='mc'.

Reads sources/char_mc.json (produced by `node pipeline/scripts/gen_mc.mjs`, which romanizes the
廣韻 corpus bundled in the nk2028/tshet-uinh package, CC0, with Baxter's transcription). Each value
is one Baxter reading (馬 = maeX, 母 = muwX, 海 = xojX); a character with several 廣韻 readings stores
each. Only characters already in our `character` table are ingested (no new glyphs, no DB bloat).

These readings power the "phonological why" in the structure section: a phono-semantic compound's
own MC reading next to its phonetic component's MC reading shows the historical sound link (銅 duwng
from 同 duwng; 晴 dzjeng from 青 tsheng). Runs after the character backbone (FK on character.cp).

Idempotent: clears prior kind='mc' rows, reloads, and stamps build_meta('middle_chinese').
"""
from __future__ import annotations

import json

from ..db import SOURCES_DIR

SRC = SOURCES_DIR / "char_mc.json"
VERSION = "1"  # bump when the romanization scheme or source changes


def ingest(conn) -> None:
    if not SRC.exists():
        print("      char_mc.json missing - skipping (run node pipeline/scripts/gen_mc.mjs)")
        return
    have = {cp for (cp,) in conn.execute("SELECT cp FROM character")}
    data: dict[str, list[str]] = json.loads(SRC.read_text(encoding="utf-8"))

    rows = []
    skipped = 0
    for ch, readings in data.items():
        if len(ch) != 1:
            continue
        cp = ord(ch)
        if cp not in have:
            skipped += 1
            continue
        for value in readings:
            if value:
                rows.append((cp, "mc", value))

    # idempotent reload: drop any prior MC rows, then insert the current set
    conn.execute("DELETE FROM char_reading WHERE kind='mc'")
    conn.executemany(
        "INSERT OR IGNORE INTO char_reading(cp,kind,value) VALUES (?,?,?)", rows
    )
    conn.execute(
        "INSERT OR REPLACE INTO build_meta(key,value) VALUES ('middle_chinese',?)", (VERSION,)
    )
    print(
        f"      char_reading mc rows={len(rows)} across {len({r[0] for r in rows})} characters"
        f" (skipped {skipped} chars not in our table)"
    )
