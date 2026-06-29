//! Thin wrapper over whisper-rs: load a GGML model once, transcribe 16 kHz mono
//! audio, and report the auto-detected language.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    pub model_name: String,
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
    pub fn transcribe(
        &self,
        audio_16k_mono: &[f32],
        language_mode: &str,
        translate: bool,
        dialect: &str,
        vocab: &str,
    ) -> Result<Transcript> {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8) as i32;

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
            return self.decode_buffer(audio_16k_mono, &language, confidence, translate, dialect, vocab, threads);
        }

        let bounds = split_at_silence(audio_16k_mono, sr);
        log::info!("long dictation ({} s): decoding in {} windows",
            audio_16k_mono.len() / sr, bounds.len());
        let mut full = String::new();
        let mut segments = Vec::new();
        for (ci, &(lo, hi)) in bounds.iter().enumerate() {
            let part =
                self.decode_buffer(&audio_16k_mono[lo..hi], &language, confidence, translate, dialect, vocab, threads)?;
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
        })
    }

    /// Decode a single 16 kHz mono buffer on a fresh state. `language` is a
    /// concrete code, or "auto" to let Whisper detect during the decode (the code
    /// it used is recovered after). This is the unit the long-form windower calls
    /// once per window and the short path calls once for the whole clip.
    fn decode_buffer(
        &self,
        audio_16k_mono: &[f32],
        language: &str,
        confidence: f32,
        translate: bool,
        dialect: &str,
        vocab: &str,
        threads: i32,
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

        params.set_language(Some(language));
        params.set_translate(translate);

        // Watchdog: abort a decode that runs absurdly long so a degenerate /
        // looping decode can't hold the engine lock forever and freeze the app
        // (the "unresponsive after heavy use" failure). The budget is 20x this
        // buffer's real-time length — far above this CPU build's ~3x typical, so it
        // only ever trips on a true runaway, never a legitimate long dictation.
        // On trip, `full()` returns an error the caller surfaces and recovers from.
        let budget = {
            let audio_secs = audio_16k_mono.len() as f32 / crate::audio::TARGET_RATE as f32;
            std::time::Duration::from_secs_f32((audio_secs * 20.0).max(30.0))
        };
        let decode_start = std::time::Instant::now();
        params.set_abort_callback_safe(move || decode_start.elapsed() > budget);

        // Initial prompt biases the decoder. Two independent sources, combined:
        //   - Arabic dictation gets an everyday-colloquial prime so the model stops
        //     forcing Modern Standard Arabic onto dialect speech (the main reason
        //     spoken Arabic transcribes badly). Applies whenever lang resolves to ar.
        //   - The user's custom vocabulary (names / jargon / brands) so those exact
        //     spellings are recognized instead of misheard. Applies to any language.
        // `prompt` must outlive `params` (which borrows it) until `full()` runs, so
        // it's bound here in the same scope.
        let prompt = build_initial_prompt(language, translate, dialect, vocab);
        if let Some(ref p) = prompt {
            params.set_initial_prompt(p);
        }

        state
            .full(params, audio_16k_mono)
            .map_err(|e| anyhow!("transcription failed: {e}"))?;

        let n = state
            .full_n_segments()
            .map_err(|e| anyhow!("could not read segments: {e}"))?;

        let mut segments = Vec::new();
        let mut full = String::new();
        for i in 0..n {
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
        })
    }
}

/// Above this many seconds a dictation is decoded in silence-aligned windows
/// instead of a single call, so a very long decode is bounded (and later
/// streamable / cancellable). Below it, the simple single-pass path runs and the
/// output is identical to before. whisper.cpp already windows internally at 30 s,
/// so this only governs *our* window granularity, never correctness.
const LONG_AUDIO_SECS: f32 = 120.0;
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

/// Detect the spoken language from the audio's mel spectrogram, returning the
/// ISO code and its probability (0..1). Cheap relative to a full decode.
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
fn build_initial_prompt(language: &str, translate: bool, dialect: &str, vocab: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if language == "ar" && !translate {
        parts.push(dialect_prompt(dialect).to_string());
    }
    // Custom vocabulary: split on commas / newlines / semicolons, trim, dedupe-ish,
    // and present as a short phrase the decoder leans toward. Cap the length so a
    // huge list can't crowd out the audio (Whisper's prompt window is limited).
    let terms: Vec<&str> = vocab
        .split(|c| c == ',' || c == '\n' || c == ';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !terms.is_empty() {
        let mut list = terms.join(", ");
        if list.chars().count() > 320 {
            list = list.chars().take(320).collect();
        }
        parts.push(format!("Vocabulary: {list}."));
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

    #[test]
    fn initial_prompt_combines_dialect_and_vocab() {
        // English with vocab → only the vocabulary line.
        let p = super::build_initial_prompt("en", false, "auto", "EchoFlow, Mnour, Tauri").unwrap();
        assert!(p.contains("Vocabulary:"));
        assert!(p.contains("EchoFlow") && p.contains("Tauri"));
        assert!(!p.contains("محادثة"), "no Arabic prime for English");

        // Arabic with vocab → both the colloquial prime and the vocabulary.
        let p = super::build_initial_prompt("ar", false, "egyptian", "القاهرة").unwrap();
        assert!(p.contains("المصرية"), "Arabic prime present");
        assert!(p.contains("Vocabulary:") && p.contains("القاهرة"));

        // Nothing to bias → None (so we never set an empty prompt).
        assert!(super::build_initial_prompt("en", false, "auto", "   ").is_none());
        // Translate task skips the Arabic prime even for ar.
        assert!(super::build_initial_prompt("ar", true, "auto", "").is_none());
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
    fn transcribes_english_sample() {
        let model = dev_model();
        let wav = sample_wav();
        if !model.exists() || !wav.exists() {
            eprintln!("skipping: model or sample wav not present");
            return;
        }
        let engine = WhisperEngine::load(&model, "small").expect("load model");
        let audio = read_wav_16k_mono(&wav);
        let t = engine.transcribe(&audio, "auto", false, "auto", "").expect("transcribe");
        eprintln!("detected lang = {}, text = {}", t.language, t.full_text);
        assert!(!t.full_text.trim().is_empty(), "expected non-empty text");
        assert_eq!(t.language, "en", "jfk sample should detect as English");
    }
}
