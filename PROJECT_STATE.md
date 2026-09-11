# Voxable — Project State

**Last updated:** 2026-09-11 (v0.5.0 Windows catch-up)
**Version:** **0.5.0** (pulled from `main`; macOS shipped v0.5.0 on 2026-09-11). Windows installers were three releases behind (v0.2.0); this session brought the Windows build up to v0.5.0.
**Status:** **Windows v0.5.0 catch-up complete.** All six fixes from the catch-up doc applied and `cargo check --target x86_64-pc-windows-msvc` passes clean. Ready for a Windows release build + manual QA.

## Windows v0.5.0 catch-up (2026-09-11)

Pulled `main` (v0.2.0 → v0.5.0, 49 files, +4509/-522). Six fixes applied for Windows:

1. **Flow Bar vibrancy (acrylic)** — `set_effects(Effect::Acrylic)` in setup (cfg(windows)). CSS `backdrop-filter: blur()` fallback already existed in `flowbar.css`.
2. **Onboarding permission screen** — `permission_status` returns `accessibility: true` + `microphone: "granted"` on Windows (no macOS-style prompts needed). Onboarding screen shows so the user can set a hotkey, but does NOT auto-dismiss on Windows (guarded by `IS_MACOS`); user clicks "Start using Voxable" to finish. Polling is skipped on Windows.
3. **Flow Bar right-click context menu** — `show_flowbar_menu` uses the Hub window as the popup host on Windows (Flow Bar is `WS_EX_NOACTIVATE` so `popup_menu` on it would fail).
4. **Hub window show/focus** — `show_hub` calls `app.show()` after `window.show()`/`set_focus()` on Windows (background app needs process activation to bring a window to the foreground).
5. **Update check asset filter** — `update.rs` now matches `.nsis.zip` in addition to `.exe`/`.msi`, so Windows builds find their own release assets.
6. **Flow Bar resize (center-preserving)** — `resize_flowbar` on non-macOS platforms adjusts `y` by `(old_h - new_h) / 2` before `set_size` so the pill grows about its vertical centre.

**Verified:** `cargo check --target x86_64-pc-windows-msvc` clean (no warnings, no errors) — prior session, before the onboarding fix (JS-only, no Rust changes since).

**Next step:** build the Windows release installer on the Windows host (`npm run tauri build`), then run the manual QA checklist from the catch-up doc.

## v0.5.0 features (from macOS, now on Windows)

- Flow Bar rebuilt from Figma (289×36 pill, 36×36 dot, four states, 220ms animator-proxy resize)
- Dictionary primes Whisper's decoder (`set_initial_prompt`) + corrects transcript literally
- Update check on launch + tray, announcing a version once
- Hide-when-idle Flow Bar setting
- Hub switches no longer squeeze; history playback works; webview errors logged
- VAD against clip's own noise floor; <250ms clips not transcribed
- Push-to-talk (hold hotkey >300ms) on macOS; hands-free toggle default

## Prior releases (summary)

- **v0.4.0** (2026-09-10, macOS): Flow Bar rework, VAD fix, cpal teardown fix, `log_from_ui`
- **v0.3.0** (2026-09-10, macOS): push-to-talk, transparency, positioning, permissions
- **v0.2.0** (2026-09-09): V3 shipped — two-window system, focus-preserving auto-paste, light redesign
- **v0.1.0** (2026-09-09): initial V3 release

**Mac build (2026-09-09):** builds and runs on Apple Silicon (`Voxable.app` arm64 + Metal-linked, `Voxable_0.1.0_aarch64.dmg`). Three macOS-only fixes landed: `.cargo/config.toml` pins `MACOSX_DEPLOYMENT_TARGET=11.0` (ggml needs 10.15+ for `std::filesystem`); `src-tauri/build.rs` links `libclang_rt.osx.a` for the Metal backend's Objective-C `@available`; and `scripts/build-mac.sh` builds `--bundles app` and makes the DMG with `hdiutil create -format UDZO`, because Tauri's `bundle_dmg.sh` needs Finder automation plus `hdiutil convert` and neither works on a Kandji-managed Mac. Details in BUILD.md + LEARNINGS.md.

**macOS native pass (2026-09-09):** `fn` / 🌐 is the default hotkey on macOS, watched with a CGEventTap (`src-tauri/src/macos.rs`) because Carbon's `RegisterEventHotKey` cannot bind a bare Fn. Auto-paste now works on macOS via a synthesized Cmd+V, and foreground-app detection uses `NSWorkspace`. A first-launch window (`onboarding.html`) walks through Microphone + Accessibility, polls for changes, and links straight to the right System Settings pane; nothing prompts unless the user presses the button. The Hub gained a Home tab (words dictated, time saved, wpm, dictations, most-used app, recent 5 with a link to full history) backed by `voxable-core/src/stats.rs` (10 tests). Hotkey setting is now a recorder that captures a real keypress and asks the OS whether the combo is free, and key labels follow the platform (⌘⌥⌃⇧ / fn on macOS). The model pill reads "Whisper · base" — it was showing a bare "base" with nothing saying what it meant.

**▶ RESUME HERE:** **Finish the macOS QA checklist.** Permissions, transparency and positioning are done and verified on the machine (2026-09-10): mic + Accessibility grant once and stick across rebuilds, the Flow Bar renders transparent, and dictation → cleanup → auto-paste works end to end (`Pasted ✓`, 110–266ms). Remaining: (1) the runtime checklist below — drag/persist **across displays with different scale factors**, context menu, tray menu, Hub tabs, settings-preserve-dictionary, history playback; (2) port the onboarding screen to Windows (it reports every permission as granted there); (3) deferred: wire `mic_device` to cpal device selection. **Streaming decode was scoped and dropped** — measured 30–48x realtime, so it would have saved ~100ms; the slow feeling was the first dictation paying model load + Metal warmup. Timing logs are in `whisper.rs` if it needs re-measuring.

**Shipped v0.5.0 (2026-09-11, macOS only):** Flow Bar rebuilt from Figma (nodes 18:146/18:147) —
289x36 pill and 36x36 dot sharing one height and one left padding so the icon never moves, native
vibrancy per appearance, four states (ready / recording / transcribing / copied) driven by real
exported SVGs, and a 220ms animator-proxy resize about the vertical centre. **Accuracy:** the
dictionary now primes Whisper's decoder (`set_initial_prompt`) with written forms *and* snippet
triggers, and separately corrects the transcript literally, so both work with no LLM key. **Updates:**
checks GitHub releases on launch and from the tray, announcing a version once. **Also:** a
hide-when-idle setting, the Hub's switches no longer squeeze, history playback works again
(`ensureCtx` was removed with the sounds extraction), and the webview reports uncaught errors to the
app log. Known: ⌃⌥Space cannot be bound — macOS symbolic hotkey 61 owns it.

**Shipped v0.4.0 (2026-09-10, macOS only):** Flow Bar reworked and two accuracy bugs fixed.
The window now *is* the pill (296x60) with native macOS vibrancy, a light/dark palette, and a
native shadow; it collapses to a 60x60 glass mic dot after 1.5s idle and expands on the hotkey or a
click, with the dot draggable via gesture detection rather than a drag region. Start and stop
sounds are synthesized in a shared `sounds.js`. The hotkey section moved to the top of Settings and
its recorder arms on focus. **Transcription:** the VAD now measures against the clip's own noise
floor — a fixed 0.01 threshold was discarding most of a dictation on a quiet mic — and clips under
250ms are not transcribed at all, since Whisper hallucinates rather than failing on a fragment.
The `cpal` stream teardown handshake was also fixed (`wait_idle` waited on a flag its caller set).
`log_from_ui` gives the webview a way into the app log, which release builds otherwise lack.
**Still unverified:** whether ⌃⌥Space reaches the hotkey recorder (the recorder logs every keydown
it sees, so the log answers it), and the Windows onboarding port.

**Shipped v0.3.0 (2026-09-10, macOS only):** merged via PR #2 → `main`, released at
https://github.com/kamicrafted/voxable/releases/tag/v0.3.0 with `Voxable_0.3.0_aarch64.dmg`. Adds
**push-to-talk** (tap the hotkey to toggle hands-free, hold past 300ms to talk and release to stop;
decision logic in `voxable-core/src/hotkey.rs`, 5 tests) on top of the transparency, positioning and
permission fixes. README gained a Download section with direct asset links — all four verified 200.
**The Windows installers are still v0.2.0** and must be rebuilt on the Windows host before a
cross-platform release; note that `voxable-core/src/screen.rs` and the logical-coordinate change
affect Windows too and are untested there.

**Mac signing (2026-09-10):** permissions now survive rebuilds. Run `./scripts/create-signing-identity.sh` once per machine (it needs a TTY — two keychain prompts), then `build-mac.sh` signs with "Voxable Dev" automatically and says so. Without that identity every rebuild is a new app to TCC and re-asks for Microphone + Accessibility.

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

## V3 Architecture
- **Two Tauri windows** in one process:
  - Flow Bar: 280×72, always-on-top, skip-taskbar, draggable, position persists
  - Hub: 480×640, hidden by default, opened via tray or right-click Flow Bar
- **Rust is single source of truth**; both windows invoke the same commands
- **Rust global shortcut is the sole dictation trigger** (no JS-side hotkey)
- **whisper-rs 0.16** for local ASR (same as V2)
- **cpal 0.15** for mic capture (same as V2)
- **Vite multi-page** build (flowbar.html + index.html)
- **Vanilla JS** frontend (no framework)

## Crate split (V3)
- **`voxable-core/`** — pure logic, no Tauri/whisper/cpal/OS deps. Deps: serde, serde_json, regex, chrono, dirs.
  - `config` — `Settings` (V2 + V3 fields), `DictEntry`, `Snippet`, `FlowbarPosition`, path helpers (`history_dir`, `history_audio_dir`)
  - `snippets` — `expand_snippets` (word-boundary, case-insensitive, `NoExpand`)
  - `prompt` — `build_dictionary_prompt` (cap 200), `get_cleanup_prompt` (none|light|medium|high)
  - `history` — `History`/`HistoryEntry`: JSON index + `.f32` audio, atomic writes, 500 cap, `cleanup_old`
  - `vad` — `trim_silence` (moved from `src-tauri/src/whisper.rs` for testability)
- **`src-tauri/`** — the Tauri app (Claude Code owns). Depends on `voxable-core`.
  - **DONE:** `config.rs` re-exports `voxable_core::config`; `whisper.rs` uses `voxable_core::vad::trim_silence`; `app_context.rs` (Win32 foreground app, cfg-gated); `llm.rs` cleanup now takes dictionary + foreground app and uses `voxable_core::prompt`; `main.rs` has the ~20 V3 commands + two-window setup + tray + hotkey + right-click context menu; `AppState` carries `last_raw_text`/`last_duration_ms`/`last_result` + core `History`. Pipeline: transcribe → snippet-expand → LLM cleanup → history write (real audio+duration) → emit `dictation-complete`.
  - **Frontend DONE:** `flowbar.{html,js,css}` (transparent pill, `data-tauri-drag-region`, onMoved persistence, context menu) + tabbed Hub (`index.html`/`main.js`/`styles.css`, 5 tabs, history audio via Web Audio) + Vite multi-page (`vite.config.js`).

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

## Deferred (follow-up spike)
- `mic_device`: stored in Settings, not yet wired to cpal device selection
- `sound_enabled`: wired to the Hub's completion beep; not yet used elsewhere
- Push-to-talk: unreliable over global shortcut; hands-free (toggle) is the default
- Auto-paste is Windows-only (`SendInput`); Mac/Linux would need an equivalent (e.g. `enigo`)

## Plan
- **Spec:** `Docs/voxable/superpowers/specs/2026-09-07-voxable-v3-design.md`
- **Lean plan (active):** `Docs/voxable/superpowers/plans/2026-09-08-voxable-v3-implementation-lean.md`
  (11 tasks — **all done**: Tasks 1–3, 5 + VAD in `voxable-core` (25 tests passed, verified on Windows host); Tasks 4, 6–11 in `src-tauri` (Claude Code). Plus the two post-plan iterations above.)

## Build status
- Workspace: root `Cargo.toml` (members `src-tauri` + `voxable-core`; `[profile.release]` lives here, NOT in members).
- `voxable-core` — **VERIFIED**, `cargo test -p voxable-core` = 25 passed.
- `src-tauri` — **V3 complete**; `cargo check -p voxable` clean; **full CPU release build OK → `target/release/bundle/{msi,nsis}/` installers + `voxable.exe`** (launches, both windows, no crash).
- Toolchain on the Windows host: Rust 1.98 (MSVC), VS 2022 Build Tools, CMake (`C:\Program Files\CMake\bin`), LLVM (`LIBCLANG_PATH=C:\Program Files\LLVM\bin`), CUDA 13.3. **cargo + cmake must both be on the shell PATH for a release build** (see BUILD.md / LEARNINGS).
- **Next step:** manual QA (checklist at top), then CUDA build (`npm run tauri build -- --features cuda`, env `CUDA_PATH`+`CUDA_PATH_V13_3`+`CUDAARCHS=native`) and Mac/Metal build on a Mac. Signing still deferred. Full build recipe in `BUILD.md`.

## Known limitations
- No GPU acceleration by default (CUDA/Metal features available in Cargo.toml)
- Audio resampler is linear interpolation (good for speech, not music)
- No auto-start on boot (user must launch app)
- Win32 app detection is Windows-only (returns None on Linux/macOS)
- Auto-paste is Windows-only (`SendInput`); Mac/Linux would need an equivalent (e.g. `enigo`)
