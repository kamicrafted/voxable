# Voxable

Local voice dictation with AI cleanup. A Wispr Flow alternative that runs entirely on your machine.

## How it works
1. **Record** - Global hotkey (`Win+Alt+Space`) or Flow Bar tap captures microphone audio
2. **Transcribe** - Local Whisper model (whisper.cpp via whisper-rs) converts speech to text
3. **Clean up** - Optional LLM pass (any OpenAI-compatible API) removes filler words, fixes punctuation and grammar, applies your dictionary corrections
4. **Expand** - Snippet triggers are replaced with full text (e.g. "brb" → "be right back")
5. **Paste** - Polished text is auto-pasted to clipboard (or shown in Flow Bar if auto-paste is off)

**Features:** Two-window UI (Flow Bar + Hub), VAD silence trimming, language auto-detect, custom cleanup prompts, dictionary word corrections, snippet expansion, 4-level cleanup pipeline (raw → whisper → LLM → final), history with audio playback (14-day retention), Win32 foreground app context, hands-free mode, sound feedback, auto-paste, LLM provider presets (OpenAI, Ollama, DeepSeek, LM Studio, Groq), draggable Flow Bar with position persistence.

## UI

### Flow Bar (always-on-top, bottom-center)
- Status dot: idle (gray), recording (red pulse), processing (yellow), done (green), error (red)
- Timer: shows recording duration
- Result text: last dictation result
- Copy button: copies result to clipboard
- Right-click: context menu (Paste Last, History, Settings, Hide for 1 hour, Show, Quit)
- Drag to reposition (position persists)

### Hub (control panel, opened via tray or right-click)
- **Dictation tab**: Last result with copy/paste buttons
- **Settings tab**: Whisper model, LLM config, hotkey, language, auto-paste, custom prompt, hands-free mode
- **Dictionary tab**: Add/remove custom word corrections
- **Snippets tab**: Add/remove trigger→expansion pairs
- **History tab**: Past dictations with timestamps, 4-level text, audio playback

## Building

### Prerequisites

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 18+
- C/C++ toolchain:
  - **Windows**: VS 2022 Build Tools (C++), CMake, LLVM/Clang (libclang)
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Linux**: GCC/Clang, CMake, libclang
- For GPU acceleration: CUDA Toolkit (NVIDIA) or Metal (Apple Silicon, built-in)

### Build

```bash
# Install frontend deps
npm install

# Dev mode (hot reload)
npm run tauri dev

# Release build (CPU-only)
npm run tauri build

# Release build with GPU acceleration
npm run tauri build -- --features cuda    # NVIDIA (Windows/Linux)
npm run tauri build -- --features metal   # Apple Silicon (macOS)
```

### Windows CUDA build

Requires these environment variables:
- `CUDA_PATH` — CUDA toolkit install path
- `CUDA_PATH_V13_3` — version-specific path (MSBuild CUDA integration)
- `LIBCLANG_PATH` — path to libclang
- `CUDAARCHS=native` — compile for local GPU (e.g. sm_120 for RTX 5090)

### First run

1. Launch Voxable — Flow Bar appears at bottom-center of screen
2. Open Settings (right-click Flow Bar → Settings, or tray icon → Settings)
3. Configure LLM (base URL + API key + model)
4. Press `Win+Alt+Space` to start dictation
5. First run downloads the Whisper model (~142 MB for "base")

## Configuration

Settings are stored in `%APPDATA%/voxable/settings.json` (Windows), `~/Library/Application Support/voxable/settings.json` (macOS), `~/.config/voxable/settings.json` (Linux).

| Setting | Default | Description |
|---------|---------|-------------|
| `whisper_model` | `base` | Model size: tiny, base, small, medium, large-v3 |
| `llm_base_url` | `https://api.openai.com/v1` | OpenAI-compatible API base URL |
| `llm_api_key` | *(empty)* | API key (empty = no LLM cleanup) |
| `llm_model` | `gpt-4o-mini` | Model name for cleanup |
| `hotkey` | `Win+Alt+Space` | Global push-to-talk hotkey |
| `language` | `en` | Whisper language (or `auto` to auto-detect) |
| `auto_paste` | `true` | Auto-copy to clipboard after cleanup |
| `custom_prompt` | *(empty)* | Custom LLM cleanup prompt (empty = default) |
| `hands_free` | `false` | Toggle recording on hotkey (vs push-to-talk) |
| `dictionary` | `[]` | Custom word corrections: `[{misspelling, correction}]` |
| `flowbar_position` | *(auto)* | Flow Bar position: `{x, y}` |

## Snippets

Stored in `snippets.json` in the app data directory. Format:
```json
[
  { "trigger": "brb", "expansion": "be right back" },
  { "trigger": "tyvm", "expansion": "thank you very much" }
]
```

Triggers are matched case-insensitively as whole words. Expansion replaces the trigger in the final text.

## Dictionary

Custom word corrections injected into the LLM system prompt. Format (in settings.json):
```json
{
  "dictionary": [
    { "misspelling": "recieve", "correction": "receive" },
    { "misspelling": "seperate", "correction": "separate" }
  ]
}
```

## Whisper models

| Name | Size | Notes |
|------|------|-------|
| `tiny` | 39MB | Fastest, lowest accuracy |
| `base` | 74MB | Default. Good speed/accuracy balance |
| `small` | 244MB | Decent balance |
| `medium` | 769MB | Good accuracy |
| `large-v3` | 1.5GB | Best accuracy, slowest (needs GPU) |

Models are downloaded from HuggingFace (`ggerganov/whisper.cpp`) on first use.

## Notes

- **Max recording**: 2 minutes per session (auto-stops)
- **Resampling**: Audio is resampled to 16kHz (Whisper's native rate) via linear interpolation
- **LLM is optional**: If no API key is set, you get raw Whisper text (still usable, just less polished)
- **GPU is optional**: CPU-only build works fine; GPU gives 5-10x faster transcription
- **History retention**: 14 days, auto-cleaned on app start
- **Win32 app context**: Windows-only; LLM prompt includes the focused app title for context
- **Code signing**: Not signed (personal/internal use). Expect SmartScreen prompt on Windows, right-click→Open on macOS
