//! On-demand load / soak tests for the transcription pipeline. They reproduce
//! the two reliability problems we hunt for:
//!
//!   * a per-clip memory leak that only shows after ~2 hours of heavy use, and
//!   * a single very long clip (20-min dictation) blowing up RAM / latency.
//!
//! Ignored by default: they need `models/ggml-small.bin` + `models/jfk.wav` and
//! run for minutes. Each prints a CSV-ish line per step so the trend is easy to
//! eyeball or paste into a sheet. Run from `src-tauri/`:
//!
//!   # repeated short dictations — watch rss_mb for a per-clip leak (crash #1)
//!   LOADTEST_ITERS=200 cargo test --release soak_repeated_dictation -- --ignored --nocapture
//!
//!   # one long clip — watch peak rss + decode time (20-min OOM, crash #3)
//!   LOADTEST_MINUTES=20 cargo test --release soak_long_clip -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use crate::audio;
use crate::mem::rss_mb;
use crate::whisper::WhisperEngine;

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/ggml-small.bin")
}

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/jfk.wav")
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Read jfk.wav into 16 kHz mono f32, the same way the engine unit test does.
fn jfk_16k() -> Vec<f32> {
    let mut reader = hound::WavReader::open(sample_path()).expect("open jfk.wav");
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
    };
    let mono = audio::to_mono(&samples, spec.channels);
    audio::resample_to_16k(&mono, spec.sample_rate)
}

fn assets_present() -> bool {
    if !model_path().exists() || !sample_path().exists() {
        eprintln!("skipping load test: models/ggml-small.bin or models/jfk.wav not present");
        return false;
    }
    true
}

/// Model evaluation: transcribe jfk.wav with an arbitrary model and print the
/// wall time + output, so candidate models can be compared on this machine
/// before shipping one as the default. Run from `src-tauri/`:
///
///   BENCH_MODEL_PATH=path/to/ggml-x.bin cargo test --release bench_model -- --ignored --nocapture
#[test]
#[ignore]
fn bench_model() {
    let Ok(path) = std::env::var("BENCH_MODEL_PATH") else {
        eprintln!("skipping: set BENCH_MODEL_PATH to a ggml model file");
        return;
    };
    if !sample_path().exists() {
        eprintln!("skipping: models/jfk.wav not present");
        return;
    }
    let audio = jfk_16k();
    let secs = audio.len() as f32 / audio::TARGET_RATE as f32;
    let t = Instant::now();
    let engine = WhisperEngine::load(PathBuf::from(&path).as_path(), "bench").expect("load model");
    let load_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let tr = engine
        .transcribe(&audio, "en", false, "auto", "", false, &Arc::new(AtomicBool::new(false)))
        .expect("transcribe");
    let dec = t.elapsed().as_secs_f64();
    eprintln!(
        "# model={path}\n# load={load_s:.1}s decode={dec:.1}s clip={secs:.1}s ratio={:.2}x rss={:.0} MB\n# text: {}",
        dec / secs as f64,
        rss_mb(),
        tr.full_text
    );
}

/// Crash candidate #1: a slow per-clip leak. Reuses one engine (production keeps
/// the model cached in AppState) and calls `transcribe` in a loop, which creates
/// a fresh whisper state each time — exactly the production path. A rising
/// `delta_mb` that does NOT plateau is the leak.
#[test]
#[ignore]
fn soak_repeated_dictation() {
    if !assets_present() {
        return;
    }
    let iters = env_usize("LOADTEST_ITERS", 50);
    let engine = WhisperEngine::load(&model_path(), "small").expect("load model");
    let audio = jfk_16k();
    let base = rss_mb();
    eprintln!("# iter,elapsed_ms,rss_mb,delta_mb");
    for i in 1..=iters {
        let t = Instant::now();
        let _ = engine
            .transcribe(&audio, "en", false, "auto", "", false, &Arc::new(AtomicBool::new(false)))
            .expect("transcribe");
        let rss = rss_mb();
        eprintln!("{i},{},{:.1},{:.1}", t.elapsed().as_millis(), rss, rss - base);
    }
    let end = rss_mb();
    eprintln!(
        "# RSS grew {:.1} MB over {} iters ({:.3} MB/iter)",
        end - base,
        iters,
        (end - base) / iters as f64
    );
}

/// Crash candidate #3: one very long clip. Stitches jfk.wav up to N minutes and
/// runs a single decode, reporting peak RSS and wall time. This is the path a
/// 20-minute continuous dictation takes.
#[test]
#[ignore]
fn soak_long_clip() {
    if !assets_present() {
        return;
    }
    let minutes = env_usize("LOADTEST_MINUTES", 3);
    let one = jfk_16k();
    let target = minutes * 60 * audio::TARGET_RATE as usize;
    let mut long = Vec::with_capacity(target);
    while long.len() < target {
        long.extend_from_slice(&one);
    }
    long.truncate(target);
    eprintln!(
        "# built {} min ({} samples), rss before = {:.1} MB",
        minutes,
        long.len(),
        rss_mb()
    );
    let engine = WhisperEngine::load(&model_path(), "small").expect("load model");
    let t = Instant::now();
    let tr = engine
        .transcribe(&long, "en", false, "auto", "", false, &Arc::new(AtomicBool::new(false)))
        .expect("transcribe long clip");
    eprintln!(
        "# decoded {} min in {:.1}s, rss after = {:.1} MB, chars = {}",
        minutes,
        t.elapsed().as_secs_f64(),
        rss_mb(),
        tr.full_text.chars().count()
    );
}
