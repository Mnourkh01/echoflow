// Soft start/stop cues via the Web Audio API. Every cue is a short, gentle tone
// with a smooth envelope and a low-pass filter so it sounds warm, not beepy.
// Nothing is bundled (pure synthesis). Volume + pack come from settings.
//
// Design rules that keep these pleasant (not the earlier "weird" swoops):
//   - almost NO pitch glide (a big from→to sweep reads as a toy siren)
//   - warm mid/low pitches (300–700 Hz); high tones sound cheap/piercing
//   - sine/triangle only, minimal consonant partials (octave) at low gain
//   - soft attack + smooth exponential release
//   - a shared low-pass rolls off any harshness

let ctx: AudioContext | null = null;

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
  to: number; // base frequency glide target (keep close to `from`)
  dur: number; // seconds
  attack: number; // seconds to peak
  release: number; // seconds of tail
  partials?: Partial[]; // extra harmonics for a richer timbre
}

interface Pack {
  start: Tone;
  stop: Tone;
}

// Curated, warm cues. `start` sits a touch higher than `stop` so open/close read
// differently, but the glide within each is tiny so nothing swoops.
const PACKS: Record<string, Pack> = {
  // Clean, neutral sine — the default.
  soft: {
    start: { type: "sine", from: 440, to: 466, dur: 0.16, attack: 0.008, release: 0.14 },
    stop: { type: "sine", from: 466, to: 415, dur: 0.16, attack: 0.008, release: 0.14 },
  },
  // Short rounded bloop.
  pop: {
    start: { type: "sine", from: 420, to: 392, dur: 0.09, attack: 0.004, release: 0.085 },
    stop: { type: "sine", from: 360, to: 320, dur: 0.09, attack: 0.004, release: 0.085 },
  },
  // Warm wooden tap (triangle + octave, quick decay).
  marimba: {
    start: { type: "triangle", from: 523, to: 523, dur: 0.18, attack: 0.004, release: 0.17, partials: [{ mult: 2, gain: 0.18 }] },
    stop: { type: "triangle", from: 392, to: 392, dur: 0.18, attack: 0.004, release: 0.17, partials: [{ mult: 2, gain: 0.18 }] },
  },
  // Gentle bell with a soft octave and a longer, mellow tail.
  chime: {
    start: { type: "sine", from: 587, to: 587, dur: 0.3, attack: 0.006, release: 0.28, partials: [{ mult: 2, gain: 0.22 }] },
    stop: { type: "sine", from: 494, to: 494, dur: 0.3, attack: 0.006, release: 0.28, partials: [{ mult: 2, gain: 0.22 }] },
  },
  // Crisp, tiny tick — minimal and quick.
  click: {
    start: { type: "sine", from: 680, to: 680, dur: 0.055, attack: 0.002, release: 0.05 },
    stop: { type: "sine", from: 540, to: 540, dur: 0.055, attack: 0.002, release: 0.05 },
  },
  // Low, mellow, grounded (sine + octave).
  warm: {
    start: { type: "sine", from: 330, to: 349, dur: 0.22, attack: 0.01, release: 0.2, partials: [{ mult: 2, gain: 0.2 }] },
    stop: { type: "sine", from: 330, to: 294, dur: 0.22, attack: 0.01, release: 0.2, partials: [{ mult: 2, gain: 0.2 }] },
  },
};

// Ceiling at full volume, pushed high for clear audibility over media playback.
const MAX_PEAK = 0.92;

function render(tone: Tone, vol: number) {
  const ac = audio();
  if (!ac || vol <= 0) return;
  const now = ac.currentTime;
  const eased = Math.pow(Math.max(0, Math.min(1, vol)), 0.7);
  const peak = Math.max(0.0001, eased * MAX_PEAK);

  // Master gain (one smooth envelope) → low-pass (warmth) → out.
  const master = ac.createGain();
  master.gain.setValueAtTime(0.0001, now);
  master.gain.exponentialRampToValueAtTime(peak, now + tone.attack);
  master.gain.exponentialRampToValueAtTime(0.0001, now + tone.dur + tone.release);

  const lp = ac.createBiquadFilter();
  lp.type = "lowpass";
  lp.frequency.value = 6500; // keep presence/audibility; only tame the very top
  lp.Q.value = 0.5;

  // Makeup gain (well above 1) then a limiter, so the cue is genuinely loud and
  // cuts through media playback without clipping/distortion.
  const makeup = ac.createGain();
  makeup.gain.value = 1.9;
  const limiter = ac.createDynamicsCompressor();
  limiter.threshold.value = -4;
  limiter.knee.value = 4;
  limiter.ratio.value = 20;
  limiter.attack.value = 0.002;
  limiter.release.value = 0.12;
  master.connect(lp).connect(makeup).connect(limiter).connect(ac.destination);

  const voices: Array<{ mult: number; gain: number }> = [{ mult: 1, gain: 1 }, ...(tone.partials ?? [])];
  for (const v of voices) {
    const osc = ac.createOscillator();
    const g = ac.createGain();
    osc.type = tone.type;
    osc.frequency.setValueAtTime(tone.from * v.mult, now);
    if (tone.to !== tone.from) {
      osc.frequency.exponentialRampToValueAtTime(tone.to * v.mult, now + Math.max(0.05, tone.dur * 0.7));
    }
    g.gain.value = v.gain;
    osc.connect(g).connect(master);
    osc.start(now);
    osc.stop(now + tone.dur + tone.release + 0.02);
  }
}

function packOf(key: string): Pack {
  return PACKS[key] ?? PACKS.soft;
}

// Custom recorded cues: drop real audio files (any modern, royalty-free
// notification you like) into `public/sounds/` as start.mp3 + stop.mp3. Real
// recordings are far louder and cleaner than synthesis. Played via
// HTMLAudioElement; if a file is missing, it just no-ops (no crash).
const SAMPLE_URLS: Record<"start" | "stop", string> = {
  start: "/sounds/start.ogg",
  stop: "/sounds/stop.ogg",
};
// How far above the raw file level we can push (a limiter after catches peaks),
// so a soft recording still comes out loud.
const SAMPLE_BOOST = 5;
const buffers: { start?: AudioBuffer; stop?: AudioBuffer } = {};

async function playSample(which: "start" | "stop", vol: number) {
  const ac = audio();
  if (!ac || vol <= 0) return;
  try {
    let buf = buffers[which];
    if (!buf) {
      const res = await fetch(SAMPLE_URLS[which]);
      buf = await ac.decodeAudioData(await res.arrayBuffer());
      buffers[which] = buf;
    }
    const src = ac.createBufferSource();
    src.buffer = buf;
    // Amplify well past 1.0 (Web Audio allows it), then limit so it stays loud
    // but never clips — this is why it can be louder than the file itself.
    const gain = ac.createGain();
    gain.gain.value = Math.max(0, Math.min(1, vol)) * SAMPLE_BOOST;
    const limiter = ac.createDynamicsCompressor();
    limiter.threshold.value = -3;
    limiter.knee.value = 4;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.002;
    limiter.release.value = 0.12;
    src.connect(gain).connect(limiter).connect(ac.destination);
    src.start();
  } catch {
    /* ignore */
  }
}

export const playStart = () => (pack === "custom" ? playSample("start", volume) : render(packOf(pack).start, volume));
export const playStop = () => (pack === "custom" ? playSample("stop", volume) : render(packOf(pack).stop, volume));

/** Play a one-off preview (used by the settings picker) without changing config. */
export function previewSound(packKey: string, vol: number, which: "start" | "stop" = "start") {
  if (packKey === "custom") {
    playSample(which, Math.max(0, Math.min(1, vol / 100)));
    return;
  }
  const p = packOf(packKey);
  render(which === "stop" ? p.stop : p.start, Math.max(0, Math.min(1, vol / 100)));
}

/** The ordered list of pack keys for the settings UI. */
export const SOUND_PACKS = ["soft", "pop", "marimba", "chime", "click", "warm", "custom"] as const;
