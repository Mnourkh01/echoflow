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

/// Resample mono audio to 16 kHz with linear interpolation. Good enough for
/// speech; a polyphase resampler is a later refinement.
pub fn resample_to_16k(mono: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == TARGET_RATE || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = TARGET_RATE as f64 / from_rate as f64;
    let out_len = ((mono.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Peak-normalize so quiet mics produce a strong enough signal for Whisper.
/// Weak audio is the main cause of wrong language detection and gibberish.
/// Gain is capped so near-silent clips don't get blown up into noise.
fn normalize_peak(samples: &mut [f32], target: f32, max_gain: f32) {
    let peak = samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
    if peak < 1e-4 {
        return; // effectively silence; leave it alone
    }
    let gain = (target / peak).min(max_gain);
    if gain > 1.0 {
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

/// Full pipeline: raw interleaved capture -> 16 kHz mono, gain-normalized, and
/// padded to at least one second so Whisper's mel front end has enough to work with.
pub fn prepare_for_whisper(captured: &CapturedAudio) -> Vec<f32> {
    let mono = to_mono(&captured.samples, captured.channels);
    let mut out = resample_to_16k(&mono, captured.sample_rate);
    normalize_peak(&mut out, 0.95, 25.0);
    let min_len = TARGET_RATE as usize;
    if out.len() < min_len {
        out.resize(min_len, 0.0);
    }
    out
}
