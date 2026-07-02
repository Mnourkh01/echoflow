import { useEffect, useState } from "react";
import RecordControl, { type RecState } from "../components/RecordControl";
import { I18nProvider } from "../lib/i18n";

type Mood = "happy" | "sad" | "update" | "switch";

/**
 * Dev-only visual harness (`npm run dev` + `/?preview=mascot` in a browser).
 * Renders the record control outside Tauri so every mascot/orb state can be
 * eyeballed and screenshotted without a microphone or the Rust backend.
 * Never reached in production: main.tsx gates it behind import.meta.env.DEV.
 */
export default function MascotPreview() {
  const [state, setState] = useState<RecState>("idle");
  const [variant, setVariant] = useState<"orb" | "robot">("robot");
  const [flash, setFlash] = useState<Mood | null>(null);
  const [level, setLevel] = useState(0);

  // Fake a talking voice while "recording": a wandering envelope.
  useEffect(() => {
    if (state !== "recording") {
      setLevel(0);
      return;
    }
    let t = 0;
    const id = window.setInterval(() => {
      t += 0.1;
      const talk = Math.abs(Math.sin(t * 2.1)) * (0.5 + 0.5 * Math.abs(Math.sin(t * 0.63)));
      setLevel(Math.max(0.03, Math.min(1, talk)));
    }, 100);
    return () => window.clearInterval(id);
  }, [state]);

  function fireFlash(m: Mood) {
    setFlash(m);
    window.setTimeout(() => setFlash((f) => (f === m ? null : f)), 1500);
  }

  const btn =
    "rounded-lg border border-white/10 bg-white/[0.06] px-3 py-1.5 text-xs text-ink-200 hover:text-white";

  return (
    <I18nProvider lang="en">
      <div className="flex h-screen flex-col items-center justify-center gap-6">
        <RecordControl
          state={state}
          level={level}
          elapsedMs={12_340}
          onToggle={() => setState((s) => (s === "recording" ? "idle" : "recording"))}
          onCancel={() => setState("idle")}
          variant={variant}
          flash={flash}
        />
        <div className="flex flex-wrap items-center justify-center gap-2">
          {(["idle", "recording", "transcribing"] as RecState[]).map((s) => (
            <button key={s} className={btn} onClick={() => setState(s)}>
              {s}
            </button>
          ))}
          <button className={btn} onClick={() => setVariant((v) => (v === "orb" ? "robot" : "orb"))}>
            variant: {variant}
          </button>
          {(["happy", "sad", "update", "switch"] as Mood[]).map((m) => (
            <button key={m} className={btn} onClick={() => fireFlash(m)}>
              {m}
            </button>
          ))}
        </div>
      </div>
    </I18nProvider>
  );
}
