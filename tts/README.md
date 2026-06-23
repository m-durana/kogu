# Kogu Japanese TTS sidecar

The browser's `SpeechSynthesis` voice cannot honour Japanese pitch accent, so Japanese readings are
synthesized locally by OpenJTalk (`pyopenjtalk`), forcing the stored **Kanjium** downstep so 箸
(atamadaka) and 橋 (odaka) sound right. The Rust backend proxies to this sidecar; the frontend falls
back to `SpeechSynthesis` when it is unavailable (offline).

```
frontend speakJa()  ->  /api/tts/ja?kana=はし&accent=1   (nginx)
                    ->  kogu /tts/ja                      (Rust proxy, src/tts.rs)
                    ->  127.0.0.1:4120/synth              (this service)
                    ->  pyopenjtalk: run_frontend -> force njd['acc'] -> make_label -> synthesize
                    ->  ffmpeg WAV->mp3 (64k mono), cached on disk by (kana, accent)
```

## Files
- `synth_service.py` — the loopback HTTP service (stdlib `http.server`, no web framework).
- `test_synth.py` — standalone asserts (no pytest dep). Run with the venv (below).
- `kogu-tts.service` — the systemd unit (installed copy lives at `/etc/systemd/system/`).

## Setup (one-time, on the host)
The venv lives on the data volume (root fs is near-full). pyopenjtalk builds from source (needs g++)
and downloads the OpenJTalk dictionary (~103 MB) into the venv on first run.

```sh
python3 -m venv /mnt/HC_Volume_102319212/kogu-tts/venv
TMPDIR=/mnt/HC_Volume_102319212/tmp \
  /mnt/HC_Volume_102319212/kogu-tts/venv/bin/pip install --no-cache-dir pyopenjtalk
sudo cp tts/kogu-tts.service /etc/systemd/system/ && sudo systemctl daemon-reload
sudo systemctl enable --now kogu-tts.service
```

Requires `ffmpeg` on PATH. Cache: `/mnt/HC_Volume_102319212/kogu-tts/cache` (bounded, LRU-evicted).

## Test
```sh
KOGU_TTS_CACHE=/tmp/kogu-tts-test \
  /mnt/HC_Volume_102319212/kogu-tts/venv/bin/python tts/test_synth.py
```

## Deploy after editing `synth_service.py`
```sh
sudo systemctl restart kogu-tts.service
```
