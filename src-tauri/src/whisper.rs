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
    ) -> Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("failed to create whisper state: {e}"))?;

        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8) as i32;

        // Step 1: settle on the language up front. A forced mode wins; otherwise
        // run a cheap detect pass on the mel so we know the language (and its
        // probability) BEFORE decoding — this is what lets auto mode pick up the
        // Arabic dialect prime and report an honest confidence to the user.
        let (language, confidence): (String, f32) = match language_mode {
            "auto" | "" => detect_language(&mut state, audio_16k_mono, threads)
                .unwrap_or_else(|e| {
                    log::warn!("language detect failed, falling back to auto: {e}");
                    ("auto".to_string(), 0.0)
                }),
            // Any explicit code is honored as a forced language: en, ar, and the
            // European set (fr/de/es/it/pt/nl...) all skip detection.
            code => (code.to_string(), 1.0),
        };

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
        // hallucinating into) the next one — a common source of wrong words.
        params.set_no_context(true);
        // Temperature fallback: retry hotter when a decode is low-confidence,
        // which is common with strong accents.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.2);

        params.set_language(Some(language.as_str()));
        params.set_translate(translate);

        // Arabic dictation: prime the model with everyday colloquial Arabic so it
        // stops forcing Modern Standard Arabic onto dialect speech — the main
        // reason normal spoken Arabic transcribes badly. Applies whenever the
        // language resolves to Arabic, including auto-detect.
        if language == "ar" && !translate {
            params.set_initial_prompt(dialect_prompt(dialect));
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
        // full run; recover the language it actually used. Shadowing moves the
        // old String now that `params` (which borrowed it) is consumed.
        let language = if language == "auto" {
            let id = state.full_lang_id_from_state().unwrap_or(-1);
            whisper_rs::get_lang_str(id)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            language
        };

        Ok(Transcript {
            full_text: full,
            language,
            language_confidence: confidence,
            segments,
        })
    }
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
    fn transcribes_english_sample() {
        let model = dev_model();
        let wav = sample_wav();
        if !model.exists() || !wav.exists() {
            eprintln!("skipping: model or sample wav not present");
            return;
        }
        let engine = WhisperEngine::load(&model, "small").expect("load model");
        let audio = read_wav_16k_mono(&wav);
        let t = engine.transcribe(&audio, "auto", false, "auto").expect("transcribe");
        eprintln!("detected lang = {}, text = {}", t.language, t.full_text);
        assert!(!t.full_text.trim().is_empty(), "expected non-empty text");
        assert_eq!(t.language, "en", "jfk sample should detect as English");
    }
}
