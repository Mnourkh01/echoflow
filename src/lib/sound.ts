// Soft start/stop cues via the Web Audio API. Every pack is a short, gentle
// tone with a smooth attack and release (no clicks, no harsh beeps). Volume and
// the chosen pack come from settings; nothing is bundled (pure synthesis).

let ctx: AudioContext | null = null;

// Module-level config kept in sync with settings (set via `configureSound`).
let volume = 0.7; // 0..1
let pack = "soft";

function audio(): AudioContext | null {
  try {
    if (!ctx) ctx = new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/** Keep the synth in sync with the user's settings. */
export function configureSound(opts: { volume?: number; pack?: string }) {
  if (typeof opts.volume === "number") volume = Math.max(0, Math.min(1, opts.volume / 100));
  if (opts.pack) pack = opts.pack;
}

type Partial = { mult: number; gain: number }; // harmonic relative to the base
interface Tone {
  type: OscillatorType;
  from: number; // base frequency at onset
  to: number; // base frequency glide target
  dur: number; // seconds
  attack: number; // seconds to peak
  release: number; // seconds of tail
  partials?: Partial[]; // extra harmonics for richer timbres (bell/glass)
}

interface Pack {
  start: Tone;
  stop: Tone;
}

// All packs are deliberately mellow: low gain, soft envelopes, pleasant
// intervals. `start` rises (open), `stop` falls (close).
const PACKS: Record<string, Pack> = {
  // Clean sine swoops — the original cue, gentle and neutral.
  soft: {
    start: { type: "sine", from: 520, to: 740, dur: 0.18, attack: 0.02, release: 0.14 },
    stop: { type: "sine", from: 700, to: 470, dur: 0.18, attack: 0.02, release: 0.14 },
  },
  // Warm wooden pluck (triangle, quick decay) — like a marimba tap.
  marimba: {
    start: { type: "triangle", from: 392, to: 523, dur: 0.2, attack: 0.006, release: 0.18, partials: [{ mult: 2, gain: 0.25 }] },
    stop: { type: "triangle", from: 523, to: 349, dur: 0.2, attack: 0.006, release: 0.18, partials: [{ mult: 2, gain: 0.25 }] },
  },
  // Bright shimmer with a high partial — airy, glassy.
  glass: {
    start: { type: "sine", from: 680, to: 920, dur: 0.26, attack: 0.015, release: 0.22, partials: [{ mult: 2.01, gain: 0.3 }, { mult: 3, gain: 0.12 }] },
    stop: { type: "sine", from: 900, to: 640, dur: 0.26, attack: 0.015, release: 0.22, partials: [{ mult: 2.01, gain: 0.3 }] },
  },
  // Rounded short bloop — minimal and quick.
  pop: {
    start: { type: "sine", from: 560, to: 620, dur: 0.1, attack: 0.008, release: 0.08 },
    stop: { type: "sine", from: 520, to: 440, dur: 0.1, attack: 0.008, release: 0.08 },
  },
  // Bell-like with stacked partials and a long, soft tail.
  chime: {
    start: { type: "sine", from: 660, to: 660, dur: 0.4, attack: 0.01, release: 0.36, partials: [{ mult: 2, gain: 0.4 }, { mult: 2.76, gain: 0.18 }] },
    stop: { type: "sine", from: 528, to: 528, dur: 0.4, attack: 0.01, release: 0.36, partials: [{ mult: 2, gain: 0.4 }, { mult: 2.76, gain: 0.18 }] },
  },
};

// Ceiling at full volume. Raised from the old 0.26 so users who want a clearly
// audible cue can get one, while a gentle perceptual curve (below) keeps the low
// and middle of the slider soft, so it's "loud enough but not too loud".
const MAX_PEAK = 0.4;

function render(tone: Tone, vol: number) {
  const ac = audio();
  if (!ac || vol <= 0) return;
  const now = ac.currentTime;
  // Map the 0..1 slider to loudness with a mild curve: ear-perceived loudness is
  // roughly logarithmic, so a linear slider feels like it does little until the
  // very top. Easing (vol^1.3) keeps small values quiet for fine control and
  // lets the top of the slider reach the full, louder ceiling.
  const eased = Math.pow(Math.max(0, Math.min(1, vol)), 1.3);
  const peak = Math.max(0.0001, eased * MAX_PEAK);

  // Master gain shared by the base tone + any partials, with one smooth envelope.
  const master = ac.createGain();
  master.gain.setValueAtTime(0.0001, now);
  master.gain.exponentialRampToValueAtTime(peak, now + tone.attack);
  master.gain.exponentialRampToValueAtTime(0.0001, now + tone.dur + tone.release);
  master.connect(ac.destination);

  const voices: Array<{ mult: number; gain: number }> = [
    { mult: 1, gain: 1 },
    ...(tone.partials ?? []),
  ];
  for (const v of voices) {
    const osc = ac.createOscillator();
    const g = ac.createGain();
    osc.type = tone.type;
    osc.frequency.setValueAtTime(tone.from * v.mult, now);
    osc.frequency.exponentialRampToValueAtTime(tone.to * v.mult, now + Math.max(0.05, tone.dur * 0.7));
    g.gain.value = v.gain;
    osc.connect(g).connect(master);
    osc.start(now);
    osc.stop(now + tone.dur + tone.release + 0.02);
  }
}

function packOf(key: string): Pack {
  return PACKS[key] ?? PACKS.soft;
}

export const playStart = () => render(packOf(pack).start, volume);
export const playStop = () => render(packOf(pack).stop, volume);

/** Play a one-off preview (used by the settings picker) without changing config. */
export function previewSound(packKey: string, vol: number, which: "start" | "stop" = "start") {
  const p = packOf(packKey);
  render(which === "stop" ? p.stop : p.start, Math.max(0, Math.min(1, vol / 100)));
}

/** The ordered list of pack keys for the settings UI. */
export const SOUND_PACKS = ["soft", "marimba", "glass", "pop", "chime"] as const;
