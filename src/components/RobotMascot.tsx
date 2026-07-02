import { useEffect, useRef, useState } from "react";
import { Heart, Sparkles, PenLine } from "lucide-react";
import type { RecState } from "./RecordControl";

type Mood = "happy" | "sad" | "update" | "switch";

interface Props {
  state: RecState;
  level: number; // 0..1 live mic level
  onToggle: () => void;
  /** Transient mood from the app: happy = result · sad = error/no-speech ·
   *  update = new version · switch = mode/language change. */
  flash?: Mood | null;
  label?: string;
}

let heartSeq = 0;

const C = 120; // sphere centre
// Dense radial "data streaks" forming the corona. Deterministic pseudo-random
// lengths/opacity so the layout is stable across renders (no flicker on state).
const STREAKS = Array.from({ length: 72 }, (_, i) => {
  const a = (i / 72) * Math.PI * 2;
  const len = 6 + ((i * 37) % 26);
  const op = 0.22 + ((i * 29) % 60) / 100;
  const inner = 58 + ((i * 17) % 8);
  return {
    x1: C + Math.cos(a) * inner,
    y1: C + Math.sin(a) * inner,
    x2: C + Math.cos(a) * (inner + len),
    y2: C + Math.sin(a) * (inner + len),
    op,
    w: (i % 6 === 0 ? 2.2 : 1),
  };
});
// Glinting nodes scattered on the sphere.
const NODES = Array.from({ length: 22 }, (_, i) => {
  const a = i * 2.399963; // golden angle
  const r = 20 + ((i * 53) % 62);
  return { x: C + Math.cos(a) * r, y: C + Math.sin(a) * r * 0.86, r: 1.2 + ((i * 13) % 3), d: (i % 6) * 0.18 };
});
// Latitude rings (perspective ellipses) for the wireframe globe.
const LATS = [
  { ry: 20 }, { ry: 40 }, { ry: 58 }, { ry: 72 },
];

/**
 * EchoFlow's mascot — a holographic data sphere (JARVIS-style): a wireframe globe
 * with orbiting rings and a dense corona of data-streaks spinning around a hot
 * core, in the app's aurora accent. It IS the record button (tap = start/stop).
 *   idle    → the sphere turns slowly, corona flickers, core breathes
 *   listen  → it energizes: brighter, larger, glow pulses with your voice
 *   curious → a "?" when it's recording but hears nothing
 *   think   → a fast scan ring sweeps while transcribing
 *   result  → a sparkle + bright surge
 *   tap     → hearts spark off it
 *   update  → a sparkle surge · switch → a surge on mode/language change
 *   error   → the whole sphere turns red + shakes
 * Pure SVG + CSS keyframes (index.css); reduced-motion flattens it.
 */
export default function RobotMascot({ state, level, onToggle, flash, label }: Props) {
  const [hearts, setHearts] = useState<{ id: number; x: number }[]>([]);
  const [waiting, setWaiting] = useState(false);
  const [pop, setPop] = useState(false); // one-shot spring pop on switch / result / tap
  const lastVoice = useRef(0);

  const recording = state === "recording";
  const busy = state === "transcribing";
  const lvl = Math.max(0, Math.min(1, level));

  useEffect(() => {
    if (!recording) {
      setWaiting(false);
      return;
    }
    lastVoice.current = performance.now();
    setWaiting(false);
    const id = window.setInterval(() => {
      if (performance.now() - lastVoice.current > 1300) setWaiting(true);
    }, 250);
    return () => window.clearInterval(id);
  }, [recording]);
  useEffect(() => {
    if (recording && level > 0.045) {
      lastVoice.current = performance.now();
      setWaiting((w) => (w ? false : w));
    }
  }, [level, recording]);

  // A single smooth spring pop whenever the mode/language switches or a result
  // lands — the reaction is a scale that eases (transition below), so it never
  // snaps the position the way swapping motion classes did.
  useEffect(() => {
    if (flash === "switch" || flash === "happy" || flash === "update") {
      setPop(true);
      const id = window.setTimeout(() => setPop(false), 430);
      return () => window.clearTimeout(id);
    }
  }, [flash]);

  const sad = flash === "sad";
  const curious = recording && waiting && !flash;
  const celebrate = flash === "happy" || flash === "update" || flash === "switch";

  function react() {
    const ids = [heartSeq++, heartSeq++, heartSeq++];
    const batch = ids.map((id, i) => ({ id, x: (i - 1) * 18 + (Math.random() * 8 - 4) }));
    setHearts((h) => [...h, ...batch]);
    const idset = new Set(ids);
    window.setTimeout(() => setHearts((h) => h.filter((x) => !idset.has(x.id))), 1100);
    setPop(true);
    window.setTimeout(() => setPop(false), 430);
  }
  function handleClick() {
    react();
    if (!busy) onToggle();
  }

  // Colour: the app accent normally, red on error. Everything inherits it via
  // currentColor, so it recolours with the palette.
  const col = sad ? "rgb(239 68 68)" : "rgb(var(--aurora-teal))";
  const col2 = sad ? "rgb(190 30 60)" : "rgb(var(--aurora-iris))";
  const energy = recording ? 0.55 + lvl * 0.45 : busy ? 0.55 : celebrate ? 0.8 : 0.4;
  const glowBlur = 18 + (recording ? lvl * 34 : 0) + (celebrate ? 16 : 0);
  const scale = 1 + lvl * 0.14; // voice-reactive only; celebration is the pop layer
  // State-driven brightness (NOT voice-driven), so power-up/down between states
  // eases smoothly without making the live voice reaction mushy.
  const groupOpacity = sad ? 0.95 : recording ? 1 : busy ? 0.92 : celebrate ? 1 : 0.72;

  const shake = sad ? "rb-shake" : "";

  return (
    <button
      onClick={handleClick}
      aria-label={label}
      className="group relative grid place-items-center rounded-full p-1 outline-none focus-visible:ring-4 focus-visible:ring-aurora-teal/30"
    >
      <span className="rb-shadow pointer-events-none absolute -bottom-1 left-1/2 h-3 w-28 -translate-x-1/2 rounded-[50%] bg-black/45 blur-[3px]" />

      {hearts.map((h) => (
        <span key={h.id} className="rb-heart-rise pointer-events-none absolute left-1/2 top-4 z-10" style={{ marginLeft: h.x }}>
          <Heart className="h-4 w-4" style={{ color: "#ff8fc7" }} fill="#ff8fc7" />
        </span>
      ))}
      {curious && (
        <span className="rb-q pointer-events-none absolute right-3 top-1 z-10">
          <span className="grid h-8 w-8 place-items-center rounded-full rounded-bl-md bg-white text-lg font-extrabold text-[#262a66] shadow-lg">?</span>
        </span>
      )}
      {/* result = "it wrote your words": a pen. update = a sparkle. */}
      {flash === "happy" && (
        <span className="rb-q pointer-events-none absolute left-3 top-2 z-10">
          <span className="grid h-8 w-8 place-items-center rounded-full bg-white/95 shadow-lg">
            <PenLine className="h-5 w-5" style={{ color: "#262a66" }} strokeWidth={2.5} />
          </span>
        </span>
      )}
      {flash === "update" && (
        <span className="rb-q pointer-events-none absolute left-3 top-2 z-10">
          <Sparkles className="h-6 w-6" style={{ color: "rgb(var(--aurora-teal))" }} fill="rgb(var(--aurora-teal))" />
        </span>
      )}

      <div className="rb-float">
        <div
          style={{
            transform: `scale(${pop ? 1.12 : 1})`,
            transformOrigin: "center",
            transition: "transform 0.42s cubic-bezier(0.34, 1.56, 0.64, 1)",
          }}
        >
        <svg
          viewBox="0 0 240 240"
          className={`h-56 w-56 ${shake}`}
          style={{
            color: col,
            filter: `drop-shadow(0 0 ${glowBlur}px rgb(${sad ? "239 68 68" : "var(--aurora-teal)"} / ${0.35 + energy * 0.35}))`,
            opacity: 0.96,
            transition: "color 0.4s ease, filter 0.22s ease",
          }}
        >
          <defs>
            <radialGradient id="jvCore" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor="#ffffff" stopOpacity="0.95" />
              <stop offset="35%" stopColor={col} stopOpacity="0.9" />
              <stop offset="100%" stopColor={col} stopOpacity="0" />
            </radialGradient>
            <radialGradient id="jvHaze" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor={col} stopOpacity="0.28" />
              <stop offset="70%" stopColor={col2} stopOpacity="0.10" />
              <stop offset="100%" stopColor={col2} stopOpacity="0" />
            </radialGradient>
            {/* Limb shading: light from upper-left, darkened rim — turns the flat
                wireframe disc into a ball without any SVG filter cost. */}
            <radialGradient id="jvShade" cx="38%" cy="32%" r="78%">
              <stop offset="0%" stopColor="#ffffff" stopOpacity="0.07" />
              <stop offset="55%" stopColor="#000000" stopOpacity="0" />
              <stop offset="100%" stopColor="#000000" stopOpacity="0.5" />
            </radialGradient>
          </defs>

          <g
            style={{
              transformBox: "view-box",
              transformOrigin: "120px 120px",
              transform: `scale(${scale})`,
              opacity: groupOpacity,
              transition: "opacity 0.45s ease, transform 0.12s linear",
            }}
          >
            {/* inner haze */}
            <circle cx={C} cy={C} r="86" fill="url(#jvHaze)" />

            {/* radial data-streak corona (slow spin). The outer wrapper carries
                the live-voice reaction (breathes wider + brighter as you speak)
                so it never fights the inner group's rotation/flicker animation. */}
            <g
              style={{
                transformBox: "view-box",
                transformOrigin: "120px 120px",
                transform: `scale(${1 + (recording ? lvl * 0.13 : 0)})`,
                opacity: 0.5 + energy * 0.5,
                transition: "opacity 0.3s ease, transform 0.1s linear",
              }}
            >
              <g className="jv-rot jv-flicker" stroke="currentColor" strokeLinecap="round">
                {STREAKS.map((s, i) => (
                  <line key={i} x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} strokeWidth={s.w} opacity={s.op} />
                ))}
              </g>
            </g>

            {/* wireframe globe: latitude ellipses + a spinning longitude set */}
            <g fill="none" stroke="currentColor" strokeWidth="1" opacity="0.5">
              <circle cx={C} cy={C} r="72" />
              {LATS.map((l, i) => (
                <ellipse key={i} cx={C} cy={C} rx="72" ry={l.ry} />
              ))}
            </g>
            <g className="jv-globe" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.45">
              <ellipse cx={C} cy={C} rx="72" ry="30" />
              <ellipse cx={C} cy={C} rx="72" ry="55" />
            </g>
            {/* limb shading over the wireframe: reads as a lit sphere, not a disc */}
            <circle cx={C} cy={C} r="72" fill="url(#jvShade)" opacity="0.6" />

            {/* orbiting rings (the JARVIS swoosh arcs) */}
            <g className="jv-rot-mid">
              <ellipse cx={C} cy={C} rx="88" ry="30" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.7" transform={`rotate(22 ${C} ${C})`} />
            </g>
            <g className="jv-rot-fast">
              <ellipse cx={C} cy={C} rx="84" ry="26" fill="none" stroke={col2} strokeWidth="1.6" opacity="0.6" transform={`rotate(-38 ${C} ${C})`} strokeDasharray="6 10" />
            </g>
            {/* faint third orbit, counter-rotating: adds parallax depth at rest */}
            <g className="jv-rot-rev">
              <ellipse cx={C} cy={C} rx="93" ry="34" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.32" transform={`rotate(64 ${C} ${C})`} strokeDasharray="2 13" />
            </g>

            {/* glinting nodes */}
            <g className="jv-rot-mid" fill="currentColor">
              {NODES.map((n, i) => (
                <circle key={i} className="rb-twinkle" cx={n.x} cy={n.y} r={n.r} style={{ animationDelay: `${n.d}s` }} />
              ))}
            </g>

            {/* scan ring while thinking */}
            {busy && (
              <circle className="orb-ring" cx={C} cy={C} r="80" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeDasharray="14 240" style={{ transformOrigin: "120px 120px" }} />
            )}

            {/* hot core: wide bloom + tight bloom + white-hot pinpoint */}
            <circle className="jv-core" cx={C} cy={C} r="30" fill="url(#jvCore)" />
            <circle className="jv-core" cx={C} cy={C} r="16" fill="url(#jvCore)" opacity="0.85" />
            <circle className="jv-core" cx={C} cy={C} r="7" fill="#ffffff" opacity="0.9" />
          </g>
        </svg>
        </div>
      </div>
    </button>
  );
}
