import { invoke } from "@tauri-apps/api/core";

// Mirrors the Rust serde structs returned by src-tauri/src/commands.rs.

export interface Segment {
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface RecordingResult {
  id: number;
  created_at: string;
  duration_ms: number;
  language: string;
  language_confidence: number;
  model: string;
  audio_path: string;
  full_text: string;
  pinned: boolean;
  segments: Segment[];
  // True when an AI enhance step (clean/translate/prompt) failed (usually offline)
  // and the app fell back to the raw transcript.
  enhance_failed?: boolean;
  // True when the AI reply did not look like the user's words (it answered or
  // talked about the task), so the raw transcript was kept instead.
  enhance_rejected?: boolean;
  // True when auto-type couldn't reach the origin field (window gone, or an admin
  // window blocking input). Text was left on the clipboard instead.
  paste_blocked?: boolean;
  save_failed?: boolean; // delivered, but not saved to history (id is -1)
}

export interface RecordingSummary {
  id: number;
  created_at: string;
  duration_ms: number;
  language: string;
  preview: string;
  pinned: boolean;
}

export interface Prompt {
  id: number;
  created_at: string;
  title: string;
  text: string;
}

export type OutputMode = "raw" | "translate" | "polish" | "prompt";

export interface Settings {
  input_device: string | null;
  model: string;
  language_mode: string; // "auto" | "en" | "ar" | European codes (fr, de, es, it, pt, nl)
  dialect: string; // "auto" | egyptian | levantine | gulf | iraqi | maghrebi
  custom_vocab: string; // names/jargon/brands to bias the decoder toward
  ptt_hotkey: string;
  toggle_hotkey: string; // global shortcut to flip main window <-> floating pill
  cancel_hotkey: string; // discards the live take; registered only while one is live ("" = off)
  capture_mode: "hold" | "toggle";
  auto_type: boolean;
  auto_copy: boolean;
  keep_line_breaks: boolean;
  sound: boolean;
  sound_pack: string; // cue set: pip | pip_updown (retired keys play pip)
  sound_volume: number; // 0..100
  accent: string; // accent palette key: iris | teal | amber | rose | emerald | sky
  mic_style: string; // record-button look: "orb" | "robot"
  pill_style: string; // floating pill visualizer: "wave" | "pulse" | "dots" | "minimal"
  noise_suppression: boolean;
  output_mode: OutputMode;
  translate_target: string;
  translate_from: string | null; // Translate source code; null = auto-detect
  restore_diacritics: boolean;
  voice_commands: boolean; // raw mode: spoken "new line"/"period"/... -> real punctuation
  mix_arabic: boolean; // English lock + AI step: keep spoken Arabic words for the AI to translate
  ai_engine: "cli" | "api";
  cli_command: string;
  cli_model: string; // CLI model alias: "haiku" (default) or any alias the CLI offers
  cli_effort: string; // CLI --effort: "low" (default, thinking off) .. "max"
  api_provider: "anthropic" | "openai" | "custom";
  api_key: string;
  api_model: string;
  api_base_url: string;
  ui_lang: "en" | "ar";
  always_admin: boolean; // relaunch elevated on every start (UAC prompt at launch)
  theme: string;
  retention_days: number; // auto-delete recordings older than this; 0 = keep all
  idle_unload_minutes: number; // free model from RAM after idle; 0 = keep loaded
  onboarded: boolean; // first-run walkthrough has been seen/skipped
}

export interface ApiUsage {
  input_tokens: number;
  output_tokens: number;
  calls: number;
}

export interface ModelInfo {
  name: string;
  present: boolean;
  size_mb: number;
}

export interface DownloadProgress {
  name: string;
  downloaded: number;
  total: number;
}

export interface DownloadEnd {
  name: string;
  message: string;
}

export interface CliModel {
  alias: string;
  id: string | null; // full model id the alias resolves to today; null if the probe failed
}

export interface CliModels {
  cli_version: string;
  checked_at: number;
  models: CliModel[];
  efforts: string[];
}

export interface AppStatus {
  model: string;
  model_present: boolean;
  gpu: boolean;
  backend: string;
}

export const api = {
  listInputDevices: () => invoke<string[]>("list_input_devices"),
  cliModels: (command: string, refresh: boolean) =>
    invoke<CliModels>("cli_models", { command, refresh }),
  startRecording: (device: string | null) =>
    invoke<void>("start_recording", { device }),
  stopRecording: () => invoke<RecordingResult | null>("stop_recording"),
  cancelRecording: () => invoke<void>("cancel_recording"),
  cancelTranscription: () => invoke<void>("cancel_transcription"),
  getLevel: () => invoke<number>("get_level"),
  isRecording: () => invoke<boolean>("is_recording"),
  listRecordings: (query: string | null) =>
    invoke<RecordingSummary[]>("list_recordings", { query }),
  getRecording: (id: number) =>
    invoke<RecordingResult>("get_recording", { id }),
  deleteRecording: (id: number) =>
    invoke<void>("delete_recording", { id }),
  setPinned: (id: number, pinned: boolean) =>
    invoke<void>("set_pinned", { id, pinned }),
  clearData: () => invoke<number>("clear_recordings"),
  savePrompt: (text: string, title?: string) =>
    invoke<number>("save_prompt", { text, title: title ?? null }),
  listPrompts: () => invoke<Prompt[]>("list_prompts"),
  deletePrompt: (id: number) => invoke<void>("delete_prompt", { id }),
  exportRecording: (id: number, format: "txt" | "srt" | "docx", path: string) =>
    invoke<void>("export_recording", { id, format, path }),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) =>
    invoke<boolean>("update_settings", { settings }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (name: string) => invoke<void>("download_model", { name }),
  deleteModel: (name: string) => invoke<void>("delete_model", { name }),
  setModel: (name: string) => invoke<void>("set_model", { name }),
  getUsage: () => invoke<ApiUsage>("get_usage"),
  resetUsage: () => invoke<void>("reset_usage"),
  appStatus: () => invoke<AppStatus>("app_status"),
  isElevated: () => invoke<boolean>("is_elevated"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
  showPillMenu: () => invoke<void>("show_pill_menu"),
};
