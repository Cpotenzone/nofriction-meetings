use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use crate::transcription::TranscriptionProvider;

// Reuse the existing structures from deepgram_client.rs
// (Normally we would import them if they were public, but simpler to redefine or move here)

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    channel: Option<Channel>,
    is_final: Option<bool>,
    start: Option<f64>,
    duration: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    transcript: String,
    confidence: f32,
    words: Option<Vec<Word>>,
}

#[derive(Debug, Deserialize)]
struct Word {
    #[allow(dead_code)]
    word: String,
    #[allow(dead_code)]
    start: f64,
    #[allow(dead_code)]
    end: f64,
    #[allow(dead_code)]
    confidence: f64,
    speaker: Option<u32>,
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
    channels: u16,
}

pub struct DeepgramProvider {
    api_key: Arc<RwLock<Option<String>>>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    samples_sent: Arc<AtomicU64>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
    live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
}

impl DeepgramProvider {
    pub fn new() -> Self {
        Self {
            api_key: Arc::new(RwLock::new(None)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            samples_sent: Arc::new(AtomicU64::new(0)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
            live_intel_agent: Arc::new(RwLock::new(None)),
        }
    }

    async fn connect_internal(
        api_key: String,
        model: String,
        app: AppHandle,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        samples_sent: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) -> Result<(), String> {
        // Use selected model with advanced features
        // Build a clean URL without escape characters
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model={}&language=en-US&smart_format=true&punctuate=true&diarize=true&dictation=true&endpointing=10&utterance_end_ms=1000&vad_events=true&interim_results=true&encoding=linear16&sample_rate=16000&channels=1",
            model
        );

        log::info!("🔗 Deepgram URL: {} (Model: {})", url, model);

        let request = http::Request::builder()
            .method("GET")
            .uri(url)
            .header("Authorization", format!("Token {}", api_key))
            .header("Host", "api.deepgram.com")
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let (ws_stream, _response) = connect_async(request)
            .await
            .map_err(|e| format!("Failed to connect to Deepgram: {}", e))?;

        is_connected.store(true, Ordering::SeqCst);
        log::info!("✅ Connected to Deepgram WebSocket (nova-3)");

        let (mut write, mut read) = ws_stream.split();

        // Create channel for audio batches
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(100);
        *audio_tx_holder.write() = Some(audio_tx);

        // Spawn task to process and send audio
        let is_connected_send = is_connected.clone();
        tokio::spawn(async move {
            let mut buffer: VecDeque<f32> = VecDeque::with_capacity(8000);
            let batch_size = 320usize; // 20ms @ 16kHz

            loop {
                // Wait for audio with short timeout
                let result =
                    tokio::time::timeout(std::time::Duration::from_millis(20), audio_rx.recv())
                        .await;

                match result {
                    Ok(Some(batch)) => {
                        let resampled = Self::resample_to_16k_mono(
                            &batch.samples,
                            batch.sample_rate,
                            batch.channels,
                        );
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

                    // Log audio amplitude for debugging
                    let count = samples_sent.fetch_add(1, Ordering::Relaxed);
                    if count % 50 == 0 {
                        let max_amp = chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        let rms: f32 =
                            (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
                        log::info!(
                            "🔊 Audio #{}: max_amp={:.4}, rms={:.4}",
                            count,
                            max_amp,
                            rms
                        );
                    }

                    let bytes = Self::f32_to_i16_bytes(&chunk);

                    if count % 200 == 0 {
                        log::info!("🎧 Sent audio chunk #{}", count);
                    }

                    if let Err(e) = write.send(Message::Binary(bytes.into())).await {
                        log::error!("Failed to send audio: {}", e);
                        return;
                    }
                }
            }

            let _ = write.close().await;
        });

        // Spawn task to receive transcriptions
        let is_connected_recv = is_connected.clone();
        let database_recv = database.clone();
        let meeting_id_recv = meeting_id.clone();
        let intel_agent_recv = live_intel_agent.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if !is_connected_recv.load(Ordering::SeqCst) {
                    break;
                }

                match msg {
                    Ok(Message::Text(text)) => {
                        // Log raw response for debugging
                        log::info!("🔍 Deepgram raw: {}", &text[..text.len().min(500)]);

                        if let Ok(response) = serde_json::from_str::<DeepgramResponse>(&text) {
                            // Log parsed response structure
                            if response.channel.is_some() {
                                log::info!(
                                    "🎤 Deepgram response: channel present, is_final={:?}",
                                    response.is_final
                                );
                            }

                            if let Some(channel) = response.channel {
                                if let Some(alt) = channel.alternatives.first() {
                                    // Log the transcript text for debugging
                                    log::info!(
                                        "📜 Transcript text: '{}' (len={})",
                                        alt.transcript,
                                        alt.transcript.len()
                                    );

                                    if !alt.transcript.is_empty() {
                                        let is_final = response.is_final.unwrap_or(false);
                                        let segment = TranscriptSegment {
                                            text: alt.transcript.clone(),
                                            is_final,
                                            confidence: alt.confidence,
                                            start: response.start.unwrap_or(0.0),
                                            duration: response.duration.unwrap_or(0.0),
                                            speaker: alt
                                                .words
                                                .as_ref()
                                                .and_then(|w| w.first())
                                                .and_then(|w| w.speaker)
                                                .map(|s| format!("Speaker {}", s)),
                                        };

                                        // Log transcript reception
                                        if is_final {
                                            log::info!("📝 TRANSCRIPT [FINAL]: {}", alt.transcript);
                                        } else {
                                            log::debug!(
                                                "📝 transcript [interim]: {}",
                                                alt.transcript
                                            );
                                        }

                                        // Emit to frontend
                                        if let Err(e) = app.emit("live_transcript", &segment) {
                                            log::error!("Failed to emit transcript: {}", e);
                                        }

                                        // Process with LiveIntelAgent
                                        if let Some(agent) = intel_agent_recv.read().as_ref() {
                                            let mut agent = agent.write();
                                            let intel_segment =
                                                crate::catch_up_agent::TranscriptSegment {
                                                    id: uuid::Uuid::new_v4().to_string(),
                                                    timestamp_ms: (segment.start * 1000.0) as i64,
                                                    speaker: segment.speaker.clone(),
                                                    text: segment.text.clone(),
                                                };
                                            agent.process_segment(intel_segment);
                                        }

                                        // Save FINAL transcripts
                                        if is_final {
                                            if let Some(db) = database_recv.read().as_ref().cloned()
                                            {
                                                if let Some(mid) =
                                                    meeting_id_recv.read().as_ref().cloned()
                                                {
                                                    let text_clone = alt.transcript.clone();
                                                    let speaker_clone = segment.speaker.clone();
                                                    let confidence = alt.confidence;
                                                    tokio::spawn(async move {
                                                        let _ = db
                                                            .add_transcript(
                                                                &mid,
                                                                &text_clone,
                                                                speaker_clone.as_deref(),
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
impl TranscriptionProvider for DeepgramProvider {
    fn start(&self) {
        let api_key = match self.api_key.read().clone() {
            Some(k) => k,
            None => {
                log::warn!("Cannot connect to Deepgram: no API key");
                return;
            }
        };

        let app = match self.app_handle.read().clone() {
            Some(a) => a,
            None => {
                log::warn!("Cannot connect to Deepgram: no app handle");
                return;
            }
        };

        if self.is_connected.load(Ordering::SeqCst) {
            return;
        }

        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let samples_sent = self.samples_sent.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();
        let live_intel_agent = self.live_intel_agent.clone();

        let app_handle_clone = app.clone();

        // Fetch model from settings (needs async)
        tokio::spawn(async move {
            let model = {
                let state: tauri::State<crate::AppState> = app_handle_clone.state();
                match state.settings.get_deepgram_model().await {
                    Ok(Some(m)) => m,
                    _ => "nova-3".to_string(),
                }
            };

            if let Err(e) = Self::connect_internal(
                api_key,
                model,
                app,
                is_connected,
                audio_tx_holder,
                samples_sent,
                database,
                meeting_id,
                live_intel_agent,
            )
            .await
            {
                log::error!("Deepgram connection failed: {}", e);
            }
        });
    }

    fn stop(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
        log::info!("Deepgram disconnected");
    }

    fn process_audio(&self, samples: &[f32], sample_rate: u32, channels: u16) {
        if samples.is_empty() {
            return;
        }

        // Debug logging for first few batches to verify input
        static LOG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
        if count < 5 || count % 200 == 0 {
            log::debug!(
                "Deepgram Audio Input: {} samples, {} Hz, {} channels",
                samples.len(),
                sample_rate,
                channels
            );
        }

        if !self.is_connected.load(Ordering::SeqCst) {
            // Log periodically to help debug connection issues
            static DROPPED_COUNT: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let drop_count = DROPPED_COUNT.fetch_add(1, Ordering::Relaxed);
            if drop_count % 500 == 0 {
                log::warn!(
                    "⚠️ Deepgram not connected - dropped {} audio batches",
                    drop_count
                );
            }
            return;
        }

        if let Some(tx) = self.audio_tx.read().as_ref() {
            let batch = AudioBatch {
                samples: samples.to_vec(),
                sample_rate,
                channels: if channels == 0 { 1 } else { channels }, // Guard against 0 channels
            };
            if tx.try_send(batch).is_err() {
                log::trace!("Audio queue full, batch dropped");
            }
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
        live_intel_agent: Arc<RwLock<LiveIntelAgent>>,
    ) {
        *self.app_handle.write() = Some(app_handle);
        *self.database.write() = Some(database);
        *self.meeting_id.write() = Some(meeting_id);
        *self.live_intel_agent.write() = Some(live_intel_agent);
    }
}
