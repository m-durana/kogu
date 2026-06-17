"""Phase 3.3 - stream-extract structured Han-compound component ROLES from kaikki (no full storage).

Most Han characters are phono-semantic compounds (形聲): one component carries the MEANING, another
the SOUND. Wiktionary encodes this in the `Han compound` template, which wiktextract surfaces in
`etymology_templates` — e.g. for 愛:
    {"name":"Han compound","args":{"1":"旡","2":"心","ls":"psc","c1":"p","c2":"s","t2":"heart"}}
meaning 旡 = phonetic, 心 = semantic ("heart"). `extract_etymology.py` keeps only the prose and throws
this away; here we stream the same dumps and keep ONLY the structured component roles for single
characters, writing a compact JSONL to sources/ (which lives on /mnt):
    {"char":"愛","ls":"psc","components":[{"ch":"旡","role":"phonetic"},{"ch":"心","role":"semantic","gloss":"heart"}]}

No LLM, no role-guessing from prose — only the structured template. Run (rarely):
    python -m kogupipe.extract_components
"""
from __future__ import annotations

import json
import urllib.request

from .db import SOURCES_DIR

UA = "wenbun-pipeline/0.0 (+https://miro.build)"
KAIKKI = {
    "zh": "https://kaikki.org/dictionary/Chinese/kaikki.org-dictionary-Chinese.jsonl",
    "ja": "https://kaikki.org/dictionary/Japanese/kaikki.org-dictionary-Japanese.jsonl",
}
OUT = SOURCES_DIR / "components.jsonl"

# c1/c2/… role codes in the Han compound template → plain role. 's' = semantic (signific), 'p' =
# phonetic. Others (form/iconic/simplified-of …) are kept verbatim but the UI only badges s/p.
_ROLE = {"s": "semantic", "p": "phonetic", "f": "form", "i": "iconic"}


def _is_single_han(w: str) -> bool:
    if len(w) != 1:
        return False
    cp = ord(w)
    return (0x3400 <= cp <= 0x9FFF) or (0xF900 <= cp <= 0xFAFF) or (0x20000 <= cp <= 0x3FFFF)


def _clean_comp(v: str) -> str:
    """A component arg is normally a single glyph; strip stray markup/space and keep the first char."""
    v = (v or "").strip()
    return v[0] if v else ""


def _from_template(t: dict) -> list[dict] | None:
    args = t.get("args", {}) or {}
    comps: list[dict] = []
    for i in range(1, 8):
        ch = _clean_comp(args.get(str(i), ""))
        if not ch or not _is_single_han(ch):
            continue
        role = _ROLE.get((args.get(f"c{i}") or "").strip().lower())
        gloss = (args.get(f"t{i}") or "").strip() or None
        comps.append({"ch": ch, "role": role, "gloss": gloss})
    return comps or None


def extract(lang: str, url: str, seen: dict) -> int:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    n = kept = 0
    with urllib.request.urlopen(req, timeout=180) as r:
        for raw in r:
            n += 1
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            w = obj.get("word", "")
            if not _is_single_han(w) or w in seen:
                continue
            for t in obj.get("etymology_templates", []):
                if (t.get("name") or "").lower().replace("_", " ") != "han compound":
                    continue
                comps = _from_template(t)
                if not comps:
                    continue
                # only useful when it actually distinguishes roles (has a phonetic or semantic tag)
                if not any(c["role"] in ("phonetic", "semantic") for c in comps):
                    continue
                ls = (t.get("args", {}) or {}).get("ls")
                seen[w] = {"char": w, "ls": ls, "components": comps}
                kept += 1
                break
            if n % 200000 == 0:
                print(f"  {lang}: scanned {n:,}, kept {kept:,}")
    print(f"  {lang}: done - scanned {n:,}, kept {kept:,}")
    return kept


def main() -> int:
    seen: dict = {}  # char -> record; zh streamed first so it wins ties
    for lang, url in KAIKKI.items():
        print(f"streaming {lang} from {url}")
        extract(lang, url, seen)
    with open(OUT, "w", encoding="utf-8") as out:
        for rec in seen.values():
            out.write(json.dumps(rec, ensure_ascii=False) + "\n")
    print(f"wrote {len(seen):,} character records -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
