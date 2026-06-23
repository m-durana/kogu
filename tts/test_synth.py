"""Tests for the Japanese TTS sidecar. Run with the tts venv (has pyopenjtalk + ffmpeg on PATH):

    KOGU_TTS_CACHE=/tmp/kogu-tts-test \
      /mnt/HC_Volume_102319212/kogu-tts/venv/bin/python tts/test_synth.py

No pytest dependency (keeps the sidecar venv lean) — plain asserts, exits non-zero on failure.
"""
import os
import sys
import tempfile

os.environ.setdefault("KOGU_TTS_CACHE", tempfile.mkdtemp(prefix="kogu-tts-test-"))

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synth_service as s  # noqa: E402


def test_kana_guard():
    assert s.KANA_RE.match("はし")
    assert s.KANA_RE.match("クウコウ")
    assert s.KANA_RE.match("こーひー")
    assert not s.KANA_RE.match("hello")
    assert not s.KANA_RE.match("漢字")


def test_wav_is_riff():
    wav = s._synth_wav("はし", "1")
    assert wav[:4] == b"RIFF" and wav[8:12] == b"WAVE", "not a WAV container"
    assert len(wav) > 1000


def test_accent_changes_audio():
    # atamadaka (1) vs heiban (0) must differ; heiban (0) and odaka (2) are identical in isolation.
    atama = s._synth_wav("はし", "1")
    heiban = s._synth_wav("はし", "0")
    odaka = s._synth_wav("はし", "2")
    assert atama != heiban, "accent 1 vs 0 should change the audio"
    assert heiban == odaka, "accent 0 vs 2 should match in isolation (no following particle)"


def test_mp3_and_cache_roundtrip():
    mp3 = s.synth("くうこう", "0")
    assert len(mp3) > 500
    # an mp3 frame sync (0xFF Ex) or an ID3 tag from lame
    assert mp3[:3] == b"ID3" or (mp3[0] == 0xFF and (mp3[1] & 0xE0) == 0xE0), "not mp3 data"
    path = s._cache_path("くうこう", "0")
    assert os.path.exists(path), "synth did not write the cache file"
    assert s.synth("くうこう", "0") == mp3, "cached read should match"


if __name__ == "__main__":
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    failed = 0
    for t in tests:
        try:
            t()
            print(f"  ok  {t.__name__}")
        except Exception as e:
            failed += 1
            print(f"FAIL  {t.__name__}: {e}")
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    sys.exit(1 if failed else 0)
