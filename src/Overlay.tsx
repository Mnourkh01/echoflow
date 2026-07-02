import { useEffect, useState } from "react";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Languages, X } from "lucide-react";
import { api, type Settings } from "./lib/api";
import { applyAccent } from "./lib/theme";
import logo from "./assets/echoflow.png";

type Mode = "idle" | "recording" | "processing";

// A denser waveform reads more like a modern voice UI. Center bars are tallest
// (Siri-style envelope); a small deterministic jitter keeps it lively.
const N_BARS = 13;

/**
 * The floating always-on-top pill. It self-shows when recording starts (so you
 * see it's listening from any app), animates the live mic level, and hides
 * shortly after the result — unless the app has been minimized into it.
 */
export default function Overlay() {
  const [mode, setMode] = useState<Mode>("idle");
  const [level, setLevel] = useState(0);
  const [pillStyle, setPillStyle] = useState<string>("wave");
  // Briefly true after a Translate-mode dictation that was already in the target
  // language (nothing to translate) so the pill can flag "you're still on Translate".
  const [warn, setWarn] = useState(false);
  const self = getCurrentWindow();

  useEffect(() => {
    const offStart = listen("rec-started", async () => {
      setMode("recording");
      try {
        await self.show();
      } catch {
        /* ignore */
      }
    });
    const offStop = listen("rec-stopped", () => setMode("processing"));
    const offResult = listen<{ translate_warning?: boolean }>("dictation-result", (e) => {
      if (e.payload?.translate_warning) {
        setWarn(true);
        window.setTimeout(() => setWarn(false), 2800);
        finish(2800); // hold the pill open long enough to read the warning
      } else {
        finish();
      }
    });
    const offError = listen("rec-error", () => finish());
    const offCanceled = listen("rec-canceled", () => finish());
    return () => {
      offStart.then((f) => f());
      offStop.then((f) => f());
      offResult.then((f) => f());
      offError.then((f) => f());
      offCanceled.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The pill is its own window/document, so apply the accent palette here too
  // (and keep it in sync when settings change from the main window / tray).
  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        applyAccent(s.accent);
        setPillStyle(s.pill_style || "wave");
      })
      .catch(() => {});
    const off = listen<Settings>("settings-changed", (e) => {
      applyAccent(e.payload.accent);
      setPillStyle(e.payload.pill_style || "wave");
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  // Drive the waveform from the live capture level while recording.
  useEffect(() => {
    if (mode !== "recording") return;
    const id = setInterval(async () => {
      try {
        setLevel(await api.getLevel());
      } catch {
        /* ignore transient */
      }
    }, 80);
    return () => clearInterval(id);
  }, [mode]);

  function finish(hideDelay = 1100) {
    setMode("idle");
    setLevel(0);
    // Hide after a beat, but stay if the app is minimized into the pill.
    window.setTimeout(async () => {
      try {
        const main = await Window.getByLabel("main");
        const mainVisible = main ? await main.isVisible() : true;
        if (mainVisible) await self.hide();
      } catch {
        try {
          await self.hide();
        } catch {
          /* ignore */
        }
      }
    }, hideDelay);
  }

  // Discard the take from the pill: drop the clip while recording, abort the
  // decode while processing. Backend emits rec-canceled so every window resets.
  async function cancelTake() {
    try {
      if (mode === "recording") await api.cancelRecording();
      else if (mode === "processing") await api.cancelTranscription();
    } catch {
      /* already finished */
    }
  }

  async function restore() {
    try {
      const main = await Window.getByLabel("main");
      if (main) {
        await main.show();
        await main.setFocus();
      }
      await self.hide();
    } catch {
      /* ignore */
    }
  }

  const active = mode === "recording";
  const processing = mode === "processing";

  // The pill's voice visualizer, user-selectable in Settings. Every style reacts
  // to the live level, breathes at idle, and runs a scanner while transcribing.
  function renderVisual() {
    if (pillStyle === "pulse") {
      const s = processing ? 15 : active ? 9 + level * 15 : 9;
      return (
        <div data-tauri-drag-region className="flex h-4 flex-1 items-center justify-center">
          <span
            className={!active && !processing ? "pill-breathe" : ""}
            style={{
              width: s,
              height: s,
              borderRadius: 999,
              background: active || processing ? "radial-gradient(circle at 40% 35%, rgb(var(--aurora-teal)), rgb(var(--aurora-iris)))" : "rgb(255 255 255 / 0.25)",
              boxShadow: active ? `0 0 ${6 + level * 12}px rgb(var(--aurora-teal) / 0.85)` : processing ? "0 0 9px rgb(var(--aurora-iris) / 0.75)" : undefined,
              transition: "width 100ms ease-out, height 100ms ease-out, box-shadow 100ms ease-out",
            }}
          />
        </div>
      );
    }

    if (pillStyle === "dots") {
      return (
        <div data-tauri-drag-region className="flex h-4 flex-1 items-center justify-center gap-[3px]">
          {Array.from({ length: 5 }).map((_, i) => {
            const env = 1 - (Math.abs(i - 2) / 2) * 0.5;
            if (processing) {
              return <span key={i} className="pill-wave" style={{ width: 4, height: 4, borderRadius: 999, background: "rgb(var(--aurora-iris))", boxShadow: "0 0 5px rgb(var(--aurora-iris) / 0.7)", animationDelay: `${i * 0.08}s` }} />;
            }
            const rise = active ? Math.min(9, level * 22 * env) : 0;
            return (
              <span
                key={i}
                className={active ? "" : "pill-breathe"}
                style={{
                  width: 4,
                  height: 4,
                  borderRadius: 999,
                  background: active ? "rgb(var(--aurora-teal))" : "rgb(255 255 255 / 0.28)",
                  boxShadow: active && rise > 3 ? "0 0 6px rgb(var(--aurora-teal) / 0.85)" : undefined,
                  transform: `translateY(${-rise}px)`,
                  transition: "transform 90ms ease-out",
                  animationDelay: `${i * 0.12}s`,
                }}
              />
            );
          })}
        </div>
      );
    }

    if (pillStyle === "minimal") {
      const w = processing ? 62 : active ? Math.min(92, 24 + level * 95) : 20;
      return (
        <div data-tauri-drag-region className="flex h-4 flex-1 items-center justify-center">
          <span
            className={processing ? "pill-wave" : ""}
            style={{
              height: 3,
              width: `${w}%`,
              borderRadius: 999,
              background: active || processing ? "linear-gradient(to right, transparent, rgb(var(--aurora-teal)), rgb(var(--aurora-iris)), transparent)" : "rgb(255 255 255 / 0.2)",
              boxShadow: active ? "0 0 8px rgb(var(--aurora-teal) / 0.7)" : undefined,
              transition: "width 110ms ease-out",
            }}
          />
        </div>
      );
    }

    // "wave" (default): the aurora waveform.
    return (
      <div data-tauri-drag-region className="flex h-4 flex-1 items-center justify-center gap-[2px]">
        {Array.from({ length: N_BARS }).map((_, i) => {
          const c = (N_BARS - 1) / 2;
          const env = 1 - (Math.abs(i - c) / c) * 0.6;
          const e = env * (0.8 + 0.2 * Math.abs(Math.sin(i * 1.7)));
          if (processing) {
            return <span key={i} className="pill-wave w-[2px] rounded-full" style={{ height: "60%", background: "linear-gradient(to top, rgb(var(--aurora-iris)), rgb(var(--aurora-teal)))", boxShadow: "0 0 5px rgb(var(--aurora-iris) / 0.7)", animationDelay: `${i * 0.05}s` }} />;
          }
          const h = active ? Math.max(12, Math.min(100, 12 + level * 108 * e)) : 18 * e;
          const lit = active && level * e > 0.12;
          return (
            <span
              key={i}
              className={active ? "w-[2px] rounded-full" : "pill-breathe w-[2px] rounded-full"}
              style={{
                height: `${h}%`,
                background: active ? "linear-gradient(to top, rgb(var(--aurora-iris)), rgb(var(--aurora-teal)))" : "rgb(255 255 255 / 0.22)",
                boxShadow: lit ? "0 0 6px rgb(var(--aurora-teal) / 0.85)" : undefined,
                transition: "height 90ms ease-out",
                animationDelay: active ? undefined : `${(i % 5) * 0.14}s`,
              }}
            />
          );
        })}
      </div>
    );
  }

  return (
    <div
      data-tauri-drag-region
      onContextMenu={(e) => {
        e.preventDefault();
        api.showPillMenu().catch(() => {});
      }}
      className={[
        "flex h-screen w-screen cursor-grab items-center gap-2 rounded-full border bg-ink-900/65 px-2 shadow-2xl backdrop-blur-xl transition-colors",
        warn ? "border-amber-400/70" : "border-white/15",
      ].join(" ")}
    >
      {warn ? (
        <div
          data-tauri-drag-region
          className="flex h-4 flex-1 items-center justify-center gap-1 text-amber-300"
          title="You spoke the language you're translating to. Switch mode?"
        >
          <Languages className="h-3 w-3 shrink-0" />
          <span className="text-[10px] font-medium leading-none">On Translate</span>
        </div>
      ) : (
        renderVisual()
      )}
      {(active || processing) && (
        <button
          onClick={cancelTake}
          title="Discard"
          className="grid h-6 w-6 shrink-0 place-items-center rounded-full text-ink-400 transition hover:bg-white/10 hover:text-red-300"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
      <button
        onClick={restore}
        title="Open EchoFlow"
        className="relative grid h-7 w-7 shrink-0 place-items-center rounded-full transition hover:bg-white/5"
      >
        {(active || processing) && (
          <span
            aria-hidden
            className="pill-ring pointer-events-none absolute inset-0 rounded-full"
            style={{
              background:
                "conic-gradient(from 0deg, transparent 0deg, rgb(var(--aurora-teal)) 90deg, rgb(var(--aurora-iris)) 210deg, transparent 320deg)",
              WebkitMask: "radial-gradient(farthest-side, transparent 62%, #000 64%)",
              mask: "radial-gradient(farthest-side, transparent 62%, #000 64%)",
            }}
          />
        )}
        <img src={logo} alt="EchoFlow" draggable={false} className="relative h-6 w-6" />
      </button>
    </div>
  );
}
