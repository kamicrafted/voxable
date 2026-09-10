//! What a dictation hotkey press and release mean.
//!
//! One key serves both modes. A quick tap toggles hands-free dictation, the way it
//! always has; holding the key is push-to-talk and ends when you let go. The press
//! behaves identically either way — only the release differs — so nothing has to
//! guess at press time which mode the user meant.

/// How long the key must be held for the release to end dictation. Below this a
/// press is a tap, and dictation keeps running until the next press.
pub const HOLD_THRESHOLD_MS: u64 = 300;

/// What the caller should do when the hotkey is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAction {
    /// End dictation — the key was held, so this was push-to-talk.
    Stop,
    /// Do nothing: either it was a tap, or the press had already ended dictation.
    Nothing,
}

/// Decide what a release means.
///
/// `press_started_recording` distinguishes the two things a press can do. The press
/// is still a toggle, so pressing the key while hands-free dictation is running
/// *stops* it — and the release that follows must not stop it a second time or
/// restart it, no matter how long the key was held.
pub fn release_action(press_started_recording: bool, held_ms: u64) -> ReleaseAction {
    if !press_started_recording {
        return ReleaseAction::Nothing;
    }
    if held_ms >= HOLD_THRESHOLD_MS {
        ReleaseAction::Stop
    } else {
        ReleaseAction::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tap_leaves_dictation_running() {
        // Hands-free: press starts, quick release changes nothing, next press stops.
        assert_eq!(release_action(true, 40), ReleaseAction::Nothing);
        assert_eq!(release_action(true, 299), ReleaseAction::Nothing);
    }

    #[test]
    fn a_hold_stops_on_release() {
        assert_eq!(release_action(true, 300), ReleaseAction::Stop);
        assert_eq!(release_action(true, 2_500), ReleaseAction::Stop);
    }

    #[test]
    fn the_threshold_itself_counts_as_a_hold() {
        assert_eq!(release_action(true, HOLD_THRESHOLD_MS), ReleaseAction::Stop);
        assert_eq!(
            release_action(true, HOLD_THRESHOLD_MS - 1),
            ReleaseAction::Nothing
        );
    }

    #[test]
    fn releasing_after_a_press_that_stopped_dictation_does_nothing() {
        // Tapping to end a hands-free dictation. The release must not restart it,
        // and must not stop it again, however long the key was down.
        assert_eq!(release_action(false, 10), ReleaseAction::Nothing);
        assert_eq!(release_action(false, 5_000), ReleaseAction::Nothing);
    }

    #[test]
    fn a_zero_length_press_is_a_tap() {
        // Some taps report 0ms. Treating that as a hold would start and immediately
        // stop dictation, leaving an empty recording.
        assert_eq!(release_action(true, 0), ReleaseAction::Nothing);
    }
}
