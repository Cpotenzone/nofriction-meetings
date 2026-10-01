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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use crate::transcription::{filter, TranscriptionProvider};

// ─── Tuning constants (16kHz mono) ──────────────────────────────────────────

const SAMPLE_RATE: usize = 16_000;
const FRAME_SAMPLES: usize = SAMPLE_RATE / 20; // 50ms energy frames
/// Absolute floor for the speech threshold; the live threshold adapts upward
/// to the room's measured noise floor (fans, system audio hiss, open mics).
const MIN_SPEECH_RMS: f32 = 0.006;
const MAX_SPEECH_RMS: f32 = 0.05;
const NOISE_FLOOR_MULTIPLIER: f32 = 2.5;
const TRAILING_SILENCE_FRAMES: usize = 10; // ~500ms pause ends an utterance
const MIN_UTTERANCE_SAMPLES: usize = SAMPLE_RATE * 3 / 4; // 0.75s
/// Continuous speech is force-segmented here. Kept short so text lands while
/// it's still relevant; the cut is placed at the quietest nearby frame so
/// words aren't sliced in half.
const MAX_UTTERANCE_SAMPLES: usize = SAMPLE_RATE * 12;
const CUT_SEARCH_SAMPLES: usize = SAMPLE_RATE * 3;
const LEAD_PAD_SAMPLES: usize = SAMPLE_RATE / 5; // 200ms
const MAX_SILENCE_SAMPLES: usize = SAMPLE_RATE * 5;
/// A live (interim) hypothesis is refreshed after this much new audio.
const PARTIAL_EVERY_SAMPLES: usize = SAMPLE_RATE * 4 / 5; // 0.8s
/// No audio for this long (paused recording, stalled device) ends the open
/// utterance and re-anchors the sample clock to wall time on resume, so
/// timestamps after a pause stay correct.
const AUDIO_GAP: std::time::Duration = std::time::Duration::from_millis(600);
/// Encoder context for interim decodes (see inference_worker).
const PARTIAL_AUDIO_CTX: i32 = 768;
const PARTIAL_AUDIO_CTX_MAX_SAMPLES: usize = SAMPLE_RATE * 15;
/// Tail of the previous final transcript fed to Whisper as context.
const PROMPT_CONTEXT_CHARS: usize = 220;
/// After this much time without kept speech the prompt context is dropped,
/// so a stale (or farewell) line can't prime the decoder on later noise.
const CONTEXT_RESET_AFTER: chrono::Duration = chrono::Duration::seconds(45);
/// A dropped segment marks the next ~30s of audio as "right after junk".
const JUNK_STREAK_WINDOW: chrono::Duration = chrono::Duration::seconds(30);
/// Average token log-probability below this is low confidence (the
/// standard whisper `logprob_thold`).
const LOW_CONFIDENCE_LOGPROB: f32 = -1.0;
/// whisper.cpp skips a window as silence when no_speech_prob is above this
/// AND the average log-probability is below LOW_CONFIDENCE_LOGPROB.
const NO_SPEECH_THOLD: f32 = 0.6;

// ─── Global model configuration (set at startup / from settings) ────────────

static MODELS_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));
static PREFERRED_MODEL: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new("large-v3-turbo-q5_0".to_string()));

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
    utterance_id: u64,
    start_time: chrono::DateTime<chrono::Utc>,
    samples: Vec<f32>,
    /// Interim hypothesis for live display; not persisted.
    partial: bool,
    /// Speech threshold at the time the utterance was cut, for the
    /// low-energy hallucination guard.
    speech_rms: f32,
    /// Meeting the audio belongs to, fixed when the job is created so a
    /// quick stop→start can't file the last utterance under the new meeting.
    meeting_id: Option<String>,
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
    /// Groups an utterance's interim hypotheses with its final segments so
    /// the UI can replace live text in place instead of appending.
    pub utterance_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionStatus {
    connected: bool,
    provider: String,
    error: Option<String>,
    reconnect_count: u64,
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Tracks the ambient noise floor so endpointing works in a noisy room and
/// with always-on system audio, where a fixed RMS threshold never sees
/// "silence" and every utterance runs to the hard cap.
struct NoiseFloor {
    floor: f32,
}

impl NoiseFloor {
    fn new() -> Self {
        Self { floor: MIN_SPEECH_RMS / NOISE_FLOOR_MULTIPLIER }
    }

    /// Fast attack downward, slow release upward: quiet frames pull the floor
    /// down quickly, sustained loud audio only nudges it.
    fn observe(&mut self, rms: f32) {
        let rate = if rms < self.floor { 0.2 } else { 0.002 };
        self.floor += (rms - self.floor) * rate;
    }

    fn threshold(&self) -> f32 {
        (self.floor * NOISE_FLOOR_MULTIPLIER).clamp(MIN_SPEECH_RMS, MAX_SPEECH_RMS)
    }
}

/// Keep the tail of `text`, starting at a word boundary.
fn tail_chars(text: &str, max: usize) -> &str {
    let len = text.chars().count();
    if len <= max {
        return text;
    }
    let skip = text.char_indices().nth(len - max).map(|(i, _)| i).unwrap_or(0);
    let tail = &text[skip..];
    match tail.find(' ') {
        Some(sp) => tail[sp + 1..].trim_start(),
        None => tail,
    }
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
    /// Incremented per start(); a loop only tears down shared state it owns.
    generation: Arc<AtomicU64>,
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
            generation: Arc::new(AtomicU64::new(0)),
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
    ///
    /// While an utterance is still being spoken, a fresh interim hypothesis
    /// is requested every ~0.8s (only when the worker is idle, so partials
    /// never queue up behind finals). When the speaker pauses — or continuous
    /// speech hits the cap — the utterance is finalized with the more accurate
    /// decoder and persisted.
    async fn processing_loop(
        app: AppHandle,
        model_path: PathBuf,
        should_run: Arc<AtomicBool>,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
        generation: Arc<AtomicU64>,
        my_generation: u64,
    ) {
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(256);
        let job_meeting = meeting_id.clone();
        *audio_tx_holder.write() = Some(audio_tx);

        // Dedicated inference thread: whisper.cpp inference is CPU/GPU-bound
        // and blocking; a std thread keeps it off the async runtime entirely.
        let (job_tx, job_rx) = std::sync::mpsc::channel::<UtteranceJob>();
        let worker_busy = Arc::new(AtomicBool::new(false));
        let pending_finals = Arc::new(AtomicUsize::new(0));
        let worker_app = app.clone();
        let worker_connected = is_connected.clone();
        let (wb, wp) = (worker_busy.clone(), pending_finals.clone());
        let worker = std::thread::Builder::new()
            .name("whisper-inference".into())
            .spawn(move || {
                Self::inference_worker(
                    worker_app,
                    model_path,
                    worker_connected,
                    database,
                    live_intel_agent,
                    job_rx,
                    wb,
                    wp,
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

        let mut buffer: Vec<f32> = Vec::with_capacity(MAX_UTTERANCE_SAMPLES * 2);
        let mut frame_rms_cache: Vec<f32> = Vec::new();
        let mut buffer_start_sample: u64 = 0;
        let mut anchor: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut noise = NoiseFloor::new();
        let mut utterance_id: u64 = 1;
        let mut last_partial_len: usize = 0;
        let mut last_audio_at = std::time::Instant::now();

        let sample_time = |anchor: chrono::DateTime<chrono::Utc>, n: u64| {
            anchor + chrono::Duration::microseconds((n as f64 / SAMPLE_RATE as f64 * 1e6) as i64)
        };

        loop {
            let running = should_run.load(Ordering::SeqCst)
                && is_connected.load(Ordering::SeqCst)
                && generation.load(Ordering::SeqCst) == my_generation;

            match tokio::time::timeout(std::time::Duration::from_millis(50), audio_rx.recv()).await
            {
                Ok(Some(batch)) => {
                    // Resuming after a gap: the open utterance was already
                    // flushed; restart the sample clock at wall time.
                    if last_audio_at.elapsed() > AUDIO_GAP && anchor.is_some() {
                        buffer.clear();
                        frame_rms_cache.clear();
                        buffer_start_sample = 0;
                        last_partial_len = 0;
                        anchor = None;
                    }
                    last_audio_at = std::time::Instant::now();
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

            // Per-frame RMS; only newly completed frames update the noise floor
            let complete_frames = buffer.len() / FRAME_SAMPLES;
            while frame_rms_cache.len() < complete_frames {
                let i = frame_rms_cache.len();
                let rms = frame_rms(&buffer[i * FRAME_SAMPLES..(i + 1) * FRAME_SAMPLES]);
                noise.observe(rms);
                frame_rms_cache.push(rms);
            }
            let threshold = noise.threshold();
            let is_speech = |rms: f32| rms >= threshold;

            let first_speech = frame_rms_cache.iter().position(|&r| is_speech(r));

            let Some(first_speech) = first_speech else {
                if buffer.len() > MAX_SILENCE_SAMPLES {
                    // Keep the tail so a word starting at the boundary survives
                    let keep = LEAD_PAD_SAMPLES.min(buffer.len());
                    let cut = (buffer.len() - keep) / FRAME_SAMPLES * FRAME_SAMPLES;
                    buffer.drain(..cut);
                    frame_rms_cache.drain(..cut / FRAME_SAMPLES);
                    buffer_start_sample += cut as u64;
                }
                if !running {
                    break;
                }
                continue;
            };

            // Trim leading silence down to a small pad
            let lead_samples = first_speech * FRAME_SAMPLES;
            if lead_samples > LEAD_PAD_SAMPLES * 2 {
                let cut = (lead_samples - LEAD_PAD_SAMPLES) / FRAME_SAMPLES * FRAME_SAMPLES;
                buffer.drain(..cut);
                frame_rms_cache.drain(..cut / FRAME_SAMPLES);
                buffer_start_sample += cut as u64;
                last_partial_len = last_partial_len.saturating_sub(cut);
            }

            let trailing_silent = frame_rms_cache.len() >= TRAILING_SILENCE_FRAMES
                && frame_rms_cache[frame_rms_cache.len() - TRAILING_SILENCE_FRAMES..]
                    .iter()
                    .all(|&r| !is_speech(r));

            // Audio stopped arriving (paused / device stall): end the utterance
            let audio_gap = last_audio_at.elapsed() > AUDIO_GAP;

            let should_flush = buffer.len() >= MAX_UTTERANCE_SAMPLES
                || (buffer.len() >= MIN_UTTERANCE_SAMPLES && trailing_silent)
                || ((audio_gap || !running) && buffer.len() >= FRAME_SAMPLES);

            if should_flush {
                let take = if buffer.len() >= MAX_UTTERANCE_SAMPLES {
                    Self::quietest_cut(&frame_rms_cache, MAX_UTTERANCE_SAMPLES)
                } else {
                    buffer.len()
                };
                let utterance: Vec<f32> = buffer.drain(..take).collect();
                frame_rms_cache.drain(..(take / FRAME_SAMPLES).min(frame_rms_cache.len()));
                // A partial frame may remain at the head; recompute it lazily
                if take % FRAME_SAMPLES != 0 {
                    frame_rms_cache.clear();
                }
                let start_time = sample_time(anchor_ts, buffer_start_sample);
                buffer_start_sample += utterance.len() as u64;
                last_partial_len = 0;

                pending_finals.fetch_add(1, Ordering::SeqCst);
                if job_tx
                    .send(UtteranceJob {
                        utterance_id,
                        start_time,
                        samples: utterance,
                        partial: false,
                        speech_rms: threshold,
                        meeting_id: job_meeting.read().clone(),
                    })
                    .is_err()
                {
                    log::warn!("Whisper inference thread gone; stopping local STT loop");
                    break;
                }
                utterance_id += 1;
            } else if running
                && buffer.len() >= last_partial_len + PARTIAL_EVERY_SAMPLES
                && !worker_busy.load(Ordering::SeqCst)
                && pending_finals.load(Ordering::SeqCst) == 0
            {
                last_partial_len = buffer.len();
                worker_busy.store(true, Ordering::SeqCst);
                let _ = job_tx.send(UtteranceJob {
                    utterance_id,
                    start_time: sample_time(anchor_ts, buffer_start_sample),
                    samples: buffer.clone(),
                    partial: true,
                    speech_rms: threshold,
                    meeting_id: None,
                });
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

        // Clear any live line left on screen
        let _ = app.emit(
            "live_transcript_discard",
            serde_json::json!({ "utterance_id": format!("w{}", utterance_id) }),
        );

        // A newer start() may already own the shared sender/flags
        if generation.load(Ordering::SeqCst) == my_generation {
            is_connected.store(false, Ordering::SeqCst);
            *audio_tx_holder.write() = None;
            Self::emit_status(&app, false, None);
        }
        log::info!("Local Whisper processing loop exited");
    }

    /// Where to split continuous speech at the cap: the quietest 50ms frame in
    /// the last few seconds before `max_samples`, so the cut lands in a
    /// breath rather than mid-word.
    fn quietest_cut(frame_rms: &[f32], max_samples: usize) -> usize {
        let max_frame = (max_samples / FRAME_SAMPLES).min(frame_rms.len());
        let min_frame = max_frame.saturating_sub(CUT_SEARCH_SAMPLES / FRAME_SAMPLES);
        if max_frame <= min_frame {
            return max_samples;
        }
        let quietest = (min_frame..max_frame)
            .min_by(|&a, &b| frame_rms[a].total_cmp(&frame_rms[b]))
            .unwrap_or(max_frame);
        // Cut after the quiet frame
        ((quietest + 1) * FRAME_SAMPLES).min(max_samples)
    }

    /// Runs on a dedicated thread. Loads the model once, then transcribes
    /// utterances in speech order.
    #[allow(clippy::too_many_arguments)]
    fn inference_worker(
        app: AppHandle,
        model_path: PathBuf,
        is_connected: Arc<AtomicBool>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
        job_rx: std::sync::mpsc::Receiver<UtteranceJob>,
        busy: Arc<AtomicBool>,
        pending_finals: Arc<AtomicUsize>,
    ) {
        log::info!("🧊 Loading Whisper model: {:?}", model_path);
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu = true;
        ctx_params.flash_attn = true;
        let ctx = match WhisperContext::new_with_params(&model_path.to_string_lossy(), ctx_params)
            .or_else(|e| {
                log::warn!("Flash attention unavailable ({}); retrying without", e);
                WhisperContext::new_with_params(
                    &model_path.to_string_lossy(),
                    WhisperContextParameters::default(),
                )
            }) {
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

        let eot = ctx.token_eot();
        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() as i32).min(8))
            .unwrap_or(4);

        log::info!(
            "✅ Local Whisper ready ({} threads, model {:?}, live partials on)",
            n_threads,
            model_path.file_name().unwrap_or_default()
        );

        // Rolling context: recent final text primes the decoder so names and
        // jargon stay consistent across utterances. Only KEPT text enters it
        // (never filtered junk), and it is dropped after a long silence.
        let mut context = String::new();
        let mut last_final = String::new();
        let mut last_kept_end: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut last_junk_at: Option<chrono::DateTime<chrono::Utc>> = None;

        while let Ok(job) = job_rx.recv() {
            busy.store(true, Ordering::SeqCst);
            let started = std::time::Instant::now();
            let audio_secs = job.samples.len() as f64 / SAMPLE_RATE as f64;
            let uid = format!("w{}", job.utterance_id);

            // Finals: beam search for accuracy, unless finals are backing up.
            // Partials: fast greedy single-segment decode.
            let backlog = pending_finals.load(Ordering::SeqCst) > 1;
            let strategy = if job.partial || backlog {
                SamplingStrategy::Greedy { best_of: 1 }
            } else {
                SamplingStrategy::BeamSearch {
                    beam_size: 5,
                    patience: -1.0,
                }
            };
            let mut params = FullParams::new(strategy);
            params.set_n_threads(n_threads);
            params.set_translate(false);
            params.set_language(Some("en"));
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            params.set_suppress_nst(true);
            params.set_no_context(true);
            // Standard whisper anti-hallucination heuristics (explicit, so a
            // crate default change can't silently turn them off): temperature
            // fallback when a decode is repetitive (entropy) or unsure
            // (logprob), and skip windows that are most likely not speech.
            params.set_temperature(0.0);
            params.set_temperature_inc(0.2);
            params.set_entropy_thold(2.4);
            params.set_logprob_thold(LOW_CONFIDENCE_LOGPROB);
            params.set_no_speech_thold(NO_SPEECH_THOLD);
            if !job.partial {
                // Word timings for finals (stored as offsets + ms, used to
                // pin edits/strikes to exact moments). Cheap: no DTW.
                params.set_token_timestamps(true);
            }
            if job.partial {
                params.set_single_segment(true);
                params.set_no_timestamps(true);
                // Shrink the encoder window (1500 = 30s) to ~15s, which still
                // covers the longest utterance. Measured on M4 Pro with
                // large-v3-turbo-q5: ~3x faster partials with identical text.
                // Going lower garbles short clips, so this is a floor.
                if job.samples.len() <= PARTIAL_AUDIO_CTX_MAX_SAMPLES {
                    params.set_audio_ctx(PARTIAL_AUDIO_CTX);
                }
            }
            if let Some(last) = last_kept_end {
                if job.start_time - last > CONTEXT_RESET_AFTER && !context.is_empty() {
                    log::debug!("Whisper prompt context reset after silence");
                    context.clear();
                }
            }
            let mut prompt = tail_chars(&context, PROMPT_CONTEXT_CHARS).to_string();
            if !prompt.is_empty() && filter::is_repetitive(&prompt) {
                // A looping tail would prime more of the same
                context.clear();
                prompt.clear();
            }
            if !prompt.is_empty() {
                params.set_initial_prompt(&prompt);
            }

            // whisper.cpp needs ≥1s of audio; pad short clips with silence
            let mut samples = job.samples;
            if samples.len() < SAMPLE_RATE + SAMPLE_RATE / 10 {
                samples.resize(SAMPLE_RATE + SAMPLE_RATE / 10, 0.0);
            }

            let result = state.full(params, &samples);
            if !job.partial {
                pending_finals.fetch_sub(1, Ordering::SeqCst);
            }
            if let Err(e) = result {
                log::error!("Whisper inference failed: {}", e);
                busy.store(false, Ordering::SeqCst);
                continue;
            }

            let num_segments = state.full_n_segments().unwrap_or(0);

            if job.partial {
                let text = (0..num_segments)
                    .filter_map(|i| state.full_get_segment_text(i).ok())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                if !text.is_empty()
                    && !Self::is_non_speech_marker(&text)
                    && !filter::is_junk(&text)
                {
                    let segment = TranscriptSegment {
                        text,
                        is_final: false,
                        confidence: 0.5,
                        start: job.start_time.timestamp_micros() as f64 / 1e6,
                        duration: audio_secs,
                        speaker: None,
                        utterance_id: Some(uid.clone()),
                    };
                    let _ = app.emit("live_transcript", &segment);
                }
                busy.store(false, Ordering::SeqCst);
                continue;
            }

            let utterance_rms = frame_rms(&samples);
            // Barely above the adaptive speech threshold: noise, not a voice
            let low_energy = utterance_rms < job.speech_rms * 1.5;
            let mut emitted_any = false;
            for i in 0..num_segments {
                let text = match state.full_get_segment_text(i) {
                    Ok(t) => t.trim().to_string(),
                    Err(_) => continue,
                };
                if text.is_empty() || Self::is_non_speech_marker(&text) {
                    continue;
                }
                // Hallucination filter: stock phrases, repetition loops
                // ("Bye-bye. Bye-bye. …"), with whisper's own confidence
                let avg_logprob = Self::segment_avg_logprob(&state, i, eot);
                let signals = filter::Signals {
                    low_energy,
                    low_confidence: avg_logprob.map(|lp| lp < LOW_CONFIDENCE_LOGPROB).unwrap_or(false),
                    after_junk: last_junk_at
                        .map(|t| job.start_time - t < JUNK_STREAK_WINDOW)
                        .unwrap_or(false),
                };
                let text = match filter::classify(&text, signals) {
                    filter::Verdict::Keep(t) => t,
                    filter::Verdict::Drop(why) => {
                        // Never log transcript text
                        log::debug!(
                            "Dropping likely hallucination ({}, {} chars, logprob {:?})",
                            why,
                            text.chars().count(),
                            avg_logprob
                        );
                        last_junk_at = Some(job.start_time);
                        crate::meeting_end::note_filtered_segment();
                        continue;
                    }
                };
                // Whisper sometimes echoes its prompt / previous line verbatim
                if text.len() > 12 && text == last_final {
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
                    utterance_id: Some(uid.clone()),
                };
                emitted_any = true;
                let segment_end =
                    segment_start + chrono::Duration::microseconds((duration * 1e6) as i64);
                last_kept_end = Some(segment_end);
                crate::meeting_end::note_real_speech(segment_end);

                // Never log transcript text: logs outlive edits and strikes
                log::info!("📝 Whisper [{:.1}s]: {} chars", duration, text.chars().count());
                let word_timings =
                    Self::word_timings(&state, i, &text, eot, (t0 * 1000.0).round() as i64);

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
                let mid = job.meeting_id.clone();
                if let (Some(db), Some(mid)) = (db, mid) {
                    let t = text.clone();
                    if let Err(e) = tauri::async_runtime::block_on(
                        db.add_transcript_full(
                            &mid,
                            &t,
                            None,
                            true,
                            0.9,
                            segment_start,
                            word_timings.as_deref(),
                        ),
                    ) {
                        log::warn!("Failed to persist transcript: {}", e);
                    }
                }

                if !context.is_empty() {
                    context.push(' ');
                }
                context.push_str(&text);
                if context.len() > PROMPT_CONTEXT_CHARS * 4 {
                    context = tail_chars(&context, PROMPT_CONTEXT_CHARS * 2).to_string();
                }
                last_final = text;
            }

            // Tell the UI the live hypothesis resolved to nothing
            if !emitted_any {
                let _ = app.emit(
                    "live_transcript_discard",
                    serde_json::json!({ "utterance_id": uid }),
                );
            }

            log::debug!(
                "Whisper: {:.1}s audio in {:.0}ms",
                audio_secs,
                started.elapsed().as_millis()
            );
            busy.store(false, Ordering::SeqCst);
        }

        log::info!("Whisper inference worker exited");
    }

    /// Average log-probability of a segment's text tokens (the standard
    /// whisper confidence measure). None when the segment has no tokens.
    fn segment_avg_logprob(state: &whisper_rs::WhisperState, segment: i32, eot: i32) -> Option<f32> {
        let n = state.full_n_tokens(segment).ok()?;
        let (mut sum, mut count) = (0.0f32, 0u32);
        for j in 0..n {
            let Ok(data) = state.full_get_token_data(segment, j) else { continue };
            if data.id >= eot {
                continue; // special / timestamp tokens
            }
            sum += data.plog;
            count += 1;
        }
        (count > 0).then(|| sum / count as f32)
    }

    /// Per-word timings for one final segment as JSON
    /// (`redaction::WordTiming`: UTF-16 offsets into `text`, ms relative to
    /// the segment start). None if tokens don't line up with the text.
    fn word_timings(
        state: &whisper_rs::WhisperState,
        segment: i32,
        text: &str,
        eot: i32,
        seg_start_ms: i64,
    ) -> Option<String> {
        let n = state.full_n_tokens(segment).ok()?;
        // (word, t0_ms, t1_ms) built from sub-word tokens
        let mut words: Vec<(String, i64, i64)> = Vec::new();
        for j in 0..n {
            let data = state.full_get_token_data(segment, j).ok()?;
            if data.id >= eot {
                continue; // special / timestamp tokens
            }
            let piece = state.full_get_token_text_lossy(segment, j).ok()?;
            if piece.is_empty() {
                continue;
            }
            let (t0, t1) = (data.t0 * 10 - seg_start_ms, data.t1 * 10 - seg_start_ms);
            let starts_word = piece.starts_with(' ') || words.is_empty();
            let trimmed = piece.trim();
            if trimmed.is_empty() {
                continue;
            }
            if starts_word {
                words.push((trimmed.to_string(), t0.max(0), t1.max(0)));
            } else if let Some(last) = words.last_mut() {
                last.0.push_str(trimmed);
                last.2 = t1.max(last.2);
            }
        }
        // Map words onto the text, in order, by UTF-16 offset
        let mut timings = Vec::new();
        let mut cursor = 0usize; // byte offset
        for (w, t0, t1) in words {
            let at = text[cursor..].find(&w)? + cursor;
            let s = text[..at].encode_utf16().count();
            let e = s + w.encode_utf16().count();
            timings.push(crate::redaction::WordTiming { s, e, t0, t1 });
            cursor = at + w.len();
        }
        if timings.is_empty() {
            return None;
        }
        serde_json::to_string(&timings).ok()
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
        let generation = self.generation.clone();
        let my_generation = generation.fetch_add(1, Ordering::SeqCst) + 1;

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
                generation,
                my_generation,
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
    fn test_noise_floor_adapts_to_room() {
        let mut nf = NoiseFloor::new();
        for _ in 0..500 {
            nf.observe(0.02); // steady hum well above the fixed floor
        }
        // Threshold rises above the hum so the hum reads as silence…
        assert!(nf.threshold() > 0.02);
        // …but stays below real speech
        assert!(nf.threshold() < 0.08);
        for _ in 0..50 {
            nf.observe(0.001);
        }
        assert!((nf.threshold() - MIN_SPEECH_RMS).abs() < 1e-3);
    }

    #[test]
    fn test_quietest_cut_lands_in_pause() {
        let frames = MAX_UTTERANCE_SAMPLES / FRAME_SAMPLES;
        let mut rms = vec![0.1f32; frames + 20];
        rms[frames - 10] = 0.001; // a breath 0.5s before the cap
        let cut = LocalWhisperProvider::quietest_cut(&rms, MAX_UTTERANCE_SAMPLES);
        assert_eq!(cut, (frames - 9) * FRAME_SAMPLES);
        assert!(cut <= MAX_UTTERANCE_SAMPLES);
    }

    #[test]
    fn test_tail_chars_word_boundary() {
        assert_eq!(tail_chars("short", 20), "short");
        let t = tail_chars("alpha beta gamma delta", 10);
        assert!(t.len() <= 10 && !t.starts_with(' '));
        assert_eq!(t, "delta");
    }

    #[test]
    fn test_hallucination_phrases() {
        // Whisper path: weak audio turns a lone stock phrase into junk
        let weak = filter::Signals { low_energy: true, ..Default::default() };
        assert!(matches!(filter::classify(" Thank you. ", weak), filter::Verdict::Drop(_)));
        assert!(filter::is_junk("Thanks for watching!"));
        assert!(filter::is_junk("Bye-bye. Bye-bye. Bye-bye."));
        assert!(!filter::is_junk("Thank you for joining the call"));
    }

    #[test]
    fn test_model_resolution_unconfigured() {
        // Without configure(), resolution fails with guidance, not a panic
        // (MODELS_DIR may be set by other tests; only assert no panic)
        let _ = resolve_model_path();
    }
}
