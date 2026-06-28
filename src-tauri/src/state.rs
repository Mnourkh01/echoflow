//! Shared application state managed by Tauri.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
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
    /// Names of models currently downloading, to prevent duplicate downloads.
    pub downloads: Mutex<HashSet<String>>,
    engine: Mutex<Option<WhisperEngine>>,
    /// When the model was last used, so an idle monitor can free its RAM.
    last_activity: Mutex<Instant>,
}

impl AppState {
    pub fn new(paths: AppPaths, db: Db, settings: Settings) -> Self {
        Self {
            paths,
            db,
            settings: RwLock::new(settings),
            level: Arc::new(AtomicU32::new(0)),
            recording: Mutex::new(None),
            downloads: Mutex::new(HashSet::new()),
            engine: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.lock().is_some()
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
            .transcribe(audio_16k, language_mode, translate, dialect);
        self.touch();
        result
    }
}
