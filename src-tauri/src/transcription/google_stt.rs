use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::database::DatabaseManager;
use crate::transcription::TranscriptionProvider;

#[derive(Debug, Serialize)]
struct GoogleSTTRequest {
    config: GoogleSTTConfig,
    audio: GoogleSTTAudio,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSTTConfig {
    encoding: String,
    sample_rate_hertz: u32,
    language_code: String,
    enable_automatic_punctuation: bool,
    model: String,
}

#[derive(Debug, Serialize)]
struct GoogleSTTAudio {
    content: String, // base64 encoded
}

#[derive(Debug, Deserialize)]
struct GoogleSTTResponse {
    results: Option<Vec<GoogleSTTResult>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSTTResult {
    alternatives: Vec<GoogleSTTAlternative>,
    is_final: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GoogleSTTAlternative {
    transcript: String,
    confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
    pub start: f64,
    pub duration: f64,
    pub speaker: Option<String>,
}

struct AudioBatch {
    samples: Vec<f32>,
    sample_rate: u32,
}

pub struct GoogleSTTProvider {
    service_account_key: Arc<RwLock<Option<String>>>,
    access_token: Arc<RwLock<Option<String>>>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
}

impl GoogleSTTProvider {
    pub fn new() -> Self {
        Self {
            service_account_key: Arc::new(RwLock::new(None)),
            access_token: Arc::new(RwLock::new(None)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_access_token(service_account_json: &str) -> Result<String, String> {
        // Parse service account JSON
        let sa: serde_json::Value = serde_json::from_str(service_account_json)
            .map_err(|e| format!("Invalid service account JSON: {}", e))?;

        let client_email = sa["client_email"]
            .as_str()
            .ok_or("Missing client_email in service account")?;
        let private_key = sa["private_key"]
            .as_str()
            .ok_or("Missing private_key in service account")?;

        // Create JWT for Google OAuth
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = serde_json::json!({
            "iss": client_email,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "aud": "https://oauth2.googleapis.com/token",
            "exp": now + 3600,
            "iat": now,
        });

        // Sign JWT (simplified - in production use proper JWT library)
        // For now, we'll use a simpler approach with reqwest
        let client = reqwest::Client::new();
        let response: reqwest::Response = client
            .post("https://oauth2.googleapis.com/token")
            .json(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &format!("{}", claims)), // Simplified
            ])
            .send()
            .await
            .map_err(|e| format!("Failed to get access token: {}", e))?;

        let token_response: serde_json::Value = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        token_response["access_token"]
            .as_str()
            .map(String::from)
            .ok_or("No access token in response".to_string())
    }

    async fn process_audio_internal(
        access_token: String,
        app: AppHandle,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
    ) -> Result<(), String> {
        is_connected.store(true, Ordering::SeqCst);
        log::info!("✅ Google Cloud STT ready");

        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(100);
        *audio_tx_holder.write() = Some(audio_tx);

        let client = reqwest::Client::new();

        // Process audio in batches
        tokio::spawn(async move {
            let mut buffer: VecDeque<f32> = VecDeque::with_capacity(16000); // 1 second buffer

            loop {
                let result =
                    tokio::time::timeout(std::time::Duration::from_millis(100), audio_rx.recv())
                        .await;

                match result {
                    Ok(Some(batch)) => {
                        let resampled =
                            Self::resample_to_16k_mono(&batch.samples, batch.sample_rate);
                        buffer.extend(resampled);
                    }
                    Ok(None) => break,
                    Err(_) => {}
                }

                // Send 1 second chunks
                if buffer.len() >= 16000 {
                    if !is_connected.load(Ordering::SeqCst) {
                        return;
                    }

                    let chunk: Vec<f32> = buffer.drain(..16000).collect();
                    let bytes = Self::f32_to_i16_bytes(&chunk);
                    let base64_data =
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);

                    let request = GoogleSTTRequest {
                        config: GoogleSTTConfig {
                            encoding: "LINEAR16".to_string(),
                            sample_rate_hertz: 16000,
                            language_code: "en-US".to_string(),
                            enable_automatic_punctuation: true,
                            model: "latest_long".to_string(),
                        },
                        audio: GoogleSTTAudio {
                            content: base64_data,
                        },
                    };

                    // Send to Google Cloud STT API
                    let response = client
                        .post("https://speech.googleapis.com/v1/speech:recognize")
                        .bearer_auth(&access_token)
                        .json(&request)
                        .send()
                        .await;

                    if let Ok(resp) = response {
                        if let Ok(stt_response) = resp.json::<GoogleSTTResponse>().await {
                            if let Some(results) = stt_response.results {
                                for result in results {
                                    if let Some(alt) = result.alternatives.first() {
                                        if !alt.transcript.trim().is_empty() {
                                            let is_final = result.is_final.unwrap_or(true);
                                            let segment = TranscriptSegment {
                                                text: alt.transcript.clone(),
                                                is_final,
                                                confidence: alt.confidence.unwrap_or(0.9),
                                                start: 0.0,
                                                duration: 0.0,
                                                speaker: None,
                                            };

                                            // Emit to frontend
                                            if let Err(e) = app.emit("live_transcript", &segment) {
                                                log::error!("Failed to emit transcript: {}", e);
                                            }

                                            // Save transcripts
                                            if is_final {
                                                if let Some(db) = database.read().as_ref().cloned()
                                                {
                                                    if let Some(mid) =
                                                        meeting_id.read().as_ref().cloned()
                                                    {
                                                        let text_clone = alt.transcript.clone();
                                                        let confidence =
                                                            alt.confidence.unwrap_or(0.9);
                                                        tokio::spawn(async move {
                                                            let _ = db
                                                                .add_transcript(
                                                                    &mid,
                                                                    &text_clone,
                                                                    None,
                                                                    true,
                                                                    confidence,
                                                                )
                                                                .await;
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
        samples
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * 32767.0) as i16
            })
            .flat_map(|s| s.to_le_bytes())
            .collect()
    }

    fn resample_to_16k_mono(samples: &[f32], from_rate: u32) -> Vec<f32> {
        if samples.is_empty() {
            return vec![];
        }

        let mono: Vec<f32> = if samples.len() > 1 {
            samples
                .chunks(2)
                .map(|chunk| {
                    if chunk.len() == 2 {
                        (chunk[0] + chunk[1]) / 2.0
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
                log::warn!("Cannot connect to Google STT: no service account key");
                return;
            }
        };

        let app = match self.app_handle.read().clone() {
            Some(a) => a,
            None => {
                log::warn!("Cannot connect to Google STT: no app handle");
                return;
            }
        };

        if self.is_connected.load(Ordering::SeqCst) {
            return;
        }

        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();
        let access_token_holder = self.access_token.clone();

        tokio::spawn(async move {
            // Get access token
            match Self::get_access_token(&service_account).await {
                Ok(token) => {
                    *access_token_holder.write() = Some(token.clone());
                    if let Err(e) = Self::process_audio_internal(
                        token,
                        app,
                        is_connected,
                        audio_tx_holder,
                        database,
                        meeting_id,
                    )
                    .await
                    {
                        log::error!("Google STT processing failed: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to get Google STT access token: {}", e);
                }
            }
        });
    }

    fn stop(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
        log::info!("Google STT disconnected");
    }

    fn process_audio(&self, samples: &[f32], sample_rate: u32) {
        if !self.is_connected.load(Ordering::SeqCst) || samples.is_empty() {
            return;
        }

        if let Some(tx) = self.audio_tx.read().as_ref() {
            let batch = AudioBatch {
                samples: samples.to_vec(),
                sample_rate,
            };
            let _ = tx.try_send(batch);
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
    ) {
        *self.app_handle.write() = Some(app_handle);
        *self.database.write() = Some(database);
        *self.meeting_id.write() = Some(meeting_id);
    }
}
