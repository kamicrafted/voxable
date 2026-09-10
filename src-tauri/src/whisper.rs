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
        let t_load = std::time::Instant::now();
        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| format!("Failed to load whisper model: {}", e))?;
        log::info!(
            "model '{}' loaded in {}ms (first dictation pays this once)",
            model_name,
            t_load.elapsed().as_millis()
        );

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
        let t_start = std::time::Instant::now();
        let trimmed = voxable_core::vad::trim_silence(audio);
        if trimmed.is_empty() {
            return Ok(String::new());
        }
        // Whisper does not fail on a fragment, it invents words — a 0.1s clip once
        // produced a phrase that was never spoken. Better to report nothing.
        const MIN_SPEECH_SAMPLES: usize = 16_000 / 4; // 250 ms
        if trimmed.len() < MIN_SPEECH_SAMPLES {
            log::info!(
                "transcribe: only {}ms of speech; too short to transcribe reliably",
                trimmed.len() * 1000 / 16_000
            );
            return Ok(String::new());
        }
        let trim_ms = t_start.elapsed().as_millis();
        let speech_secs = trimmed.len() as f32 / 16_000.0;
        let captured_secs = audio.len() as f32 / 16_000.0;
        // The gap between these two is the whole question when words go missing:
        // captured is what the microphone actually delivered, speech is what is left
        // after leading/trailing silence is trimmed. A large gap means either a long
        // pause before or after speaking, or capture that stopped early — and
        // `trimmed_head/tail` says which end it came off.
        let head_secs = {
            let first_loud = audio.iter().position(|s| s.abs() > 0.01).unwrap_or(0);
            first_loud as f32 / 16_000.0
        };
        let tail_secs = captured_secs - head_secs - speech_secs;

        // Level per second of the captured audio. If the stream dies partway through,
        // this reads as real levels followed by zeros, which no amount of reasoning
        // about durations can show.
        let rms_trace: Vec<String> = audio
            .chunks(16_000)
            .map(|sec| {
                let sum: f32 = sec.iter().map(|s| s * s).sum();
                format!("{:.3}", (sum / sec.len().max(1) as f32).sqrt())
            })
            .collect();

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

        let t_state = std::time::Instant::now();
        let mut state = ctx.create_state().map_err(|e| e.to_string())?;
        let state_ms = t_state.elapsed().as_millis();

        let t_decode = std::time::Instant::now();
        let audio_clone = trimmed.to_vec();
        state
            .full(params, &audio_clone)
            .map_err(|e| format!("Transcription failed: {}", e))?;
        let decode_ms = t_decode.elapsed().as_millis();

        // Realtime factor: how many seconds of speech we decode per second of wall
        // clock. Anything under about 5x will feel like waiting.
        let rtf = if decode_ms > 0 {
            speech_secs / (decode_ms as f32 / 1000.0)
        } else {
            0.0
        };
        log::info!(
            "transcribe: captured={:.1}s speech={:.1}s (trimmed {:.1}s head, {:.1}s tail) \
             trim={}ms new_state={}ms decode={}ms ({:.1}x realtime)",
            captured_secs,
            speech_secs,
            head_secs,
            tail_secs.max(0.0),
            trim_ms,
            state_ms,
            decode_ms,
            rtf
        );
        log::info!("transcribe: level per second [{}]", rms_trace.join(" "));

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
