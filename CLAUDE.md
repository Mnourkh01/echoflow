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

Toolchain lives outside the repo under `.tools/` (Rust via rustup, portable CMake).
whisper-rs-sys needs libclang for bindgen: set `LIBCLANG_PATH` (release.ps1 falls back
to the pip wheel's `clang\native`) and pin `CMAKE_GENERATOR="Visual Studio 16 2019"`,
or a fresh clone fails with "Unable to find libclang". MSVC build tools are required
and already installed on this machine.

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
- `large-v3-turbo-q5_0` (547 MB quantized) is the default quality model: near
  large-v3-turbo accuracy, and on the Vulkan GPU build an 11 s clip decodes in
  ~0.8 s (vs 15 s CPU). `small` remains the light fallback. Benchmark any
  candidate with
  `BENCH_MODEL_PATH=... cargo test --release bench_model -- --ignored --nocapture`.
- Whisper threads cap at 8: measured on the 12700H hybrid CPU, 10 threads is
  the plateau and 16 is 2x SLOWER (E-core spin contention). `ECHOFLOW_THREADS`
  overrides for experiments.
- CSP is configured in `tauri.conf.json` (separate dev/prod policies).
- Release builds ship with `--features gpu-vulkan` (whisper.cpp Vulkan on the
  RTX 3050; ~19x faster than CPU on large models, auto CPU fallback if no
  Vulkan device). Building it needs `VULKAN_SDK=C:\VulkanSDK\mini` + glslc on
  PATH (a hand-assembled mini SDK; see release.ps1 and the build notes memory).
  Plain `cargo build`/`cargo test` stay CPU and need no Vulkan SDK.
- Auto language detect is constrained to the settings-offered set (`AUTO_LANGS`
  in `whisper.rs`) so dialectal Arabic can't misdetect as Farsi/Urdu.
- Custom vocabulary = decoder bias (initial prompt) + deterministic fuzzy
  post-correction (`text::apply_custom_vocab`) + a preserve-spellings line in
  the enhance system prompts.
- Enhance modes on the CLI engine pre-spawn `claude` the moment recording
  starts (`commands::prepare_while_recording`), so its ~1.5 s startup overlaps
  the speech; the result is taken from the stream-json `result` event, not
  process exit. The parked process must use `--input-format stream-json`:
  plain text stdin gives up after 3 s with no input. The child also gets
  `MAX_THINKING_TOKENS=0` (Haiku 4.5 otherwise thinks ~4 s on a grammar fix
  even at `--effort low`) and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
  (otherwise a hidden extra Haiku call rides along). Every path that ends a
  recording must call `take_warm_cli()` so the process never lingers.
  Recording start also reloads the Whisper model if the idle monitor freed it.
- Language lock: with the recognition language set to `en` or `ar`, Clean writing
  and Prompt mode write only that language and translate stray words
  (`enhance::Lock`); Auto keeps the spoken language; other forced codes act like
  Auto. Raw mode has no AI step, so it cannot translate stray words.
- Translate listens for `translate_from` (`commands::effective_translate_from`:
  unset means Arabic when the target is English, else auto), never the
  recognition language. The offline Whisper-translate fallback is skipped for
  turbo models (never trained to translate) and refused if it comes back Arabic.
- Enhance prompts are built only through `EnhanceCfg::from_settings`, so the
  pre-spawned CLI's prompt matches the one used at the end. Every reply passes
  `enhance::check_reply` before it is typed: a meta reply ("I can only rewrite
  transcripts...") or an answer-shaped one keeps the raw transcript and sets
  `enhance_rejected`. Re-check trick dictations on Sonnet AND Haiku after any
  prompt change (`cargo test --lib dump_prompts -- --ignored` exports them).
- Quiet speech: `audio::speech_present` (30 ms frames) is the silence gate, the VAD
  floor is 4e-4 on the 95th-percentile frame, `SPEECH_LEVEL` is 0.003. Re-run
  `whisper::tests::replay_quiet_speech` after touching any of them.
- Retention purge runs at launch, on settings save and every idle-monitor minute;
  it is skipped while the settings failed to load or the wall clock jumped ahead
  of real elapsed time.
- `?preview=settings` on the Vite dev server shows the Settings panel with the
  Tauri IPC mocked (dev builds only).
- Esc / the X button cancels a take: while recording it discards the clip;
  while transcribing it aborts the decode via the whisper abort callback, and a
  cancel during the AI step drops the take before it is saved or typed. The
  cancel key (`cancel_hotkey`, default Escape) is a global shortcut held only
  while a take is live (`spawn_cancel_key_guard`, one thread per take), so it
  works in pill mode. The shortcut handler runs on the main thread under the
  plugin's lock: it must never call the plugin, and the guard never holds an
  AppState lock across a plugin call. `stop_recording` is an async command so an
  in-app decode does not hold the main thread.
- `?preview=mascot` on the Vite dev server shows a browser-only harness for the
  record control (dev builds only). `?preview=first-run` does the same for the
  first-run model picker (`&lang=ar`, `&present=small`, `&fail=download|set_model`).
- Model catalog (friendly names, one-line descriptions, sizes, the recommended
  pick) lives in `src/lib/models.ts` + i18n; ids stay the ggml file names and the
  Rust `KNOWN_MODELS` allowlist. Switching models goes through `set_model`
  (changes only `model`, never purges, saves only trusted settings), never
  `update_settings`. `delete_model` refuses the model in use.
- Start/stop sounds are two pip packs the user picked by ear on 2026-09-12 out of
  forty candidates; do not add or swap a pack he has not heard. They are files in
  `src/assets/sounds/`, rendered by `scripts/render-sounds.py` (numpy + ffmpeg)
  and mastered to -11 LUFS momentary max, true peak under -1 dBTP. Re-run the
  script after a design change; do not tune loudness in `sound.ts`, whose curve
  puts the default 70% at the file level and 100% at about +4 dB into a limiter.
  Saved retired pack keys (soft, chime, custom...) play `pip`.

## Phases

See `PLAN.md`. Current: **v1 / Beta**.
