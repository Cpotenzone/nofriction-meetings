// noFriction Meetings - Tauri Commands
// Frontend-callable commands for recording, transcription, frames, and settings
//
// Architecture: Commands are being split into domain-specific submodules.
// New commands should go in the appropriate submodule, not in this file.
//
// Submodules:
//   vault.rs   — Obsidian vault integration (v3.0.0)
//   intel.rs   — Calendar intelligence, data chatbot, meeting reports (v3.1.0+)
//   prompt.rs  — Prompt CRUD, model configuration, themes (v2.6.0+)
//   ai.rs      — AI chat, local RAG over meeting transcripts, conversations
//   capture.rs — Always-on recording, dork mode (study mode)

pub mod vault;
pub mod intel;
pub mod prompt;
pub mod ai;
pub mod capture;
pub mod local_stt;
pub mod capture_sources;
pub mod people;
pub use vault::*;
pub use intel::*;
pub use prompt::*;
pub use ai::*;
pub use capture::*;
pub use local_stt::*;
pub use capture_sources::*;
pub use self::people::*;

use crate::capture_engine::{
    AudioBuffer, AudioDevice, CapturedFrame, MonitorInfo, RecordingStatus,
};
use crate::database::{Frame, Meeting, SearchResult, SyncedTimeline, Transcript};
use crate::meeting_intel::{CalendarEvent, MeetingState, MeetingStateResolver};
use crate::settings::AppSettings;
use crate::transcription::ProviderType;
use crate::{AppState, InitStatus, InitializationState};
use base64::Engine;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// Check initialization status (safe to call before AppState is ready)
#[tauri::command(rename_all = "camelCase")]
pub async fn check_init_status(
    state: State<'_, InitializationState>,
) -> Result<InitStatus, String> {
    Ok(state.0.read().clone())
}

#[derive(serde::Serialize)]
pub struct PermissionStatus {
    pub screen_recording: bool,
    pub microphone: bool,
    pub accessibility: bool,
    pub calendar: bool,
}

/// Check macOS permissions (without triggering prompts)
#[tauri::command(rename_all = "camelCase")]
pub async fn check_permissions() -> Result<PermissionStatus, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::accessibility_extractor::AccessibilityExtractor;
        use crate::calendar_client::{CalendarAccessStatus, CalendarClient};

        // Check screen recording permission
        let screen_recording = check_screen_recording_permission();

        // Check microphone permission
        let microphone = check_microphone_permission();

        // Check accessibility permission
        let accessibility = AccessibilityExtractor::is_trusted();

        // Check calendar permission
        let calendar = CalendarClient::check_access() == CalendarAccessStatus::Authorized;

        Ok(PermissionStatus {
            screen_recording,
            microphone,
            accessibility,
            calendar,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS, assume all permissions granted
        Ok(PermissionStatus {
            screen_recording: true,
            microphone: true,
            accessibility: true,
            calendar: true,
        })
    }
}

// TCC screen-capture APIs (macOS 10.15+): preflight is a fast, accurate
// check; request triggers the system prompt / adds the app to the
// Screen Recording list without needing to attempt a capture.
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Check screen recording permission on macOS (fast, no capture attempt)
#[cfg(target_os = "macos")]
pub fn check_screen_recording_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Raw AVCaptureDevice authorization status for the microphone.
/// 0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = Authorized
#[cfg(target_os = "macos")]
fn microphone_auth_status_raw() -> i64 {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CString;

    unsafe {
        // AVMediaTypeAudio is "soun"
        let media_type_str = CString::new("soun").unwrap();
        let cls_nsstring = class!(NSString);
        let media_type: *mut Object =
            msg_send![cls_nsstring, stringWithUTF8String:media_type_str.as_ptr()];

        let cls_device = class!(AVCaptureDevice);
        msg_send![cls_device, authorizationStatusForMediaType:media_type]
    }
}

/// Check microphone permission on macOS
#[cfg(target_os = "macos")]
pub fn check_microphone_permission() -> bool {
    // Only true if strictly Authorized. Returning false for NotDetermined
    // prevents the infinite prompt loop.
    microphone_auth_status_raw() == 3
}

/// Microphone authorization status as a string, so the UI can distinguish
/// "never asked" (show Grant button) from "denied" (send to System Settings).
#[tauri::command(rename_all = "camelCase")]
pub async fn get_microphone_auth_status() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(match microphone_auth_status_raw() {
            3 => "authorized",
            2 => "denied",
            1 => "restricted",
            _ => "not_determined",
        }
        .to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok("authorized".to_string())
    }
}

/// Open the relevant System Settings privacy pane so the user can flip the
/// toggle when a permission was previously denied.
#[tauri::command(rename_all = "camelCase")]
pub async fn open_system_settings(app: tauri::AppHandle, pane: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = match pane.as_str() {
            "microphone" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            "screen_recording" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            "accessibility" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            "calendar" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
            "notifications" => "x-apple.systempreferences:com.apple.preference.notifications",
            _ => "x-apple.systempreferences:com.apple.preference.security",
        };
        // NSWorkspace via the opener plugin: no child process (App Sandbox)
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("Failed to open System Settings: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, pane);
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct ScreenTestResult {
    pub success: bool,
    pub frame_width: Option<u32>,
    pub frame_height: Option<u32>,
    pub error: Option<String>,
}

/// Test screen capture - attempts to capture a single frame
#[tauri::command(rename_all = "camelCase")]
pub async fn test_screen_capture() -> Result<ScreenTestResult, String> {
    #[cfg(target_os = "macos")]
    {
        use xcap::Monitor;

        match Monitor::all() {
            Ok(monitors) => {
                // Find primary monitor or use the first available
                let monitor = monitors
                    .into_iter()
                    .find(|m| m.is_primary().unwrap_or(false))
                    .or_else(|| Monitor::all().ok().and_then(|mut m: Vec<Monitor>| m.pop()));

                if let Some(monitor) = monitor {
                    match monitor.capture_image() {
                        Ok(image) => {
                            let width = image.width();
                            let height = image.height();
                            Ok(ScreenTestResult {
                                success: true,
                                frame_width: Some(width),
                                frame_height: Some(height),
                                error: None,
                            })
                        }
                        Err(e) => Ok(ScreenTestResult {
                            success: false,
                            frame_width: None,
                            frame_height: None,
                            error: Some(format!("Failed to capture frame: {}", e)),
                        }),
                    }
                } else {
                    Ok(ScreenTestResult {
                        success: false,
                        frame_width: None,
                        frame_height: None,
                        error: Some("No monitor found".to_string()),
                    })
                }
            }
            Err(e) => Ok(ScreenTestResult {
                success: false,
                frame_width: None,
                frame_height: None,
                error: Some(format!("Failed to list monitors: {}", e)),
            }),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(ScreenTestResult {
            success: false,
            frame_width: None,
            frame_height: None,
            error: Some("Screen capture test only available on macOS".to_string()),
        })
    }
}

#[derive(serde::Serialize)]
pub struct MicTestResult {
    pub success: bool,
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub error: Option<String>,
}

/// Test microphone - attempts to initialize the mic
#[tauri::command(rename_all = "camelCase")]
pub async fn test_microphone() -> Result<MicTestResult, String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    // Safeguard: Check permission PASSIVELY before triggering CPAL initialization
    // This prevents the "infinite loop" of prompts if the app hasn't been granted access.
    if !check_microphone_permission() {
        return Ok(MicTestResult {
            success: false,
            device_name: None,
            sample_rate: None,
            channels: None,
            error: Some("Microphone permission not granted (passive check)".to_string()),
        });
    }

    let host = cpal::default_host();
    match host.default_input_device() {
        Some(device) => {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            match device.default_input_config() {
                Ok(config) => Ok(MicTestResult {
                    success: true,
                    device_name: Some(name),
                    sample_rate: Some(config.sample_rate().0),
                    channels: Some(config.channels()),
                    error: None,
                }),
                Err(e) => Ok(MicTestResult {
                    success: false,
                    device_name: Some(name),
                    sample_rate: None,
                    channels: None,
                    error: Some(format!("Failed to get config: {}", e)),
                }),
            }
        }
        None => Ok(MicTestResult {
            success: false,
            device_name: None,
            sample_rate: None,
            channels: None,
            error: Some("No microphone found".to_string()),
        }),
    }
}

#[derive(serde::Serialize)]
pub struct AccessibilityTestResult {
    pub success: bool,
    pub is_trusted: bool,
    pub app_name: Option<String>,
    pub text_sample: Option<String>,
    pub text_length: Option<usize>,
    pub error: Option<String>,
}

/// Test accessibility - attempts to extract text from focused window
#[tauri::command(rename_all = "camelCase")]
pub async fn test_accessibility() -> Result<AccessibilityTestResult, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::accessibility_extractor::AccessibilityExtractor;

        let is_trusted = AccessibilityExtractor::is_trusted();
        if !is_trusted {
            return Ok(AccessibilityTestResult {
                success: false,
                is_trusted: false,
                app_name: None,
                text_sample: None,
                text_length: None,
                error: Some("Accessibility permission not granted".to_string()),
            });
        }

        let extractor = AccessibilityExtractor::new();
        match extractor.extract_focused_window() {
            Ok(result) => {
                let sample = if result.text.len() > 200 {
                    format!("{}...", &result.text[..200])
                } else {
                    result.text.clone()
                };
                Ok(AccessibilityTestResult {
                    success: true,
                    is_trusted: true,
                    app_name: result.app_name.clone(),
                    text_sample: Some(sample),
                    text_length: Some(result.text.len()),
                    error: None,
                })
            }
            Err(e) => Ok(AccessibilityTestResult {
                success: false,
                is_trusted: true,
                app_name: None,
                text_sample: None,
                text_length: None,
                error: Some(e),
            }),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(AccessibilityTestResult {
            success: false,
            is_trusted: false,
            app_name: None,
            text_sample: None,
            text_length: None,
            error: Some("Accessibility test only available on macOS".to_string()),
        })
    }
}

/// Request a specific permission (triggers macOS prompt)
#[tauri::command(rename_all = "camelCase")]
pub async fn request_permission(permission_type: String) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        match permission_type.as_str() {
            "screen_recording" => {
                // Proper TCC request: shows the system prompt and registers
                // the app in Privacy & Security → Screen Recording.
                // Note: after granting, macOS requires an app relaunch.
                let granted = unsafe { CGRequestScreenCaptureAccess() };
                Ok(granted)
            }
            "microphone" => {
                if check_microphone_permission() {
                    return Ok(true);
                }
                // Denied/restricted: macOS will NOT re-prompt — the user must
                // flip the toggle in System Settings (UI offers that button).
                let status = microphone_auth_status_raw();
                if status == 2 || status == 1 {
                    return Ok(false);
                }

                // NotDetermined: querying device config does NOT trigger the
                // TCC prompt (the old bug — "grant" appeared to do nothing).
                // Actually STARTING an input stream does. Run one briefly on
                // a blocking thread; the UI polls status while the user
                // answers the prompt.
                let _ = tokio::task::spawn_blocking(|| {
                    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
                    let host = cpal::default_host();
                    let Some(device) = host.default_input_device() else {
                        return;
                    };
                    let Ok(config) = device.default_input_config() else {
                        return;
                    };
                    match device.build_input_stream(
                        &config.into(),
                        |_data: &[f32], _| {},
                        |e| log::debug!("permission-probe stream error: {}", e),
                        None,
                    ) {
                        Ok(stream) => {
                            let _ = stream.play();
                            // Keep the stream alive long enough for TCC to
                            // register the access attempt and show the prompt
                            std::thread::sleep(std::time::Duration::from_millis(800));
                            drop(stream);
                        }
                        Err(e) => log::debug!("permission-probe stream build failed: {}", e),
                    }
                })
                .await;

                Ok(check_microphone_permission())
            }
            "accessibility" => {
                // Trigger accessibility permission prompt
                use crate::accessibility_extractor::AccessibilityExtractor;

                // Request with prompt
                let is_trusted = AccessibilityExtractor::request_permission_with_prompt();
                Ok(is_trusted)
            }
            _ => Err(format!("Unknown permission type: {}", permission_type)),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = permission_type;
        Ok(true) // Non-macOS always granted
    }
}

/// Start recording with frame capture and live transcription
#[tauri::command(rename_all = "camelCase")]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    // Generate a new meeting ID
    let meeting_id = uuid::Uuid::new_v4().to_string();
    let title = format!("Meeting {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));

    // Create meeting in database
    state
        .database
        .create_meeting(&meeting_id, &title)
        .await
        .map_err(|e| format!("Failed to create meeting: {}", e))?;

    // ═══════════════════════════════════════════════════════════════════════════
    // Calendar Integration: Check for matching calendar event
    // ═══════════════════════════════════════════════════════════════════════════
    let calendar_event = {
        let client = state.calendar_client.read();

        // Force refresh on MacOS to ensure fresh data
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = client.fetch_events() {
                log::warn!("Failed to refresh calendar events: {}", e);
            }
        }

        client.get_current_event()
    }; // Guard dropped here before any .await

    // Meeting-end detection uses the event's scheduled end
    let calendar_window = calendar_event
        .as_ref()
        .map(|e| (e.end_time, e.title.clone()));

    if let Some(event) = calendar_event {
        log::info!(
            "📅 Calendar match found: '{}' with {} attendees",
            event.title,
            event.attendees.len()
        );

        // Update meeting title to calendar event title
        let _ = state
            .database
            .update_meeting_title(&meeting_id, &event.title)
            .await;

        // Store calendar event ID
        let _ = state
            .database
            .set_meeting_calendar_event(&meeting_id, &event.event_id)
            .await;

        // Store meeting details + attendees (names from the invite)
        match crate::people::link_meeting_to_event(&state.database.get_pool(), &meeting_id, &event).await {
            Ok(n) => log::info!("  👥 Linked {} people from calendar", n),
            Err(e) => log::warn!("Failed to store calendar attendees: {}", e),
        }

        // Emit calendar match event to frontend
        #[derive(serde::Serialize, Clone)]
        struct CalendarMatchPayload {
            meeting_id: String,
            event_title: String,
            attendee_count: usize,
            attendee_names: Vec<String>,
            start_time: String,
            end_time: String,
        }

        let attendee_names: Vec<String> = event
            .participants
            .iter()
            .filter(|p| !p.is_self)
            .map(|p| {
                p.name
                    .clone()
                    .unwrap_or_else(|| crate::attendee_intel::extract_name_from_email(&p.email))
            })
            .collect();

        let _ = app.emit(
            "calendar_match",
            CalendarMatchPayload {
                meeting_id: meeting_id.clone(),
                event_title: event.title.clone(),
                attendee_count: event.attendees.len(),
                attendee_names,
                start_time: event.start_time.to_rfc3339(),
                end_time: event.end_time.to_rfc3339(),
            },
        );
    } else {
        log::info!("📅 No matching calendar event found for this recording");
    }

    // Get app data directory for frame storage
    let frames_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("frames")
        .join(&meeting_id);

    std::fs::create_dir_all(&frames_dir)
        .map_err(|e| format!("Failed to create frames directory: {}", e))?;

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Initialize Stateful Screen Ingest
    // ═══════════════════════════════════════════════════════════════════════════

    // Start metrics collection
    state.metrics_collector.start_meeting(&meeting_id);

    // Start state builder for this meeting
    {
        let state_builder = state.state_builder.read();
        state_builder.start_meeting(&meeting_id);
    }

    // Phase 2: Start episode builder
    {
        let episode_builder = state.episode_builder.read();
        episode_builder.start_meeting(&meeting_id);
    }

    // Phase 3: Start timeline builder
    state
        .timeline_builder
        .start_meeting(&meeting_id, chrono::Utc::now());

    log::info!(
        "📊 Stateful capture initialized for meeting: {} (Phase 1-3)",
        meeting_id
    );

    // Set up Transcription connection
    {
        // Use transcription manager
        let tm = &state.transcription_manager;
        // Local transcription only. Never auto-select a service from saved keys.
        tm.set_context(
            app.clone(),
            state.database.clone(),
            meeting_id.clone(),
            state.live_intel_agent.clone(),
        );
        tm.start();
    }

    // Set up audio callback to stream to Transcription Provider
    let transcription_manager = state.transcription_manager.clone();
    let mixer = Arc::new(crate::audio_mixer::AudioMixer::new());

    let audio_callback: Arc<dyn Fn(AudioBuffer) + Send + Sync> = Arc::new(move |buffer| {
        if buffer.samples.is_empty() {
            return;
        }

        // Mic and system audio arrive on separate threads; mix them into one
        // aligned 16kHz stream rather than splicing them end-to-end.
        // Forwarded under the mixer lock so chunks stay in order (non-blocking)
        mixer.push_with(buffer.source, &buffer.samples, buffer.sample_rate, buffer.channels, |mixed| {
            transcription_manager.process_audio(mixed, crate::audio_mixer::MIX_SAMPLE_RATE, 1);
        });
    });

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Stateful Frame Callback (DeDupGate + StateBuilder)
    // ═══════════════════════════════════════════════════════════════════════════
    let db_for_frames = state.database.clone();
    let meeting_id_for_frames = meeting_id.clone();
    let frames_dir_clone = frames_dir.clone();
    let state_builder = state.state_builder.clone();
    let metrics_collector = state.metrics_collector.clone();
    let settings_for_frames = state.settings.clone();
    let app_for_frames = app.clone();

    // Estimated bytes per frame (for savings calculation)
    const ESTIMATED_FRAME_BYTES: u64 = 50_000; // ~50KB per JPEG

    let frame_callback: Arc<dyn Fn(CapturedFrame) + Send + Sync> = Arc::new(move |frame| {
        let db = db_for_frames.clone();
        let mid = meeting_id_for_frames.clone();
        let dir = frames_dir_clone.clone();
        let builder = state_builder.clone();
        let metrics = metrics_collector.clone();
        let settings = settings_for_frames.clone();
        let app_handle = app_for_frames.clone();

        // Process frame through StateBuilder (stateful dedup)
        tokio::spawn(async move {
            // Start CPU timer
            let timer_start = std::time::Instant::now();

            // Record frame received
            metrics.record_frame();

            // Process through StateBuilder (pHash + delta scoring)
            let result = {
                let builder = builder.read();
                builder.process_frame(&frame.source, frame.image.clone(), frame.timestamp)
            };

            use crate::state_builder::FrameProcessResult;

            match result {
                FrameProcessResult::Extended {
                    state_id,
                    new_end_ts,
                } => {
                    // Frame was a duplicate - extend current state duration
                    metrics.record_duplicate_skipped(ESTIMATED_FRAME_BYTES);

                    // Update state end_ts in database
                    if let Err(e) = db.extend_screen_state(&state_id, new_end_ts).await {
                        log::warn!("Failed to extend screen state: {}", e);
                    }

                    log::trace!("📺 Frame duplicate, extended state: {}", state_id);
                }

                FrameProcessResult::NewState {
                    completed_state,
                    new_state_id,
                } => {
                    // State boundary detected - save keyframe
                    metrics.record_new_state();

                    // Finalize the completed state if any
                    if let Some(completed) = completed_state {
                        log::debug!(
                            "📺 State completed: {} (duration: {:?}ms)",
                            completed.state_id,
                            completed.duration_ms()
                        );
                    }

                    // Get pending keyframe to save
                    let pending_keyframe = {
                        let builder = builder.read();
                        builder.take_pending_keyframe(&frame.source)
                    };

                    if let Some(keyframe_image) = pending_keyframe {
                        // Generate keyframe path (state-based, not frame-number-based)
                        let filename = format!("state_{}.jpg", new_state_id);
                        let keyframe_path = dir.join(&filename);

                        // Save keyframe as JPEG
                        if let Err(e) = keyframe_image.to_rgb8().save(&keyframe_path) {
                            log::warn!("Failed to save keyframe: {}", e);
                        } else {
                            metrics.record_image_write(ESTIMATED_FRAME_BYTES);

                            // Get state info for database insertion
                            let _state_record = {
                                let _builder = builder.read();
                                // Access current state info from accumulator
                                // For now we insert with minimal info
                                None::<crate::state_builder::ScreenState>
                            };

                            // Insert new screen state into database
                            let flags_json = "{}";
                            if let Err(e) = db
                                .add_screen_state(
                                    &new_state_id,
                                    &mid,
                                    frame.timestamp,
                                    Some(frame.timestamp),
                                    "",  // phash - would need to pass from StateBuilder
                                    0.0, // delta_score
                                    Some(keyframe_path.to_str().unwrap_or("")),
                                    "other",
                                    flags_json,
                                )
                                .await
                            {
                                log::warn!("Failed to save screen state: {}", e);
                            }
                            let _ = db
                                .set_screen_state_source(
                                    &new_state_id,
                                    &frame.source,
                                    &frame.label,
                                    frame.app_name.as_deref(),
                                )
                                .await;
                            let _ = app_handle.emit(
                                "frame_captured",
                                serde_json::json!({
                                    "state_id": new_state_id,
                                    "meeting_id": mid,
                                    "path": keyframe_path.to_string_lossy(),
                                    "source": frame.source,
                                    "label": frame.label,
                                    "timestamp": frame.timestamp.to_rfc3339(),
                                    "manual": false,
                                }),
                            );

                            log::debug!("📺 New state: {} → {:?}", new_state_id, keyframe_path);

                            // Queue frame for VLM analysis if enabled
                            if let Ok(app_settings) = settings.get_all().await {
                                if app_settings.queue_frames_for_vlm {
                                    if let Err(e) = db
                                        .queue_frame(
                                            None, // frame_id - using screen state
                                            keyframe_path.to_str().unwrap_or(""),
                                            frame.timestamp,
                                        )
                                        .await
                                    {
                                        log::warn!("Failed to queue frame for VLM: {}", e);
                                    } else {
                                        log::debug!("📸 Queued frame for VLM: {}", new_state_id);
                                    }
                                }
                            }
                        }
                    }
                }

                FrameProcessResult::PassThrough => {
                    // Stateful capture disabled, fall back to legacy behavior
                    let filename = format!("frame_{}.jpg", frame.frame_number);
                    let thumbnail_path = dir.join(&filename);

                    if let Err(e) = frame.image.to_rgb8().save(&thumbnail_path) {
                        log::warn!("Failed to save frame thumbnail: {}", e);
                        return;
                    }

                    if let Err(e) = db
                        .add_frame(
                            &mid,
                            frame.timestamp,
                            Some(thumbnail_path.to_str().unwrap_or("")),
                            None,
                        )
                        .await
                    {
                        log::warn!("Failed to save frame to database: {}", e);
                    }
                }
            }

            // Record CPU time
            metrics.record_cpu_time(timer_start.elapsed());
        });
    });

    // Load capture settings BEFORE acquiring lock
    let (frame_interval, capture_mic, capture_system, capture_screen) =
        match state.settings.get_all().await {
            Ok(s) => (
                s.frame_capture_interval_ms,
                s.capture_microphone,
                s.capture_system_audio,
                s.capture_screen,
            ),
            Err(_) => (1000, true, true, true),
        };

    // Set callbacks and start capture
    {
        let engine = state.capture_engine.read();
        engine.set_audio_callback(audio_callback);
        engine.set_frame_callback(frame_callback);
        engine.set_frame_interval(frame_interval);
        engine.set_sources(capture_mic, capture_system, capture_screen);
    }

    // Clone app handle before engine.start() consumes it
    let app_for_segment = app.clone();
    let app_for_end_monitor = app.clone();

    {
        let engine = state.capture_engine.read();
        engine.start(app)?;
    }

    log::info!(
        "🎬 Recording started: {} (stateful capture enabled, frames → {:?})",
        meeting_id,
        frames_dir
    );

    // Watch for the meeting ending (call app releases the mic, window closes,
    // calendar end, sustained silence) — see meeting_end.rs
    crate::meeting_end::start_monitor(app_for_end_monitor, meeting_id.clone(), calendar_window);

    // ═══════════════════════════════════════════════════════════════════════════
    // Recording Segmentation: emit event at 75 minutes to prompt user
    // ═══════════════════════════════════════════════════════════════════════════
    {
        let app_seg = app_for_segment;
        let mid_seg = meeting_id.clone();
        tokio::spawn(async move {
            // Wait 75 minutes
            tokio::time::sleep(std::time::Duration::from_secs(75 * 60)).await;
            // Only prompt if this same meeting is still being recorded
            let still_recording = {
                let st = app_seg.state::<AppState>();
                let engine = st.capture_engine.read();
                engine.is_recording() && st.state_builder.read().current_meeting_id().as_deref() == Some(mid_seg.as_str())
            };
            if !still_recording {
                return;
            }
            log::info!(
                "⏱️ Recording {} has reached 75 minutes — prompting user",
                mid_seg
            );
            let _ = app_seg.emit(
                "recording_segment_prompt",
                serde_json::json!({
                    "meeting_id": mid_seg,
                    "elapsed_minutes": 75
                }),
            );
        });
    }

    Ok(meeting_id)
}

/// Stop recording
#[tauri::command(rename_all = "camelCase")]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<(), String> {
    stop_recording_core(&state).await
}

/// Stop initiated by the backend (meeting-end auto-stop when the UI didn't
/// act): the same steps as the UI's Stop — video, accessibility unlink,
/// then the core stop — and tell the UI it happened.
pub async fn stop_recording_from_backend(app: &AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "App not ready".to_string())?;
    // ffmpeg video exists only in the DMG build
    #[cfg(not(feature = "mas"))]
    {
        let recorder = video::get_video_recorder();
        let active = recorder.read().get_status().is_some();
        if active {
            if let Err(e) = recorder.write().stop() {
                log::warn!("Video recording failed to stop: {}", e);
            }
            state.power_manager.release_assertion();
        }
    }
    state.accessibility_capture.set_meeting_id(None);
    stop_recording_core(&state).await?;
    let _ = app.emit("recording-stopped-automatically", ());
    Ok(())
}

/// Shared stop path (UI Stop, tray, and backend auto-stop).
pub async fn stop_recording_core(state: &AppState) -> Result<(), String> {
    let was_recording = {
        let engine = state.capture_engine.read();
        engine.get_status().is_recording
    };
    // Read before the state builder forgets it below
    let stopped_meeting_id = state.state_builder.read().current_meeting_id();
    // Ends meeting-end detection; Some(end) if this stop follows a detected end
    let trim_after = crate::meeting_end::on_recording_stopped(stopped_meeting_id.as_deref());

    // Stop capture engine
    {
        let engine = state.capture_engine.read();
        engine.stop()?;
    }

    // Stop transcription provider
    {
        state.transcription_manager.stop();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Finalize Stateful Screen Ingest
    // ═══════════════════════════════════════════════════════════════════════════
    if was_recording {
        // End state builder session
        let final_state = {
            let state_builder = state.state_builder.read();
            state_builder.end_meeting()
        };

        for completed in final_state {
            log::info!(
                "📺 Final state completed: {} (duration: {:?}ms)",
                completed.state_id,
                completed.duration_ms()
            );
        }

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 2: Finalize Episode Building
        // ═══════════════════════════════════════════════════════════════════════
        let episodes = {
            let episode_builder = state.episode_builder.read();
            episode_builder.finalize_all()
        };

        log::info!(
            "📚 Episode building completed: {} episodes created",
            episodes.len()
        );

        // Save episodes to database
        for episode in &episodes {
            // Create the episode first
            if let Err(e) = state
                .database
                .create_episode(
                    &episode.episode_id,
                    &episode.meeting_id,
                    episode.start_ts,
                    episode.app_name.as_deref(),
                    episode.window_title.as_deref(),
                )
                .await
            {
                log::warn!("Failed to create episode: {}", e);
                continue;
            }

            // Then update with final stats
            if let Some(end_ts) = episode.end_ts {
                if let Err(e) = state
                    .database
                    .update_episode(
                        &episode.episode_id,
                        end_ts,
                        episode.state_count,
                        episode.duration_ms(),
                    )
                    .await
                {
                    log::warn!("Failed to update episode: {}", e);
                }
            }
        }

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 3: Finalize Timeline Generation
        // ═══════════════════════════════════════════════════════════════════════
        let timeline_events = state.timeline_builder.end_meeting(chrono::Utc::now());
        let topic_clusters = state.timeline_builder.get_topics();

        log::info!(
            "📊 Timeline generation completed: {} events, {} topics",
            timeline_events.len(),
            topic_clusters.len()
        );

        // Save timeline events to database
        for event in &timeline_events {
            if let Err(e) = state
                .database
                .add_timeline_event(
                    &event.event_id,
                    &event.meeting_id,
                    event.ts,
                    event.event_type.as_str(),
                    &event.title,
                    event.description.as_deref(),
                    event.app_name.as_deref(),
                    event.window_title.as_deref(),
                    event.duration_ms,
                    event.episode_id.as_deref(),
                    event.state_id.as_deref(),
                    event.topic.as_deref(),
                    event.importance,
                )
                .await
            {
                log::warn!("Failed to save timeline event: {}", e);
            }
        }

        // Save topic clusters to database
        for topic in &topic_clusters {
            if let Err(e) = state
                .database
                .add_topic_cluster(
                    &topic.topic_id,
                    &state
                        .timeline_builder
                        .get_events()
                        .first()
                        .map(|e| e.meeting_id.clone())
                        .unwrap_or_default(),
                    &topic.name,
                    topic.description.as_deref(),
                    topic.start_ts,
                    topic.end_ts,
                    topic.event_count,
                    topic.total_duration_ms,
                )
                .await
            {
                log::warn!("Failed to save topic cluster: {}", e);
            }
        }

        // End metrics collection and log summary
        if let Some(metrics) = state.metrics_collector.end_meeting() {
            metrics.log_summary();

            // Log highlights for easy verification
            log::info!(
                "🎯 Stateful capture summary: {} frames → {} states ({:.1}% reduction)",
                metrics.frames_in,
                metrics.states_out,
                metrics.dedup_ratio * 100.0
            );
        }

        log::info!("🎬 Recording stopped successfully (Phase 1-3 finalized)");

        // Close the meeting row (ended_at + duration). Previously never
        // called, so every meeting stayed "open" and auto-report saw 0s.
        if let Some(mid) = stopped_meeting_id.as_deref() {
            if let Err(e) = state.database.end_meeting(mid).await {
                log::warn!("Failed to mark meeting {} ended: {}", mid, e);
            }
            // Drop junk the transcriber produced after the meeting ended.
            // Delayed so the transcriber's final flush lands first.
            if let Some(end_at) = trim_after {
                let db = state.database.clone();
                let mid = mid.to_string();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    crate::meeting_end::trim_junk_after(&db, &mid, end_at).await;
                });
            }
        }

        // v3.0.0: Obsidian Auto-Export
        if let Ok(settings) = state.settings.get_all().await {
            if settings.obsidian_auto_export && settings.obsidian_vault_path.is_some() {
                // Determine the meeting ID that just ended
                let meeting_id = {
                    let timeline = state.timeline_builder.get_events();
                    timeline
                        .first()
                        .map(|e| e.meeting_id.clone())
                        .unwrap_or_default()
                };

                if !meeting_id.is_empty() {
                    log::info!(
                        "🚀 Triggering Obsidian Auto-Export for meeting: {}",
                        meeting_id
                    );
                    let db_clone = state.database.clone();
                    let vm_clone = state.vault_manager.clone();
                    tokio::spawn(async move {
                        if let Err(e) = internal_export_meeting(
                            db_clone,
                            vm_clone,
                            "Inbox".to_string(),
                            meeting_id,
                        )
                        .await
                        {
                            log::error!("❌ Obsidian Auto-Export failed: {}", e);
                        } else {
                            log::info!("✅ Obsidian Auto-Export complete");
                        }
                    });
                }
            }
        }
    }

    // v3.1.0: Auto-generate AI meeting report for recordings > 6 minutes
    {
        let meeting_id = stopped_meeting_id.clone().unwrap_or_else(|| {
            let timeline = state.timeline_builder.get_events();
            timeline
                .first()
                .map(|e| e.meeting_id.clone())
                .unwrap_or_default()
        });

        if !meeting_id.is_empty() {
            let db_clone = state.database.clone();
            let settings_clone = state.settings.clone();
            let ai_clone = state.ai_client.clone();
            tokio::spawn(async move {
                // Check if auto-report is enabled and meeting duration > 6 min
                let settings = match settings_clone.get_all().await {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("Failed to load settings for auto-report: {}", e);
                        return;
                    }
                };

                if !settings.auto_generate_report {
                    log::info!("📊 Auto-report disabled, skipping");
                    return;
                }

                // No AI provider configured (or not yet approved): skip
                if !crate::ai::is_ready(crate::ai::Kind::Text) {
                    log::info!("📊 Auto-report skipped — no AI provider ready (Settings → AI Engine)");
                    return;
                }

                // Check meeting duration
                match db_clone.get_meeting(&meeting_id).await {
                    Ok(Some(meeting)) => {
                        let duration = meeting.duration_seconds.unwrap_or(0);
                        if duration < 360 {
                            log::info!(
                                "📊 Meeting {} is {} seconds (< 6 min), skipping auto-report",
                                meeting_id,
                                duration
                            );
                            return;
                        }

                        log::info!(
                            "📊 Generating auto-report for meeting {} ({} seconds)",
                            meeting_id,
                            duration
                        );

                        let ai_client = { ai_clone.read().clone() };
                        let generator = crate::meeting_notes::MeetingNotesGenerator::new(ai_client);
                        let prompt = settings.meeting_report_prompt;

                        match generator
                            .generate_notes_with_prompt(&meeting_id, &db_clone, &prompt)
                            .await
                        {
                            Ok(notes) => {
                                log::info!(
                                    "✅ Auto-report generated for meeting {}: {}",
                                    meeting_id,
                                    notes.summary.chars().take(80).collect::<String>()
                                );
                            }
                            Err(e) => {
                                log::error!("❌ Auto-report generation failed: {}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("📊 Meeting {} not found for auto-report", meeting_id);
                    }
                    Err(e) => {
                        log::error!("📊 Failed to get meeting for auto-report: {}", e);
                    }
                }
            });
        }
    }

    Ok(())
}

/// Get recording status
#[tauri::command(rename_all = "camelCase")]
pub async fn get_recording_status(state: State<'_, AppState>) -> Result<RecordingStatus, String> {
    let engine = state.capture_engine.read();
    Ok(engine.get_status())
}

/// Capture a single screenshot (for preview)
#[tauri::command(rename_all = "camelCase")]
pub async fn capture_screenshot(monitor_id: Option<u32>) -> Result<String, String> {
    let image = crate::capture_engine::CaptureEngine::capture_screenshot(monitor_id)?;

    // Convert to base64 JPEG for frontend display
    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);
    image
        .to_rgb8()
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    let base64 = base64::engine::general_purpose::STANDARD.encode(&buffer);
    Ok(format!("data:image/jpeg;base64,{}", base64))
}

/// Get transcripts for a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_transcripts(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Transcript>, String> {
    // UI view: keep strike-marker tokens so the transcript can draw the bar
    state
        .database
        .get_transcripts_marked(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))
}

/// Search transcripts across all meetings
#[tauri::command(rename_all = "camelCase")]
pub async fn search_transcripts(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    state
        .database
        .search_transcripts(&query)
        .await
        .map_err(|e| format!("Failed to search: {}", e))
}

/// Get frames for a meeting (rewind timeline)

#[tauri::command(rename_all = "camelCase")]
pub async fn debug_log(message: String) {
    eprintln!("[FRONTEND] {}", message);
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_frames(
    meeting_id: String,
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Frame>, String> {
    let limit = limit.unwrap_or(1000);
    state
        .database
        .get_frames(&meeting_id, limit)
        .await
        .map_err(|e| format!("Failed to get frames: {}", e))
}

/// Get frame count for a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_frame_count(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    state
        .database
        .count_frames(&meeting_id)
        .await
        .map_err(|e| format!("Failed to count frames: {}", e))
}

/// Get a frame thumbnail as base64
/// Supports both legacy frames (integer IDs) and screen_states (UUID state_ids)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_frame_thumbnail(
    frame_id: String,
    _thumbnail: bool,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // First, check if this is a UUID (screen_state) or integer (legacy frame)
    let is_uuid = frame_id.contains('-') && frame_id.len() > 20;

    if is_uuid {
        // Search screen_states by state_id
        let meetings = state
            .database
            .list_meetings(100)
            .await
            .map_err(|e| format!("Failed to get meetings: {}", e))?;

        for meeting in meetings {
            let screen_states = state
                .database
                .get_screen_states(&meeting.id, 10000)
                .await
                .map_err(|e| format!("Failed to get screen states: {}", e))?;

            if let Some(screen_state) = screen_states.iter().find(|s| s.state_id == frame_id) {
                if let Some(ref path) = screen_state.keyframe_path {
                    if let Ok(data) = std::fs::read(path) {
                        let base64 = base64::engine::general_purpose::STANDARD.encode(&data);
                        return Ok(Some(base64));
                    }
                }
            }
        }
    } else {
        // Legacy integer ID - search frames table
        let id: i64 = frame_id.parse().unwrap_or(0);

        let meetings = state
            .database
            .list_meetings(100)
            .await
            .map_err(|e| format!("Failed to get meetings: {}", e))?;

        for meeting in meetings {
            let frames = state
                .database
                .get_frames(&meeting.id, 10000)
                .await
                .map_err(|e| format!("Failed to get frames: {}", e))?;

            if let Some(frame) = frames.iter().find(|f| f.id == id) {
                if let Some(ref path) = frame.file_path {
                    if let Ok(data) = std::fs::read(path) {
                        let base64 = base64::engine::general_purpose::STANDARD.encode(&data);
                        return Ok(Some(base64));
                    } else {
                        log::warn!("Failed to read frame file: {}", path);
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Get available audio devices
#[tauri::command(rename_all = "camelCase")]
pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    crate::capture_engine::CaptureEngine::list_audio_devices()
}

/// Set the audio input device (persisted)
#[tauri::command(rename_all = "camelCase")]
pub async fn set_audio_device(device_id: String, state: State<'_, AppState>) -> Result<(), String> {
    {
        let engine = state.capture_engine.read();
        engine.set_microphone(device_id.clone());
    }

    state
        .settings
        .set_selected_microphone(&device_id)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    log::info!("Microphone set to: {}", device_id);
    Ok(())
}

/// Get available monitors
#[tauri::command(rename_all = "camelCase")]
pub async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    crate::capture_engine::CaptureEngine::list_monitors()
}

/// Set the monitor (persisted)
#[tauri::command(rename_all = "camelCase")]
pub async fn set_monitor(monitor_id: u32, state: State<'_, AppState>) -> Result<(), String> {
    {
        let engine = state.capture_engine.read();
        engine.set_monitor(monitor_id);
    }

    state
        .settings
        .set_selected_monitor(monitor_id)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    log::info!("Monitor set to: {}", monitor_id);
    Ok(())
}

/// Local transcription is the only supported engine. Retired cloud ids fail closed.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_provider(provider: String, state: State<'_, AppState>) -> Result<(), String> {
    if provider != "local" {
        return Err("Only local Whisper transcription is supported. Configure AI endpoints separately.".into());
    }
    state.transcription_manager.switch_provider(ProviderType::Local);
    state.settings.set_transcription_provider("local").await.map_err(|e| e.to_string())
}

/// Get all settings. Secret values are never returned; see `secret_status`.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .get_all()
        .await
        .map(|s| s.redacted_for_ui())
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Get a single setting value
#[tauri::command(rename_all = "camelCase")]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // Secrets never go back to the UI
    if crate::secrets::is_secret_setting(&key) || crate::secrets::looks_secret(&key) {
        return Ok(None);
    }
    state
        .settings
        .get(&key)
        .await
        .map_err(|e| format!("Failed to get setting: {}", e))
}

/// Set a single setting value
#[tauri::command(rename_all = "camelCase")]
pub async fn set_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Known secrets are routed to the Keychain by SettingsManager; anything
    // else that looks like a credential is refused rather than stored in
    // plaintext. AI settings have their own commands.
    if (!crate::secrets::is_secret_setting(&key) && crate::secrets::looks_secret(&key))
        || key == crate::ai::config::SETTINGS_KEY
    {
        return Err(format!("'{}' can't be set here", key));
    }
    state
        .settings
        .set(&key, &value)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Get all meetings
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meetings(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Meeting>, String> {
    let limit = limit.unwrap_or(50);
    state
        .database
        .list_meetings(limit)
        .await
        .map_err(|e| format!("Failed to list meetings: {}", e))
}

/// Get a single meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Option<Meeting>, String> {
    state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))
}

/// Delete a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_meeting(meeting_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .delete_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to delete meeting: {}", e))?;

    // Rows in other tables go with ON DELETE CASCADE; the meeting's files
    // (screenshots, screen video, cached frames) go here.
    let dirs = meeting_file_dirs(&crate::paths::app_data_dir(), &crate::paths::app_cache_dir(), &meeting_id);
    let mut failed = 0;
    for d in dirs.iter().filter(|d| d.exists()) {
        if let Err(e) = std::fs::remove_dir_all(d) {
            failed += 1;
            log::warn!("Could not remove {} for deleted meeting: {}", d.display(), e);
        }
    }
    log::info!("Meeting deleted: {} ({} file folder(s) left behind)", meeting_id, failed);
    Ok(())
}

/// Folders that hold one meeting's files: `<data>/frames/<id>` (screenshots),
/// `<data>/<id>` (screen video, DMG build) and `<cache>/<id>` (extracted
/// frames/thumbnails). Empty when `meeting_id` isn't a plain id, so a bad
/// value can never point outside those folders.
pub fn meeting_file_dirs(data: &std::path::Path, cache: &std::path::Path, meeting_id: &str) -> Vec<std::path::PathBuf> {
    let safe = !meeting_id.is_empty()
        && meeting_id.len() <= 64
        && meeting_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !["frames", "models", "logs", "backups", "snapshots"].contains(&meeting_id);
    if !safe {
        return Vec::new();
    }
    vec![data.join("frames").join(meeting_id), data.join(meeting_id), cache.join(meeting_id)]
}

#[cfg(test)]
mod meeting_file_tests {
    use super::meeting_file_dirs;
    use std::path::Path;

    #[test]
    fn meeting_dirs_only_for_plain_ids() {
        let (d, c) = (Path::new("/data"), Path::new("/cache"));
        let id = "508b6752-52a7-48ca-a682-fb039c90be9b";
        assert_eq!(
            meeting_file_dirs(d, c, id),
            vec![d.join("frames").join(id), d.join(id), c.join(id)]
        );
        for bad in ["", "..", "../x", "a/b", "frames", "models", "logs", "backups", "snapshots", "x y"] {
            assert!(meeting_file_dirs(d, c, bad).is_empty(), "{:?}", bad);
        }
    }
}

/// Get synced timeline for rewind (frames + transcripts aligned by timestamp)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_synced_timeline(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SyncedTimeline>, String> {
    log::info!("📊 get_synced_timeline called for meeting: {}", meeting_id);
    state
        .database
        .get_synced_timeline(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get synced timeline: {}", e))
}

/// Get timeline events for a meeting (Phase 3)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_timeline_events(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::TimelineEventRecord>, String> {
    state
        .database
        .get_timeline_events(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get timeline events: {}", e))
}

/// Get topic clusters for a meeting (Phase 3)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_topic_clusters(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::TopicClusterRecord>, String> {
    state
        .database
        .get_topic_clusters(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get topic clusters: {}", e))
}

// ============================================
// Accessibility & Meeting Timeline Commands
// ============================================

/// Response for accessibility snapshots
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessibilitySnapshot {
    pub snapshot_id: String,
    pub meeting_id: Option<String>,
    pub ts: String,
    pub text: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub quality_score: f32,
    pub word_count: i32,
}

/// Unified timeline entry for synced Rewind view
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineEntry {
    pub id: String,
    pub entry_type: String, // "transcript", "accessibility", "screenshot"
    pub timestamp: String,
    pub text: Option<String>,
    pub speaker: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub image_path: Option<String>,
    pub confidence: Option<f32>,
}

/// Full meeting timeline response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeetingTimeline {
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub entries: Vec<TimelineEntry>,
    pub transcript_count: usize,
    pub accessibility_count: usize,
    pub screenshot_count: usize,
}

/// Get accessibility snapshots for a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_accessibility_snapshots(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AccessibilitySnapshot>, String> {
    let snapshots = state
        .database
        .get_text_snapshots_by_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get snapshots: {}", e))?;

    Ok(snapshots
        .into_iter()
        .map(|s| AccessibilitySnapshot {
            snapshot_id: s.snapshot_id,
            meeting_id: s.meeting_id,
            ts: s.ts,
            text: s.text,
            app_name: s.app_name,
            window_title: s.window_title,
            quality_score: s.quality_score,
            word_count: s.word_count,
        })
        .collect())
}

/// Get unified meeting timeline with transcripts, accessibility snapshots, and screenshots
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_timeline(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<MeetingTimeline, String> {
    // Get meeting info
    let meeting = state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?
        .ok_or_else(|| format!("Meeting not found: {}", meeting_id))?;

    // Get transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    // Get accessibility snapshots
    let acc_snapshots = state
        .database
        .get_text_snapshots_by_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get accessibility snapshots: {}", e))?;

    // Get frames/screenshots
    let frames = state
        .database
        .get_frames(&meeting_id, 1000)
        .await
        .map_err(|e| format!("Failed to get frames: {}", e))?;

    // Build timeline entries
    let mut entries: Vec<TimelineEntry> = Vec::new();

    // Add transcripts
    for t in &transcripts {
        if t.is_final && !t.text.trim().is_empty() {
            entries.push(TimelineEntry {
                id: format!("t_{}", t.id),
                entry_type: "transcript".to_string(),
                timestamp: t.timestamp.to_rfc3339(),
                text: Some(t.text.clone()),
                speaker: t.speaker.clone(),
                app_name: None,
                window_title: None,
                image_path: None,
                confidence: Some(t.confidence),
            });
        }
    }

    // Add accessibility snapshots
    for s in &acc_snapshots {
        entries.push(TimelineEntry {
            id: format!("a_{}", s.snapshot_id),
            entry_type: "accessibility".to_string(),
            timestamp: s.ts.clone(),
            text: Some(s.text.clone()),
            speaker: None,
            app_name: s.app_name.clone(),
            window_title: s.window_title.clone(),
            image_path: None,
            confidence: None,
        });
    }

    // Add screenshots/frames
    for f in &frames {
        if let Some(path) = &f.file_path {
            entries.push(TimelineEntry {
                id: format!("s_{}", f.id),
                entry_type: "screenshot".to_string(),
                timestamp: f.timestamp.to_rfc3339(),
                text: f.ocr_text.clone(),
                speaker: None,
                app_name: None,
                window_title: None,
                image_path: Some(path.clone()),
                confidence: None,
            });
        }
    }

    // Screen states (the capture pipeline since stateful ingest): one keyframe
    // per distinct screen, per display/window
    let screen_states = state
        .database
        .get_screen_states(&meeting_id, 20000)
        .await
        .unwrap_or_default();
    for st in &screen_states {
        if let Some(path) = &st.keyframe_path {
            if path.is_empty() {
                continue;
            }
            entries.push(TimelineEntry {
                id: format!("ss_{}", st.state_id),
                entry_type: "screenshot".to_string(),
                timestamp: st.start_ts.clone(),
                text: None,
                speaker: None,
                app_name: st.app_name.clone(),
                window_title: st.window_title.clone(),
                image_path: Some(path.clone()),
                confidence: None,
            });
        }
    }

    // Sort by actual time (RFC3339 strings with differing precision/offsets
    // don't sort lexically)
    entries.sort_by_key(|e| {
        chrono::DateTime::parse_from_rfc3339(&e.timestamp)
            .map(|d| d.timestamp_micros())
            .unwrap_or(0)
    });
    let screenshot_count = entries.iter().filter(|e| e.entry_type == "screenshot").count();

    Ok(MeetingTimeline {
        meeting_id: meeting.id,
        title: meeting.title,
        started_at: meeting.started_at.to_rfc3339(),
        ended_at: meeting.ended_at.map(|e| e.to_rfc3339()),
        transcript_count: transcripts.iter().filter(|t| t.is_final).count(),
        accessibility_count: acc_snapshots.len(),
        screenshot_count,
        entries,
    })
}

// ============================================
// Capture Mode Settings Commands
// ============================================

/// Set capture microphone toggle
#[tauri::command(rename_all = "camelCase")]
pub async fn set_capture_microphone(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_capture_microphone(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set capture system audio toggle
#[tauri::command(rename_all = "camelCase")]
pub async fn set_capture_system_audio(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_capture_system_audio(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set capture screen toggle
#[tauri::command(rename_all = "camelCase")]
pub async fn set_capture_screen(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set_capture_screen(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set always-on capture toggle
#[tauri::command(rename_all = "camelCase")]
pub async fn set_always_on_capture(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_always_on_capture(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set queue frames for VLM toggle
#[tauri::command(rename_all = "camelCase")]
pub async fn set_queue_frames_for_vlm(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_queue_frames_for_vlm(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set frame capture interval (ms)
#[tauri::command(rename_all = "camelCase")]
pub async fn set_frame_capture_interval(
    interval_ms: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_frame_capture_interval(interval_ms)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Get all capture settings
#[tauri::command(rename_all = "camelCase")]
pub async fn get_capture_settings(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))?;

    Ok(serde_json::json!({
        "capture_microphone": settings.capture_microphone,
        "capture_system_audio": settings.capture_system_audio,
        "capture_screen": settings.capture_screen,
        "always_on_capture": settings.always_on_capture,
        "queue_frames_for_vlm": settings.queue_frames_for_vlm,
        "frame_capture_interval_ms": settings.frame_capture_interval_ms,
    }))
}

// ============================================
// VLM Processing Commands (Phase 4)
// ============================================

use crate::database::ActivityLogEntry;

/// Result of VLM analysis batch
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisResult {
    pub frames_processed: usize,
    pub activities_created: usize,
    pub errors: Vec<String>,
}

/// Analyze pending frames with VLM
#[tauri::command(rename_all = "camelCase")]
pub async fn analyze_pending_frames(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<AnalysisResult, String> {
    let limit = limit.unwrap_or(10);

    // Check if VLM is available
    if !crate::vlm_client::vlm_is_available().await {
        // Resolve again to return the precise reason (consent, no vision model, …)
        return Err(crate::ai::client::resolve(crate::ai::Kind::Vision)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "No vision model is set up (Settings → AI Engine).".to_string()));
    }

    // Get pending frames
    let pending = state
        .database
        .get_pending_frames(limit)
        .await
        .map_err(|e| format!("Failed to get pending frames: {}", e))?;

    if pending.is_empty() {
        return Ok(AnalysisResult {
            frames_processed: 0,
            activities_created: 0,
            errors: vec![],
        });
    }

    // Get active theme and prompt (with fallback logic similar to Scheduler)
    let mut frames_processed = 0;
    let mut activities_created = 0;
    let mut errors = Vec::new();

    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .unwrap_or_else(|_| "prospecting".to_string());
    let prompt_key = format!("{}_context_analysis", active_theme);

    let prompt = match state.prompt_manager.get_prompt(&prompt_key).await {
        Ok(Some(p)) => p.system_prompt,
        Ok(None) => {
            // Try fallback to generic frame_analysis
            match state.prompt_manager.get_prompt("frame_analysis").await {
                Ok(Some(p)) => p.system_prompt,
                _ => {
                    // Hard fallback
                    r#"Analyze this screenshot and describe what the user is doing. 
                    Respond in JSON format with these fields:
                    {
                      "app_name": "name of the main application visible",
                      "window_title": "title of the window or document",
                      "category": "one of: development, communication, research, writing, design, media, browsing, system, other",
                      "summary": "brief description of what the user is doing",
                      "focus_area": "specific task or project",
                      "visible_files": [],
                      "confidence": 0.8
                    }
                    Only respond with valid JSON."#.to_string()
                }
            }
        }
        Err(e) => {
            return Err(format!("Failed to retrieve prompt: {}", e));
        }
    };

    for frame in pending {
        // Analyze frame with VLM (standalone function)
        match crate::vlm_client::vlm_analyze_frame(&frame.frame_path, &prompt).await {
            Ok(context) => {
                frames_processed += 1;

                // Create activity log entry
                let activity = ActivityLogEntry {
                    id: None,
                    start_time: frame.captured_at,
                    end_time: None,
                    duration_seconds: None,
                    app_name: context.app_name,
                    window_title: context.window_title,
                    category: context.category,
                    summary: context.summary,
                    focus_area: context.focus_area,
                    visible_files: if context.visible_files.is_empty() {
                        None
                    } else {
                        Some(context.visible_files.join(", "))
                    },
                    confidence: Some(context.confidence),
                    frame_ids: Some(frame.id.to_string()),
                };

                // Store in activity_log
                match state.database.add_activity(&activity).await {
                    Ok(activity_id) => {
                        activities_created += 1;

                        // Phase 3: Extract and store entities (Identical logic to Scheduler)
                        if let Some(entities_json) = context.entities {
                            if let Some(obj) = entities_json.as_object() {
                                for (entity_type, list) in obj {
                                    if let Some(items) = list.as_array() {
                                        for item in items {
                                            if let Some(name) =
                                                item.get("name").and_then(|s| s.as_str())
                                            {
                                                let conf = item
                                                    .get("confidence")
                                                    .and_then(|c| c.as_f64())
                                                    .or_else(|| {
                                                        item.get("confidence")
                                                            .and_then(|s| s.as_str().map(|_| 0.8))
                                                    })
                                                    .map(|f| f as f32)
                                                    .unwrap_or(context.confidence);

                                                let _ = state
                                                    .database
                                                    .add_entity(
                                                        activity_id,
                                                        entity_type,
                                                        name,
                                                        Some(item),
                                                        conf,
                                                        Some(&active_theme),
                                                    )
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Mark frame as analyzed
                        let _ = state.database.mark_frame_analyzed(frame.id).await;
                    }
                    Err(e) => {
                        errors.push(format!("Failed to store activity: {}", e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "VLM analysis failed for {}: {}",
                    frame.frame_path, e
                ));
            }
        }
    }

    log::info!(
        "🔍 VLM Analysis: {} frames processed, {} activities created",
        frames_processed,
        activities_created
    );

    Ok(AnalysisResult {
        frames_processed,
        activities_created,
        errors,
    })
}

/// Get pending frame count
#[tauri::command(rename_all = "camelCase")]
pub async fn get_pending_frame_count(state: State<'_, AppState>) -> Result<i64, String> {
    state
        .database
        .count_unsynced_frames()
        .await
        .map_err(|e| format!("Failed to count frames: {}", e))
}

/// Get activity stats for today
#[tauri::command(rename_all = "camelCase")]
pub async fn get_activity_stats(
    date: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let date = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    state
        .database
        .get_activity_stats(&date)
        .await
        .map_err(|e| format!("Failed to get stats: {}", e))
}

// ============================================
// Search Commands (Phase 6)
// ============================================

/// Knowledge base search result (all local: SQLite activity_log + transcript FTS)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KBSearchResult {
    pub id: String,
    pub source: String, // always "local"
    pub timestamp: Option<String>,
    pub app_name: Option<String>,
    pub category: Option<String>,
    pub summary: String,
    pub score: Option<f32>,
}

/// Search options
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchOptions {
    pub query: Option<String>,        // Free-text query (transcript FTS + activity match)
    pub start_date: Option<String>,   // ISO date for time range
    pub end_date: Option<String>,     // ISO date for time range
    pub category: Option<String>,     // Filter by category
    pub limit: Option<u32>,           // Max results
    pub sources: Option<Vec<String>>, // accepted for compatibility; everything is local
}

/// Search the local knowledge base: activity_log plus meeting transcripts
/// (SQLite FTS5, bm25-ranked). Nothing leaves the device.
#[tauri::command(rename_all = "camelCase")]
pub async fn search_knowledge_base(
    options: SearchOptions,
    state: State<'_, AppState>,
) -> Result<Vec<KBSearchResult>, String> {
    let mut results = Vec::new();
    let limit = options.limit.unwrap_or(20) as i32;

    // Search local SQLite activity_log
    {
        let local_activities = state
            .database
            .get_activities_filtered(
                options.start_date.as_deref(),
                options.end_date.as_deref(),
                options.category.as_deref(),
                limit,
            )
            .await
            .unwrap_or_default();

        for activity in local_activities {
            // Filter by query if provided (simple text match)
            if let Some(ref query) = options.query {
                let query_lower = query.to_lowercase();
                let matches = activity.summary.to_lowercase().contains(&query_lower)
                    || activity.category.to_lowercase().contains(&query_lower)
                    || activity
                        .focus_area
                        .as_ref()
                        .map(|f| f.to_lowercase().contains(&query_lower))
                        .unwrap_or(false);
                if !matches {
                    continue;
                }
            }

            results.push(KBSearchResult {
                id: activity.id.map(|i| i.to_string()).unwrap_or_default(),
                source: "local".to_string(),
                timestamp: Some(activity.start_time.to_rfc3339()),
                app_name: activity.app_name,
                category: Some(activity.category),
                summary: activity.summary,
                score: activity.confidence,
            });
        }

        // Also search meeting transcripts via FTS5 (bm25-ranked) — the
        // richest local data.
        if let Some(fts_query) = options
            .query
            .as_deref()
            .and_then(crate::database::fts_or_query)
        {
            if let Ok(hits) = state
                .database
                .search_transcript_context(&fts_query, limit as i64)
                .await
            {
                for hit in hits {
                    results.push(KBSearchResult {
                        id: format!("transcript-{}-{}", hit.meeting_id, hit.transcript_id),
                        source: "local".to_string(),
                        timestamp: Some(hit.timestamp),
                        app_name: None,
                        category: Some("transcript".to_string()),
                        summary: format!("[{}] {}", hit.meeting_title, hit.snippet),
                        // bm25 relevance is negative-is-better; normalize
                        // to a rough 0-1 confidence for ranking
                        score: Some((1.0 / (1.0 + hit.relevance.abs())) as f32),
                    });
                }
            }
        }
    }

    // Sort by score, then by timestamp
    results.sort_by(|a, b| match (&b.score, &a.score) {
        (Some(bs), Some(as_)) => bs.partial_cmp(as_).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.timestamp.cmp(&a.timestamp),
    });

    // Limit results
    results.truncate(limit as usize);

    log::info!("🔍 Knowledge base search: {} results", results.len());
    Ok(results)
}

/// Get local activity history (from activity_log)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_local_activities(
    start_date: Option<String>,
    end_date: Option<String>,
    category: Option<String>,
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::ActivityLogEntry>, String> {
    state
        .database
        .get_activities_filtered(
            start_date.as_deref(),
            end_date.as_deref(),
            category.as_deref(),
            limit.unwrap_or(50),
        )
        .await
        .map_err(|e| format!("Failed to get activities: {}", e))
}

/// Clear cache - remove pending frames and temporary data
#[tauri::command(rename_all = "camelCase")]
pub async fn clear_cache(state: State<'_, AppState>) -> Result<(), String> {
    // Clear frame queue
    state
        .database
        .clear_frame_queue()
        .await
        .map_err(|e| format!("Failed to clear frame queue: {}", e))?;

    // Clear activity log (optional - could be configurable)
    state
        .database
        .clear_activity_log()
        .await
        .map_err(|e| format!("Failed to clear activity log: {}", e))?;

    log::info!("Cache cleared successfully");
    Ok(())
}

/// Export all data as JSON
#[tauri::command(rename_all = "camelCase")]
pub async fn export_data(state: State<'_, AppState>) -> Result<String, String> {
    // Get all meetings
    let meetings = state
        .database
        .list_meetings(1000)
        .await
        .map_err(|e| format!("Failed to get meetings: {}", e))?;

    // Get transcripts for each meeting
    let mut export_data = serde_json::json!({
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "version": "1.0.0",
        "meetings": []
    });

    let meetings_array = export_data["meetings"].as_array_mut().unwrap();

    for meeting in meetings {
        let transcripts = state
            .database
            .get_transcripts(&meeting.id)
            .await
            .unwrap_or_default();
        let frames = state
            .database
            .get_frames(&meeting.id, 1000)
            .await
            .unwrap_or_default();

        // Markers only (when/why), never content
        let stricken_screens: Vec<serde_json::Value> = crate::redaction::list_strikes(
            state.database.pool(),
            &meeting.id,
        )
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.kind == "screen")
        .map(|r| {
            serde_json::json!({
                "at": r.media_start,
                "text": crate::redaction::SCREEN_STRICKEN_PLACEHOLDER,
                "count": r.item_count,
                "reason": r.reason,
            })
        })
        .collect();

        meetings_array.push(serde_json::json!({
            "id": meeting.id,
            "title": meeting.title,
            "started_at": meeting.started_at,
            "ended_at": meeting.ended_at,
            "duration_seconds": meeting.duration_seconds,
            "transcripts": transcripts,
            "frame_count": frames.len(),
            "stricken_screens": stricken_screens,
        }));
    }

    // Get activity log
    let activities = state
        .database
        .get_activities_filtered(None, None, None, 1000)
        .await
        .unwrap_or_default();
    export_data["activities"] = serde_json::to_value(activities).unwrap_or(serde_json::json!([]));

    serde_json::to_string_pretty(&export_data)
        .map_err(|e| format!("Failed to serialize export data: {}", e))
}

// ============================================
// Meeting Intelligence Commands
// ============================================
// Calendar Commands
// ============================================

use crate::calendar_client::{CalendarClient, CalendarEventNative};

/// Get calendar events for today/tomorrow
#[tauri::command(rename_all = "camelCase")]
pub async fn get_calendar_events(
    state: State<'_, AppState>,
) -> Result<Vec<CalendarEventNative>, String> {
    // Request access if needed (static method, no instance needed)
    if !CalendarClient::request_access().await? {
        return Err("Calendar access denied".to_string());
    }

    let client = state.calendar_client.read();
    client.fetch_events()
}

// ============================================
// Intelligence / Meeting State Commands
// ============================================

use crate::catch_up_agent::{CatchUpAgent, CatchUpCapsule, MeetingMetadata, TranscriptSegment};
use crate::live_intel_agent::{LiveInsightEvent, LiveIntelAgent};

/// Get current meeting state (mode, timing, confidence)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_state(state: State<'_, AppState>) -> Result<MeetingState, String> {
    let resolver = MeetingStateResolver::new();
    let now = chrono::Utc::now();

    // Get recording status to check if transcript is running
    let is_transcribing = {
        let engine = state.capture_engine.read();
        engine.get_status().is_recording
    };

    // Get calendar events from shared client
    // We only try to fetch if we have access, otherwise we proceed with empty list
    // to avoid blocking or errors during state polling.
    let calendar_events: Vec<CalendarEvent> = {
        let client = state.calendar_client.read();
        if let Ok(events) = client.fetch_events() {
            events
                .into_iter()
                .map(|e| CalendarEvent {
                    id: e.event_id,
                    title: e.title,
                    start_time: e.start_time,
                    end_time: e.end_time,
                    attendees: e.attendees,
                    description: e.notes,
                    meeting_url: e.meeting_url,
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    // Placeholder for future active window detection
    let active_window: Option<&str> = None;

    // Audio activity is currently tied to transcription status
    let audio_active = is_transcribing;

    let meeting_state = resolver.resolve(
        now,
        &calendar_events,
        is_transcribing,
        active_window,
        audio_active,
    );

    Ok(meeting_state)
}

/// Generate a catch-up capsule for late joiners
#[tauri::command(rename_all = "camelCase")]
pub async fn generate_catch_up(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<CatchUpCapsule, String> {
    // Get transcripts for the meeting
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    if transcripts.is_empty() {
        return Ok(CatchUpCapsule::default());
    }

    // Convert to segments
    let segments: Vec<TranscriptSegment> = transcripts
        .iter()
        .map(|t| TranscriptSegment {
            id: t.id.to_string(),
            timestamp_ms: t.timestamp.timestamp_millis(),
            speaker: t.speaker.clone(),
            text: t.text.clone(),
        })
        .collect();

    // Get meeting info
    let meeting = state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?;

    let metadata = MeetingMetadata {
        title: meeting
            .as_ref()
            .map(|m| m.title.clone())
            .unwrap_or_default(),
        description: None,
        attendees: Vec::new(), // TODO: Get from calendar
        scheduled_duration_min: None,
    };

    // Calculate minutes since start
    let meeting_start = meeting
        .as_ref()
        .map(|m| m.started_at)
        .unwrap_or_else(chrono::Utc::now);
    let duration = chrono::Utc::now().signed_duration_since(meeting_start);
    let minutes_since_start = duration.num_minutes() as i32;

    // Create agent and generate catch-up
    let ai_client = crate::ai_client::AIClient::new();
    let agent = CatchUpAgent::new(ai_client);

    // Resolve persona-specific catch-up prompt from PromptManager
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .unwrap_or_else(|_| "personal".to_string());

    let prompt_name = format!("catch_up_capsule_{}", active_theme);
    let custom_system_prompt = state
        .prompt_manager
        .get_prompt_by_name(&prompt_name, Some(&active_theme))
        .await
        .ok()
        .flatten()
        .map(|p| p.system_prompt);

    agent
        .generate(
            &segments,
            &metadata,
            minutes_since_start,
            None,
            custom_system_prompt.as_deref(),
        )
        .await
}

/// Get live insights stream for current recording
#[tauri::command(rename_all = "camelCase")]
pub async fn get_live_insights(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<LiveInsightEvent>, String> {
    // "Live insights during meetings" off → no rule-based events AND no AI
    // call. The AI phase below sends the transcript to the user's provider,
    // so the switch must gate it too (privacy policy promises an off switch).
    // Read the saved setting too: the in-memory flag defaults to on until
    // startup loads it, and an "off" choice must hold from the first call.
    let saved = state
        .settings
        .get(crate::live_intel_agent::SETTING_ENABLED)
        .await
        .ok()
        .flatten();
    if !crate::live_intel_agent::is_enabled()
        || !crate::live_intel_agent::parse_enabled(saved.as_deref())
    {
        return Ok(Vec::new());
    }

    // Get recent transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    // Phase 1: Fast rule-based extraction
    let mut agent = LiveIntelAgent::new();

    let segments: Vec<TranscriptSegment> = transcripts
        .iter()
        .rev()
        .take(50)
        .rev()
        .map(|t| TranscriptSegment {
            id: t.id.to_string(),
            timestamp_ms: t.timestamp.timestamp_millis(),
            speaker: t.speaker.clone(),
            text: t.text.clone(),
        })
        .collect();

    for segment in &segments {
        agent.process_segment(segment.clone());
    }

    let mut all_events = agent.get_all_events().to_vec();

    // Phase 2: AI-powered deep analysis (if prompt available)
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .unwrap_or_else(|_| "personal".to_string());

    let intel_prompt_name = format!("live_intel_system_{}", active_theme);
    if let Ok(Some(db_prompt)) = state
        .prompt_manager
        .get_prompt_by_name(&intel_prompt_name, Some(&active_theme))
        .await
    {
        // Run AI analysis on recent segments (last 20 for performance)
        let recent_segments: Vec<_> = segments.iter().rev().take(20).rev().cloned().collect();
        let ai_events = agent
            .ai_analyze(&recent_segments, &db_prompt.system_prompt)
            .await;
        all_events.extend(ai_events);
    }

    Ok(all_events)
}

/// Pin an insight for later reference
#[tauri::command(rename_all = "camelCase")]
pub async fn pin_insight(
    meeting_id: String,
    insight_type: String,
    insight_text: String,
    _timestamp_ms: i64,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // Store pinned insight in database
    // TODO: Add pinned_insights table
    log::info!(
        "Pinning insight for meeting {}: {} - {}",
        meeting_id,
        insight_type,
        insight_text
    );
    Ok(())
}

/// Mark a decision point explicitly
#[tauri::command(rename_all = "camelCase")]
pub async fn mark_decision(
    meeting_id: String,
    decision_text: String,
    _context: Option<String>,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // Store decision in database
    // TODO: Add decisions table
    log::info!(
        "Marking decision for meeting {}: {}",
        meeting_id,
        decision_text
    );
    Ok(())
}

// ============================================================================
// Video Recording Commands
// ============================================================================

// m1: compiled out of the Mac App Store build (ffmpeg is not available in
// the App Sandbox). Screenshots from capture_engine still feed the timeline.
#[cfg(not(feature = "mas"))]
pub use video::*;

#[cfg(not(feature = "mas"))]
mod video {
use super::*;
use crate::chunk_manager::{ChunkManager, StorageStats};
use crate::frame_extractor::{ExtractedFrame, FrameExtractor};
use crate::video_recorder::{PinMoment, RecordingSession, VideoRecorder};

// Lazy static for video recorder (global instance)
use std::sync::OnceLock;
static VIDEO_RECORDER: OnceLock<parking_lot::RwLock<VideoRecorder>> = OnceLock::new();
static FRAME_EXTRACTOR: OnceLock<FrameExtractor> = OnceLock::new();
static CHUNK_MANAGER: OnceLock<ChunkManager> = OnceLock::new();

pub(super) fn get_video_recorder() -> &'static parking_lot::RwLock<VideoRecorder> {
    VIDEO_RECORDER.get_or_init(|| parking_lot::RwLock::new(VideoRecorder::default()))
}

/// Meeting whose screen video is being recorded right now, if any (its
/// current chunk can't be re-encoded by a screen delete/strike).
pub fn video_recording_meeting() -> Option<String> {
    let rec = VIDEO_RECORDER.get()?;
    let status = rec.read().get_status()?;
    status.is_active.then_some(status.meeting_id)
}

fn get_frame_extractor() -> &'static FrameExtractor {
    FRAME_EXTRACTOR.get_or_init(FrameExtractor::default)
}

fn get_chunk_manager() -> &'static ChunkManager {
    CHUNK_MANAGER.get_or_init(ChunkManager::default)
}

/// Start video recording for a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn start_video_recording(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let recorder = get_video_recorder();
    recorder.write().start(&meeting_id)?;

    // Prevent sleep during video recording
    let _ = state
        .power_manager
        .prevent_sleep("Video Recording Active")
        .map_err(|e| log::warn!("Failed to prevent sleep: {}", e));
    Ok(())
}

/// Stop video recording
#[tauri::command(rename_all = "camelCase")]
pub async fn stop_video_recording(state: State<'_, AppState>) -> Result<RecordingSession, String> {
    let recorder = get_video_recorder();
    let result = recorder.write().stop();

    // Release sleep assertion
    state.power_manager.release_assertion();

    result
}

/// Get current video recording status
#[tauri::command(rename_all = "camelCase")]
pub async fn get_video_recording_status() -> Result<Option<RecordingSession>, String> {
    let recorder = get_video_recorder();
    Ok(recorder.read().get_status())
}

/// Pin the current moment in recording
#[tauri::command(rename_all = "camelCase")]
pub async fn video_pin_moment(label: Option<String>) -> Result<PinMoment, String> {
    let recorder = get_video_recorder();
    recorder.read().pin_moment(label)
}

/// Extract a frame at a specific timestamp
#[tauri::command(rename_all = "camelCase")]
pub async fn extract_frame_at(
    meeting_id: String,
    chunk_number: u32,
    timestamp_secs: f64,
) -> Result<ExtractedFrame, String> {
    let chunk_manager = get_chunk_manager();
    let chunks = chunk_manager.get_chunks(&meeting_id)?;

    let chunk = chunks
        .iter()
        .find(|c| c.chunk_number == chunk_number)
        .ok_or_else(|| format!("Chunk {} not found", chunk_number))?;

    let extractor = get_frame_extractor();
    extractor.extract_at(&chunk.path, timestamp_secs, &meeting_id)
}

/// Extract thumbnail for timeline view
#[tauri::command(rename_all = "camelCase")]
pub async fn extract_thumbnail(
    meeting_id: String,
    chunk_number: u32,
    timestamp_secs: f64,
    size: Option<u32>,
) -> Result<String, String> {
    let chunk_manager = get_chunk_manager();
    let chunks = chunk_manager.get_chunks(&meeting_id)?;

    let chunk = chunks
        .iter()
        .find(|c| c.chunk_number == chunk_number)
        .ok_or_else(|| format!("Chunk {} not found", chunk_number))?;

    let extractor = get_frame_extractor();
    let thumb_path = extractor.extract_thumbnail(
        &chunk.path,
        timestamp_secs,
        &meeting_id,
        size.unwrap_or(200),
    )?;

    Ok(thumb_path.to_string_lossy().to_string())
}

/// Get storage statistics
#[tauri::command(rename_all = "camelCase")]
pub async fn get_storage_stats() -> Result<StorageStats, String> {
    let manager = get_chunk_manager();
    manager.get_stats()
}

/// Apply retention policies
#[tauri::command(rename_all = "camelCase")]
pub async fn apply_retention() -> Result<(u32, u64), String> {
    let manager = get_chunk_manager();
    manager.apply_retention()
}

/// Delete a meeting's video storage
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_video_storage(meeting_id: String) -> Result<u64, String> {
    let manager = get_chunk_manager();
    manager.delete_meeting(&meeting_id)
}
}

// ============================================
// VLM Scheduler Commands
// ============================================

/// Set VLM auto-processing enabled/disabled
#[tauri::command(rename_all = "camelCase")]
pub async fn set_vlm_auto_process(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    // Save to settings
    state
        .settings
        .set_vlm_auto_process(enabled)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    // Update scheduler
    state.vlm_scheduler.set_enabled(enabled);

    if enabled {
        state.vlm_scheduler.start();
    }

    log::info!("VLM auto-processing set to: {}", enabled);
    Ok(())
}

/// Set VLM processing interval in seconds
#[tauri::command(rename_all = "camelCase")]
pub async fn set_vlm_process_interval(secs: u32, state: State<'_, AppState>) -> Result<(), String> {
    // Save to settings
    state
        .settings
        .set_vlm_process_interval(secs)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    // Update scheduler
    state.vlm_scheduler.set_interval(secs);

    log::info!("VLM processing interval set to: {}s", secs);
    Ok(())
}

/// Get VLM scheduler status
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vlm_scheduler_status(
    state: State<'_, AppState>,
) -> Result<crate::vlm_scheduler::VLMSchedulerStatus, String> {
    Ok(state.vlm_scheduler.get_status().await)
}

// ============================================
// AI Chat Model Commands
// ============================================

/// Set the AI chat model
#[tauri::command(rename_all = "camelCase")]
pub async fn set_ai_chat_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set_ai_chat_model(&model)
        .await
        .map_err(|e| format!("Failed to save model: {}", e))?;

    log::info!("AI chat model set to: {}", model);
    Ok(())
}

/// Get the AI chat model
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ai_chat_model(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .settings
        .get_ai_chat_model()
        .await
        .map_err(|e| format!("Failed to get model: {}", e))
}

// ============================================
// Accessibility Capture Commands
// ============================================

/// Get accessibility capture status
#[tauri::command(rename_all = "camelCase")]
pub async fn get_accessibility_capture_status(
    state: State<'_, AppState>,
) -> Result<crate::accessibility_capture::AccessibilityCaptureStats, String> {
    Ok(state.accessibility_capture.get_stats())
}

/// Start accessibility capture
#[tauri::command(rename_all = "camelCase")]
pub async fn start_accessibility_capture(state: State<'_, AppState>) -> Result<(), String> {
    // Also enable in settings
    state
        .settings
        .set_accessibility_capture_enabled(true)
        .await
        .map_err(|e| format!("Failed to enable setting: {}", e))?;

    // Update config in service
    state.accessibility_capture.update_config(
        crate::accessibility_capture::AccessibilityCaptureConfig {
            enabled: true,
            interval_secs: 10,
            min_word_count: 5,
            deduplicate: true,
        },
    );

    // Start the service if not running
    if !state.accessibility_capture.is_running() {
        state
            .accessibility_capture
            .start(state.database.clone(), state.settings.clone())?;
    }

    log::info!("📝 Accessibility capture started");
    Ok(())
}

/// Stop accessibility capture
#[tauri::command(rename_all = "camelCase")]
pub async fn stop_accessibility_capture(state: State<'_, AppState>) -> Result<(), String> {
    // Disable in settings
    state
        .settings
        .set_accessibility_capture_enabled(false)
        .await
        .map_err(|e| format!("Failed to disable setting: {}", e))?;

    // Stop the service and clear meeting ID
    state.accessibility_capture.set_meeting_id(None);
    state.accessibility_capture.stop();

    log::info!("📝 Accessibility capture stopped");
    Ok(())
}

/// Set the current meeting ID for accessibility captures
/// Call this when starting a meeting to link captures to the meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn set_accessibility_meeting_id(
    meeting_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .accessibility_capture
        .set_meeting_id(meeting_id.clone());
    log::info!(
        "📝 Accessibility capture meeting ID set to: {:?}",
        meeting_id
    );
    Ok(())
}

// ===== Calendar Integration Commands =====

/// Check if calendar access is authorized (macOS EventKit)
#[tauri::command(rename_all = "camelCase")]
pub async fn check_calendar_access() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::calendar_client::{CalendarAccessStatus, CalendarClient};
        let status = CalendarClient::check_access();
        Ok(status == CalendarAccessStatus::Authorized)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

/// Request calendar access permission
#[tauri::command(rename_all = "camelCase")]
pub async fn request_calendar_access() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::calendar_client::CalendarClient;
        // Request access with a polling-based approach (static method)
        CalendarClient::request_access().await
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Calendar access only available on macOS".to_string())
    }
}

/// Get the current/active meeting from calendar (if any)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_current_meeting() -> Result<Option<serde_json::Value>, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::calendar_client::CalendarClient;

        let client = CalendarClient::new();
        match client.get_current_event() {
            Some(event) => Ok(Some(serde_json::json!({
                "id": event.event_id,
                "title": event.title,
                "start_time": event.start_time.to_rfc3339(),
                "end_time": event.end_time.to_rfc3339(),
                "location": event.location,
                "notes": event.notes,
                "is_all_day": event.is_all_day,
                "calendar_name": event.calendar_name,
            }))),
            None => Ok(None),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

/// Get upcoming meetings for today
#[tauri::command(rename_all = "camelCase")]
pub async fn get_upcoming_meetings(hours: Option<i64>) -> Result<Vec<serde_json::Value>, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::calendar_client::CalendarClient;
        use chrono::{Duration, Utc};

        let _hours = hours.unwrap_or(24);
        let now = Utc::now();
        let lookahead = now + Duration::hours(hours.unwrap_or(24));

        let client = CalendarClient::new();
        let events = client.fetch_events()?;

        // Filter to upcoming events (not all-day, starts in future within lookahead)
        let json_events: Vec<serde_json::Value> = events
            .iter()
            .filter(|e| !e.is_all_day && e.start_time > now && e.start_time <= lookahead)
            .map(|e| {
                serde_json::json!({
                    "id": e.event_id,
                    "title": e.title,
                    "start_time": e.start_time.to_rfc3339(),
                    "end_time": e.end_time.to_rfc3339(),
                    "location": e.location,
                    "calendar_name": e.calendar_name,
                })
            })
            .collect();

        Ok(json_events)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![])
    }
}

// ===== Capture Metrics Commands =====

/// Get capture metrics report for the current or last meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_capture_metrics(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    match state.metrics_collector.snapshot() {
        Some(metrics) => Ok(serde_json::json!({
            "meeting_id": metrics.meeting_id,
            "frames_processed": metrics.frames_in,
            "states_created": metrics.states_out,
            "duplicates_skipped": metrics.duplicates_skipped,
            "images_written": metrics.images_written,
            "bytes_saved": metrics.bytes_saved_estimate,
            "bytes_saved_formatted": format_bytes(metrics.bytes_saved_estimate),
            "dedup_percentage": if metrics.frames_in > 0 {
                100.0 * (1.0 - (metrics.states_out as f64 / metrics.frames_in as f64))
            } else {
                0.0
            },
            "ocr_calls": metrics.ocr_calls,
            "snapshots": metrics.snapshots_created,
            "patches": metrics.patches_created,
            "cpu_time_ms": metrics.cpu_time_ms,
        })),
        None => Ok(serde_json::json!({
            "message": "No active meeting"
        })),
    }
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

// ============================================================================
// VIDEO DIAGNOSTICS COMMANDS
// ============================================================================

#[derive(serde::Serialize)]
pub struct CaptureDiagnostics {
    pub monitors: Vec<MonitorInfo>,
    pub current_monitor_id: Option<u32>,
    pub frame_interval_ms: u32,
    pub is_recording: bool,
    pub screen_permission: bool,
    pub mic_permission: bool,
}

/// Get comprehensive capture diagnostics for troubleshooting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_capture_diagnostics(
    state: State<'_, AppState>,
) -> Result<CaptureDiagnostics, String> {
    use crate::capture_engine::CaptureEngine;

    // Get monitor list
    let monitors = CaptureEngine::list_monitors()?;

    // Get capture engine status
    let engine = state.capture_engine.read();
    let status = engine.get_status();
    let frame_interval_ms = 1000; // Default, would need to expose this from engine

    // Check permissions
    #[cfg(target_os = "macos")]
    let screen_permission = check_screen_recording_permission();
    #[cfg(not(target_os = "macos"))]
    let screen_permission = true;

    #[cfg(target_os = "macos")]
    let mic_permission = check_microphone_permission();
    #[cfg(not(target_os = "macos"))]
    let mic_permission = true;

    Ok(CaptureDiagnostics {
        monitors,
        current_monitor_id: None, // Would need to expose from engine
        frame_interval_ms,
        is_recording: status.is_recording,
        screen_permission,
        mic_permission,
    })
}

#[derive(serde::Serialize)]
pub struct TestCaptureResult {
    pub image_base64: String,
    pub actual_width: u32,
    pub actual_height: u32,
    pub expected_width: u32,
    pub expected_height: u32,
    pub monitor_name: String,
    pub dimensions_match: bool,
}

/// Capture a single test frame for diagnostics
#[tauri::command(rename_all = "camelCase")]
pub async fn test_live_capture(_state: State<'_, AppState>) -> Result<TestCaptureResult, String> {
    use xcap::Monitor;

    // Get primary monitor
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
    let monitor = monitors
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .ok_or_else(|| "No primary monitor found".to_string())?;

    let expected_width = monitor.width().unwrap_or(0);
    let expected_height = monitor.height().unwrap_or(0);
    let monitor_name = monitor.name().unwrap_or_else(|_| "Unknown".to_string());

    // Capture test frame
    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture test frame: {}", e))?;

    let actual_width = image.width();
    let actual_height = image.height();

    // Convert to JPEG and base64
    let mut jpeg_bytes = Vec::new();
    let dynamic_image = image::DynamicImage::ImageRgba8(image);
    dynamic_image
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg_bytes),
            image::ImageFormat::Jpeg,
        )
        .map_err(|e| format!("Failed to encode JPEG: {}", e))?;

    let image_base64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);

    // Check if dimensions match (allowing small variance for retina scaling)
    let dimensions_match = actual_width == expected_width && actual_height == expected_height;

    Ok(TestCaptureResult {
        image_base64,
        actual_width,
        actual_height,
        expected_width,
        expected_height,
        monitor_name,
        dimensions_match,
    })
}

/// Start real-time transcription (without recording/saving to disk)
#[tauri::command(rename_all = "camelCase")]
pub async fn start_realtime_transcription(
    app: AppHandle,
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("🎤 Starting real-time transcription for: {}", meeting_id);
    let tm = state.transcription_manager.clone();

    crate::transcription::local_whisper::resolve_model_path()?;

    // Set context so the provider has app_handle, database, meeting_id, etc.
    tm.set_context(
        app,
        state.database.clone(),
        meeting_id.clone(),
        state.live_intel_agent.clone(),
    );

    tm.start();
    log::info!("✅ Real-time transcription started for: {}", meeting_id);
    Ok(())
}

/// Stop real-time transcription
#[tauri::command(rename_all = "camelCase")]
pub async fn stop_realtime_transcription(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("🛑 Stopping real-time transcription");
    let tm = state.transcription_manager.clone();
    tm.stop();
    Ok(())
}

// Meeting Intelligence System Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Generate AI meeting notes from transcripts
#[tauri::command(rename_all = "camelCase")]
pub async fn generate_meeting_notes(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<crate::meeting_notes::GeneratedNotes, String> {
    let ai_client = state.ai_client.read().clone();
    let generator = crate::meeting_notes::MeetingNotesGenerator::new(ai_client);

    generator.generate_notes(&meeting_id, &state.database).await
}

/// Get existing meeting notes
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_notes(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<crate::database::MeetingNotes>, String> {
    state
        .database
        .get_meeting_notes(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting notes: {}", e))
}

// ============================================================================
// v3.0.0: Obsidian Vault Commands → moved to commands/vault.rs
// ============================================================================


