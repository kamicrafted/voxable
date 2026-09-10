//! Usage stats derived from history entries (pure — no Tauri, no I/O).
//!
//! Everything the Hub's home screen shows is computed here so it can be unit
//! tested, and so the same numbers are available to any other caller.

use serde::{Deserialize, Serialize};

use crate::history::HistoryEntry;

/// Words per minute a person types. Used as the baseline that dictation is
/// compared against for "time saved".
///
/// 40 wpm is the common figure for an average touch typist on prose. It is an
/// assumption, not a measurement of this user, so the UI labels the number as an
/// estimate.
pub const TYPING_WPM: f64 = 40.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Stats {
    /// Number of dictations recorded.
    pub dictations: usize,
    /// Total words across every cleaned transcript.
    pub words: usize,
    /// Total time spent speaking, in seconds.
    pub speaking_seconds: f64,
    /// Speaking rate across all dictations, in words per minute.
    pub words_per_minute: f64,
    /// Estimated minutes saved versus typing the same words at [`TYPING_WPM`].
    pub minutes_saved: f64,
    /// Dictations in the last 7 days.
    pub dictations_this_week: usize,
    /// Words in the last 7 days.
    pub words_this_week: usize,
    /// The app dictated into most often, with its share of dictations.
    pub top_app: Option<(String, usize)>,
}

/// Count words the way a person would: runs of non-whitespace.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|w| !w.is_empty()).count()
}

/// Compute stats from every entry.
///
/// `now_rfc3339` is passed in rather than read from the clock so the "this week"
/// window is testable.
pub fn compute(entries: &[HistoryEntry], now_rfc3339: &str) -> Stats {
    let mut stats = Stats::default();
    if entries.is_empty() {
        return stats;
    }

    let week_ago = parse_rfc3339_secs(now_rfc3339).map(|now| now - 7.0 * 86_400.0);
    let mut app_counts: Vec<(String, usize)> = Vec::new();

    for entry in entries {
        let words = word_count(&entry.cleaned);
        stats.dictations += 1;
        stats.words += words;
        stats.speaking_seconds += entry.duration_ms as f64 / 1000.0;

        if let (Some(week_ago), Some(at)) = (week_ago, parse_rfc3339_secs(&entry.timestamp)) {
            if at >= week_ago {
                stats.dictations_this_week += 1;
                stats.words_this_week += words;
            }
        }

        if let Some(app) = entry.app.as_ref().filter(|a| !a.is_empty()) {
            match app_counts.iter_mut().find(|(name, _)| name == app) {
                Some((_, count)) => *count += 1,
                None => app_counts.push((app.clone(), 1)),
            }
        }
    }

    let speaking_minutes = stats.speaking_seconds / 60.0;
    if speaking_minutes > 0.0 {
        stats.words_per_minute = stats.words as f64 / speaking_minutes;
    }

    // Saved = how long typing those words would have taken, minus the time actually
    // spent speaking them. Never negative: dictating slower than you type saves
    // nothing, it does not cost you time you already had.
    let typing_minutes = stats.words as f64 / TYPING_WPM;
    stats.minutes_saved = (typing_minutes - speaking_minutes).max(0.0);

    app_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    stats.top_app = app_counts.into_iter().next();

    stats
}

/// Seconds since the Unix epoch for an RFC 3339 UTC timestamp.
///
/// Hand-rolled rather than pulling `chrono` into this crate, which is deliberately
/// dependency-light. Returns `None` for anything it cannot parse, so a corrupt row
/// drops out of the weekly window instead of breaking every stat.
fn parse_rfc3339_secs(ts: &str) -> Option<f64> {
    let bytes = ts.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> { ts.get(range)?.parse().ok() };

    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let hour = num(11..13)?;
    let minute = num(14..16)?;
    let second = num(17..19)?;

    // Days from the civil epoch (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ts: &str, cleaned: &str, duration_ms: u64, app: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            id: ts.to_string(),
            timestamp: ts.to_string(),
            app: app.map(str::to_string),
            raw: cleaned.to_string(),
            cleaned: cleaned.to_string(),
            model: "base".into(),
            cleanup_level: "medium".into(),
            duration_ms,
        }
    }

    #[test]
    fn empty_history_is_all_zeroes() {
        let stats = compute(&[], "2026-09-09T12:00:00Z");
        assert_eq!(stats, Stats::default());
    }

    #[test]
    fn counts_words_and_dictations() {
        let entries = [
            entry("2026-09-09T12:00:00Z", "one two three", 6_000, None),
            entry("2026-09-09T12:01:00Z", "four five", 6_000, None),
        ];
        let stats = compute(&entries, "2026-09-09T12:02:00Z");
        assert_eq!(stats.dictations, 2);
        assert_eq!(stats.words, 5);
        assert_eq!(stats.speaking_seconds, 12.0);
    }

    #[test]
    fn words_per_minute_uses_speaking_time() {
        // 30 words in 30 seconds is 60 wpm.
        let text = vec!["word"; 30].join(" ");
        let stats = compute(&[entry("2026-09-09T12:00:00Z", &text, 30_000, None)], "2026-09-09T12:00:30Z");
        assert!((stats.words_per_minute - 60.0).abs() < 0.001);
    }

    #[test]
    fn time_saved_compares_against_typing() {
        // 40 words spoken in 30s: typing takes 1 minute, speaking took 0.5.
        let text = vec!["word"; 40].join(" ");
        let stats = compute(&[entry("2026-09-09T12:00:00Z", &text, 30_000, None)], "2026-09-09T12:00:30Z");
        assert!((stats.minutes_saved - 0.5).abs() < 0.001);
    }

    #[test]
    fn time_saved_never_goes_negative() {
        // Two words in a two-minute recording is far slower than typing.
        let stats = compute(&[entry("2026-09-09T12:00:00Z", "hi there", 120_000, None)], "2026-09-09T12:02:00Z");
        assert_eq!(stats.minutes_saved, 0.0);
    }

    #[test]
    fn weekly_window_excludes_older_entries() {
        let entries = [
            entry("2026-09-01T12:00:00Z", "old words here", 3_000, None),
            entry("2026-09-08T12:00:00Z", "new words", 3_000, None),
        ];
        let stats = compute(&entries, "2026-09-09T12:00:00Z");
        assert_eq!(stats.dictations, 2);
        assert_eq!(stats.dictations_this_week, 1);
        assert_eq!(stats.words_this_week, 2);
    }

    #[test]
    fn top_app_is_the_most_frequent() {
        let entries = [
            entry("2026-09-09T12:00:00Z", "a", 1_000, Some("Slack")),
            entry("2026-09-09T12:01:00Z", "b", 1_000, Some("Safari")),
            entry("2026-09-09T12:02:00Z", "c", 1_000, Some("Slack")),
        ];
        let stats = compute(&entries, "2026-09-09T12:03:00Z");
        assert_eq!(stats.top_app, Some(("Slack".into(), 2)));
    }

    #[test]
    fn entries_without_an_app_do_not_produce_a_top_app() {
        let stats = compute(&[entry("2026-09-09T12:00:00Z", "a", 1_000, None)], "2026-09-09T12:01:00Z");
        assert_eq!(stats.top_app, None);
    }

    #[test]
    fn unparseable_timestamps_drop_out_of_the_weekly_window_only() {
        let entries = [entry("not-a-timestamp", "one two", 1_000, None)];
        let stats = compute(&entries, "2026-09-09T12:00:00Z");
        assert_eq!(stats.words, 2);
        assert_eq!(stats.words_this_week, 0);
    }

    #[test]
    fn rfc3339_parses_against_known_epoch_seconds() {
        assert_eq!(parse_rfc3339_secs("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_rfc3339_secs("2026-09-09T12:00:00Z"), Some(1_788_955_200.0));
    }
}
