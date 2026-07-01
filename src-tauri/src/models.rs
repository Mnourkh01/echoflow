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
    /// Set only for a fresh Translate-mode dictation where the spoken language
    /// already matched the target (e.g. English speech with target = English).
    /// The translate step is skipped (the native words are kept) and the UI +
    /// pill flash a "still on Translate" warning. Always false for stored rows.
    pub translate_warning: bool,
    /// True when an AI enhance step (clean / translate / prompt) was requested
    /// but failed — almost always a lost connection — so we fell back to the raw
    /// transcript. Lets the UI tell the user their words are raw, not enhanced.
    /// Always false for stored rows.
    pub enhance_failed: bool,
    /// True when auto-type couldn't deliver into the origin field — the window
    /// the user started in is gone, or it's an elevated app (Task Manager and
    /// other admin windows block input from a normal-rights app). The text is
    /// left on the clipboard instead, and the UI tells the user to paste (or to
    /// restart as administrator). Always false for stored rows.
    pub paste_blocked: bool,
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
    pub custom_vocab: String,  // user's names/jargon/brands to bias the decoder toward (comma/newline separated)
    pub ptt_hotkey: String,
    pub toggle_hotkey: String, // global shortcut to flip the main window <-> floating pill
    pub capture_mode: String,  // "hold" (push to talk) | "toggle"
    pub auto_type: bool,      // type result into the focused app
    pub auto_copy: bool,      // leave the result on the clipboard so it can be pasted anywhere
    pub keep_line_breaks: bool, // keep newlines when typing (off = one line, never presses Enter)
    pub sound: bool,          // soft start/stop chime on/off
    pub sound_pack: String,   // which chime set: "soft" | "marimba" | "glass" | "pop" | "chime"
    pub sound_volume: i64,    // chime loudness 0..100
    pub accent: String,       // accent palette key: "iris" | "teal" | "amber" | "rose" | "emerald" | "sky"
    pub mic_style: String,    // record-button look: "orb" (glass sphere) | "robot" (animated mascot)
    pub pill_style: String,   // floating pill visualizer: "wave" | "pulse" | "dots" | "minimal"
    pub noise_suppression: bool, // RNNoise denoise on the mic before transcription
    pub output_mode: String,  // "raw" | "translate" | "polish" | "prompt"
    pub translate_target: String, // language name to translate INTO (e.g. "English")
    pub restore_diacritics: bool, // put accents back on English loanwords (café, résumé)
    pub voice_commands: bool,  // raw mode: spoken "new line"/"period"/... become real punctuation
    pub ai_engine: String,    // "cli" (default) | "api"
    pub cli_command: String,  // CLI used when ai_engine == "cli" (e.g. "claude")
    pub cli_model: String,    // model for the CLI path: "haiku" (default) | "sonnet" | "opus"
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
            custom_vocab: String::new(),
            ptt_hotkey: "CommandOrControl+Shift+Space".to_string(),
            toggle_hotkey: "CommandOrControl+Shift+E".to_string(),
            capture_mode: "toggle".to_string(),
            auto_type: true,
            auto_copy: true,
            keep_line_breaks: false,
            sound: true,
            sound_pack: "soft".to_string(),
            sound_volume: 70,
            accent: "iris".to_string(),
            mic_style: "orb".to_string(),
            pill_style: "wave".to_string(),
            // Default OFF: a real VAD now trims silence, and raw mic into Whisper
            // is cleaner for most rooms. Still a toggle for noisy environments.
            noise_suppression: false,
            output_mode: "raw".to_string(),
            translate_target: "English".to_string(),
            restore_diacritics: true,
            voice_commands: false,
            ai_engine: "cli".to_string(),
            cli_command: "claude".to_string(),
            cli_model: "haiku".to_string(),
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
