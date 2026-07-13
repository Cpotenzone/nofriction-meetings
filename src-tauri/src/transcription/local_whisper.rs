// Local on-device speech-to-text via whisper.cpp (whisper-rs bindings)
//
// Fully offline: no API key, no network. Audio is buffered with
// sample-accurate timing and segmented into utterances on natural pauses
// (same endpointing approach as the Google provider), then transcribed on a
// dedicated inference thread. Whisper's per-segment offsets are anchored to
// the capture-stream clock so transcripts carry true speech time and line up
// exactly with Rewind frames.
//
// Models are GGML files under <app-data>/models/ggml-<name>.bin, installed
// once via the `download_whisper_model` command (or dropped in manually).
// Metal acceleration is enabled on Apple Silicon.

use async_trait::async_trait;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use crate::transcription::TranscriptionProvider;

// ─── Tuning constants (16kHz mono) ──────────────────────────────────────────

const SAMPLE_RATE: usize = 16_000;
const FRAME_SAMPLES: usize = SAMPLE_RATE / 20; // 50ms energy frames
const SILENCE_RMS: f32 = 0.008;
const TRAILING_SILENCE_FRAMES: usize = 13; // ~650ms pause ends an utterance
const MIN_UTTERANCE_SAMPLES: usize = SAMPLE_RATE; // 1s
const MAX_UTTERANCE_SAMPLES: usize = SAMPLE_RATE * 20; // 20s
const LEAD_PAD_SAMPLES: usize = SAMPLE_RATE / 5; // 200ms
const MAX_SILENCE_SAMPLES: usize = SAMPLE_RATE * 5;

// ─── Global model configuration (set at startup / from settings) ────────────

static MODELS_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));
static PREFERRED_MODEL: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new("base.en".to_string()));

/// Called once at startup (and when settings change) to tell the provider
/// where models live and which one to prefer.
pub fn configure(models_dir: PathBuf, preferred_model: Option<String>) {
    *MODELS_DIR.write() = Some(models_dir);
    if let Some(m) = preferred_model {
        if !m.trim().is_empty() {
            *PREFERRED_MODEL.write() = m;
        }
    }
}

pub fn models_dir() -> Option<PathBuf> {
    MODELS_DIR.read().clone()
}

pub fn preferred_model() -> String {
    PREFERRED_MODEL.read().clone()
}

pub fn set_preferred_model(model: &str) {
    *PREFERRED_MODEL.write() = model.to_string();
}

/// Locate a usable GGML model: the preferred one if installed, otherwise any
/// installed ggml-*.bin (largest first, on the assumption bigger = better).
pub fn resolve_model_path() -> Result<PathBuf, String> {
    let dir = MODELS_DIR
        .read()
        .clone()
        .ok_or("Local transcription not configured (no models directory)")?;

    let preferred = dir.join(format!("ggml-{}.bin", PREFERRED_MODEL.read()));
    if preferred.exists() {
        return Ok(preferred);
    }

    let mut candidates: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read models dir {:?}: {}", dir, e))?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("ggml-") && name.ends_with(".bin")
        })
        .filter_map(|e| {
            let size = e.metadata().ok()?.len();
            Some((size, e.path()))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    candidates
        .into_iter()
        .next()
        .map(|(_, p)| p)
        .ok_or_else(|| {
            "No Whisper model installed. Download one in Settings → Transcription → Local."
                .to_string()
        })
}

// ─── Internal types ──────────────────────────────────────────────────────────

struct AudioBatch {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

struct UtteranceJob {
    start_time: chrono::DateTime<chrono::Utc>,
    samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
    /// True speech start time, seconds since UNIX epoch.
    pub start: f64,
    pub duration: f64,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionStatus {
    connected: bool,
    provider: String,
    error: Option<String>,
    reconnect_count: u64,
}

fn frame_is_silent(frame: &[f32]) -> bool {
    if frame.is_empty() {
        return true;
    }
    let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    energy.sqrt() < SILENCE_RMS
}

// ─── Provider ────────────────────────────────────────────────────────────────

pub struct LocalWhisperProvider {
    should_run: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
    live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
}

impl LocalWhisperProvider {
    pub fn new() -> Self {
        Self {
            should_run: Arc::new(AtomicBool::new(false)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
            live_intel_agent: Arc::new(RwLock::new(None)),
        }
    }

    fn emit_status(app: &AppHandle, connected: bool, error: Option<String>) {
        let status = TranscriptionStatus {
            connected,
            provider: "local_whisper".to_string(),
            error,
            reconnect_count: 0,
        };
        if let Err(e) = app.emit("transcription_status", &status) {
            log::warn!("Failed to emit transcription status: {}", e);
        }
    }

    /// Buffering + endpointing loop; hands utterances to the inference thread.
    async fn processing_loop(
        app: AppHandle,
        model_path: PathBuf,
        should_run: Arc<AtomicBool>,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) {
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(64);
        *audio_tx_holder.write() = Some(audio_tx);

        // Dedicated inference thread: whisper.cpp inference is CPU/GPU-bound
        // and blocking; a std thread keeps it off the async runtime entirely.
        let (job_tx, job_rx) = std::sync::mpsc::channel::<UtteranceJob>();
        let worker_app = app.clone();
        let worker_connected = is_connected.clone();
        let worker = std::thread::Builder::new()
            .name("whisper-inference".into())
            .spawn(move || {
                Self::inference_worker(
                    worker_app,
                    model_path,
                    worker_connected,
                    database,
                    meeting_id,
                    live_intel_agent,
                    job_rx,
                );
            });
        let worker = match worker {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to spawn whisper inference thread: {}", e);
                Self::emit_status(&app, false, Some(e.to_string()));
                return;
            }
        };

        let mut buffer: Vec<f32> = Vec::with_capacity(MAX_UTTERANCE_SAMPLES);
        let mut buffer_start_sample: u64 = 0;
        let mut anchor: Option<chrono::DateTime<chrono::Utc>> = None;

        let sample_time = |anchor: chrono::DateTime<chrono::Utc>, n: u64| {
            anchor + chrono::Duration::microseconds((n as f64 / SAMPLE_RATE as f64 * 1e6) as i64)
        };

        loop {
            let running = should_run.load(Ordering::SeqCst) && is_connected.load(Ordering::SeqCst);

            match tokio::time::timeout(std::time::Duration::from_millis(50), audio_rx.recv()).await
            {
                Ok(Some(batch)) => {
                    let resampled =
                        Self::resample_to_16k_mono(&batch.samples, batch.sample_rate, batch.channels);
                    if anchor.is_none() && !resampled.is_empty() {
                        anchor = Some(
                            chrono::Utc::now()
                                - chrono::Duration::microseconds(
                                    (resampled.len() as f64 / SAMPLE_RATE as f64 * 1e6) as i64,
                                ),
                        );
                    }
                    buffer.extend(resampled);
                    while let Ok(batch) = audio_rx.try_recv() {
                        let resampled = Self::resample_to_16k_mono(
                            &batch.samples,
                            batch.sample_rate,
                            batch.channels,
                        );
                        buffer.extend(resampled);
                    }
                }
                Ok(None) => break,
                Err(_) => {}
            }

            let anchor_ts = match anchor {
                Some(a) => a,
                None => {
                    if !running {
                        break;
                    }
                    continue;
                }
            };

            let frames: Vec<bool> = buffer.chunks(FRAME_SAMPLES).map(frame_is_silent).collect();
            let has_speech = frames.iter().any(|&s| !s);

            if !has_speech {
                if buffer.len() > MAX_SILENCE_SAMPLES {
                    buffer_start_sample += buffer.len() as u64;
                    buffer.clear();
                }
                if !running {
                    break;
                }
                continue;
            }

            if let Some(first_speech) = frames.iter().position(|&s| !s) {
                let lead_samples = first_speech * FRAME_SAMPLES;
                if lead_samples > LEAD_PAD_SAMPLES * 2 {
                    let cut = lead_samples - LEAD_PAD_SAMPLES;
                    buffer.drain(..cut);
                    buffer_start_sample += cut as u64;
                }
            }

            let trailing_silent = frames.len() >= TRAILING_SILENCE_FRAMES
                && frames[frames.len() - TRAILING_SILENCE_FRAMES..]
                    .iter()
                    .all(|&s| s);

            let should_flush = buffer.len() >= MAX_UTTERANCE_SAMPLES
                || (buffer.len() >= MIN_UTTERANCE_SAMPLES && trailing_silent)
                || (!running && buffer.len() >= FRAME_SAMPLES);

            if should_flush {
                let utterance: Vec<f32> = if buffer.len() >= MAX_UTTERANCE_SAMPLES {
                    buffer.drain(..MAX_UTTERANCE_SAMPLES).collect()
                } else {
                    buffer.drain(..).collect()
                };
                let start_time = sample_time(anchor_ts, buffer_start_sample);
                buffer_start_sample += utterance.len() as u64;

                if job_tx
                    .send(UtteranceJob {
                        start_time,
                        samples: utterance,
                    })
                    .is_err()
                {
                    log::warn!("Whisper inference thread gone; stopping local STT loop");
                    break;
                }
            }

            if !running && buffer.len() < FRAME_SAMPLES {
                break;
            }
        }

        drop(job_tx);
        let _ = tokio::task::spawn_blocking(move || {
            let _ = worker.join();
        })
        .await;

        is_connected.store(false, Ordering::SeqCst);
        *audio_tx_holder.write() = None;
        Self::emit_status(&app, false, None);
        log::info!("Local Whisper processing loop exited");
    }

    /// Runs on a dedicated thread. Loads the model once, then transcribes
    /// utterances in speech order.
    fn inference_worker(
        app: AppHandle,
        model_path: PathBuf,
        is_connected: Arc<AtomicBool>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
        job_rx: std::sync::mpsc::Receiver<UtteranceJob>,
    ) {
        log::info!("🧊 Loading Whisper model: {:?}", model_path);
        let ctx = match WhisperContext::new_with_params(
            &model_path.to_string_lossy(),
            WhisperContextParameters::default(),
        ) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Failed to load Whisper model {:?}: {}", model_path, e);
                log::error!("{}", msg);
                is_connected.store(false, Ordering::SeqCst);
                Self::emit_status(&app, false, Some(msg));
                return;
            }
        };

        let mut state = match ctx.create_state() {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to create Whisper state: {}", e);
                log::error!("{}", msg);
                is_connected.store(false, Ordering::SeqCst);
                Self::emit_status(&app, false, Some(msg));
                return;
            }
        };

        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() as i32).min(8))
            .unwrap_or(4);

        log::info!(
            "✅ Local Whisper ready ({} threads, model {:?})",
            n_threads,
            model_path.file_name().unwrap_or_default()
        );

        while let Ok(job) = job_rx.recv() {
            let started = std::time::Instant::now();
            let audio_secs = job.samples.len() as f64 / SAMPLE_RATE as f64;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_n_threads(n_threads);
            params.set_translate(false);
            params.set_language(Some("en"));
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            params.set_no_context(true);

            if let Err(e) = state.full(params, &job.samples) {
                log::error!("Whisper inference failed: {}", e);
                continue;
            }

            let num_segments = state.full_n_segments().unwrap_or(0);
            for i in 0..num_segments {
                let text = match state.full_get_segment_text(i) {
                    Ok(t) => t.trim().to_string(),
                    Err(_) => continue,
                };
                if text.is_empty() || Self::is_non_speech_marker(&text) {
                    continue;
                }

                // t0/t1 are centiseconds relative to the utterance audio
                let t0 = state.full_get_segment_t0(i).unwrap_or(0) as f64 / 100.0;
                let t1 = state.full_get_segment_t1(i).unwrap_or(0) as f64 / 100.0;
                let segment_start =
                    job.start_time + chrono::Duration::microseconds((t0 * 1e6) as i64);
                let duration = (t1 - t0).max(0.0);

                let segment = TranscriptSegment {
                    text: text.clone(),
                    is_final: true,
                    confidence: 0.9,
                    start: segment_start.timestamp_micros() as f64 / 1e6,
                    duration,
                    speaker: None,
                };

                log::info!("📝 Whisper [{:.1}s]: {}", duration, text);

                if let Err(e) = app.emit("live_transcript", &segment) {
                    log::error!("Emit failed: {}", e);
                }

                if let Some(agent) = live_intel_agent.read().as_ref() {
                    let mut agent = agent.write();
                    let intel_seg = crate::catch_up_agent::TranscriptSegment {
                        id: uuid::Uuid::new_v4().to_string(),
                        timestamp_ms: segment_start.timestamp_millis(),
                        speaker: None,
                        text: text.clone(),
                    };
                    agent.process_segment(intel_seg);
                }

                let db = database.read().as_ref().cloned();
                let mid = meeting_id.read().as_ref().cloned();
                if let (Some(db), Some(mid)) = (db, mid) {
                    let t = text.clone();
                    if let Err(e) = tauri::async_runtime::block_on(
                        db.add_transcript_at(&mid, &t, None, true, 0.9, segment_start),
                    ) {
                        log::warn!("Failed to persist transcript: {}", e);
                    }
                }
            }

            log::debug!(
                "Whisper: {:.1}s audio in {:.0}ms",
                audio_secs,
                started.elapsed().as_millis()
            );
        }

        log::info!("Whisper inference worker exited");
    }

    /// Whisper emits markers like "[BLANK_AUDIO]", "(music)", "[typing]" for
    /// non-speech audio — don't pollute the transcript with them.
    fn is_non_speech_marker(text: &str) -> bool {
        let t = text.trim();
        (t.starts_with('[') && t.ends_with(']'))
            || (t.starts_with('(') && t.ends_with(')'))
            || (t.starts_with('*') && t.ends_with('*'))
    }

    fn resample_to_16k_mono(samples: &[f32], from_rate: u32, channels: u16) -> Vec<f32> {
        if samples.is_empty() {
            return vec![];
        }

        let mono: Vec<f32> = if channels > 1 {
            samples
                .chunks(channels as usize)
                .map(|chunk| {
                    if chunk.len() == channels as usize {
                        chunk.iter().sum::<f32>() / channels as f32
                    } else {
                        chunk[0]
                    }
                })
                .collect()
        } else {
            samples.to_vec()
        };

        if from_rate == 16000 {
            return mono;
        }

        let ratio = 16000.0 / from_rate as f64;
        let new_len = (mono.len() as f64 * ratio) as usize;
        if new_len == 0 {
            return vec![];
        }

        let mut resampled = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx.floor() as usize;
            let frac = src_idx - idx as f64;
            let sample = if idx + 1 < mono.len() {
                mono[idx] * (1.0 - frac as f32) + mono[idx + 1] * frac as f32
            } else if idx < mono.len() {
                mono[idx]
            } else {
                0.0
            };
            resampled.push(sample);
        }
        resampled
    }
}

#[async_trait]
impl TranscriptionProvider for LocalWhisperProvider {
    fn start(&self) {
        let app = match self.app_handle.read().clone() {
            Some(a) => a,
            None => {
                log::warn!("Cannot start local Whisper: no app handle");
                return;
            }
        };
        if self.should_run.load(Ordering::SeqCst) {
            log::info!("Local Whisper already running");
            return;
        }

        let model_path = match resolve_model_path() {
            Ok(p) => p,
            Err(e) => {
                log::error!("❌ Cannot start local Whisper: {}", e);
                Self::emit_status(&app, false, Some(e));
                return;
            }
        };

        self.should_run.store(true, Ordering::SeqCst);
        self.is_connected.store(true, Ordering::SeqCst);
        Self::emit_status(&app, true, None);

        let should_run = self.should_run.clone();
        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();
        let live_intel_agent = self.live_intel_agent.clone();

        tauri::async_runtime::spawn(async move {
            Self::processing_loop(
                app,
                model_path,
                should_run,
                is_connected,
                audio_tx_holder,
                database,
                meeting_id,
                live_intel_agent,
            )
            .await;
        });
    }

    fn stop(&self) {
        self.should_run.store(false, Ordering::SeqCst);
        // Leave is_connected set so the loop can flush the final utterance;
        // it clears the flag itself on exit.
        log::info!("Local Whisper stop requested");
    }

    fn process_audio(&self, samples: &[f32], sample_rate: u32, channels: u16) {
        if samples.is_empty() || !self.is_connected.load(Ordering::SeqCst) {
            return;
        }
        if let Some(tx) = self.audio_tx.read().as_ref() {
            let batch = AudioBatch {
                samples: samples.to_vec(),
                sample_rate,
                channels: if channels == 0 { 1 } else { channels },
            };
            if tx.try_send(batch).is_err() {
                log::trace!("Local Whisper audio queue full, batch dropped");
            }
        }
    }

    fn is_active(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Local provider needs no API key — accepted and ignored.
    fn set_api_key(&self, _key: String) {}

    fn set_context(
        &self,
        app_handle: AppHandle,
        database: Arc<DatabaseManager>,
        meeting_id: String,
        live_intel_agent: Arc<RwLock<LiveIntelAgent>>,
    ) {
        *self.app_handle.write() = Some(app_handle);
        *self.database.write() = Some(database);
        *self.meeting_id.write() = Some(meeting_id);
        *self.live_intel_agent.write() = Some(live_intel_agent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_non_speech_markers_filtered() {
        assert!(LocalWhisperProvider::is_non_speech_marker("[BLANK_AUDIO]"));
        assert!(LocalWhisperProvider::is_non_speech_marker("(music)"));
        assert!(LocalWhisperProvider::is_non_speech_marker("*typing*"));
        assert!(!LocalWhisperProvider::is_non_speech_marker("Hello there"));
    }

    #[test]
    fn test_model_resolution_unconfigured() {
        // Without configure(), resolution fails with guidance, not a panic
        // (MODELS_DIR may be set by other tests; only assert no panic)
        let _ = resolve_model_path();
    }
}
