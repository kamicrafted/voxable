use crate::config::{model_info, model_path, Settings};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

/// Ensure the model file exists on disk, downloading it if necessary.
/// Standalone (not a method) so it can be awaited without holding the engine lock.
pub async fn ensure_model(model_name: &str) -> Result<PathBuf, String> {
    let path = model_path(model_name);
    if path.exists() {
        return Ok(path);
    }

    let (file, size) = model_info(model_name);
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        file
    );

    log::info!(
        "Downloading whisper model '{}' (~{}) to {}",
        model_name,
        size,
        path.display()
    );

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = path.with_extension("downloading");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Model download returned HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read model bytes: {}", e))?;

    std::fs::write(&tmp_path, &bytes)
        .map_err(|e| format!("Failed to write model file: {}", e))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to finalize model file: {}", e))?;

    log::info!("Model '{}' downloaded to {}", model_name, path.display());
    Ok(path)
}

pub struct WhisperEngine {
    ctx: Option<WhisperContext>,
    model_name: Option<String>,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            ctx: None,
            model_name: None,
        }
    }

    /// Load the whisper context for the given model, loading from disk.
    pub fn load(&mut self, model_name: &str, path: &PathBuf) -> Result<(), String> {
        if self.model_name.as_deref() == Some(model_name) && self.ctx.is_some() {
            return Ok(());
        }

        // Use the GPU when a backend was compiled in (cuda/metal features);
        // harmless on CPU-only builds where there is no GPU backend to use.
        let use_gpu = cfg!(any(feature = "cuda", feature = "metal"));
        log::info!(
            "Loading whisper model '{}' from {} (gpu: {})",
            model_name,
            path.display(),
            use_gpu
        );
        let mut params = WhisperContextParameters::new();
        params.use_gpu(use_gpu);
        let path_str = path
            .to_str()
            .ok_or("Model path is not valid UTF-8")?;
        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| format!("Failed to load whisper model: {}", e))?;

        self.ctx = Some(ctx);
        self.model_name = Some(model_name.to_string());
        Ok(())
    }

    /// Transcribe raw f32 PCM audio (16kHz mono) to text.
    /// `language` is a BCP-47 code ("en", "es", etc.) or "auto" for detection.
    pub fn transcribe(&mut self, audio: &[f32], language: &str) -> Result<String, String> {
        let ctx = self
            .ctx
            .as_ref()
            .ok_or("Whisper model not loaded")?;

        // VAD: trim leading/trailing silence to speed up transcription
        // (implementation lives in voxable-core, unit-tested there).
        let trimmed = voxable_core::vad::trim_silence(audio, 0.01, 30);
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        // Greedy decoding is far faster than beam search on CPU and is plenty
        // accurate for clear dictation. (Beam search was ~5x slower here.)
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(std::cmp::max(
            1,
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4) as i32,
        ));
        if language != "auto" {
            params.set_language(Some(language));
        }
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);

        let mut state = ctx.create_state().map_err(|e| e.to_string())?;
        let audio_clone = trimmed.to_vec();
        state
            .full(params, &audio_clone)
            .map_err(|e| format!("Transcription failed: {}", e))?;

        let mut text = String::new();
        for segment in state.as_iter() {
            if let Ok(s) = segment.to_str() {
                text.push_str(s);
                text.push(' ');
            }
        }

        Ok(text.trim().to_string())
    }
}

/// Global engine state shared across commands.
pub struct AppState {
    pub whisper: Arc<Mutex<WhisperEngine>>,
    pub settings: Arc<Mutex<Settings>>,
    /// Most recently captured audio (16 kHz mono f32), set by `stop_recording`.
    pub last_audio: Arc<Mutex<Vec<f32>>>,
    /// Raw Whisper transcript of `last_audio`, set by `transcribe`.
    pub last_raw_text: Arc<Mutex<Option<String>>>,
    /// Duration of `last_audio` in ms, set by `stop_recording`.
    pub last_duration_ms: Arc<Mutex<u32>>,
    /// Final cleaned text of the last dictation, set by `cleanup`.
    pub last_result: Arc<Mutex<Option<String>>>,
    /// Persistent dictation history (JSON index + `.f32` audio on disk).
    pub history: Arc<Mutex<voxable_core::history::History>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let history = voxable_core::history::History::new(&voxable_core::config::history_dir());
        Self {
            whisper: Arc::new(Mutex::new(WhisperEngine::new())),
            settings: Arc::new(Mutex::new(settings)),
            last_audio: Arc::new(Mutex::new(Vec::new())),
            last_raw_text: Arc::new(Mutex::new(None)),
            last_duration_ms: Arc::new(Mutex::new(0)),
            last_result: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(history)),
        }
    }
}
