import { Mic, Square, Loader2 } from "lucide-react";
import LevelMeter from "./LevelMeter";
import { useT } from "../lib/i18n";

export type RecState = "idle" | "recording" | "transcribing";

interface Props {
  state: RecState;
  level: number;
  elapsedMs: number;
  onToggle: () => void;
}

function fmt(ms: number) {
  const t = Math.floor(ms / 1000);
  const m = Math.floor(t / 60);
  const s = t % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function RecordControl({ state, level, elapsedMs, onToggle }: Props) {
  const { t } = useT();
  const recording = state === "recording";
  const busy = state === "transcribing";

  return (
    <div className="flex flex-col items-center gap-4 py-6">
      <div className="relative grid h-24 w-24 place-items-center">
        {/* Expanding sonar rings while recording — the "alive / listening" cue. */}
        {recording && (
          <>
            <span className="echo-ring pointer-events-none absolute inset-0 rounded-full bg-accent/30" />
            <span
              className="echo-ring pointer-events-none absolute inset-0 rounded-full bg-accent-deep/25"
              style={{ animationDelay: "0.9s" }}
            />
          </>
        )}
        <button
          onClick={onToggle}
          disabled={busy}
          aria-label={recording ? t("stop_recording") : t("start_recording")}
          className={[
            "relative grid h-24 w-24 place-items-center rounded-full transition",
            "outline-none focus-visible:ring-4 focus-visible:ring-accent/40",
            busy
              ? "echo-glow cursor-wait bg-gradient-to-br from-accent-deep to-accent text-white"
              : recording
                ? "echo-glow bg-gradient-to-br from-accent to-accent-deep text-white"
                : "bg-ink-800 text-ink-400 hover:bg-ink-700 hover:text-white hover:shadow-[0_0_18px_2px_rgba(61,123,247,0.35)]",
          ].join(" ")}
        >
          {busy ? (
            <Loader2 className="h-9 w-9 animate-spin" />
          ) : recording ? (
            <Square className="h-8 w-8" fill="currentColor" />
          ) : (
            <Mic className="h-9 w-9" />
          )}
        </button>
      </div>

      <div className="h-10 w-full max-w-sm">
        <LevelMeter level={level} active={recording} />
      </div>

      <div className="text-sm text-ink-400">
        {busy ? (
          t("transcribing")
        ) : recording ? (
          <span className="tabular-nums text-accent">{fmt(elapsedMs)}</span>
        ) : (
          <span>
            {t("talk_hint_pre")}{" "}
            <kbd className="rounded bg-ink-800 px-1.5 py-0.5 text-ink-400">Space</kbd>{" "}
            {t("talk_hint_post")}
          </span>
        )}
      </div>
    </div>
  );
}
