# Kogu

A dictionary for the Han script: one word across Mandarin, Cantonese and Japanese at once,
and why the written forms differ (reforms, character mergers, readings back to Middle
Chinese). Cantonese is first-class (粵字, jyutping); everything is compiled from open
datasets, nothing is generated.

**Live at <https://kogu.miro.build>.** The site runs on a free JSON API, documented at
[kogu.miro.build/api-docs](https://kogu.miro.build/api-docs/).

## Layout

```
backend/    Rust serving binary (Axum + SQLite/FTS5), DB memory-resident
frontend/   Svelte + Vite SPA/PWA
pipeline/   offline ingestion: open sources -> one SQLite file
tts/        Japanese TTS sidecar (Open JTalk, Kanjium pitch accent, loopback-only)
deploy/     deploy script, systemd unit, nginx vhost
```

Handwriting input proxies Google Input Tools; OCR runs locally (PP-OCRv5 ONNX, in-process).

## Building

- Backend: `cd backend && cargo build --release`; tests with `cargo test --release`
  (the dev profile's OCR dependencies are huge).
- Frontend: `cd frontend && pnpm install && pnpm build`.
- Database: `pipeline/README.md` has the fetch and build order. The backend reads
  `data/kogu.sqlite` (override with `KOGU_DB`).

## Licensing

Code MIT ([`LICENSE`](LICENSE)). Dictionary database CC BY-SA 4.0, built from the open
sources credited in [`NOTICE.md`](NOTICE.md).
