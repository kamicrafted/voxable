# Building Voxable

Voxable is a Tauri v2 app: a Rust backend (`src-tauri`) + a pure-logic crate
(`voxable-core`) + a vanilla-JS multi-page frontend (Vite). It ships two windows
in one process — a **Flow Bar** (`flowbar.html`) and a **Hub** (`index.html`).

## Prerequisites (Windows)

- **Rust** (MSVC toolchain) — `rustup default stable-msvc`
- **Visual Studio 2022 Build Tools** (C++ workload) — MSVC linker for whisper.cpp
- **CMake** — builds the bundled whisper.cpp
- **LLVM / Clang** — `whisper-rs` uses `bindgen`; set `LIBCLANG_PATH`
- **Node 18+** and `npm`

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

## Quick start

```powershell
npm install
npm run tauri dev      # run with hot-reload (both windows)
```

## Production build (CPU — portable, ship this to others)

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
npm install
npm run tauri build
```

Output: `src-tauri/target/release/bundle/` (MSI + NSIS installers, plus the
standalone `voxable.exe`). The CPU build runs anywhere — no GPU required.

## Both installers at once (named cpu / cuda)

`scripts/build-installers.ps1` builds both variants back-to-back and names them
`Voxable_0.1.0_x64_cpu-*` (portable) and `Voxable_0.1.0_x64_cuda-*` (this machine),
removing the ambiguous generic name:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-installers.ps1
```

> Switching between CPU and CUDA forces `whisper-rs-sys` (whisper.cpp) to recompile
> each time, so the CUDA half is the slow one (~10–15 min).

### Shareable CUDA installer (multi-arch + self-contained, ~400MB)

`scripts/build-cuda-shareable.ps1` builds a CUDA installer that runs on **any**
Windows machine with an NVIDIA driver — no CUDA toolkit or PATH setup needed:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-cuda-shareable.ps1
```

It (1) compiles for archs `75;86;89;120` (Turing→Blackwell, i.e. 20xx–50xx) and
(2) bundles the CUDA runtime DLLs next to the exe via `src-tauri/tauri.cuda.conf.json`
(the DLLs must be staged in `src-tauri/cuda-runtime/` first — copy them from
`…\CUDA\v13.3\bin\x64\`). The plain per-machine CUDA build (`build-installers.ps1`)
uses `CUDAARCHS=native` and is not self-contained.

## GPU build (CUDA — this machine only, NOT portable)

Links `cublas64_13.dll` / cudart, so it only runs where the CUDA toolkit + an
NVIDIA GPU are present. Do not distribute it.

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CUDA_PATH     = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3"
$env:CUDA_PATH_V13_3 = $env:CUDA_PATH   # MSBuild reads the versioned var
$env:CUDAARCHS    = "native"            # RTX 5090 = Blackwell sm_120
npm run tauri build -- --features cuda
```

## macOS (Metal) — must be built on a Mac

Prerequisites (one time):

```bash
xcode-select --install     # or a full Xcode install — either provides clang + libclang
brew install cmake node    # cmake builds whisper.cpp
# Rust: rustup (https://rustup.rs), then restart the shell
```

`brew install llvm` is not needed — Xcode already ships a `libclang.dylib`, and
`scripts/build-mac.sh` points `LIBCLANG_PATH` at it.

Clone and build:

```bash
git clone <your-repo-url> voxable
cd voxable
./scripts/build-mac.sh          # Metal (Apple Silicon); pass --cpu for a CPU build
```

Output: `target/release/bundle/` — `macos/Voxable.app` and
`dmg/Voxable_<version>_aarch64.dmg` (drag-to-Applications). Metal-linked builds run only on macOS; the CPU
build is portable. Code signing / notarization is not set up — for personal use,
right-click → Open the first time, or `xattr -dr com.apple.quarantine <app>`.

### Permissions and code signing (local development)

macOS attaches an Accessibility or Microphone grant to the app's **code signature**.
Tauri signs ad-hoc, and an ad-hoc signature changes with every build, so each rebuild
looks like a new app and the permission you granted stops applying — while often still
showing as switched on in System Settings, which makes it look like the app is broken.

Fix it once:

```bash
./scripts/create-signing-identity.sh          # self-signed "Voxable Dev" identity
tccutil reset Accessibility com.voxable.app   # clear grants tied to old signatures
tccutil reset Microphone com.voxable.app
./scripts/build-mac.sh                        # signs with the identity when present
```

`build-mac.sh` re-signs with that identity automatically and says so; without it, it
prints a warning instead. The identity is for local development only — it is not a
Developer ID and does nothing for Gatekeeper on anyone else's machine.

### Three macOS build fixes (already in the repo)

- **Deployment target.** ggml uses `std::filesystem`, which libc++ marks unavailable
  before macOS 10.15, and the `cc` crate defaults Apple builds to 10.13 →
  `'path' is unavailable: introduced in macOS 10.15`. `.cargo/config.toml` pins
  `MACOSX_DEPLOYMENT_TARGET = "11.0"`. Setting it as a shell variable is not reliable:
  it does not survive `npm run tauri build`, and CMake caches the old value in
  `target/release/build/whisper-rs-sys-*/out/build/CMakeCache.txt` — delete that
  directory if you ever see the 10.15 error again.
- **compiler-rt.** whisper.cpp's Metal backend uses Objective-C `@available`, which
  clang lowers to `__isPlatformVersionAtLeast` in compiler-rt. rustc does not link
  compiler-rt, so the link fails with an undefined symbol. `src-tauri/build.rs` adds
  `libclang_rt.osx.a` (found via `xcrun clang -print-runtime-dir`) on macOS only.
- **DMG bundling.** Tauri's `bundle_dmg.sh` does not work on a Kandji-managed Mac. It
  first runs an AppleScript that asks Finder to lay out the disk-image window (times out
  as `AppleEvent timed out (-1712)` from a non-GUI shell; `CI=1` makes Tauri pass
  `--skip-jenkins` to skip it), and then calls `hdiutil convert`, which fails with
  `Resource temporarily unavailable` (errno 35 from `CUDIFFileAccess::updateHeader`) —
  reproducible on a 10 MB throwaway image with no project involved. So
  `scripts/build-mac.sh` builds `--bundles app` and makes the disk image itself with
  `hdiutil create -srcfolder … -format UDZO`, which compresses fine and skips `convert`
  entirely. Result is a standard drag-to-Applications DMG.

## Tests

```powershell
cargo test -p voxable-core     # pure logic (config, snippets, prompt, history, vad) — 25 tests
cargo check -p voxable         # type-check the Tauri app
npm run build                  # produces dist/index.html + dist/flowbar.html
```

## Architecture notes

- **Rust is the single source of truth.** Both windows `invoke` the same
  `#[tauri::command]` handlers; the global shortcut is the sole dictation trigger
  and emits `toggle-recording` to the Flow Bar.
- **Config profiles live in the workspace-root `Cargo.toml`** (`[profile.release]`),
  not in member crates — member profiles are ignored in a workspace.
- **GPU is a Cargo feature** (`cuda` / `metal`); the default build is CPU.
- Whisper models download on first use to `%APPDATA%/../Roaming/voxable/models`
  (via `dirs::config_dir()`), history to `…/voxable/history`.
