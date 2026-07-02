# CLAUDE.md — Voice to Text

Offline English/Arabic voice-to-text desktop app. Tauri 2 (Rust core) + React/TS webview.
Whisper (whisper.cpp via `whisper-rs`) runs locally on CPU. No cloud, no paid APIs.

## Stack

- **Shell:** Tauri 2.x
- **Frontend:** React 18 + TypeScript + Vite, Tailwind 3, `lucide-react` icons
- **STT:** `whisper-rs` (whisper.cpp), GGML models, CPU build
- **Audio:** `cpal` capture + custom mono/16 kHz resample (`src-tauri/src/audio.rs`)
- **DB:** `rusqlite` (bundled SQLite)
- **Export:** `docx-rs` (.docx), hand-built .srt, plain .txt

## Layout

```
src/                     React UI (App, components, lib/api.ts)
src-tauri/src/
  audio.rs    capture (dedicated thread) + resample
  whisper.rs  WhisperEngine (load + transcribe + lang detect) + unit test
  db.rs       SQLite schema + queries
  export.rs   txt / srt / docx
  models.rs   serde DTOs shared with the frontend
  paths.rs    model/audio/db locations
  state.rs    AppState (managed by Tauri)
  commands.rs #[tauri::command] surface
  lib.rs      builder, plugins, setup, invoke_handler
src-tauri/models/        GGML model(s), git-ignored; jfk.wav for the engine test
```

## Commands

```bash
# dev (hot reload)
npm run tauri dev

# build a release bundle
npm run tauri build

# frontend only
npm run dev
npm run build

# Rust: engine unit test (skips itself if model/sample absent)
cd src-tauri && cargo test
cd src-tauri && cargo build
```

Toolchain lives outside the repo under `.tools/` (Rust via rustup, portable CMake) plus
LLVM in `%LOCALAPPDATA%\llvm` (libclang for bindgen). MSVC build tools are required and
already installed on this machine.

## Conventions

- Rust modules `audio`/`whisper`/`db`/`export` stay Tauri-independent (so the engine is
  unit-testable without a window). Tauri wiring lives only in `commands.rs` + `lib.rs`.
- Commands return `Result<T, String>`; map `anyhow` errors at the boundary.
- The cpal `Stream` is `!Send` — never put it in `AppState`; it lives on its thread.
- Whisper needs **16 kHz mono f32**. Always go through `audio::prepare_for_whisper`.
- Language codes: `auto` | `en` | `ar`. RTL set is `ar/he/fa/ur` (see `src/lib/format.ts`
  and `export.rs`).

## Gotchas

- First transcription loads the model (a few seconds); the UI shows a transcribing state.
- Global PTT hotkey defaults to `CommandOrControl+Shift+Space`; in-app hold **Space** also works.
- Models resolve from app-data `models/`, then exe-dir `models/`, then the source
  `src-tauri/models/` (dev). Filenames are `ggml-<name>.bin`.
- CSP is configured in `tauri.conf.json` (separate dev/prod policies).
- GPU is off (CPU build). GPU is a Phase 3 cargo feature (Vulkan/CUDA).
- Auto language detect is constrained to the settings-offered set (`AUTO_LANGS`
  in `whisper.rs`) so dialectal Arabic can't misdetect as Farsi/Urdu.
- Custom vocabulary = decoder bias (initial prompt) + deterministic fuzzy
  post-correction (`text::apply_custom_vocab`) + a preserve-spellings line in
  the enhance system prompts.
- Esc / the X button cancels a take: while recording it discards the clip;
  while transcribing it aborts the decode via the whisper abort callback.
- `?preview=mascot` on the Vite dev server shows a browser-only harness for the
  record control (dev builds only).

## Phases

See `PLAN.md`. Current: **v1 / Beta**.
