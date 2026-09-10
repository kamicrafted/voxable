//! LLM prompt assembly (pure string building — no reqwest).
//!
//! `build_dictionary_prompt` renders the user's dictionary as a prompt section
//! (capped at 200 entries). `get_cleanup_prompt` returns the system prompt for a
//! cleanup level. The actual LLM call (`cleanup_text`) stays in `src-tauri`.

use crate::config::{DictEntry, Snippet};

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

/// Characters of vocabulary to hand Whisper. Its prompt window is about 224 tokens
/// and it is shared with nothing else, but a long list starts steering punctuation
/// and phrasing as well as spelling, so keep it short.
const WHISPER_VOCAB_CHARS: usize = 220;

/// Terms to prime Whisper's decoder with, from the dictionary and snippet triggers.
///
/// This is conditioning, not search-and-replace: the terms are prepended as context,
/// which makes the decoder more likely to produce those exact spellings. It is the
/// only one of the two dictionary mechanisms that works with no LLM configured.
///
/// Both sources matter, for different reasons. Dictionary *replacements* are the
/// spellings the user wants out (the left-hand side is whatever Whisper mishears, so
/// it would teach it the wrong thing). Snippet *triggers* are phrases the user says
/// out loud expecting an expansion — and expansion is a text match, so it silently
/// fails whenever the trigger is transcribed as something else.
pub fn build_whisper_vocabulary(dictionary: &[DictEntry], snippets: &[Snippet]) -> String {
    let mut terms: Vec<&str> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for term in dictionary
        .iter()
        .map(|e| e.replacement.trim())
        .chain(snippets.iter().map(|s| s.trigger.trim()))
    {
        if term.is_empty() {
            continue;
        }
        let key = term.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        terms.push(term);
    }

    if terms.is_empty() {
        return String::new();
    }

    // Grow up to the cap rather than truncating mid-term, which would prime the
    // decoder with a fragment.
    let mut out = String::new();
    for term in terms {
        let addition = if out.is_empty() {
            term.to_string()
        } else {
            format!(", {term}")
        };
        if out.len() + addition.len() > WHISPER_VOCAB_CHARS {
            break;
        }
        out.push_str(&addition);
    }
    if out.is_empty() {
        return String::new();
    }
    // A sentence, not a bare list: Whisper conditions on natural text, and a trailing
    // period keeps it from running the vocabulary into the transcript.
    format!("Glossary: {out}.")
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

    #[test]
    fn vocabulary_is_empty_without_entries() {
        assert_eq!(build_whisper_vocabulary(&[], &[]), "");
    }

    #[test]
    fn vocabulary_uses_the_replacement_not_the_misheard_word() {
        // The left-hand side is whatever Whisper got wrong — priming it with that
        // would teach the decoder the mistake.
        let dict = vec![DictEntry {
            word: "kahmeecrafted".into(),
            replacement: "Kamicrafted".into(),
        }];
        let out = build_whisper_vocabulary(&dict, &[]);
        assert!(out.contains("Kamicrafted"));
        assert!(!out.contains("kahmeecrafted"));
    }

    #[test]
    fn vocabulary_includes_snippet_triggers() {
        // Expansion is a text match, so a trigger Whisper mishears never fires.
        let snips = vec![Snippet {
            trigger: "kami gmail".into(),
            expansion: "hello@kamicrafted.com".into(),
        }];
        assert!(build_whisper_vocabulary(&[], &snips).contains("kami gmail"));
    }

    #[test]
    fn vocabulary_reads_as_a_sentence() {
        let dict = vec![DictEntry {
            word: "a".into(),
            replacement: "Voxable".into(),
        }];
        assert_eq!(build_whisper_vocabulary(&dict, &[]), "Glossary: Voxable.");
    }

    #[test]
    fn vocabulary_deduplicates_case_insensitively() {
        let dict = vec![DictEntry {
            word: "x".into(),
            replacement: "Voxable".into(),
        }];
        let snips = vec![Snippet {
            trigger: "voxable".into(),
            expansion: "y".into(),
        }];
        assert_eq!(build_whisper_vocabulary(&dict, &snips), "Glossary: Voxable.");
    }

    #[test]
    fn vocabulary_stops_at_a_whole_term_not_mid_word() {
        let dict: Vec<DictEntry> = (0..60)
            .map(|i| DictEntry {
                word: format!("w{i}"),
                replacement: format!("Supercalifragilistic{i}"),
            })
            .collect();
        let out = build_whisper_vocabulary(&dict, &[]);
        assert!(out.len() <= WHISPER_VOCAB_CHARS + "Glossary: .".len());
        // No trailing fragment: the last term before the period is complete.
        let body = out
            .trim_start_matches("Glossary: ")
            .trim_end_matches('.');
        for term in body.split(", ") {
            assert!(
                dict.iter().any(|d| d.replacement == term),
                "truncated term: {term}"
            );
        }
    }

    #[test]
    fn vocabulary_skips_blank_entries() {
        let dict = vec![DictEntry {
            word: "x".into(),
            replacement: "   ".into(),
        }];
        assert_eq!(build_whisper_vocabulary(&dict, &[]), "");
    }

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
