//! Microphone capture and resampling to the 16 kHz mono format Whisper needs.
//!
//! cpal's `Stream` is not `Send`, so it cannot live inside the Tauri-managed
//! state. We keep it on a dedicated capture thread and hand the recorded
//! samples back when the thread is told to stop.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

pub const TARGET_RATE: u32 = 16_000;

pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct RecordingHandle {
    stop_flag: Arc<AtomicBool>,
    join: JoinHandle<Result<CapturedAudio>>,
}

impl RecordingHandle {
    /// Stop capture and return everything recorded so far.
    pub fn stop(self) -> Result<CapturedAudio> {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.join
            .join()
            .map_err(|_| anyhow!("capture thread panicked"))?
    }
}

/// List names of available input devices.
pub fn list_input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                out.push(name);
            }
        }
    }
    Ok(out)
}

fn peak_level(level: &AtomicU32, data: &[f32]) {
    let peak = data.iter().fold(0f32, |m, &s| m.max(s.abs()));
    level.store(peak.to_bits(), Ordering::Relaxed);
}

/// Start capturing from the named device (or system default). Blocks only long
/// enough to confirm the stream opened, then records on a background thread.
pub fn start(device_name: Option<String>, level: Arc<AtomicU32>) -> Result<RecordingHandle> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_thread = stop_flag.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

    let join = std::thread::spawn(move || -> Result<CapturedAudio> {
        let host = cpal::default_host();
        let device = match &device_name {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| anyhow!("input device '{name}' not found"))?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow!("no default microphone found"))?,
        };

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let err_fn = |e| log::error!("audio stream error: {e}");

        let stream = {
            let buf = buffer.clone();
            let lvl = level.clone();
            match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        buf.lock().extend_from_slice(data);
                        peak_level(&lvl, data);
                    },
                    err_fn,
                    None,
                )?,
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        peak_level(&lvl, &f);
                        buf.lock().extend(f);
                    },
                    err_fn,
                    None,
                )?,
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &_| {
                        let f: Vec<f32> =
                            data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).collect();
                        peak_level(&lvl, &f);
                        buf.lock().extend(f);
                    },
                    err_fn,
                    None,
                )?,
                other => bail!("unsupported sample format: {other:?}"),
            }
        };

        stream.play()?;
        ready_tx.send(Ok(())).ok();

        while !stop_thread.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
        }

        drop(stream);
        let samples = std::mem::take(&mut *buffer.lock());
        Ok(CapturedAudio {
            samples,
            sample_rate,
            channels,
        })
    });

    // Surface open/permission errors synchronously to the caller.
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(RecordingHandle { stop_flag, join }),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            // Thread returned before signalling ready: join to get the real error.
            match join.join() {
                Ok(Err(e)) => Err(e),
                Ok(Ok(_)) => Err(anyhow!("capture stopped before it started")),
                Err(_) => Err(anyhow!("capture thread panicked on start")),
            }
        }
    }
}

/// Root-mean-square loudness of the RAW capture (before gain normalization).
/// Used to tell real speech from a silent mis-click. Must run on raw samples:
/// `prepare_for_whisper` normalizes the peak, which would erase this signal.
pub fn rms_level(captured: &CapturedAudio) -> f32 {
    let mono = to_mono(&captured.samples, captured.channels);
    if mono.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = mono.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / mono.len() as f64).sqrt() as f32
}

/// Average all channels into one.
/// Whether the raw capture holds speech at all, judged on 30 ms frames of the
/// mono signal: the loud frames (95th percentile) must be plainly audible, or
/// stand 2.5x above the room floor (20th percentile). A whole-clip RMS gate of
/// 0.0010 threw away whispered dictation that Whisper could still read
/// (measured at raw RMS 0.0005); 3.5x still dropped it over a normal mic floor.
pub fn speech_present(captured: &CapturedAudio) -> bool {
    let mono = to_mono(&captured.samples, captured.channels);
    let frame = (captured.sample_rate as usize * 30 / 1000).max(1);
    let rms: Vec<f32> = mono.chunks(frame).map(crate::vad::frame_rms).collect();
    if rms.is_empty() {
        return false;
    }
    let floor = crate::vad::percentile(&rms, 0.2);
    let loud = crate::vad::percentile(&rms, 0.95);
    loud >= 0.0010 || (loud >= 0.000_25 && loud >= floor * 2.5)
}

pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Zero crossings of the sinc kernel on each side of its centre: 48 puts the
/// band edge within ~0.6 kHz of the cutoff at ~300 taps per output sample.
const SINC_ZERO_CROSSINGS: f64 = 48.0;

/// Band-limited resample: a Blackman-Harris windowed-sinc low-pass run as a
/// polyphase filter bank (one row of taps per output phase), cutoff at 47% of
/// the lower rate, near the ffmpeg resampler Whisper's training audio went
/// through. From the 48 kHz laptop mic the speech band stays flat to ~6.9 kHz
/// and everything above ~8.1 kHz is removed; the old moving average plus
/// linear interpolation let 8-24 kHz hiss fold back into the speech band at only
/// -3 to -12 dB, smearing the consonants Whisper tells words apart by.
pub fn resample(mono: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || from_rate == 0 || to_rate == 0 || mono.is_empty() {
        return mono.to_vec();
    }
    let g = gcd(from_rate, to_rate);
    let (up, down) = ((to_rate / g) as usize, (from_rate / g) as usize);
    // Cutoff in cycles per input sample.
    let fc = 0.47 * from_rate.min(to_rate) as f64 / from_rate as f64;
    let half = (SINC_ZERO_CROSSINGS / (2.0 * fc)).ceil() as usize;
    let taps = 2 * half;

    // Row p serves outputs that land p/up of the way past input sample `base`;
    // tap k reads input `base + k + 1 - half`. Each row is scaled to unity gain.
    let mut bank = vec![0f32; up * taps];
    for (p, row) in bank.chunks_exact_mut(taps).enumerate() {
        let frac = p as f64 / up as f64;
        let coeffs: Vec<f64> = (0..taps)
            .map(|k| {
                let t = (k + 1) as f64 - half as f64 - frac;
                let arg = std::f64::consts::PI * 2.0 * fc * t;
                let sinc = if t == 0.0 { 1.0 } else { arg.sin() / arg };
                sinc * blackman_harris(t / half as f64)
            })
            .collect();
        let sum: f64 = coeffs.iter().sum();
        for (slot, c) in row.iter_mut().zip(&coeffs) {
            *slot = (c / sum) as f32;
        }
    }

    let out_len = ((mono.len() * up + down / 2) / down).max(1);
    (0..out_len)
        .map(|n| {
            let pos = n * down;
            let row = &bank[(pos % up) * taps..][..taps];
            dot_zero_padded(mono, (pos / up) as isize + 1 - half as isize, row)
        })
        .collect()
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// 4-term Blackman-Harris window over x in [-1, 1] (about -92 dB sidelobes).
fn blackman_harris(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        return 0.0;
    }
    let a = std::f64::consts::PI * x;
    0.35875 + 0.48829 * a.cos() + 0.14128 * (2.0 * a).cos() + 0.01168 * (3.0 * a).cos()
}

/// `taps` dotted with `x[start..]`, reading zeros outside `x`.
fn dot_zero_padded(x: &[f32], start: isize, taps: &[f32]) -> f32 {
    let lo = start.max(0) as usize;
    let hi = (start + taps.len() as isize).clamp(0, x.len() as isize) as usize;
    if lo >= hi {
        return 0.0;
    }
    let xs = &x[lo..hi];
    let ts = &taps[(lo as isize - start) as usize..][..xs.len()];
    // Eight independent sums let the compiler vectorize the loop.
    let (cx, ct) = (xs.chunks_exact(8), ts.chunks_exact(8));
    let tail: f32 = cx.remainder().iter().zip(ct.remainder()).map(|(a, b)| a * b).sum();
    let mut acc = [0f32; 8];
    for (a, b) in cx.zip(ct) {
        for j in 0..8 {
            acc[j] += a[j] * b[j];
        }
    }
    acc.iter().sum::<f32>() + tail
}

/// Resample mono audio to 16 kHz (Whisper's required rate).
pub fn resample_to_16k(mono: &[f32], from_rate: u32) -> Vec<f32> {
    resample(mono, from_rate, TARGET_RATE)
}

/// One-pole high-pass at `cutoff_hz`. Removes the low-frequency rumble and hum
/// (desk thumps, fans, AC, electrical hum) that sits below any speech content —
/// it costs nothing, cleans the signal for RNNoise/Whisper, and stops that
/// energy from dominating the loudness normalization and the VAD's noise floor.
fn high_pass(samples: &mut [f32], sample_rate: u32, cutoff_hz: f32) {
    if samples.is_empty() || sample_rate == 0 {
        return;
    }
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / sample_rate as f32;
    let a = rc / (rc + dt);
    let mut prev_x = samples[0];
    let mut prev_y = 0.0f32;
    for s in samples.iter_mut() {
        let x = *s;
        let y = a * (prev_y + x - prev_x);
        prev_x = x;
        prev_y = y;
        *s = y;
    }
}

/// RNNoise denoise. Expects 48 kHz mono; processes 480-sample (10 ms) frames.
/// RNNoise works in the i16 amplitude range, so scale up before and back after.
///
/// RNNoise's noise estimate needs a few frames to converge; without a warmup it
/// audibly gates/mangles the first word of the clip. We prime the state on a
/// dozen frames of the clip's own opening (discarded), so the real first frames
/// are processed by an already-converged model.
fn denoise_48k(mono48: &[f32]) -> Vec<f32> {
    use nnnoiseless::DenoiseState;
    let frame = DenoiseState::FRAME_SIZE;
    let mut st = DenoiseState::new();
    let mut in_buf = vec![0f32; frame];
    let mut out_buf = vec![0f32; frame];

    // Warmup: run the first ~120 ms through the state and throw the output away.
    let warm = mono48.len().min(frame * 12);
    for chunk in mono48[..warm].chunks(frame) {
        for (i, slot) in in_buf.iter_mut().enumerate() {
            *slot = chunk.get(i).copied().unwrap_or(0.0) * 32768.0;
        }
        st.process_frame(&mut out_buf, &in_buf);
    }

    let mut out = Vec::with_capacity(mono48.len());
    for chunk in mono48.chunks(frame) {
        for (i, slot) in in_buf.iter_mut().enumerate() {
            *slot = chunk.get(i).copied().unwrap_or(0.0) * 32768.0;
        }
        st.process_frame(&mut out_buf, &in_buf);
        for &s in out_buf.iter().take(chunk.len()) {
            out.push(s / 32768.0);
        }
    }
    out
}

/// Loudness-normalize so quiet mics and whispered speech produce a strong,
/// consistent signal for Whisper. We push the RMS up to a whisper-friendly
/// target rather than just the peak: peak-only normalization barely lifts quiet
/// speech (a single transient pins the gain), which is why whispering used to be
/// missed or transcribed as gibberish. Pure silence is left alone, and the raw
/// RMS gate in `commands` runs *before* this, so we only ever boost real audio.
fn normalize_loudness(samples: &mut [f32], target_rms: f32, max_gain: f32) {
    let peak = samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
    if peak < 1e-4 {
        return; // effectively silence; leave it alone
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len().max(1) as f64).sqrt() as f32;
    if rms < 1e-6 {
        return;
    }
    // Bring RMS up to target; clamp handles the rare transient that clips. For
    // dictation, consistent loudness matters far more than sparing a click.
    let gain = (target_rms / rms).min(max_gain);
    if gain > 1.0 {
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

/// Full pipeline: raw interleaved capture -> 16 kHz mono, cleaned, trimmed to
/// speech, gain-normalized, and padded to at least one second so Whisper's mel
/// front end has enough to work with.
///
/// Order matters and is deliberate:
///   1. high-pass  — kill rumble/hum first so it never pollutes anything below
///   2. denoise    — RNNoise (when on) works best on the full-rate, HPF'd signal
///   3. resample   — down to Whisper's 16 kHz
///   4. VAD trim   — cut lead/tail silence so Whisper decodes only speech
///   5. normalize  — gain computed on the speech-only clip, so long silences can
///                   no longer skew the RMS and over/under-boost the voice
pub fn prepare_for_whisper(captured: &CapturedAudio, denoise: bool) -> Vec<f32> {
    let mut mono = to_mono(&captured.samples, captured.channels);
    high_pass(&mut mono, captured.sample_rate, 80.0);
    let mut out = if denoise {
        let m48 = resample(&mono, captured.sample_rate, 48_000);
        let clean = denoise_48k(&m48);
        resample(&clean, 48_000, TARGET_RATE)
    } else {
        resample_to_16k(&mono, captured.sample_rate)
    };
    // VAD: drop silence so Whisper decodes only speech — faster, and far less
    // likely to hallucinate words during quiet stretches. Conservative; returns
    // the clip unchanged if it can't confidently find speech.
    out = crate::vad::trim_to_speech(&out);
    normalize_loudness(&mut out, 0.12, 120.0);
    let min_len = TARGET_RATE as usize;
    if out.len() < min_len {
        out.resize(min_len, 0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(parts: &[(usize, f32)]) -> CapturedAudio {
        let mut samples = Vec::new();
        for &(n, amp) in parts {
            samples.extend((0..n).map(|i| amp * (i as f32 * 0.2).sin()));
        }
        CapturedAudio { samples, sample_rate: 16_000, channels: 1 }
    }

    #[test]
    fn silence_and_room_floors_are_not_speech() {
        assert!(!speech_present(&clip(&[(32_000, 0.0)])));
        assert!(!speech_present(&clip(&[(32_000, 1.4e-4)])), "noise-cancelling mic floor");
        assert!(!speech_present(&clip(&[(32_000, 8.5e-4)])), "steady fan noise");
    }

    #[test]
    fn whispered_dictation_is_speech() {
        // Quiet floor, then speech at raw RMS ~0.0005. The whole-clip RMS is
        // under the old 0.0010 gate, so this clip used to be thrown away.
        let c = clip(&[(8_000, 1.4e-4), (24_000, 7e-4), (8_000, 1.4e-4)]);
        assert!(rms_level(&c) < 0.0010);
        assert!(speech_present(&c));
    }

    #[test]
    fn quiet_speech_over_a_normal_mic_floor_is_speech() {
        // Mirrors the replay case that 3.5x dropped: syllables at RMS ~0.0008
        // with noise at RMS ~0.0003 between them.
        let mut seed: u32 = 7;
        let mut noise = |amp: f32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp
        };
        let mut samples: Vec<f32> = (0..16_000).map(|_| noise(5e-4)).collect();
        for _ in 0..12 {
            samples.extend((0..2_400).map(|i| 0.0011 * (i as f32 * 0.2).sin() + noise(5e-4)));
            samples.extend((0..1_600).map(|_| noise(5e-4)));
        }
        samples.extend((0..16_000).map(|_| noise(5e-4)));
        let c = CapturedAudio { samples, sample_rate: 16_000, channels: 1 };
        assert!(rms_level(&c) < 0.0010, "the old whole-clip gate would have dropped it");
        assert!(speech_present(&c));
    }

    #[test]
    fn normal_speech_is_speech() {
        assert!(speech_present(&clip(&[(8_000, 0.0), (16_000, 0.1), (8_000, 0.0)])));
    }

    fn tone(rate: u32, hz: f32, amp: f32) -> Vec<f32> {
        let step = 2.0 * std::f32::consts::PI * hz / rate as f32;
        (0..rate as usize).map(|i| amp * (step * i as f32).sin()).collect()
    }

    /// Level of the middle half of a resampled one-second tone of amplitude 0.5,
    /// in dB against the input, away from the filter's edge ramps.
    fn gain_db(out: &[f32]) -> f32 {
        let q = out.len() / 4;
        let mid = &out[q..out.len() - q];
        let rms = (mid.iter().map(|&s| s * s).sum::<f32>() / mid.len() as f32).sqrt();
        20.0 * (rms / (0.5 / 2f32.sqrt())).log10()
    }

    #[test]
    fn downsampling_removes_what_16k_cannot_hold() {
        // The laptop mic runs at 48 kHz. Sound above 8 kHz must vanish, not fold
        // back into the speech band as a false tone (20 kHz lands on 4 kHz).
        for (rate, hz) in [(48_000, 9_000.0), (48_000, 12_000.0), (48_000, 20_000.0), (44_100, 10_000.0)] {
            let out = resample(&tone(rate, hz, 0.5), rate, TARGET_RATE);
            assert_eq!(out.len(), 16_000);
            let db = gain_db(&out);
            assert!(db < -60.0, "{hz} Hz from {rate} Hz leaked through at {db:.1} dB");
        }
    }

    #[test]
    fn downsampling_keeps_the_speech_band_flat() {
        for (rate, hz) in [(48_000, 150.0), (48_000, 3_000.0), (48_000, 6_000.0), (44_100, 6_000.0)] {
            let db = gain_db(&resample(&tone(rate, hz, 0.5), rate, TARGET_RATE));
            assert!(db.abs() < 0.1, "{hz} Hz from {rate} Hz came out at {db:.2} dB");
        }
    }

    /// Cost of a minute of 48 kHz mic audio (ignored; run with
    /// `cargo test --release --lib resample_speed -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn resample_speed() {
        let minute: Vec<f32> = (0..60).flat_map(|_| tone(48_000, 440.0, 0.5)).collect();
        let t0 = std::time::Instant::now();
        let out = resample(&minute, 48_000, TARGET_RATE);
        eprintln!("60 s at 48 kHz -> {} samples in {:.1} ms", out.len(), t0.elapsed().as_secs_f64() * 1e3);
    }

    #[test]
    fn upsampling_keeps_the_tone() {
        let out = resample(&tone(16_000, 1_000.0, 0.5), 16_000, 48_000);
        assert_eq!(out.len(), 48_000);
        let db = gain_db(&out);
        assert!(db.abs() < 0.1, "1 kHz came out at {db:.2} dB");
    }
}
