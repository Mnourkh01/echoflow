# EchoFlow

Free, offline voice to text for Windows. Hold a hotkey, speak, and your words are typed
into whatever app you're in. Speech is transcribed locally with
[Whisper](https://github.com/ggerganov/whisper.cpp); no cloud, no account, nothing leaves
your PC.

## Download

Get the latest installer from the
[releases page](https://github.com/Mnourkh01/echoflow/releases/latest). Once installed,
EchoFlow updates itself; new versions are downloaded and applied in place.

## Features

- **Offline + private.** Local Whisper, no account, no subscription, nothing uploaded.
- **English, Arabic, and European languages.** Auto-detect, or force a language. French,
  German, Spanish, Italian, Portuguese, and Dutch keep their diacritics; English output
  restores accents on common loanwords (café, résumé, naïve).
- **Arabic dialects.** Egyptian, Levantine, Gulf, Iraqi, and Maghrebi priming for spoken
  Arabic instead of forced Modern Standard.
- **Push to talk or toggle.** Hold a global hotkey anywhere (works unfocused), or press
  once to start and again to stop. In-app, hold Space.
- **Output modes.** Raw text, Clean writing, Prompt mode, or Translate to English. Switch
  from the header or by right-clicking the tray icon.
- **Types anywhere.** Result is pasted into the focused field and/or left on the clipboard.
- **History + prompts.** Every transcript is saved locally, searchable, and exportable to
  `.txt`, `.srt`, or `.docx`. Pin the ones you want to keep; save reusable prompts.
- **Right-to-left** rendering and an Arabic font for Arabic transcripts.
- **Guided onboarding** on first run, with a one-click model download.
- **Quiet-speech friendly.** Loudness normalization + noise suppression so even a whisper
  is picked up.

The Clean / Prompt / Translate modes run through a local AI CLI (`claude` by default, your
existing subscription) or your own API key. Raw transcription is always 100% offline.

## Build from source

Requirements:

- Windows with Microsoft C++ Build Tools (MSVC)
- [Rust](https://rustup.rs) (stable, MSVC host), CMake, and LLVM/libclang available to the
  build
- Node.js 18+

```bash
npm install
npm run tauri dev      # development with hot reload
npm run tauri build    # release bundle
```

A Whisper GGML model is downloaded on first run into app-data and kept across updates. For
local engine tests, drop `ggml-small.bin` (and `jfk.wav`) into `src-tauri/models/`:

```bash
curl -L -o src-tauri/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

Bigger models (`medium`, `large-v3-turbo`, `large-v3`) hear dialects and accents better at
the cost of size and CPU.

## How it works

Capture (`cpal`) → mono + 16 kHz resample (anti-aliased) → loudness normalize → Whisper
(`whisper-rs`) → optional AI clean/translate → SQLite + WAV on disk. See `CLAUDE.md` for the
module map.

## Notes

- First transcription loads the model and takes a few seconds.
- Whisper runs on the CPU in this build; GPU acceleration is planned.
