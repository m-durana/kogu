"""Romaji reading index - lets Japanese words be found by romaji input, tolerant of long-vowel and
n/m spelling variants (tokyo = toukyou = tōkyō → 東京; shinbun = shimbun → 新聞).

Adds a lexeme_reading row kind='romaji_plain' for every kana reading, holding a folded Hepburn key.
search.rs folds the query the same way and matches against it.
"""
from ..db import SOURCES_DIR  # noqa: F401  (keeps import style consistent across ingest modules)

# base hiragana -> romaji (digraphs handled separately)
_BASE = {
    "あ": "a", "い": "i", "う": "u", "え": "e", "お": "o",
    "か": "ka", "き": "ki", "く": "ku", "け": "ke", "こ": "ko",
    "が": "ga", "ぎ": "gi", "ぐ": "gu", "げ": "ge", "ご": "go",
    "さ": "sa", "し": "shi", "す": "su", "せ": "se", "そ": "so",
    "ざ": "za", "じ": "ji", "ず": "zu", "ぜ": "ze", "ぞ": "zo",
    "た": "ta", "ち": "chi", "つ": "tsu", "て": "te", "と": "to",
    "だ": "da", "ぢ": "ji", "づ": "zu", "で": "de", "ど": "do",
    "な": "na", "に": "ni", "ぬ": "nu", "ね": "ne", "の": "no",
    "は": "ha", "ひ": "hi", "ふ": "fu", "へ": "he", "ほ": "ho",
    "ば": "ba", "び": "bi", "ぶ": "bu", "べ": "be", "ぼ": "bo",
    "ぱ": "pa", "ぴ": "pi", "ぷ": "pu", "ぺ": "pe", "ぽ": "po",
    "ま": "ma", "み": "mi", "む": "mu", "め": "me", "も": "mo",
    "や": "ya", "ゆ": "yu", "よ": "yo",
    "ら": "ra", "り": "ri", "る": "ru", "れ": "re", "ろ": "ro",
    "わ": "wa", "ゐ": "i", "ゑ": "e", "を": "o", "ん": "n",
    "ぁ": "a", "ぃ": "i", "ぅ": "u", "ぇ": "e", "ぉ": "o",
    "ゔ": "vu",
}
_YOON = {
    "ゃ": "ya", "ゅ": "yu", "ょ": "yo",
}
_VOWELS = set("aeiou")


def kana_to_romaji(s: str) -> str:
    # normalise katakana -> hiragana
    s = "".join(chr(ord(c) - 0x60) if "ァ" <= c <= "ヶ" else c for c in s)
    out: list[str] = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == "っ":  # sokuon - double the next consonant
            nxt = s[i + 1] if i + 1 < len(s) else ""
            r = _BASE.get(nxt, "")
            if r and r[0] not in _VOWELS:
                out.append(r[0])
            i += 1
            continue
        if c in ("ー", "～"):  # long-vowel mark - repeat previous vowel
            if out and out[-1] and out[-1][-1] in _VOWELS:
                out.append(out[-1][-1])
            i += 1
            continue
        # digraph: a base ending in -i (except n) + small ya/yu/yo
        if i + 1 < len(s) and s[i + 1] in _YOON:
            base = _BASE.get(c, "")
            if base.endswith("i") and base != "i":
                stem = base[:-1]
                if stem.endswith(("sh", "ch", "j")):
                    out.append(stem + _YOON[s[i + 1]][1:])  # sha/sho/shu, cha…, ja…
                else:
                    out.append(stem + _YOON[s[i + 1]])
                i += 2
                continue
        out.append(_BASE.get(c, ""))
        i += 1
    return "".join(out)


def fold(romaji: str) -> str:
    """Canonical key: lowercase a-z, macrons unfolded, long vowels collapsed, n before b/m/p."""
    macron = {"ā": "a", "ī": "i", "ū": "u", "ē": "e", "ō": "o", "â": "a", "î": "i", "û": "u", "ê": "e", "ô": "o"}
    s = "".join(macron.get(c, c) for c in romaji.lower())
    s = "".join(c for c in s if c.isascii() and c.isalpha())
    # m -> n before a labial (shimbun -> shinbun)
    for lab in ("b", "p", "m"):
        s = s.replace("m" + lab, "n" + lab)
    # collapse long vowels / vowel digraphs representing length
    for a, b in (("ou", "o"), ("oo", "o"), ("uu", "u"), ("ee", "e"), ("ei", "e"), ("aa", "a"), ("ii", "i")):
        s = s.replace(a, b)
    return s


def ingest(conn) -> None:
    rows = conn.execute(
        "SELECT lexeme_id, value FROM lexeme_reading WHERE kind='kana'"
    ).fetchall()
    seen: set[tuple[int, str]] = set()
    out = []
    for lex, kana in rows:
        key = fold(kana_to_romaji(kana))
        if key and (lex, key) not in seen:
            seen.add((lex, key))
            out.append((lex, "romaji_plain", key))
    conn.executemany(
        "INSERT INTO lexeme_reading (lexeme_id, kind, value) VALUES (?1, ?2, ?3)", out
    )
    print(f"    romaji: {len(out)} romaji_plain keys from {len(rows)} kana readings")
