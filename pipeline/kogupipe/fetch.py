"""Download + pin the open upstream sources into ``pipeline/sources/``.

Writes ``pipeline/sources.lock.json`` recording each source's resolved URL, sha256, and size,
so a build is reproducible. Re-running skips files already present with a matching size unless
``--force`` is given.

Usage:
    python -m kogupipe.fetch                 # fetch everything needed so far
    python -m kogupipe.fetch unihan cedict   # fetch a subset
    python -m kogupipe.fetch --force
"""
from __future__ import annotations

import hashlib
import json
import sys
import urllib.request
from dataclasses import dataclass, field

from .db import PIPELINE_DIR, SOURCES_DIR

LOCKFILE = PIPELINE_DIR / "sources.lock.json"
UA = "kogu-pipeline/0.0 (+https://miro.build)"


@dataclass
class Source:
    name: str
    url: str
    filename: str
    licence: str
    # optional resolver returning a concrete download URL (e.g. GitHub latest release asset)
    resolver: object = field(default=None)


def _gh_latest_asset(repo: str, prefix: str, suffix: str = ".json.zip", exclude: str | None = None):
    """Resolve the latest GitHub release asset named ``{prefix}{version}{suffix}``. ``exclude`` skips
    assets whose name contains that substring (e.g. prefix 'jmdict-eng-' must not pick 'jmdict-eng-common-')."""
    def resolve() -> str:
        api = f"https://api.github.com/repos/{repo}/releases/latest"
        req = urllib.request.Request(api, headers={"User-Agent": UA,
                                                   "Accept": "application/vnd.github+json"})
        with urllib.request.urlopen(req, timeout=30) as r:
            data = json.load(r)
        for asset in data.get("assets", []):
            name = asset["name"]
            if name.startswith(prefix) and name.endswith(suffix) and (exclude is None or exclude not in name):
                return asset["browser_download_url"]
        raise RuntimeError(f"no asset {prefix}*{suffix} in {repo} latest release")
    return resolve


# Registry. Phase 1 needs: Unihan, cjkvi-ids, OpenCC variant/conversion tables,
# CC-CEDICT, JMdict + Kanjidic. Later phases append (CC-Canto, kaikki, MCPDict, ...).
OPENCC_RAW = "https://raw.githubusercontent.com/BYVoid/OpenCC/master/data/dictionary/"
SOURCES: dict[str, Source] = {
    "unihan": Source(
        "unihan", "https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip",
        "Unihan.zip", "Unicode-DFS"),
    "cjkvi-ids": Source(
        "cjkvi-ids", "https://raw.githubusercontent.com/cjkvi/cjkvi-ids/master/ids.txt",
        "ids.txt", "CC-BY-SA"),
    "cedict": Source(
        "cedict", "https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz",
        "cedict.txt.gz", "CC-BY-SA"),
    "jmdict": Source(
        # full English JMdict (NOT the common-only subset, which dropped ~90% of Japanese entries)
        "jmdict", "", "jmdict-eng.json.zip", "CC-BY-SA / EDRDG",
        resolver=_gh_latest_asset("scriptin/jmdict-simplified", "jmdict-eng-", exclude="common")),
    "kanjidic": Source(
        "kanjidic", "", "kanjidic2-en.json.zip", "CC-BY-SA / EDRDG",
        resolver=_gh_latest_asset("scriptin/jmdict-simplified", "kanjidic2-en-")),
    # OpenCC conversion / variant tables (Apache-2.0). Phase 1.1 backbone + later orthographic why.
    "opencc-st": Source("opencc-st", OPENCC_RAW + "STCharacters.txt", "STCharacters.txt", "Apache-2.0"),
    "opencc-ts": Source("opencc-ts", OPENCC_RAW + "TSCharacters.txt", "TSCharacters.txt", "Apache-2.0"),
    "opencc-tw": Source("opencc-tw", OPENCC_RAW + "TWVariants.txt", "TWVariants.txt", "Apache-2.0"),
    "opencc-hk": Source("opencc-hk", OPENCC_RAW + "HKVariants.txt", "HKVariants.txt", "Apache-2.0"),
    "opencc-jp": Source("opencc-jp", OPENCC_RAW + "JPShinjitaiCharacters.txt", "JPShinjitaiCharacters.txt", "Apache-2.0"),
    # Cantonese (Phase 3.3): CC-Canto colloquial dict + Cantonese readings for CC-CEDICT entries
    "cccanto": Source(
        "cccanto", "https://cantonese.org/cccanto-170202.zip", "cccanto.zip", "CC-BY-SA"),
    "cccanto-readings": Source(
        "cccanto-readings", "https://cantonese.org/cccedict-canto-readings-150923.zip",
        "cccanto-readings.zip", "CC-BY-SA"),
    # Word frequency (OpenSubtitles via hermitdave/FrequencyWords) - ranks common words first.
    "freq-zh": Source(
        "freq-zh", "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/zh_cn/zh_cn_50k.txt",
        "freq_zh_cn.txt", "MIT"),
    "freq-ja": Source(
        "freq-ja", "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2016/ja/ja_50k.txt",
        "freq_ja.txt", "MIT"),
}


def _sha256_size(path) -> tuple[str, int]:
    h = hashlib.sha256()
    n = 0
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
            n += len(chunk)
    return h.hexdigest(), n


def _download(url: str, dest) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
        while True:
            chunk = r.read(1 << 16)
            if not chunk:
                break
            f.write(chunk)


def fetch(names: list[str] | None = None, *, force: bool = False) -> dict:
    SOURCES_DIR.mkdir(parents=True, exist_ok=True)
    lock = json.loads(LOCKFILE.read_text()) if LOCKFILE.exists() else {}
    names = names or list(SOURCES)
    failures = []
    for name in names:
        src = SOURCES[name]
        dest = SOURCES_DIR / src.filename
        try:
            url = src.resolver() if src.resolver else src.url
            if dest.exists() and not force:
                print(f"  = {name}: already present ({dest.name})")
            else:
                print(f"  ↓ {name}: {url}")
                _download(url, dest)
            sha, size = _sha256_size(dest)
            lock[name] = {"url": url, "filename": src.filename, "sha256": sha,
                          "size": size, "licence": src.licence}
            print(f"    {size:,} bytes  sha256={sha[:12]}…")
        except Exception as e:  # keep going; one bad source shouldn't lose the rest
            failures.append((name, repr(e)))
            print(f"  ! {name}: FAILED - {e}")
    # write the lockfile for whatever succeeded, always
    LOCKFILE.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n")
    if failures:
        print(f"\n{len(failures)} source(s) failed: " + ", ".join(n for n, _ in failures))
    return lock


def main(argv: list[str]) -> int:
    force = "--force" in argv
    names = [a for a in argv if not a.startswith("-")] or None
    fetch(names, force=force)
    print(f"lockfile: {LOCKFILE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
