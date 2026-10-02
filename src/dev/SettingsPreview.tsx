import { useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SettingsPanel from "../components/SettingsPanel";
import { I18nProvider } from "../lib/i18n";
import type { Settings } from "../lib/api";
import { fakeDownload, mockModels } from "./mockModels";

const params = new URLSearchParams(window.location.search);
const lang = params.get("lang") === "ar" ? "ar" : "en";
const fail = params.get("fail");
const models = mockModels(["small", "large-v3-turbo-q5_0"]);

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
  cancel_hotkey: "Escape",
  capture_mode: "toggle",
  auto_type: true,
  auto_copy: true,
  keep_line_breaks: false,
  sound: true,
  sound_pack: "pip",
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
  mix_arabic: false,
  ai_engine: "cli",
  cli_command: "claude",
  cli_model: "sonnet",
  cli_effort: "low",
  api_provider: "anthropic",
  api_key: "",
  api_model: "",
  api_base_url: "",
  ui_lang: lang,
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
          model: settings.model,
          custom_vocab: settings.custom_vocab,
          translate_from: settings.translate_from,
          translate_target: settings.translate_target,
          retention_days: settings.retention_days,
        }));
        return true;
      case "list_models":
        return models.map((m) => ({ ...m }));
      case "set_model": {
        if (fail === "set_model") throw "could not save the model choice";
        settings.model = (payload as { name: string }).name;
        setTimeout(() => emit("settings-changed", { ...settings }), 0);
        return null;
      }
      case "delete_model": {
        const name = (payload as { name: string }).name;
        if (name === settings.model) throw "this model is in use, switch to another model first";
        const m = models.find((x) => x.name === name);
        if (m) m.present = false;
        return null;
      }
      case "download_model":
        fakeDownload((payload as { name: string }).name, models, fail === "download");
        return null;
      case "cli_models":
        // Real probes take about a second; `&fail=cli` shows the fallback list.
        return new Promise((resolve, reject) =>
          setTimeout(() => {
            if (fail === "cli") return reject("could not start 'claude'");
            resolve({
              cli_version: "2.1.287",
              checked_at: Date.now() / 1000,
              models: [
                { alias: "haiku", id: "claude-haiku-4-5-20251001" },
                { alias: "sonnet", id: "claude-sonnet-5-5" },
                { alias: "opus", id: "claude-opus-5-5" },
                { alias: "fable", id: "claude-fable-5-1" },
              ],
              efforts: ["low", "medium", "high", "xhigh", "max"],
            });
          }, 900)
        );
      case "list_input_devices":
        return ["Headset microphone"];
      case "get_usage":
        return { input_tokens: 0, output_tokens: 0, calls: 0 };
      case "is_elevated":
        return false;
      case "plugin:app|version":
        return "1.10.1";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

/**
 * Dev-only harness (`npm run dev` + `/?preview=settings` in a browser): the
 * Settings panel with the Tauri backend mocked, so its controls can be checked
 * and screenshotted without the app. Add `&lang=ar`, `&fail=set_model` or
 * `&fail=download`. main.tsx gates it behind import.meta.env.DEV.
 */
export default function SettingsPreview() {
  const [open, setOpen] = useState(true);
  return (
    <I18nProvider lang={lang}>
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
