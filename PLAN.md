# Voice to Text — PLAN

## Product

A free, offline desktop app that turns speech into text for **English and Arabic**.
It auto-detects which language is being spoken, handles background noise, records by
push-to-talk or a record toggle, and saves every transcript with its audio so you can
search, replay, and export it.

**Who it is for:** anyone who dictates notes, captions, or messages in English or
Arabic and does not want a subscription or to send audio to the cloud.

**One-sentence outcome:** press a key, talk, get accurate text in the right language,
saved and exportable, with nothing leaving the machine.

## Domain glossary

- **STT / ASR** — speech to text / automatic speech recognition.
- **Whisper** — OpenAI's open-source multilingual ASR model. We run it locally.
- **GGML model** — the on-disk Whisper weight file (`ggml-small.bin`, etc.).
- **VAD** — voice activity detection; tells speech apart from silence/noise.
- **PTT** — push to talk; hold a key to record, release to transcribe.
- **Segment** — a timestamped chunk of transcript returned by Whisper.
- **MSA** — Modern Standard Arabic (Whisper's strongest Arabic).

## Architecture

Tauri 2 desktop app. Rust core does capture + inference + storage; a React webview is
the UI. See `docs/system.md` for the diagram.

- **Capture:** `cpal` on a dedicated thread (the stream is not `Send`), any device rate.
- **Prep:** downmix to mono, linear resample to 16 kHz, pad to ≥1 s.
- **Inference:** `whisper-rs` (whisper.cpp), CPU build, language `auto`/`en`/`ar`.
- **Storage:** SQLite (`rusqlite`, bundled) for transcripts + segments; WAV files on disk.
- **Search:** `LIKE` substring (predictable for mixed Arabic/English).
- **Export:** `.txt`, `.srt` (from segments), `.docx` (`docx-rs`, RTL-aware).
- **Hotkey:** `tauri-plugin-global-shortcut` emits `ptt-down` / `ptt-up` to the UI.

## Non-functional requirements

- **Offline:** no network calls at runtime. Model ships/downloaded once.
- **Private:** audio and text never leave the device.
- **Free:** no paid APIs, ever.
- **Responsive:** UI never blocks; transcription runs off the UI thread.
- **Resilient:** missing model, no mic, empty audio → clear message, no crash.
- **Bilingual + RTL:** Arabic renders right-to-left with an Arabic font.

## Tech decisions (and why)

- **whisper.cpp over faster-whisper:** no Python runtime to bundle; one Rust/C binary.
- **CPU build for beta:** no CUDA SDK dependency; compiles anywhere. GPU (Vulkan/CUDA)
  is a Phase 3 cargo-feature.
- **LIKE over FTS5:** FTS5's default tokenizer is awkward for Arabic; substring search
  is predictable across both languages for a single-user history.
- **Linear resampler over a DSP crate:** Whisper is robust to it; fewer dependencies.

## Phases and exit criteria

### v1 / Beta (this build)
Record (PTT + toggle) → transcribe locally → auto-detect language → save text + audio →
history list + search → copy + export .txt/.srt/.docx → settings (mic, model, language).
**Exit:** record an English clip and an Arabic clip, get correct transcript with the
language auto-detected, see it in history, search finds it, every export opens correctly.
Engine unit test passes on the English sample.

### Stable
On-demand model download + size picker; Silero VAD noise trimming; per-segment polish;
re-transcribe with a bigger model; RTL/bidi polish for mixed text.
**Exit:** noisy Arabic clip transcribes acceptably; settings persist; .docx opens RTL-correct.

### Production
GPU feature flag + detection; Windows installer + first-run download UX; full failure-mode
handling; rotating logs; tightened CSP; accessibility (keyboard-only) pass.
**Exit:** clean install on a fresh machine, dictate EN+AR, export, no console errors.

## Risks

- **Arabic dialects/diacritics:** Whisper favors MSA, output un-diacritized. `large-v3` > `small`.
- **Code-switching in one breath:** Whisper picks the dominant language per clip.
- **Model size/RAM:** `small` ≈ 466 MB; `large-v3` ≈ 1.5 GB, more RAM/CPU.
- **First transcription latency:** model loads on first use; UI shows a transcribing state.
