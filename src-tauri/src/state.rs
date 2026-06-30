//! Shared application state managed by Tauri.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::{Mutex, RwLock};

use crate::audio::RecordingHandle;
use crate::db::Db;
use crate::models::Settings;
use crate::paths::AppPaths;
use crate::whisper::{Transcript, WhisperEngine};

pub struct AppState {
    pub paths: AppPaths,
    pub db: Db,
    pub settings: RwLock<Settings>,
    pub level: Arc<AtomicU32>,
    pub recording: Mutex<Option<RecordingHandle>>,
    /// True while a transcription is running (capture stopped, result not yet
    /// delivered). Capture's `recording` slot empties the instant we take the
    /// handle, so without this a second start/stop would slip in and stack
    /// another job behind the engine lock — the "double-tap posts twice / 10-15s
    /// freeze" bug. Set via `begin_transcribing`, cleared by the returned guard.
    transcribing: AtomicBool,
    /// Last time a toggle-mode hotkey press was acted on, to debounce double-taps.
    last_toggle: Mutex<Instant>,
    /// Names of models currently downloading, to prevent duplicate downloads.
    pub downloads: Mutex<HashSet<String>>,
    engine: Mutex<Option<WhisperEngine>>,
    /// When the model was last used, so an idle monitor can free its RAM.
    last_activity: Mutex<Instant>,
    /// The window that had focus the instant recording started (the user's real
    /// target field). Stored as the raw HWND value so it's `Send`. On stop, we
    /// restore focus here before typing, so dictation lands where the user began
    /// even if they tabbed away to a folder / page to reference something while
    /// talking. `None` means "type wherever focus is now" (capture started from
    /// our own window, or not on Windows).
    target_window: Mutex<Option<isize>>,
}

/// Clears the `transcribing` flag when dropped, so it can never get stuck on
/// even if transcription panics or returns early.
pub struct TranscribeGuard<'a>(&'a AtomicBool);

impl Drop for TranscribeGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl AppState {
    pub fn new(paths: AppPaths, db: Db, settings: Settings) -> Self {
        Self {
            paths,
            db,
            settings: RwLock::new(settings),
            level: Arc::new(AtomicU32::new(0)),
            recording: Mutex::new(None),
            transcribing: AtomicBool::new(false),
            last_toggle: Mutex::new(Instant::now()),
            downloads: Mutex::new(HashSet::new()),
            engine: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            target_window: Mutex::new(None),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.lock().is_some()
    }

    /// Remember the window that owned focus when recording began (the auto-type
    /// target). `None` clears it (type wherever focus lands).
    pub fn set_target_window(&self, hwnd: Option<isize>) {
        *self.target_window.lock() = hwnd;
    }

    /// The focus target captured at record start, if any.
    pub fn target_window(&self) -> Option<isize> {
        *self.target_window.lock()
    }

    /// True while the last clip is still being transcribed / typed out.
    pub fn is_busy(&self) -> bool {
        self.transcribing.load(Ordering::SeqCst)
    }

    /// Mark transcription as started. The returned guard clears the flag on drop
    /// (panic-safe), so `is_busy` can never get permanently stuck.
    pub fn begin_transcribing(&self) -> TranscribeGuard<'_> {
        self.transcribing.store(true, Ordering::SeqCst);
        TranscribeGuard(&self.transcribing)
    }

    /// Debounce toggle-mode hotkey presses: allow one only if the last accepted
    /// press was at least `min_gap` ago. Swallows the second tap of a double-click
    /// so it can't start-then-immediately-restart a recording.
    pub fn toggle_allowed(&self, min_gap: Duration) -> bool {
        let mut last = self.last_toggle.lock();
        if last.elapsed() < min_gap {
            return false;
        }
        *last = Instant::now();
        true
    }

    /// Mark the model as just-used, resetting the idle-unload timer.
    pub fn touch(&self) {
        *self.last_activity.lock() = Instant::now();
    }

    /// Drop the loaded model to free RAM if it has been idle longer than `idle`
    /// and nothing is recording. The next transcription reloads it lazily.
    /// `idle` of zero means "never unload".
    pub fn maybe_unload_idle(&self, idle: Duration) {
        if idle.is_zero() || self.is_recording() {
            return;
        }
        if self.last_activity.lock().elapsed() < idle {
            return;
        }
        let mut guard = self.engine.lock();
        if guard.is_some() {
            *guard = None;
            log::info!("freed idle whisper model from memory");
        }
    }

    /// Pre-load the configured model so the first transcription isn't slow.
    pub fn warmup(&self) {
        let model = self.settings.read().model.clone();
        let mut guard = self.engine.lock();
        if guard.is_some() {
            return;
        }
        if let Some(path) = self.paths.find_model(&model) {
            match WhisperEngine::load(&path, &model) {
                Ok(e) => {
                    *guard = Some(e);
                    log::info!("model '{model}' warmed up");
                }
                Err(e) => log::warn!("model warmup failed: {e}"),
            }
        }
    }

    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Transcribe, loading (or swapping) the model lazily as needed.
    pub fn transcribe(
        &self,
        audio_16k: &[f32],
        model: &str,
        language_mode: &str,
        translate: bool,
        dialect: &str,
        vocab: &str,
    ) -> Result<Transcript> {
        self.touch();
        let mut guard = self.engine.lock();
        let need_load = match guard.as_ref() {
            Some(e) => e.model_name != model,
            None => true,
        };
        if need_load {
            let path = self
                .paths
                .find_model(model)
                .ok_or_else(|| anyhow!("model '{model}' is not downloaded"))?;
            *guard = Some(WhisperEngine::load(&path, model)?);
        }
        let result = guard
            .as_ref()
            .expect("engine loaded above")
            .transcribe(audio_16k, language_mode, translate, dialect, vocab);
        self.touch();
        // Leave a memory trail in the log so a slow leak over hours of heavy use
        // is visible after the fact (the "becomes unresponsive after ~2h" report).
        log::info!("transcribe finished; rss = {:.0} MB", crate::mem::rss_mb());
        result
    }
}
