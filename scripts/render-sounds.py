"""Render EchoFlow's start/stop cues to src/assets/sounds/<pack>-<start|stop>.ogg.

The user picked these two by ear on 2026-09-12 out of forty candidates: short,
clean, bright pips. Each is a pure tone with raised-cosine edges (no click) and
no saturation, mastered to the same momentary loudness so switching packs never
changes how loud a volume setting is. Do not add a pack the user has not heard
and chosen. Files of packs no longer listed here are deleted on each run.

    python scripts/render-sounds.py      # needs numpy, and ffmpeg with libvorbis on PATH
"""

import glob
import os
import re
import subprocess
import tempfile
import wave

import numpy as np

SR = 48_000
# Loudest 400 ms window (EBU R128 momentary), per file. The old synthesized beep
# measured -15.5 LUFS at 100% volume; this sits well above it with peak headroom.
TARGET_LUFS = -11.0
PEAK_CEILING = 10 ** (-1.0 / 20)  # -1 dBFS
OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "src", "assets", "sounds"))


def tone(freq, dur, attack=0.004, release=0.02):
    t = np.arange(int(dur * SR)) / SR
    # Phase from the running sum, the formula the listening round used, so these
    # files decode to exactly the audio the user picked.
    x = np.sin(2 * np.pi * np.cumsum(np.full_like(t, freq)) / SR)
    env = np.ones_like(t)
    a, r = max(1, int(attack * SR)), max(1, int(release * SR))
    env[:a] *= np.sin(np.linspace(0, np.pi / 2, a)) ** 2
    env[-r:] *= np.cos(np.linspace(0, np.pi / 2, r)) ** 2
    return x * env


def seq(*parts):
    """Tones placed at start times (seconds), plus a 10 ms tail."""
    end = max(int(at * SR) + len(sig) for sig, at in parts)
    x = np.zeros(end + int(0.01 * SR))
    for sig, at in parts:
        i = int(at * SR)
        x[i:i + len(sig)] += sig
    return x


# Start rises, stop falls, so the two read as open and close.
PACKS = {
    "pip": {"start": lambda: tone(1500, 0.08), "stop": lambda: tone(1000, 0.08)},
    "pip_updown": {
        "start": lambda: seq((tone(1000, 0.06), 0), (tone(1500, 0.06), 0.075)),
        "stop": lambda: seq((tone(1500, 0.06), 0), (tone(1000, 0.06), 0.075)),
    },
}


def write_wav(path, x):
    pcm = np.clip(np.round(x * 32767), -32768, 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def measure(path):
    """Loudest momentary loudness (LUFS) and true peak (dBTP) via ffmpeg ebur128.
    One second of padding lets clips shorter than the 400 ms window register."""
    err = subprocess.run(
        ["ffmpeg", "-hide_banner", "-nostats", "-i", path, "-af", "apad=pad_dur=1,ebur128=peak=true", "-f", "null", "-"],
        capture_output=True, text=True,
    ).stderr
    m = [float(v) for v in re.findall(r"M:\s*(-?\d+(?:\.\d+)?)", err)]
    tp = re.findall(r"Peak:\s*(-?\d+(?:\.\d+)?)\s*dBFS", err)
    return max(m), float(tp[-1]) if tp else float("nan")


def limit(x):
    """Soft limiter: linear at low level, bends smoothly into the ceiling."""
    return PEAK_CEILING * np.tanh(x / PEAK_CEILING)


def master(x, wav_path):
    """Gain to the target loudness, never more than 6 dB into the limiter."""
    x = x / (np.max(np.abs(x)) + 1e-12)
    gain = 0.5
    for _ in range(5):
        write_wav(wav_path, limit(x * gain))
        m, _ = measure(wav_path)
        if abs(m - TARGET_LUFS) < 0.3:
            break
        gain = min(gain * 10 ** ((TARGET_LUFS - m) / 20), 2 * PEAK_CEILING)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    wanted = {f"{p}-{cue}.ogg" for p in PACKS for cue in ("start", "stop")}
    for old in glob.glob(os.path.join(OUT_DIR, "*.ogg")):
        if os.path.basename(old) not in wanted:
            os.remove(old)
    with tempfile.TemporaryDirectory() as tmp:
        for name, cues in PACKS.items():
            for cue, build in cues.items():
                wav = os.path.join(tmp, f"{name}-{cue}.wav")
                master(build(), wav)
                out = os.path.join(OUT_DIR, f"{name}-{cue}.ogg")
                # bitexact: a fixed Ogg stream serial, so a re-run does not change the files
                subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", wav, "-c:a", "libvorbis",
                                "-q:a", "6", "-fflags", "+bitexact", "-flags:a", "+bitexact", out], check=True)
                m, tp = measure(out)
                print(f"{name}-{cue}: momentary max {m:.1f} LUFS, true peak {tp:.1f} dBTP, "
                      f"{os.path.getsize(out) // 1024} KB")


if __name__ == "__main__":
    main()
