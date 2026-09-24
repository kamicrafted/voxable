# Voxable — Project State

**Last updated:** 2026-09-21 (v0.5.3 macOS shipped; Windows 0.5.3 pending)
**Version:** **0.5.3**. macOS is at [v0.5.3](https://github.com/kamicrafted/voxable/releases/tag/v0.5.3). Windows is still at [v0.5.2-windows](https://github.com/kamicrafted/voxable/releases/tag/v0.5.2-windows).
**Status:** **macOS shipped, Windows pending.** There was no macOS 0.5.2 release: QA found an abort on quit, so the version went straight to 0.5.3.

## v0.5.3 (2026-09-21, macOS shipped; Windows pending)

Built on the personal Mac (`dave`, clone at `~/Claude/Code/Personal/voxable`). Three fixes came out of this session's QA:

1. **Abort on quit, macOS 15+** (#5). ggml's Metal device is a C++ static. Its destructor runs inside `exit()` and asserts that every Metal buffer has been freed. `AppState` kept the `WhisperContext` alive, so every quit after a dictation crashed. The fix drops the model on `RunEvent::Exit`, with a 2 s lock timeout. This fix only affects macOS.
2. **Save settings gave no feedback** (#6). "Settings saved" went to the status line on the hidden Dictation tab. The button now shows "Saving…", then "Saved ✓". This is a shared frontend change.
3. **A clean macOS build failed with the 10.15 `std::filesystem` error** (#4). Tauri CLI 2.11.4 exports `MACOSX_DEPLOYMENT_TARGET` from `bundle.macOS.minimumSystemVersion` (default 10.13), and that export overrides the `.cargo/config.toml` `[env]` pin. The fix pins `minimumSystemVersion` to 11.0.

QA passed on the Mac:
- Onboarding shows the Microphone and Accessibility prompts.
- `medium` downloads after you click Save and then dictate.
- Quitting after a dictation leaves no crash report.

The update check skips releases that have no asset for the running platform, so Windows 0.5.2 users are not offered 0.5.3.

**Windows 0.5.3 also adds hold-to-talk** (walkie-talkie): the global hotkey now handles key release on Windows (global-hotkey 0.8 polls the key after the press), so a quick tap toggles hands-free and holding ≥300ms is push-to-talk — matching macOS. Fixes the "have to tap twice" report.

**Next step (Windows host):** building/releasing v0.5.3-windows now (CPU + shareable CUDA), README Windows rows updated to 0.5.3. Host QA to confirm: the Save button shows "Saved ✓", and holding the hotkey records then stops on release (tap still toggles).

## v0.5.2 (2026-09-11, Windows shipped; macOS pending)

Four host-found fixes after v0.5.0 (all found only because the host compiles and runs the app):

1. **`app.show()` compile fix** (`e69f97f`) — macOS-only API in a `#[cfg(windows)]` block; removed (redundant: `window.show()` + `set_focus()` handle it).
2. **Windows-native onboarding** (`7b2b0f8`) — the macOS-authored screen asked for Accessibility, referenced `fn`, said "System Settings", dead permission buttons. Rewrote platform-split (`data-os` on `<html>`); Windows gets a native how-to screen.
3. **Dropped native acrylic on Windows** (`ab30a1f`) — acrylic fills the window rect and can't be the round collapsed dot. CSS paints the shape instead.
4. **Streaming model download + progress** (`e58fc64`, `3dee453`) — `ensure_model` buffered the whole model in RAM (~1.5 GB for medium) with no progress and no partial cleanup; a model switch froze the Flow Bar on "Transcribing". Now streams via `Response::chunk()`, emits a `model-download` event the Flow Bar shows as "Downloading model N%", deletes truncated downloads. Model size labels corrected (base 148 MB, medium 1.5 GB, large-v3 3.1 GB).

**Verified on host:** `cargo check` clean; `cargo test -p voxable-core` → 77 passed.

**Open:** hotkey `Win+Alt+Space` is "already registered" on Dave's machine (Windows 11); he set `Alt+Win+Z`. Worth investigating what owns the default and choosing a safer one.

**Next step:** macOS 0.5.2 build on the Mac (`git pull` → `build-mac.sh` → tag `v0.5.2` → `gh release create` → update README macOS row → QA: onboarding unchanged, model switch shows "Downloading model N%").

## v0.5.0 features (from macOS, now on Windows)

- Flow Bar rebuilt from Figma (289×36 pill, 36×36 dot, four states, 220ms animator-proxy resize)
- Dictionary primes Whisper's decoder (`set_initial_prompt`) + corrects transcript literally
- Update check on launch + tray, announcing a version once
- Hide-when-idle Flow Bar setting
- Hub switches no longer squeeze; history playback works; webview errors logged
- VAD against clip's own noise floor; <250ms clips not transcribed
- Push-to-talk (hold hotkey >300ms) on macOS; hands-free toggle default


## Manual QA checklist (V3, on the running app)
- [ ] Flow Bar appears (transparent pill, bottom-center, always-on-top); drag it → position persists across restart
- [ ] Global hotkey (Super+Alt+Space) toggles recording via the Flow Bar (single owner, no double-trigger)
- [ ] Speak → transcribe → cleanup → result copied to clipboard; a History entry with audio is written
- [ ] Right-click Flow Bar → context menu (Paste last, History…, Settings…, Hide 1 hour, Quit) works
- [ ] Tray icon menu (Open Voxable, Settings, Show Flow Bar, Quit) works
- [ ] Hub: all 5 tabs render; Settings save preserves dictionary/snippets; Dictionary/Snippets add/remove persist; History "Play" plays back audio
- [ ] Installers: `target/release/bundle/{msi,nsis}/` install & run on a clean machine (CPU build is portable)

## What it is
Voxable is a local voice dictation app (Wispr Flow alternative) built with Tauri v2.
V3 transforms it into a two-window system matching Wispr Flow's architecture:
a minimal Flow Bar (always-on-top, bottom-center) for the daily dictation loop,
and a Hub window (control panel) for settings, dictionary, snippets, and history.

## Architecture
- **Two Tauri windows** in one process:
  - Flow Bar: 289×36 pill (collapses to 36×36 dot), always-on-top, non-activating, draggable, position persists
  - Hub: 480×640, hidden by default, opened via tray or right-click Flow Bar
- **Rust is single source of truth**; both windows invoke the same commands
- **Rust global shortcut is the sole dictation trigger** (no JS-side hotkey)
- **whisper-rs** for local ASR (CPU / CUDA / Metal backends)
- **cpal** for mic capture
- **Vite multi-page** build (flowbar.html + index.html + onboarding.html)
- **Vanilla JS** frontend (no framework)

## Crate split
- **`voxable-core/`** — pure logic, no Tauri/whisper/cpal/OS deps. Deps: serde, serde_json, regex, chrono, dirs.
  - `config` — `Settings`, `DictEntry`, `Snippet`, `FlowbarPosition`, path helpers
  - `snippets` — `expand_snippets` (word-boundary, case-insensitive, `NoExpand`)
  - `prompt` — `build_dictionary_prompt`, `get_cleanup_prompt`
  - `history` — `History`/`HistoryEntry`: JSON index + `.f32` audio, atomic writes, 500 cap
  - `vad` — `trim_silence`
  - `hotkey` — press/hold decision logic (5 tests)
  - `screen` — Flow Bar placement math (logical points, multi-monitor)
  - `stats` — Home tab stats (words dictated, time saved, wpm, dictations, most-used app)
  - `version` — update check against GitHub releases
- **`src-tauri/`** — the Tauri app. Depends on `voxable-core`.
  - `main.rs` — ~25 commands, two-window setup, tray, hotkey, context menu, onboarding
  - `whisper.rs` — model management (streaming download + progress), transcription
  - `audio.rs` — cpal recording
  - `app_context.rs` — Win32 foreground app detection (cfg-gated)
  - `macos.rs` — Fn hotkey via CGEventTap, auto-paste, NSWorkspace (cfg-gated)
  - Frontend: `flowbar.{html,js,css}`, `index.html`/`main.js`/`styles.css` (Hub, 5 tabs), `onboarding.html`

## V3 New Features (vs V2)
- Flow Bar UI (replaces single-window UI)
- Hub with tabs: Dictation, Settings, Dictionary, Snippets, History
- Dictionary: custom word corrections injected into LLM prompt
- Snippets: trigger phrase → expansion (e.g. "brb" → "be right back")
- 4-level cleanup: Raw → Whisper → LLM → Final (visible in history)
- History with audio playback (14-day retention, stored in app data dir)
- Win32 foreground app detection (Windows-only, for LLM context)
- Right-click Flow Bar → context menu (Paste Last, History, Settings, Hide 1hr, Show, Quit)
- Tray icon with full menu
- Hands-free mode (tap to start, tap to stop) — **only mode** (push-to-talk deferred)
- Flow Bar drag + position persistence (OS-level via `-webkit-app-region: drag`)
- Audio playback in History tab

## Deferred
- `mic_device`: stored in Settings, not yet wired to cpal device selection
- `sound_enabled`: wired to the Hub's completion beep; not yet used elsewhere
- Push-to-talk: unreliable over global shortcut; hands-free (toggle) is the default. On macOS the Fn CGEventTap already receives both press and release, so hold-to-talk is available whenever we want it — `settings.mode` is stored but never read.
- Auto-paste is Windows-only (`SendInput`); Mac/Linux would need an equivalent (e.g. `enigo`)


## Known limitations
- No GPU acceleration by default (CUDA/Metal features available in Cargo.toml)
- Audio resampler is linear interpolation (good for speech, not music)
- No auto-start on boot (user must launch app)
- Win32 app detection is Windows-only (returns None on Linux/macOS)
- Auto-paste is Windows-only (`SendInput`); Mac/Linux would need an equivalent (e.g. `enigo`)
