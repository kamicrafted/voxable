//! Dictation history: JSON index + raw `.f32` audio files.
//!
//! Storage layout under a single directory:
//! - `history.json` — the entry index (atomic writes: tmp + rename)
//! - `audio/<id>.f32` — raw little-endian f32 samples for each entry
//!
//! `add_entry` enforces a 500-entry cap (oldest entries' audio is deleted).
//! `cleanup_old` prunes entries older than `max_days` and keeps at most
//! `max_entries` newest. Timestamps are RFC 3339 strings (UTC).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A single dictation history entry (the JSON index row).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    /// RFC 3339 UTC timestamp, e.g. `2026-09-09T12:34:56.789Z`.
    pub timestamp: String,
    /// Foreground app name at dictation time (Windows only; `None` elsewhere).
    pub app: Option<String>,
    /// Raw Whisper transcription.
    pub raw: String,
    /// Final cleaned text.
    pub cleaned: String,
    /// Whisper model used.
    pub model: String,
    /// Cleanup level used (`none` | `light` | `medium` | `high`).
    pub cleanup_level: String,
    /// Duration of the recorded audio in milliseconds.
    pub duration_ms: u64,
}

/// Maximum number of entries kept in the index.
const MAX_ENTRIES: usize = 500;

const INDEX_FILE: &str = "history.json";
const AUDIO_SUBDIR: &str = "audio";

/// History storage rooted at `dir`.
pub struct History {
    storage: PathBuf,
}

impl History {
    pub fn new(dir: &Path) -> Self {
        Self {
            storage: dir.to_path_buf(),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.storage.join(INDEX_FILE)
    }

    fn audio_dir(&self) -> PathBuf {
        self.storage.join(AUDIO_SUBDIR)
    }

    fn audio_path(&self, id: &str) -> PathBuf {
        self.audio_dir().join(format!("{}.f32", id))
    }

    /// Append an entry to the index and write its audio.
    ///
    /// The index write is atomic (tmp file + rename). The 500-entry cap is
    /// enforced by dropping the oldest entries (and deleting their audio).
    pub fn add_entry(&self, entry: &HistoryEntry, audio: &[f32]) -> Result<(), String> {
        fs::create_dir_all(&self.storage)
            .map_err(|e| format!("create history dir: {}", e))?;
        fs::create_dir_all(self.audio_dir())
            .map_err(|e| format!("create audio dir: {}", e))?;

        // Write audio first (raw little-endian f32).
        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for sample in audio {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(self.audio_path(&entry.id), &bytes)
            .map_err(|e| format!("write audio: {}", e))?;

        // Load existing entries, append, enforce cap.
        let mut entries = self.load_entries()?;
        entries.push(entry.clone());
        if entries.len() > MAX_ENTRIES {
            let overflow = entries.len() - MAX_ENTRIES;
            for old in entries.iter().take(overflow) {
                let _ = fs::remove_file(self.audio_path(&old.id));
            }
            entries.drain(0..overflow);
        }

        // Atomic index write.
        let json = serde_json::to_string_pretty(&entries)
            .map_err(|e| format!("serialize history: {}", e))?;
        let tmp = self.index_path().with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("write history tmp: {}", e))?;
        fs::rename(&tmp, self.index_path()).map_err(|e| format!("rename history: {}", e))?;

        Ok(())
    }

    /// Load all entries, oldest first. Missing index → empty vec.
    pub fn load_entries(&self) -> Result<Vec<HistoryEntry>, String> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path).map_err(|e| format!("read history: {}", e))?;
        let entries: Vec<HistoryEntry> =
            serde_json::from_str(&content).map_err(|e| format!("parse history: {}", e))?;
        Ok(entries)
    }

    /// Read the stored audio for an entry. Missing file → empty vec (not an
    /// error — audio may have been pruned).
    pub fn get_audio(&self, id: &str) -> Result<Vec<f32>, String> {
        let path = self.audio_path(id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&path).map_err(|e| format!("read audio: {}", e))?;
        if bytes.len() % 4 != 0 {
            return Err(format!("audio file {} is not a multiple of 4 bytes", path.display()));
        }
        let mut samples = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(chunk);
            samples.push(f32::from_le_bytes(buf));
        }
        Ok(samples)
    }

    /// Delete entries older than `max_days` and keep at most `max_entries`
    /// newest. Audio files for pruned entries are removed.
    pub fn cleanup_old(&self, max_days: u32, max_entries: usize) -> Result<(), String> {
        let entries = self.load_entries()?;
        if entries.is_empty() {
            return Ok(());
        }

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(max_days as u64 * 86400))
            .unwrap_or(UNIX_EPOCH);

        let mut kept: Vec<HistoryEntry> = Vec::new();
        for entry in entries {
            let ts = parse_timestamp(&entry.timestamp);
            if ts >= cutoff {
                kept.push(entry);
            } else {
                // Age-pruned entry: remove its audio file too (not just the index row).
                let _ = fs::remove_file(self.audio_path(&entry.id));
            }
        }

        if kept.len() > max_entries {
            let overflow = kept.len() - max_entries;
            for old in kept.iter().take(overflow) {
                let _ = fs::remove_file(self.audio_path(&old.id));
            }
            kept.drain(0..overflow);
        }

        // Persist the pruned index (atomic).
        let json = serde_json::to_string_pretty(&kept)
            .map_err(|e| format!("serialize history: {}", e))?;
        let tmp = self.index_path().with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("write history tmp: {}", e))?;
        fs::rename(&tmp, self.index_path()).map_err(|e| format!("rename history: {}", e))?;

        Ok(())
    }
}

/// Parse an RFC 3339 timestamp into a `SystemTime`. Unparseable → UNIX epoch
/// (so the entry is treated as old and pruned).
fn parse_timestamp(ts: &str) -> SystemTime {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| {
            let secs = dt.timestamp();
            let nanos = dt.timestamp_subsec_nanos();
            if secs >= 0 {
                UNIX_EPOCH + Duration::new(secs as u64, nanos)
            } else {
                UNIX_EPOCH - Duration::new((-secs) as u64, nanos)
            }
        })
        .unwrap_or(UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn tmp_dir(tag: &str) -> PathBuf {
        // Unique per test so parallel tests don't collide.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("voxable-history-test-{}-{}-{}", std::process::id(), nanos, tag));
        p
    }

    fn entry(id: &str, timestamp: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            timestamp: timestamp.into(),
            app: Some("Code".into()),
            raw: "raw text".into(),
            cleaned: "Cleaned text.".into(),
            model: "base".into(),
            cleanup_level: "medium".into(),
            duration_ms: 1234,
        }
    }

    #[test]
    fn add_load_round_trip() {
        let dir = tmp_dir("roundtrip");
        let h = History::new(&dir);
        let e = entry("id-1", "2026-09-09T10:00:00Z");
        h.add_entry(&e, &[1.0, 2.0, 3.0]).unwrap();

        let loaded = h.load_entries().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], e);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_round_trip_real_samples() {
        let dir = tmp_dir("audio");
        let h = History::new(&dir);
        // 1000 non-zero samples, deterministic.
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001 + 0.5).collect();
        let e = entry("id-a", "2026-09-09T10:00:00Z");
        h.add_entry(&e, &samples).unwrap();

        let back = h.get_audio("id-a").unwrap();
        assert_eq!(back.len(), 1000);
        assert_eq!(back[0], samples[0]);
        assert_eq!(back[999], samples[999]);
        assert_eq!(back, samples);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn five_hundred_entry_cap() {
        let dir = tmp_dir("cap");
        let h = History::new(&dir);
        // 501 entries with increasing timestamps (oldest first).
        for i in 0..501u32 {
            let e = entry(
                &format!("id-{i}"),
                &format!("2026-09-09T{:02}:00:00Z", i % 24),
            );
            h.add_entry(&e, &[0.5]).unwrap();
        }
        let loaded = h.load_entries().unwrap();
        assert_eq!(loaded.len(), 500);
        // The oldest entry (id-0) was dropped; the newest (id-500) remains.
        assert_eq!(loaded[0].id, "id-1");
        assert_eq!(loaded[499].id, "id-500");
        // Dropped entry's audio is gone; kept entries' audio exists.
        assert!(h.get_audio("id-0").unwrap().is_empty());
        assert_eq!(h.get_audio("id-500").unwrap(), vec![0.5]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_old_by_age_and_count() {
        let dir = tmp_dir("cleanup");
        let h = History::new(&dir);
        // 3 entries: one 30 days old, two recent.
        h.add_entry(&entry("old", "2026-08-01T00:00:00Z"), &[0.1])
            .unwrap();
        h.add_entry(&entry("recent-1", "2026-09-08T00:00:00Z"), &[0.2])
            .unwrap();
        h.add_entry(&entry("recent-2", "2026-09-09T00:00:00Z"), &[0.3])
            .unwrap();

        h.cleanup_old(14, 2).unwrap();
        let loaded = h.load_entries().unwrap();
        // The 30-day-old entry is pruned by age; then the count cap keeps 2.
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "recent-1");
        assert_eq!(loaded[1].id, "recent-2");
        assert!(h.get_audio("old").unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_audio_missing_id_is_empty() {
        let dir = tmp_dir("missing");
        let h = History::new(&dir);
        let audio = h.get_audio("does-not-exist").unwrap();
        assert!(audio.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
