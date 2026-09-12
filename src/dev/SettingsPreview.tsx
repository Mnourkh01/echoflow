import { useState } from "react";
import { mockIPC } from "@tauri-apps/api/mocks";
import SettingsPanel from "../components/SettingsPanel";
import { I18nProvider } from "../lib/i18n";
import type { Settings } from "../lib/api";

// A real-world setup: English recognition, Translate into English, and no
// translate source chosen yet, so the panel must show Arabic as the source.
const settings: Settings = {
  input_device: null,
  model: "large-v3-turbo-q5_0",
  language_mode: "en",
  dialect: "auto",
  custom_vocab: "EchoFlow, Folowise",
  ptt_hotkey: "CommandOrControl+Shift+Space",
  toggle_hotkey: "CommandOrControl+Shift+E",
  capture_mode: "toggle",
  auto_type: true,
  auto_copy: true,
  keep_line_breaks: false,
  sound: true,
  sound_pack: "soft",
  sound_volume: 70,
  accent: "iris",
  mic_style: "orb",
  pill_style: "wave",
  noise_suppression: true,
  output_mode: "translate",
  translate_target: "English",
  translate_from: null,
  restore_diacritics: true,
  voice_commands: false,
  ai_engine: "cli",
  cli_command: "claude",
  cli_model: "sonnet",
  api_provider: "anthropic",
  api_key: "",
  api_model: "",
  api_base_url: "",
  ui_lang: "en",
  always_admin: false,
  theme: "dark",
  retention_days: 7,
  idle_unload_minutes: 15,
  onboarded: true,
};

mockIPC(
  (cmd, payload) => {
    switch (cmd) {
      case "get_settings":
        return { ...settings };
      case "update_settings":
        Object.assign(settings, (payload as { settings: Settings }).settings);
        console.info("update_settings", JSON.stringify({
          translate_from: settings.translate_from,
          translate_target: settings.translate_target,
          retention_days: settings.retention_days,
        }));
        return true;
      case "list_models":
        return [
          { name: "large-v3-turbo-q5_0", present: true, size_mb: 547 },
          { name: "small", present: false, size_mb: 466 },
        ];
      case "list_input_devices":
        return ["Headset microphone"];
      case "get_usage":
        return { input_tokens: 0, output_tokens: 0, calls: 0 };
      case "is_elevated":
        return false;
      case "plugin:app|version":
        return "1.7.0";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

/**
 * Dev-only harness (`npm run dev` + `/?preview=settings` in a browser): the
 * Settings panel with the Tauri backend mocked, so its controls can be checked
 * and screenshotted without the app. main.tsx gates it behind import.meta.env.DEV.
 */
export default function SettingsPreview() {
  const [open, setOpen] = useState(true);
  return (
    <I18nProvider lang="en">
      <button
        className="m-4 rounded-lg border border-white/10 bg-white/[0.06] px-3 py-1.5 text-xs text-ink-200"
        onClick={() => setOpen(true)}
      >
        Open settings
      </button>
      <SettingsPanel open={open} onClose={() => setOpen(false)} onSaved={() => setOpen(false)} />
    </I18nProvider>
  );
}
