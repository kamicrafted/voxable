//! Snippet expansion (pure — no Tauri).
//!
//! `expand_snippets` replaces snippet triggers in text with their expansions.
//! Matching is word-boundary and case-insensitive; replacement text is literal
//! (`regex::NoExpand`) so `$` in expansions is not treated as a backreference.
//! A snippet whose trigger fails to compile as a regex is skipped (logged).

use regex::Regex;

use crate::config::{DictEntry, Snippet};

/// Replace each `from` with its `to`, whole-word and case-insensitive.
///
/// Shared by snippet expansion and dictionary corrections, which are the same
/// operation on different inputs. Replacements are literal (`regex::NoExpand`) so a
/// `$` in the output is not read as a backreference, and a pair that fails to
/// compile is skipped rather than panicking.
fn substitute(text: &str, pairs: impl Iterator<Item = (String, String)>) -> String {
    let mut out = text.to_string();
    for (from, to) in pairs {
        if from.is_empty() || to.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(&from));
        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(e) => {
                eprintln!("voxable: skipping {from:?}: invalid regex: {e}");
                continue;
            }
        };
        out = re.replace_all(&out, regex::NoExpand(&to)).to_string();
    }
    out
}

/// Apply dictionary corrections to transcribed text.
///
/// The dictionary has two consumers and this is the one that needs no LLM: whatever
/// Whisper actually typed is replaced with the written form. Priming the decoder
/// (see `prompt::build_whisper_vocabulary`) gets the spelling close; this makes it
/// exact.
///
/// Runs *before* snippet expansion, so a correction can repair a trigger phrase that
/// was misheard and let its snippet match after all.
pub fn apply_dictionary(text: &str, entries: &[DictEntry]) -> String {
    substitute(
        text,
        entries
            .iter()
            .map(|e| (e.word.trim().to_string(), e.replacement.trim().to_string())),
    )
}

/// Replace every snippet trigger in `text` with its expansion.
///
/// For each snippet the trigger is matched as a whole word, case-insensitively:
/// `\b<escaped trigger>\b` with `(?i)`. Snippets are applied in the order given;
/// an empty trigger or expansion is skipped. A trigger that does not compile as a
/// regex is skipped with a warning (never panics).
pub fn expand_snippets(text: &str, snippets: &[Snippet]) -> String {
    substitute(
        text,
        snippets
            .iter()
            .map(|s| (s.trigger.clone(), s.expansion.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_entry_corrects_what_whisper_typed() {
        let dict = vec![DictEntry {
            word: "Kamirafted".into(),
            replacement: "Kamicrafted".into(),
        }];
        assert_eq!(
            apply_dictionary("I work at Kamirafted today", &dict),
            "I work at Kamicrafted today"
        );
    }

    #[test]
    fn dictionary_correction_is_case_insensitive_and_whole_word() {
        let dict = vec![DictEntry {
            word: "voxable".into(),
            replacement: "Voxable".into(),
        }];
        assert_eq!(apply_dictionary("VOXABLE is fine", &dict), "Voxable is fine");
        // Not a whole word: left alone.
        assert_eq!(apply_dictionary("voxables", &dict), "voxables");
    }

    #[test]
    fn a_dictionary_entry_can_repair_a_snippet_trigger() {
        // The reason corrections run first: Whisper mishears the trigger, so the
        // snippet would never match on its own.
        let dict = vec![DictEntry {
            word: "Commie Gmail".into(),
            replacement: "kami gmail".into(),
        }];
        let snips = vec![Snippet {
            trigger: "kami gmail".into(),
            expansion: "hello@kamicrafted.com".into(),
        }];
        let corrected = apply_dictionary("send it to Commie Gmail", &dict);
        assert_eq!(
            expand_snippets(&corrected, &snips),
            "send it to hello@kamicrafted.com"
        );
    }

    #[test]
    fn a_blank_side_is_skipped() {
        let dict = vec![
            DictEntry { word: "".into(), replacement: "x".into() },
            DictEntry { word: "y".into(), replacement: "  ".into() },
        ];
        assert_eq!(apply_dictionary("y and z", &dict), "y and z");
    }

    #[test]
    fn a_dollar_sign_in_the_replacement_is_literal() {
        let dict = vec![DictEntry {
            word: "price".into(),
            replacement: "$5".into(),
        }];
        assert_eq!(apply_dictionary("the price", &dict), "the $5");
    }

    fn snip(trigger: &str, expansion: &str) -> Snippet {
        Snippet {
            trigger: trigger.into(),
            expansion: expansion.into(),
        }
    }

    #[test]
    fn single_trigger_replaced() {
        let out = expand_snippets("say brb to them", &[snip("brb", "be right back")]);
        assert_eq!(out, "say be right back to them");
    }

    #[test]
    fn multiple_triggers_order_independent() {
        let snippets = vec![snip("brb", "be right back"), snip("omw", "on my way")];
        // "omw" appears before "brb" in the text, after "brb" in the list.
        let out = expand_snippets("omw, then brb", &snippets);
        assert_eq!(out, "on my way, then be right back");
    }

    #[test]
    fn case_insensitive_matching() {
        let out = expand_snippets("BRB and Brb", &[snip("brb", "be right back")]);
        assert_eq!(out, "be right back and be right back");
    }

    #[test]
    fn word_boundary_no_partial_match() {
        let out = expand_snippets("the category is fine", &[snip("cat", "cat")]);
        assert_eq!(out, "the category is fine");
        // But a whole-word match still works.
        let out2 = expand_snippets("a cat sat", &[snip("cat", "kitten")]);
        assert_eq!(out2, "a kitten sat");
    }

    #[test]
    fn empty_trigger_or_expansion_skipped() {
        let snippets = vec![snip("", "x"), snip("brb", "")];
        let out = expand_snippets("brb", &snippets);
        assert_eq!(out, "brb");
    }

    #[test]
    fn dollar_in_expansion_is_literal() {
        let out = expand_snippets("cost: brb", &[snip("brb", "$5")]);
        assert_eq!(out, "cost: $5");
    }
}
