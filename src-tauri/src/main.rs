// No console window in release builds — this is a GUI app, not a terminal.
// (Debug keeps the console so `log::info!` output is visible during dev.)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_context;
mod audio;
mod config;
mod llm;
mod whisper;

use audio::Recorder;
use config::{load_settings, model_path, DictEntry, FlowbarPosition, Settings, Snippet};
use std::str::FromStr;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, State,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use voxable_core::history::HistoryEntry;
use whisper::AppState;

// Flow Bar logical size (must match tauri.conf.json + flowbar.css).
// The window is larger than the visible pill so its soft shadow isn't clipped.
const FLOWBAR_W: f64 = 340.0;
const FLOWBAR_H: f64 = 96.0;
const FLOWBAR_MARGIN: f64 = 40.0; // gap from the bottom edge

fn main() {
    init_logging();

    // TEMP: bare Tauri app with the same linked dependencies, to separate a runtime
    // problem from a linkage problem.
    let context = tauri::generate_context!();
    if std::env::var("VOX_MINIMAL").is_ok() {
        tauri::Builder::default()
            .run(context)
            .expect("minimal run failed");
        return;
    }

    let default_settings = Settings::default();
    let app_state = AppState::new(default_settings.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(app_state)
        .manage(Recorder::new())
        // Single global handler for BOTH the tray menu and the Flow Bar
        // right-click context menu (all muda menu events route here).
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hub" => show_hub(app),
            "open_settings" => {
                show_hub(app);
                let _ = app.emit_to("hub", "hub-navigate", "settings");
            }
            "open_history" => {
                show_hub(app);
                let _ = app.emit_to("hub", "hub-navigate", "history");
            }
            "paste_last" => {
                let state = app.state::<AppState>();
                let last = state.last_result.lock().clone();
                if let Some(text) = last {
                    let _ = app.clipboard().write_text(text);
                }
            }
            "hide_1hr" => {
                hide_flowbar_for(app, 1);
            }
            "show_flowbar" => {
                clear_flowbar_hide(app);
            }
            _ => {}
        })
        .setup(|app| {
            let handle = app.handle().clone();

            // Load persisted settings into shared state.
            let settings = load_settings(&handle);
            {
                let state = app.state::<AppState>();
                *state.settings.lock() = settings.clone();
            }

            // Prune stale history (14-day / 500-entry retention).
            {
                let state = app.state::<AppState>();
                let res = state.history.lock().cleanup_old(14, 500);
                if let Err(e) = res {
                    log::warn!("history cleanup failed: {}", e);
                }
            }

            // --- System tray ---
            let show_item =
                MenuItem::with_id(&handle, "show_hub", "Open Voxable", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(&handle, "open_settings", "Settings", true, None::<&str>)?;
            let showbar_item =
                MenuItem::with_id(&handle, "show_flowbar", "Show Flow Bar", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(&handle)?;
            let quit_item = PredefinedMenuItem::quit(&handle, Some("Quit"))?;
            let menu = Menu::with_items(
                &handle,
                &[&show_item, &settings_item, &showbar_item, &sep, &quit_item],
            )?;

            let mut tray_builder = TrayIconBuilder::with_id("voxable-tray")
                .menu(&menu)
                .tooltip("Voxable")
                // macOS menu-bar convention: a left click opens the menu. On Windows the
                // menu belongs on right-click, and left-click opens the Hub (below).
                .show_menu_on_left_click(cfg!(target_os = "macos"));
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            let tray_icon = tray_builder.build(app)?;
            app.manage(tray_icon);

            // --- Global hotkey (non-fatal if the combo is taken) ---
            let hotkey = if settings.hotkey.is_empty() {
                "Super+Alt+Space".to_string()
            } else {
                settings.hotkey.clone()
            };
            if let Err(e) = register_hotkey(&handle, &hotkey) {
                log::warn!(
                    "Could not register global hotkey '{}': {}. Use the Flow Bar button or set a different hotkey in Settings.",
                    hotkey, e
                );
            }

            // --- Flow Bar: position + hidden-until state ---
            if let Some(flowbar) = app.get_webview_window("flowbar") {
                // Make it non-activating so it never steals focus from the
                // user's active text field (Windows only).
                #[cfg(windows)]
                if let Ok(h) = flowbar.hwnd() {
                    app_context::set_noactivate(h.0 as isize);
                }

                apply_flowbar_position(&flowbar, settings.flowbar_position.as_ref());

                let hidden = is_hidden_now(settings.flowbar_hidden_until.as_deref())
                    || !settings.flowbar_visible;
                if hidden {
                    let _ = flowbar.hide();
                } else {
                    let _ = flowbar.show();
                }
            }

            // --- Hub: hide on close instead of quitting ---
            if let Some(hub) = app.get_webview_window("hub") {
                let win = hub.clone();
                hub.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            transcribe,
            cleanup,
            get_settings,
            save_settings,
            copy_to_clipboard,
            paste_to_active,
            model_status,
            get_dictionary,
            add_dictionary_entry,
            remove_dictionary_entry,
            get_snippets,
            add_snippet,
            remove_snippet,
            get_history,
            clear_history,
            get_history_audio,
            get_flowbar_position,
            set_flowbar_position,
            hide_flowbar,
            show_flowbar,
            show_flowbar_menu,
            toggle_window,
            set_hotkey,
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // macOS: both windows hide rather than close, so clicking the Dock icon has
            // nothing to restore unless we do it here.
            #[cfg(target_os = "macos")]
            if matches!(_event, tauri::RunEvent::Reopen { .. }) {
                show_hub(_app);
            }
        });
}

// --- Logging ---

/// Log to stderr and to `~/Library/Logs/Voxable/voxable.log`.
///
/// A GUI app launched from Finder has nowhere to write stderr, so without a log file
/// there is no way to see what happened on a user's machine.
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    );

    if let Some(path) = log_file_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    builder.init();
    log::info!("--- Voxable {} starting ---", env!("CARGO_PKG_VERSION"));
}

fn log_file_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(dirs::home_dir()?.join("Library/Logs/Voxable/voxable.log"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(dirs::data_local_dir()?.join("Voxable").join("voxable.log"))
    }
}

// --- Helpers ---

/// Show + focus the Hub window.
fn show_hub(app: &AppHandle) {
    match app.get_webview_window("hub") {
        Some(window) => {
            center_on_active_monitor(app, &window);
            if let Err(e) = window.show() {
                log::error!("hub show() failed: {e}");
            }
            if let Err(e) = window.set_focus() {
                log::error!("hub set_focus() failed: {e}");
            }
            // macOS: an accessory/background app cannot raise a window without also
            // activating the process.
            #[cfg(target_os = "macos")]
            if let Err(e) = app.show() {
                log::error!("app show() failed: {e}");
            }
        }
        None => log::error!("no window labeled 'hub' — check tauri.conf.json"),
    }
}

/// Center a hidden window on the monitor under the cursor.
///
/// With no `x`/`y` in tauri.conf.json the OS picks the spot, and on a multi-display Mac
/// the Hub landed on a coordinate where nothing rendered. Placing it ourselves means
/// "Open Voxable" always puts the window where the user is looking. Only applied while
/// the window is hidden, so a window the user has dragged stays put.
fn center_on_active_monitor(app: &AppHandle, window: &tauri::WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        return;
    }
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|c| window.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let (mp, ms) = (monitor.position(), monitor.size());
    let x = mp.x + (ms.width as i32 - size.width as i32) / 2;
    let y = mp.y + (ms.height as i32 - size.height as i32) / 2;
    log::info!("centering hub at ({x}, {y}) on monitor {:?}", monitor.name());
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// True if `until` is a valid RFC 3339 timestamp still in the future.
fn is_hidden_now(until: Option<&str>) -> bool {
    match until {
        Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt > chrono::Utc::now())
            .unwrap_or(false),
        None => false,
    }
}

/// Persist a "hide the Flow Bar for N hours" state and hide the window.
fn hide_flowbar_for(app: &AppHandle, hours: u32) {
    let state = app.state::<AppState>();
    let settings = {
        let mut s = state.settings.lock();
        let until = chrono::Utc::now() + chrono::Duration::hours(hours as i64);
        s.flowbar_hidden_until = Some(until.to_rfc3339());
        s.clone()
    };
    let _ = config::save_settings(app, &settings);
    if let Some(w) = app.get_webview_window("flowbar") {
        let _ = w.hide();
    }
}

/// Clear any "hidden until" state and show the Flow Bar.
fn clear_flowbar_hide(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = {
        let mut s = state.settings.lock();
        s.flowbar_hidden_until = None;
        s.flowbar_visible = true;
        s.clone()
    };
    let _ = config::save_settings(app, &settings);
    if let Some(w) = app.get_webview_window("flowbar") {
        let _ = w.show();
    }
}

/// Apply the saved Flow Bar position (physical pixels), or default to
/// bottom-center of the primary monitor.
fn apply_flowbar_position(
    window: &tauri::WebviewWindow,
    saved: Option<&FlowbarPosition>,
) {
    if let Some(pos) = saved {
        let _ = window.set_position(PhysicalPosition::new(pos.x, pos.y));
        return;
    }
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size(); // physical
        let origin = monitor.position(); // physical
        let bar_w = FLOWBAR_W * scale;
        let bar_h = FLOWBAR_H * scale;
        let margin = FLOWBAR_MARGIN * scale;
        let x = origin.x as f64 + (size.width as f64 - bar_w) / 2.0;
        let y = origin.y as f64 + size.height as f64 - bar_h - margin;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Register (or re-register) the global dictation hotkey. The handler ensures the
/// Flow Bar is visible and emits `toggle-recording` to it (sole dictation owner).
fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut =
        Shortcut::from_str(hotkey).map_err(|e| format!("Invalid hotkey '{}': {}", hotkey, e))?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let handle = app.clone();
    gs.on_shortcut(shortcut, move |_app, _sc, event| {
        if event.state != ShortcutState::Pressed {
            return;
        }
        if let Some(w) = handle.get_webview_window("flowbar") {
            // Show but DO NOT focus — the Flow Bar is non-activating so the
            // user's text field keeps focus (that's what lets us paste into it).
            if !w.is_visible().unwrap_or(false) {
                let _ = w.show();
            }
        }
        let _ = handle.emit_to("flowbar", "toggle-recording", ());
    })
    .map_err(|e| format!("Failed to register hotkey '{}': {}", hotkey, e))
}

// --- Recording pipeline commands ---

/// Start microphone capture. Returns immediately; audio accumulates in the
/// background until `stop_recording` (or the 2-minute cap).
#[tauri::command]
fn start_recording(recorder: State<'_, Recorder>) -> Result<(), String> {
    recorder.start()
}

#[derive(serde::Serialize)]
struct StopResult {
    duration_ms: u32,
    samples: usize,
}

/// Stop capture; stash audio + duration in app state.
#[tauri::command]
fn stop_recording(
    recorder: State<'_, Recorder>,
    state: State<'_, AppState>,
) -> Result<StopResult, String> {
    let duration_ms = recorder.duration_ms();
    let audio = recorder.stop();
    let samples = audio.len();
    *state.last_audio.lock() = audio;
    *state.last_duration_ms.lock() = duration_ms;
    Ok(StopResult {
        duration_ms,
        samples,
    })
}

/// Transcribe the most recently captured audio; store the raw text in state.
#[tauri::command]
async fn transcribe(state: State<'_, AppState>) -> Result<String, String> {
    let audio = { state.last_audio.lock().clone() };
    if audio.is_empty() {
        return Err("No audio captured".into());
    }

    let (model_name, language) = {
        let settings = state.settings.lock();
        (settings.whisper_model.clone(), settings.language.clone())
    };

    // Ensure model is downloaded (no locks held across this await).
    let path = whisper::ensure_model(&model_name).await?;

    // Load model into the engine if needed (brief lock, no await inside).
    {
        let mut engine = state.whisper.lock();
        engine.load(&model_name, &path)?;
    }

    // Transcription is CPU-bound and blocking — run it off the async runtime.
    let whisper_arc = Arc::clone(&state.whisper);
    let result = tokio::task::spawn_blocking(move || {
        let mut engine = whisper_arc.lock();
        engine.transcribe(&audio, &language)
    })
    .await
    .map_err(|e| format!("Transcription task failed: {}", e))??;

    *state.last_raw_text.lock() = Some(result.clone());
    Ok(result)
}

/// Snippet-expand + LLM-clean the last transcript, write a history entry with the
/// real audio + duration, and broadcast `dictation-complete`.
#[tauri::command]
async fn cleanup(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let raw = state.last_raw_text.lock().clone().unwrap_or_default();
    if raw.trim().is_empty() {
        return Err("No transcript to clean up".into());
    }

    let settings = state.settings.lock().clone();
    let audio = { state.last_audio.lock().clone() };
    let duration_ms = *state.last_duration_ms.lock() as u64;

    // Snippet expansion happens pre-LLM.
    let expanded = voxable_core::snippets::expand_snippets(&raw, &settings.snippets);

    // Foreground app (Windows only) as LLM context.
    let foreground = app_context::get_foreground_app();

    let cleaned = match llm::cleanup_text(
        &settings,
        &expanded,
        &settings.dictionary,
        foreground.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("LLM cleanup failed, using expanded raw text: {}", e);
            expanded.clone()
        }
    };

    // Persist to history with real audio + duration.
    {
        let entry = HistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            app: foreground.clone(),
            raw: raw.clone(),
            cleaned: cleaned.clone(),
            model: settings.whisper_model.clone(),
            cleanup_level: settings.cleanup_level.clone(),
            duration_ms,
        };
        if let Err(e) = state.history.lock().add_entry(&entry, &audio) {
            log::warn!("Failed to write history entry: {}", e);
        }
    }

    *state.last_result.lock() = Some(cleaned.clone());
    let _ = app.emit("dictation-complete", &cleaned);
    Ok(cleaned)
}

// --- Settings ---

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().clone())
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    config::save_settings(&app, &settings)?;
    *state.settings.lock() = settings;
    Ok(())
}

#[tauri::command]
fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Paste the current clipboard into the focused window (synthesized Ctrl+V).
/// Relies on the Flow Bar being non-activating, so the user's field still has
/// focus. Call after `copy_to_clipboard`.
#[tauri::command]
fn paste_to_active() -> Result<(), String> {
    app_context::send_ctrl_v()
}

#[derive(serde::Serialize)]
struct ModelStatus {
    model: String,
    downloaded: bool,
    path: String,
}

#[tauri::command]
fn model_status(state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let settings = state.settings.lock();
    let model_name = settings.whisper_model.clone();
    let path = model_path(&model_name);
    Ok(ModelStatus {
        model: model_name,
        downloaded: path.exists(),
        path: path.to_string_lossy().to_string(),
    })
}

// --- Dictionary ---

#[tauri::command]
fn get_dictionary(state: State<'_, AppState>) -> Result<Vec<DictEntry>, String> {
    Ok(state.settings.lock().dictionary.clone())
}

#[tauri::command]
fn add_dictionary_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    word: String,
    replacement: String,
) -> Result<(), String> {
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".into());
    }
    let settings = {
        let mut s = state.settings.lock();
        if let Some(e) = s.dictionary.iter_mut().find(|e| e.word == word) {
            e.replacement = replacement;
        } else {
            s.dictionary.push(DictEntry { word, replacement });
        }
        s.clone()
    };
    config::save_settings(&app, &settings)
}

#[tauri::command]
fn remove_dictionary_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    word: String,
) -> Result<(), String> {
    let settings = {
        let mut s = state.settings.lock();
        s.dictionary.retain(|e| e.word != word);
        s.clone()
    };
    config::save_settings(&app, &settings)
}

// --- Snippets ---

#[tauri::command]
fn get_snippets(state: State<'_, AppState>) -> Result<Vec<Snippet>, String> {
    Ok(state.settings.lock().snippets.clone())
}

#[tauri::command]
fn add_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    trigger: String,
    expansion: String,
) -> Result<(), String> {
    let trigger = trigger.trim().to_string();
    if trigger.is_empty() {
        return Err("Trigger cannot be empty".into());
    }
    let settings = {
        let mut s = state.settings.lock();
        if let Some(e) = s.snippets.iter_mut().find(|e| e.trigger == trigger) {
            e.expansion = expansion;
        } else {
            s.snippets.push(Snippet { trigger, expansion });
        }
        s.clone()
    };
    config::save_settings(&app, &settings)
}

#[tauri::command]
fn remove_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    trigger: String,
) -> Result<(), String> {
    let settings = {
        let mut s = state.settings.lock();
        s.snippets.retain(|e| e.trigger != trigger);
        s.clone()
    };
    config::save_settings(&app, &settings)
}

// --- History ---

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    // Newest first for display.
    let mut entries = state.history.lock().load_entries()?;
    entries.reverse();
    Ok(entries)
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    // Prune everything (keep 0 entries, drop all audio).
    state.history.lock().cleanup_old(0, 0)
}

#[tauri::command]
fn get_history_audio(state: State<'_, AppState>, id: String) -> Result<Vec<f32>, String> {
    state.history.lock().get_audio(&id)
}

// --- Flow Bar ---

#[tauri::command]
fn get_flowbar_position(state: State<'_, AppState>) -> Result<Option<FlowbarPosition>, String> {
    Ok(state.settings.lock().flowbar_position.clone())
}

#[tauri::command]
fn set_flowbar_position(
    app: AppHandle,
    state: State<'_, AppState>,
    position: FlowbarPosition,
) -> Result<(), String> {
    let settings = {
        let mut s = state.settings.lock();
        s.flowbar_position = Some(position);
        s.clone()
    };
    config::save_settings(&app, &settings)
}

#[tauri::command]
fn hide_flowbar(app: AppHandle, hours: u32) -> Result<(), String> {
    hide_flowbar_for(&app, hours.max(1));
    Ok(())
}

#[tauri::command]
fn show_flowbar(app: AppHandle) -> Result<(), String> {
    clear_flowbar_hide(&app);
    Ok(())
}

/// Show the Flow Bar right-click context menu at the cursor.
#[tauri::command]
fn show_flowbar_menu(app: AppHandle) -> Result<(), String> {
    let paste = MenuItem::with_id(&app, "paste_last", "Paste last result", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let history = MenuItem::with_id(&app, "open_history", "History…", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let settings = MenuItem::with_id(&app, "open_settings", "Settings…", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let hide = MenuItem::with_id(&app, "hide_1hr", "Hide for 1 hour", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep1 = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let sep2 = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let quit = PredefinedMenuItem::quit(&app, Some("Quit Voxable")).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(
        &app,
        &[&paste, &history, &settings, &sep1, &hide, &sep2, &quit],
    )
    .map_err(|e| e.to_string())?;

    if let Some(window) = app.get_webview_window("flowbar") {
        window.popup_menu(&menu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// --- Window ---

#[tauri::command]
fn toggle_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("hub") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(())
}

#[tauri::command]
fn set_hotkey(app: AppHandle, state: State<'_, AppState>, hotkey: String) -> Result<(), String> {
    register_hotkey(&app, &hotkey)?;
    let settings = {
        let mut s = state.settings.lock();
        s.hotkey = hotkey;
        s.clone()
    };
    config::save_settings(&app, &settings)
}
