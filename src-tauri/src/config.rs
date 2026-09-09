//! Tauri-side settings glue. The `Settings` model + path helpers now live in the
//! pure, unit-tested `voxable_core` crate; this module re-exports them and keeps
//! only `load_settings`/`save_settings`, which need `tauri::AppHandle`.

use tauri::Manager;

pub use voxable_core::config::{
    config_dir, config_path, history_audio_dir, history_dir, model_info, model_path, models_dir,
    DictEntry, FlowbarPosition, Settings, Snippet,
};

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    let path = config_path();
    if path.exists() {
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<Settings>(&json) {
                return settings;
            }
        }
    }
    // Also try app data dir as fallback
    if let Ok(data_dir) = app.path().app_data_dir() {
        let path = data_dir.join("settings.json");
        if path.exists() {
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str::<Settings>(&json) {
                    return settings;
                }
            }
        }
    }
    Settings::default()
}

pub fn save_settings(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(config_path(), &json).map_err(|e| e.to_string())?;
    // Also write to app data dir for cross-platform consistency
    if let Ok(data_dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::write(data_dir.join("settings.json"), &json);
    }
    Ok(())
}
