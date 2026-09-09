//! Snippet expansion (pure — no Tauri).
//!
//! `expand_snippets` replaces snippet triggers in text with their expansions.
//! Matching is word-boundary and case-insensitive; replacement text is literal
//! (`regex::NoExpand`) so `$` in expansions is not treated as a backreference.
//! A snippet whose trigger fails to compile as a regex is skipped (logged).

use regex::Regex;

use crate::config::Snippet;

/// Replace every snippet trigger in `text` with its expansion.
///
/// For each snippet the trigger is matched as a whole word, case-insensitively:
/// `\b<escaped trigger>\b` with `(?i)`. Snippets are applied in the order given;
/// an empty trigger or expansion is skipped. A trigger that does not compile as a
/// regex is skipped with a warning (never panics).
pub fn expand_snippets(text: &str, snippets: &[Snippet]) -> String {
    let mut out = text.to_string();
    for snippet in snippets {
        if snippet.trigger.is_empty() || snippet.expansion.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(&snippet.trigger));
        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(e) => {
                eprintln!(
                    "voxable: skipping snippet {:?}: invalid regex: {}",
                    snippet.trigger, e
                );
                continue;
            }
        };
        out = re.replace_all(&out, regex::NoExpand(&snippet.expansion)).to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
