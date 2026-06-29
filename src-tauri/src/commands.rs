//! Tauri command surface. Thin glue: validate, call into state/modules, map
//! errors to strings the frontend can show.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::audio;
use crate::enhance;
use crate::export;
use crate::models::{
    ApiUsage, AppStatus, ModelInfo, Prompt, RecordingResult, RecordingSummary, Settings,
};
use crate::paths::AppPaths;
use crate::state::AppState;

const KNOWN_MODELS: [&str; 6] = ["tiny", "base", "small", "medium", "large-v3-turbo", "large-v3"];

/// Below this raw RMS the clip is treated as silence (a mis-click) and discarded.
/// Kept deliberately low so whispered / very quiet speech still gets through;
/// loudness normalization (audio.rs) then lifts it to a level Whisper can read.
const SILENCE_RMS: f32 = 0.0010;
/// Peak level that counts as someone actually speaking (for the silence guard).
/// Low enough that a whisper trips it, high enough to ignore room tone.
const SPEECH_LEVEL: f32 = 0.012;
/// If no speech is heard within this long after starting, auto-cancel.
const NO_SPEECH_GRACE: Duration = Duration::from_secs(15);

type R<T> = Result<T, String>;

/// Windows: spawn child processes (curl) without popping a console window.
fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn write_wav_16k(path: &Path, audio: &[f32]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio::TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in audio {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        w.write_sample(v)?;
    }
    w.finalize()?;
    Ok(())
}

/// (Re)register every global shortcut: push-to-talk plus the window<->pill toggle.
/// Both live on the one plugin, and registering calls `unregister_all()` first, so
/// they MUST be registered together (registering PTT alone would drop the toggle).
/// An empty string disables that shortcut. Returns whether PTT is now active
/// (the value `update_settings` reports back to the UI).
pub fn register_shortcuts(app: &AppHandle, ptt: &str, toggle: &str) -> bool {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut ptt_ok = true;
    // Skip a key equal to PTT so we never double-register the same accelerator.
    for (key, is_ptt) in [(ptt.trim(), true), (toggle.trim(), false)] {
        if key.is_empty() || (!is_ptt && key == ptt.trim()) {
            continue;
        }
        if let Err(e) = gs.register(key) {
            log::warn!("could not register hotkey '{key}': {e}");
            if is_ptt {
                ptt_ok = false;
            }
        }
    }
    ptt_ok
}

#[tauri::command]
pub fn list_input_devices() -> R<Vec<String>> {
    audio::list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn is_recording(state: State<AppState>) -> bool {
    state.is_recording()
}

#[tauri::command]
pub fn get_level(state: State<AppState>) -> f32 {
    state.current_level()
}

/// Collapse all whitespace (newlines included) into single spaces so auto-typed
/// text never triggers Enter/submit in a terminal, chat box, or search bar.
/// This is the key fix for prompt/polish output, which is multi-line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether the text has real content worth typing. Whisper can emit pure
/// punctuation ("...", " . . .") on weak or near-silent audio; don't inject that.
fn has_words(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

/// Map a translation target language name to an ISO code (drives RTL rendering
/// and the history badge). Falls back to English.
fn lang_code_for(target: &str) -> String {
    match target.trim().to_ascii_lowercase().as_str() {
        "arabic" => "ar",
        "english" => "en",
        "french" => "fr",
        "spanish" => "es",
        "german" => "de",
        "italian" => "it",
        "portuguese" => "pt",
        "turkish" => "tr",
        "russian" => "ru",
        "hindi" => "hi",
        "urdu" => "ur",
        "persian" | "farsi" => "fa",
        "hebrew" => "he",
        "chinese" => "zh",
        "japanese" => "ja",
        "korean" => "ko",
        _ => "en",
    }
    .to_string()
}

/// Type the text into whatever app/field currently has focus (system-wide
/// dictation). Primary path is clipboard + Ctrl+V: it inserts the whole string
/// atomically, which is reliable for long text and correct for Unicode (Arabic).
/// Synthesizing one key event per character drops characters on longer text and
/// can mangle Unicode (the "only some words / random dots" bug), so it is only a
/// fallback when the clipboard is unavailable.
#[cfg(windows)]
fn type_into_focused(text: &str, leave_on_clipboard: bool) {
    if text.is_empty() {
        return;
    }
    // Preserve the user's clipboard, paste ours, then restore it — unless we're
    // told to leave the result on the clipboard (auto-copy) for pasting elsewhere.
    let prior = clipboard::get_text();
    match clipboard::set_text(text) {
        Ok(()) => {
            clipboard::send_ctrl_v();
            // Give the target app time to read the paste before we restore.
            std::thread::sleep(std::time::Duration::from_millis(160));
            if !leave_on_clipboard {
                if let Some(prev) = prior {
                    let _ = clipboard::set_text(&prev);
                }
            }
            log::info!("auto-type: pasted {} chars", text.chars().count());
        }
        Err(e) => {
            log::warn!("auto-type: clipboard paste failed ({e}); using key events");
            send_unicode_chunked(text);
            if leave_on_clipboard {
                let _ = clipboard::set_text(text);
            }
        }
    }
}

/// Fallback: synthesize Unicode key events in small chunks with a brief pause so
/// the target app's input queue can drain (one giant SendInput drops events).
#[cfg(windows)]
fn send_unicode_chunked(text: &str) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in units.chunks(16) {
        let mut inputs: Vec<INPUT> = Vec::with_capacity(chunk.len() * 2);
        for &unit in chunk {
            for up in [false, true] {
                let mut flags = KEYEVENTF_UNICODE;
                if up {
                    flags |= KEYEVENTF_KEYUP;
                }
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: unit,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
        }
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    log::info!("auto-type: sent {} chars via key events", text.chars().count());
}

/// Minimal Windows clipboard helpers (set/get CF_UNICODETEXT) and a Ctrl+V
/// keystroke, used only by `type_into_focused`. All best-effort.
#[cfg(windows)]
mod clipboard {
    use std::time::Duration;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
    };

    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const VK_V: u16 = 0x56;

    unsafe fn open_retry() -> bool {
        for _ in 0..20 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    pub fn get_text() -> Option<String> {
        unsafe {
            if !open_retry() {
                return None;
            }
            let h = GetClipboardData(CF_UNICODETEXT);
            let result = if h.is_null() {
                None
            } else {
                let ptr = GlobalLock(h) as *const u16;
                if ptr.is_null() {
                    None
                } else {
                    let mut len = 0usize;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                    GlobalUnlock(h);
                    Some(s)
                }
            };
            CloseClipboard();
            result
        }
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        unsafe {
            // Open first: a busy clipboard is the common failure, and bailing here
            // means we never allocated, so there is nothing to leak.
            if !open_retry() {
                return Err("OpenClipboard failed".into());
            }
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = utf16.len() * std::mem::size_of::<u16>();
            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if hmem.is_null() {
                CloseClipboard();
                return Err("GlobalAlloc failed".into());
            }
            let dst = GlobalLock(hmem) as *mut u16;
            if dst.is_null() {
                CloseClipboard();
                return Err("GlobalLock failed".into());
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
            GlobalUnlock(hmem);

            EmptyClipboard();
            // On success the system takes ownership of hmem (do not free it).
            let set = SetClipboardData(CF_UNICODETEXT, hmem as _);
            CloseClipboard();
            if set.is_null() {
                return Err("SetClipboardData failed".into());
            }
            Ok(())
        }
    }

    fn key(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn send_ctrl_v() {
        let inputs = [
            key(VK_CONTROL, false),
            key(VK_V, false),
            key(VK_V, true),
            key(VK_CONTROL, true),
        ];
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }
}

#[cfg(not(windows))]
fn type_into_focused(_text: &str, _leave_on_clipboard: bool) {
    log::warn!("auto-type into the active app is only implemented on Windows");
}

/// Run the transcript through the configured AI engine (local CLI or the user's
/// own API key). On the API path, token usage is recorded for the cost meter.
fn run_enhance(state: &AppState, system: &str, text: &str) -> Result<String, String> {
    let (engine, cli_command, provider, base_url, api_key, api_model) = {
        let s = state.settings.read();
        (
            s.ai_engine.clone(),
            s.cli_command.clone(),
            s.api_provider.clone(),
            s.api_base_url.clone(),
            s.api_key.clone(),
            s.api_model.clone(),
        )
    };

    if engine == "api" {
        let r = enhance::run_api(&provider, &base_url, &api_key, &api_model, system, text)
            .map_err(|e| e.to_string())?;
        let _ = state.db.add_usage(r.input_tokens as i64, r.output_tokens as i64);
        log::info!(
            "api usage: +{} in / +{} out tokens",
            r.input_tokens,
            r.output_tokens
        );
        Ok(r.text)
    } else {
        enhance::run_cli(&cli_command, system, text).map_err(|e| e.to_string())
    }
}

/// Start capture. Shared by the command and the global hotkey handler.
pub fn begin_recording(state: &AppState, device: Option<String>) -> Result<(), String> {
    let mut slot = state.recording.lock();
    if slot.is_some() {
        return Err("already recording".into());
    }
    // Don't start a new clip while the previous one is still transcribing — it
    // would queue behind the engine lock and the two would surface seconds apart.
    if state.is_busy() {
        return Err("still transcribing the previous clip".into());
    }
    let dev = device.or_else(|| state.settings.read().input_device.clone());
    state.level.store(0, Ordering::Relaxed);
    let handle = audio::start(dev, state.level.clone()).map_err(|e| e.to_string())?;
    *slot = Some(handle);
    Ok(())
}

/// Stop capture, transcribe, save, and (optionally) type the result into the
/// focused app. Shared by the command and the global hotkey handler.
pub fn end_recording(state: &AppState, inject: bool) -> Result<Option<RecordingResult>, String> {
    let handle = state
        .recording
        .lock()
        .take()
        .ok_or_else(|| "not recording".to_string())?;
    // Hold the busy flag for the whole transcribe+type pass so a second press
    // can't start a new clip or stack another job behind the engine lock. The
    // guard clears it on every exit path, including the early silent-discard.
    let _busy = state.begin_transcribing();
    let captured = handle.stop().map_err(|e| e.to_string())?;
    state.level.store(0, Ordering::Relaxed);

    // Silent mis-click: nothing was actually said. Discard quietly — no
    // transcription, no saved row, no auto-type.
    if audio::rms_level(&captured) < SILENCE_RMS {
        log::info!("discarded recording: no speech detected");
        return Ok(None);
    }

    let frames = if captured.channels > 0 {
        captured.samples.len() / captured.channels as usize
    } else {
        captured.samples.len()
    };
    let duration_ms = if captured.sample_rate > 0 {
        (frames as f64 / captured.sample_rate as f64 * 1000.0) as i64
    } else {
        0
    };

    let denoise = state.settings.read().noise_suppression;
    let audio_16k = audio::prepare_for_whisper(&captured, denoise);

    let (model, lang_mode, dialect, vocab, auto_type, auto_copy, keep_line_breaks, voice_commands, output_mode, translate_target) = {
        let s = state.settings.read();
        (
            s.model.clone(),
            s.language_mode.clone(),
            s.dialect.clone(),
            s.custom_vocab.clone(),
            s.auto_type,
            s.auto_copy,
            s.keep_line_breaks,
            s.voice_commands,
            s.output_mode.clone(),
            s.translate_target.clone(),
        )
    };

    // Always transcribe in the spoken language first. The CLI step does the
    // language work (it handles Arabic dialects far better than Whisper's weak
    // built-in translate task), so native text is the right input for it.
    let mut transcript = state
        .transcribe(&audio_16k, &model, &lang_mode, false, &dialect, &vocab)
        .map_err(|e| e.to_string())?;
    // Language the user actually spoke (detected in auto mode, or the forced code).
    let source_lang = transcript.language.clone();

    // In Translate mode, if the spoken language already matches the target (e.g.
    // English speech with target = English) there is nothing to translate, and
    // running the model anyway tends to drift or paraphrase out of scope. Detect
    // that, skip the CLI, keep the user's own words, and raise a warning the UI +
    // pill surface so they know they're still on Translate.
    let mut translate_warning = false;
    // Set when an AI enhance is attempted but fails (offline) and we fall back to
    // the raw transcript, so the UI can tell the user their text wasn't enhanced.
    let mut enhance_failed = false;

    // translate / polish / prompt run the transcript through the local CLI.
    let cli_system: Option<String> = match output_mode.as_str() {
        "translate" => {
            if source_lang == lang_code_for(&translate_target) {
                translate_warning = true;
                None
            } else {
                Some(enhance::translate_system(&translate_target))
            }
        }
        "polish" => Some(enhance::POLISH_SYSTEM.to_string()),
        "prompt" => Some(enhance::PROMPT_SYSTEM.to_string()),
        _ => None,
    };
    if let Some(system) = cli_system {
        if has_words(&transcript.full_text) {
            match run_enhance(state, &system, &transcript.full_text) {
                Ok(out) => {
                    transcript.full_text = out;
                    transcript.segments.clear(); // enhanced output no longer maps to timings
                    // Only Translate changes the language. Clean writing / Prompt
                    // polish the user's words IN THEIR OWN language, so keep the
                    // spoken language (so RTL + Arabic font render correctly).
                    transcript.language = if output_mode == "translate" {
                        lang_code_for(&translate_target)
                    } else {
                        source_lang.clone()
                    };
                }
                Err(e) => {
                    log::warn!("{output_mode} enhance failed: {e}");
                    enhance_failed = true;
                    // Translate must still yield something useful: fall back to
                    // Whisper's offline English translate task.
                    if output_mode == "translate" {
                        if let Ok(mut t2) =
                            state.transcribe(&audio_16k, &model, &lang_mode, true, &dialect, &vocab)
                        {
                            t2.language = "en".to_string();
                            transcript = t2;
                            // Offline translate still produced English, so this
                            // isn't a raw-text fallback the user needs warning about.
                            enhance_failed = false;
                        }
                    }
                    // polish / prompt: keep the native transcript (user's words).
                }
            }
        }
    }

    // Raw mode: honor inline voice commands ("new line", "period", "comma"...) so
    // the user controls punctuation and line breaks by voice. Other modes leave
    // formatting to the LLM, so this only runs on the untouched transcript.
    if output_mode == "raw" && voice_commands {
        transcript.full_text =
            crate::text::apply_voice_commands(&transcript.full_text, &transcript.language);
    }

    // Restore Latin diacritics on English output (cafe -> café, resume -> résumé,
    // naive -> naïve...). Whisper routinely drops the accent on these loanwords;
    // a small offline dictionary puts it back. Toggleable in settings.
    if transcript.language == "en" && state.settings.read().restore_diacritics {
        transcript.full_text = crate::text::restore_diacritics(&transcript.full_text);
    }

    let stamp = Utc::now().timestamp_millis();
    let audio_path = state.paths.new_audio_path(stamp);
    write_wav_16k(&audio_path, &audio_16k).map_err(|e| e.to_string())?;
    let created_at = Utc::now().to_rfc3339();
    let audio_path_str = audio_path.to_string_lossy().into_owned();

    let id = state
        .db
        .insert_recording(
            &created_at,
            duration_ms,
            &transcript.language,
            transcript.language_confidence,
            &model,
            &audio_path_str,
            audio::TARGET_RATE,
            &transcript.full_text,
            &transcript.segments,
        )
        .map_err(|e| e.to_string())?;

    let has = has_words(&transcript.full_text);
    let did_type = inject && auto_type && has;
    if did_type {
        // A spoken "new line" command is an explicit request for a break, so keep
        // newlines when raw voice commands produced any, even if keep_line_breaks
        // (which governs incidental newlines) is off.
        let keep = keep_line_breaks
            || (output_mode == "raw" && voice_commands && transcript.full_text.contains('\n'));
        let typed = if keep {
            transcript.full_text.clone()
        } else {
            one_line(&transcript.full_text)
        };
        type_into_focused(&typed, auto_copy);
    } else if auto_copy && has {
        // Not typing into another app (in-app capture, or auto-type off): leave the
        // result on the clipboard so the user can paste it anywhere.
        #[cfg(windows)]
        {
            let _ = clipboard::set_text(&transcript.full_text);
        }
    }

    Ok(Some(RecordingResult {
        id,
        created_at,
        duration_ms,
        language: transcript.language,
        language_confidence: transcript.language_confidence,
        model,
        audio_path: audio_path_str,
        full_text: transcript.full_text,
        pinned: false,
        segments: transcript.segments,
        translate_warning,
        enhance_failed,
    }))
}

#[tauri::command]
pub fn start_recording(app: AppHandle, state: State<AppState>, device: Option<String>) -> R<()> {
    begin_recording(state.inner(), device)?;
    spawn_silence_guard(app);
    Ok(())
}

/// Watch the live level after a recording starts; if no speech is heard within
/// the grace window, auto-cancel and discard. Protects against a mis-click that
/// would otherwise keep recording silence (especially in toggle mode). Runs in
/// Rust so it works even when every window is hidden (JS timers get throttled).
pub fn spawn_silence_guard(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let start = Instant::now();
        let mut heard_speech = false;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if !state.is_recording() {
                return; // user (or transcription) already ended it
            }
            if state.current_level() >= SPEECH_LEVEL {
                heard_speech = true;
            }
            if !heard_speech && start.elapsed() >= NO_SPEECH_GRACE {
                if let Some(handle) = state.recording.lock().take() {
                    let _ = handle.stop();
                }
                state.level.store(0, Ordering::Relaxed);
                log::info!("auto-canceled recording: no speech within grace window");
                let _ = app.emit("rec-canceled", "no-speech");
                return;
            }
        }
    });
}

#[tauri::command]
pub fn cancel_recording(state: State<AppState>) -> R<()> {
    if let Some(h) = state.recording.lock().take() {
        let _ = h.stop();
    }
    state.level.store(0, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> R<Option<RecordingResult>> {
    // In-app button/Space: show in our window, do not type into another app.
    // Returns None when the clip was silent (mis-click) and got discarded.
    end_recording(state.inner(), false)
}

#[tauri::command]
pub fn list_recordings(state: State<AppState>, query: Option<String>) -> R<Vec<RecordingSummary>> {
    state
        .db
        .list_recordings(query.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recording(state: State<AppState>, id: i64) -> R<RecordingResult> {
    state.db.get_recording(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_recording(state: State<AppState>, id: i64) -> R<()> {
    let audio_path = state.db.delete_recording(id).map_err(|e| e.to_string())?;
    if let Some(p) = audio_path {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

/// Pin/unpin a recording. Pinned recordings survive the retention auto-delete.
#[tauri::command]
pub fn set_pinned(state: State<AppState>, id: i64, pinned: bool) -> R<()> {
    state.db.set_pinned(id, pinned).map_err(|e| e.to_string())
}

/// Permanently delete all recordings (rows + audio files). Returns how many
/// were removed. Behind a confirm in the UI; there is no undo.
#[tauri::command]
pub fn clear_recordings(state: State<AppState>) -> R<usize> {
    let paths = state.db.clear_all_recordings().map_err(|e| e.to_string())?;
    let n = paths.len();
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
    log::info!("cleared history: {n} recordings removed");
    Ok(n)
}

// ---- Saved prompts library ------------------------------------------------

/// Save a reusable prompt. Title defaults to the first words if not given.
#[tauri::command]
pub fn save_prompt(state: State<AppState>, text: String, title: Option<String>) -> R<i64> {
    let text = text.trim();
    if text.is_empty() {
        return Err("nothing to save".into());
    }
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let t: String = text.chars().take(60).collect();
            t
        });
    state.db.save_prompt(&title, text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_prompts(state: State<AppState>) -> R<Vec<Prompt>> {
    state.db.list_prompts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_prompt(state: State<AppState>, id: i64) -> R<()> {
    state.db.delete_prompt(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_recording(state: State<AppState>, id: i64, format: String, path: String) -> R<()> {
    let rec = state.db.get_recording(id).map_err(|e| e.to_string())?;
    export::export(&rec, &format, Path::new(&path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().clone()
}

/// Returns whether the global hotkey registered successfully.
#[tauri::command]
pub fn update_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> R<bool> {
    // Re-registering the global hotkey calls unregister_all(), which would briefly
    // kill push-to-talk. Settings saves happen on every mode / language switch, so
    // only touch the shortcut when the hotkey itself actually changed — otherwise
    // switching clean/prompt/translate silently dropped voice detection.
    let (old_ptt, old_toggle) = {
        let s = state.settings.read();
        (s.ptt_hotkey.clone(), s.toggle_hotkey.clone())
    };
    state.db.save_settings(&settings).map_err(|e| e.to_string())?;
    let ptt = settings.ptt_hotkey.clone();
    let toggle = settings.toggle_hotkey.clone();
    let retention = settings.retention_days;
    *state.settings.write() = settings;
    // Apply a shortened retention window immediately, not just on next launch.
    if let Ok(paths) = state.db.purge_older_than(retention) {
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
    }
    // Keep the tray's mode menu in sync with changes made in the UI.
    crate::refresh_tray_menu(&app);
    // Notify other windows (the floating pill) so live-applied settings like the
    // accent palette take effect everywhere without a restart.
    let _ = app.emit("settings-changed", state.settings.read().clone());
    let ok = if ptt != old_ptt || toggle != old_toggle {
        register_shortcuts(&app, &ptt, &toggle)
    } else {
        true // unchanged: leave the live shortcuts intact
    };
    Ok(ok)
}

#[tauri::command]
pub fn list_models(state: State<AppState>) -> Vec<ModelInfo> {
    KNOWN_MODELS
        .iter()
        .map(|&name| {
            let path = state.paths.find_model(name);
            let present = path.is_some();
            let size_mb = path.as_deref().map(AppPaths::audio_size_mb).unwrap_or(0.0);
            ModelInfo {
                name: name.to_string(),
                present,
                size_mb,
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_usage(state: State<AppState>) -> ApiUsage {
    let (input_tokens, output_tokens, calls) = state.db.get_usage().unwrap_or((0, 0, 0));
    ApiUsage {
        input_tokens,
        output_tokens,
        calls,
    }
}

#[tauri::command]
pub fn reset_usage(state: State<AppState>) -> R<()> {
    state.db.reset_usage().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn app_status(state: State<AppState>) -> AppStatus {
    let model = state.settings.read().model.clone();
    let model_present = state.paths.model_present(&model);
    // GPU is a compile-time choice (the `gpu-vulkan` feature). Report what this
    // build actually uses so the UI status line is honest.
    let gpu = cfg!(feature = "gpu-vulkan");
    AppStatus {
        model,
        model_present,
        gpu,
        backend: if gpu {
            "whisper.cpp (Vulkan GPU)".to_string()
        } else {
            "whisper.cpp (CPU)".to_string()
        },
    }
}

// ---- Pill / overlay window menu ------------------------------------------

/// Native right-click menu on the floating pill. Top section switches output
/// mode (raw / clean / prompt / translate) with a tick on the active one, so the
/// user can change mode from the pill without opening the window. Items are
/// handled by the app-level menu handler in `lib.rs` (ids `pill_*`).
#[tauri::command]
pub fn show_pill_menu(app: AppHandle) -> R<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};

    let mode = app.state::<AppState>().settings.read().output_mode.clone();
    let check = |id: &str, label: &str, on: bool| {
        CheckMenuItem::with_id(&app, id, label, true, on, None::<&str>).map_err(|e| e.to_string())
    };

    let raw = check("pill_mode_raw", "Raw text", mode == "raw")?;
    let polish = check("pill_mode_polish", "Clean writing", mode == "polish")?;
    let prompt = check("pill_mode_prompt", "Prompt mode", mode == "prompt")?;
    let translate = check("pill_mode_translate", "Translate to English", mode == "translate")?;

    let sep1 = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let sep2 = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;

    let open = MenuItem::with_id(&app, "pill_open", "Open EchoFlow", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let maximize = MenuItem::with_id(&app, "pill_max", "Maximize", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(&app, "pill_quit", "Close EchoFlow", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(
        &app,
        &[
            &raw, &polish, &prompt, &translate, &sep1, &open, &maximize, &sep2, &quit,
        ],
    )
    .map_err(|e| e.to_string())?;
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay.popup_menu(&menu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---- Model download -------------------------------------------------------

#[derive(Clone, Serialize)]
struct DownloadProgress {
    name: String,
    downloaded: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct DownloadEnd {
    name: String,
    message: String,
}

/// Approximate byte sizes, used only when a HEAD request can't get the real one
/// (so the progress bar still has a sensible denominator).
fn approx_model_size(name: &str) -> u64 {
    match name {
        "tiny" => 77_691_713,
        "base" => 147_951_465,
        "small" => 487_601_967,
        "medium" => 1_533_763_059,
        "large-v3-turbo" => 1_624_555_275,
        "large-v3" => 3_095_033_483,
        _ => 0,
    }
}

fn model_url(name: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin?download=true")
}

/// Read the final Content-Length via curl HEAD (follows HF's redirect to the CDN).
fn head_content_length(url: &str) -> Option<u64> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sIL", "--max-time", "30", url]);
    let out = no_window(&mut cmd).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|l| {
            let lower = l.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<u64>().ok())
        })
        .last()
}

/// Download `ggml-<name>.bin` into the app-data models dir with curl (resumable),
/// emitting progress events. Blocks; run it on a worker thread.
fn run_model_download(app: &AppHandle, name: &str, dest_dir: &Path) -> Result<(), String> {
    let file = dest_dir.join(AppPaths::model_file_name(name));
    let part = dest_dir.join(format!("{}.part", AppPaths::model_file_name(name)));
    let url = model_url(name);
    let total = head_content_length(&url).unwrap_or_else(|| approx_model_size(name));

    // Poll the partial file size and report progress until told to stop.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let app = app.clone();
        let part = part.clone();
        let name = name.to_string();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let downloaded = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                let _ = app.emit(
                    "model-download-progress",
                    DownloadProgress {
                        name: name.clone(),
                        downloaded,
                        total,
                    },
                );
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    // `-C -` resumes a partial file, which matters on a flaky connection.
    let mut curl = Command::new("curl");
    curl.args(["-L", "--fail", "--silent", "--show-error", "-C", "-", "-o"])
        .arg(&part)
        .arg(&url);
    let status = no_window(&mut curl).status();
    stop.store(true, Ordering::Relaxed);

    let status = status.map_err(|e| format!("could not start curl: {e}"))?;
    if !status.success() {
        return Err(format!(
            "download failed (curl exit {})",
            status.code().unwrap_or(-1)
        ));
    }

    std::fs::rename(&part, &file).map_err(|e| format!("could not finalize file: {e}"))?;
    let _ = app.emit(
        "model-download-progress",
        DownloadProgress {
            name: name.to_string(),
            downloaded: total,
            total,
        },
    );
    Ok(())
}

/// Start a background download of a model. Progress/done/error arrive as events.
#[tauri::command]
pub fn download_model(app: AppHandle, state: State<AppState>, name: String) -> R<()> {
    if !KNOWN_MODELS.contains(&name.as_str()) {
        return Err(format!("unknown model '{name}'"));
    }
    if state.paths.model_present(&name) {
        return Err("model already downloaded".into());
    }
    {
        let mut dl = state.downloads.lock();
        if dl.contains(&name) {
            return Err("already downloading".into());
        }
        dl.insert(name.clone());
    }
    let dest_dir = state.paths.model_dirs[0].clone();
    let app2 = app.clone();
    std::thread::spawn(move || {
        let result = run_model_download(&app2, &name, &dest_dir);
        app2.state::<AppState>().downloads.lock().remove(&name);
        match result {
            Ok(()) => {
                log::info!("model '{name}' downloaded");
                let _ = app2.emit(
                    "model-download-done",
                    DownloadEnd {
                        name,
                        message: String::new(),
                    },
                );
            }
            Err(e) => {
                log::warn!("model '{name}' download failed: {e}");
                let _ = app2.emit("model-download-error", DownloadEnd { name, message: e });
            }
        }
    });
    Ok(())
}

/// Delete a downloaded model from the app-data models dir (never the bundled dev copy).
#[tauri::command]
pub fn delete_model(state: State<AppState>, name: String) -> R<()> {
    let path = state.paths.model_dirs[0].join(AppPaths::model_file_name(&name));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
