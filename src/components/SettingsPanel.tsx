import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { X, Trash2, AlertTriangle, RefreshCw } from "lucide-react";
import type { ApiUsage, DownloadEnd, DownloadProgress, ModelInfo, OutputMode, Settings } from "../lib/api";
import { api } from "../lib/api";
import { checkForUpdate, installUpdate } from "../lib/updater";
import { translate, type Lang, type StringKey } from "../lib/i18n";

// Model order + approximate download sizes (MB) shown before a model is present.
const KNOWN_MODELS = ["tiny", "base", "small", "medium", "large-v3-turbo", "large-v3"] as const;
const MODEL_MB: Record<string, number> = {
  tiny: 74,
  base: 141,
  small: 465,
  medium: 1462,
  "large-v3-turbo": 1549,
  "large-v3": 2952,
};
const fmtSize = (mb: number) =>
  mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
const pct = (p: DownloadProgress) =>
  p.total > 0 ? Math.min(100, Math.floor((p.downloaded / p.total) * 100)) : 0;

// Translation targets. Value is the English name (sent to the model); label is
// the user-facing native name.
const LANGUAGES: { value: string; label: string }[] = [
  { value: "English", label: "English" },
  { value: "Arabic", label: "العربية" },
  { value: "French", label: "Français" },
  { value: "Spanish", label: "Español" },
  { value: "German", label: "Deutsch" },
  { value: "Italian", label: "Italiano" },
  { value: "Portuguese", label: "Português" },
  { value: "Turkish", label: "Türkçe" },
  { value: "Russian", label: "Русский" },
  { value: "Hindi", label: "हिन्दी" },
  { value: "Urdu", label: "اردو" },
  { value: "Persian", label: "فارسی" },
  { value: "Hebrew", label: "עברית" },
  { value: "Chinese", label: "中文" },
  { value: "Japanese", label: "日本語" },
  { value: "Korean", label: "한국어" },
];

// Retention windows offered to the user. 0 = keep everything.
const RETENTION: [number, StringKey][] = [
  [7, "retain_1w"],
  [14, "retain_2w"],
  [30, "retain_1m"],
  [0, "retain_forever"],
];

// Idle windows before the model is freed from RAM. 0 = keep loaded.
const IDLE_UNLOAD: [number, StringKey][] = [
  [5, "idle_5"],
  [15, "idle_15"],
  [0, "idle_never"],
];

// Arabic dialect primes. value matches the Rust `dialect` setting.
const DIALECTS: [string, StringKey][] = [
  ["auto", "dialect_auto"],
  ["egyptian", "dialect_egyptian"],
  ["levantine", "dialect_levantine"],
  ["gulf", "dialect_gulf"],
  ["iraqi", "dialect_iraqi"],
  ["maghrebi", "dialect_maghrebi"],
];

interface Props {
  open: boolean;
  onClose: () => void;
  onSaved: (s: Settings) => void;
  onDataCleared?: () => void;
}

export default function SettingsPanel({ open, onClose, onSaved, onDataCleared }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [warn, setWarn] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testLevel, setTestLevel] = useState(0);
  const [usage, setUsage] = useState<ApiUsage | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [cleared, setCleared] = useState(false);
  const [version, setVersion] = useState("");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const settingsLangRef = useRef<Lang>("en");
  settingsLangRef.current = settings?.ui_lang ?? "en";
  const testingRef = useRef(false);
  testingRef.current = testing;

  const refreshModels = () => api.listModels().then(setModels).catch(() => {});

  useEffect(() => {
    if (!open) return;
    api.getSettings().then(setSettings);
    api.listInputDevices().then(setDevices).catch(() => setDevices([]));
    api.getUsage().then(setUsage).catch(() => {});
    getVersion().then(setVersion).catch(() => {});
    setConfirmClear(false);
    setCleared(false);
    setUpdateMsg(null);
    refreshModels();
  }, [open]);

  // Live model-download events from the backend.
  useEffect(() => {
    if (!open) return;
    const offProg = listen<DownloadProgress>("model-download-progress", (e) =>
      setProgress((p) => ({ ...p, [e.payload.name]: e.payload }))
    );
    const offDone = listen<DownloadEnd>("model-download-done", (e) => {
      setProgress((p) => {
        const rest = { ...p };
        delete rest[e.payload.name];
        return rest;
      });
      refreshModels();
    });
    const offErr = listen<DownloadEnd>("model-download-error", (e) => {
      setProgress((p) => {
        const rest = { ...p };
        delete rest[e.payload.name];
        return rest;
      });
      setWarn(`${translate(settingsLangRef.current, "download_failed")}: ${e.payload.message}`);
    });
    return () => {
      offProg.then((f) => f());
      offDone.then((f) => f());
      offErr.then((f) => f());
    };
  }, [open]);

  // Lock the page behind the dialog: only the settings list may scroll.
  useEffect(() => {
    if (!open) return;
    const onWheel = (e: WheelEvent) => {
      const list = scrollRef.current;
      if (list && list.contains(e.target as Node)) return;
      e.preventDefault();
    };
    window.addEventListener("wheel", onWheel, { passive: false });
    return () => window.removeEventListener("wheel", onWheel);
  }, [open]);

  // Live level while testing the mic.
  useEffect(() => {
    if (!testing) return;
    const id = setInterval(async () => {
      try {
        setTestLevel(await api.getLevel());
      } catch {
        /* ignore */
      }
    }, 80);
    return () => clearInterval(id);
  }, [testing]);

  // Stop any mic test if the panel closes/unmounts.
  useEffect(() => {
    return () => {
      if (testingRef.current) api.cancelRecording().catch(() => {});
    };
  }, []);

  if (!open || !settings) return null;

  // Bind the translator to the language being edited so the panel updates live.
  const lang: Lang = settings.ui_lang;
  const t = (k: StringKey) => translate(lang, k);

  function patch(p: Partial<Settings>) {
    setSettings((s) => (s ? { ...s, ...p } : s));
  }

  function startDownload(name: string) {
    setWarn(null);
    setProgress((p) => ({ ...p, [name]: { name, downloaded: 0, total: 0 } }));
    api.downloadModel(name).catch((e) => {
      setProgress((p) => {
        const rest = { ...p };
        delete rest[name];
        return rest;
      });
      setWarn(String(e));
    });
  }

  function removeModel(name: string) {
    api.deleteModel(name).then(refreshModels).catch((e) => setWarn(String(e)));
  }

  async function clearHistory() {
    try {
      await api.clearData();
      setConfirmClear(false);
      setCleared(true);
      onDataCleared?.();
      window.setTimeout(() => setCleared(false), 2500);
    } catch (e) {
      setConfirmClear(false);
      setWarn(String(e));
    }
  }

  async function checkUpdates() {
    setCheckingUpdate(true);
    setUpdateMsg(null);
    try {
      const upd = await checkForUpdate();
      if (!upd) {
        setUpdateMsg(t("up_to_date"));
      } else {
        setUpdateMsg(`${t("update_available")} v${upd.version}`);
        await installUpdate(upd); // downloads, installs, relaunches
      }
    } catch (e) {
      setUpdateMsg(String(e));
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function startMicTest() {
    if (!settings) return;
    try {
      await api.startRecording(settings.input_device ?? null);
      setTesting(true);
    } catch (e) {
      setWarn(String(e));
    }
  }

  async function stopMicTest() {
    setTesting(false);
    setTestLevel(0);
    try {
      await api.cancelRecording();
    } catch {
      /* ignore */
    }
  }

  async function save() {
    if (!settings) return;
    const ok = await api.updateSettings(settings);
    if (!ok) {
      setWarn(t("hotkey_warn"));
      return;
    }
    onSaved(settings);
    onClose();
  }

  return (
    <div
      dir={lang === "ar" ? "rtl" : "ltr"}
      className="fixed inset-0 z-20 grid place-items-center bg-black/50 p-6"
    >
      <div className="flex max-h-[calc(100vh-3rem)] w-full max-w-md flex-col rounded-2xl border border-ink-800 bg-ink-900 shadow-2xl">
        <div className="flex shrink-0 items-center justify-between p-6 pb-4">
          <h2 className="text-lg font-semibold">{t("settings")}</h2>
          <button onClick={onClose} className="tool-btn">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div ref={scrollRef} className="flex-1 space-y-5 overflow-y-auto overscroll-contain px-6 py-1">
          <Field label={t("app_language")}>
            <div className="flex gap-2">
              {(["en", "ar"] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => patch({ ui_lang: m })}
                  className={[
                    "flex-1 rounded-lg px-3 py-2 text-sm transition",
                    settings.ui_lang === m
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {m === "en" ? "English" : "العربية"}
                </button>
              ))}
            </div>
          </Field>

          <Field label={t("output")}>
            <div className="grid grid-cols-2 gap-2">
              {(
                [
                  ["raw", "mode_raw", "mode_raw_desc"],
                  ["translate", "mode_translate", "mode_translate_desc"],
                  ["polish", "mode_polish", "mode_polish_desc"],
                  ["prompt", "mode_prompt", "mode_prompt_desc"],
                ] as [OutputMode, StringKey, StringKey][]
              ).map(([m, label]) => (
                <button
                  key={m}
                  onClick={() => patch({ output_mode: m })}
                  className={[
                    "rounded-lg px-3 py-2 text-sm transition",
                    settings.output_mode === m
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {t(label)}
                </button>
              ))}
            </div>
            <p className="mt-1.5 text-xs text-ink-500">
              {t(
                settings.output_mode === "raw"
                  ? "mode_raw_desc"
                  : settings.output_mode === "translate"
                    ? "mode_translate_desc"
                    : settings.output_mode === "polish"
                      ? "mode_polish_desc"
                      : "mode_prompt_desc"
              )}
            </p>
          </Field>

          {settings.output_mode === "translate" && (
            <Field label={t("translate_to")}>
              <select
                value={settings.translate_target}
                onChange={(e) => patch({ translate_target: e.target.value })}
                className="field"
              >
                {LANGUAGES.map((l) => (
                  <option key={l.value} value={l.value}>
                    {l.label}
                  </option>
                ))}
              </select>
            </Field>
          )}

          {settings.output_mode !== "raw" && (
            <Field label={t("ai_engine")}>
              <div className="flex gap-2">
                {(["cli", "api"] as const).map((m) => (
                  <button
                    key={m}
                    onClick={() => patch({ ai_engine: m })}
                    className={[
                      "flex-1 rounded-lg px-3 py-2 text-sm transition",
                      settings.ai_engine === m
                        ? "bg-accent text-white"
                        : "bg-ink-800 text-ink-400 hover:text-white",
                    ].join(" ")}
                  >
                    {m === "cli" ? t("engine_cli") : t("engine_api")}
                  </button>
                ))}
              </div>
              <p className="mt-1.5 text-xs text-ink-500">{t("engine_hint")}</p>

              {settings.ai_engine === "cli" ? (
                <input
                  value={settings.cli_command}
                  onChange={(e) => patch({ cli_command: e.target.value })}
                  placeholder="claude"
                  spellCheck={false}
                  dir="ltr"
                  className="field mt-2"
                />
              ) : (
                <div className="mt-2 space-y-2">
                  <select
                    value={settings.api_provider}
                    onChange={(e) =>
                      patch({ api_provider: e.target.value as Settings["api_provider"] })
                    }
                    className="field"
                  >
                    <option value="anthropic">Anthropic (Claude)</option>
                    <option value="openai">OpenAI (GPT)</option>
                    <option value="custom">Custom (OpenAI-compatible)</option>
                  </select>
                  <input
                    type="password"
                    value={settings.api_key}
                    onChange={(e) => patch({ api_key: e.target.value })}
                    placeholder={t("api_key")}
                    spellCheck={false}
                    dir="ltr"
                    autoComplete="off"
                    className="field"
                  />
                  <input
                    value={settings.api_model}
                    onChange={(e) => patch({ api_model: e.target.value })}
                    placeholder={
                      settings.api_provider === "anthropic"
                        ? "claude-sonnet-4-5"
                        : settings.api_provider === "openai"
                          ? "gpt-4o-mini"
                          : "model-id"
                    }
                    spellCheck={false}
                    dir="ltr"
                    className="field"
                  />
                  {settings.api_provider === "custom" && (
                    <input
                      value={settings.api_base_url}
                      onChange={(e) => patch({ api_base_url: e.target.value })}
                      placeholder="https://api.example.com/v1"
                      spellCheck={false}
                      dir="ltr"
                      className="field"
                    />
                  )}
                  <p className="text-xs text-ink-500">{t("api_key_hint")}</p>
                  {usage && (
                    <div className="flex items-center justify-between rounded-lg bg-ink-800/60 px-3 py-2 text-xs">
                      <span className="text-ink-300">
                        {t("usage")}:{" "}
                        <span className="tabular-nums text-ink-100">
                          {usage.input_tokens.toLocaleString()} /{" "}
                          {usage.output_tokens.toLocaleString()} / {usage.calls}
                        </span>{" "}
                        <span className="text-ink-500">({t("usage_line")})</span>
                      </span>
                      <button
                        onClick={() =>
                          api.resetUsage().then(() => api.getUsage().then(setUsage))
                        }
                        className="text-ink-400 hover:text-accent"
                      >
                        {t("reset")}
                      </button>
                    </div>
                  )}
                </div>
              )}
            </Field>
          )}

          <Field label={t("microphone")}>
            <div className="flex gap-2">
              <select
                value={settings.input_device ?? ""}
                onChange={(e) => patch({ input_device: e.target.value || null })}
                className="field flex-1"
              >
                <option value="">{t("system_default")}</option>
                {devices.map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
              </select>
              <button
                onClick={() => api.listInputDevices().then(setDevices).catch(() => {})}
                className="shrink-0 rounded-lg bg-ink-800 px-3 text-xs text-ink-300 hover:text-white"
                title={t("refresh")}
              >
                {t("refresh")}
              </button>
            </div>
            <div className="mt-2 flex items-center gap-2">
              <button
                onClick={testing ? stopMicTest : startMicTest}
                className={[
                  "rounded-lg px-3 py-1.5 text-xs font-medium transition",
                  testing ? "bg-accent text-white" : "bg-ink-800 text-ink-200 hover:text-white",
                ].join(" ")}
              >
                {testing ? t("stop_test") : t("test_mic")}
              </button>
              <div className="h-2 flex-1 overflow-hidden rounded-full bg-ink-800">
                <div
                  className="h-full rounded-full bg-accent transition-[width] duration-75"
                  style={{ width: `${Math.min(100, Math.round(testLevel * 140))}%` }}
                />
              </div>
            </div>
            <p className="mt-1 text-xs text-ink-500">{t("mic_test_hint")}</p>
          </Field>

          <Field label={t("model")}>
            <div className="space-y-1.5">
              {KNOWN_MODELS.map((m) => {
                const info = models.find((x) => x.name === m);
                const present = info?.present ?? false;
                const active = settings.model === m;
                const prog = progress[m];
                const sizeMb = present && info && info.size_mb > 0 ? info.size_mb : MODEL_MB[m] ?? 0;
                return (
                  <div
                    key={m}
                    className="flex items-center gap-2 rounded-lg bg-ink-800/60 px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm text-ink-100">{m}</span>
                        {active && (
                          <span className="rounded bg-accent/20 px-1.5 py-0.5 text-[10px] text-accent">
                            {t("in_use")}
                          </span>
                        )}
                      </div>
                      <span className="text-xs text-ink-500">{fmtSize(sizeMb)}</span>
                      {prog && (
                        <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-ink-700">
                          <div
                            className="h-full bg-accent transition-all"
                            style={{ width: `${pct(prog)}%` }}
                          />
                        </div>
                      )}
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {prog ? (
                        <span className="text-xs tabular-nums text-ink-400">
                          {pct(prog)}%
                        </span>
                      ) : present ? (
                        <>
                          {!active && (
                            <button
                              onClick={() => patch({ model: m })}
                              className="rounded-md bg-ink-700 px-2.5 py-1 text-xs text-ink-200 hover:text-white"
                            >
                              {t("use_model")}
                            </button>
                          )}
                          {!active && m !== "small" && (
                            <button
                              onClick={() => removeModel(m)}
                              className="tool-btn"
                              title={t("delete")}
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          )}
                        </>
                      ) : (
                        <button
                          onClick={() => startDownload(m)}
                          className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white hover:bg-accent-deep"
                        >
                          {t("download")}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
            <p className="mt-1.5 text-xs text-ink-500">{t("bigger_better_arabic")}</p>
          </Field>

          <Field label={t("language")}>
            <div className="flex gap-2">
              {(["auto", "en", "ar"] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => patch({ language_mode: m })}
                  className={[
                    "flex-1 rounded-lg px-3 py-2 text-sm transition",
                    settings.language_mode === m
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {m === "auto" ? t("auto_detect") : m === "en" ? t("english") : t("arabic")}
                </button>
              ))}
            </div>
          </Field>

          {settings.language_mode !== "en" && (
            <Field label={t("dialect")}>
              <select
                value={settings.dialect}
                onChange={(e) => patch({ dialect: e.target.value })}
                className="field"
              >
                {DIALECTS.map(([value, label]) => (
                  <option key={value} value={value}>
                    {t(label)}
                  </option>
                ))}
              </select>
              <p className="mt-1.5 text-xs text-ink-500">{t("dialect_hint")}</p>
            </Field>
          )}

          <Field label={t("hotkey_mode")}>
            <div className="flex gap-2">
              {(["hold", "toggle"] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => patch({ capture_mode: m })}
                  className={[
                    "flex-1 rounded-lg px-3 py-2 text-sm transition",
                    settings.capture_mode === m
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {m === "hold" ? t("push_to_talk") : t("toggle")}
                </button>
              ))}
            </div>
            <p className="mt-1 text-xs text-ink-500">{t("hotkey_mode_hint")}</p>
          </Field>

          <Field label={t("global_hotkey")}>
            <HotkeyRecorder
              value={settings.ptt_hotkey}
              placeholderSet={t("click_set_shortcut")}
              placeholderRecording={t("press_shortcut")}
              clearLabel={t("clear")}
              onChange={(v) => {
                patch({ ptt_hotkey: v });
                setWarn(null);
              }}
            />
            <p className="mt-1 text-xs text-ink-500">{t("hotkey_hint")}</p>
          </Field>

          <Toggle
            label={t("type_into_active")}
            desc={t("type_into_active_desc")}
            checked={settings.auto_type}
            onChange={(v) => patch({ auto_type: v })}
          />
          <Toggle
            label={t("auto_copy")}
            desc={t("auto_copy_desc")}
            checked={settings.auto_copy}
            onChange={(v) => patch({ auto_copy: v })}
          />
          {settings.auto_type && (
            <Toggle
              label={t("keep_line_breaks")}
              desc={t("keep_line_breaks_desc")}
              checked={settings.keep_line_breaks}
              onChange={(v) => patch({ keep_line_breaks: v })}
            />
          )}
          <Toggle
            label={t("sound_cue")}
            desc={t("sound_cue_desc")}
            checked={settings.sound}
            onChange={(v) => patch({ sound: v })}
          />
          <Toggle
            label={t("noise_suppression")}
            desc={t("noise_suppression_desc")}
            checked={settings.noise_suppression}
            onChange={(v) => patch({ noise_suppression: v })}
          />

          <Field label={t("storage")}>
            <span className="mb-1.5 block text-sm text-ink-300">{t("auto_delete")}</span>
            <div className="grid grid-cols-2 gap-2">
              {RETENTION.map(([days, label]) => (
                <button
                  key={days}
                  onClick={() => patch({ retention_days: days })}
                  className={[
                    "rounded-lg px-3 py-2 text-sm transition",
                    settings.retention_days === days
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {t(label)}
                </button>
              ))}
            </div>
            <p className="mt-1.5 text-xs text-ink-500">{t("auto_delete_hint")}</p>
            <p className="mt-1 text-xs text-accent-soft">{t("pinned_safe_hint")}</p>

            <span className="mb-1.5 mt-4 block text-sm text-ink-300">{t("free_memory")}</span>
            <div className="grid grid-cols-3 gap-2">
              {IDLE_UNLOAD.map(([mins, label]) => (
                <button
                  key={mins}
                  onClick={() => patch({ idle_unload_minutes: mins })}
                  className={[
                    "rounded-lg px-3 py-2 text-sm transition",
                    settings.idle_unload_minutes === mins
                      ? "bg-accent text-white"
                      : "bg-ink-800 text-ink-400 hover:text-white",
                  ].join(" ")}
                >
                  {t(label)}
                </button>
              ))}
            </div>
            <p className="mt-1.5 text-xs text-ink-500">{t("free_memory_hint")}</p>

            <div className="mt-3 rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
              <p className="text-sm text-ink-200">{t("clear_history")}</p>
              <p className="mt-0.5 text-xs text-ink-500">{t("clear_history_desc")}</p>
              {cleared ? (
                <p className="mt-2 text-xs font-medium text-accent-soft">{t("history_cleared")}</p>
              ) : confirmClear ? (
                <div className="mt-2 flex items-center gap-2">
                  <span className="flex items-center gap-1.5 text-xs text-amber-300">
                    <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
                    {t("clear_confirm_q")}
                  </span>
                  <div className="ms-auto flex gap-2">
                    <button
                      onClick={() => setConfirmClear(false)}
                      className="rounded-md px-2.5 py-1 text-xs text-ink-400 hover:text-white"
                    >
                      {t("cancel")}
                    </button>
                    <button
                      onClick={clearHistory}
                      className="rounded-md bg-red-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-red-500"
                    >
                      {t("clear_confirm_yes")}
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  onClick={() => setConfirmClear(true)}
                  className="mt-2 inline-flex items-center gap-1.5 rounded-md bg-ink-800 px-2.5 py-1 text-xs text-amber-300 hover:bg-ink-700"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("clear_history")}
                </button>
              )}
            </div>
          </Field>

          <Field label={t("about")}>
            <div className="flex items-center justify-between rounded-lg bg-ink-800/60 px-3 py-2 text-xs">
              <span className="text-ink-300">
                {t("app_version")}{" "}
                <span className="tabular-nums text-ink-100">{version || "…"}</span>
              </span>
              <button
                onClick={checkUpdates}
                disabled={checkingUpdate}
                className="inline-flex items-center gap-1.5 text-ink-400 transition hover:text-accent disabled:opacity-50"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${checkingUpdate ? "animate-spin" : ""}`} />
                {checkingUpdate ? t("checking") : t("check_updates")}
              </button>
            </div>
            {updateMsg && <p className="mt-1.5 text-xs text-ink-500">{updateMsg}</p>}
          </Field>
        </div>

        {warn && (
          <p className="shrink-0 px-6 pt-2 text-xs text-amber-400">{warn}</p>
        )}
        <div className="flex shrink-0 justify-end gap-2 p-6 pt-4">
          <button
            onClick={onClose}
            className="rounded-lg px-4 py-2 text-sm text-ink-400 hover:text-white"
          >
            {t("cancel")}
          </button>
          <button
            onClick={save}
            className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-deep"
          >
            {t("save")}
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm text-ink-300">{label}</span>
      {children}
    </label>
  );
}

function Toggle({
  label,
  desc,
  checked,
  onChange,
}: {
  label: string;
  desc: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div>
        <p className="text-sm text-ink-300">{label}</p>
        <p className="text-xs text-ink-500">{desc}</p>
      </div>
      <button
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={[
          "mt-0.5 h-6 w-11 shrink-0 rounded-full p-0.5 transition",
          checked ? "bg-accent" : "bg-ink-700",
        ].join(" ")}
      >
        <span
          className={[
            "block h-5 w-5 rounded-full bg-white transition-transform",
            checked ? "translate-x-5 rtl:-translate-x-5" : "translate-x-0",
          ].join(" ")}
        />
      </button>
    </div>
  );
}

// Map a key press to a Tauri accelerator string. Returns null for
// modifier-only or unsupported keys so we never store an invalid shortcut.
function eventToAccel(e: KeyboardEvent): string | null {
  const code = e.code;
  const modifierCodes = [
    "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight",
    "AltLeft", "AltRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight",
  ];
  if (modifierCodes.includes(code)) return null;

  const mods: string[] = [];
  if (e.ctrlKey || e.metaKey) mods.push("CommandOrControl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");

  let key = "";
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^F\d{1,2}$/.test(code)) key = code;
  else if (code === "Space") key = "Space";
  else if (code === "ArrowUp") key = "Up";
  else if (code === "ArrowDown") key = "Down";
  else if (code === "ArrowLeft") key = "Left";
  else if (code === "ArrowRight") key = "Right";
  else if (code === "Enter") key = "Enter";
  else if (code === "Tab") key = "Tab";
  else if (code === "Minus") key = "-";
  else if (code === "Equal") key = "=";
  else if (code === "Backquote") key = "`";
  else return null;

  return [...mods, key].join("+");
}

function HotkeyRecorder({
  value,
  onChange,
  placeholderSet,
  placeholderRecording,
  clearLabel,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholderSet: string;
  placeholderRecording: string;
  clearLabel: string;
}) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      const accel = eventToAccel(e);
      if (accel) {
        onChange(accel);
        setRecording(false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange]);

  return (
    <button
      type="button"
      onClick={() => setRecording((r) => !r)}
      className="field flex w-full items-center justify-between text-left"
    >
      <span
        className={
          recording ? "text-accent" : value ? "text-ink-100" : "text-ink-500"
        }
        dir="ltr"
      >
        {recording ? placeholderRecording : value || placeholderSet}
      </span>
      {value && !recording && (
        <span
          onClick={(e) => {
            e.stopPropagation();
            onChange("");
          }}
          className="text-xs text-ink-500 hover:text-accent"
        >
          {clearLabel}
        </span>
      )}
    </button>
  );
}
