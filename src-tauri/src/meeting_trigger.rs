// noFriction Meetings - Meeting Trigger Engine
// Determines when to activate transcription based on calendar, app detection, or manual trigger
//
// Features:
// - Calendar event matching (±5 min window)
// - Meeting app detection (Zoom, Meet, Teams)
// - Manual button/hotkey triggers
// - Optional audio pattern detection (VAD)

use crate::calendar_client::{CalendarClient, CalendarEventNative};

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Meeting trigger source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerSource {
    /// Triggered by calendar event match
    Calendar,
    /// Triggered by meeting app detection
    AppDetection,
    /// Triggered by manual button click
    Manual,
    /// Triggered by audio pattern (VAD)
    AudioPattern,
    /// Triggered by hotkey
    Hotkey,
}

/// Active meeting session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub trigger_source: TriggerSource,
    pub calendar_event: Option<CalendarEventNative>,
    pub detected_app: Option<String>,
    pub expected_end: Option<DateTime<Utc>>,
    pub is_active: bool,
}

/// Meeting detection (suggestion before recording)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDetection {
    pub id: String,
    pub detected_at: DateTime<Utc>,
    pub source: TriggerSource,
    pub app_name: Option<String>,
    pub calendar_event: Option<CalendarEventNative>,
    pub is_using_audio: bool,
    pub is_screen_sharing: bool,
}

/// Callback for meeting detection suggestions
pub type DetectionCallback = Arc<dyn Fn(MeetingDetection) + Send + Sync>;

/// Configuration for meeting triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTriggerConfig {
    /// Enable calendar-based triggering
    pub calendar_trigger_enabled: bool,
    /// Minutes before/after calendar event to trigger
    pub calendar_window_minutes: i64,
    /// Enable app detection triggering
    pub app_trigger_enabled: bool,
    /// List of meeting apps to detect
    pub meeting_apps: Vec<String>,
    /// Enable audio pattern detection
    pub audio_trigger_enabled: bool,
    /// Minimum seconds of conversation to trigger
    pub audio_min_conversation_secs: u32,
}

impl Default for MeetingTriggerConfig {
    fn default() -> Self {
        Self {
            calendar_trigger_enabled: true,
            calendar_window_minutes: 5,
            app_trigger_enabled: true,
            meeting_apps: vec![
                "zoom.us".to_string(),
                "Zoom".to_string(),
                "Google Meet".to_string(),
                "Microsoft Teams".to_string(),
                "Slack".to_string(),
                "Discord".to_string(),
                "FaceTime".to_string(),
                "Webex".to_string(),
            ],
            audio_trigger_enabled: false,
            audio_min_conversation_secs: 30,
        }
    }
}

/// Trigger event callback
pub type TriggerCallback = Arc<dyn Fn(MeetingSession) + Send + Sync>;
pub type EndCallback = Arc<dyn Fn(MeetingSession) + Send + Sync>;

/// Meeting Trigger Engine
pub struct MeetingTriggerEngine {
    config: Arc<RwLock<MeetingTriggerConfig>>,
    is_running: Arc<AtomicBool>,
    current_session: Arc<RwLock<Option<MeetingSession>>>,
    on_meeting_start: Arc<RwLock<Option<TriggerCallback>>>,
    on_meeting_end: Arc<RwLock<Option<EndCallback>>>,
    on_meeting_detected: Arc<RwLock<Option<DetectionCallback>>>,
    app_handle: Option<AppHandle>,
    /// Track which detections user has dismissed (don't re-suggest)
    dismissed_detections: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl MeetingTriggerEngine {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(MeetingTriggerConfig::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            current_session: Arc::new(RwLock::new(None)),
            on_meeting_start: Arc::new(RwLock::new(None)),
            on_meeting_end: Arc::new(RwLock::new(None)),
            on_meeting_detected: Arc::new(RwLock::new(None)),
            app_handle: None,
            dismissed_detections: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// Set callback for meeting detection suggestions
    pub fn on_meeting_detected(&self, callback: DetectionCallback) {
        *self.on_meeting_detected.write() = Some(callback);
    }

    /// Dismiss a detection (user said no)
    pub fn dismiss_detection(&self, detection_id: &str) {
        self.dismissed_detections
            .write()
            .insert(detection_id.to_string());
        log::info!("🎯 Detection dismissed: {}", detection_id);
    }

    /// Clear dismissed detections (for new day)
    pub fn clear_dismissed(&self) {
        self.dismissed_detections.write().clear();
    }

    pub fn with_config(config: MeetingTriggerConfig) -> Self {
        let engine = Self::new();
        *engine.config.write() = config;
        engine
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app_handle = Some(app);
    }

    /// Set callback for when meeting starts
    pub fn on_meeting_start(&self, callback: TriggerCallback) {
        *self.on_meeting_start.write() = Some(callback);
    }

    /// Set callback for when meeting ends
    pub fn on_meeting_end(&self, callback: EndCallback) {
        *self.on_meeting_end.write() = Some(callback);
    }

    /// Get current active session
    pub fn get_current_session(&self) -> Option<MeetingSession> {
        self.current_session.read().clone()
    }

    /// Check if a meeting is currently active
    pub fn is_meeting_active(&self) -> bool {
        self.current_session
            .read()
            .as_ref()
            .map(|s| s.is_active)
            .unwrap_or(false)
    }

    /// Start monitoring for meeting triggers
    pub fn start(&self, calendar_client: Arc<RwLock<CalendarClient>>) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err("Meeting trigger engine already running".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        log::info!("🎯 Meeting trigger engine started");

        // Start calendar monitoring loop
        let is_running = self.is_running.clone();
        let config = self.config.clone();
        let current_session = self.current_session.clone();
        let _on_start = self.on_meeting_start.clone();
        let on_end = self.on_meeting_end.clone();
        let app_handle_cal = self.app_handle.clone();
        let dismissed_cal = self.dismissed_detections.clone();

        std::thread::spawn(move || {
            while is_running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(30)); // Check every 30s

                let cfg = config.read().clone();

                // Check calendar trigger
                if cfg.calendar_trigger_enabled {
                    if let Some(event) =
                        Self::check_calendar_trigger(&calendar_client, cfg.calendar_window_minutes)
                    {
                        // Only suggest if no active session
                        let session_exists = current_session.read().is_some();
                        let detection_id = format!("cal-{}", event.event_id);

                        // Skip if already dismissed or recording
                        if !session_exists && !dismissed_cal.read().contains(&detection_id) {
                            log::info!("📅 Calendar meeting detected: {}", event.title);

                            // Emit detection event to frontend for user confirmation
                            if let Some(ref app) = app_handle_cal {
                                let detection = MeetingDetection {
                                    id: detection_id,
                                    detected_at: Utc::now(),
                                    source: TriggerSource::Calendar,
                                    app_name: None,
                                    calendar_event: Some(event.clone()),
                                    is_using_audio: false,
                                    is_screen_sharing: false,
                                };
                                let _ = app.emit("meeting-detected", detection);
                            }
                        }
                    }
                }

                // Check if current meeting should end (calendar event ended)
                if let Some(session) = current_session.read().clone() {
                    if let Some(expected_end) = session.expected_end {
                        if Utc::now() > expected_end + Duration::minutes(5) {
                            log::info!("🎯 Meeting ended (past expected end time)");

                            if let Some(ref cb) = *on_end.read() {
                                cb(session.clone());
                            }

                            *current_session.write() = None;
                        }
                    }
                }
            }

            log::info!("🎯 Meeting trigger engine loop stopped");
        });

        // Start app detection loop (separate thread)
        let is_running_app = self.is_running.clone();
        let config_app = self.config.clone();
        let current_session_app = self.current_session.clone();
        let on_start_app = self.on_meeting_start.clone();

        std::thread::spawn(move || {
            while is_running_app.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(5)); // Check every 5s

                let cfg = config_app.read().clone();

                if !cfg.app_trigger_enabled {
                    continue;
                }

                // Check if meeting app is frontmost
                if let Some(app_name) = Self::get_frontmost_meeting_app(&cfg.meeting_apps) {
                    let session_exists = current_session_app.read().is_some();

                    if !session_exists {
                        let session = MeetingSession {
                            id: uuid::Uuid::new_v4().to_string(),
                            started_at: Utc::now(),
                            trigger_source: TriggerSource::AppDetection,
                            calendar_event: None,
                            detected_app: Some(app_name.clone()),
                            expected_end: None,
                            is_active: true,
                        };

                        log::info!("🎯 Meeting triggered by app detection: {}", app_name);
                        *current_session_app.write() = Some(session.clone());

                        if let Some(ref cb) = *on_start_app.read() {
                            cb(session);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop monitoring
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        log::info!("🎯 Meeting trigger engine stopped");
    }

    /// Manually start a meeting session
    pub fn start_manual_meeting(&self) -> MeetingSession {
        let session = MeetingSession {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            trigger_source: TriggerSource::Manual,
            calendar_event: None,
            detected_app: None,
            expected_end: None,
            is_active: true,
        };

        log::info!("🎯 Meeting started manually");
        *self.current_session.write() = Some(session.clone());

        if let Some(ref app) = self.app_handle {
            let _ = app.emit("meeting-started", session.clone());
        }

        if let Some(ref cb) = *self.on_meeting_start.read() {
            cb(session.clone());
        }

        session
    }

    /// Manually end the current meeting
    pub fn end_meeting(&self) -> Option<MeetingSession> {
        let session = self.current_session.write().take();

        if let Some(ref s) = session {
            log::info!("🎯 Meeting ended: {}", s.id);

            if let Some(ref app) = self.app_handle {
                let _ = app.emit("meeting-ended", s.clone());
            }

            if let Some(ref cb) = *self.on_meeting_end.read() {
                cb(s.clone());
            }
        }

        session
    }

    /// Extend the current meeting by specified minutes
    pub fn extend_meeting(&self, minutes: i64) {
        if let Some(ref mut session) = *self.current_session.write() {
            let new_end = session.expected_end.unwrap_or(Utc::now()) + Duration::minutes(minutes);
            session.expected_end = Some(new_end);
            log::info!(
                "🎯 Meeting extended by {} minutes, new end: {}",
                minutes,
                new_end
            );
        }
    }

    /// Check if a calendar event matches current time
    fn check_calendar_trigger(
        calendar_client: &Arc<RwLock<CalendarClient>>,
        window_minutes: i64,
    ) -> Option<CalendarEventNative> {
        let now = Utc::now();
        let window = Duration::minutes(window_minutes);

        // Get events from calendar client (fetch_events returns today's events)
        let client = calendar_client.read();
        let events = client.fetch_events().ok()?;

        for event in events {
            // Skip all-day events
            if event.is_all_day {
                continue;
            }

            // Check if current time is within window of event start/end
            let event_start = event.start_time;
            let event_end = event.end_time;

            if now >= event_start - window && now <= event_end + window {
                return Some(event);
            }
        }

        None
    }

    /// Get the frontmost app if it's a meeting app
    #[cfg(target_os = "macos")]
    fn get_frontmost_meeting_app(meeting_apps: &[String]) -> Option<String> {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};

        unsafe {
            let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front_app: *mut Object = msg_send![workspace, frontmostApplication];

            if front_app.is_null() {
                return None;
            }

            let name_ns: *mut Object = msg_send![front_app, localizedName];
            if name_ns.is_null() {
                return None;
            }

            let name_utf8: *const std::os::raw::c_char = msg_send![name_ns, UTF8String];
            if name_utf8.is_null() {
                return None;
            }

            let name = std::ffi::CStr::from_ptr(name_utf8)
                .to_string_lossy()
                .to_string();

            // Check if it's a meeting app
            for app in meeting_apps {
                if name.to_lowercase().contains(&app.to_lowercase()) {
                    return Some(name);
                }
            }

            None
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn get_frontmost_meeting_app(_meeting_apps: &[String]) -> Option<String> {
        None
    }

    /// Get ALL running meeting apps (not just frontmost)
    #[cfg(target_os = "macos")]
    pub fn get_running_meeting_apps(meeting_apps: &[String]) -> Vec<String> {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};

        let mut found_apps = Vec::new();

        unsafe {
            let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            let running_apps: *mut Object = msg_send![workspace, runningApplications];

            if running_apps.is_null() {
                return found_apps;
            }

            let count: usize = msg_send![running_apps, count];

            for i in 0..count {
                let app: *mut Object = msg_send![running_apps, objectAtIndex: i];
                if app.is_null() {
                    continue;
                }

                let name_ns: *mut Object = msg_send![app, localizedName];
                if name_ns.is_null() {
                    continue;
                }

                let name_utf8: *const std::os::raw::c_char = msg_send![name_ns, UTF8String];
                if name_utf8.is_null() {
                    continue;
                }

                let name = std::ffi::CStr::from_ptr(name_utf8)
                    .to_string_lossy()
                    .to_string();

                // Check if it's a meeting app
                for app_pattern in meeting_apps {
                    if name.to_lowercase().contains(&app_pattern.to_lowercase()) {
                        if !found_apps.contains(&name) {
                            found_apps.push(name.clone());
                        }
                        break;
                    }
                }
            }
        }

        found_apps
    }

    #[cfg(not(target_os = "macos"))]
    pub fn get_running_meeting_apps(_meeting_apps: &[String]) -> Vec<String> {
        Vec::new()
    }

    /// Check if an app is using the microphone (via macOS orange dot indicator)
    /// This checks if any audio input device is being used
    #[cfg(target_os = "macos")]
    pub fn check_audio_usage() -> bool {
        use std::process::Command;

        // Use ioreg to check for audio device in use
        // The orange dot appears when input devices are accessed
        let output = Command::new("sh")
            .arg("-c")
            .arg("ioreg -l | grep -i 'IOAudioEngineState' | head -1")
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // If there's audio engine activity, microphone might be in use
            return !stdout.is_empty();
        }

        false
    }

    #[cfg(not(target_os = "macos"))]
    pub fn check_audio_usage() -> bool {
        false
    }

    /// Detect meetings and emit suggestions (instead of auto-start)
    pub fn detect_and_suggest(
        &self,
        calendar_client: &Arc<RwLock<CalendarClient>>,
    ) -> Option<MeetingDetection> {
        let cfg = self.config.read().clone();

        // Check running meeting apps
        let running_apps = Self::get_running_meeting_apps(&cfg.meeting_apps);
        let audio_active = Self::check_audio_usage();
        let frontmost_app = Self::get_frontmost_meeting_app(&cfg.meeting_apps);

        for app_name in running_apps {
            // FIX: Only suggest meeting if app is FRONTMOST or AUDIO IS ACTIVE
            // This prevents false positives from Teams/Slack running in the background
            let is_frontmost = frontmost_app.as_ref() == Some(&app_name);
            if !is_frontmost && !audio_active {
                log::debug!(
                    "🎯 Skipping {} - not frontmost and no audio activity",
                    app_name
                );
                continue;
            }

            // Create detection ID based on app
            let detection_id = format!("app-{}", app_name.to_lowercase().replace(" ", "-"));

            // Skip if already dismissed
            if self.dismissed_detections.read().contains(&detection_id) {
                continue;
            }

            // Skip if already recording this
            if self.current_session.read().is_some() {
                continue;
            }

            let detection = MeetingDetection {
                id: detection_id,
                detected_at: Utc::now(),
                source: TriggerSource::AppDetection,
                app_name: Some(app_name.clone()),
                calendar_event: None,
                is_using_audio: audio_active,
                is_screen_sharing: false,
            };

            log::info!(
                "🎯 Meeting detected: {} (audio: {})",
                app_name,
                audio_active
            );

            // Emit frontend event
            if let Some(ref app) = self.app_handle {
                let _ = app.emit("meeting-detected", detection.clone());
            }

            return Some(detection);
        }

        // Check calendar events
        if cfg.calendar_trigger_enabled {
            if let Some(event) =
                Self::check_calendar_trigger(calendar_client, cfg.calendar_window_minutes)
            {
                let detection_id = format!("cal-{}", event.event_id);

                if !self.dismissed_detections.read().contains(&detection_id)
                    && self.current_session.read().is_none()
                {
                    let detection = MeetingDetection {
                        id: detection_id,
                        detected_at: Utc::now(),
                        source: TriggerSource::Calendar,
                        app_name: None,
                        calendar_event: Some(event.clone()),
                        is_using_audio: false,
                        is_screen_sharing: false,
                    };

                    log::info!("🎯 Calendar meeting detected: {}", event.title);

                    if let Some(ref app) = self.app_handle {
                        let _ = app.emit("meeting-detected", detection.clone());
                    }

                    return Some(detection);
                }
            }
        }

        None
    }
}

impl Default for MeetingTriggerEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MeetingTriggerConfig::default();
        assert!(config.calendar_trigger_enabled);
        assert!(config.app_trigger_enabled);
        assert!(!config.audio_trigger_enabled);
        assert_eq!(config.meeting_apps.len(), 8);
    }

    #[test]
    fn test_manual_meeting() {
        let engine = MeetingTriggerEngine::new();
        assert!(!engine.is_meeting_active());

        let session = engine.start_manual_meeting();
        assert!(engine.is_meeting_active());
        assert_eq!(session.trigger_source, TriggerSource::Manual);

        engine.end_meeting();
        assert!(!engine.is_meeting_active());
    }
}
