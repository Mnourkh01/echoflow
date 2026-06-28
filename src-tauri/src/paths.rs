//! Filesystem locations for models, recorded audio, and the database.

use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct AppPaths {
    /// Base app-data directory; kept for diagnostics and future use.
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub audio_dir: PathBuf,
    pub db_path: PathBuf,
    /// Directories searched for `ggml-<name>.bin`, in priority order.
    pub model_dirs: Vec<PathBuf>,
}

impl AppPaths {
    pub fn new(data_dir: PathBuf) -> std::io::Result<Self> {
        let audio_dir = data_dir.join("audio");
        let db_path = data_dir.join("voice.db");
        std::fs::create_dir_all(&audio_dir)?;

        let mut model_dirs = vec![data_dir.join("models")];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                model_dirs.push(dir.join("models"));
            }
        }
        // Dev fallback: the models folder shipped in the source tree.
        model_dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"));

        std::fs::create_dir_all(&model_dirs[0])?;
        Ok(Self {
            data_dir,
            audio_dir,
            db_path,
            model_dirs,
        })
    }

    pub fn model_file_name(model: &str) -> String {
        format!("ggml-{model}.bin")
    }

    /// First existing path for the named model, if any.
    pub fn find_model(&self, model: &str) -> Option<PathBuf> {
        let file = Self::model_file_name(model);
        self.model_dirs
            .iter()
            .map(|d| d.join(&file))
            .find(|p| p.exists())
    }

    pub fn model_present(&self, model: &str) -> bool {
        self.find_model(model).is_some()
    }

    pub fn new_audio_path(&self, stamp_millis: i64) -> PathBuf {
        self.audio_dir.join(format!("rec-{stamp_millis}.wav"))
    }

    pub fn audio_size_mb(path: &Path) -> f64 {
        std::fs::metadata(path)
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    }
}
