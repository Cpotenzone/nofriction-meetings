// noFriction Meetings - Deepgram Client
// WebSocket streaming transcription with batched audio handling

use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Deepgram response message
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
    word: String,
    start: f64,
    end: f64,
    confidence: f64,
    speaker: Option<u32>,
}

/// Transcript segment for frontend
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
    pub start: f64,
    pub duration: f64,
    pub speaker: Option<String>,
}

/// Audio batch for processing
struct AudioBatch {
    samples: Vec<f32>,
    sample_rate: u32,
}

/// Deepgram client for streaming transcription
pub struct DeepgramClient {
    api_key: Arc<RwLock<Option<String>>>,
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    samples_sent: Arc<AtomicU64>,
    database: Arc<RwLock<Option<Arc<crate::database::DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
}

impl DeepgramClient {
    pub fn new() -> Self {
        Self {
            api_key: Arc::new(RwLock::new(None)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            samples_sent: Arc::new(AtomicU64::new(0)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_api_key(&self, key: String) {
        *self.api_key.write() = Some(key);
    }

    pub fn get_api_key(&self) -> Option<String> {
        self.api_key.read().clone()
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.read().is_some()
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub fn set_app_handle(&self, app: AppHandle) {
        *self.app_handle.write() = Some(app);
    }

    pub fn get_app_handle(&self) -> Option<AppHandle> {
        self.app_handle.read().clone()
    }

    pub fn set_database(&self, db: Arc<crate::database::DatabaseManager>) {
        *self.database.write() = Some(db);
    }

    pub fn set_meeting_id(&self, id: String) {
        *self.meeting_id.write() = Some(id);
    }

    pub fn clear_meeting(&self) {
        *self.meeting_id.write() = None;
    }

    pub fn set_disconnected(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
        log::info!("Deepgram marked as disconnected");
    }

    /// Start connection (spawns async task internally)
    pub fn start_connection(&self) {
        let api_key = match self.get_api_key() {
            Some(k) => k,
            None => {
                log::warn!("Cannot connect to Deepgram: no API key");
                return;
            }
        };

        let app = match self.get_app_handle() {
            Some(a) => a,
            None => {
                log::warn!("Cannot connect to Deepgram: no app handle");
                return;
            }
        };

        if self.is_connected.load(Ordering::SeqCst) {
            log::info!("Already connected to Deepgram");
            return;
        }

        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let samples_sent = self.samples_sent.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();

        tokio::spawn(async move {
            match Self::connect_internal(
                api_key, 
                app, 
                is_connected.clone(), 
                audio_tx_holder, 
                samples_sent,
                database,
                meeting_id,
            ).await {
                Ok(_) => log::info!("Deepgram connection established"),
                Err(e) => log::error!("Deepgram connection failed: {}", e),
            }
        });
    }

    async fn connect_internal(
        api_key: String,
        app: AppHandle,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        samples_sent: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<crate::database::DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
    ) -> Result<(), String> {
        // Use nova-2 model with interim results for faster feedback
        let url = "wss://api.deepgram.com/v1/listen?\
                   model=nova-2&\
                   language=en-US&\
                   smart_format=true&\
                   punctuate=true&\
                   diarize=true&\
                   interim_results=true&\
                   encoding=linear16&\
                   sample_rate=16000&\
                   channels=1";

        let request = http::Request::builder()
            .method("GET")
            .uri(url)
            .header("Authorization", format!("Token {}", api_key))
            .header("Host", "api.deepgram.com")
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
            .body(())
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let (ws_stream, _response) = connect_async(request).await
            .map_err(|e| format!("Failed to connect to Deepgram: {}", e))?;

        is_connected.store(true, Ordering::SeqCst);
        log::info!("✅ Connected to Deepgram WebSocket (nova-2)");

        let (mut write, mut read) = ws_stream.split();

        // Create channel for audio batches - smaller buffer for lower latency
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(100);
        *audio_tx_holder.write() = Some(audio_tx);

        // Spawn task to process and send audio - LOW LATENCY MODE
        let is_connected_send = is_connected.clone();
        tokio::spawn(async move {
            let mut buffer: VecDeque<f32> = VecDeque::with_capacity(8000);
            // Target: 16kHz mono, send every 20ms = 320 samples for low latency
            // Deepgram recommends 20-100ms chunks
            let batch_size = 320usize;
            
            loop {
                // Wait for audio with short timeout for responsiveness
                let result = tokio::time::timeout(
                    std::time::Duration::from_millis(20),
                    audio_rx.recv()
                ).await;

                match result {
                    Ok(Some(batch)) => {
                        // Resample to 16kHz mono
                        let resampled = Self::resample_to_16k_mono(&batch.samples, batch.sample_rate);
                        buffer.extend(resampled);
                    }
                    Ok(None) => break, // Channel closed
                    Err(_) => {} // Timeout - send what we have
                }

                // Send buffered audio immediately when we have enough
                while buffer.len() >= batch_size {
                    if !is_connected_send.load(Ordering::SeqCst) {
                        return;
                    }

                    let chunk: Vec<f32> = buffer.drain(..batch_size).collect();
                    let bytes = Self::f32_to_i16_bytes(&chunk);
                    
                    let count = samples_sent.fetch_add(1, Ordering::Relaxed);
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
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if !is_connected_recv.load(Ordering::SeqCst) {
                    break;
                }

                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(response) = serde_json::from_str::<DeepgramResponse>(&text) {
                            if let Some(channel) = response.channel {
                                if let Some(alt) = channel.alternatives.first() {
                                    if !alt.transcript.is_empty() {
                                        let is_final = response.is_final.unwrap_or(false);
                                        let segment = TranscriptSegment {
                                            text: alt.transcript.clone(),
                                            is_final,
                                            confidence: alt.confidence,
                                            start: response.start.unwrap_or(0.0),
                                            duration: response.duration.unwrap_or(0.0),
                                            speaker: alt.words.as_ref()
                                                .and_then(|w| w.first())
                                                .and_then(|w| w.speaker)
                                                .map(|s| format!("Speaker {}", s)),
                                        };

                                        log::info!("📝 Transcript{}: {}", 
                                            if segment.is_final { " [FINAL]" } else { "" },
                                            segment.text);

                                        // Emit to frontend for live display
                                        if let Err(e) = app.emit("live_transcript", &segment) {
                                            log::error!("Failed to emit transcript: {}", e);
                                        }

                                        // Save FINAL transcripts to database
                                        if is_final {
                                            if let Some(db) = database_recv.read().as_ref().cloned() {
                                                if let Some(mid) = meeting_id_recv.read().as_ref().cloned() {
                                                    let text_clone = alt.transcript.clone();
                                                    let speaker_clone = segment.speaker.clone();
                                                    let confidence = alt.confidence;
                                                    tokio::spawn(async move {
                                                        match db.add_transcript(
                                                            &mid,
                                                            &text_clone,
                                                            speaker_clone.as_deref(),
                                                            true,
                                                            confidence,
                                                        ).await {
                                                            Ok(id) => log::info!("💾 Transcript saved (id: {})", id),
                                                            Err(e) => log::error!("Failed to save transcript: {}", e),
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        log::info!("Deepgram connection closed: {:?}", frame);
                        break;
                    }
                    Err(e) => {
                        log::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            is_connected_recv.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    /// Queue audio samples for sending (non-blocking)
    pub fn queue_audio(&self, samples: &[f32], sample_rate: u32) {
        if !self.is_connected.load(Ordering::SeqCst) {
            return;
        }

        if samples.is_empty() {
            return;
        }

        if let Some(tx) = self.audio_tx.read().as_ref() {
            let batch = AudioBatch {
                samples: samples.to_vec(),
                sample_rate,
            };
            
            // Non-blocking send - drop if full
            if tx.try_send(batch).is_err() {
                // Only log occasionally
                static DROP_COUNT: AtomicU64 = AtomicU64::new(0);
                let count = DROP_COUNT.fetch_add(1, Ordering::Relaxed);
                if count % 100 == 0 {
                    log::warn!("Audio batch dropped (buffer full) #{}", count);
                }
            }
        }
    }

    /// Convert f32 samples to i16 PCM bytes
    fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
        samples.iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * 32767.0) as i16
            })
            .flat_map(|s| s.to_le_bytes())
            .collect()
    }

    /// Resample to 16kHz mono
    fn resample_to_16k_mono(samples: &[f32], from_rate: u32) -> Vec<f32> {
        if samples.is_empty() {
            return vec![];
        }

        // First convert to mono if stereo (assume interleaved)
        let mono: Vec<f32> = if samples.len() > 1 {
            samples.chunks(2)
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

        // Then resample if needed
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

impl Default for DeepgramClient {
    fn default() -> Self {
        Self::new()
    }
}
