//! Voice-activity detection: trims leading and trailing silence from 16 kHz mono.
//!
//! Frame energy relative to the clip's own noise floor, not an absolute amplitude.
//! An absolute threshold assumes it knows how loud the microphone is: at 0.01 it
//! discarded most of a real dictation on a mic whose speech sat at an RMS of 0.007,
//! leaving Whisper a fragment to hallucinate from. The noise floor is measured per
//! clip instead, so a quiet mic and a loud room both work.

/// 20 ms frames at 16 kHz — long enough for a stable energy estimate, short enough
/// to land trim boundaries between words.
const FRAME: usize = 320;
/// Speech must exceed the noise floor by this factor. Low enough for quiet speech,
/// high enough that room noise alone never qualifies.
const SPEECH_OVER_NOISE: f32 = 2.5;
/// Absolute floor, so a clip of pure digital silence is not "speech 2.5x above zero".
const MIN_SPEECH_RMS: f32 = 0.0015;
/// Consecutive speech frames needed to accept a boundary (60 ms) — ignores clicks.
const MIN_RUN: usize = 3;
/// Kept either side of the detected speech, so nothing is clipped mid-consonant.
const PAD_FRAMES: usize = 5; // 100 ms

fn frame_rms(audio: &[f32]) -> Vec<f32> {
    audio
        .chunks(FRAME)
        .map(|f| {
            let sum: f32 = f.iter().map(|s| s * s).sum();
            (sum / f.len().max(1) as f32).sqrt()
        })
        .collect()
}

/// The 10th-percentile frame energy: what this clip sounds like when nobody is
/// talking. Using a percentile rather than the minimum keeps one freak-quiet frame
/// from setting the floor at zero.
fn noise_floor(rms: &[f32]) -> f32 {
    let mut sorted = rms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (sorted.len() / 10).min(sorted.len().saturating_sub(1));
    sorted.get(idx).copied().unwrap_or(0.0)
}

/// Trim leading and trailing silence. Returns empty when the clip contains no speech.
///
/// The returned audio is always a contiguous span: nothing is ever removed from the
/// middle, so words cannot go missing between the first and last speech detected.
pub fn trim_silence(audio: &[f32]) -> Vec<f32> {
    if audio.is_empty() {
        return Vec::new();
    }
    let rms = frame_rms(audio);
    if rms.is_empty() {
        return Vec::new();
    }

    let threshold = (noise_floor(&rms) * SPEECH_OVER_NOISE).max(MIN_SPEECH_RMS);
    let is_speech: Vec<bool> = rms.iter().map(|&r| r > threshold).collect();

    let run_starts_at = |i: usize| -> bool {
        (i..(i + MIN_RUN).min(is_speech.len())).all(|j| is_speech[j])
            && i + MIN_RUN <= is_speech.len()
    };

    let Some(first) = (0..is_speech.len()).find(|&i| run_starts_at(i)) else {
        return Vec::new();
    };
    // Last frame that ends a qualifying run.
    let last = (0..is_speech.len())
        .rev()
        .find(|&i| i + 1 >= MIN_RUN && run_starts_at(i + 1 - MIN_RUN))
        .unwrap_or(first);

    let start_frame = first.saturating_sub(PAD_FRAMES);
    let end_frame = (last + 1 + PAD_FRAMES).min(rms.len());
    let start = start_frame * FRAME;
    let end = (end_frame * FRAME).min(audio.len());
    if end <= start {
        return Vec::new();
    }
    audio[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = 16000;

    /// `lead` samples of room noise, `speech` samples at `level`, then `trail` noise.
    fn clip(lead: usize, speech: usize, trail: usize, level: f32, noise: f32) -> Vec<f32> {
        let mut v = Vec::new();
        let mut push = |n: usize, amp: f32| {
            for i in 0..n {
                // Alternating sign so RMS ≈ amp regardless of frame alignment.
                v.push(if i % 2 == 0 { amp } else { -amp });
            }
        };
        push(lead, noise);
        push(speech, level);
        push(trail, noise);
        v
    }

    fn secs(v: &[f32]) -> f32 {
        v.len() as f32 / SR as f32
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(trim_silence(&[]).is_empty());
    }

    #[test]
    fn digital_silence_returns_empty() {
        assert!(trim_silence(&vec![0.0; SR]).is_empty());
    }

    #[test]
    fn quiet_speech_on_a_quiet_mic_is_kept() {
        // Dave's machine: speech at RMS ~0.010 over a noise floor of ~0.001. The old
        // absolute threshold of 0.01 discarded almost all of this.
        let audio = clip(SR / 2, 3 * SR, SR / 2, 0.010, 0.001);
        let trimmed = trim_silence(&audio);
        assert!(
            secs(&trimmed) > 2.9,
            "kept only {:.2}s of 3s of quiet speech",
            secs(&trimmed)
        );
    }

    #[test]
    fn loud_speech_in_a_noisy_room_is_kept() {
        let audio = clip(SR / 2, 2 * SR, SR / 2, 0.20, 0.03);
        assert!(secs(&trim_silence(&audio)) > 1.9);
    }

    #[test]
    fn leading_and_trailing_silence_is_removed() {
        // 2s of noise, 1s of speech, 2s of noise: keep ~1s plus padding, not 5s.
        let audio = clip(2 * SR, SR, 2 * SR, 0.05, 0.001);
        let kept = secs(&trim_silence(&audio));
        assert!(kept > 0.9, "clipped the speech: {kept:.2}s");
        assert!(kept < 1.5, "kept the silence too: {kept:.2}s");
    }

    #[test]
    fn a_short_utterance_survives() {
        // Half a second of speech must not be whittled down to a fragment; that is
        // what made Whisper hallucinate unrelated words.
        let audio = clip(SR / 4, SR / 2, SR / 4, 0.012, 0.001);
        assert!(secs(&trim_silence(&audio)) > 0.45);
    }

    #[test]
    fn a_single_click_is_not_speech() {
        let mut audio = vec![0.001; SR];
        for i in 0..80 {
            audio[SR / 2 + i] = 0.5; // 5 ms, under the 60 ms minimum run
        }
        assert!(trim_silence(&audio).is_empty());
    }

    #[test]
    fn the_result_is_always_contiguous() {
        // Speech, a pause, more speech: the pause stays, so no words go missing.
        let mut audio = clip(SR / 4, SR / 2, 0, 0.05, 0.001);
        audio.extend(clip(SR, SR / 2, SR / 4, 0.05, 0.001));
        let trimmed = trim_silence(&audio);
        assert!(secs(&trimmed) > 1.9, "dropped the middle: {:.2}s", secs(&trimmed));
    }
}
