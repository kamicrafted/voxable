use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

const TARGET_RATE: u32 = 16000;
const MAX_DURATION_MS: usize = 120_000; // 2 minute cap

/// Commands sent to the dedicated audio thread.
enum AudioCmd {
    Start,
    Stop,
}

/// A recorder whose actual `cpal::Stream` lives on a dedicated thread.
///
/// `cpal::Stream` is `!Send`, so it can never be stored in Tauri's shared
/// state or moved across threads. Instead we own it entirely on one thread
/// and drive it via a channel; only `Send + Sync` handles are kept here.
pub struct Recorder {
    // `mpsc::Sender` is not guaranteed `Sync` across Rust versions; Tauri's
    // managed `State<T>` requires `Sync`, so guard it with a mutex.
    tx: Mutex<Sender<AudioCmd>>,
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
    sample_count: Arc<AtomicUsize>,
    /// Set by the audio thread while a `cpal::Stream` exists, cleared once it has
    /// been dropped. Written only there, so it reports what actually happened
    /// rather than what was requested.
    stream_active: Arc<AtomicBool>,
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCmd>();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let recording = Arc::new(AtomicBool::new(false));
        let sample_count = Arc::new(AtomicUsize::new(0));
        let stream_active = Arc::new(AtomicBool::new(false));

        {
            let samples = samples.clone();
            let recording = recording.clone();
            let sample_count = sample_count.clone();
            let stream_active = stream_active.clone();
            std::thread::spawn(move || {
                audio_thread(rx, samples, recording, sample_count, stream_active)
            });
        }

        Self {
            tx: Mutex::new(tx),
            samples,
            recording,
            sample_count,
            stream_active,
        }
    }

    pub fn start(&self) -> Result<(), String> {
        if self.recording.load(Ordering::SeqCst) {
            return Err("Already recording".into());
        }
        // Never build a stream while the previous one is still installed. Doing so
        // let the pending teardown land on the new stream, which then delivered
        // silence for the rest of the recording — audible as a dictation that
        // captured the right duration but only transcribed its first moment.
        if !self.wait_for_stream(false) {
            log::warn!("previous audio stream did not shut down; starting anyway");
        }
        self.samples.lock().clear();
        self.sample_count.store(0, Ordering::SeqCst);
        self.recording.store(true, Ordering::SeqCst);
        self.tx
            .lock()
            .send(AudioCmd::Start)
            .map_err(|e| format!("Audio thread unavailable: {}", e))
    }

    pub fn stop(&self) -> Vec<f32> {
        self.recording.store(false, Ordering::SeqCst);
        let _ = self.tx.lock().send(AudioCmd::Stop);
        // Wait for the audio thread to actually drop the stream so no more samples
        // can arrive, then drain the buffer.
        if !self.wait_for_stream(false) {
            log::warn!("audio stream still active 500ms after Stop");
        }
        std::mem::take(&mut *self.samples.lock())
    }

    /// Block until the audio thread reports the stream in state `want`.
    ///
    /// Returns false on timeout. The flag is written only by the audio thread —
    /// waiting on a flag this side sets itself is what made the previous version a
    /// no-op that returned immediately every time.
    fn wait_for_stream(&self, want: bool) -> bool {
        for _ in 0..100 {
            if self.stream_active.load(Ordering::SeqCst) == want {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    pub fn duration_ms(&self) -> u32 {
        let count = self.sample_count.load(Ordering::SeqCst);
        (count as u32) * 1000 / TARGET_RATE
    }
}

/// The dedicated audio thread: owns the `cpal::Stream` for its whole lifetime.
fn audio_thread(
    rx: Receiver<AudioCmd>,
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
    sample_count: Arc<AtomicUsize>,
    stream_active: Arc<AtomicBool>,
) {
    let mut stream: Option<cpal::Stream> = None;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AudioCmd::Start => {
                // A stream left over from a previous recording must go first, or
                // two streams share the device.
                if stream.take().is_some() {
                    log::warn!("dropping a leftover audio stream before starting");
                }
                match build_stream(&samples, &recording, &sample_count) {
                    Ok(s) => {
                        if let Err(e) = s.play() {
                            log::error!("Failed to start audio stream: {}", e);
                            recording.store(false, Ordering::SeqCst);
                        } else {
                            stream = Some(s);
                            stream_active.store(true, Ordering::SeqCst);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to build audio stream: {}", e);
                        recording.store(false, Ordering::SeqCst);
                    }
                }
            }
            AudioCmd::Stop => {
                // Dropping the stream stops capture. Drop before clearing the flag
                // so nobody can start a new stream while this one is still closing.
                stream = None;
                stream_active.store(false, Ordering::SeqCst);
            }
        }
    }
}

fn build_stream(
    samples: &Arc<Mutex<Vec<f32>>>,
    recording: &Arc<AtomicBool>,
    sample_count: &Arc<AtomicUsize>,
) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input audio device found")?;

    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;

    let source_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    log::info!(
        "Recording from '{}' at {}Hz, {}ch (target {}Hz)",
        device.name().unwrap_or_else(|_| "unknown".into()),
        source_rate,
        channels,
        TARGET_RATE
    );

    let samples = samples.clone();
    let recording = recording.clone();
    let sample_count = sample_count.clone();

    let err_fn = |err| log::error!("Audio stream error: {}", err);

    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _ts: &cpal::InputCallbackInfo| {
                if !recording.load(Ordering::SeqCst) {
                    return;
                }

                // Downmix to mono if needed.
                let mono: Vec<f32> = if channels == 1 {
                    data.to_vec()
                } else {
                    data.iter().step_by(channels).copied().collect()
                };

                // Resample to the target rate if needed.
                let chunk = if source_rate != TARGET_RATE {
                    resample_linear(&mono, source_rate, TARGET_RATE)
                } else {
                    mono
                };

                let count = {
                    let mut buf = samples.lock();
                    buf.extend_from_slice(&chunk);
                    buf.len()
                };
                sample_count.store(count, Ordering::SeqCst);

                // Auto-stop at max duration (measured in target-rate samples).
                let max_samples = MAX_DURATION_MS * (TARGET_RATE as usize) / 1000;
                if count >= max_samples {
                    recording.store(false, Ordering::SeqCst);
                }
            },
            err_fn,
            Some(Duration::from_secs(2)),
        )
        .map_err(|e| format!("Failed to build audio stream: {}", e))?;

    Ok(stream)
}

/// Simple linear interpolation resampler. Good enough for speech (not music).
fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = from_rate as f32 / to_rate as f32;
    let out_len = ((input.len() as f32) / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let pos = (i as f32) * ratio;
        let idx = pos as usize;
        let frac = pos - idx as f32;

        let s0 = input.get(idx).copied().unwrap_or(0.0);
        let s1 = input.get(idx + 1).copied().unwrap_or(0.0);
        output.push(s0 + (s1 - s0) * frac);
    }

    output
}
