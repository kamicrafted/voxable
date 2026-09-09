# LEARNINGS

Durable, de-duplicated lessons for Voxable. Keep lean (re-read every session). Research/design
detail lives in `Docs/voxable/` (e.g. `wispr-flow-ui-research.md`, `design.md`) — link, don't inline.

## Tauri v2 API gotchas
- **Events:** `app.emit(event, payload)` broadcasts to all windows; `app.emit_to(label, …)` targets one. **`emit_all` is v1 — does not exist in v2.**
- **Global shortcut:** `on_shortcut(shortcut, |app, sc, event| { … })` — NOT `register(..)`. Handler returns `()` (no `?`, no `Ok(())`); gate on `event.state == ShortcutState::Pressed`.
- **Registration must be non-fatal:** with `panic = "abort"` (release), returning `Err` from `setup` hard-crashes at startup. Log a warning and continue. (Real crash when 1Password owned `Ctrl+Shift+Space`.)
- **Capabilities must list the actual window labels** (`capabilities/default.json` `"windows": [...]`). Renaming/adding windows without updating it blocks all core/plugin IPC at runtime.
- **Tray:** keep the handle with `app.manage(tray)` — there is no `app.tray_icon(tray)` setter. Menu items need an `on_menu_event` handler or they do nothing.
- **Clipboard:** `clipboard().write_text(..)` (not `set_text`); plugin must be in `Cargo.toml` + registered (`tauri_plugin_clipboard_manager`).
- **Command names = the Rust fn name;** frontend `invoke("<name>")` must match exactly, passing every non-`State`/`AppHandle` arg by name.
- `is_visible()` returns `Result<bool>`; `PredefinedMenuItem::quit(app, Some("Quit"))` takes `Option<&str>`.
- **Multi-window:** define windows in `tauri.conf.json` `app.windows[]` (each with its own `url`, `visible:false` to start hidden); Rust gets a window via `app.get_webview_window("<label>")`, JS via `getCurrentWindow()`; shared `State<T>`.
- **JS DPI:** `PhysicalPosition` imports from `@tauri-apps/api/dpi`; `set_position()` takes a `PhysicalPosition`, not `{x,y}`.
- **Vite multi-page:** add `build.rollupOptions.input` with both HTML entry points. In an ESM `vite.config.js` (`"type":"module"`) there is no `__dirname` — resolve entries with `fileURLToPath(new URL("./index.html", import.meta.url))`.
- **`get_window`/`get_webview_window`:** `Manager::get_window` (returns `Window`) is behind the **`unstable`** cargo feature — do NOT use it. `get_webview_window` (returns `WebviewWindow`) is always available; `WebviewWindow` has its own `.popup_menu(&menu)` / `.set_position()` / `.primary_monitor()`, so you never need the bare `Window`.
- **Context menus:** build a `Menu`, call `webview_window.popup_menu(&menu)`. ALL muda menu events (tray items AND popup context-menu items) route to the single global handler registered via `Builder::on_menu_event(|app, event| …)` — match on `event.id()`. Don't also set the tray's own `on_menu_event` or items double-fire; give every item a distinct id string (ids are independent of command names). `PredefinedMenuItem::quit` self-handles (no event). A `separator` can't be reused in one menu — make one per slot.
- **Flow Bar dragging:** `-webkit-app-region: drag` does NOT work in WebView2 (Windows). Use the `data-tauri-drag-region` attribute on the drag surface + `"core:window:allow-start-dragging"` capability. Persist position by listening to `getCurrentWindow().onMoved` (debounced) → `set_flowbar_position` with the **physical** `payload.{x,y}`; restore with `PhysicalPosition::new(x,y)` (no scale-factor math needed when you store physical both ways).
- **Transparent Flow Bar:** window `"transparent": true` + `"decorations": false`; paint the pill background in CSS (`html,body { background: transparent }`). The window must be LARGER than the visible pill (inset the pill with `position:absolute; inset:18px 22px`) or the pill's box-shadow gets clipped at the window edge.
- **Focus-preserving dictation (the whole point of the pill):** the Flow Bar must NOT take focus, or it steals it from the user's text field. (1) In the hotkey handler / show paths, `show()` but never `set_focus()` on the Flow Bar. (2) On Windows, add `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` to its HWND via `GetWindowLongPtrW`/`SetWindowLongPtrW(GWL_EXSTYLE)` so even a mouse click on it won't move focus. Get the HWND with `webview_window.hwnd()` (cfg(windows)-only) and pass `h.0 as isize` across the fn boundary (avoids `windows`-crate version-identity issues). Then **auto-paste** = `copy_to_clipboard` → `SendInput` Ctrl+V (VK_CONTROL/VK_V down+up) into the still-focused field. Needs windows feature `Win32_UI_Input_KeyboardAndMouse`; a ~60 ms JS delay after the clipboard write before pasting is enough.
- **`[hidden]` vs `display`:** an id rule like `#copy { display: grid }` OVERRIDES the UA `[hidden]{display:none}` (higher specificity), so `el.hidden = true` won't hide it. Add an explicit `[hidden]{display:none!important}` to any CSS that sets `display` on elements you also toggle via the `hidden` attribute.

## Build environment (Windows host)
- **`cmake` and `cargo` must be on the shell PATH for a release build.** `cargo check` can pass while `npm run tauri build` fails at `whisper-rs-sys` with "is `cmake` not installed?" — because `check` reuses the cached *debug* whisper.cpp, but the *release* profile recompiles whisper.cpp and needs cmake. Add both before building:
  `$env:PATH = "C:\Users\hello\.cargo\bin;C:\Program Files\CMake\bin;$env:PATH"` (and `LIBCLANG_PATH=C:\Program Files\LLVM\bin`). cmake lives at `C:\Program Files\CMake\bin`.

## whisper-rs 0.16
- `WhisperContextParameters` builder returns `&mut` — build as `let mut p = …::new(); p.use_gpu(x);` (don't chain).
- `WhisperContext::new_with_params(path, params)` → `ctx.create_state()` → `state.full(params, &audio)` → iterate `state.as_iter()`, `segment.to_str()`.
- `full()` borrows `audio` while `state` borrows `ctx`; clone audio before passing (≤~1.9 MB for 2 min @16 kHz).
- **Decoding:** greedy (`SamplingStrategy::Greedy`) is much faster than beam search on CPU and fine for dictation — beam=5 was the cause of slow transcription.

## cpal 0.15
- `cpal::default_host()` → `Host` (a value, NOT a `Result`). `default_input_device()` → `Option`; `default_input_config()` → `Result`.
- `build_input_stream(&config, cb: Fn(&[f32], &InputCallbackInfo), err_fn, None)` → `Result<Stream>`. **Must call `stream.play()`** or no audio flows.
- **`cpal::Stream` is `!Send`** → cannot live in Tauri managed `State`. Own it on a dedicated thread, drive via `mpsc`; keep only `Send+Sync` handles (Sender in a Mutex for `Sync`, `Arc<Mutex<Vec<f32>>>`, atomics).
- **Never hold a mutex guard across `.await`** — deadlocks and makes the future non-`Send`.

## App/pipeline behavior
- **stop_recording race safety:** clearing the `recording` atomic *before* draining the buffer is what makes it safe (the audio callback early-returns once the flag is false). A `wait_idle()` that polls the `recording` flag is a **no-op** (stop() already set it false) — don't rely on it; the flag-clear is the real guard.
- **VAD:** simple energy-based `trim_silence()` (>0.01 RMS, 30 ms min) before `full()` saves CPU and avoids silence hallucinations. Not a real VAD model — fine for clear dictation.
- **LLM cleanup degrades gracefully:** on any LLM error, log + return the raw Whisper text. An optional enhancement stage must never fail the whole pipeline.

## GUI / console
- **`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` at the top of `main.rs` is REQUIRED**, or the release exe is a console-subsystem app and Windows attaches a terminal window that shows all `log::info!`/stderr output ("floating terminal showing whisper model load…"). Debug keeps the console so dev logs are visible. (Was missing through V2/V3 until Dave flagged the stray terminal.)

## Build & GPU
- **CUDA 13 runtime DLLs live in `…\CUDA\v13.3\bin\x64\`, NOT `…\bin\`** — and the CUDA installer only adds `bin` to PATH. So even on a machine WITH the toolkit, a CUDA-linked exe fails at startup with "cublas64_13.dll was not found" (load-time import, so runtime PATH fixes are too late). The CUDA build needs these three next to `voxable.exe` (or `bin\x64` on PATH): `cublas64_13.dll` (~53MB), `cublasLt64_13.dll` (~463MB!), `cudart64_13.dll` (~0.5MB). Install dir is `%LOCALAPPDATA%\Voxable\`.
- **Self-contained CUDA installer = bundle those DLLs via Tauri resources.** On Windows Tauri's `resource_dir()` IS the exe dir (`tauri-utils::platform::resource_dir_from`), so bundled resources land next to the exe where the loader finds them. Use the MAP form to flatten to the exe dir (array form keeps the subfolder → not found): in a CUDA-only merge config `tauri.cuda.conf.json`, `"bundle":{"resources":{"cuda-runtime/cublas64_13.dll":"cublas64_13.dll", …}}`, built with `tauri build --features cuda --config src-tauri/tauri.cuda.conf.json`. Result runs on any machine with just an NVIDIA driver (no toolkit/PATH). Verified by launching the built exe with CUDA removed from PATH — no error dialog. Cost: installer ~400MB (cublasLt).
- **Shareable CUDA = multi-arch, not `CUDAARCHS=native`.** `native` builds SASS only for the build machine's GPU (5090 = sm_120), so it won't run on a 40-series etc. Set `CUDAARCHS="75;86;89;120"` (Turing/Ampere/Ada/Blackwell = 20xx→50xx). CUDA 13 dropped Maxwell/Pascal/Volta (sm_50–70) but keeps sm_75+. Scripts: `scripts/build-installers.ps1` (both, this-machine CUDA) and `scripts/build-cuda-shareable.ps1` (multi-arch + bundled DLLs).
- **GPU is a cargo feature** (same source, per-platform): `[features] cuda = ["whisper-rs/cuda"]`, `metal = ["whisper-rs/metal"]`; `use_gpu(cfg!(any(feature="cuda",feature="metal")))`. CPU = omit the flag.
- **Windows CUDA build:** `npm run tauri build -- --features cuda`; env needs `CUDA_PATH` **and** `CUDA_PATH_V13_3` (MSBuild reads the versioned one), `LIBCLANG_PATH`, `CUDAARCHS=native` (5090 = Blackwell sm_120).
- **CUDA build is NOT portable:** links `cublas64_13.dll`/cudart → runs only where the CUDA toolkit is installed + an NVIDIA GPU. Ship the **CPU build** to others; CUDA is for this machine only.

## Container build environment
- **The omp container has rustc/cargo but NO libc dev files** (`Scrt1.o`, `crti.o`, `libc.so`, `libm.so` missing; no `sudo`, uid 1000). `cargo check`/`cargo test` fail at the **link** stage of build scripts (proc-macro2, quote) — type-checking of the crate itself is never reached. Fix = rebuild the omp image with `build-essential`/`libc6-dev` (the handoff's "Dave runs once" prereq). Until then, Rust work in-container is **unverified** — hand off for compile+test on a machine with a C toolchain.
- **whisper-rs needs a C/C++ toolchain AND libclang** (bindgen). Can't build in the non-root omp container → build on Windows (VS Build Tools + LLVM + CMake).

## Process lessons (from the V3 review)
- **`#[serde(default)]` cuts both ways:** great for migrations, but a "save settings" that sends only a *subset* silently resets omitted fields to defaults → data loss. Send the full object or merge server-side.
- **In-memory mutation ≠ persistence:** CRUD commands that edit `state.settings` must also write to disk.
- **Trace data flow per feature:** history "Play audio" was fully wired in the UI but the backend saved empty audio + `duration_ms:0` — a dead flagship feature. One data-flow line per feature catches these gaps.
- **Push-to-talk over a global shortcut is unreliable** (release events for a modifier chord). Default to hands-free (toggle); spike push-to-talk before committing.
