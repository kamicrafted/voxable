//! Settings model + path helpers (pure — no Tauri).
//!
//! omp Task 1 extends `Settings` with the V3 fields and adds the `DictEntry`,
//! `Snippet`, `FlowbarPosition` structs + `history_dir()`/`history_audio_dir()`
//! helpers (see the lean plan's Interfaces section). The Tauri-specific
//! `load_settings`/`save_settings` (which need `AppHandle`) stay in `src-tauri`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub whisper_model: String,
    pub llm_base_url: String,
    pub llm_api_key: String,
    pub llm_model: String,
    pub hotkey: String,
    pub language: String,
    pub auto_paste: bool,
    pub custom_prompt: String,
    // --- V3 fields ---
    pub cleanup_level: String,
    pub mode: String,
    pub mic_device: String,
    pub flowbar_position: Option<FlowbarPosition>,
    pub flowbar_visible: bool,
    pub flowbar_hidden_until: Option<String>,
    pub sound_enabled: bool,
    pub dictionary: Vec<DictEntry>,
    pub snippets: Vec<Snippet>,
    /// False until the user has been through the first-launch permission screen.
    pub onboarding_complete: bool,
    /// Hide the Flow Bar entirely when idle instead of shrinking it to a dot.
    /// Nothing is left on screen, so the tray's "Show Flow Bar" is the way back.
    #[serde(default)]
    pub flowbar_hide_when_idle: bool,
    /// The update version the user has already been told about, so a new release is
    /// announced once rather than at every launch until they act on it.
    #[serde(default)]
    pub update_notified_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowbarPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictEntry {
    pub word: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub trigger: String,
    pub expansion: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            whisper_model: "base".into(),
            llm_base_url: "https://api.openai.com/v1".into(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".into(),
            hotkey: default_hotkey().into(),
            language: "en".into(),
            auto_paste: true,
            custom_prompt: String::new(),
            cleanup_level: "medium".into(),
            mode: "hands-free".into(),
            mic_device: "default".into(),
            flowbar_position: None,
            flowbar_visible: true,
            flowbar_hidden_until: None,
            sound_enabled: true,
            dictionary: Vec::new(),
            snippets: Vec::new(),
            onboarding_complete: false,
            flowbar_hide_when_idle: false,
            update_notified_version: None,
        }
    }
}

/// The out-of-the-box hotkey.
///
/// On macOS this is the Fn / 🌐 key, which Voxable watches with an event tap rather
/// than a registered shortcut — see `src-tauri/src/macos.rs`. Elsewhere it is a
/// three-key combo, because a bare modifier is not something the OS will hand us.
pub fn default_hotkey() -> &'static str {
    if cfg!(target_os = "macos") {
        "Fn"
    } else {
        "Ctrl+Alt+Space"
    }
}

pub fn history_dir() -> PathBuf {
    config_dir().join("history")
}

pub fn history_audio_dir() -> PathBuf {
    history_dir().join("audio")
}

pub fn model_info(name: &str) -> (String, String) {
    let (file, size) = match name {
        "tiny" => ("ggml-tiny.bin", "39MB"),
        "base" => ("ggml-base.bin", "74MB"),
        "small" => ("ggml-small.bin", "244MB"),
        "medium" => ("ggml-medium.bin", "769MB"),
        "large-v3" => ("ggml-large-v3.bin", "1.5GB"),
        other => (other, "?"),
    };
    (file.to_string(), size.to_string())
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voxable")
}

pub fn models_dir() -> PathBuf {
    config_dir().join("models")
}

pub fn config_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn model_path(model_name: &str) -> PathBuf {
    let (file, _) = model_info(model_name);
    models_dir().join(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.whisper_model, "base");
        assert_eq!(back.hotkey, default_hotkey());
        assert!(back.auto_paste);
    }

    #[test]
    fn v3_fields_round_trip() {
        let mut s = Settings::default();
        s.cleanup_level = "high".into();
        s.flowbar_position = Some(FlowbarPosition { x: 10.0, y: 20.0 });
        s.flowbar_hidden_until = Some("2026-09-09T00:00:00Z".into());
        s.flowbar_hide_when_idle = true;
        s.update_notified_version = Some("0.9.9".into());
        s.dictionary.push(DictEntry { word: "voxable".into(), replacement: "Voxable".into() });
        s.snippets.push(Snippet { trigger: "brb".into(), expansion: "be right back".into() });
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleanup_level, "high");
        assert_eq!(back.mode, "hands-free");
        assert_eq!(back.mic_device, "default");
        assert_eq!(back.flowbar_position, Some(FlowbarPosition { x: 10.0, y: 20.0 }));
        assert_eq!(back.flowbar_hidden_until.as_deref(), Some("2026-09-09T00:00:00Z"));
        assert!(back.flowbar_hide_when_idle);
        assert_eq!(back.update_notified_version.as_deref(), Some("0.9.9"));
        assert!(back.sound_enabled);
        assert_eq!(back.dictionary[0].word, "voxable");
        assert_eq!(back.snippets[0].expansion, "be right back");
    }

    #[test]
    fn v2_json_deserializes_with_v3_defaults() {
        // A settings file written by V2 (no V3 fields) must load, with V3 defaults.
        let v2_json = r#"{
            "whisper_model": "small",
            "llm_base_url": "https://api.openai.com/v1",
            "llm_api_key": "",
            "llm_model": "gpt-4o-mini",
            "hotkey": "Super+Alt+Space",
            "language": "en",
            "auto_paste": true,
            "custom_prompt": ""
        }"#;
        let back: Settings = serde_json::from_str(v2_json).unwrap();
        assert_eq!(back.whisper_model, "small");
        assert_eq!(back.cleanup_level, "medium");
        assert_eq!(back.mode, "hands-free");
        assert!(back.flowbar_position.is_none());
        assert!(back.flowbar_visible);
        assert!(back.sound_enabled);
        assert!(back.dictionary.is_empty());
        assert!(back.snippets.is_empty());
    }

    #[test]
    fn history_dirs_nested() {
        assert!(history_audio_dir().ends_with("history/audio"));
        assert!(history_dir().ends_with("history"));
    }

    #[test]
    fn model_info_known() {
        assert_eq!(model_info("base").0, "ggml-base.bin");
    }
}
