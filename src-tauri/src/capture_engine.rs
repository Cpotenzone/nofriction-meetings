// noFriction Meetings - Capture Engine v5
// Dual audio capture: Microphone (default host) + System Audio (ScreenCaptureKit host)
// Screen capture via xcap

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use image::DynamicImage;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use xcap::{Monitor, Window};

/// Audio buffer from capture
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp: f64,
    pub source: AudioSource,
}

/// A captured frame with metadata
#[derive(Clone)]
pub struct CapturedFrame {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub image: Arc<DynamicImage>,
    pub monitor_id: u32,
    pub frame_number: u64,
    /// Stable key for the capture source ("display:<id>" / "window:<id>")
    pub source: String,
    /// Human label, e.g. "Built-in Retina Display" or "Zoom — Weekly sync"
    pub label: String,
    /// Owning app for window captures
    pub app_name: Option<String>,
}

/// Something the user chose to capture: a whole display or a single window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureTarget {
    Display { id: u32 },
    Window { id: u32 },
}

impl CaptureTarget {
    pub fn key(&self) -> String {
        match self {
            CaptureTarget::Display { id } => format!("display:{}", id),
            CaptureTarget::Window { id } => format!("window:{}", id),
        }
    }
}

/// A capturable display or window, as listed for the source picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSource {
    pub target: CaptureTarget,
    pub title: String,
    pub app_name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    /// Small JPEG preview as a data URL
    pub thumbnail: Option<String>,
}

/// Apps whose windows are never useful capture targets.
const IGNORED_WINDOW_APPS: &[&str] = &[
    "Window Server",
    "Dock",
    "Control Center",
    "Notification Center",
    "Wallpaper",
    "SystemUIServer",
    "Spotlight",
    "TextInputMenuAgent",
    "loginwindow",
    "noFriction Meetings",
    "nofriction-meetings",
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AudioSource {
    Microphone,
    System,
}

/// Capture mode for Always-On Recording
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureMode {
    /// Ambient mode: screen capture only at longer intervals (30s default), no audio
    Ambient,
    /// Meeting mode: full capture with audio and faster intervals (2s default)
    Meeting,
    /// Paused: no capture
    Paused,
}

impl Default for CaptureMode {
    fn default() -> Self {
        CaptureMode::Paused
    }
}

/// Recording status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub duration_seconds: u64,
    pub video_frames: usize,
    pub audio_samples: usize,
    /// Set when system audio capture failed (usually permission)
    #[serde(default)]
    pub audio_warning: Option<String>,
}

/// Audio device info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_input: bool,
}

/// Monitor info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Audio callback type
pub type AudioCallback = Arc<dyn Fn(AudioBuffer) + Send + Sync>;

/// Frame callback type
pub type FrameCallback = Arc<dyn Fn(CapturedFrame) + Send + Sync>;

// Global state for capture threads
static MIC_RUNNING: AtomicBool = AtomicBool::new(false);
static SYSTEM_AUDIO_RUNNING: AtomicBool = AtomicBool::new(false);
static SCREEN_RUNNING: AtomicBool = AtomicBool::new(false);
/// Recording paused: capture threads stay alive but drop audio and frames,
/// so resume is instant and nothing is captured while paused.
static CAPTURE_PAUSED: AtomicBool = AtomicBool::new(false);
/// Why system audio (other participants' voices) isn't being captured, if it
/// isn't — surfaced in the UI instead of only in the log.
static SYSTEM_AUDIO_ERROR: RwLock<Option<String>> = RwLock::new(None);
const SYSTEM_AUDIO_PERMISSION_HINT: &str = "Other participants' audio isn't being captured — allow noFriction Meetings under System Settings → Privacy & Security → Screen & System Audio Recording, then restart the recording.";
const SCREEN_PERMISSION_ERROR: &str = "Screen Recording permission not granted";

/// PERMISSION SAFEGUARD: without Screen Recording access, every xcap
/// capture_image() call (one per display/window, every tick) makes macOS
/// show its consent dialog again. Only screenshot once access is granted;
/// the explicit prompt lives in the onboarding `request_permission` command.
fn screen_access_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::commands::check_screen_recording_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Main capture engine - dual audio + screen
pub struct CaptureEngine {
    is_running: Arc<AtomicBool>,
    video_frame_count: Arc<AtomicUsize>,
    mic_audio_count: Arc<AtomicUsize>,
    system_audio_count: Arc<AtomicUsize>,
    frame_number: Arc<AtomicU64>,
    start_time: Arc<RwLock<Option<std::time::Instant>>>,
    selected_mic_id: Arc<RwLock<Option<String>>>,
    selected_monitor_id: Arc<RwLock<Option<u32>>>,
    frame_interval_ms: Arc<RwLock<u32>>,
    audio_callback: Arc<RwLock<Option<AudioCallback>>>,
    frame_callback: Arc<RwLock<Option<FrameCallback>>>,
    /// Current capture mode (Always-On Recording)
    capture_mode: Arc<RwLock<CaptureMode>>,
    /// Whether audio capture is enabled (off in Ambient mode)
    audio_enabled: Arc<AtomicBool>,
    /// User-chosen displays/windows; empty = the selected (or primary) display.
    /// Read every tick, so changes apply mid-recording.
    capture_targets: Arc<RwLock<Vec<CaptureTarget>>>,
    /// Per-recording source toggles (from settings)
    mic_enabled: Arc<AtomicBool>,
    system_audio_enabled: Arc<AtomicBool>,
    screen_enabled: Arc<AtomicBool>,
}

impl CaptureEngine {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            video_frame_count: Arc::new(AtomicUsize::new(0)),
            mic_audio_count: Arc::new(AtomicUsize::new(0)),
            system_audio_count: Arc::new(AtomicUsize::new(0)),
            frame_number: Arc::new(AtomicU64::new(0)),
            start_time: Arc::new(RwLock::new(None)),
            selected_mic_id: Arc::new(RwLock::new(None)),
            selected_monitor_id: Arc::new(RwLock::new(None)),
            frame_interval_ms: Arc::new(RwLock::new(1000)), // Default: 1 screenshot per second
            audio_callback: Arc::new(RwLock::new(None)),
            frame_callback: Arc::new(RwLock::new(None)),
            capture_mode: Arc::new(RwLock::new(CaptureMode::Paused)),
            audio_enabled: Arc::new(AtomicBool::new(true)),
            capture_targets: Arc::new(RwLock::new(Vec::new())),
            mic_enabled: Arc::new(AtomicBool::new(true)),
            system_audio_enabled: Arc::new(AtomicBool::new(true)),
            screen_enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Choose which audio/screen sources the next `start()` captures.
    pub fn set_sources(&self, mic: bool, system_audio: bool, screen: bool) {
        self.mic_enabled.store(mic, Ordering::SeqCst);
        self.system_audio_enabled.store(system_audio, Ordering::SeqCst);
        self.screen_enabled.store(screen, Ordering::SeqCst);
    }

    /// Replace the set of displays/windows being captured. Takes effect on
    /// the next capture tick, including during a recording.
    pub fn set_capture_targets(&self, targets: Vec<CaptureTarget>) {
        log::info!("📺 Capture targets: {:?}", targets);
        *self.capture_targets.write() = targets;
    }

    pub fn capture_targets(&self) -> Vec<CaptureTarget> {
        self.capture_targets.read().clone()
    }

    pub fn is_recording(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Pause/resume a running recording without tearing down capture.
    pub fn set_paused(&self, paused: bool) {
        CAPTURE_PAUSED.store(paused, Ordering::SeqCst);
        log::info!("Recording {}", if paused { "paused" } else { "resumed" });
    }

    pub fn is_paused(&self) -> bool {
        CAPTURE_PAUSED.load(Ordering::SeqCst)
    }

    /// Set the audio callback (receives both mic and system audio)
    pub fn set_audio_callback(&self, callback: AudioCallback) {
        *self.audio_callback.write() = Some(callback);
    }

    /// Set the frame callback
    pub fn set_frame_callback(&self, callback: FrameCallback) {
        *self.frame_callback.write() = Some(callback);
    }

    /// Set the frame capture interval in milliseconds
    pub fn set_frame_interval(&self, interval_ms: u32) {
        let clamped = interval_ms.clamp(100, 60000); // 100ms to 60s range
        *self.frame_interval_ms.write() = clamped;
        log::info!(
            "Frame interval set to {}ms ({:.1} FPS)",
            clamped,
            1000.0 / clamped as f32
        );
    }

    /// Get current capture mode
    pub fn get_mode(&self) -> CaptureMode {
        *self.capture_mode.read()
    }

    /// Set capture mode (internal use)
    fn set_mode(&self, mode: CaptureMode) {
        *self.capture_mode.write() = mode;
        log::info!("Capture mode set to: {:?}", mode);
    }

    /// Start ambient capture (screen only, no audio, 30s intervals)
    pub fn start_ambient(&self, app: AppHandle) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            // If already running, just switch mode
            self.set_mode(CaptureMode::Ambient);
            self.audio_enabled.store(false, Ordering::SeqCst);
            *self.frame_interval_ms.write() = 30000; // 30 seconds
            log::info!("Switched to Ambient mode (30s intervals, no audio)");
            return Ok(());
        }

        // Start fresh in ambient mode
        self.set_mode(CaptureMode::Ambient);
        self.audio_enabled.store(false, Ordering::SeqCst);
        *self.frame_interval_ms.write() = 30000; // 30 seconds

        self.start_screen_only(app)?;
        log::info!("🌙 Ambient capture started (screen only @ 30s)");
        Ok(())
    }

    /// Start meeting capture (full audio + screen) at the configured
    /// frame interval (default 1s — the "screenshot every second" objective)
    pub fn start_meeting(&self, app: AppHandle, interval_ms: u32) -> Result<(), String> {
        let interval_ms = interval_ms.clamp(100, 60000);
        if self.is_running.load(Ordering::SeqCst) {
            // If already running, switch mode and enable audio
            self.set_mode(CaptureMode::Meeting);
            self.audio_enabled.store(true, Ordering::SeqCst);
            *self.frame_interval_ms.write() = interval_ms;

            // Start audio capture if not running
            if !MIC_RUNNING.load(Ordering::SeqCst) {
                MIC_RUNNING.store(true, Ordering::SeqCst);
                let mic_count = self.mic_audio_count.clone();
                let audio_callback_mic = self.audio_callback.clone();
                let selected_mic = self.selected_mic_id.read().clone();
                std::thread::spawn(move || {
                    Self::run_mic_capture(mic_count, audio_callback_mic, selected_mic);
                });
            }

            log::info!(
                "Switched to Meeting mode ({}ms intervals, audio enabled)",
                interval_ms
            );
            return Ok(());
        }

        // Start fresh in meeting mode
        self.set_mode(CaptureMode::Meeting);
        self.audio_enabled.store(true, Ordering::SeqCst);
        *self.frame_interval_ms.write() = interval_ms;

        self.start(app)?;
        log::info!(
            "🎙️ Meeting capture started (audio + screen @ {}ms)",
            interval_ms
        );
        Ok(())
    }

    /// Pause capture (stop everything but retain mode)
    pub fn pause(&self) -> Result<(), String> {
        self.set_mode(CaptureMode::Paused);
        self.stop()?;
        log::info!("⏸️ Capture paused");
        Ok(())
    }

    /// Start screen capture only (for ambient mode)
    fn start_screen_only(&self, _app: AppHandle) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err("Already recording".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.video_frame_count.store(0, Ordering::SeqCst);
        self.frame_number.store(0, Ordering::SeqCst);
        *self.start_time.write() = Some(std::time::Instant::now());

        // Start screen capture only
        SCREEN_RUNNING.store(true, Ordering::SeqCst);
        let frame_count = self.video_frame_count.clone();
        let frame_number = self.frame_number.clone();
        let frame_callback = self.frame_callback.clone();
        let monitor_id = self.selected_monitor_id.read().clone();
        let targets = self.capture_targets.clone();
        let interval_ms = *self.frame_interval_ms.read();

        log::info!(
            "Starting ambient screen capture at {}ms interval",
            interval_ms
        );
        tokio::spawn(async move {
            Self::run_screen_capture(
                frame_count,
                frame_number,
                frame_callback,
                monitor_id,
                targets,
                interval_ms,
            )
            .await;
        });

        Ok(())
    }

    pub fn start(&self, _app: AppHandle) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err("Already recording".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        CAPTURE_PAUSED.store(false, Ordering::SeqCst);
        self.video_frame_count.store(0, Ordering::SeqCst);
        self.mic_audio_count.store(0, Ordering::SeqCst);
        self.system_audio_count.store(0, Ordering::SeqCst);
        self.frame_number.store(0, Ordering::SeqCst);
        *self.start_time.write() = Some(std::time::Instant::now());

        let (use_mic, use_sys, use_screen) = (
            self.mic_enabled.load(Ordering::SeqCst),
            self.system_audio_enabled.load(Ordering::SeqCst),
            self.screen_enabled.load(Ordering::SeqCst),
        );

        // Start microphone capture
        if use_mic {
            MIC_RUNNING.store(true, Ordering::SeqCst);
            let mic_count = self.mic_audio_count.clone();
            let audio_callback_mic = self.audio_callback.clone();
            let selected_mic = self.selected_mic_id.read().clone();

            std::thread::spawn(move || {
                Self::run_mic_capture(mic_count, audio_callback_mic, selected_mic);
            });
        }

        // Start system audio capture (ScreenCaptureKit)
        *SYSTEM_AUDIO_ERROR.write() = None;
        if use_sys {
            SYSTEM_AUDIO_RUNNING.store(true, Ordering::SeqCst);
            let sys_count = self.system_audio_count.clone();
            let audio_callback_sys = self.audio_callback.clone();

            std::thread::spawn(move || {
                Self::run_system_audio_capture(sys_count, audio_callback_sys);
            });
        }

        // Start screen capture with configurable interval
        if use_screen {
            SCREEN_RUNNING.store(true, Ordering::SeqCst);
            let frame_count = self.video_frame_count.clone();
            let frame_number = self.frame_number.clone();
            let frame_callback = self.frame_callback.clone();
            let monitor_id = self.selected_monitor_id.read().clone();
            let targets = self.capture_targets.clone();
            let interval_ms = *self.frame_interval_ms.read();

            log::info!(
                "Starting screen capture at {}ms interval ({:.1} FPS)",
                interval_ms,
                1000.0 / interval_ms as f32
            );
            tokio::spawn(async move {
                // Short delay to prevent permission prompt race - 500ms is sufficient
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                Self::run_screen_capture(
                    frame_count,
                    frame_number,
                    frame_callback,
                    monitor_id,
                    targets,
                    interval_ms,
                )
                .await;
            });
        }

        log::info!(
            "Capture engine started (mic: {}, system audio: {}, screen: {})",
            use_mic,
            use_sys,
            use_screen
        );
        Ok(())
    }

    /// Stop capture
    pub fn stop(&self) -> Result<(), String> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err("Not recording".to_string());
        }

        self.is_running.store(false, Ordering::SeqCst);
        MIC_RUNNING.store(false, Ordering::SeqCst);
        SYSTEM_AUDIO_RUNNING.store(false, Ordering::SeqCst);
        SCREEN_RUNNING.store(false, Ordering::SeqCst);

        log::info!("Capture engine stopped");
        Ok(())
    }

    /// Get current recording status
    pub fn get_status(&self) -> RecordingStatus {
        let duration = self
            .start_time
            .read()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        RecordingStatus {
            is_recording: self.is_running.load(Ordering::SeqCst),
            duration_seconds: duration,
            video_frames: self.video_frame_count.load(Ordering::SeqCst),
            audio_samples: self.mic_audio_count.load(Ordering::SeqCst)
                + self.system_audio_count.load(Ordering::SeqCst),
            audio_warning: SYSTEM_AUDIO_ERROR.read().clone(),
        }
    }

    /// Run microphone capture (default cpal host)
    fn run_mic_capture(
        mic_count: Arc<AtomicUsize>,
        callback: Arc<RwLock<Option<AudioCallback>>>,
        selected_mic: Option<String>,
    ) {
        let host = cpal::default_host();

        let device = if let Some(ref mic_id) = selected_mic {
            host.input_devices()
                .ok()
                .and_then(|mut devs| devs.find(|d| d.name().map(|n| n == *mic_id).unwrap_or(false)))
                .or_else(|| {
                    log::warn!("Selected mic '{}' not found, using default", mic_id);
                    host.default_input_device()
                })
        } else {
            host.default_input_device()
        };

        let device = match device {
            Some(d) => d,
            None => {
                log::warn!("No microphone found");
                return;
            }
        };

        let device_name = device.name().unwrap_or_default();
        log::info!("🎤 Microphone: {}", device_name);

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Mic config error: {}", e);
                return;
            }
        };

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        log::info!("🎤 Config: {}Hz, {} channels", sample_rate, channels);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !MIC_RUNNING.load(Ordering::SeqCst) {
                    return;
                }

                let n = mic_count.fetch_add(1, Ordering::Relaxed);

                if CAPTURE_PAUSED.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(cb) = callback.read().as_ref() {
                    let audio = AudioBuffer {
                        samples: data.to_vec(),
                        sample_rate,
                        channels,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64(),
                        source: AudioSource::Microphone,
                    };
                    cb(audio);
                }

                if n % 100 == 0 {
                    log::trace!("🎤 Mic #{}", n);
                }
            },
            |err| log::error!("Mic error: {}", err),
            None,
        );

        match stream {
            Ok(s) => {
                if let Err(e) = s.play() {
                    log::error!("Failed to play mic stream: {}", e);
                    return;
                }
                log::info!("✅ Microphone capture started");

                while MIC_RUNNING.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }

                log::info!("🎤 Microphone capture stopped");
            }
            Err(e) => {
                log::error!("Failed to build mic stream: {}", e);
            }
        }
    }

    /// Run system audio capture (ScreenCaptureKit host)
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn run_system_audio_capture(
        sys_count: Arc<AtomicUsize>,
        callback: Arc<RwLock<Option<AudioCallback>>>,
    ) {
        // Try to get the ScreenCaptureKit host with retry
        let sck_host = Self::get_sck_host_with_retry(3);

        let host = match sck_host {
            Ok(h) => h,
            Err(e) => {
                log::warn!("⚠️ ScreenCaptureKit not available: {}", e);
                log::warn!("⚠️ System audio capture disabled. Only microphone will be captured.");
                *SYSTEM_AUDIO_ERROR.write() = Some(SYSTEM_AUDIO_PERMISSION_HINT.to_string());
                return;
            }
        };

        // Get default input device from SCK (this is system audio output loopback)
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                log::warn!("⚠️ No system audio device from ScreenCaptureKit");
                *SYSTEM_AUDIO_ERROR.write() = Some(SYSTEM_AUDIO_PERMISSION_HINT.to_string());
                return;
            }
        };

        let device_name = device.name().unwrap_or_default();
        log::info!("🔊 System Audio: {}", device_name);

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                log::error!("System audio config error: {}", e);
                return;
            }
        };

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        log::info!("🔊 Config: {}Hz, {} channels", sample_rate, channels);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !SYSTEM_AUDIO_RUNNING.load(Ordering::SeqCst) {
                    return;
                }

                let n = sys_count.fetch_add(1, Ordering::Relaxed);

                if CAPTURE_PAUSED.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(cb) = callback.read().as_ref() {
                    let audio = AudioBuffer {
                        samples: data.to_vec(),
                        sample_rate,
                        channels,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64(),
                        source: AudioSource::System,
                    };
                    cb(audio);
                }

                if n % 100 == 0 {
                    log::trace!("🔊 System audio #{}", n);
                }
            },
            |err| log::error!("System audio error: {}", err),
            None,
        );

        match stream {
            Ok(s) => {
                if let Err(e) = s.play() {
                    log::error!("Failed to play system audio stream: {}", e);
                    return;
                }
                log::info!("✅ System audio capture started");

                while SYSTEM_AUDIO_RUNNING.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }

                log::info!("🔊 System audio capture stopped");
            }
            Err(e) => {
                log::error!("Failed to build system audio stream: {}", e);
            }
        }
    }

    /// Fallback for non-macOS (no system audio)
    #[cfg(not(target_os = "macos"))]
    fn run_system_audio_capture(
        _sys_count: Arc<AtomicUsize>,
        _callback: Arc<RwLock<Option<AudioCallback>>>,
    ) {
        log::info!("System audio capture not available on this platform");
    }

    /// Get ScreenCaptureKit host with retry (it can be flaky)
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_sck_host_with_retry(max_retries: usize) -> Result<cpal::Host, String> {
        use rand::Rng;

        let mut retries = 0;
        let mut delay_ms = 50u64;

        loop {
            match cpal::host_from_id(cpal::HostId::ScreenCaptureKit) {
                Ok(host) => return Ok(host),
                Err(e) => {
                    if retries >= max_retries {
                        return Err(format!(
                            "ScreenCaptureKit host failed after {} retries: {}",
                            max_retries, e
                        ));
                    }

                    // Add jitter
                    let jitter = rand::rng().random_range(0..20) as u64;
                    let delay = std::time::Duration::from_millis(delay_ms + jitter);

                    log::warn!(
                        "ScreenCaptureKit retry {} in {}ms: {}",
                        retries + 1,
                        delay_ms + jitter,
                        e
                    );
                    std::thread::sleep(delay);

                    retries += 1;
                    delay_ms = std::cmp::min(delay_ms * 2, 500);
                }
            }
        }
    }

    /// Run screen capture (xcap). Each tick captures every chosen target;
    /// with no explicit targets it falls back to the selected/primary display.
    async fn run_screen_capture(
        frame_count: Arc<AtomicUsize>,
        frame_number: Arc<AtomicU64>,
        frame_callback: Arc<RwLock<Option<FrameCallback>>>,
        monitor_id: Option<u32>,
        targets: Arc<RwLock<Vec<CaptureTarget>>>,
        interval_ms: u32,
    ) {
        let capture_interval = std::time::Duration::from_millis(interval_ms as u64);
        let mut last_logged: Option<Vec<CaptureTarget>> = None;
        let mut missing_logged: std::collections::HashSet<String> = Default::default();

        while SCREEN_RUNNING.load(Ordering::SeqCst) {
            if CAPTURE_PAUSED.load(Ordering::Relaxed) {
                tokio::time::sleep(capture_interval).await;
                continue;
            }
            let mut current = targets.read().clone();
            if current.is_empty() {
                match Self::default_display(monitor_id) {
                    Some(id) => current.push(CaptureTarget::Display { id }),
                    None => {
                        log::error!("No monitor found for capture");
                        tokio::time::sleep(capture_interval).await;
                        continue;
                    }
                }
            }
            if last_logged.as_ref() != Some(&current) {
                log::info!("📺 Screen capture sources: {:?}", current);
                last_logged = Some(current.clone());
            }

            // xcap calls block on CoreGraphics; keep them off the async workers
            let captured = tokio::task::spawn_blocking(move || {
                current
                    .iter()
                    .map(|t| (t.clone(), Self::capture_target(t)))
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();

            let timestamp = chrono::Utc::now();
            for (target, result) in captured {
                match result {
                    Ok((image, label, app_name, mon_id)) => {
                        missing_logged.remove(&target.key());
                        let num = frame_number.fetch_add(1, Ordering::SeqCst);
                        frame_count.fetch_add(1, Ordering::SeqCst);
                        let frame = CapturedFrame {
                            timestamp,
                            image: Arc::new(image),
                            monitor_id: mon_id,
                            frame_number: num,
                            source: target.key(),
                            label,
                            app_name,
                        };
                        if let Some(callback) = frame_callback.read().as_ref() {
                            callback(frame);
                        }
                    }
                    Err(e) => {
                        // Closed/minimized windows are expected; log once per source
                        if missing_logged.insert(target.key()) {
                            log::warn!("Capture of {} unavailable: {}", target.key(), e);
                        }
                    }
                }
            }

            tokio::time::sleep(capture_interval).await;
        }

        log::info!("📺 Screen capture stopped");
    }

    fn default_display(preferred: Option<u32>) -> Option<u32> {
        let monitors = Monitor::all().ok()?;
        preferred
            .and_then(|id| monitors.iter().find(|m| m.id().ok() == Some(id)))
            .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
            .or_else(|| monitors.first())
            .and_then(|m| m.id().ok())
    }

    /// Capture one target. Returns (image, label, app_name, monitor_id).
    pub fn capture_target(
        target: &CaptureTarget,
    ) -> Result<(DynamicImage, String, Option<String>, u32), String> {
        if !screen_access_granted() {
            return Err(SCREEN_PERMISSION_ERROR.to_string());
        }
        match target {
            CaptureTarget::Display { id } => {
                let monitor = Monitor::all()
                    .map_err(|e| format!("Failed to list monitors: {}", e))?
                    .into_iter()
                    .find(|m| m.id().ok() == Some(*id))
                    .ok_or_else(|| "display not connected".to_string())?;
                let image = monitor.capture_image().map_err(|e| e.to_string())?;
                let label = monitor.name().unwrap_or_else(|_| format!("Display {}", id));
                Ok((DynamicImage::ImageRgba8(image), label, None, *id))
            }
            CaptureTarget::Window { id } => {
                let window = Window::all()
                    .map_err(|e| format!("Failed to list windows: {}", e))?
                    .into_iter()
                    .find(|w| w.id().ok() == Some(*id))
                    .ok_or_else(|| "window closed".to_string())?;
                if window.is_minimized().unwrap_or(false) {
                    return Err("window minimized".to_string());
                }
                let image = window.capture_image().map_err(|e| e.to_string())?;
                let app = window.app_name().ok().filter(|a| !a.is_empty());
                let title = window.title().unwrap_or_default();
                let label = match (&app, title.is_empty()) {
                    (Some(a), false) => format!("{} — {}", a, title),
                    (Some(a), true) => a.clone(),
                    (None, _) => title,
                };
                let mon_id = window
                    .current_monitor()
                    .ok()
                    .and_then(|m| m.id().ok())
                    .unwrap_or(0);
                Ok((DynamicImage::ImageRgba8(image), label, app, mon_id))
            }
        }
    }

    /// Displays and on-screen windows the user can choose to capture.
    /// Blocking (CoreGraphics); call from a blocking context.
    pub fn list_capture_sources(with_thumbnails: bool) -> Result<Vec<CaptureSource>, String> {
        let with_thumbnails = with_thumbnails && screen_access_granted();
        let mut sources = Vec::new();

        for m in Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))? {
            let Ok(id) = m.id() else { continue };
            let thumbnail = if with_thumbnails {
                m.capture_image().ok().and_then(|img| thumbnail_data_url(&DynamicImage::ImageRgba8(img)))
            } else {
                None
            };
            sources.push(CaptureSource {
                target: CaptureTarget::Display { id },
                title: m.name().unwrap_or_else(|_| format!("Display {}", id)),
                app_name: None,
                width: m.width().unwrap_or(0),
                height: m.height().unwrap_or(0),
                is_primary: m.is_primary().unwrap_or(false),
                thumbnail,
            });
        }

        let windows = Window::all().map_err(|e| format!("Failed to list windows: {}", e))?;
        let mut listed = 0;
        for w in windows {
            // Cap after filtering, so helper/menu-bar windows can't crowd out
            // real ones (and we don't screenshot dozens just for thumbnails)
            if listed >= 40 {
                break;
            }
            let (Ok(id), Ok(width), Ok(height)) = (w.id(), w.width(), w.height()) else {
                continue;
            };
            let app = w.app_name().unwrap_or_default();
            let title = w.title().unwrap_or_default();
            if width < 200
                || height < 120
                || app.is_empty()
                || w.is_minimized().unwrap_or(false)
                || IGNORED_WINDOW_APPS.iter().any(|a| a.eq_ignore_ascii_case(&app))
            {
                continue;
            }
            let thumbnail = if with_thumbnails {
                w.capture_image().ok().and_then(|img| thumbnail_data_url(&DynamicImage::ImageRgba8(img)))
            } else {
                None
            };
            listed += 1;
            sources.push(CaptureSource {
                target: CaptureTarget::Window { id },
                title: if title.is_empty() { app.clone() } else { title },
                app_name: Some(app),
                width,
                height,
                is_primary: false,
                thumbnail,
            });
        }

        Ok(sources)
    }

    /// List available audio input devices
    pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
        // PERMISSION SAFEGUARD: Only list devices if permission is already granted.
        // This prevents the macOS "infinite prompt loop" on app startup.
        #[cfg(target_os = "macos")]
        {
            if !crate::commands::check_microphone_permission() {
                return Ok(Vec::new());
            }
        }

        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        let devices: Vec<AudioDevice> = host
            .input_devices()
            .map_err(|e| format!("Failed to enumerate devices: {}", e))?
            .filter_map(|device| {
                let name = device.name().ok()?;
                Some(AudioDevice {
                    id: name.clone(),
                    name: name.clone(),
                    is_default: default_name.as_ref().map(|n| n == &name).unwrap_or(false),
                    is_input: true,
                })
            })
            .collect();

        // System audio devices (SCK) are not listed to avoid triggering permission prompts.
        // System audio capture will use the default loopback if enabled.

        Ok(devices)
    }

    /// List available monitors
    pub fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
        let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;

        let infos: Vec<MonitorInfo> = monitors
            .into_iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let id = m.id().ok()?;
                let name = m.name().unwrap_or_else(|_| format!("Display {}", i + 1));
                let width = m.width().ok()?;
                let height = m.height().ok()?;
                let is_primary = m.is_primary().unwrap_or(i == 0);

                Some(MonitorInfo {
                    id,
                    name,
                    width,
                    height,
                    is_primary,
                })
            })
            .collect();

        Ok(infos)
    }

    /// Capture a single screenshot
    pub fn capture_screenshot(monitor_id: Option<u32>) -> Result<DynamicImage, String> {
        if !screen_access_granted() {
            return Err(SCREEN_PERMISSION_ERROR.to_string());
        }
        let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;

        let monitor = if let Some(id) = monitor_id {
            monitors
                .into_iter()
                .find(|m| m.id().unwrap_or(0) == id)
                .ok_or_else(|| "Monitor not found".to_string())?
        } else {
            monitors
                .into_iter()
                .next()
                .ok_or_else(|| "No monitors available".to_string())?
        };

        let image = monitor
            .capture_image()
            .map_err(|e| format!("Failed to capture: {}", e))?;

        Ok(DynamicImage::ImageRgba8(image))
    }

    /// Set selected microphone
    pub fn set_microphone(&self, device_id: String) {
        *self.selected_mic_id.write() = Some(device_id);
    }

    /// Set selected monitor
    pub fn set_monitor(&self, monitor_id: u32) {
        *self.selected_monitor_id.write() = Some(monitor_id);
    }
}

impl Default for CaptureEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Downscale to a ~320px-wide JPEG data URL for picker previews.
fn thumbnail_data_url(image: &DynamicImage) -> Option<String> {
    use base64::Engine;
    let thumb = image.thumbnail(320, 200).to_rgb8();
    let mut bytes = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .ok()?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}
