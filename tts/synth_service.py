#!/usr/bin/env python3
"""Local Japanese TTS sidecar for Kogu — OpenJTalk (pyopenjtalk) with FORCED pitch accent.

The browser SpeechSynthesis voice can't honour Japanese pitch accent, so the Rust backend proxies
ja readings here (/api/tts/ja -> kogu /tts/ja -> this service). We synthesize the kana and override
the accent nucleus with the stored Kanjium downstep, so 箸 (atamadaka) and 橋 (odaka) sound right.

  GET /health                      -> 200 "ok"
  GET /synth?kana=はし&accent=1     -> audio/mpeg (mp3)

`accent` is the Kanjium downstep index: 0 = heiban, 1 = atamadaka, n = drop after mora n. Omit it to
let OpenJTalk pick its own (only correct for non-homographs). Output mp3s are cached on disk keyed by
(kana, accent), so repeat plays and post-deploy SW re-fetches are instant.

Runs on 127.0.0.1:4120 (loopback only); see kogu-tts.service. Single bounded dependency: pyopenjtalk.
"""
from __future__ import annotations

import copy
import hashlib
import io
import os
import re
import subprocess
import sys
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

import numpy as np
import pyopenjtalk

HOST = "127.0.0.1"
PORT = int(os.environ.get("KOGU_TTS_PORT", "4120"))
CACHE_DIR = os.environ.get("KOGU_TTS_CACHE", "/mnt/HC_Volume_102319212/kogu-tts/cache")
CACHE_MAX_FILES = 20000  # ~10-15 KB each -> a few hundred MB ceiling; oldest evicted past this
MP3_BITRATE = "64k"
KANA_RE = re.compile(r"^[぀-ヿㇰ-ㇿｦ-ﾟー]+$")

os.makedirs(CACHE_DIR, exist_ok=True)


def _cache_path(kana: str, accent: str | None) -> str:
    key = hashlib.sha1(f"{kana}|{accent or ''}".encode("utf-8")).hexdigest()
    return os.path.join(CACHE_DIR, key + ".mp3")


def _synth_wav(kana: str, accent: str | None) -> bytes:
    """Synthesize kana to 16-bit mono WAV bytes, forcing the accent nucleus when given."""
    njd = pyopenjtalk.run_frontend(kana)
    if accent is not None and njd:
        try:
            n = int(accent)
        except ValueError:
            n = None
        if n is not None and n >= 0:
            njd = copy.deepcopy(njd)
            njd[0]["acc"] = n
            # a multi-token reading (rare for a single dictionary entry): chain the rest into ONE
            # accent phrase so the single Kanjium nucleus governs the whole word.
            for e in njd[1:]:
                e["acc"] = 0
                e["chain_flag"] = 1
    labels = pyopenjtalk.make_label(njd)
    x, sr = pyopenjtalk.synthesize(labels)
    pcm = np.clip(x, -32768, 32767).astype("<i2")
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(int(sr))
        w.writeframes(pcm.tobytes())
    return buf.getvalue()


def _wav_to_mp3(wav: bytes) -> bytes:
    p = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-f", "wav", "-i", "pipe:0",
         "-codec:a", "libmp3lame", "-b:a", MP3_BITRATE, "-ac", "1", "-f", "mp3", "pipe:1"],
        input=wav, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
    )
    return p.stdout


def _evict_if_needed() -> None:
    try:
        files = [os.path.join(CACHE_DIR, f) for f in os.listdir(CACHE_DIR) if f.endswith(".mp3")]
        if len(files) <= CACHE_MAX_FILES:
            return
        files.sort(key=lambda p: os.path.getmtime(p))
        for p in files[: len(files) - CACHE_MAX_FILES]:
            try:
                os.remove(p)
            except OSError:
                pass
    except OSError:
        pass


def synth(kana: str, accent: str | None) -> bytes:
    path = _cache_path(kana, accent)
    if os.path.exists(path):
        os.utime(path, None)  # touch for LRU
        with open(path, "rb") as f:
            return f.read()
    mp3 = _wav_to_mp3(_synth_wav(kana, accent))
    tmp = path + ".tmp"
    with open(tmp, "wb") as f:
        f.write(mp3)
    os.replace(tmp, path)
    _evict_if_needed()
    return mp3


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):  # quiet; systemd journal would fill with one line per clip
        pass

    def _send(self, code: int, body: bytes, ctype: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        u = urlparse(self.path)
        if u.path == "/health":
            self._send(200, b"ok", "text/plain")
            return
        if u.path != "/synth":
            self._send(404, b"not found", "text/plain")
            return
        q = parse_qs(u.query)
        kana = (q.get("kana", [""])[0] or "").strip()
        accent = q.get("accent", [None])[0]
        if not kana or len(kana) > 32 or not KANA_RE.match(kana):
            self._send(400, b"bad kana", "text/plain")
            return
        try:
            self._send(200, synth(kana, accent), "audio/mpeg")
        except Exception as e:  # never crash the worker on one bad synth
            sys.stderr.write(f"synth failed for {kana!r}/{accent!r}: {e}\n")
            self._send(500, b"synth error", "text/plain")


def main() -> int:
    # warm the engine (loads the htsvoice) so the first real request isn't a cold ~260ms hit
    try:
        synth("あ", "0")
    except Exception as e:
        sys.stderr.write(f"warmup failed: {e}\n")
    srv = ThreadingHTTPServer((HOST, PORT), Handler)
    sys.stderr.write(f"kogu-tts listening on http://{HOST}:{PORT} (cache {CACHE_DIR})\n")
    srv.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
