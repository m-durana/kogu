"""Populate curated English search aliases (lexeme_alias) in place on the live DB.

Closed-class words (pronouns, particles) come from CC-Canto with glosses that list only some of the
English paradigm: 佢 is glossed "(Cantonese) he, she, it" but English users also search "him", "her",
"his"; 佢哋 is "they" but also "them"/"their". Because search matches gloss TEXT, those queries miss
the correct word entirely and a high-frequency written-Standard-Chinese form (她的) surfaces instead.

This attaches the full English paradigm to each canonical word as hidden search terms. It is the
general lever: to cover another gap, add a row to PARADIGMS -- no code change. Idempotent: it drops
its own source='paradigm' rows and rewrites them, so it is safe to re-run on every refresh/rebuild.

Usage: KOGU_DB=data/kogu.sqlite pipeline/.venv/bin/python pipeline/refresh_aliases.py
"""
import os
import sqlite3
import sys

DB = os.environ.get("KOGU_DB", "data/kogu.sqlite")

# (variety, headword, [English aliases]). The aliases are the FULL paradigm the canonical word covers,
# including forms its dictionary gloss omits. Kept deliberately conservative: only closed-class words
# where the mapping is unambiguous.
PARADIGMS = [
    # Cantonese personal pronouns (genderless; CC-Canto glosses drop object/possessive forms).
    ("yue", "佢",   ["he", "him", "his", "she", "her", "hers", "it", "its"]),
    ("yue", "佢哋", ["they", "them", "their", "theirs"]),
    ("yue", "佢地", ["they", "them", "their", "theirs"]),
    ("yue", "我哋", ["we", "us", "our", "ours"]),
    ("yue", "我地", ["we", "us", "our", "ours"]),
    ("yue", "你哋", ["you", "your", "yours"]),
    ("yue", "你地", ["you", "your", "yours"]),
    # Mandarin/Standard-Chinese pronouns: gendered, so keep the alias set to the matching gender
    # (她=she, 他=he) rather than lumping every 3rd-person term together.
    ("zh", "她",   ["she", "her", "hers"]),
    ("zh", "他",   ["he", "him", "his"]),
    ("zh", "它",   ["it", "its"]),
    ("zh", "她們", ["they", "them", "their", "theirs"]),
    ("zh", "他們", ["they", "them", "their", "theirs"]),
    ("zh", "我",   ["i", "me", "my", "mine"]),
    ("zh", "我們", ["we", "us", "our", "ours"]),
    ("zh", "你",   ["you", "your", "yours"]),
    ("zh", "你們", ["you", "your", "yours"]),
    # Japanese: 彼女=she/her, 彼=he/him (also girlfriend/boyfriend as separate senses). No clean
    # single-word ja pronoun for it/they, so those are left to the gloss layer.
    ("ja", "彼女", ["she", "her", "hers"]),
    ("ja", "彼",   ["he", "him", "his"]),
]


def ensure_schema(conn):
    conn.execute(
        """CREATE TABLE IF NOT EXISTS lexeme_alias (
               lexeme_id INTEGER NOT NULL REFERENCES lexeme(id),
               term      TEXT NOT NULL,
               source    TEXT NOT NULL DEFAULT 'paradigm',
               PRIMARY KEY (lexeme_id, term)
           ) WITHOUT ROWID"""
    )
    conn.execute("CREATE INDEX IF NOT EXISTS idx_lexeme_alias_term ON lexeme_alias(term)")


def main():
    conn = sqlite3.connect(DB)
    ensure_schema(conn)
    conn.execute("DELETE FROM lexeme_alias WHERE source = 'paradigm'")

    inserted, missing = 0, []
    for variety, headword, terms in PARADIGMS:
        ids = [r[0] for r in conn.execute(
            "SELECT id FROM lexeme WHERE variety = ? AND headword = ?", (variety, headword))]
        if not ids:
            missing.append(f"{variety}:{headword}")
            continue
        for lid in ids:
            for term in terms:
                conn.execute(
                    "INSERT OR IGNORE INTO lexeme_alias(lexeme_id, term, source) VALUES (?, ?, 'paradigm')",
                    (lid, term.lower()),
                )
                inserted += 1
    conn.commit()
    total = conn.execute("SELECT COUNT(*) FROM lexeme_alias").fetchone()[0]
    conn.close()
    print(f"aliases: wrote {inserted} paradigm rows, {total} total"
          + (f"; skipped missing lexemes: {', '.join(missing)}" if missing else ""))


if __name__ == "__main__":
    main()
