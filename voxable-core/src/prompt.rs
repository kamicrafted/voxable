//! LLM prompt assembly (pure string building — no reqwest).
//!
//! `build_dictionary_prompt` renders the user's dictionary as a prompt section
//! (capped at 200 entries). `get_cleanup_prompt` returns the system prompt for a
//! cleanup level. The actual LLM call (`cleanup_text`) stays in `src-tauri`.

use crate::config::DictEntry;

/// Maximum number of dictionary entries injected into the prompt.
const DICT_PROMPT_CAP: usize = 200;

/// Build the dictionary section of the LLM system prompt.
///
/// Returns an empty string for an empty dictionary (nothing to inject).
/// Entries are rendered as `word → replacement` lines; at most 200 entries
/// are included (extras are silently dropped).
pub fn build_dictionary_prompt(entries: &[DictEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Correct these terms exactly when they appear (case-sensitive):\n",
    );
    for entry in entries.iter().take(DICT_PROMPT_CAP) {
        out.push_str(&format!("- {} → {}\n", entry.word, entry.replacement));
    }
    out
}

/// Return the base cleanup system prompt for a level.
///
/// Levels: `"none"`, `"light"`, `"medium"`, `"high"`. Unknown levels fall back
/// to the `"medium"` prompt.
pub fn get_cleanup_prompt(level: &str) -> &str {
    match level {
        "none" => "Return the input text exactly as-is. Do not change anything.",
        "light" => {
            "Clean up the transcription lightly: fix obvious punctuation and capitalization
only. Do not rephrase, summarize, or remove content. Preserve the speaker's words."
        }
        "medium" => {
            "Clean up the transcription: fix punctuation, capitalization, and obvious
mis-transcriptions. Remove filler words (um, uh, you know) and false starts.
Keep the original meaning, tone, and level of detail. Do not add information."
        }
        "high" => {
            "Clean up and polish the transcription: fix punctuation, capitalization, and
mis-transcriptions; remove filler words and false starts; fix obvious grammar.
You may reword slightly for clarity, but never change the meaning, tone, or
level of detail. Do not add information that was not spoken."
        }
        _ => {
            "Clean up the transcription: fix punctuation, capitalization, and obvious
mis-transcriptions. Remove filler words (um, uh, you know) and false starts.
Keep the original meaning, tone, and level of detail. Do not add information."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(word: &str, replacement: &str) -> DictEntry {
        DictEntry {
            word: word.into(),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn empty_dictionary_empty_prompt() {
        assert_eq!(build_dictionary_prompt(&[]), "");
    }

    #[test]
    fn three_entries_correct_format() {
        let entries = vec![
            entry("voxable", "Voxable"),
            entry("llm", "LLM"),
            entry("tauri", "Tauri"),
        ];
        let prompt = build_dictionary_prompt(&entries);
        assert!(prompt.starts_with("Correct these terms exactly"));
        assert!(prompt.contains("- voxable → Voxable\n"));
        assert!(prompt.contains("- llm → LLM\n"));
        assert!(prompt.contains("- tauri → Tauri\n"));
        assert_eq!(prompt.matches('\n').count(), 4); // header + 3 lines
    }

    #[test]
    fn capped_at_200_entries() {
        let entries: Vec<DictEntry> = (0..201).map(|i| entry(&i.to_string(), "x")).collect();
        let prompt = build_dictionary_prompt(&entries);
        assert!(prompt.contains("- 0 → x\n"));
        assert!(prompt.contains("- 199 → x\n"));
        assert!(!prompt.contains("- 200 → x\n"));
        // header + 200 lines = 201 newlines
        assert_eq!(prompt.matches('\n').count(), 201);
    }

    #[test]
    fn cleanup_prompt_none_is_exact() {
        assert_eq!(
            get_cleanup_prompt("none"),
            "Return the input text exactly as-is. Do not change anything."
        );
    }

    #[test]
    fn cleanup_prompt_levels_distinct() {
        let light = get_cleanup_prompt("light");
        let medium = get_cleanup_prompt("medium");
        let high = get_cleanup_prompt("high");
        assert_ne!(light, medium);
        assert_ne!(medium, high);
        assert_ne!(light, high);
        // Unknown level falls back to the medium prompt.
        assert_eq!(get_cleanup_prompt("bogus"), medium);
    }
}
