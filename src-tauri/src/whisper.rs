//! Thin wrapper over whisper-rs: load a GGML model once, transcribe 16 kHz mono
//! audio, and report the auto-detected language.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Languages auto-detect is allowed to pick: exactly the set the app offers in
/// Settings. Whisper ranks all ~99 languages, and dialectal Arabic routinely
/// comes back as Farsi/Urdu (and accented English as Dutch/Afrikaans) when the
/// whole list competes. Restricting the argmax to the supported set fixes those
/// misdetections without touching forced-language mode.
pub(crate) const AUTO_LANGS: &[&str] = &["en", "ar", "fr", "de", "es", "it", "pt", "nl"];

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub full_text: String,
    pub language: String,
    pub language_confidence: f32,
    pub segments: Vec<Segment>,
    /// Mean log-probability of the text tokens. whisper.cpp does not say when
    /// its temperature fallback fired; a low value is the visible trace of it.
    pub avg_logprob: Option<f32>,
}

impl Transcript {
    /// Cut Whisper's runaway repeats (`text::collapse_loops`) to one copy in the
    /// text and in the segments, where a loop is often one segment decoded over
    /// and over. True when anything was cut.
    pub fn collapse_loops(&mut self) -> bool {
        let Some(text) = crate::text::collapse_loops(&self.full_text) else {
            return false;
        };
        self.full_text = text;
        let mut kept: Vec<Segment> = Vec::with_capacity(self.segments.len());
        let mut i = 0;
        while i < self.segments.len() {
            let key = crate::text::normalize_words(&self.segments[i].text);
            let mut j = i + 1;
            while j < self.segments.len() && crate::text::normalize_words(&self.segments[j].text) == key {
                j += 1;
            }
            if j - i >= 3 {
                let mut seg = self.segments[i].clone();
                seg.end_ms = self.segments[j - 1].end_ms;
                kept.push(seg);
            } else {
                kept.extend_from_slice(&self.segments[i..j]);
            }
            i = j;
        }
        for seg in &mut kept {
            if let Some(t) = crate::text::collapse_loops(&seg.text) {
                seg.text = t;
            }
        }
        self.segments = kept;
        true
    }
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    pub model_name: String,
}

/// State read by the C-side abort callback: the user's cancel flag plus the
/// runaway-watchdog deadline. Owned by the `decode_buffer` stack frame for the
/// duration of the decode; whisper worker threads only ever read it.
struct AbortCtl {
    cancel: Arc<AtomicBool>,
    deadline: std::time::Instant,
}

/// ggml abort callback: return true to abort the running graph. `user_data` is
/// the `*const AbortCtl` registered in `decode_buffer` (guaranteed alive until
/// `full()` returns). Atomics + `Instant` reads are thread-safe.
unsafe extern "C" fn abort_trampoline(user_data: *mut std::ffi::c_void) -> bool {
    let ctl = &*(user_data as *const AbortCtl);
    ctl.cancel.load(Ordering::Relaxed) || std::time::Instant::now() > ctl.deadline
}

impl WhisperEngine {
    pub fn load(model_path: &Path, model_name: &str) -> Result<Self> {
        let path = model_path
            .to_str()
            .ok_or_else(|| anyhow!("model path is not valid UTF-8"))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| anyhow!("failed to load model '{model_name}': {e}"))?;
        Ok(Self {
            ctx,
            model_name: model_name.to_string(),
        })
    }

    /// `language_mode` is "auto", "en", or "ar". "auto" detects the language
    /// before transcribing (so we can report real confidence and apply Arabic
    /// dialect priming automatically). `dialect` selects the Arabic prime.
    /// `translate` = true makes Whisper output English (translate task).
    /// `cancel` aborts the decode when set (the user's cancel button); the
    /// function then returns an error containing "canceled".
    /// `mixed` adds the mixed-language prime to English decodes.
    #[allow(clippy::too_many_arguments)]
    pub fn transcribe(
        &self,
        audio_16k_mono: &[f32],
        language_mode: &str,
        translate: bool,
        dialect: &str,
        vocab: &str,
        mixed: bool,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Transcript> {
        let threads = whisper_threads();

        // Settle on the language up front. A forced mode wins; otherwise run a
        // cheap detect pass on the mel so we know the language (and its
        // probability) BEFORE decoding — this is what lets auto mode pick up the
        // Arabic dialect prime and report an honest confidence. Doing it once
        // here also keeps every window of a long dictation on the SAME language,
        // instead of re-detecting (and possibly disagreeing) window to window.
        let (language, confidence): (String, f32) = match language_mode {
            "auto" | "" => {
                let mut state = self
                    .ctx
                    .create_state()
                    .map_err(|e| anyhow!("failed to create whisper state: {e}"))?;
                detect_language(&mut state, audio_16k_mono, threads).unwrap_or_else(|e| {
                    log::warn!("language detect failed, falling back to auto: {e}");
                    ("auto".to_string(), 0.0)
                })
            }
            // Any explicit code is honored as a forced language: en, ar, and the
            // European set (fr/de/es/it/pt/nl...) all skip detection.
            code => (code.to_string(), 1.0),
        };

        // Short clips (almost all dictation) take the simple single-pass path —
        // byte-for-byte the old behavior. Only genuinely long dictation is decoded
        // in silence-aligned windows, so one multi-minute decode is bounded into
        // chunks (each with its own runaway watchdog) and we have a per-window seam
        // to stream / cancel from later. Auto with a FAILED detect also stays
        // single-pass: per-window auto-detect is messy, and detect only fails on
        // near-silence anyway.
        let sr = crate::audio::TARGET_RATE as usize;
        let long = audio_16k_mono.len() > (LONG_AUDIO_SECS * sr as f32) as usize;
        if !long || language == "auto" {
            return self.decode_buffer(audio_16k_mono, &language, confidence, translate, dialect, vocab, mixed, threads, cancel);
        }

        let bounds = split_at_silence(audio_16k_mono, sr);
        log::info!("long dictation ({} s): decoding in {} windows",
            audio_16k_mono.len() / sr, bounds.len());
        let mut full = String::new();
        let mut segments = Vec::new();
        let mut logprobs: Vec<f32> = Vec::new();
        for (ci, &(lo, hi)) in bounds.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(anyhow!("canceled"));
            }
            let part =
                self.decode_buffer(&audio_16k_mono[lo..hi], &language, confidence, translate, dialect, vocab, mixed, threads, cancel)?;
            logprobs.extend(part.avg_logprob);
            let offset_ms = (lo as f32 / sr as f32 * 1000.0) as i64;
            for mut seg in part.segments {
                seg.start_ms += offset_ms;
                seg.end_ms += offset_ms;
                if !full.is_empty() {
                    full.push(' ');
                }
                full.push_str(&seg.text);
                segments.push(seg);
            }
            log::info!("  window {}/{} decoded", ci + 1, bounds.len());
        }

        Ok(Transcript {
            full_text: full,
            language,
            language_confidence: confidence,
            segments,
            avg_logprob: (!logprobs.is_empty()).then(|| logprobs.iter().sum::<f32>() / logprobs.len() as f32),
        })
    }

    /// Decode a single 16 kHz mono buffer on a fresh state. `language` is a
    /// concrete code, or "auto" to let Whisper detect during the decode (the code
    /// it used is recovered after). This is the unit the long-form windower calls
    /// once per window and the short path calls once for the whole clip.
    #[allow(clippy::too_many_arguments)]
    fn decode_buffer(
        &self,
        audio_16k_mono: &[f32],
        language: &str,
        confidence: f32,
        translate: bool,
        dialect: &str,
        vocab: &str,
        mixed: bool,
        threads: i32,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("failed to create whisper state: {e}"))?;

        // Beam search beats greedy noticeably on accented / dialectal speech;
        // worth the extra CPU for transcription quality.
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        params.set_n_threads(threads);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Dictation is short and self-contained: don't carry decoded text forward
        // as context between segments. Prevents the prior phrase from biasing (and
        // hallucinating into) the next one — a common source of wrong words. Also
        // why splitting a long clip into windows is safe: no cross-window context
        // is lost because we never used any.
        params.set_no_context(true);
        // Temperature fallback: retry hotter when a decode is low-confidence,
        // which is common with strong accents.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.2);
        // Suppress non-speech token strings (♪, bracketed sound-event tags...).
        // Dictation never wants them, and on weak audio they are exactly the
        // tokens hallucinated decodes reach for.
        params.set_suppress_nst(true);

        params.set_language(Some(language));
        params.set_translate(translate);

        // Watchdog: abort a decode that runs absurdly long so a degenerate /
        // looping decode can't hold the engine lock forever and freeze the app
        // (the "unresponsive after heavy use" failure). The budget is 20x this
        // buffer's real-time length — far above this CPU build's ~3x typical, so it
        // only ever trips on a true runaway, never a legitimate long dictation.
        // The same callback also serves the user's cancel button: setting the
        // shared flag makes whisper.cpp bail out at the next graph-node check.
        // On trip, `full()` returns an error the caller surfaces and recovers from.
        //
        // NOTE: this deliberately uses the raw callback API, NOT whisper-rs's
        // `set_abort_callback_safe`. That wrapper (0.14.4) double-boxes the
        // closure but its trampoline casts `user_data` back to the concrete
        // closure type instead of `Box<dyn FnMut() -> bool>` — a type confusion
        // that reads garbage and returns arbitrary abort decisions (it broke the
        // decode outright once the closure captured an Arc). `abort_ctl` is
        // owned by this stack frame and outlives `full()` below.
        let budget = {
            let audio_secs = audio_16k_mono.len() as f32 / crate::audio::TARGET_RATE as f32;
            std::time::Duration::from_secs_f32((audio_secs * 20.0).max(30.0))
        };
        let abort_ctl = Box::new(AbortCtl {
            cancel: cancel.clone(),
            deadline: std::time::Instant::now() + budget,
        });
        unsafe {
            params.set_abort_callback(Some(abort_trampoline));
            params.set_abort_callback_user_data(&*abort_ctl as *const AbortCtl as *mut _);
        }

        // Initial prompt biases the decoder. Two independent sources, combined:
        //   - Arabic dictation gets an everyday-colloquial prime so the model stops
        //     forcing Modern Standard Arabic onto dialect speech (the main reason
        //     spoken Arabic transcribes badly). Applies whenever lang resolves to ar.
        //   - The user's custom vocabulary (names / jargon / brands) so those exact
        //     spellings are recognized instead of misheard. Applies to any language.
        // `prompt` must outlive `params` (which borrows it) until `full()` runs, so
        // it's bound here in the same scope.
        let prompt = build_initial_prompt(language, translate, dialect, vocab, mixed);
        if let Some(ref p) = prompt {
            params.set_initial_prompt(p);
        }

        state.full(params, audio_16k_mono).map_err(|e| {
            if cancel.load(Ordering::Relaxed) {
                anyhow!("canceled")
            } else {
                anyhow!("transcription failed: {e}")
            }
        })?;

        let n = state
            .full_n_segments()
            .map_err(|e| anyhow!("could not read segments: {e}"))?;

        let mut segments = Vec::new();
        let mut full = String::new();
        // Text tokens only: ids from end-of-text up are timestamps and markers.
        let eot = self.ctx.token_eot();
        let (mut logprob_sum, mut logprob_n) = (0.0f64, 0u32);
        for i in 0..n {
            for j in 0..state.full_n_tokens(i).unwrap_or(0) {
                if let Ok(tok) = state.full_get_token_data(i, j) {
                    if tok.id < eot {
                        logprob_sum += tok.plog as f64;
                        logprob_n += 1;
                    }
                }
            }
            let text = state.full_get_segment_text_lossy(i).unwrap_or_default();
            let t0 = state.full_get_segment_t0(i).unwrap_or(0) * 10; // cs -> ms
            let t1 = state.full_get_segment_t1(i).unwrap_or(0) * 10;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !full.is_empty() {
                full.push(' ');
            }
            full.push_str(trimmed);
            segments.push(Segment {
                start_ms: t0,
                end_ms: t1,
                text: trimmed.to_string(),
            });
        }

        // If the detect pass failed we asked Whisper to auto-detect during the
        // full run; recover the language it actually used.
        let language = if language == "auto" {
            let id = state.full_lang_id_from_state().unwrap_or(-1);
            whisper_rs::get_lang_str(id)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            language.to_string()
        };

        Ok(Transcript {
            full_text: full,
            language,
            language_confidence: confidence,
            segments,
            avg_logprob: (logprob_n > 0).then(|| (logprob_sum / logprob_n as f64) as f32),
        })
    }
}

/// Above this many seconds a dictation is decoded in silence-aligned windows
/// instead of a single call. Set HIGH on purpose: whisper.cpp already windows
/// internally at 30 s with a shared state, so single-pass is both correct AND
/// faster (our chunking re-creates state per window, ~1.6x slower). Chunking's
/// real payoff is the per-window seam for streaming / cancel, which isn't wired
/// yet — so we only engage it for genuinely extreme clips (>10 min), where
/// keeping the decode bounded and interruptible beats raw speed. Lower this when
/// streaming partials are built. Below it, the simple fast path runs unchanged.
const LONG_AUDIO_SECS: f32 = 600.0;
const CHUNK_TARGET_SECS: f32 = 28.0; // aim per window (≈ whisper's own 30 s window)
const CHUNK_MAX_SECS: f32 = 40.0; // hard cap before forcing a cut
const SPLIT_SEARCH_SECS: f32 = 6.0; // hunt this far back from the cap for a quiet spot

/// Partition long audio into contiguous windows of ~22-40 s, cutting at the
/// quietest 30 ms frame near each window's end so a split lands in a pause rather
/// than mid-word. Windows tile the whole buffer with no gaps or overlap.
fn split_at_silence(samples: &[f32], sr: usize) -> Vec<(usize, usize)> {
    let n = samples.len();
    let target = (CHUNK_TARGET_SECS * sr as f32) as usize;
    let maxlen = (CHUNK_MAX_SECS * sr as f32) as usize;
    let search = (SPLIT_SEARCH_SECS * sr as f32) as usize;
    const FRAME: usize = 480; // 30 ms @ 16 kHz

    let mut bounds = Vec::new();
    let mut start = 0usize;
    while start < n {
        if n - start <= maxlen {
            bounds.push((start, n));
            break;
        }
        // Search [start+target-search, start+max] for the lowest-energy frame.
        let lo = start + target.saturating_sub(search);
        let hi = (start + maxlen).min(n);
        let mut cut = lo;
        let mut quietest = f32::MAX;
        let mut i = lo;
        while i + FRAME <= hi {
            let r = crate::vad::frame_rms(&samples[i..i + FRAME]);
            if r < quietest {
                quietest = r;
                cut = i;
            }
            i += FRAME;
        }
        // Always advance past `start` (guards a degenerate empty search window).
        if cut <= start {
            cut = hi;
        }
        bounds.push((start, cut));
        start = cut;
    }
    if bounds.is_empty() {
        bounds.push((0, n));
    }
    bounds
}

/// Thread count: measured on the 12700H (6P+8E hybrid), ggml gets SLOWER past
/// ~8 threads (E-cores + spin sync stall the P-cores: 8 threads 15s, 16 threads
/// 34s on the same clip). min(8) is the sweet spot for hybrid CPUs;
/// ECHOFLOW_THREADS overrides for benching / odd machines.
fn whisper_threads() -> i32 {
    std::env::var("ECHOFLOW_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8)
        })
        .clamp(1, 32) as i32
}

impl WhisperEngine {
    /// Language ID alone, no decode: the likeliest of `AUTO_LANGS` and its
    /// probability, from the first 30 s (what auto mode would pick).
    pub fn detect(&self, audio_16k_mono: &[f32]) -> Result<(String, f32)> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("failed to create whisper state: {e}"))?;
        detect_language(&mut state, audio_16k_mono, whisper_threads())
    }
}

/// Detect the spoken language from the audio's mel spectrogram, returning the
/// ISO code and its probability (0..1). Cheap relative to a full decode.
/// The argmax runs over `AUTO_LANGS` only (see the const above); the raw global
/// winner is used just as a fallback if none of the supported codes got a prob.
fn detect_language(
    state: &mut whisper_rs::WhisperState,
    audio_16k_mono: &[f32],
    threads: i32,
) -> Result<(String, f32)> {
    state
        .pcm_to_mel(audio_16k_mono, threads as usize)
        .map_err(|e| anyhow!("pcm_to_mel failed: {e}"))?;
    let (id, probs) = state
        .lang_detect(0, threads as usize)
        .map_err(|e| anyhow!("lang_detect failed: {e}"))?;

    let mut best: Option<(&'static str, f32)> = None;
    for (i, &p) in probs.iter().enumerate() {
        let Some(code) = whisper_rs::get_lang_str(i as i32) else {
            continue;
        };
        if !AUTO_LANGS.contains(&code) {
            continue;
        }
        if best.map_or(true, |(_, bp)| p > bp) {
            best = Some((code, p));
        }
    }
    if let Some((code, confidence)) = best {
        let global = whisper_rs::get_lang_str(id).unwrap_or("?");
        if global != code {
            log::info!(
                "language detect: constrained to '{code}' ({confidence:.2}); global winner was '{global}'"
            );
        }
        return Ok((code.to_string(), confidence));
    }
    // Fallback: probs vector didn't cover the supported set (shouldn't happen).
    let code = whisper_rs::get_lang_str(id)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("unknown language id {id}"))?;
    let confidence = probs.get(id as usize).copied().unwrap_or(0.0);
    Ok((code, confidence))
}

/// Colloquial-Arabic priming text per dialect. A dialect-specific prompt beats a
/// generic one at stopping the MSA bias; "auto" uses a mixed-dialect blob.
fn dialect_prompt(dialect: &str) -> &'static str {
    match dialect {
        "egyptian" => "محادثة بالعامية المصرية. عامل ايه؟ ازيك يا باشا؟ ايه الأخبار؟ عايز اقولك حاجة.",
        "levantine" => "محادثة بالعامية الشامية. كيفك؟ شو أخبارك؟ شو عم تعمل؟ بدي احكيلك شي.",
        "gulf" => "محادثة بالعامية الخليجية. شلونك؟ وش تسوي؟ شخبارك؟ ابي اقولك شي.",
        "iraqi" => "محادثة بالعامية العراقية. شلونك؟ شكو ماكو؟ شدتسوي؟ اريد اكلك شي.",
        "maghrebi" => "محادثة بالدارجة المغاربية. كيداير؟ واش خبارك؟ شنو كتدير؟ بغيت نقولك حاجة.",
        _ => "محادثة يومية بالعربية العامية. شلونك؟ إزيك يا باشا؟ شو أخبارك؟ كيفك؟ وش تسوي؟ عامل ايه؟",
    }
}

/// Build Whisper's `initial_prompt` from the two things that bias decoding: the
/// Arabic colloquial prime (when relevant) and the user's custom vocabulary.
/// Returns None when neither applies (so we don't set an empty prompt).
///
/// The vocabulary is presented as bare terms, as if they were part of earlier
/// speech — Whisper treats the initial prompt as preceding transcript, so a
/// labelled meta-line like "Vocabulary: ..." reads as foreign text and biases
/// far less than the terms appearing plainly. The cap cuts at a term boundary
/// so a truncated list never ends mid-name.
/// A short code-switched line. In the prompt of a forced-English decode it makes
/// Whisper write spoken Arabic in Arabic script instead of dropping it or
/// guessing English words, so the AI step can translate it. Measured on mixed
/// clips and on the user's own English takes (no Arabic appeared in those).
pub(crate) const MIXED_PRIME: &str = "So يعني I think we should, خلينا نشوف, maybe change the design شوي.";

pub(crate) fn build_initial_prompt(
    language: &str,
    translate: bool,
    dialect: &str,
    vocab: &str,
    mixed: bool,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if language == "ar" && !translate {
        parts.push(dialect_prompt(dialect).to_string());
    }
    // Before the vocabulary: whisper.cpp keeps the END of a long prompt.
    if mixed && language == "en" && !translate {
        parts.push(MIXED_PRIME.to_string());
    }
    let terms = crate::text::vocab_terms(vocab);
    if !terms.is_empty() {
        let mut list = String::new();
        for t in terms {
            let sep = if list.is_empty() { 0 } else { 2 };
            if list.chars().count() + sep + t.chars().count() > 400 {
                break;
            }
            if !list.is_empty() {
                list.push_str(", ");
            }
            list.push_str(t);
        }
        list.push('.');
        parts.push(list);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dev_model() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/ggml-small.bin")
    }

    fn sample_wav() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/jfk.wav")
    }

    fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
        let mut reader = hound::WavReader::open(path).expect("open wav");
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i16>()
                .map(|s| s.unwrap() as f32 / 32768.0)
                .collect(),
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        };
        let mono = crate::audio::to_mono(&samples, spec.channels);
        crate::audio::resample_to_16k(&mono, spec.sample_rate)
    }

    /// Replay tools for real dictations (ignored; run by hand). The model path
    /// comes from ECHOFLOW_MODEL, the clips from ECHOFLOW_REPLAY_FILES
    /// (`;`-separated WAV paths). Example:
    /// `cargo test --lib replay_language_probs -- --ignored --nocapture`
    fn replay_setup() -> (WhisperEngine, Vec<PathBuf>) {
        let model = PathBuf::from(std::env::var("ECHOFLOW_MODEL").expect("ECHOFLOW_MODEL"));
        let files = std::env::var("ECHOFLOW_REPLAY_FILES").expect("ECHOFLOW_REPLAY_FILES");
        let engine = WhisperEngine::load(&model, "replay").expect("load model");
        let files = files.split(';').filter(|s| !s.trim().is_empty()).map(PathBuf::from).collect();
        (engine, files)
    }

    fn word_overlap(reference: &str, hyp: &str) -> f32 {
        let words = |s: &str| -> Vec<String> {
            s.split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(|w| w.to_lowercase())
                .collect()
        };
        let r = words(reference);
        let h = words(hyp);
        if r.is_empty() {
            return 0.0;
        }
        let hit = r.iter().filter(|w| h.contains(w)).count();
        hit as f32 / r.len() as f32
    }

    /// Whisper's own English vs Arabic probability for each clip, the number an
    /// auto or "smart" language choice would be based on.
    #[test]
    #[ignore]
    fn replay_language_probs() {
        let (engine, files) = replay_setup();
        let en = whisper_rs::get_lang_id("en").unwrap() as usize;
        let ar = whisper_rs::get_lang_id("ar").unwrap() as usize;
        for f in files {
            let audio = read_wav_16k_mono(&f);
            let mut state = engine.ctx.create_state().unwrap();
            state.pcm_to_mel(&audio, 8).unwrap();
            let (id, probs) = state.lang_detect(0, 8).unwrap();
            eprintln!(
                "{}  en={:.3}  ar={:.3}  winner={}",
                f.file_name().unwrap().to_string_lossy(),
                probs[en],
                probs[ar],
                whisper_rs::get_lang_str(id).unwrap_or("?")
            );
        }
    }

    /// Decode each clip in ECHOFLOW_REPLAY_LANG (default "en"), with the custom
    /// vocabulary from ECHOFLOW_REPLAY_VOCAB (default none) and the mixed prime
    /// when ECHOFLOW_REPLAY_MIXED=1.
    #[test]
    #[ignore]
    fn replay_decode() {
        let (engine, files) = replay_setup();
        let lang = std::env::var("ECHOFLOW_REPLAY_LANG").unwrap_or_else(|_| "en".into());
        let vocab = std::env::var("ECHOFLOW_REPLAY_VOCAB").unwrap_or_default();
        let mixed = std::env::var("ECHOFLOW_REPLAY_MIXED").is_ok_and(|v| v == "1");
        let cancel = Arc::new(AtomicBool::new(false));
        for f in files {
            let audio = read_wav_16k_mono(&f);
            let t = engine.transcribe(&audio, &lang, false, "auto", &vocab, mixed, &cancel).unwrap();
            eprintln!(
                "{} [{lang}{}] logprob {:?} loop {} | {}",
                f.file_name().unwrap().to_string_lossy(),
                if mixed { " +mixed" } else { "" },
                t.avg_logprob,
                crate::text::collapse_loops(&t.full_text).is_some(),
                t.full_text
            );
        }
    }

    /// Accuracy and speed bench over real dictations: `replay_decode` with the
    /// wall time of each decode, one tab-separated line per clip on stdout so
    /// runs on different models or prompts can be diffed line by line.
    #[test]
    #[ignore]
    fn replay_bench() {
        let (engine, files) = replay_setup();
        let lang = std::env::var("ECHOFLOW_REPLAY_LANG").unwrap_or_else(|_| "en".into());
        let vocab = std::env::var("ECHOFLOW_REPLAY_VOCAB").unwrap_or_default();
        let cancel = Arc::new(AtomicBool::new(false));
        // The first decode pays the GPU pipeline setup; keep it out of the numbers.
        engine.transcribe(&read_wav_16k_mono(&files[0]), &lang, false, "auto", &vocab, false, &cancel).unwrap();
        let (mut decode, mut audio_secs) = (0.0f64, 0.0f64);
        for f in &files {
            let audio = read_wav_16k_mono(f);
            let t0 = std::time::Instant::now();
            let t = engine.transcribe(&audio, &lang, false, "auto", &vocab, false, &cancel).unwrap();
            let secs = t0.elapsed().as_secs_f64();
            decode += secs;
            audio_secs += audio.len() as f64 / 16_000.0;
            println!(
                "{}\t{:.2}\t{:.3}\t{}",
                f.file_name().unwrap().to_string_lossy(),
                secs,
                t.avg_logprob.unwrap_or(0.0),
                t.full_text
            );
        }
        eprintln!("decoded {audio_secs:.0} s of audio in {decode:.1} s");
    }

    /// Quiet-speech sweep: each clip is turned down, padded with a second of
    /// mic-floor noise on both sides, and run through the real capture pipeline
    /// (`prepare_for_whisper`, with and without RNNoise) before decoding. The
    /// saved clips are already VAD-trimmed and normalized, so they stand in for
    /// clean speech; turning them down is quieter speech, not true whispering.
    #[test]
    #[ignore]
    fn replay_quiet_speech() {
        let (engine, files) = replay_setup();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut seed: u32 = 12345;
        let mut noise = |amp: f32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp
        };
        for f in files {
            let clean = read_wav_16k_mono(&f);
            let reference = engine.transcribe(&clean, "en", false, "auto", "", false, &cancel).unwrap().full_text;
            eprintln!("\n{}  reference: {reference}", f.file_name().unwrap().to_string_lossy());
            for gain in [0.03f32, 0.01, 0.004] {
                let mut samples: Vec<f32> = (0..16_000).map(|_| noise(0.0005)).collect();
                samples.extend(clean.iter().map(|s| s * gain + noise(0.0005)));
                samples.extend((0..16_000).map(|_| noise(0.0005)));
                let raw_rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                let raw_peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
                for denoise in [false, true] {
                    let captured = crate::audio::CapturedAudio {
                        samples: samples.clone(),
                        sample_rate: 16_000,
                        channels: 1,
                    };
                    if !crate::audio::speech_present(&captured) {
                        eprintln!("  gain {gain:<5} denoise {denoise:<5} raw_rms {raw_rms:.4} raw_peak {raw_peak:.4} DISCARDED as no speech");
                        continue;
                    }
                    let prepared = crate::audio::prepare_for_whisper(&captured, denoise);
                    let text = engine.transcribe(&prepared, "en", false, "auto", "", false, &cancel).unwrap().full_text;
                    eprintln!(
                        "  gain {gain:<5} denoise {denoise:<5} raw_rms {raw_rms:.4} raw_peak {raw_peak:.4} kept {:.1}s/{:.1}s overlap {:.2} | {text}",
                        prepared.len() as f32 / 16_000.0,
                        samples.len() as f32 / 16_000.0,
                        word_overlap(&reference, &text)
                    );
                }
            }
        }
    }

    #[test]
    fn initial_prompt_combines_dialect_and_vocab() {
        // English with vocab → only the bare terms (no meta label).
        let p = super::build_initial_prompt("en", false, "auto", "EchoFlow, Mnour, Tauri", false).unwrap();
        assert!(p.contains("EchoFlow") && p.contains("Tauri"));
        assert!(!p.contains("Vocabulary"), "no meta label in the prompt");
        assert!(!p.contains("محادثة"), "no Arabic prime for English");

        // Arabic with vocab → both the colloquial prime and the terms.
        let p = super::build_initial_prompt("ar", false, "egyptian", "القاهرة", false).unwrap();
        assert!(p.contains("المصرية"), "Arabic prime present");
        assert!(p.contains("القاهرة"));

        // Nothing to bias → None (so we never set an empty prompt).
        assert!(super::build_initial_prompt("en", false, "auto", "   ", false).is_none());
        // Translate task skips the Arabic prime even for ar.
        assert!(super::build_initial_prompt("ar", true, "auto", "", false).is_none());
    }

    #[test]
    fn mixed_prime_only_joins_english_decodes_and_sits_before_the_vocab() {
        let vocab = "foloprint, Opti4Club";
        let p = super::build_initial_prompt("en", false, "auto", vocab, true).unwrap();
        let prime_at = p.find(super::MIXED_PRIME).expect("prime present");
        let vocab_at = p.find("foloprint").expect("vocab present");
        assert!(prime_at < vocab_at, "{p}");
        assert!(super::build_initial_prompt("en", false, "auto", "", true).unwrap() == super::MIXED_PRIME);

        // Off, or any other decode, is today's prompt byte for byte.
        for (lang, translate) in [("en", false), ("ar", false), ("ar", true), ("fr", false)] {
            let on = super::build_initial_prompt(lang, translate, "auto", vocab, lang != "en");
            let off = super::build_initial_prompt(lang, translate, "auto", vocab, false);
            assert_eq!(on, off, "{lang}/{translate}");
        }
    }

    #[test]
    fn initial_prompt_caps_at_term_boundary() {
        // A long list truncates between terms, never mid-name.
        let vocab: Vec<String> = (0..100).map(|i| format!("TermNumber{i:02}Xyz")).collect();
        let p = super::build_initial_prompt("en", false, "auto", &vocab.join(", "), false).unwrap();
        assert!(p.chars().count() <= 401, "prompt stays capped");
        let body = p.trim_end_matches('.');
        assert!(
            body.split(", ").all(|t| t.starts_with("TermNumber") && t.ends_with("Xyz")),
            "every kept term is intact: {body}"
        );
    }

    #[test]
    fn a_looped_decode_keeps_one_copy_in_text_and_segments() {
        let seg = |s: i64, text: &str| Segment { start_ms: s, end_ms: s + 1000, text: text.into() };
        let mut t = Transcript {
            full_text: "We can prepare it. I think it should be prepared. I think it should be prepared. \
I think it should be prepared. I think it should be prepared."
                .into(),
            language: "en".into(),
            language_confidence: 1.0,
            segments: vec![
                seg(0, "We can prepare it."),
                seg(1000, "I think it should be prepared."),
                seg(2000, "I think it should be prepared."),
                seg(3000, "I think it should be prepared."),
                seg(4000, "I think it should be prepared."),
            ],
            avg_logprob: None,
        };
        assert!(t.collapse_loops());
        assert_eq!(t.full_text, "We can prepare it. I think it should be prepared.");
        assert_eq!(t.segments.len(), 2);
        assert_eq!((t.segments[1].start_ms, t.segments[1].end_ms), (1000, 5000));
        let mut clean = t.clone();
        assert!(!clean.collapse_loops(), "nothing left to cut");
    }

    #[test]
    fn split_tiles_long_audio_contiguously() {
        let sr = 16_000usize;
        let audio = vec![0.05f32; sr * 150]; // 150 s, uniform low energy
        let bounds = super::split_at_silence(&audio, sr);
        assert!(bounds.len() >= 4, "150 s should make several windows");
        assert_eq!(bounds.first().unwrap().0, 0, "must start at 0");
        assert_eq!(bounds.last().unwrap().1, audio.len(), "must cover to the end");
        for w in bounds.windows(2) {
            assert_eq!(w[0].1, w[1].0, "windows must tile with no gap/overlap");
        }
        let maxlen = (super::CHUNK_MAX_SECS * sr as f32) as usize;
        for &(lo, hi) in &bounds {
            assert!(hi > lo, "no empty window");
            assert!(hi - lo <= maxlen, "window stays under the hard cap");
        }
    }

    #[test]
    fn split_cuts_inside_the_silence() {
        let sr = 16_000usize;
        // 30 s loud, 2 s silence, 30 s loud (62 s > cap → must split once, and the
        // cut should land in the silent gap at [30 s, 32 s], not mid-speech).
        let mut audio = vec![0.4f32; sr * 30];
        audio.extend(vec![0.0f32; sr * 2]);
        audio.extend(vec![0.4f32; sr * 30]);
        let bounds = super::split_at_silence(&audio, sr);
        let first_cut = bounds[0].1;
        assert!(
            first_cut >= sr * 30 && first_cut <= sr * 32,
            "expected a cut inside the silence, got sample {first_cut}"
        );
    }

    #[test]
    fn cancel_aborts_decode() {
        let model = dev_model();
        let wav = sample_wav();
        if !model.exists() || !wav.exists() {
            eprintln!("skipping: model or sample wav not present");
            return;
        }
        let engine = WhisperEngine::load(&model, "small").expect("load model");
        let audio = read_wav_16k_mono(&wav);
        // Cancel already requested: the decode must abort, not produce text.
        let cancel = Arc::new(AtomicBool::new(true));
        let err = engine
            .transcribe(&audio, "en", false, "auto", "", false, &cancel)
            .expect_err("pre-canceled decode must fail");
        assert!(err.to_string().contains("canceled"), "got: {err}");
    }

    #[test]
    fn transcribes_english_sample() {
        let model = dev_model();
        let wav = sample_wav();
        if !model.exists() || !wav.exists() {
            eprintln!("skipping: model or sample wav not present");
            return;
        }
        let engine = WhisperEngine::load(&model, "small").expect("load model");
        let audio = read_wav_16k_mono(&wav);
        let cancel = Arc::new(AtomicBool::new(false));
        let t = engine.transcribe(&audio, "auto", false, "auto", "", false, &cancel).expect("transcribe");
        eprintln!("detected lang = {}, text = {}", t.language, t.full_text);
        assert!(!t.full_text.trim().is_empty(), "expected non-empty text");
        assert_eq!(t.language, "en", "jfk sample should detect as English");
    }
}
