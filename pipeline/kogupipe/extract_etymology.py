"""Phase 3.2 — stream-extract Wiktionary etymology + origin badges from kaikki (no full storage).

Streams the (large) kaikki per-language JSONL line by line, keeps ONLY entries whose word is a
surface form already in our DB, and writes a compact JSONL per language to sources/:
    {"word":..., "lang":"zh"|"ja", "ety":"<text>", "badges":[...]}

No LLM — just passthrough of Wiktionary's own etymology_text + structured origin templates.

Run (rarely):  python -m kogupipe.extract_etymology
"""
from __future__ import annotations

import json
import sqlite3
import urllib.request

from .db import DB_PATH, SOURCES_DIR

UA = "wenbun-pipeline/0.0 (+https://miro.build)"
KAIKKI = {
    "zh": "https://kaikki.org/dictionary/Chinese/kaikki.org-dictionary-Chinese.jsonl",
    "ja": "https://kaikki.org/dictionary/Japanese/kaikki.org-dictionary-Japanese.jsonl",
}

# wiktextract etymology_template name -> badge (origin category)
_LANG_NAME = {"ja": "japanese", "en": "english", "zh": "chinese", "sa": "sanskrit",
              "pt": "portuguese", "nl": "dutch", "fr": "french", "de": "german"}


def _badges(obj: dict) -> list[str]:
    out: set[str] = set()
    for t in obj.get("etymology_templates", []):
        name = (t.get("name") or "").lower()
        args = t.get("args", {}) or {}
        if name in ("wasei kango", "wasei-kango", "waseikango"):
            out.add("wasei-kango")
        elif name in ("bor", "borrowed", "bor+", "obor"):
            src = (args.get("2") or args.get("3") or "").lower()
            label = _LANG_NAME.get(src)
            out.add(f"borrowed-from-{label}" if label else "borrowed")
        elif name in ("cal", "calque", "calque of"):
            out.add("calque")
        elif name in ("psm", "phono-semantic matching"):
            out.add("phono-semantic-matching")
    return sorted(out)


def load_forms() -> set[str]:
    conn = sqlite3.connect(str(DB_PATH))
    forms = {r[0] for r in conn.execute("SELECT DISTINCT form FROM surface_form")}
    conn.close()
    return forms


def extract(lang: str, url: str, forms: set[str]) -> int:
    out_path = SOURCES_DIR / f"etymology.{lang}.jsonl"
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    n = kept = 0
    with urllib.request.urlopen(req, timeout=120) as r, open(out_path, "w", encoding="utf-8") as out:
        for raw in r:
            n += 1
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            word = obj.get("word")
            if not word or word not in forms:
                continue
            ety = (obj.get("etymology_text") or "").strip()
            badges = _badges(obj)
            if not ety and not badges:
                continue
            rec = {"word": word, "lang": lang, "ety": ety[:600], "badges": badges}
            out.write(json.dumps(rec, ensure_ascii=False) + "\n")
            kept += 1
            if n % 200000 == 0:
                print(f"  {lang}: scanned {n:,}, kept {kept:,}")
    print(f"  {lang}: done — scanned {n:,}, kept {kept:,} -> {out_path.name}")
    return kept


def main() -> int:
    forms = load_forms()
    print(f"loaded {len(forms):,} surface forms")
    for lang, url in KAIKKI.items():
        print(f"streaming {lang} from {url}")
        extract(lang, url, forms)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
