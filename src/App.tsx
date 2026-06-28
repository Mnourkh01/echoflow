import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { Settings as SettingsIcon, Cpu, AlertTriangle, Minimize2 } from "lucide-react";
import RecordControl, { type RecState } from "./components/RecordControl";
import TranscriptView from "./components/TranscriptView";
import HistorySidebar from "./components/HistorySidebar";
import SettingsPanel from "./components/SettingsPanel";
import ModeSwitcher from "./components/ModeSwitcher";
import UpdateBanner from "./components/UpdateBanner";
import FirstRunModel from "./components/FirstRunModel";
import { api, type AppStatus, type OutputMode, type RecordingResult, type RecordingSummary, type Settings } from "./lib/api";
import { checkForUpdate, type Update } from "./lib/updater";
import { I18nProvider, translate } from "./lib/i18n";
import { playStart, playStop } from "./lib/sound";

export default function App() {
  const [recState, setRecState] = useState<RecState>("idle");
  const [level, setLevel] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [current, setCurrent] = useState<RecordingResult | null>(null);
  const [history, setHistory] = useState<RecordingSummary[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);

  // Show a soft, auto-clearing notice (e.g. "no speech detected").
  const flashNotice = useCallback((msg: string) => {
    setNotice(msg);
    window.setTimeout(() => setNotice((n) => (n === msg ? null : n)), 2500);
  }, []);

  // Refs so global/keyboard event handlers always see the latest state.
  const recStateRef = useRef(recState);
  recStateRef.current = recState;
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const refreshHistory = useCallback(async (q: string | null = null) => {
    setHistory(await api.listRecordings(q && q.length ? q : null));
  }, []);

  useEffect(() => {
    api.appStatus().then(setStatus).catch(() => {});
    api.getSettings().then(setSettings).catch(() => {});
    refreshHistory();
    // Quietly check for an update on launch; a hit renders the banner.
    checkForUpdate().then(setUpdate).catch(() => {});
  }, [refreshHistory]);

  const start = useCallback(async () => {
    if (recStateRef.current !== "idle") return;
    setError(null);
    try {
      await api.startRecording(settingsRef.current?.input_device ?? null);
      if (settingsRef.current?.sound ?? true) playStart();
      setElapsed(0);
      setRecState("recording");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const stop = useCallback(async () => {
    if (recStateRef.current !== "recording") return;
    if (settingsRef.current?.sound ?? true) playStop();
    setRecState("transcribing");
    try {
      const res = await api.stopRecording();
      if (res) {
        setCurrent(res);
        setActiveId(res.id);
        await refreshHistory();
      } else {
        // Silent mis-click: discarded by the backend.
        flashNotice(translate(settingsRef.current?.ui_lang ?? "en", "no_speech_notice"));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRecState("idle");
      setLevel(0);
    }
  }, [refreshHistory, flashNotice]);

  const toggle = useCallback(() => {
    if (recStateRef.current === "recording") stop();
    else if (recStateRef.current === "idle") start();
  }, [start, stop]);

  // Live level meter + elapsed timer while recording.
  useEffect(() => {
    if (recState !== "recording") return;
    const t0 = performance.now();
    const id = setInterval(async () => {
      setElapsed(performance.now() - t0);
      try {
        setLevel(await api.getLevel());
      } catch {
        /* ignore transient */
      }
    }, 100);
    return () => clearInterval(id);
  }, [recState]);

  // In-app push to talk: hold Space (ignore when typing in a field).
  useEffect(() => {
    const isTyping = () => {
      const el = document.activeElement;
      return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
    };
    const down = (e: KeyboardEvent) => {
      if (e.code === "Space" && !e.repeat && !isTyping()) {
        e.preventDefault();
        start();
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.code === "Space" && !isTyping()) {
        e.preventDefault();
        stop();
      }
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [start, stop]);

  // Global hotkey dictation is driven by Rust; it emits these events so the UI
  // can reflect state, play the cue sound, and show the typed-out result.
  useEffect(() => {
    const sound = () => settingsRef.current?.sound ?? true;
    const offStarted = listen("rec-started", () => {
      if (sound()) playStart();
      setElapsed(0);
      setRecState("recording");
    });
    const offStopped = listen("rec-stopped", () => {
      if (sound()) playStop();
      setRecState("transcribing");
    });
    const offResult = listen<RecordingResult>("dictation-result", (e) => {
      setCurrent(e.payload);
      setActiveId(e.payload.id);
      refreshHistory();
      setRecState("idle");
      setLevel(0);
    });
    const offError = listen<string>("rec-error", (e) => {
      setError(e.payload);
      setRecState("idle");
      setLevel(0);
    });
    const offCanceled = listen("rec-canceled", () => {
      setRecState("idle");
      setLevel(0);
      flashNotice(translate(settingsRef.current?.ui_lang ?? "en", "no_speech_notice"));
    });
    return () => {
      offStarted.then((f) => f());
      offStopped.then((f) => f());
      offResult.then((f) => f());
      offError.then((f) => f());
      offCanceled.then((f) => f());
    };
  }, [refreshHistory, flashNotice]);

  // Apply the app language direction to the whole document.
  const uiLang = settings?.ui_lang ?? "en";
  useEffect(() => {
    document.documentElement.lang = uiLang;
    document.documentElement.dir = uiLang === "ar" ? "rtl" : "ltr";
  }, [uiLang]);

  // Quick output-mode change from the header: persist immediately.
  const changeMode = useCallback(async (mode: OutputMode) => {
    if (!settingsRef.current) return;
    const next = { ...settingsRef.current, output_mode: mode };
    setSettings(next);
    try {
      await api.updateSettings(next);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Quick language change from the header (Auto / EN / AR): persist immediately.
  const changeLang = useCallback(async (mode: "auto" | "en" | "ar") => {
    if (!settingsRef.current) return;
    const next = { ...settingsRef.current, language_mode: mode };
    setSettings(next);
    try {
      await api.updateSettings(next);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Shrink the app into the floating pill (instead of closing).
  async function minimizeToPill() {
    try {
      const overlay = await Window.getByLabel("overlay");
      if (overlay) await overlay.show();
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  async function selectRecording(id: number) {
    setActiveId(id);
    setCurrent(await api.getRecording(id));
  }

  async function deleteRecording(id: number) {
    await api.deleteRecording(id);
    if (activeId === id) {
      setActiveId(null);
      setCurrent(null);
    }
    await refreshHistory();
  }

  // Pin/unpin a recording so it survives the retention auto-delete.
  async function togglePin(id: number, pinned: boolean) {
    await api.setPinned(id, pinned);
    setCurrent((c) => (c && c.id === id ? { ...c, pinned } : c));
    await refreshHistory();
  }

  return (
    <I18nProvider lang={uiLang}>
    <div className="flex h-screen overflow-hidden">
      <HistorySidebar
        items={history}
        activeId={activeId}
        onSelect={selectRecording}
        onDelete={deleteRecording}
        onPin={togglePin}
        onSearch={(q) => refreshHistory(q)}
      />

      <main className="flex flex-1 flex-col overflow-hidden">
        <header className="flex items-center justify-between gap-3 border-b border-ink-800 px-5 py-3">
          <div className="flex items-center gap-2 text-xs text-ink-500">
            <Cpu className="h-3.5 w-3.5" />
            <span>{status ? status.backend : translate(uiLang, "starting")}</span>
            {status && !status.model_present && (
              <span className="inline-flex items-center gap-1 text-amber-400">
                <AlertTriangle className="h-3.5 w-3.5" /> {translate(uiLang, "model_missing")}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            {settings && (
              <div className="flex items-center rounded-lg bg-ink-900 p-0.5 text-xs" title={translate(uiLang, "language")}>
                {(["auto", "en", "ar"] as const).map((m) => (
                  <button
                    key={m}
                    onClick={() => changeLang(m)}
                    className={[
                      "rounded-md px-2 py-1 transition",
                      settings.language_mode === m
                        ? "bg-accent text-white"
                        : "text-ink-400 hover:text-white",
                    ].join(" ")}
                  >
                    {m === "auto" ? translate(uiLang, "auto_detect") : m.toUpperCase()}
                  </button>
                ))}
              </div>
            )}
            {settings && (
              <ModeSwitcher value={settings.output_mode} onChange={changeMode} />
            )}
            <button
              onClick={minimizeToPill}
              className="tool-btn"
              title={translate(uiLang, "minimize_to_pill")}
            >
              <Minimize2 className="h-4 w-4" />
            </button>
            <button
              onClick={() => setSettingsOpen(true)}
              className="tool-btn"
              title={translate(uiLang, "settings")}
            >
              <SettingsIcon className="h-4 w-4" />
            </button>
          </div>
        </header>

        {update && <UpdateBanner update={update} onError={setError} />}

        {error && (
          <div className="flex items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-5 py-2 text-sm text-amber-300">
            <AlertTriangle className="h-4 w-4 shrink-0" />
            <span className="selectable">{error}</span>
          </div>
        )}

        {notice && !error && (
          <div className="border-b border-accent/20 bg-accent/10 px-5 py-2 text-sm text-accent-soft">
            {notice}
          </div>
        )}

        <RecordControl
          state={recState}
          level={level}
          elapsedMs={elapsed}
          onToggle={toggle}
        />

        <TranscriptView rec={current} onTogglePin={togglePin} />
      </main>

      {status && !status.model_present && (
        <FirstRunModel
          onReady={() => api.appStatus().then(setStatus).catch(() => {})}
        />
      )}

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={(s) => {
          setSettings(s);
          // A shorter retention window may have purged old rows on save.
          refreshHistory();
        }}
        onDataCleared={() => {
          setActiveId(null);
          setCurrent(null);
          refreshHistory();
        }}
      />
    </div>
    </I18nProvider>
  );
}
