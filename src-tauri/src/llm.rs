//! LLM cleanup pass. The prompt assembly (level presets + dictionary section)
//! lives in the pure, unit-tested `voxable_core::prompt` module; this file only
//! does the actual HTTP call to an OpenAI-compatible `/chat/completions` endpoint.

use crate::config::{DictEntry, Settings};
use serde_json::json;
use voxable_core::prompt::{build_dictionary_prompt, get_cleanup_prompt};

/// Run `raw_text` through the configured LLM cleanup pass.
///
/// The system prompt is `get_cleanup_prompt(level)` (or the user's `custom_prompt`
/// when set) plus the dictionary section plus a foreground-app context line. If no
/// LLM is configured, or the cleanup level is `"none"` with no custom prompt, the
/// input is returned unchanged (no network call).
pub async fn cleanup_text(
    settings: &Settings,
    raw_text: &str,
    dictionary: &[DictEntry],
    foreground_app: Option<&str>,
) -> Result<String, String> {
    let level = settings.cleanup_level.trim();
    let has_custom = !settings.custom_prompt.trim().is_empty();

    // "none" with no custom prompt: nothing to do, skip the API call entirely.
    if level == "none" && !has_custom {
        return Ok(raw_text.to_string());
    }

    let base_url = settings.llm_base_url.trim().trim_end_matches('/');
    let api_key = settings.llm_api_key.trim();
    let model = if settings.llm_model.trim().is_empty() {
        "gpt-4o-mini"
    } else {
        settings.llm_model.trim()
    };

    if base_url.is_empty() || api_key.is_empty() {
        // No LLM configured — return raw text as-is.
        return Ok(raw_text.to_string());
    }

    // Base prompt: a custom prompt overrides the level preset.
    let mut system_prompt = if has_custom {
        settings.custom_prompt.trim().to_string()
    } else {
        get_cleanup_prompt(level).to_string()
    };

    let dict_section = build_dictionary_prompt(dictionary);
    if !dict_section.is_empty() {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&dict_section);
    }

    if let Some(app) = foreground_app {
        if !app.is_empty() {
            system_prompt.push_str(&format!(
                "\n\nContext: the user is dictating into the app \"{}\". Use this only to \
                 disambiguate homophones/formatting; do not mention it in the output.",
                app
            ));
        }
    }

    let url = format!("{}/chat/completions", base_url);

    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": raw_text}
        ],
        "temperature": 0.2,
        "max_tokens": 2000
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read LLM response: {}", e))?;

    if !status.is_success() {
        return Err(format!("LLM API error ({}): {}", status, text));
    }

    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

    let cleaned = parsed["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Unexpected LLM response format")?
        .trim()
        .to_string();

    Ok(cleaned)
}
