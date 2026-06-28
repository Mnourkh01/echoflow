import { useEffect, useState } from "react";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Languages } from "lucide-react";
import { api } from "./lib/api";
import logo from "./assets/echoflow.png";

type Mode = "idle" | "recording" | "processing";

// Static per-bar multipliers so the waveform looks lively, not uniform.
const BARS = [0.45, 0.8, 1, 0.65, 0.95, 0.55, 0.85];

/**
 * The floating always-on-top pill. It self-shows when recording starts (so you
 * see it's listening from any app), animates the live mic level, and hides
 * shortly after the result — unless the app has been minimized into it.
 */
export default function Overlay() {
  const [mode, setMode] = useState<Mode>("idle");
  const [level, setLevel] = useState(0);
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

  return (
    <div
      data-tauri-drag-region
      onContextMenu={(e) => {
        e.preventDefault();
        api.showPillMenu().catch(() => {});
      }}
      className={[
        "flex h-screen w-screen cursor-grab items-center gap-2 rounded-full border bg-ink-900/95 px-2 shadow-2xl transition-colors",
        warn ? "border-amber-400/70" : "border-white/10",
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
        <div data-tauri-drag-region className="flex h-4 flex-1 items-center justify-center gap-[3px]">
          {BARS.map((b, i) => {
            const h = processing
              ? 55
              : active
                ? Math.max(16, Math.min(100, 16 + level * 95 * b))
                : 20 * b;
            return (
              <span
                key={i}
                className={[
                  "w-[2px] rounded-full transition-[height] duration-100",
                  active
                    ? "bg-accent"
                    : processing
                      ? "bg-accent/70 animate-pulse"
                      : "bg-ink-600",
                ].join(" ")}
                style={{ height: `${h}%` }}
              />
            );
          })}
        </div>
      )}
      <button
        onClick={restore}
        title="Open EchoFlow"
        className="grid h-7 w-7 shrink-0 place-items-center rounded-full transition hover:bg-white/5"
      >
        <img src={logo} alt="EchoFlow" draggable={false} className="h-6 w-6" />
      </button>
    </div>
  );
}
