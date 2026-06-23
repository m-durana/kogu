"""Warm the TTS cache with the most common Japanese readings so their FIRST play is instant.

On-demand synthesis stays the core (it covers the long tail and any reading), but the cold synth is
~100-270 ms; pre-synthesizing the top-N readings by corpus frequency moves that cost off the user's
first tap for the words people actually look up. Prerendered mp3s land in the SAME (kana, accent) cache
the sidecar serves, so this is a cache warm-up, not a separate code path.

    KOGU_TTS_CACHE=/mnt/HC_Volume_102319212/kogu-tts/cache \
      /mnt/HC_Volume_102319212/kogu-tts/venv/bin/python tts/prerender.py [N]

Default N = 12000. Idempotent: skips readings already cached, so it's safe to re-run / resume.
"""
import os
import sqlite3
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synth_service as s  # noqa: E402

DB = os.environ.get("KOGU_DB", "/srv/miro/kogu/data/kogu.sqlite")
N = int(sys.argv[1]) if len(sys.argv) > 1 else 12000


def rows(n: int):
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    try:
        # distinct (kana, accent) ranked by the lexeme's best corpus frequency
        return con.execute(
            "SELECT lr.value, lr.accent FROM lexeme_reading lr JOIN lexeme l ON l.id = lr.lexeme_id "
            "WHERE l.variety='ja' AND lr.kind='kana' AND lr.accent IS NOT NULL AND l.freq IS NOT NULL "
            "GROUP BY lr.value, lr.accent ORDER BY MAX(l.freq) DESC LIMIT ?",
            (n,),
        ).fetchall()
    finally:
        con.close()


def main() -> int:
    items = rows(N)
    print(f"prerendering up to {len(items)} common ja readings into {s.CACHE_DIR}")
    done = skip = fail = 0
    t0 = time.time()
    for kana, accent in items:
        if not kana or len(kana) > 32 or not s.KANA_RE.match(kana):
            skip += 1
            continue
        if os.path.exists(s._cache_path(kana, accent)):
            skip += 1
            continue
        try:
            s.synth(kana, accent)
            done += 1
        except Exception as e:
            fail += 1
            sys.stderr.write(f"  fail {kana!r}/{accent!r}: {e}\n")
        n = done + skip + fail
        if n % 500 == 0:
            rate = done / max(1e-9, time.time() - t0)
            print(f"  {n}/{len(items)}  synth={done} skip={skip} fail={fail}  {rate:.1f}/s")
    print(f"done: synth={done} skip={skip} fail={fail} in {time.time() - t0:.0f}s; cache now "
          f"{len([f for f in os.listdir(s.CACHE_DIR) if f.endswith('.mp3')])} files")
    return 1 if fail and not done else 0


if __name__ == "__main__":
    raise SystemExit(main())
