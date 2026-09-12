// Start/stop cues are short pips the user picked by ear, rendered and
// loudness-matched offline by scripts/render-sounds.py into src/assets/sounds.
// Both packs master to the same level, so switching never changes how loud a
// volume setting is.

type Cue = "start" | "stop";

/** The ordered list of pack keys for the settings UI; the first is the default. */
export const SOUND_PACKS = ["pip", "pip_updown"] as const;
const DEFAULT_PACK = SOUND_PACKS[0];

/** Saved settings may still name a retired pack (soft, chime, custom...): play the default. */
export const knownPack = (key: string): string =>
  (SOUND_PACKS as readonly string[]).includes(key) ? key : DEFAULT_PACK;

const FILES = import.meta.glob<string>("../assets/sounds/*.ogg", {
  eager: true,
  query: "?url",
  import: "default",
});

let ctx: AudioContext | null = null;
let volume = 0.7; // 0..1
let pack: string = DEFAULT_PACK;
const buffers = new Map<string, Promise<AudioBuffer | null>>();

function audio(): AudioContext | null {
  try {
    if (!ctx) ctx = new AudioContext();
    if (ctx.state === "suspended") ctx.resume().catch(() => {});
    return ctx;
  } catch {
    return null;
  }
}

const urlFor = (packKey: string, cue: Cue): string | undefined =>
  FILES[`../assets/sounds/${knownPack(packKey)}-${cue}.ogg`];

function load(url: string): Promise<AudioBuffer | null> {
  const cached = buffers.get(url);
  if (cached) return cached;
  const ac = audio();
  if (!ac) return Promise.resolve(null);
  const p = fetch(url)
    .then((r) => r.arrayBuffer())
    .then((b) => ac.decodeAudioData(b))
    .catch(() => null);
  buffers.set(url, p);
  void p.then((b) => {
    if (!b) buffers.delete(url); // a failed load may succeed on the next try
  });
  return p;
}

// 70% (the default) plays a file at its mastered level; above that the gain
// climbs to about +4 dB into a limiter, so 100% is clearly louder than 70%.
const gainFor = (vol: number) => 1.6 * Math.pow(vol, 1.5);

async function play(packKey: string, cue: Cue, vol: number) {
  if (vol <= 0) return;
  const ac = audio();
  const url = urlFor(packKey, cue);
  if (!ac || !url) return;
  const buf = await load(url);
  if (!buf) return;
  const src = ac.createBufferSource();
  src.buffer = buf;
  const gain = ac.createGain();
  gain.gain.value = gainFor(Math.min(1, vol));
  const limiter = ac.createDynamicsCompressor();
  limiter.threshold.value = -1;
  limiter.knee.value = 0;
  limiter.ratio.value = 20;
  limiter.attack.value = 0.001;
  limiter.release.value = 0.1;
  src.connect(gain).connect(limiter).connect(ac.destination);
  src.start();
}

/** Keep the player in sync with the user's settings; preloads the chosen pack. */
export function configureSound(opts: { volume?: number; pack?: string }) {
  if (typeof opts.volume === "number") volume = Math.max(0, Math.min(1, opts.volume / 100));
  if (opts.pack) pack = opts.pack;
  for (const cue of ["start", "stop"] as const) {
    const url = urlFor(pack, cue);
    if (url) void load(url);
  }
}

export const playStart = () => void play(pack, "start", volume);
export const playStop = () => void play(pack, "stop", volume);

/** Play a one-off preview (used by the settings picker) without changing config. */
export function previewSound(packKey: string, vol: number, cue: Cue = "start") {
  void play(packKey, cue, Math.max(0, Math.min(1, vol / 100)));
}
