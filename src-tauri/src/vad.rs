//! Lightweight energy-based voice activity detection (VAD).
//!
//! Trims leading/trailing silence and collapses long internal gaps in 16 kHz
//! mono audio, so Whisper decodes only speech: faster, and far less likely to
//! hallucinate text during silence. Deliberately conservative — generous
//! hangover + padding so quiet or whispered speech is never clipped, and it
//! returns the input unchanged whenever it can't confidently find speech.
//!
//! Pure Rust, no model dependency (kept that way on purpose: native ML deps have
//! been painful to build on this toolchain). A Silero / webrtc VAD can later drop
//! in behind `trim_to_speech` if we want sharper boundaries.

const FRAME: usize = 480; // 30 ms @ 16 kHz
const HANGOVER: usize = 8; // ~240 ms of silence kept around speech (natural pauses)
// Internal silence longer than this collapses to this many frames. Generous
// (~1.5 s) on purpose: thinking pauses inside a dictation stay intact — every
// splice is a discontinuity Whisper can mis-decode across, so we only collapse
// gaps long enough to invite silence hallucinations, and the cut then lands
// deep inside near-silence where the seam is inaudible.
const MAX_GAP: usize = 50;
const PAD: usize = 5; // ~150 ms lead/tail padding so words aren't clipped

pub(crate) fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

/// Value at percentile `p` (0..1) of `v`. Used to estimate the noise floor.
pub(crate) fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (((s.len() - 1) as f32) * p).round() as usize;
    s[idx.min(s.len() - 1)]
}

/// Expand every `true` run in `mask` by `k` frames on each side (hangover).
fn dilate(mask: &[bool], k: usize) -> Vec<bool> {
    let n = mask.len();
    let mut out = vec![false; n];
    for i in 0..n {
        if mask[i] {
            let lo = i.saturating_sub(k);
            let hi = (i + k).min(n - 1);
            for slot in out.iter_mut().take(hi + 1).skip(lo) {
                *slot = true;
            }
        }
    }
    out
}

/// Trim to speech. Returns the trimmed audio, or the input unchanged when no
/// speech is confidently found (never returns empty — the caller still pads to
/// at least one second for Whisper's mel front end).
pub fn trim_to_speech(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    if n < FRAME * 3 {
        return samples.to_vec(); // too short to bother
    }

    let rms: Vec<f32> = samples.chunks(FRAME).map(frame_rms).collect();
    let peak = rms.iter().cloned().fold(0.0f32, f32::max);
    if peak < 1e-3 {
        return samples.to_vec(); // basically silent; leave it for the caller's gate
    }

    // Threshold sits above the noise floor but well below the speech level, so
    // quiet speech still counts as voiced. The three terms guard a noisy floor,
    // a loud clip, and a near-silent floor. The speech level is the 95th
    // percentile frame, not the max, so one loud syllable can never lift the
    // line over the soft words around it. The absolute floor is low (4e-4 RMS,
    // measured before normalization): at 3e-3 a soft talker on a
    // noise-cancelling mic kept 1.4 s of a 6.3 s clip, and "what model are you
    // using right now" came out "what hotel are you". Losing spoken words is a
    // far worse failure than keeping a little extra silence.
    let floor = percentile(&rms, 0.2);
    let loud = percentile(&rms, 0.95);
    let thresh = (floor * 2.5).max(loud * 0.08).max(4e-4);

    let voiced = dilate(&rms.iter().map(|&r| r >= thresh).collect::<Vec<_>>(), HANGOVER);
    let first = match voiced.iter().position(|&v| v) {
        Some(i) => i,
        None => return samples.to_vec(), // found nothing; don't gut the clip
    };
    let last = voiced.iter().rposition(|&v| v).unwrap();

    // Keep voiced frames (plus lead/tail pad); collapse long internal silence.
    let lo = first.saturating_sub(PAD);
    let hi = (last + PAD).min(rms.len() - 1);
    let mut keep = vec![false; rms.len()];
    let mut gap = 0usize;
    for (i, slot) in keep.iter_mut().enumerate().take(hi + 1).skip(lo) {
        if voiced[i] {
            *slot = true;
            gap = 0;
        } else {
            if gap < MAX_GAP {
                *slot = true;
            }
            gap += 1;
        }
    }

    let mut out = Vec::with_capacity(n);
    for (i, frame) in samples.chunks(FRAME).enumerate() {
        if keep.get(i).copied().unwrap_or(false) {
            out.extend_from_slice(frame);
        }
    }
    if out.is_empty() {
        samples.to_vec()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (i as f32 * 0.2).sin())
            .collect()
    }

    #[test]
    fn trims_surrounding_silence() {
        // 1 s silence + 0.5 s tone + 1 s silence.
        let mut audio = vec![0.0f32; 16_000];
        audio.extend(tone(8_000, 0.3));
        audio.extend(vec![0.0f32; 16_000]);
        let trimmed = trim_to_speech(&audio);
        // Removed most of the 2 s of silence...
        assert!(trimmed.len() < audio.len() - 16_000, "should drop most silence");
        // ...but kept the speech (plus hangover/pad), so not gutted.
        assert!(trimmed.len() >= 8_000, "must keep the spoken region");
    }

    #[test]
    fn leaves_pure_silence_alone() {
        let audio = vec![0.0f32; 32_000];
        assert_eq!(trim_to_speech(&audio).len(), audio.len());
    }

    #[test]
    fn keeps_quiet_speech_around_one_loud_syllable() {
        // A soft talker on a noise-cancelling mic: near-silent floor, speech at
        // RMS ~0.0014 and one normal-volume syllable. The quiet words around it
        // must survive; losing them is the "I have to speak up" bug.
        let mut audio = tone(8_000, 1e-4);
        audio.extend(tone(16_000, 0.002));
        audio.extend(tone(1_600, 0.05));
        audio.extend(tone(16_000, 0.002));
        audio.extend(tone(8_000, 1e-4));
        let trimmed = trim_to_speech(&audio);
        assert!(trimmed.len() >= 33_600, "kept {} of 33600 speech samples", trimmed.len());
    }

    #[test]
    fn keeps_all_speech_when_no_silence() {
        let audio = tone(32_000, 0.3);
        let trimmed = trim_to_speech(&audio);
        assert_eq!(trimmed.len(), audio.len(), "uniform speech shouldn't be trimmed");
    }
}
