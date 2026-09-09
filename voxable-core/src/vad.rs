//! Voice-activity detection (energy-based silence trim). Pure (`&[f32]` → `Vec<f32>`).
//!
//! `trim_silence` trims leading/trailing silence from 16 kHz mono audio. It is
//! the same algorithm that lives in `src-tauri/src/whisper.rs` (moved here so
//! it is unit-testable with rustup alone; `src-tauri` re-exports it).

/// Trim leading and trailing silence from audio.
///
/// `threshold` is the sample amplitude below which a sample is considered
/// silence. `min_speech_ms` is the minimum duration of non-silence to keep
/// (avoids trimming short pauses between words). Returns an empty vec when no
/// speech is found.
pub fn trim_silence(audio: &[f32], threshold: f32, min_speech_ms: u32) -> Vec<f32> {
    if audio.is_empty() {
        return Vec::new();
    }

    let sample_rate = 16000;
    let min_speech_samples = (min_speech_ms as usize) * sample_rate / 1000;

    // Find first non-silent frame
    let mut start = 0;
    let mut found_speech = false;
    for i in 0..audio.len() {
        if audio[i].abs() > threshold {
            // Check if we have enough consecutive non-silent samples
            let end = (i + min_speech_samples).min(audio.len());
            let mut count = 0;
            for j in i..end {
                if audio[j].abs() > threshold {
                    count += 1;
                }
            }
            if count >= min_speech_samples / 2 {
                start = i;
                found_speech = true;
                break;
            }
        }
    }

    if !found_speech {
        return Vec::new();
    }

    // Find last non-silent frame
    let mut end = audio.len();
    for i in (0..audio.len()).rev() {
        if audio[i].abs() > threshold {
            let begin = i.saturating_sub(min_speech_samples);
            let mut count = 0;
            for j in begin..=i {
                if audio[j].abs() > threshold {
                    count += 1;
                }
            }
            if count >= min_speech_samples / 2 {
                end = i + 1;
                break;
            }
        }
    }

    if end <= start {
        return Vec::new();
    }

    audio[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = 16000;

    /// Silence for `lead` samples, speech for `speech` samples, silence for `trail`.
    fn audio_with_silence(lead: usize, speech: usize, trail: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; lead];
        for i in 0..speech {
            v.push(0.5 * ((i % 7) as f32 - 3.0) / 3.0); // non-zero wobble
        }
        v.extend(std::iter::repeat(0.0).take(trail));
        v
    }

    #[test]
    fn all_silence_returns_empty() {
        let audio = vec![0.0; SR];
        assert!(trim_silence(&audio, 0.01, 30).is_empty());
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(trim_silence(&[], 0.01, 30).is_empty());
    }

    #[test]
    fn speech_in_middle_trims_edges() {
        // 0.5s silence, 0.5s speech, 0.5s silence.
        let audio = audio_with_silence(SR / 2, SR / 2, SR / 2);
        let trimmed = trim_silence(&audio, 0.01, 30);
        // The trimmed span should be close to the speech region (± a few frames).
        assert!(trimmed.len() >= SR / 2 - 100);
        assert!(trimmed.len() <= SR / 2 + 100);
        // No leading/trailing silence remains.
        assert!(trimmed.first().unwrap().abs() > 0.01);
        assert!(trimmed.last().unwrap().abs() > 0.01);
    }

    #[test]
    fn short_burst_below_min_speech_is_dropped() {
        // A 10 ms blip (< 30 ms min speech) should be treated as silence.
        let mut audio = vec![0.0; SR / 2];
        for _ in 0..SR / 100 {
            audio.push(0.5);
        }
        audio.extend(std::iter::repeat(0.0).take(SR / 2));
        assert!(trim_silence(&audio, 0.01, 30).is_empty());
    }
}
