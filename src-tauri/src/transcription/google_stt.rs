// Google Cloud Speech-to-Text V2 — Streaming via REST chunked approach
// Uses the V2 recognize endpoint with Chirp 2 model for high-quality transcription.
// Sends small audio chunks (500ms) for near-real-time results with speaker diarization.

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use crate::transcription::TranscriptionProvider;

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
    auto_decoding_config: AutoDecodingConfig,
    language_codes: Vec<String>,
    model: String,
    features: RecognitionFeatures,
}

#[derive(Debug, Serialize)]
struct AutoDecodingConfig {}

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
    #[serde(rename = "languageCode")]
    _language_code: Option<String>,
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
    word: Option<String>,
    speaker_label: Option<String>,
    #[allow(dead_code)]
    start_offset: Option<String>,
    #[allow(dead_code)]
    end_offset: Option<String>,
}

// ─── Frontend Event Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
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
    /// Enable speaker diarization
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

    /// Main processing loop — sends 500ms audio chunks to V2 recognize endpoint
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

        // Get initial token
        let token = match Self::get_or_refresh_token(
            &service_account,
            &token_holder,
            &expiry_holder,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                log::error!("❌ Google STT auth failed: {}", e);
                Self::emit_status(&app, false, Some(e), 0);
                return;
            }
        };

        is_connected.store(true, Ordering::SeqCst);
        Self::emit_status(&app, true, None, 0);
        log::info!(
            "✅ Google Cloud STT V2 ready (model={}, region={}, diarization={})",
            model,
            region,
            diarization_enabled
        );

        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(64);
        *audio_tx_holder.write() = Some(audio_tx);

        let client = reqwest::Client::new();
        // 500ms chunks at 16kHz = 8000 samples
        let chunk_size: usize = 8000;
        let mut buffer: VecDeque<f32> = VecDeque::with_capacity(chunk_size * 2);

        let recognizer = format!(
            "projects/{}/locations/{}/recognizers/_",
            project_id, region
        );
        let url = format!(
            "https://{}-speech.googleapis.com/v2/{}:recognize",
            region, recognizer
        );

        let mut consecutive_errors: u32 = 0;
        let _ = token; // we'll refresh per-request below

        loop {
            if !should_run.load(Ordering::SeqCst) {
                break;
            }

            // Receive audio with 50ms timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(50),
                audio_rx.recv(),
            )
            .await
            {
                Ok(Some(batch)) => {
                    let resampled = Self::resample_to_16k_mono(
                        &batch.samples,
                        batch.sample_rate,
                        batch.channels,
                    );
                    buffer.extend(resampled);
                }
                Ok(None) => break, // channel closed
                Err(_) => {}       // timeout, check buffer
            }

            // Send when we have 500ms+ of audio
            if buffer.len() < chunk_size {
                continue;
            }

            let chunk: Vec<f32> = buffer.drain(..chunk_size).collect();
            let bytes = Self::f32_to_i16_bytes(&chunk);
            let b64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &bytes,
            );

            // Refresh token if needed
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

            let features = RecognitionFeatures {
                enable_automatic_punctuation: true,
                enable_word_time_offsets: true,
                diarization_config: if diarization_enabled {
                    Some(DiarizationConfig {
                        min_speaker_count: 1,
                        max_speaker_count: 6,
                    })
                } else {
                    None
                },
            };

            let request_body = RecognizeRequest {
                config: RecognitionConfig {
                    auto_decoding_config: AutoDecodingConfig {},
                    language_codes: vec!["en-US".to_string()],
                    model: model.clone(),
                    features,
                },
                content: b64,
            };

            // Fire request without blocking audio loop
            let client = client.clone();
            let url = url.clone();
            let access_token = access_token.clone();
            let app_clone = app.clone();
            let intel_agent = live_intel_agent.clone();
            let db = database.clone();
            let mid = meeting_id.clone();

            tokio::spawn(async move {
                let resp = client
                    .post(&url)
                    .bearer_auth(&access_token)
                    .json(&request_body)
                    .send()
                    .await;

                match resp {
                    Ok(r) if r.status().is_success() => {
                        if let Ok(stt) = r.json::<RecognizeResponse>().await {
                            if let Some(results) = stt.results {
                                for result in results {
                                    if let Some(alts) = result.alternatives {
                                        if let Some(alt) = alts.first() {
                                            let text = alt.transcript.as_deref().unwrap_or("");
                                            if text.trim().is_empty() {
                                                continue;
                                            }

                                            // Extract speaker from word-level diarization
                                            let speaker = alt
                                                .words
                                                .as_ref()
                                                .and_then(|w| w.first())
                                                .and_then(|w| w.speaker_label.clone());

                                            let segment = TranscriptSegment {
                                                text: text.to_string(),
                                                is_final: true,
                                                confidence: alt.confidence.unwrap_or(0.92),
                                                start: 0.0,
                                                duration: 0.5,
                                                speaker: speaker.clone(),
                                            };

                                            log::info!("📝 GCP STT [Chirp2]: {}", text);

                                            // Emit to frontend
                                            if let Err(e) = app_clone.emit("live_transcript", &segment) {
                                                log::error!("Emit failed: {}", e);
                                            }

                                            // LiveIntelAgent
                                            if let Some(agent) = intel_agent.read().as_ref() {
                                                let mut agent = agent.write();
                                                let intel_seg = crate::catch_up_agent::TranscriptSegment {
                                                    id: uuid::Uuid::new_v4().to_string(),
                                                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                                    speaker: speaker.clone(),
                                                    text: text.to_string(),
                                                };
                                                agent.process_segment(intel_seg);
                                            }

                                            // Save to DB
                                            if let Some(db) = db.read().as_ref().cloned() {
                                                if let Some(mid) = mid.read().as_ref().cloned() {
                                                    let t = text.to_string();
                                                    let s = speaker.clone();
                                                    let c = alt.confidence.unwrap_or(0.92);
                                                    tokio::spawn(async move {
                                                        let _ = db.add_transcript(&mid, &t, s.as_deref(), true, c).await;
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(r) => {
                        let status = r.status();
                        let body = r.text().await.unwrap_or_default();
                        log::error!("GCP STT error {}: {}", status, &body[..body.len().min(200)]);
                    }
                    Err(e) => {
                        log::error!("GCP STT request failed: {}", e);
                    }
                }
            });

            consecutive_errors = 0;
        }

        // Cleanup
        is_connected.store(false, Ordering::SeqCst);
        *audio_tx_holder.write() = None;
        Self::emit_status(&app, false, None, reconnect_count.load(Ordering::Relaxed));
        log::info!("Google Cloud STT V2 processing loop exited");
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
                log::info!("🔗 Starting Google Cloud STT V2 (Chirp 2)...");
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
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
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
