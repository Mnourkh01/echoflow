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

/// Resample mono audio between two rates with linear interpolation. Good enough
/// for speech; a polyphase resampler is a later refinement.
pub fn resample(mono: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || mono.is_empty() {
        return mono.to_vec();
    }
    // Anti-alias when downsampling (e.g. 44.1/48 kHz mics -> 16 kHz): a short
    // moving-average low-pass before linear interpolation tames the aliasing that
    // otherwise smears consonants and hurts recognition accuracy. Upsampling
    // (the denoise path's 48 kHz hop) needs no pre-filter.
    let filtered;
    let src: &[f32] = if to_rate < from_rate {
        let width = ((from_rate as f32 / to_rate as f32).round() as usize).max(2);
        filtered = box_filter(mono, width);
        &filtered
    } else {
        mono
    };
    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((src.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 / ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let a = src.get(idx).copied().unwrap_or(0.0);
        let b = src.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Trailing moving average of width `width`. Cheap O(n) low-pass used as a crude
/// anti-alias filter ahead of downsampling. The small phase shift is irrelevant
/// for speech recognition.
fn box_filter(x: &[f32], width: usize) -> Vec<f32> {
    if width <= 1 {
        return x.to_vec();
    }
    let mut out = Vec::with_capacity(x.len());
    let mut sum = 0f32;
    for i in 0..x.len() {
        sum += x[i];
        if i >= width {
            sum -= x[i - width];
        }
        let count = (i + 1).min(width) as f32;
        out.push(sum / count);
    }
    out
}

/// Resample mono audio to 16 kHz (Whisper's required rate).
pub fn resample_to_16k(mono: &[f32], from_rate: u32) -> Vec<f32> {
    resample(mono, from_rate, TARGET_RATE)
}

/// RNNoise denoise. Expects 48 kHz mono; processes 480-sample (10 ms) frames.
/// RNNoise works in the i16 amplitude range, so scale up before and back after.
fn denoise_48k(mono48: &[f32]) -> Vec<f32> {
    use nnnoiseless::DenoiseState;
    let frame = DenoiseState::FRAME_SIZE;
    let mut st = DenoiseState::new();
    let mut in_buf = vec![0f32; frame];
    let mut out_buf = vec![0f32; frame];
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

/// Full pipeline: raw interleaved capture -> 16 kHz mono, gain-normalized, and
/// padded to at least one second so Whisper's mel front end has enough to work
/// with. When `denoise` is on, RNNoise cleans background noise first (at 48 kHz,
/// the rate it operates on) so speech stays clear through a noisy mic.
pub fn prepare_for_whisper(captured: &CapturedAudio, denoise: bool) -> Vec<f32> {
    let mono = to_mono(&captured.samples, captured.channels);
    let mut out = if denoise {
        let m48 = resample(&mono, captured.sample_rate, 48_000);
        let clean = denoise_48k(&m48);
        resample(&clean, 48_000, TARGET_RATE)
    } else {
        resample_to_16k(&mono, captured.sample_rate)
    };
    normalize_loudness(&mut out, 0.12, 120.0);
    // VAD: drop silence so Whisper decodes only speech — faster, and far less
    // likely to hallucinate words during quiet stretches. Conservative; returns
    // the clip unchanged if it can't confidently find speech.
    out = crate::vad::trim_to_speech(&out);
    let min_len = TARGET_RATE as usize;
    if out.len() < min_len {
        out.resize(min_len, 0.0);
    }
    out
}
