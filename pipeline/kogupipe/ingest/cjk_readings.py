"""Parse the full polyphonic pinyin / jyutping reading set for a character from Unihan fields.

Unihan's kMandarin / kCantonese give the single "customary" reading (that is all the ingest used to
read, so polyphonic characters showed only one). The complete reading sets live in richer fields:
  - pinyin:   kMandarin (primary) + kTGHZ2013 (modern standard, multi) or kHanyuPinyin (comprehensive)
  - jyutping: kCantonese (primary) + kSMSZD2003Readings ("pinyin粵jyut1,jyut2 …", multi)

Both functions take a dict mapping field-name -> list of raw Unihan value strings for ONE character and
return an ordered, de-duplicated reading list with the customary reading first.
"""
import re
import unicodedata

# a pinyin syllable: letters + tone-marked vowels only (rejects location codes like "10643.080")
_PINYIN = re.compile(r"^[^\W\d_]+$", re.UNICODE)
# a jyutping syllable: lowercase letters + a tone digit 1-6
_JYUT = re.compile(r"^[a-z]+[1-6]$")
# the kSMSZD2003Readings separator between a pinyin and its jyutping(s): 粵 U+7CB5 (and the 粤 U+7CA4 variant)
_YUE = re.compile("[粵粤]")


def _after_colon(values: list[str]) -> list[str]:
    """Readings from "location:reading,reading …" entries — strip the location prefix, split readings."""
    out: list[str] = []
    for v in values:
        for tok in v.split():
            part = tok.split(":", 1)[1] if ":" in tok else tok  # drop the "NNNN.NNN:" location code
            out.extend(part.split(","))
    return out


def _dedup(items, valid) -> list[str]:
    seen: list[str] = []
    for r in items:
        r = unicodedata.normalize("NFC", r.strip())
        if r and valid(r) and r not in seen:
            seen.append(r)
    return seen


def parse_pinyin(fields: dict[str, list[str]]) -> list[str]:
    out: list[str] = []
    for v in fields.get("kMandarin", []):
        out.extend(v.split())  # customary reading(s) first
    out.extend(_after_colon(fields.get("kTGHZ2013") or fields.get("kHanyuPinyin") or []))
    return _dedup(out, lambda r: bool(_PINYIN.match(r)))


def parse_jyutping(fields: dict[str, list[str]]) -> list[str]:
    out: list[str] = []
    for v in fields.get("kCantonese", []):
        out.extend(v.split())  # customary reading first
    for v in fields.get("kSMSZD2003Readings", []):
        for tok in v.split():
            parts = _YUE.split(tok, 1)
            if len(parts) == 2:
                out.extend(parts[1].split(","))
    return _dedup(out, lambda r: bool(_JYUT.match(r)))


# the Unihan_Readings.txt fields we need to collect to build the full reading sets
READING_SOURCE_FIELDS = ("kMandarin", "kCantonese", "kTGHZ2013", "kHanyuPinyin", "kSMSZD2003Readings")
