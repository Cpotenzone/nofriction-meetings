// Google Cloud Speech-to-Text V2 — near-real-time utterance streaming
//
// Audio is buffered with sample-accurate timing and segmented into utterances
// on natural pauses (energy endpointing), so Chirp 2 always sees whole
// phrases instead of arbitrary 500ms slices. Word time offsets returned by
// the API are anchored to the capture-stream start, so every emitted segment
// and every persisted transcript row carries the true speech time — this is
// what keeps the Rewind view frame/transcript alignment exact.
//
// Utterances are transcribed by a single sequential worker, which guarantees
// transcripts are emitted and persisted in speech order even though each
// utterance is a separate HTTP request.

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use crate::transcription::TranscriptionProvider;

// ─── Tuning constants ────────────────────────────────────────────────────────

/// Everything is resampled to 16kHz mono before buffering.
const SAMPLE_RATE: usize = 16_000;
/// Energy frames used for endpointing (50ms).
const FRAME_SAMPLES: usize = SAMPLE_RATE / 20;
/// RMS below this is treated as silence.
const SILENCE_RMS: f32 = 0.008;
/// A pause this long ends an utterance.
const TRAILING_SILENCE_FRAMES: usize = 13; // ~650ms
/// Don't flush utterances shorter than this (avoids one-word fragments).
const MIN_UTTERANCE_SAMPLES: usize = SAMPLE_RATE; // 1s
/// Force a flush at this length even mid-speech (sync recognize limit is 60s).
const MAX_UTTERANCE_SAMPLES: usize = SAMPLE_RATE * 15; // 15s
/// Keep this much leading silence as padding when trimming.
const LEAD_PAD_SAMPLES: usize = SAMPLE_RATE / 5; // 200ms
/// Drop a buffer that is pure silence once it exceeds this length.
const MAX_SILENCE_SAMPLES: usize = SAMPLE_RATE * 5; // 5s

// ─── V2 API Request/Response Types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecognizeRequest {
    config: RecognitionConfig,
    content: String, // base64-encoded audio
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecognitionConfig {
    explicit_decoding_config: ExplicitDecodingConfig,
    language_codes: Vec<String>,
    model: String,
    features: RecognitionFeatures,
}

// Raw LINEAR16 PCM has no container header, so auto-decoding cannot detect
// it — the decoding parameters must be spelled out explicitly.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplicitDecodingConfig {
    encoding: String,
    sample_rate_hertz: u32,
    audio_channel_count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecognitionFeatures {
    enable_automatic_punctuation: bool,
    enable_word_time_offsets: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    diarization_config: Option<DiarizationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiarizationConfig {
    min_speaker_count: u32,
    max_speaker_count: u32,
}

#[derive(Debug, Deserialize)]
struct RecognizeResponse {
    results: Option<Vec<SpeechResult>>,
}

#[derive(Debug, Deserialize)]
struct SpeechResult {
    alternatives: Option<Vec<SpeechAlternative>>,
}

#[derive(Debug, Deserialize)]
struct SpeechAlternative {
    transcript: Option<String>,
    confidence: Option<f32>,
    words: Option<Vec<WordInfo>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordInfo {
    #[allow(dead_code)]
    word: Option<String>,
    speaker_label: Option<String>,
    start_offset: Option<String>,
    end_offset: Option<String>,
}

// ─── Frontend Event Types ───────────────────────────────────────────────────

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

// ─── Internal ───────────────────────────────────────────────────────────────

struct AudioBatch {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

/// One endpointed utterance queued for transcription.
struct UtteranceJob {
    /// Wall-clock time of the first sample in this utterance.
    start_time: chrono::DateTime<chrono::Utc>,
    duration_secs: f64,
    audio_b64: String,
}

/// Parse a protobuf Duration JSON string like "3.500s" into seconds.
fn parse_offset_secs(offset: &Option<String>) -> Option<f64> {
    offset
        .as_ref()
        .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok())
}

fn frame_is_silent(frame: &[f32]) -> bool {
    if frame.is_empty() {
        return true;
    }
    let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    energy.sqrt() < SILENCE_RMS
}

/// Diarization is only supported on a subset of V2 models; sending the config
/// with Chirp models makes every request fail.
fn model_supports_diarization(model: &str) -> bool {
    !model.starts_with("chirp")
}

// ─── Provider ───────────────────────────────────────────────────────────────

pub struct GoogleSTTProvider {
    service_account_key: Arc<RwLock<Option<String>>>,
    access_token: Arc<RwLock<Option<String>>>,
    token_expiry: Arc<RwLock<Option<u64>>>,
    should_run: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
    live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    reconnect_count: Arc<AtomicU64>,
    /// Region for V2 API (e.g. "us-central1")
    region: Arc<RwLock<String>>,
    /// Model (e.g. "chirp_2")
    model: Arc<RwLock<String>>,
    /// Enable speaker diarization (ignored on models that don't support it)
    diarization: Arc<AtomicBool>,
}

impl GoogleSTTProvider {
    pub fn new() -> Self {
        Self {
            service_account_key: Arc::new(RwLock::new(None)),
            access_token: Arc::new(RwLock::new(None)),
            token_expiry: Arc::new(RwLock::new(None)),
            should_run: Arc::new(AtomicBool::new(false)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
            live_intel_agent: Arc::new(RwLock::new(None)),
            reconnect_count: Arc::new(AtomicU64::new(0)),
            region: Arc::new(RwLock::new("us-central1".to_string())),
            model: Arc::new(RwLock::new("chirp_2".to_string())),
            diarization: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Set region for V2 API
    pub fn set_region(&self, region: String) {
        *self.region.write() = region;
    }

    /// Set model (chirp_2, latest_long, etc.)
    pub fn set_model(&self, model: String) {
        *self.model.write() = model;
    }

    /// Enable/disable speaker diarization
    pub fn set_diarization(&self, enabled: bool) {
        self.diarization.store(enabled, Ordering::SeqCst);
    }

    fn emit_status(app: &AppHandle, connected: bool, error: Option<String>, reconnects: u64) {
        let status = TranscriptionStatus {
            connected,
            provider: "google_stt_v2".to_string(),
            error,
            reconnect_count: reconnects,
        };
        if let Err(e) = app.emit("transcription_status", &status) {
            log::warn!("Failed to emit transcription status: {}", e);
        }
    }

    async fn get_or_refresh_token(
        service_account_json: &str,
        token_holder: &Arc<RwLock<Option<String>>>,
        expiry_holder: &Arc<RwLock<Option<u64>>>,
    ) -> Result<String, String> {
        // Check if existing token is still valid (with 5 minute buffer)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Some(token) = token_holder.read().as_ref() {
            if let Some(expiry) = *expiry_holder.read() {
                if now + 300 < expiry {
                    return Ok(token.clone());
                }
            }
        }

        // Mint new token
        let sa: serde_json::Value = serde_json::from_str(service_account_json)
            .map_err(|e| format!("Invalid service account JSON: {}", e))?;

        let client_email = sa["client_email"]
            .as_str()
            .ok_or("Missing client_email")?;
        let private_key = sa["private_key"]
            .as_str()
            .ok_or("Missing private_key")?;

        #[derive(Debug, Serialize)]
        struct Claims {
            iss: String,
            scope: String,
            aud: String,
            exp: u64,
            iat: u64,
        }

        let claims = Claims {
            iss: client_email.to_string(),
            scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
            aud: "https://oauth2.googleapis.com/token".to_string(),
            exp: now + 3600,
            iat: now,
        };

        let header = Header::new(Algorithm::RS256);
        let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|e| format!("Invalid private key: {}", e))?;

        let assertion = jsonwebtoken::encode(&header, &claims, &encoding_key)
            .map_err(|e| format!("Failed to sign JWT: {}", e))?;

        let client = reqwest::Client::new();
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("Token request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Google OAuth error: {}", error_text));
        }

        let token_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token: {}", e))?;

        let token = token_response["access_token"]
            .as_str()
            .map(String::from)
            .ok_or("No access_token in response")?;

        *token_holder.write() = Some(token.clone());
        *expiry_holder.write() = Some(now + 3500); // slightly before actual expiry

        log::info!("🔑 Google Cloud STT access token refreshed");
        Ok(token)
    }

    /// Extract the GCP project ID from the service account JSON
    fn get_project_id(service_account_json: &str) -> Result<String, String> {
        let sa: serde_json::Value = serde_json::from_str(service_account_json)
            .map_err(|e| format!("Invalid SA JSON: {}", e))?;
        sa["project_id"]
            .as_str()
            .map(String::from)
            .ok_or("Missing project_id in service account".to_string())
    }

    /// Buffering + endpointing loop. Segments incoming audio into utterances
    /// and hands them to the sequential transcription worker.
    #[allow(clippy::too_many_arguments)]
    async fn processing_loop(
        service_account: String,
        region: String,
        model: String,
        diarization_enabled: bool,
        app: AppHandle,
        should_run: Arc<AtomicBool>,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        token_holder: Arc<RwLock<Option<String>>>,
        expiry_holder: Arc<RwLock<Option<u64>>>,
        reconnect_count: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) {
        let project_id = match Self::get_project_id(&service_account) {
            Ok(id) => id,
            Err(e) => {
                log::error!("❌ Cannot start Google STT: {}", e);
                Self::emit_status(&app, false, Some(e), 0);
                return;
            }
        };

        // Fail fast on bad credentials before accepting audio
        if let Err(e) =
            Self::get_or_refresh_token(&service_account, &token_holder, &expiry_holder).await
        {
            log::error!("❌ Google STT auth failed: {}", e);
            Self::emit_status(&app, false, Some(e), 0);
            return;
        }

        is_connected.store(true, Ordering::SeqCst);
        Self::emit_status(&app, true, None, 0);
        log::info!(
            "✅ Google Cloud STT V2 ready (model={}, region={}, diarization={})",
            model,
            region,
            diarization_enabled && model_supports_diarization(&model)
        );

        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(64);
        *audio_tx_holder.write() = Some(audio_tx);

        // Sequential transcription worker — preserves speech order.
        let (job_tx, job_rx) = mpsc::channel::<UtteranceJob>(32);
        let worker = tokio::spawn(Self::transcription_worker(
            service_account.clone(),
            project_id,
            region.clone(),
            model.clone(),
            diarization_enabled,
            app.clone(),
            is_connected.clone(),
            token_holder.clone(),
            expiry_holder.clone(),
            reconnect_count.clone(),
            database,
            meeting_id,
            live_intel_agent,
            job_rx,
        ));

        // Utterance buffer with sample-accurate absolute timing:
        // wall-clock of sample N  =  anchor + N / 16000.
        let mut buffer: Vec<f32> = Vec::with_capacity(MAX_UTTERANCE_SAMPLES);
        let mut buffer_start_sample: u64 = 0;
        let mut total_samples: u64 = 0;
        let mut anchor: Option<chrono::DateTime<chrono::Utc>> = None;

        let sample_time = |anchor: chrono::DateTime<chrono::Utc>, n: u64| {
            anchor + chrono::Duration::microseconds((n as f64 / SAMPLE_RATE as f64 * 1e6) as i64)
        };

        loop {
            let running = should_run.load(Ordering::SeqCst) && is_connected.load(Ordering::SeqCst);

            // Drain available audio (with a short wait so the loop idles cheaply)
            match tokio::time::timeout(std::time::Duration::from_millis(50), audio_rx.recv()).await
            {
                Ok(Some(batch)) => {
                    let resampled =
                        Self::resample_to_16k_mono(&batch.samples, batch.sample_rate, batch.channels);
                    if anchor.is_none() && !resampled.is_empty() {
                        // First audio: anchor the stream clock at the start of this batch
                        anchor = Some(
                            chrono::Utc::now()
                                - chrono::Duration::microseconds(
                                    (resampled.len() as f64 / SAMPLE_RATE as f64 * 1e6) as i64,
                                ),
                        );
                    }
                    total_samples += resampled.len() as u64;
                    buffer.extend(resampled);
                    // Keep draining without waiting while more is queued
                    while let Ok(batch) = audio_rx.try_recv() {
                        let resampled = Self::resample_to_16k_mono(
                            &batch.samples,
                            batch.sample_rate,
                            batch.channels,
                        );
                        total_samples += resampled.len() as u64;
                        buffer.extend(resampled);
                    }
                }
                Ok(None) => break, // channel closed
                Err(_) => {}       // timeout — evaluate endpointing below
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

            // ── Endpointing ────────────────────────────────────────────────
            let frames: Vec<bool> = buffer
                .chunks(FRAME_SAMPLES)
                .map(frame_is_silent)
                .collect();
            let has_speech = frames.iter().any(|&s| !s);

            // Pure-silence buffer: discard once it's clearly just quiet room
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

            // Trim long leading silence (keep a short pad before the speech)
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

                let bytes = Self::f32_to_i16_bytes(&utterance);
                let job = UtteranceJob {
                    start_time,
                    duration_secs: utterance.len() as f64 / SAMPLE_RATE as f64,
                    audio_b64: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    ),
                };
                if job_tx.send(job).await.is_err() {
                    log::warn!("Transcription worker gone; stopping Google STT loop");
                    break;
                }
            }

            if !running && buffer.len() < FRAME_SAMPLES {
                break;
            }

            // Keep the absolute clock honest even if we never flush
            debug_assert!(buffer_start_sample + buffer.len() as u64 <= total_samples + 1);
        }

        // Cleanup: close the job channel and let the worker finish in-flight work
        drop(job_tx);
        let _ = worker.await;

        is_connected.store(false, Ordering::SeqCst);
        *audio_tx_holder.write() = None;
        Self::emit_status(&app, false, None, reconnect_count.load(Ordering::Relaxed));
        log::info!("Google Cloud STT V2 processing loop exited");
    }

    /// Sequential worker: transcribes utterances one at a time so results are
    /// emitted and persisted in speech order.
    #[allow(clippy::too_many_arguments)]
    async fn transcription_worker(
        service_account: String,
        project_id: String,
        region: String,
        model: String,
        diarization_enabled: bool,
        app: AppHandle,
        is_connected: Arc<AtomicBool>,
        token_holder: Arc<RwLock<Option<String>>>,
        expiry_holder: Arc<RwLock<Option<u64>>>,
        reconnect_count: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
        mut job_rx: mpsc::Receiver<UtteranceJob>,
    ) {
        let client = reqwest::Client::new();
        let url = format!(
            "https://{}-speech.googleapis.com/v2/projects/{}/locations/{}/recognizers/_:recognize",
            region, project_id, region
        );

        let features = |diarize: bool| RecognitionFeatures {
            enable_automatic_punctuation: true,
            enable_word_time_offsets: true,
            diarization_config: if diarize {
                Some(DiarizationConfig {
                    min_speaker_count: 1,
                    max_speaker_count: 6,
                })
            } else {
                None
            },
        };
        let diarize = diarization_enabled && model_supports_diarization(&model);

        let mut consecutive_errors: u32 = 0;

        while let Some(job) = job_rx.recv().await {
            let access_token = match Self::get_or_refresh_token(
                &service_account,
                &token_holder,
                &expiry_holder,
            )
            .await
            {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Token refresh failed: {}", e);
                    consecutive_errors += 1;
                    if consecutive_errors > 5 {
                        is_connected.store(false, Ordering::SeqCst);
                        Self::emit_status(&app, false, Some(e), reconnect_count.load(Ordering::Relaxed));
                        break;
                    }
                    continue;
                }
            };

            let request_body = RecognizeRequest {
                config: RecognitionConfig {
                    explicit_decoding_config: ExplicitDecodingConfig {
                        encoding: "LINEAR16".to_string(),
                        sample_rate_hertz: SAMPLE_RATE as u32,
                        audio_channel_count: 1,
                    },
                    language_codes: vec!["en-US".to_string()],
                    model: model.clone(),
                    features: features(diarize),
                },
                content: job.audio_b64.clone(),
            };

            let resp = client
                .post(&url)
                .bearer_auth(&access_token)
                .json(&request_body)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    consecutive_errors = 0;
                    match r.json::<RecognizeResponse>().await {
                        Ok(stt) => {
                            Self::handle_response(
                                stt,
                                &job,
                                &app,
                                &database,
                                &meeting_id,
                                &live_intel_agent,
                            )
                            .await;
                        }
                        Err(e) => log::error!("GCP STT response parse failed: {}", e),
                    }
                }
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    log::error!("GCP STT error {}: {}", status, &body[..body.len().min(300)]);
                    consecutive_errors += 1;
                    if consecutive_errors > 5 {
                        is_connected.store(false, Ordering::SeqCst);
                        Self::emit_status(
                            &app,
                            false,
                            Some(format!("Google STT failing: HTTP {}", status)),
                            reconnect_count.load(Ordering::Relaxed),
                        );
                        break;
                    }
                }
                Err(e) => {
                    log::error!("GCP STT request failed: {}", e);
                    consecutive_errors += 1;
                    if consecutive_errors > 5 {
                        is_connected.store(false, Ordering::SeqCst);
                        Self::emit_status(
                            &app,
                            false,
                            Some(format!("Google STT unreachable: {}", e)),
                            reconnect_count.load(Ordering::Relaxed),
                        );
                        break;
                    }
                }
            }
        }

        log::info!("Google STT transcription worker exited");
    }

    /// Emit + persist one utterance's results with true speech timestamps.
    async fn handle_response(
        stt: RecognizeResponse,
        job: &UtteranceJob,
        app: &AppHandle,
        database: &Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: &Arc<RwLock<Option<String>>>,
        live_intel_agent: &Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) {
        let results = match stt.results {
            Some(r) => r,
            None => return,
        };

        for result in results {
            let alt = match result.alternatives.as_ref().and_then(|a| a.first()) {
                Some(a) => a,
                None => continue,
            };
            let text = alt.transcript.as_deref().unwrap_or("").trim();
            if text.is_empty() {
                continue;
            }

            // Anchor word offsets (relative to the utterance) to wall clock
            let (rel_start, rel_end) = alt
                .words
                .as_ref()
                .filter(|w| !w.is_empty())
                .map(|words| {
                    let first = parse_offset_secs(&words.first().unwrap().start_offset);
                    let last = parse_offset_secs(&words.last().unwrap().end_offset);
                    (first.unwrap_or(0.0), last.unwrap_or(job.duration_secs))
                })
                .unwrap_or((0.0, job.duration_secs));

            let segment_start = job.start_time
                + chrono::Duration::microseconds((rel_start * 1e6) as i64);
            let duration = (rel_end - rel_start).max(0.0);

            let speaker = alt
                .words
                .as_ref()
                .and_then(|w| w.first())
                .and_then(|w| w.speaker_label.clone());

            let segment = TranscriptSegment {
                text: text.to_string(),
                is_final: true,
                confidence: alt.confidence.unwrap_or(0.92),
                start: segment_start.timestamp_micros() as f64 / 1e6,
                duration,
                speaker: speaker.clone(),
            };

            log::info!("📝 GCP STT [{:.1}s]: {}", duration, text);

            if let Err(e) = app.emit("live_transcript", &segment) {
                log::error!("Emit failed: {}", e);
            }

            if let Some(agent) = live_intel_agent.read().as_ref() {
                let mut agent = agent.write();
                let intel_seg = crate::catch_up_agent::TranscriptSegment {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp_ms: segment_start.timestamp_millis(),
                    speaker: speaker.clone(),
                    text: text.to_string(),
                };
                agent.process_segment(intel_seg);
            }

            // Persist with the true speech time (in speech order — we await here)
            let db = database.read().as_ref().cloned();
            let mid = meeting_id.read().as_ref().cloned();
            if let (Some(db), Some(mid)) = (db, mid) {
                let confidence = alt.confidence.unwrap_or(0.92);
                if let Err(e) = db
                    .add_transcript_at(
                        &mid,
                        text,
                        speaker.as_deref(),
                        true,
                        confidence,
                        segment_start,
                    )
                    .await
                {
                    log::warn!("Failed to persist transcript: {}", e);
                }
            }
        }
    }

    fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
        samples
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .flat_map(|s| s.to_le_bytes())
            .collect()
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
impl TranscriptionProvider for GoogleSTTProvider {
    fn start(&self) {
        let service_account = match self.service_account_key.read().clone() {
            Some(k) => k,
            None => {
                log::warn!("Cannot start Google STT: no service account key");
                return;
            }
        };
        let app = match self.app_handle.read().clone() {
            Some(a) => a,
            None => {
                log::warn!("Cannot start Google STT: no app handle");
                return;
            }
        };
        if self.should_run.load(Ordering::SeqCst) {
            log::info!("Google STT V2 already running");
            return;
        }

        self.should_run.store(true, Ordering::SeqCst);
        self.reconnect_count.store(0, Ordering::Relaxed);

        let should_run = self.should_run.clone();
        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let token_holder = self.access_token.clone();
        let expiry_holder = self.token_expiry.clone();
        let reconnect_count = self.reconnect_count.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();
        let live_intel_agent = self.live_intel_agent.clone();
        let region = self.region.read().clone();
        let model = self.model.read().clone();
        let diarization = self.diarization.load(Ordering::SeqCst);

        tokio::spawn(async move {
            // Reconnect loop with exponential backoff
            let mut backoff_ms: u64 = 1000;
            const MAX_BACKOFF_MS: u64 = 30_000;

            while should_run.load(Ordering::SeqCst) {
                log::info!("🔗 Starting Google Cloud STT V2 (utterance streaming)...");
                Self::emit_status(&app, false, None, reconnect_count.load(Ordering::Relaxed));

                Self::processing_loop(
                    service_account.clone(),
                    region.clone(),
                    model.clone(),
                    diarization,
                    app.clone(),
                    should_run.clone(),
                    is_connected.clone(),
                    audio_tx_holder.clone(),
                    token_holder.clone(),
                    expiry_holder.clone(),
                    reconnect_count.clone(),
                    database.clone(),
                    meeting_id.clone(),
                    live_intel_agent.clone(),
                )
                .await;

                if !should_run.load(Ordering::SeqCst) {
                    break;
                }

                let n = reconnect_count.fetch_add(1, Ordering::Relaxed) + 1;
                log::warn!(
                    "⚠️ Google STT disconnected (attempt #{}) — reconnecting in {}ms",
                    n, backoff_ms
                );
                Self::emit_status(
                    &app,
                    false,
                    Some(format!("Reconnecting (attempt #{})...", n)),
                    n,
                );

                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
            }

            is_connected.store(false, Ordering::SeqCst);
            log::info!("Google STT V2 reconnect loop exited");
        });
    }

    fn stop(&self) {
        self.should_run.store(false, Ordering::SeqCst);
        // Leave is_connected set so the processing loop can flush the final
        // utterance; it clears the flag itself on exit.
        log::info!("Google STT V2 stop requested");
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
                log::trace!("GCP STT audio queue full, batch dropped");
            }
        }
    }

    fn is_active(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    fn set_api_key(&self, key: String) {
        *self.service_account_key.write() = Some(key);
    }

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
    fn test_parse_offset_secs() {
        assert_eq!(parse_offset_secs(&Some("3.500s".to_string())), Some(3.5));
        assert_eq!(parse_offset_secs(&Some("0s".to_string())), Some(0.0));
        assert_eq!(parse_offset_secs(&None), None);
        assert_eq!(parse_offset_secs(&Some("bogus".to_string())), None);
    }

    #[test]
    fn test_frame_silence_detection() {
        let silent = vec![0.0f32; FRAME_SAMPLES];
        assert!(frame_is_silent(&silent));

        let loud: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .collect();
        assert!(!frame_is_silent(&loud));
    }

    #[test]
    fn test_diarization_model_guard() {
        assert!(!model_supports_diarization("chirp_2"));
        assert!(!model_supports_diarization("chirp"));
        assert!(model_supports_diarization("latest_long"));
    }

    #[test]
    fn test_resample_passthrough_and_downmix() {
        // Stereo 16k → mono 16k, same length in frames
        let stereo: Vec<f32> = vec![0.5, -0.5, 0.5, -0.5];
        let mono = GoogleSTTProvider::resample_to_16k_mono(&stereo, 16000, 2);
        assert_eq!(mono.len(), 2);
        assert!(mono.iter().all(|&s| s.abs() < 1e-6));

        // 48k → 16k is a 3:1 reduction
        let x: Vec<f32> = vec![0.1; 4800];
        let y = GoogleSTTProvider::resample_to_16k_mono(&x, 48000, 1);
        assert_eq!(y.len(), 1600);
    }
}
