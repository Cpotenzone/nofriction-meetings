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
use xcap::Monitor;

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
}

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
        }
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

    /// Start meeting capture (full audio + screen, 2s intervals)
    pub fn start_meeting(&self, app: AppHandle) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            // If already running, switch mode and enable audio
            self.set_mode(CaptureMode::Meeting);
            self.audio_enabled.store(true, Ordering::SeqCst);
            *self.frame_interval_ms.write() = 2000; // 2 seconds

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

            log::info!("Switched to Meeting mode (2s intervals, audio enabled)");
            return Ok(());
        }

        // Start fresh in meeting mode
        self.set_mode(CaptureMode::Meeting);
        self.audio_enabled.store(true, Ordering::SeqCst);
        *self.frame_interval_ms.write() = 2000; // 2 seconds

        self.start(app)?;
        log::info!("🎙️ Meeting capture started (audio + screen @ 2s)");
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
        self.video_frame_count.store(0, Ordering::SeqCst);
        self.mic_audio_count.store(0, Ordering::SeqCst);
        self.system_audio_count.store(0, Ordering::SeqCst);
        self.frame_number.store(0, Ordering::SeqCst);
        *self.start_time.write() = Some(std::time::Instant::now());

        // Start microphone capture
        MIC_RUNNING.store(true, Ordering::SeqCst);
        let mic_count = self.mic_audio_count.clone();
        let audio_callback_mic = self.audio_callback.clone();
        let selected_mic = self.selected_mic_id.read().clone();

        std::thread::spawn(move || {
            Self::run_mic_capture(mic_count, audio_callback_mic, selected_mic);
        });

        // Start system audio capture (ScreenCaptureKit)
        // Temporarily disabled to debug permission loop/conflict
        /*
        SYSTEM_AUDIO_RUNNING.store(true, Ordering::SeqCst);
        let sys_count = self.system_audio_count.clone();
        let audio_callback_sys = self.audio_callback.clone();

        std::thread::spawn(move || {
            Self::run_system_audio_capture(sys_count, audio_callback_sys);
        });
        */

        // Start screen capture with configurable interval
        SCREEN_RUNNING.store(true, Ordering::SeqCst);
        let frame_count = self.video_frame_count.clone();
        let frame_number = self.frame_number.clone();
        let frame_callback = self.frame_callback.clone();
        let monitor_id = self.selected_monitor_id.read().clone();
        let interval_ms = *self.frame_interval_ms.read();

        log::info!(
            "Starting screen capture at {}ms interval ({:.1} FPS)",
            interval_ms,
            1000.0 / interval_ms as f32
        );
        tokio::spawn(async move {
            // Delay screen capture startup to prevent permission prompt race with audio
            // This ensures audio permission prompt (system modal) doesn't dismiss/hide screen recording prompt
            // increased to 3s as 1s was not enough
            tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;

            Self::run_screen_capture(
                frame_count,
                frame_number,
                frame_callback,
                monitor_id,
                interval_ms,
            )
            .await;
        });

        log::info!("Capture engine started (mic + system audio + screen capture)");
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
                return;
            }
        };

        // Get default input device from SCK (this is system audio output loopback)
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                log::warn!("⚠️ No system audio device from ScreenCaptureKit");
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

    /// Run screen capture (xcap)
    async fn run_screen_capture(
        frame_count: Arc<AtomicUsize>,
        frame_number: Arc<AtomicU64>,
        frame_callback: Arc<RwLock<Option<FrameCallback>>>,
        monitor_id: Option<u32>,
        interval_ms: u32,
    ) {
        let monitors = match Monitor::all() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Failed to list monitors: {}", e);
                return;
            }
        };

        let monitor = if let Some(id) = monitor_id {
            monitors.into_iter().find(|m| m.id().unwrap_or(0) == id)
        } else {
            monitors
                .into_iter()
                .find(|m| m.is_primary().unwrap_or(false))
        }
        .or_else(|| Monitor::all().ok().and_then(|mut m| m.pop()));

        let monitor = match monitor {
            Some(m) => m,
            None => {
                log::error!("No monitor found for capture");
                return;
            }
        };

        let mon_id = monitor.id().unwrap_or(0);
        let mon_name = monitor.name().unwrap_or_else(|_| "Unknown".to_string());
        let mon_width = monitor.width().unwrap_or(0);
        let mon_height = monitor.height().unwrap_or(0);
        log::info!(
            "📺 Screen capture: {} ({}x{})",
            mon_name,
            mon_width,
            mon_height
        );

        let capture_interval = std::time::Duration::from_millis(interval_ms as u64);

        while SCREEN_RUNNING.load(Ordering::SeqCst) {
            match monitor.capture_image() {
                Ok(image) => {
                    let num = frame_number.fetch_add(1, Ordering::SeqCst);
                    frame_count.fetch_add(1, Ordering::SeqCst);

                    let frame = CapturedFrame {
                        timestamp: chrono::Utc::now(),
                        image: Arc::new(DynamicImage::ImageRgba8(image)),
                        monitor_id: mon_id,
                        frame_number: num,
                    };

                    if let Some(callback) = frame_callback.read().as_ref() {
                        callback(frame);
                    }

                    if num % 10 == 0 {
                        log::trace!("📺 Frame #{}", num);
                    }
                }
                Err(e) => {
                    log::warn!("Frame capture failed: {}", e);
                }
            }

            tokio::time::sleep(capture_interval).await;
        }

        log::info!("📺 Screen capture stopped");
    }

    /// List available audio input devices
    pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
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
