//! Pure, host-independent logic for Voxable — no Tauri, whisper, cpal, or OS deps.
//!
//! This crate compiles and tests with **only rustup** (no C toolchain), so omp
//! owns the logic tasks here and verifies them with `cargo test -p voxable-core`.
//! The Tauri app (`src-tauri`) depends on this crate for these types/functions.
//!
//! Task ownership (see `Docs/voxable/superpowers/plans/2026-09-08-voxable-v3-implementation-lean.md`):
//! - Task 1 → `config`   (extend `Settings`, add `DictEntry`/`Snippet`/`FlowbarPosition`)
//! - Task 2 → `snippets` (`expand_snippets`)
//! - Task 3 → `prompt`   (`build_dictionary_prompt`, `get_cleanup_prompt`)
//! - Task 5 → `history`  (`History`, `HistoryEntry`)
//! - VAD    → `vad`      (`trim_silence`)

pub mod config;
pub mod snippets;
pub mod stats;
pub mod prompt;
pub mod screen;
pub mod history;
pub mod hotkey;
pub mod vad;
