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

// ─── Deepgram response types ───────────────────────────────────────────────

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

/// Health status emitted to the frontend
#[derive(Debug, Clone, Serialize)]
struct TranscriptionStatus {
    connected: bool,
    provider: String,
    error: Option<String>,
    reconnect_count: u64,
}

// ─── Internal types ─────────────────────────────────────────────────────────

struct AudioBatch {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

/// Signal from send/receive tasks back to the reconnect loop
enum ConnectionSignal {
    /// WebSocket error — need to reconnect
    Disconnected(String),
    /// Clean shutdown requested by user
    Stopped,
}

// ─── DeepgramProvider ───────────────────────────────────────────────────────

pub struct DeepgramProvider {
    api_key: Arc<RwLock<Option<String>>>,
    /// Whether the user intends transcription to be running (survives reconnects)
    should_run: Arc<AtomicBool>,
    /// Whether the WebSocket is currently connected
    is_connected: Arc<AtomicBool>,
    audio_tx: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    samples_sent: Arc<AtomicU64>,
    reconnect_count: Arc<AtomicU64>,
    database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
    meeting_id: Arc<RwLock<Option<String>>>,
    live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
}

impl DeepgramProvider {
    pub fn new() -> Self {
        Self {
            api_key: Arc::new(RwLock::new(None)),
            should_run: Arc::new(AtomicBool::new(false)),
            is_connected: Arc::new(AtomicBool::new(false)),
            audio_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            samples_sent: Arc::new(AtomicU64::new(0)),
            reconnect_count: Arc::new(AtomicU64::new(0)),
            database: Arc::new(RwLock::new(None)),
            meeting_id: Arc::new(RwLock::new(None)),
            live_intel_agent: Arc::new(RwLock::new(None)),
        }
    }

    /// Emit health status to the frontend
    fn emit_status(app: &AppHandle, connected: bool, error: Option<String>, reconnects: u64) {
        let status = TranscriptionStatus {
            connected,
            provider: "deepgram".to_string(),
            error,
            reconnect_count: reconnects,
        };
        if let Err(e) = app.emit("transcription_status", &status) {
            log::warn!("Failed to emit transcription status: {}", e);
        }
    }

    /// The main reconnect loop — keeps trying to connect as long as should_run is true
    async fn reconnect_loop(
        api_key: String,
        app: AppHandle,
        should_run: Arc<AtomicBool>,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        samples_sent: Arc<AtomicU64>,
        reconnect_count: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) {
        let mut backoff_ms: u64 = 1000; // Start at 1 second
        const MAX_BACKOFF_MS: u64 = 30_000; // Cap at 30 seconds

        while should_run.load(Ordering::SeqCst) {
            // Fetch model from settings
            let model = {
                let state: tauri::State<crate::AppState> = app.state();
                match state.settings.get_deepgram_model().await {
                    Ok(Some(m)) => m,
                    _ => "nova-3".to_string(),
                }
            };

            log::info!("🔗 Connecting to Deepgram (model: {})...", model);
            Self::emit_status(&app, false, None, reconnect_count.load(Ordering::Relaxed));

            match Self::run_session(
                api_key.clone(),
                model,
                app.clone(),
                should_run.clone(),
                is_connected.clone(),
                audio_tx_holder.clone(),
                samples_sent.clone(),
                database.clone(),
                meeting_id.clone(),
                live_intel_agent.clone(),
            )
            .await
            {
                Ok(ConnectionSignal::Stopped) => {
                    log::info!("Deepgram session stopped by user");
                    break;
                }
                Ok(ConnectionSignal::Disconnected(reason)) => {
                    is_connected.store(false, Ordering::SeqCst);
                    *audio_tx_holder.write() = None;

                    if !should_run.load(Ordering::SeqCst) {
                        break;
                    }

                    let n = reconnect_count.fetch_add(1, Ordering::Relaxed) + 1;
                    log::warn!(
                        "⚠️ Deepgram disconnected (attempt #{}): {} — reconnecting in {}ms",
                        n,
                        reason,
                        backoff_ms
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
                Err(e) => {
                    is_connected.store(false, Ordering::SeqCst);
                    *audio_tx_holder.write() = None;

                    if !should_run.load(Ordering::SeqCst) {
                        break;
                    }

                    let n = reconnect_count.fetch_add(1, Ordering::Relaxed) + 1;
                    log::error!(
                        "❌ Deepgram connection error (attempt #{}): {} — retrying in {}ms",
                        n,
                        e,
                        backoff_ms
                    );
                    Self::emit_status(&app, false, Some(format!("Connection error: {}", e)), n);

                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                }
            }
        }

        // Final cleanup
        is_connected.store(false, Ordering::SeqCst);
        *audio_tx_holder.write() = None;
        log::info!("Deepgram reconnect loop exited");
    }

    /// Run a single WebSocket session. Returns when the session ends.
    async fn run_session(
        api_key: String,
        model: String,
        app: AppHandle,
        should_run: Arc<AtomicBool>,
        is_connected: Arc<AtomicBool>,
        audio_tx_holder: Arc<RwLock<Option<mpsc::Sender<AudioBatch>>>>,
        samples_sent: Arc<AtomicU64>,
        database: Arc<RwLock<Option<Arc<DatabaseManager>>>>,
        meeting_id: Arc<RwLock<Option<String>>>,
        live_intel_agent: Arc<RwLock<Option<Arc<RwLock<LiveIntelAgent>>>>>,
    ) -> Result<ConnectionSignal, String> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model={}&language=en-US&smart_format=true&punctuate=true&diarize=true&dictation=true&endpointing=10&utterance_end_ms=1000&vad_events=true&interim_results=true&encoding=linear16&sample_rate=16000&channels=1",
            model
        );

        let request = http::Request::builder()
            .method("GET")
            .uri(&url)
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
        log::info!("✅ Connected to Deepgram WebSocket (model: {})", model);
        Self::emit_status(&app, true, None, 0);

        let (mut write, mut read) = ws_stream.split();

        // Create channel for audio batches — large buffer to survive background throttling
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioBatch>(500);
        *audio_tx_holder.write() = Some(audio_tx);

        // Channel to signal the reconnect loop from send/receive tasks
        let (signal_tx, mut signal_rx) = mpsc::channel::<ConnectionSignal>(2);

        // ─── Audio send task ──────────────────────────────────────────────
        let is_connected_send = is_connected.clone();
        let should_run_send = should_run.clone();
        let signal_tx_send = signal_tx.clone();
        tokio::spawn(async move {
            let mut buffer: VecDeque<f32> = VecDeque::with_capacity(16000);
            let batch_size = 320usize; // 20ms @ 16kHz
            let mut last_send = std::time::Instant::now();
            let mut consecutive_errors: u32 = 0;

            loop {
                if !should_run_send.load(Ordering::SeqCst) {
                    let _ = write.close().await;
                    let _ = signal_tx_send.send(ConnectionSignal::Stopped).await;
                    return;
                }

                // Wait for audio with 50ms timeout (more tolerant of background scheduling)
                let result =
                    tokio::time::timeout(std::time::Duration::from_millis(50), audio_rx.recv())
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
                    Ok(None) => {
                        // Channel closed — provider stopped
                        let _ = write.close().await;
                        let _ = signal_tx_send.send(ConnectionSignal::Stopped).await;
                        return;
                    }
                    Err(_) => {
                        // Timeout — check keepalive and send what we have
                    }
                }

                // Send buffered audio
                while buffer.len() >= batch_size {
                    if !is_connected_send.load(Ordering::SeqCst)
                        || !should_run_send.load(Ordering::SeqCst)
                    {
                        return;
                    }

                    let chunk: Vec<f32> = buffer.drain(..batch_size).collect();
                    let bytes = Self::f32_to_i16_bytes(&chunk);

                    let count = samples_sent.fetch_add(1, Ordering::Relaxed);
                    if count % 50 == 0 {
                        let max_amp = chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        let rms: f32 =
                            (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
                        log::debug!(
                            "🔊 Audio #{}: max_amp={:.4}, rms={:.4}",
                            count,
                            max_amp,
                            rms
                        );
                    }

                    if count % 200 == 0 {
                        log::info!("🎧 Sent audio chunk #{} (buf={})", count, buffer.len());
                    }

                    // Resilient send with retry
                    let mut sent = false;
                    for attempt in 0..3u32 {
                        match write.send(Message::Binary(bytes.clone().into())).await {
                            Ok(_) => {
                                last_send = std::time::Instant::now();
                                consecutive_errors = 0;
                                sent = true;
                                break;
                            }
                            Err(e) => {
                                if attempt < 2 {
                                    log::warn!("Audio send retry {}/3: {}", attempt + 1, e);
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                } else {
                                    log::error!("Audio send failed after 3 attempts: {}", e);
                                    consecutive_errors += 1;
                                }
                            }
                        }
                    }

                    // If we've had too many consecutive failures, signal reconnect
                    if consecutive_errors >= 3 || !sent && consecutive_errors > 0 {
                        log::error!(
                            "🔄 Too many send errors ({}), triggering reconnect",
                            consecutive_errors
                        );
                        let _ = signal_tx_send
                            .send(ConnectionSignal::Disconnected(
                                "persistent send failures".to_string(),
                            ))
                            .await;
                        return;
                    }
                }

                // Deepgram KeepAlive: send the API-level keepalive every 8s of silence
                // (Deepgram ignores WebSocket pings; it requires {"type":"KeepAlive"} text messages)
                if last_send.elapsed() > std::time::Duration::from_secs(8) {
                    let keepalive = serde_json::json!({"type": "KeepAlive"}).to_string();
                    match write.send(Message::Text(keepalive.into())).await {
                        Ok(_) => {
                            last_send = std::time::Instant::now();
                            log::trace!("💓 Deepgram KeepAlive sent");
                        }
                        Err(e) => {
                            log::warn!("KeepAlive send failed: {} — triggering reconnect", e);
                            let _ = signal_tx_send
                                .send(ConnectionSignal::Disconnected(format!(
                                    "keepalive failed: {}",
                                    e
                                )))
                                .await;
                            return;
                        }
                    }
                }
            }
        });

        // ─── Receive task ─────────────────────────────────────────────────
        let is_connected_recv = is_connected.clone();
        let should_run_recv = should_run.clone();
        let signal_tx_recv = signal_tx;
        let database_recv = database.clone();
        let meeting_id_recv = meeting_id.clone();
        let intel_agent_recv = live_intel_agent.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if !is_connected_recv.load(Ordering::SeqCst)
                    || !should_run_recv.load(Ordering::SeqCst)
                {
                    break;
                }

                match msg {
                    Ok(Message::Text(text)) => {
                        // Only log raw for first few or periodically
                        static RAW_LOG_COUNT: AtomicU64 = AtomicU64::new(0);
                        let raw_n = RAW_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
                        if raw_n < 3 || raw_n % 100 == 0 {
                            log::debug!(
                                "🔍 Deepgram raw (#{})]: {}",
                                raw_n,
                                &text[..text.len().min(300)]
                            );
                        }

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
                                            speaker: alt
                                                .words
                                                .as_ref()
                                                .and_then(|w| w.first())
                                                .and_then(|w| w.speaker)
                                                .map(|s| format!("Speaker {}", s)),
                                        };

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

                                        // Save FINAL transcripts to database
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
                    Ok(Message::Pong(_)) => {
                        log::trace!("💓 Keepalive pong received");
                    }
                    Ok(Message::Close(frame)) => {
                        log::info!("Deepgram sent close frame: {:?}", frame);
                        let _ = signal_tx_recv
                            .send(ConnectionSignal::Disconnected(
                                "server closed connection".to_string(),
                            ))
                            .await;
                        break;
                    }
                    Err(e) => {
                        log::error!("WebSocket receive error: {}", e);
                        let _ = signal_tx_recv
                            .send(ConnectionSignal::Disconnected(format!(
                                "receive error: {}",
                                e
                            )))
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
            is_connected_recv.store(false, Ordering::SeqCst);
        });

        // Wait for either task to signal disconnect or stop
        match signal_rx.recv().await {
            Some(signal) => Ok(signal),
            None => Ok(ConnectionSignal::Stopped),
        }
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

        // Prevent duplicate starts
        if self.should_run.load(Ordering::SeqCst) {
            log::info!("Deepgram already running (should_run=true)");
            return;
        }

        // Signal intent to run
        self.should_run.store(true, Ordering::SeqCst);
        self.reconnect_count.store(0, Ordering::Relaxed);
        self.samples_sent.store(0, Ordering::Relaxed);

        let should_run = self.should_run.clone();
        let is_connected = self.is_connected.clone();
        let audio_tx_holder = self.audio_tx.clone();
        let samples_sent = self.samples_sent.clone();
        let reconnect_count = self.reconnect_count.clone();
        let database = self.database.clone();
        let meeting_id = self.meeting_id.clone();
        let live_intel_agent = self.live_intel_agent.clone();

        tokio::spawn(async move {
            Self::reconnect_loop(
                api_key,
                app,
                should_run,
                is_connected,
                audio_tx_holder,
                samples_sent,
                reconnect_count,
                database,
                meeting_id,
                live_intel_agent,
            )
            .await;
        });
    }

    fn stop(&self) {
        // Signal intent to stop — this will cause the reconnect loop to exit
        self.should_run.store(false, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
        *self.audio_tx.write() = None;
        log::info!("Deepgram stop requested (should_run=false)");
    }

    fn process_audio(&self, samples: &[f32], sample_rate: u32, channels: u16) {
        if samples.is_empty() {
            return;
        }

        // Only log connection drops periodically to avoid log spam
        if !self.is_connected.load(Ordering::SeqCst) {
            static DROPPED_COUNT: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let drop_count = DROPPED_COUNT.fetch_add(1, Ordering::Relaxed);
            if drop_count % 500 == 0 {
                log::warn!(
                    "⚠️ Deepgram not connected — dropped {} audio batches (reconnecting...)",
                    drop_count
                );
            }
            return;
        }

        if let Some(tx) = self.audio_tx.read().as_ref() {
            let batch = AudioBatch {
                samples: samples.to_vec(),
                sample_rate,
                channels: if channels == 0 { 1 } else { channels },
            };
            if tx.try_send(batch).is_err() {
                // Buffer full — this batch is dropped but connection stays alive
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
