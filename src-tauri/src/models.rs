//! Serde DTOs shared between the database, the commands, and the frontend.

use serde::{Deserialize, Serialize};

pub use crate::whisper::Segment;

#[derive(Debug, Clone, Serialize)]
pub struct RecordingResult {
    pub id: i64,
    pub created_at: String,
    pub duration_ms: i64,
    pub language: String,
    pub language_confidence: f32,
    pub model: String,
    pub audio_path: String,
    pub full_text: String,
    pub pinned: bool,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingSummary {
    pub id: i64,
    pub created_at: String,
    pub duration_ms: i64,
    pub language: String,
    pub preview: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub input_device: Option<String>,
    pub model: String,
    pub language_mode: String, // "auto" | "en" | "ar"
    pub dialect: String,       // Arabic dialect prime: "auto" | egyptian | levantine | gulf | iraqi | maghrebi
    pub ptt_hotkey: String,
    pub capture_mode: String, // "hold" (push to talk) | "toggle"
    pub auto_type: bool,      // type result into the focused app
    pub auto_copy: bool,      // leave the result on the clipboard so it can be pasted anywhere
    pub keep_line_breaks: bool, // keep newlines when typing (off = one line, never presses Enter)
    pub sound: bool,          // soft start/stop chime
    pub noise_suppression: bool, // RNNoise denoise on the mic before transcription
    pub output_mode: String,  // "raw" | "translate" | "polish" | "prompt"
    pub translate_target: String, // language name to translate INTO (e.g. "English")
    pub restore_diacritics: bool, // put accents back on English loanwords (café, résumé)
    pub ai_engine: String,    // "cli" (default) | "api"
    pub cli_command: String,  // CLI used when ai_engine == "cli" (e.g. "claude")
    pub api_provider: String, // "anthropic" | "openai" | "custom"
    pub api_key: String,      // user's own key (stored locally only)
    pub api_model: String,    // model id for the chosen provider
    pub api_base_url: String, // base URL for "custom" OpenAI-compatible endpoints
    pub ui_lang: String,      // app interface language: "en" | "ar"
    pub theme: String,
    pub retention_days: i64,  // auto-delete recordings older than this; 0 = keep everything
    pub idle_unload_minutes: i64, // free the model from RAM after this idle; 0 = keep loaded
    pub onboarded: bool,      // first-run walkthrough has been seen/skipped
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input_device: None,
            model: "small".to_string(),
            language_mode: "auto".to_string(),
            dialect: "auto".to_string(),
            ptt_hotkey: "CommandOrControl+Shift+Space".to_string(),
            capture_mode: "toggle".to_string(),
            auto_type: true,
            auto_copy: true,
            keep_line_breaks: false,
            sound: true,
            noise_suppression: true,
            output_mode: "raw".to_string(),
            translate_target: "English".to_string(),
            restore_diacritics: true,
            ai_engine: "cli".to_string(),
            cli_command: "claude".to_string(),
            api_provider: "anthropic".to_string(),
            api_key: String::new(),
            api_model: String::new(),
            api_base_url: String::new(),
            ui_lang: "en".to_string(),
            theme: "dark".to_string(),
            retention_days: 14,
            idle_unload_minutes: 5,
            onboarded: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Prompt {
    pub id: i64,
    pub created_at: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub calls: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub present: bool,
    pub size_mb: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub model: String,
    pub model_present: bool,
    pub gpu: bool,
    pub backend: String,
}
