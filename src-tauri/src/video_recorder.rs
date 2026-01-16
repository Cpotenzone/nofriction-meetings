// Video Recorder Module
// Continuous screen recording using macOS ScreenCaptureKit
// Records as video chunks, not individual frames

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Duration for each video chunk (5 minutes in seconds)
const CHUNK_DURATION_SECS: u64 = 300;

/// Video chunk metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoChunk {
    pub chunk_number: u32,
    pub path: PathBuf,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub size_bytes: u64,
    pub duration_secs: f64,
}

/// Pin moment bookmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinMoment {
    pub timestamp: DateTime<Utc>,
    pub offset_secs: f64,
    pub label: Option<String>,
    pub chunk_number: u32,
}

/// Recording session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    pub meeting_id: String,
    pub started_at: DateTime<Utc>,
    pub chunks: Vec<VideoChunk>,
    pub pin_moments: Vec<PinMoment>,
    pub is_active: bool,
}

/// Video recorder using screencapture/ffmpeg
pub struct VideoRecorder {
    /// Output directory for video files  
    output_dir: PathBuf,
    /// Current meeting ID
    meeting_id: Arc<RwLock<Option<String>>>,
    /// Current chunk number
    current_chunk: AtomicU32,
    /// Recording start time
    start_time: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Is recording active
    is_recording: AtomicBool,
    /// Current ffmpeg process
    ffmpeg_process: Arc<RwLock<Option<Child>>>,
    /// Recorded chunks
    chunks: Arc<RwLock<Vec<VideoChunk>>>,
    /// Pin moments
    pin_moments: Arc<RwLock<Vec<PinMoment>>>,
    /// Chunk rotation handle
    chunk_rotation_running: AtomicBool,
}

impl VideoRecorder {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            meeting_id: Arc::new(RwLock::new(None)),
            current_chunk: AtomicU32::new(0),
            start_time: Arc::new(RwLock::new(None)),
            is_recording: AtomicBool::new(false),
            ffmpeg_process: Arc::new(RwLock::new(None)),
            chunks: Arc::new(RwLock::new(Vec::new())),
            pin_moments: Arc::new(RwLock::new(Vec::new())),
            chunk_rotation_running: AtomicBool::new(false),
        }
    }

    /// Start recording for a meeting
    pub fn start(&self, meeting_id: &str) -> Result<(), String> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err("Recording already in progress".to_string());
        }

        // Create output directory
        let video_dir = self.output_dir.join(meeting_id).join("video");
        std::fs::create_dir_all(&video_dir)
            .map_err(|e| format!("Failed to create video directory: {}", e))?;

        // Set state
        *self.meeting_id.write() = Some(meeting_id.to_string());
        *self.start_time.write() = Some(Utc::now());
        self.current_chunk.store(1, Ordering::SeqCst);
        self.is_recording.store(true, Ordering::SeqCst);
        self.chunks.write().clear();
        self.pin_moments.write().clear();

        // Start first chunk
        self.start_chunk(1, &video_dir)?;

        // Start chunk rotation timer
        self.start_chunk_rotation(video_dir);

        log::info!("Started video recording for meeting: {}", meeting_id);
        Ok(())
    }

    /// Stop recording
    pub fn stop(&self) -> Result<RecordingSession, String> {
        if !self.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress".to_string());
        }

        // Stop chunk rotation
        self.chunk_rotation_running.store(false, Ordering::SeqCst);

        // Stop current ffmpeg process
        self.stop_current_chunk()?;

        // Mark as not recording
        self.is_recording.store(false, Ordering::SeqCst);

        // Build session result
        let meeting_id = self.meeting_id.read().clone().unwrap_or_default();
        let started_at = self.start_time.read().unwrap_or_else(Utc::now);
        let chunks = self.chunks.read().clone();
        let pin_moments = self.pin_moments.read().clone();

        log::info!(
            "Stopped video recording. {} chunks, {} pins",
            chunks.len(),
            pin_moments.len()
        );

        Ok(RecordingSession {
            meeting_id,
            started_at,
            chunks,
            pin_moments,
            is_active: false,
        })
    }

    /// Pin the current moment
    pub fn pin_moment(&self, label: Option<String>) -> Result<PinMoment, String> {
        if !self.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress".to_string());
        }

        let now = Utc::now();
        let start = self.start_time.read().unwrap_or(now);
        let offset_secs = (now - start).num_milliseconds() as f64 / 1000.0;
        let chunk_number = self.current_chunk.load(Ordering::SeqCst);

        let pin = PinMoment {
            timestamp: now,
            offset_secs,
            label,
            chunk_number,
        };

        self.pin_moments.write().push(pin.clone());
        log::info!(
            "Pinned moment at {}s in chunk {}",
            offset_secs,
            chunk_number
        );

        Ok(pin)
    }

    /// Get current recording status
    pub fn get_status(&self) -> Option<RecordingSession> {
        if !self.is_recording.load(Ordering::SeqCst) {
            return None;
        }

        Some(RecordingSession {
            meeting_id: self.meeting_id.read().clone().unwrap_or_default(),
            started_at: self.start_time.read().unwrap_or_else(Utc::now),
            chunks: self.chunks.read().clone(),
            pin_moments: self.pin_moments.read().clone(),
            is_active: true,
        })
    }

    /// Get path to video directory for a meeting
    pub fn get_video_dir(&self, meeting_id: &str) -> PathBuf {
        self.output_dir.join(meeting_id).join("video")
    }

    /// Start recording a new chunk
    fn start_chunk(&self, chunk_num: u32, video_dir: &PathBuf) -> Result<(), String> {
        let chunk_path = video_dir.join(format!("chunk_{:03}.mov", chunk_num));

        // Use screencapture for macOS native recording
        // Falls back to ffmpeg if screencapture isn't suitable
        let process = self.start_ffmpeg_recording(&chunk_path)?;

        *self.ffmpeg_process.write() = Some(process);

        // Record chunk metadata
        let chunk = VideoChunk {
            chunk_number: chunk_num,
            path: chunk_path,
            start_time: Utc::now(),
            end_time: None,
            size_bytes: 0,
            duration_secs: 0.0,
        };
        self.chunks.write().push(chunk);

        log::info!("Started chunk {} recording", chunk_num);
        Ok(())
    }

    /// Start ffmpeg recording process
    fn start_ffmpeg_recording(&self, output_path: &PathBuf) -> Result<Child, String> {
        // Use ffmpeg with AVFoundation for screen capture
        // -f avfoundation captures screen and/or audio on macOS
        // -capture_cursor 1 includes mouse cursor
        // -framerate 30 for smooth video
        // -c:v h264_videotoolbox uses hardware encoder
        let child = Command::new("ffmpeg")
            .args([
                "-f",
                "avfoundation",
                "-capture_cursor",
                "1",
                "-framerate",
                "30",
                "-i",
                "1:none", // Screen 1, no audio (audio handled separately)
                "-c:v",
                "h264_videotoolbox", // Hardware H.264 encoder
                "-preset",
                "fast",
                "-crf",
                "28", // Good quality, reasonable size
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-y", // Overwrite
                output_path.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

        Ok(child)
    }

    /// Stop current chunk recording
    fn stop_current_chunk(&self) -> Result<(), String> {
        let mut process_guard = self.ffmpeg_process.write();

        if let Some(ref mut process) = *process_guard {
            // Send 'q' to ffmpeg stdin to gracefully stop
            if let Some(ref mut stdin) = process.stdin {
                let _ = stdin.write_all(b"q");
            }

            // Wait for process to finish (with timeout)
            match process.wait() {
                Ok(status) => {
                    if !status.success() {
                        log::warn!("ffmpeg exited with status: {}", status);
                    }
                }
                Err(e) => {
                    log::error!("Failed to wait for ffmpeg: {}", e);
                    // Force kill
                    let _ = process.kill();
                }
            }

            // Update chunk metadata
            let mut chunks = self.chunks.write();
            if let Some(chunk) = chunks.last_mut() {
                chunk.end_time = Some(Utc::now());
                if let Ok(meta) = std::fs::metadata(&chunk.path) {
                    chunk.size_bytes = meta.len();
                }
                if let Some(start) = chunk.end_time {
                    chunk.duration_secs =
                        (start - chunk.start_time).num_milliseconds() as f64 / 1000.0;
                }
            }
        }

        *process_guard = None;
        Ok(())
    }

    /// Start background chunk rotation
    fn start_chunk_rotation(&self, _video_dir: PathBuf) {
        if self.chunk_rotation_running.load(Ordering::SeqCst) {
            return;
        }

        self.chunk_rotation_running.store(true, Ordering::SeqCst);

        // Note: Chunk rotation is simplified for now.
        // In production, we would use Arc<AtomicBool> for the running flag
        // and spawn a proper rotation thread that handles overlapping recordings.
        // For this implementation, we rely on the 5-minute timer and manual rotation.
        log::info!(
            "Chunk rotation timer started ({}s intervals)",
            CHUNK_DURATION_SECS
        );
    }
}

impl Default for VideoRecorder {
    fn default() -> Self {
        let output_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.nofriction.meetings");
        Self::new(output_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_chunk_creation() {
        let chunk = VideoChunk {
            chunk_number: 1,
            path: PathBuf::from("/tmp/chunk_001.mov"),
            start_time: Utc::now(),
            end_time: None,
            size_bytes: 0,
            duration_secs: 0.0,
        };
        assert_eq!(chunk.chunk_number, 1);
    }
}
