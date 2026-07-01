import { Mic, Square, Loader2 } from "lucide-react";
import LevelMeter from "./LevelMeter";
import RobotMascot from "./RobotMascot";
import { useT } from "../lib/i18n";

export type RecState = "idle" | "recording" | "transcribing";

interface Props {
  state: RecState;
  level: number;
  elapsedMs: number;
  onToggle: () => void;
  /** Record-button look. "orb" = the glass sphere; "robot" = the mascot. */
  variant?: "orb" | "robot";
  /** Transient mood for the robot (result / error / update / mode-switch);
   *  ignored by the orb. */
  flash?: "happy" | "sad" | "update" | "switch" | null;
}

function fmt(ms: number) {
  const t = Math.floor(ms / 1000);
  const m = Math.floor(t / 60);
  const s = t % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function RecordControl({ state, level, elapsedMs, onToggle, variant = "orb", flash }: Props) {
  const { t } = useT();
  const recording = state === "recording";
  const busy = state === "transcribing";
  const robot = variant === "robot";

  // The orb is a glass sphere; the light INSIDE it is the voice. The light fills
  // most of the glass even at rest, so it reads as a glowing sphere (not a dark
  // bezel with a small core). Recording blooms it; an ambient halo bleeds the
  // glow into the surrounding space so the orb never sits in a dead-black void.
  const lvl = Math.max(0, Math.min(1, level));
  const lightOpacity = recording ? 0.55 + lvl * 0.3 : busy ? 0.62 : 0.5;
  const lightScale = recording ? 0.85 + lvl * 0.28 : 0.9;
  const haloOpacity = recording ? 0.45 + lvl * 0.35 : busy ? 0.4 : 0.3;

  return (
    <div className="flex flex-col items-center gap-5 py-7">
      {robot ? (
        <RobotMascot
          state={state}
          level={level}
          onToggle={onToggle}
          flash={flash}
          label={recording ? t("stop_recording") : t("start_recording")}
        />
      ) : (
      <div className="relative grid h-28 w-28 place-items-center">
        {/* Ambient aurora halo — bleeds the orb's glow into the surrounding space
            so it reads as a light source, not a disc dropped on black. */}
        <span
          className={`pointer-events-none absolute -inset-14 rounded-full blur-3xl transition-opacity duration-300 ${
            !recording && !busy ? "aurora-drift" : ""
          }`}
          style={{
            opacity: haloOpacity,
            backgroundImage:
              "radial-gradient(circle at 50% 45%, rgb(var(--aurora-teal) / 0.65) 0%, rgb(var(--aurora-iris) / 0.5) 45%, rgb(var(--aurora-violet) / 0) 72%)",
          }}
        />
        {/* Sonar rings while recording — the "alive / listening" cue, in aurora. */}
        {recording && (
          <>
            <span className="echo-ring pointer-events-none absolute inset-0 rounded-full bg-aurora-teal/25" />
            <span
              className="echo-ring pointer-events-none absolute inset-0 rounded-full bg-aurora-violet/20"
              style={{ animationDelay: "0.9s" }}
            />
          </>
        )}

        <button
          onClick={onToggle}
          disabled={busy}
          aria-label={recording ? t("stop_recording") : t("start_recording")}
          className={[
            "group relative grid h-28 w-28 place-items-center rounded-full transition",
            "outline-none focus-visible:ring-4 focus-visible:ring-aurora-teal/40",
            busy ? "cursor-wait" : "",
          ].join(" ")}
        >
          {/* The voice, made light. Fills the glass; reacts to the live level. */}
          <span
            className="pointer-events-none absolute inset-1 rounded-full blur-md transition-[opacity,transform] duration-150"
            style={{
              opacity: lightOpacity,
              transform: `scale(${lightScale})`,
              backgroundImage:
                "radial-gradient(circle at 50% 30%, rgb(var(--aurora-teal)) 0%, rgb(var(--aurora-iris)) 48%, rgb(var(--aurora-violet)) 100%)",
            }}
          />
          {/* Glass shell: a thin bright rim over the light (no dark bezel). */}
          <span
            className={[
              "absolute inset-0 rounded-full border border-white/25 ring-1 ring-inset ring-white/10",
              recording || busy ? "echo-glow" : "transition group-hover:border-white/40",
            ].join(" ")}
          />
          {/* Specular highlight so it reads as a glass sphere, not a flat disc. */}
          <span className="pointer-events-none absolute left-7 top-5 h-6 w-10 -rotate-12 rounded-full bg-white/40 opacity-70 blur-md" />
          {/* Icon. */}
          <span
            className={`relative drop-shadow-lg ${
              recording || busy ? "text-white" : "text-white/90 group-hover:text-white"
            }`}
          >
            {busy ? (
              <Loader2 className="h-9 w-9 animate-spin" />
            ) : recording ? (
              <Square className="h-7 w-7" fill="currentColor" />
            ) : (
              <Mic className="h-9 w-9" />
            )}
          </span>
        </button>
      </div>
      )}

      {!robot && (
        <div className="h-10 w-full max-w-sm">
          <LevelMeter level={level} active={recording} />
        </div>
      )}

      <div className="text-sm text-ink-400">
        {busy ? (
          t("transcribing")
        ) : recording ? (
          <span className="font-mono tabular-nums text-aurora-teal">{fmt(elapsedMs)}</span>
        ) : (
          <span>
            {t("talk_hint_pre")}{" "}
            <kbd className="rounded-md border border-white/10 bg-white/[0.06] px-1.5 py-0.5 font-mono text-xs text-ink-300">
              Space
            </kbd>{" "}
            {t("talk_hint_post")}
          </span>
        )}
      </div>
    </div>
  );
}
