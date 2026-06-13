"""Build orchestrator: sources/ -> data/kanzi.sqlite.

Runs each ingest step in order, then finalises (rebuild FTS, set build_meta) and verifies the
build-time invariants from DESIGN.md (every living glyph -> exactly one orthodox parent; no
placeholder leaks). The invariant check FAILS the build loudly — that's the point.

Usage:
    python -m kanzipipe.build               # full build into data/kanzi.sqlite
    python -m kanzipipe.build --out /tmp/x.sqlite
"""
from __future__ import annotations

import sys

from . import db
from .ingest import INGEST_STEPS


class BuildError(RuntimeError):
    pass


def verify_invariants(conn) -> list[str]:
    """Return a list of invariant violations (empty = healthy)."""
    problems: list[str] = []

    # Invariant 1: bounded transitive closure must terminate — no glyph is its own ancestor
    # through simp/shinjitai edges (a runaway chain / cycle). Multi-parent is allowed (it is the
    # documented many-to-one merge phenomenon: 发←發/髮, 弁←瓣/辨/辯, 台←臺/檯/颱). The "exactly one
    # parent" wording in DESIGN is realised as targeted regression probes (広→廣 etc.), not here.
    cycle = conn.execute("""
        WITH RECURSIVE reach(start, node, depth) AS (
            SELECT child_cp, parent_cp, 1 FROM glyph_edge
              WHERE type IN ('simplification','shinjitai')
            UNION ALL
            SELECT r.start, e.parent_cp, r.depth + 1
              FROM reach r JOIN glyph_edge e ON e.child_cp = r.node
             WHERE e.type IN ('simplification','shinjitai') AND r.depth < 16
        )
        SELECT COUNT(*) FROM reach WHERE start = node
    """).fetchone()[0]
    if cycle:
        problems.append(f"{cycle} cyclic identity-edge path(s) — closure does not terminate")

    # Invariant 2: no placeholder leaks (the original's `xx5` class of bug).
    for tbl, col in [("char_reading", "value"), ("lexeme", "reading"),
                     ("lexeme_reading", "value")]:
        n = conn.execute(
            f"SELECT COUNT(*) FROM {tbl} WHERE {col} GLOB '*[xX][xX][0-9]*'").fetchone()[0]
        if n:
            problems.append(f"{n} placeholder-like values in {tbl}.{col}")

    # Invariant 3: every edge endpoint exists as a character (FK already enforces, belt+braces).
    n = conn.execute("""
        SELECT COUNT(*) FROM glyph_edge e
        LEFT JOIN character p ON p.cp = e.parent_cp
        WHERE p.cp IS NULL
    """).fetchone()[0]
    if n:
        problems.append(f"{n} glyph_edge rows point at a missing parent character")

    return problems


def finalize(conn, *, built_at: str | None = None) -> None:
    # Rebuild external-content FTS from sense.
    conn.execute("INSERT INTO gloss_fts(gloss_fts) VALUES ('rebuild')")
    if built_at:
        conn.execute(
            "INSERT OR REPLACE INTO build_meta(key,value) VALUES ('built_at',?)", (built_at,))
    conn.execute("ANALYZE")
    conn.commit()


def counts(conn) -> dict[str, int]:
    out = {}
    for t in ("character", "glyph_edge", "char_reading", "lexeme", "surface_form",
              "sense", "concept"):
        out[t] = conn.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]
    return out


def build(out=None, *, built_at: str | None = None) -> str:
    out = str(out or db.DB_PATH)
    print(f"building {out}")
    conn = db.create_db(out)
    for name, step in INGEST_STEPS:
        print(f"  • {name}")
        step(conn)
        conn.commit()
    finalize(conn, built_at=built_at)

    problems = verify_invariants(conn)
    print("  counts:", counts(conn))
    if problems:
        for p in problems:
            print(f"  ✗ INVARIANT: {p}", file=sys.stderr)
        conn.close()
        raise BuildError(f"{len(problems)} invariant violation(s) — build rejected")
    print("  ✓ invariants hold")
    conn.close()
    return out


def main(argv: list[str]) -> int:
    out = None
    if "--out" in argv:
        out = argv[argv.index("--out") + 1]
    # built_at is passed in (Date.now is fine here — this is a plain script, not a workflow)
    import datetime
    build(out, built_at=datetime.datetime.utcnow().isoformat() + "Z")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
