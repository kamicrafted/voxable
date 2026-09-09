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
xcode-select --install                 # Xcode Command Line Tools (clang, etc.)
brew install rustup-init cmake llvm node
rustup-init -y                          # then restart the shell
```

Clone and build:

```bash
git clone <your-repo-url> voxable
cd voxable
export LIBCLANG_PATH=$(brew --prefix llvm)/lib   # whisper-rs bindgen needs libclang
npm install
npm run tauri build -- --features metal          # GPU (Apple Silicon); omit --features for CPU
```

Output: `src-tauri/target/release/bundle/` (`.dmg` + `.app`). Metal-linked builds
run only on macOS; the CPU build (`npm run tauri build`) is portable. Code signing
/ notarization is not set up — for personal use, right-click → Open the first time,
or `xattr -dr com.apple.quarantine <app>`.

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
