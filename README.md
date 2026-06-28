# Voice to Text

Free, offline desktop app that turns speech into text for **English and Arabic**. It
auto-detects the spoken language, handles noise, records by push-to-talk or a toggle, and
saves every transcript with its audio so you can search, replay, and export. Everything
runs locally with [Whisper](https://github.com/ggerganov/whisper.cpp); nothing is sent to
the cloud.

## Features

- English + Arabic, with automatic language detection per clip
- Push to talk (hold a hotkey, even when unfocused) or click to toggle
- Right-to-left rendering and an Arabic font for Arabic transcripts
- Searchable history, audio playback, and export to `.txt`, `.srt`, `.docx`
- 100% offline and private; no account, no subscription

## Requirements

- Windows (this beta), with Microsoft C++ Build Tools installed
- [Rust](https://rustup.rs) (stable, MSVC host), CMake, and LLVM/libclang on `PATH`
- Node.js 18+
- A Whisper GGML model in `src-tauri/models/` (default `ggml-small.bin`)

Download a model:

```bash
# small (~466 MB) — good English, usable Arabic
curl -L -o src-tauri/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

For stronger Arabic use `ggml-large-v3.bin`; for speed use `ggml-large-v3-turbo.bin`.

## Run

```bash
npm install
npm run tauri dev      # development with hot reload
npm run tauri build    # release bundle
```

## How it works

Capture (`cpal`) → mono + 16 kHz resample → Whisper (`whisper-rs`) → SQLite + WAV on disk.
See `docs/system.md` and `CLAUDE.md` for the architecture and module map.

## Notes

- First transcription loads the model and takes a few seconds.
- Whisper transcribes Modern Standard Arabic best; dialects are weaker and output is
  un-diacritized.
- This beta runs Whisper on the CPU. GPU acceleration is planned.
