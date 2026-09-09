# Voxable — Project State

**Last updated:** 2026-09-09
**Status:** **V3 code-complete, built, + focus/paste + light redesign.** All 11 lean-plan tasks done, plus two post-plan iterations from Dave: (1) **focus-preserving auto-paste** — Flow Bar is now non-activating (`WS_EX_NOACTIVATE`, no `set_focus`) so it never steals focus, and dictation auto-pastes into the active field via synthesized Ctrl+V (`paste_to_active` cmd + `app_context::send_ctrl_v`); (2) **light/modern Wispr-style redesign** of both Flow Bar (light pill, gradient mic, animated waveform, SVG icons, window bumped to 340×96 for shadow room) and Hub (light theme, toggle switches, gradient accents). `voxable-core` **VERIFIED (25 passed)**. `src-tauri` `cargo check` clean; **both CPU + CUDA release installers build** → named `Voxable_0.1.0_x64_cpu-*` / `_cuda-*` in `target/release/bundle/{nsis,msi}/`. Packaged app launches (both windows, no crash). Design verified visually in the browser (dev server).
**Post-ship fixes (Dave testing):** (a) added `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` — release exe was spawning a console terminal showing log output (the "floating terminal always on top"); (b) CUDA runtime DLLs: CUDA 13 puts `cublas64_13.dll`/`cublasLt64_13.dll`/`cudart64_13.dll` in `…\CUDA\v13.3\bin\x64\` (not on PATH) → "cublas64_13.dll not found" at startup. Fix: **shareable CUDA installer** now bundles the 3 DLLs next to the exe (Tauri resource map → exe dir) AND builds multi-arch (`CUDAARCHS="75;86;89;120"`, 20xx→50xx) → runs on any Windows NVIDIA machine, no toolkit/PATH. Verified: exe launches with CUDA removed from PATH, no error. CUDA installer ~400MB (cublasLt is 463MB); CPU installer stays ~3MB. Build scripts: `scripts/build-installers.ps1`, `scripts/build-cuda-shareable.ps1`; CUDA merge config `src-tauri/tauri.cuda.conf.json` (+ staged DLLs in `src-tauri/cuda-runtime/`, gitignored).
**▶ RESUME HERE:** **Manual QA only** — run the app and verify the runtime behaviors that can't be auto-tested (see "Manual QA checklist" below): mic capture → dictation, global hotkey toggle, Flow Bar drag/persist, right-click context menu, settings-save-preserves-dictionary, history audio playback. Then: wire the deferred `mic_device`/`sound_enabled` follow-up spike, and build the CUDA (this machine) + Mac (Metal) variants when needed.

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
  (11 tasks; Tasks 1–3, 5 + VAD done in `voxable-core` — unverified; Tasks 4, 6–11 are Claude Code's)

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
- `mic_device` and `sound_enabled` stored but not wired (follow-up spike)
