//! Shared application state managed by Tauri.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};

use crate::audio::RecordingHandle;
use crate::db::Db;
use crate::enhance::CliSession;
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
    /// Set by the user's cancel button to abort an in-flight transcription: the
    /// whisper abort callback polls it and bails out of the decode. Reset every
    /// time a new transcription begins.
    cancel_transcribe: Arc<AtomicBool>,
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
    /// The AI CLI pre-spawned when the current recording began, parked until
    /// its transcript is ready (see `enhance::CliSession`).
    warm_cli: Mutex<Option<CliSession>>,
    /// Which recording may park a CLI in `warm_cli`; 0 means none may.
    warm_slot: AtomicU64,
    /// Source of `warm_slot` values, one per recording.
    warm_seq: AtomicU64,
    /// False while the settings in use are defaults from a failed load; the
    /// retention purge waits for a clean load or a save (see `Db::load_settings`).
    settings_ok: AtomicBool,
    /// Wall clock and monotonic clock at launch, to spot a forward clock jump.
    started_wall: DateTime<Utc>,
    started_mono: Instant,
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
    pub fn new(paths: AppPaths, db: Db, settings: Settings, settings_ok: bool) -> Self {
        Self {
            paths,
            db,
            settings: RwLock::new(settings),
            level: Arc::new(AtomicU32::new(0)),
            recording: Mutex::new(None),
            transcribing: AtomicBool::new(false),
            cancel_transcribe: Arc::new(AtomicBool::new(false)),
            last_toggle: Mutex::new(Instant::now()),
            downloads: Mutex::new(HashSet::new()),
            engine: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            target_window: Mutex::new(None),
            warm_cli: Mutex::new(None),
            warm_slot: AtomicU64::new(0),
            warm_seq: AtomicU64::new(0),
            settings_ok: AtomicBool::new(settings_ok),
            started_wall: Utc::now(),
            started_mono: Instant::now(),
        }
    }

    pub fn settings_trusted(&self) -> bool {
        self.settings_ok.load(Ordering::SeqCst)
    }

    pub fn mark_settings_trusted(&self) {
        self.settings_ok.store(true, Ordering::SeqCst);
    }

    /// False when the wall clock has run more than an hour ahead of the time
    /// that really passed since launch (a manual date change or a bad time
    /// sync). A purge on that clock would delete recent history. Windows'
    /// monotonic clock keeps counting through sleep, so a suspend does not trip it.
    pub fn clock_sane(&self) -> bool {
        clock_sane(self.started_wall, Utc::now(), self.started_mono.elapsed())
    }

    pub fn open_warm_slot(&self) {
        let id = self.warm_seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.warm_slot.store(id, Ordering::SeqCst);
    }

    /// The recording currently accepting a pre-spawned CLI, or 0 for none.
    pub fn warm_slot(&self) -> u64 {
        self.warm_slot.load(Ordering::SeqCst)
    }

    /// Park a pre-spawned CLI for recording `slot`. If that recording has
    /// already ended or been canceled, the session is dropped, which stops it.
    /// The check and the store share the lock `take_warm_cli` uses, so a
    /// session can't slip in after its recording was closed.
    pub fn park_warm_cli(&self, slot: u64, session: CliSession) {
        let mut parked = self.warm_cli.lock();
        if self.warm_slot.load(Ordering::SeqCst) == slot {
            *parked = Some(session);
        }
    }

    /// Close the slot and hand back the parked CLI, if any. Every path that ends
    /// a recording calls this, so a pre-spawned process never outlives it.
    pub fn take_warm_cli(&self) -> Option<CliSession> {
        let mut parked = self.warm_cli.lock();
        self.warm_slot.store(0, Ordering::SeqCst);
        parked.take()
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
    /// (panic-safe), so `is_busy` can never get permanently stuck. Also clears
    /// any stale cancel request from a previous clip.
    pub fn begin_transcribing(&self) -> TranscribeGuard<'_> {
        self.cancel_transcribe.store(false, Ordering::SeqCst);
        self.transcribing.store(true, Ordering::SeqCst);
        TranscribeGuard(&self.transcribing)
    }

    /// Request that the in-flight transcription abort (the cancel button).
    pub fn request_cancel_transcribe(&self) {
        self.cancel_transcribe.store(true, Ordering::SeqCst);
    }

    /// Whether the last/current transcription was canceled by the user.
    pub fn transcribe_canceled(&self) -> bool {
        self.cancel_transcribe.load(Ordering::SeqCst)
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
        mixed: bool,
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
            .transcribe(audio_16k, language_mode, translate, dialect, vocab, mixed, &self.cancel_transcribe);
        self.touch();
        // Leave a memory trail in the log so a slow leak over hours of heavy use
        // is visible after the fact (the "becomes unresponsive after ~2h" report).
        log::info!("transcribe finished; rss = {:.0} MB", crate::mem::rss_mb());
        result
    }
}

/// A clock that moved backwards only shrinks what a purge deletes, so it counts
/// as sane; only a forward jump beyond the real elapsed time plus an hour does not.
fn clock_sane(start_wall: DateTime<Utc>, now_wall: DateTime<Utc>, real_elapsed: Duration) -> bool {
    match (now_wall - start_wall).to_std() {
        Ok(wall_elapsed) => wall_elapsed <= real_elapsed + Duration::from_secs(3600),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_jump_forward_is_not_sane() {
        let start = Utc::now();
        let a_minute = Duration::from_secs(60);
        assert!(clock_sane(start, start + chrono::Duration::seconds(60), a_minute));
        assert!(clock_sane(start, start + chrono::Duration::minutes(50), a_minute), "drift under an hour is fine");
        assert!(!clock_sane(start, start + chrono::Duration::days(30), a_minute));
        assert!(clock_sane(start, start - chrono::Duration::days(3), a_minute), "going back deletes less");
    }
}
