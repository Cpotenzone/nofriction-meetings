use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::database::DatabaseManager;
use crate::transcription::TranscriptionProvider;

#[derive(Debug, Serialize)]
struct GeminiSetupMessage {
    setup: GeminiSetup,
}

#[derive(Debug, Serialize)]
struct GeminiSetup {
    model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRealtimeInput {
    realtime_input: GeminiMediaChunks,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiMediaChunks {
    media_chunks: Vec<GeminiMediaChunk>,
}

#[derive(Debug, Serialize)]
struct GeminiMediaChunk {
    data: String, // base64 encoded audio
    mime_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    server_content: Option<GeminiServerContent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiServerContent {
    model_turn: Option<GeminiModelTurn>,
    turn_complete: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GeminiModelTurn {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
    _inline_data: Option<serde_json::Value>,
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

pub struct GeminiProvider {
    api_key: Arc<RwLock<Option<String>>>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            api_key: Arc::new(RwLock::new(None)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
        }
    }

    async fn connect_internal(
        api_key: String,
        app: AppHandle,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
    ) -> Result<(), String> {
        let url = format!(
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent?key={}",
            api_key
        );

        let (ws_stream, _response) = connect_async(&url)
            .await
            .map_err(|e| format!("Failed to connect to Gemini: {}", e))?;

        is_connected.store(true, Ordering::SeqCst);
        log::info!("✅ Connected to Gemini Live API");

        let (mut write, mut read) = ws_stream.split();

        // Send setup message
        let setup = GeminiSetupMessage {
            setup: GeminiSetup {
                model: "models/gemini-2.0-flash-exp".to_string(),
            },
        };
        let setup_json = serde_json::to_string(&setup)
            .map_err(|e| format!("Failed to serialize setup: {}", e))?;
        write
            .send(Message::Text(setup_json))
            .await
            .map_err(|e| format!("Failed to send setup: {}", e))?;

        // Create channel for audio batches
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(100);
        *audio_tx_holder.write() = Some(audio_tx);

        // Spawn task to process and send audio
        let is_connected_send = is_connected.clone();
        tokio::spawn(async move {
            let mut buffer: VecDeque<f32> = VecDeque::with_capacity(8000);
            let batch_size = 320usize; // 20ms @ 16kHz

            loop {
                let result =
                    tokio::time::timeout(std::time::Duration::from_millis(20), audio_rx.recv())
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

                while buffer.len() >= batch_size {
                    if !is_connected_send.load(Ordering::SeqCst) {
                        return;
                    }

                    let chunk: Vec<f32> = buffer.drain(..batch_size).collect();
                    let bytes = Self::f32_to_i16_bytes(&chunk);
                    let base64_data =
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);

                    let input = GeminiRealtimeInput {
                        realtime_input: GeminiMediaChunks {
                            media_chunks: vec![GeminiMediaChunk {
                                data: base64_data,
                                mime_type: "audio/pcm".to_string(),
                            }],
                        },
                    };

                    if let Ok(json) = serde_json::to_string(&input) {
                        if let Err(e) = write.send(Message::Text(json)).await {
                            log::error!("Failed to send audio to Gemini: {}", e);
                            return;
                        }
                    }
                }
            }

            let _ = write.close().await;
        });

        // Spawn task to receive transcriptions
        let is_connected_recv = is_connected.clone();
        let database_recv = database.clone();
        let meeting_id_recv = meeting_id.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if !is_connected_recv.load(Ordering::SeqCst) {
                    break;
                }

                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(response) = serde_json::from_str::<GeminiResponse>(&text) {
                            if let Some(content) = response.server_content {
                                if let Some(turn) = content.model_turn {
                                    for part in turn.parts {
                                        if let Some(transcript_text) = part.text {
                                            if !transcript_text.trim().is_empty() {
                                                let is_final =
                                                    content.turn_complete.unwrap_or(false);
                                                let segment = TranscriptSegment {
                                                    text: transcript_text.clone(),
                                                    is_final,
                                                    confidence: 0.95, // Gemini doesn't provide confidence
                                                    start: 0.0,
                                                    duration: 0.0,
                                                    speaker: None,
                                                };

                                                // Emit to frontend
                                                if let Err(e) =
                                                    app.emit("live_transcript", &segment)
                                                {
                                                    log::error!("Failed to emit transcript: {}", e);
                                                }

                                                // Save FINAL transcripts
                                                if is_final {
                                                    if let Some(db) =
                                                        database_recv.read().as_ref().cloned()
                                                    {
                                                        if let Some(mid) =
                                                            meeting_id_recv.read().as_ref().cloned()
                                                        {
                                                            let text_clone =
                                                                transcript_text.clone();
                                                            tokio::spawn(async move {
                                                                let _ = db
                                                                    .add_transcript(
                                                                        &mid,
                                                                        &text_clone,
                                                                        None,
                                                                        true,
                                                                        0.95,
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
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
            is_connected_recv.store(false, Ordering::SeqCst);
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
impl TranscriptionProvider for GeminiProvider {
    fn start(&self) {
        let api_key = match self.api_key.read().clone() {
            Some(k) => k,
            None => {
                log::warn!("Cannot connect to Gemini: no API key");
                return;
            }
        };

        let app = match self.app_handle.read().clone() {
            Some(a) => a,
            None => {
                log::warn!("Cannot connect to Gemini: no app handle");
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

        tokio::spawn(async move {
            if let Err(e) = Self::connect_internal(
                api_key,
                app,
                is_connected,
                audio_tx_holder,
                database,
                meeting_id,
            )
            .await
            {
                log::error!("Gemini connection failed: {}", e);
            }
        });
    }

    fn stop(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
        log::info!("Gemini disconnected");
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
        *self.api_key.write() = Some(key);
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
