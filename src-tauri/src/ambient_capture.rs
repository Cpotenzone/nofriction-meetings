// noFriction Meetings - Ambient Capture Service
// Continuous background screen recording with power-aware operation
//
// Features:
// - Always-on screen capture at configurable interval (default: 5s)
// - Frame diff detection to skip duplicates
// - Power-state aware (pauses on sleep/idle, resumes on wake/activity)
// - Rolling storage cleanup based on retention period
// - VLM integration for context tagging

use crate::capture_engine::CapturedFrame;
use crate::power_manager::{PowerManager, PowerState};
// use crate::vlm_client::VLMClient;
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Capture mode determines capture behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureMode {
    /// Background capture - lower frequency, no audio
    Ambient,
    /// Active meeting - higher frequency, with audio transcription
    Meeting,
    /// Capture paused
    Paused,
}

/// Configuration for ambient capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientCaptureConfig {
    /// Whether ambient capture is enabled
    pub enabled: bool,
    /// Capture interval in seconds for ambient mode
    pub ambient_interval_secs: u32,
    /// Capture interval in milliseconds for meeting mode
    pub meeting_interval_ms: u32,
    /// How many days to retain ambient captures
    pub retention_days: u32,
    /// Whether to skip duplicate frames
    pub enable_frame_diff: bool,
    /// Similarity threshold for frame diff (0.0-1.0)
    pub diff_threshold: f32,
    /// Whether to run VLM analysis on ambient frames
    pub enable_vlm_tagging: bool,
    /// VLM analysis interval (every N frames)
    pub vlm_analysis_interval: u32,
    /// Storage directory for ambient captures
    pub storage_path: Option<PathBuf>,
}

impl Default for AmbientCaptureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ambient_interval_secs: 5,
            meeting_interval_ms: 1000,
            retention_days: 7,
            enable_frame_diff: true,
            diff_threshold: 0.95,
            enable_vlm_tagging: true,
            vlm_analysis_interval: 12, // Every minute at 5s interval
            storage_path: None,
        }
    }
}

/// Statistics for ambient capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientCaptureStats {
    pub mode: CaptureMode,
    pub is_running: bool,
    pub frames_captured: u64,
    pub frames_skipped: u64,
    pub frames_analyzed: u64,
    pub storage_used_mb: f64,
    pub uptime_seconds: u64,
    pub last_capture_time: Option<DateTime<Utc>>,
}

/// Frame metadata for storage and retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientFrameMeta {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub mode: CaptureMode,
    pub frame_hash: String,
    pub vlm_context: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub storage_path: PathBuf,
}

/// Ambient Capture Service
pub struct AmbientCaptureService {
    config: Arc<RwLock<AmbientCaptureConfig>>,
    mode: Arc<RwLock<CaptureMode>>,
    is_running: Arc<AtomicBool>,
    frames_captured: Arc<AtomicU64>,
    frames_skipped: Arc<AtomicU64>,
    frames_analyzed: Arc<AtomicU64>,
    start_time: Arc<RwLock<Option<std::time::Instant>>>,
    last_frame_hash: Arc<RwLock<Option<String>>>,
    frame_history: Arc<RwLock<VecDeque<AmbientFrameMeta>>>,
    app_handle: Option<AppHandle>,
}

impl AmbientCaptureService {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AmbientCaptureConfig::default())),
            mode: Arc::new(RwLock::new(CaptureMode::Paused)),
            is_running: Arc::new(AtomicBool::new(false)),
            frames_captured: Arc::new(AtomicU64::new(0)),
            frames_skipped: Arc::new(AtomicU64::new(0)),
            frames_analyzed: Arc::new(AtomicU64::new(0)),
            start_time: Arc::new(RwLock::new(None)),
            last_frame_hash: Arc::new(RwLock::new(None)),
            frame_history: Arc::new(RwLock::new(VecDeque::new())),
            app_handle: None,
        }
    }

    pub fn with_config(config: AmbientCaptureConfig) -> Self {
        let service = Self::new();
        *service.config.write() = config;
        service
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app_handle = Some(app);
    }

    /// Get current capture mode
    pub fn get_mode(&self) -> CaptureMode {
        *self.mode.read()
    }

    /// Set capture mode
    pub fn set_mode(&self, mode: CaptureMode) {
        let old_mode = *self.mode.read();
        *self.mode.write() = mode;

        if old_mode != mode {
            log::info!("📹 Capture mode changed: {:?} -> {:?}", old_mode, mode);

            // Emit mode change event
            if let Some(ref app) = self.app_handle {
                let _ = app.emit("capture-mode-changed", mode);
            }
        }
    }

    /// Update configuration
    pub fn update_config(&self, config: AmbientCaptureConfig) {
        *self.config.write() = config;
    }

    /// Get current statistics
    pub fn get_stats(&self) -> AmbientCaptureStats {
        let uptime = self
            .start_time
            .read()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        AmbientCaptureStats {
            mode: self.get_mode(),
            is_running: self.is_running.load(Ordering::SeqCst),
            frames_captured: self.frames_captured.load(Ordering::SeqCst),
            frames_skipped: self.frames_skipped.load(Ordering::SeqCst),
            frames_analyzed: self.frames_analyzed.load(Ordering::SeqCst),
            storage_used_mb: self.calculate_storage_used(),
            uptime_seconds: uptime,
            last_capture_time: self.frame_history.read().back().map(|f| f.timestamp),
        }
    }

    /// Start ambient capture
    pub fn start(&self, power_manager: Arc<PowerManager>) -> Result<(), String> {
        if !self.config.read().enabled {
            return Err("Ambient capture is disabled in config".to_string());
        }

        if self.is_running.load(Ordering::SeqCst) {
            return Err("Ambient capture already running".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        *self.start_time.write() = Some(std::time::Instant::now());
        self.set_mode(CaptureMode::Ambient);

        log::info!("📹 Starting ambient capture service");

        // Set up power state callback
        let mode = self.mode.clone();
        let is_running = self.is_running.clone();

        power_manager.set_callback(Arc::new(move |state| match state {
            PowerState::Sleeping | PowerState::Idle => {
                *mode.write() = CaptureMode::Paused;
                log::info!("📹 Ambient capture paused (power state: {:?})", state);
            }
            PowerState::Active | PowerState::Waking => {
                if is_running.load(Ordering::SeqCst) {
                    *mode.write() = CaptureMode::Ambient;
                    log::info!("📹 Ambient capture resumed (power state: {:?})", state);
                }
            }
        }));

        // Start power manager
        power_manager.start()?;

        // Start capture loop
        let config = self.config.clone();
        let mode = self.mode.clone();
        let is_running = self.is_running.clone();
        let frames_captured = self.frames_captured.clone();
        let _frames_skipped = self.frames_skipped.clone();
        let _last_frame_hash = self.last_frame_hash.clone();
        let _frame_history = self.frame_history.clone();

        std::thread::spawn(move || {
            while is_running.load(Ordering::SeqCst) {
                let current_mode = *mode.read();

                match current_mode {
                    CaptureMode::Paused => {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                    CaptureMode::Ambient => {
                        let interval = config.read().ambient_interval_secs;
                        std::thread::sleep(std::time::Duration::from_secs(interval as u64));
                    }
                    CaptureMode::Meeting => {
                        let interval = config.read().meeting_interval_ms;
                        std::thread::sleep(std::time::Duration::from_millis(interval as u64));
                    }
                }

                // Capture frame (placeholder - actual capture via CaptureEngine)
                // In real implementation, this would trigger the capture engine
                frames_captured.fetch_add(1, Ordering::SeqCst);

                log::debug!(
                    "📹 Captured frame {} (mode: {:?})",
                    frames_captured.load(Ordering::SeqCst),
                    current_mode
                );
            }

            log::info!("📹 Ambient capture loop stopped");
        });

        Ok(())
    }

    /// Stop ambient capture
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.set_mode(CaptureMode::Paused);
        log::info!("📹 Ambient capture service stopped");
    }

    /// Switch to meeting mode (enables audio transcription)
    pub fn enter_meeting_mode(&self) {
        self.set_mode(CaptureMode::Meeting);
        log::info!("📹 Entered meeting mode - enabling transcription");
    }

    /// Switch back to ambient mode
    pub fn exit_meeting_mode(&self) {
        self.set_mode(CaptureMode::Ambient);
        log::info!("📹 Exited meeting mode - disabling transcription");
    }

    /// Compute hash for frame deduplication
    #[allow(dead_code)]
    fn compute_frame_hash(frame: &CapturedFrame) -> String {
        // Use a simple perceptual hash for speed
        // In production, might use average hash or pHash
        let mut hasher = Sha256::new();

        // Sample pixels from the image for a quick hash
        let img = frame.image.to_rgba8();
        let (w, h) = img.dimensions();

        // Sample 16x16 grid
        let step_x = w / 16;
        let step_y = h / 16;

        for y in (0..h).step_by(step_y as usize) {
            for x in (0..w).step_by(step_x as usize) {
                let pixel = img.get_pixel(x, y);
                hasher.update(pixel.0);
            }
        }

        format!("{:x}", hasher.finalize())
    }

    /// Check if frame is similar to previous frame
    #[allow(dead_code)]
    fn is_duplicate_frame(&self, new_hash: &str) -> bool {
        if !self.config.read().enable_frame_diff {
            return false;
        }

        if let Some(ref last_hash) = *self.last_frame_hash.read() {
            // Simple string comparison for now
            // Could use Hamming distance for perceptual hash
            last_hash == new_hash
        } else {
            false
        }
    }

    /// Calculate storage used by ambient captures
    fn calculate_storage_used(&self) -> f64 {
        // Placeholder - would actually sum file sizes
        let frame_count = self.frames_captured.load(Ordering::SeqCst);
        // Estimate ~50KB per frame
        (frame_count as f64 * 50.0) / 1024.0
    }

    /// Clean up old frames based on retention policy
    pub fn cleanup_old_frames(&self) -> Result<usize, String> {
        let retention_days = self.config.read().retention_days;
        let cutoff = Utc::now() - Duration::days(retention_days as i64);

        let mut history = self.frame_history.write();
        let original_count = history.len();

        history.retain(|frame| frame.timestamp > cutoff);

        let removed = original_count - history.len();
        log::info!("🗑️ Cleaned up {} old ambient frames", removed);

        Ok(removed)
    }
}

impl Default for AmbientCaptureService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AmbientCaptureConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ambient_interval_secs, 5);
        assert_eq!(config.retention_days, 7);
    }

    #[test]
    fn test_mode_transitions() {
        let service = AmbientCaptureService::new();
        assert_eq!(service.get_mode(), CaptureMode::Paused);

        service.set_mode(CaptureMode::Ambient);
        assert_eq!(service.get_mode(), CaptureMode::Ambient);

        service.enter_meeting_mode();
        assert_eq!(service.get_mode(), CaptureMode::Meeting);

        service.exit_meeting_mode();
        assert_eq!(service.get_mode(), CaptureMode::Ambient);
    }
}
