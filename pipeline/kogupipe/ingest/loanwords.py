"""Loanword origins from JMdict ``<lsource>`` (sense.languageSource): the source language and original
spelling of gairaigo / wasei-eigo (アイス → English "ice"; アールデコ → French "art déco").

Pure STRUCTURED passthrough, NOT prose written by an LLM: we deterministically format JMdict's own
fields (lang, wasei flag, original spelling) into a one-line origin + matching badges — the same way
``etymology.py`` derives badges from Wiktionary templates. We only FILL A GAP: an entry that already
has a Wiktionary etymology keeps it (INSERT OR IGNORE on the etymology PK).
"""
from __future__ import annotations

import json
import zipfile
from collections import defaultdict

from .lexemes import _JM_SEARCH_ONLY, _jm_headword, _jm_tags, JMDICT_ZIP

# JMdict uses ISO 639-2/B 3-letter codes; map the common ones to a readable English name.
LANG_NAMES = {
    "eng": "English", "fre": "French", "ger": "German", "ita": "Italian", "dut": "Dutch",
    "por": "Portuguese", "spa": "Spanish", "rus": "Russian", "chi": "Chinese", "kor": "Korean",
    "lat": "Latin", "gre": "Greek", "grc": "Ancient Greek", "ara": "Arabic", "heb": "Hebrew",
    "san": "Sanskrit", "tur": "Turkish", "hun": "Hungarian", "pol": "Polish", "swe": "Swedish",
    "nor": "Norwegian", "dan": "Danish", "fin": "Finnish", "tha": "Thai", "vie": "Vietnamese",
    "may": "Malay", "ind": "Indonesian", "tib": "Tibetan", "mon": "Mongolian", "per": "Persian",
    "afr": "Afrikaans", "haw": "Hawaiian", "ain": "Ainu", "urd": "Urdu", "hin": "Hindi",
    "tam": "Tamil", "cze": "Czech", "gle": "Irish", "slv": "Slovenian", "epo": "Esperanto",
    "glg": "Galician", "rum": "Romanian", "ukr": "Ukrainian", "gle ": "Irish",
}


def _lang_name(code: str | None) -> str:
    if not code:
        return "another language"
    return LANG_NAMES.get(code, code.upper())


def loan_text(sources: list[dict]) -> str:
    """A deterministic one-line origin from the lsource entries (one per language, first spelling kept)."""
    by_lang: list[tuple[str, str | None]] = []
    for s in sources:
        lang = s.get("lang") or ""
        if lang and lang not in [l for l, _ in by_lang]:
            by_lang.append((lang, s.get("text")))
    parts = [
        _lang_name(lang) + (f" “{text}”" if text else "")
        for lang, text in by_lang
    ] or ["another language"]
    joined = " and ".join(parts)
    if any(s.get("wasei") for s in sources):
        # wasei: looks like a loan but was coined in Japan from foreign elements (ベビーカー = "baby car")
        return f"Japanese coinage from {joined}."
    return f"From {joined}."


def loan_badges(sources: list[dict]) -> set[str]:
    out: set[str] = set()
    for s in sources:
        lang = s.get("lang")
        if lang:
            out.add("borrowed-from-" + _lang_name(lang).lower().replace(" ", "-"))
        if s.get("wasei"):
            out.add("wasei-eigo" if lang == "eng" else "wasei")
    return out


def _records():
    """Yield (headword, reading, [lsource dicts]) for every JMdict word that has a language source."""
    z = zipfile.ZipFile(JMDICT_ZIP)
    name = next(n for n in z.namelist() if n.endswith(".json"))
    data = json.loads(z.read(name))
    for w in data["words"]:
        kana = w.get("kana", [])
        if not kana:
            continue
        srcs: list[dict] = []
        for s in w.get("sense", []):
            srcs.extend(s.get("languageSource") or [])
        if not srcs:
            continue
        disp_kana = [kn for kn in kana if not (_jm_tags(kn) & _JM_SEARCH_ONLY)] or kana[:1]
        yield _jm_headword(w.get("kanji", []), disp_kana), disp_kana[0]["text"], srcs


def ingest(conn) -> None:
    # (headword, reading) -> [lexeme_id] for ja lexemes (exactly how lexemes.py created them)
    idx: dict[tuple[str, str], list[int]] = defaultdict(list)
    for lid, hw, rd in conn.execute("SELECT id, headword, reading FROM lexeme WHERE variety='ja'"):
        idx[(hw, rd)].append(lid)

    ety_rows: dict[int, str] = {}
    badge_rows: set[tuple[int, str]] = set()
    matched = 0
    for headword, reading, srcs in _records():
        lids = idx.get((headword, reading))
        if not lids:
            continue
        matched += 1
        text = loan_text(srcs)
        badges = loan_badges(srcs)
        for lid in lids:
            ety_rows.setdefault(lid, text)
            for b in badges:
                badge_rows.add((lid, b))

    conn.executemany(
        "INSERT OR IGNORE INTO etymology(lexeme_id,text,source) VALUES (?,?, 'jmdict')",
        [(lid, t) for lid, t in ety_rows.items()],
    )
    conn.executemany(
        "INSERT OR IGNORE INTO origin_badge(lexeme_id,badge) VALUES (?,?)", sorted(badge_rows)
    )
    print(f"      loanword origins matched={matched} etymology(gap-fill)={len(ety_rows)} badges={len(badge_rows)}")
