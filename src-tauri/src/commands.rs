// noFriction Meetings - Tauri Commands
// Frontend-callable commands for recording, transcription, frames, and settings

use crate::capture_engine::{
    AudioBuffer, AudioDevice, CapturedFrame, MonitorInfo, RecordingStatus,
};
use crate::database::{Frame, Meeting, SearchResult, SyncedTimeline, Transcript};
use crate::settings::AppSettings;
use crate::transcription::ProviderType;
use crate::{AppState, InitStatus, InitializationState};
use base64::Engine;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Window};

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
}

/// Check macOS permissions (without triggering prompts)
#[tauri::command(rename_all = "camelCase")]
pub async fn check_permissions() -> Result<PermissionStatus, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::accessibility_extractor::AccessibilityExtractor;

        // Check screen recording permission
        let screen_recording = check_screen_recording_permission();

        // Check microphone permission
        let microphone = check_microphone_permission();

        // Check accessibility permission
        let accessibility = AccessibilityExtractor::is_trusted();

        Ok(PermissionStatus {
            screen_recording,
            microphone,
            accessibility,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS, assume all permissions granted
        Ok(PermissionStatus {
            screen_recording: true,
            microphone: true,
            accessibility: true,
        })
    }
}

/// Check screen recording permission on macOS
#[cfg(target_os = "macos")]
fn check_screen_recording_permission() -> bool {
    // Try to list monitors and capture - if it fails, permission is not granted
    use xcap::Monitor;

    match Monitor::all() {
        Ok(monitors) => {
            if let Some(monitor) = monitors.first() {
                // Try to capture an image - this requires screen recording permission
                monitor.capture_image().is_ok()
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

/// Check microphone permission on macOS
#[cfg(target_os = "macos")]
pub fn check_microphone_permission() -> bool {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CString;

    unsafe {
        // AVMediaTypeAudio is "soun"
        let media_type_str = CString::new("soun").unwrap();
        let cls_nsstring = class!(NSString);
        let media_type: *mut Object =
            msg_send![cls_nsstring, stringWithUTF8String:media_type_str.as_ptr()];

        // Get AVCaptureDevice class
        let cls_device = class!(AVCaptureDevice);

        // Check authorization status
        // 0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = Authorized
        let status: i64 = msg_send![cls_device, authorizationStatusForMediaType:media_type];

        // Only return true if strictly Authorized
        // Returning false for NotDetermined prevents the infinite prompt loop
        status == 3
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
                // Trigger screen recording permission prompt by trying to capture
                use xcap::Monitor;

                match Monitor::all() {
                    Ok(monitors) => {
                        if let Some(monitor) = monitors.first() {
                            match monitor.capture_image() {
                                Ok(_) => Ok(true),
                                Err(_) => Ok(false),
                            }
                        } else {
                            Ok(false)
                        }
                    }
                    Err(_) => Ok(false),
                }
            }
            "microphone" => {
                // Trigger microphone permission by trying to access device
                // ONLY if not already determined or denied
                if check_microphone_permission() {
                    return Ok(true);
                }

                use cpal::traits::{DeviceTrait, HostTrait};
                let host = cpal::default_host();
                match host.default_input_device() {
                    Some(device) => match device.default_input_config() {
                        Ok(_) => Ok(true),
                        Err(_) => Ok(false),
                    },
                    None => Ok(false),
                }
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
        let mut provider_type = tm.get_provider_type();

        // Check if the current provider has a stored key (already loaded at startup)
        let has_key = tm.has_key_for_provider(provider_type);

        if !has_key {
            log::warn!(
                "No API key stored for {:?} — checking other providers for fallback",
                provider_type
            );

            // Try to auto-switch to a provider that has a key
            let fallback_providers = [
                ProviderType::Deepgram,
                ProviderType::Gemini,
                ProviderType::Gladia,
                ProviderType::GoogleSTT,
            ];

            let mut found_fallback = false;
            for alt in &fallback_providers {
                if *alt != provider_type && tm.has_key_for_provider(*alt) {
                    log::info!(
                        "Auto-switching transcription from {:?} to {:?} (has key)",
                        provider_type,
                        alt
                    );
                    tm.switch_provider(*alt);
                    provider_type = *alt;
                    // Also persist the switch
                    let provider_str = match alt {
                        ProviderType::Deepgram => "deepgram",
                        ProviderType::Gemini => "gemini",
                        ProviderType::Gladia => "gladia",
                        ProviderType::GoogleSTT => "google_stt",
                    };
                    let _ = state
                        .settings
                        .set_transcription_provider(provider_str)
                        .await;
                    found_fallback = true;
                    break;
                }
            }

            if !found_fallback {
                log::warn!(
                    "No transcription API key configured for any provider - transcription disabled"
                );
            }
        }

        log::info!(
            "Setting up Transcription connection for {:?}...",
            provider_type
        );
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

    let audio_callback: Arc<dyn Fn(AudioBuffer) + Send + Sync> = Arc::new(move |buffer| {
        if buffer.samples.is_empty() {
            return;
        }

        // Queue audio to provider (non-blocking)
        transcription_manager.process_audio(&buffer.samples, buffer.sample_rate, buffer.channels);
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

    // Estimated bytes per frame (for savings calculation)
    const ESTIMATED_FRAME_BYTES: u64 = 50_000; // ~50KB per JPEG

    let frame_callback: Arc<dyn Fn(CapturedFrame) + Send + Sync> = Arc::new(move |frame| {
        let db = db_for_frames.clone();
        let mid = meeting_id_for_frames.clone();
        let dir = frames_dir_clone.clone();
        let builder = state_builder.clone();
        let metrics = metrics_collector.clone();
        let settings = settings_for_frames.clone();

        // Process frame through StateBuilder (stateful dedup)
        tokio::spawn(async move {
            // Start CPU timer
            let timer_start = std::time::Instant::now();

            // Record frame received
            metrics.record_frame();

            // Process through StateBuilder (pHash + delta scoring)
            let result = {
                let builder = builder.read();
                builder.process_frame(frame.image.clone(), frame.timestamp)
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
                        builder.take_pending_keyframe()
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

    // Load frame capture interval from settings BEFORE acquiring lock
    let frame_interval = match state.settings.get_all().await {
        Ok(settings) => settings.frame_capture_interval_ms,
        Err(_) => 1000, // Default to 1 second
    };

    // Set callbacks and start capture
    {
        let engine = state.capture_engine.read();
        engine.set_audio_callback(audio_callback);
        engine.set_frame_callback(frame_callback);
        engine.set_frame_interval(frame_interval);
    }

    {
        let engine = state.capture_engine.read();
        engine.start(app)?;
    }

    log::info!(
        "🎬 Recording started: {} (stateful capture enabled, frames → {:?})",
        meeting_id,
        frames_dir
    );
    Ok(meeting_id)
}

/// Stop recording
#[tauri::command(rename_all = "camelCase")]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<(), String> {
    let was_recording = {
        let engine = state.capture_engine.read();
        engine.get_status().is_recording
    };

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

        if let Some(completed) = final_state {
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
    state
        .database
        .get_transcripts(&meeting_id)
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

/// Set the Deepgram API key (persisted)
#[tauri::command(rename_all = "camelCase")]
pub async fn set_deepgram_api_key(
    api_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Store on the per-provider key map (survives provider switches)
    state
        .transcription_manager
        .set_api_key_for_provider(ProviderType::Deepgram, api_key.clone());

    state
        .settings
        .set_deepgram_api_key(&api_key)
        .await
        .map_err(|e| format!("Failed to save API key: {}", e))?;

    log::info!("Deepgram API key saved and applied");
    Ok(())
}

/// Set the Deepgram Model
#[tauri::command(rename_all = "camelCase")]
pub async fn set_deepgram_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set_deepgram_model(&model)
        .await
        .map_err(|e| format!("Failed to save settings: {}", e))?;
    Ok(())
}

/// Get the Deepgram Model
#[tauri::command(rename_all = "camelCase")]
pub async fn get_deepgram_model(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .settings
        .get_deepgram_model()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Set the Gemini API key
#[tauri::command(rename_all = "camelCase")]
pub async fn set_gemini_api_key(api_key: String, state: State<'_, AppState>) -> Result<(), String> {
    // Store on the per-provider key map (survives provider switches)
    state
        .transcription_manager
        .set_api_key_for_provider(ProviderType::Gemini, api_key.clone());

    state
        .settings
        .set_gemini_api_key(&api_key)
        .await
        .map_err(|e| format!("Failed to save API key: {}", e))?;

    log::info!("Gemini API key saved and applied");
    Ok(())
}

/// Get the Gemini API key (masked)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_gemini_api_key(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .settings
        .get("gemini_api_key")
        .await
        .map(|opt| {
            opt.map(|key| {
                if key.len() > 4 {
                    format!("****{}", &key[key.len() - 4..])
                } else {
                    "****".to_string()
                }
            })
        })
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Set the Gemini Model
#[tauri::command(rename_all = "camelCase")]
pub async fn set_gemini_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set_gemini_model(&model)
        .await
        .map_err(|e| format!("Failed to save settings: {}", e))?;
    Ok(())
}

/// Get the Gemini Model
#[tauri::command(rename_all = "camelCase")]
pub async fn get_gemini_model(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .settings
        .get_gemini_model()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Set the Gladia API key
#[tauri::command(rename_all = "camelCase")]
pub async fn set_gladia_api_key(api_key: String, state: State<'_, AppState>) -> Result<(), String> {
    // Store on the per-provider key map (survives provider switches)
    state
        .transcription_manager
        .set_api_key_for_provider(ProviderType::Gladia, api_key.clone());

    state
        .settings
        .set_gladia_api_key(&api_key)
        .await
        .map_err(|e| format!("Failed to save API key: {}", e))?;

    log::info!("Gladia API key saved and applied");
    Ok(())
}

/// Set the Google STT key
#[tauri::command(rename_all = "camelCase")]
pub async fn set_google_stt_key(
    key_json: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Store on the per-provider key map (survives provider switches)
    state
        .transcription_manager
        .set_api_key_for_provider(ProviderType::GoogleSTT, key_json.clone());

    state
        .settings
        .set_google_stt_key(&key_json)
        .await
        .map_err(|e| format!("Failed to save API key: {}", e))?;

    log::info!("Google STT key saved and applied");
    Ok(())
}

/// Set active transcription provider
#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_provider(
    provider: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use crate::transcription::ProviderType;

    let p_type = match provider.as_str() {
        "deepgram" => ProviderType::Deepgram,
        "gemini" => ProviderType::Gemini,
        "gladia" => ProviderType::Gladia,
        "google_stt" => ProviderType::GoogleSTT,
        _ => return Err("Invalid provider".to_string()),
    };

    state.transcription_manager.switch_provider(p_type);

    state
        .settings
        .set_transcription_provider(&provider)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    log::info!("Transcription provider set to: {}", provider);
    Ok(())
}

/// Get the Deepgram API key (masked for display)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_deepgram_api_key(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let key = state
        .settings
        .get_deepgram_api_key()
        .await
        .map_err(|e| format!("Failed to get API key: {}", e))?;

    Ok(key.map(|k| {
        if k.len() > 8 {
            format!("{}...{}", &k[..4], &k[k.len() - 4..])
        } else {
            "****".to_string()
        }
    }))
}

/// Get all settings
#[tauri::command(rename_all = "camelCase")]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Get a single setting value
#[tauri::command(rename_all = "camelCase")]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state
        .settings
        .get(&key)
        .await
        .map_err(|e| format!("Failed to get setting: {}", e))
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

    log::info!("Meeting deleted: {}", meeting_id);
    Ok(())
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
// Intelligence / Meeting State Commands
// ============================================
use crate::meeting_intel::{CalendarEvent, MeetingState, MeetingStateResolver};

// ============================================
// AI Commands (Ollama Integration)
// ============================================

use crate::ai_client::{AIClient, AIPreset, ChatMessage, OllamaModel};

/// Check if Ollama is available
#[tauri::command(rename_all = "camelCase")]
pub async fn check_ollama() -> Result<bool, String> {
    let client = AIClient::new();
    Ok(client.is_available().await)
}

/// Get available Ollama models
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ollama_models() -> Result<Vec<OllamaModel>, String> {
    let client = AIClient::new();
    client.list_models().await
}

/// Get AI presets
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ai_presets() -> Result<Vec<AIPreset>, String> {
    Ok(AIPreset::get_all_presets())
}

/// Chat with AI using a preset
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chat(
    preset_id: String,
    message: String,
    meeting_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();

    // Get the preset
    let presets = AIPreset::get_all_presets();
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .cloned()
        .unwrap_or_else(AIPreset::qa);

    // Build context from meeting if provided
    let context = if let Some(ref id) = meeting_id {
        let transcripts = state
            .database
            .get_transcripts(id)
            .await
            .map_err(|e| format!("Failed to get transcripts: {}", e))?;

        let transcript_text: String = transcripts
            .iter()
            .filter(|t| t.is_final)
            .map(|t| {
                format!(
                    "[{}] {}: {}",
                    t.timestamp.format("%H:%M:%S"),
                    t.speaker.as_deref().unwrap_or("Speaker"),
                    t.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Some(transcript_text)
    } else {
        None
    };

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: message,
    }];

    client.chat(&preset, messages, context.as_deref()).await
}

/// Summarize a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn summarize_meeting(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();

    // Get transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    if transcripts.is_empty() {
        return Err("No transcripts found for this meeting".to_string());
    }

    let content: String = transcripts
        .iter()
        .filter(|t| t.is_final)
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");

    client.summarize(&content).await
}

/// Extract action items from a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn extract_action_items(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();

    // Get transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    if transcripts.is_empty() {
        return Err("No transcripts found for this meeting".to_string());
    }

    let content: String = transcripts
        .iter()
        .filter(|t| t.is_final)
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");

    client.extract_action_items(&content).await
}

// ============================================
// Knowledge Base Commands (VLM, Supabase, Pinecone)
// ============================================

use crate::pinecone_client::{ActivityMetadata, VectorMatch};
use crate::supabase_client::Activity;
use crate::vlm_client::ActivityContext;

/// Check if VLM API is available
#[tauri::command(rename_all = "camelCase")]
pub async fn check_vlm(_state: State<'_, AppState>) -> Result<bool, String> {
    Ok(crate::vlm_client::vlm_is_available().await)
}

/// Check if VLM has vision model
#[tauri::command(rename_all = "camelCase")]
pub async fn check_vlm_vision(_state: State<'_, AppState>) -> Result<bool, String> {
    crate::vlm_client::vlm_has_vision_model().await
}

/// Analyze a frame with VLM
#[tauri::command(rename_all = "camelCase")]
pub async fn analyze_frame(
    frame_path: String,
    _state: State<'_, AppState>,
) -> Result<ActivityContext, String> {
    // Use default prompt for manual analysis
    let prompt = r#"Analyze this screenshot and describe what the user is doing. 
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
Only respond with valid JSON."#;

    crate::vlm_client::vlm_analyze_frame(&frame_path, prompt).await
}

/// Analyze multiple frames (batch)
#[tauri::command(rename_all = "camelCase")]
pub async fn analyze_frames_batch(
    frame_paths: Vec<String>,
    _state: State<'_, AppState>,
) -> Result<Vec<ActivityContext>, String> {
    let prompt = r#"Analyze this screenshot. Respond in JSON with: app_name, category, summary, confidence."#;
    let frames: Vec<(String, String)> = frame_paths
        .into_iter()
        .map(|p| (p, prompt.to_string()))
        .collect();

    let results = crate::vlm_client::vlm_analyze_frames_batch(frames).await;

    // Collect successful results
    Ok(results.into_iter().filter_map(|r| r.ok()).collect())
}

// =============================================================================
// TheBrain Cloud API Commands
// =============================================================================

/// Authenticate with TheBrain API
#[tauri::command(rename_all = "camelCase")]
pub async fn thebrain_authenticate(
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    // Authenticate
    let token = crate::vlm_client::vlm_authenticate(&username, &password).await?;

    // Store credentials in settings (for persistence)
    let settings = state.settings.clone();
    let _ = settings.set("thebrain_username", &username).await;
    let _ = settings.set("thebrain_token", &token).await;
    // Note: We don't store password in settings for security

    log::info!("✅ TheBrain authentication successful");
    Ok(true)
}

/// Check if TheBrain API is connected
#[tauri::command(rename_all = "camelCase")]
pub async fn check_thebrain(_state: State<'_, AppState>) -> Result<bool, String> {
    Ok(crate::vlm_client::vlm_is_authenticated() && crate::vlm_client::vlm_is_available().await)
}

/// Set VLM API URL and reconfigure the client
#[tauri::command(rename_all = "camelCase")]
pub async fn set_vlm_api_url(url: String, state: State<'_, AppState>) -> Result<(), String> {
    // Save to settings
    state
        .settings
        .set("vlm_base_url", &url)
        .await
        .map_err(|e| format!("Failed to save VLM URL: {}", e))?;

    // Reconfigure the VLM client with new URL
    crate::vlm_client::vlm_configure(&url, None);

    log::info!("✅ VLM API URL updated to: {}", url);
    Ok(())
}

/// Get available models from TheBrain API
#[tauri::command(rename_all = "camelCase")]
pub async fn get_thebrain_models(
    _state: State<'_, AppState>,
) -> Result<Vec<crate::vlm_client::ModelStatus>, String> {
    crate::vlm_client::vlm_get_models().await
}

/// Capture text from the current focused window using accessibility APIs
/// and store it as a text snapshot in the database
#[tauri::command(rename_all = "camelCase")]
pub async fn capture_accessibility_snapshot(
    state: State<'_, AppState>,
) -> Result<CapturedSnapshotResult, String> {
    use crate::snapshot_extractor::{ExtractionResult, ExtractionSource, SnapshotExtractor};
    use uuid::Uuid;

    log::info!("📸 Capturing accessibility snapshot from focused window...");

    let extractor = SnapshotExtractor::new();

    // First try accessibility, fall back to OCR if needed
    let result = extractor.extract_from_accessibility(None, None, None, None);

    match result {
        ExtractionResult::Success(snapshot) => {
            let snapshot_id = Uuid::new_v4().to_string();
            let text_preview = if snapshot.text.len() > 200 {
                format!("{}...", &snapshot.text[..200])
            } else {
                snapshot.text.clone()
            };

            // Store to database
            if let Err(e) = state
                .database
                .add_text_snapshot(
                    &snapshot_id,
                    None, // episode_id
                    None, // state_id
                    chrono::Utc::now(),
                    &snapshot.text,
                    &snapshot.text_hash,
                    snapshot.quality_score,
                    ExtractionSource::Accessibility.as_str(),
                )
                .await
            {
                log::warn!("Failed to save snapshot to database: {}", e);
            }

            log::info!(
                "✅ Captured {} words from accessibility API",
                snapshot.word_count
            );

            Ok(CapturedSnapshotResult {
                success: true,
                text_preview,
                word_count: snapshot.word_count,
                source: "accessibility".to_string(),
                snapshot_id,
            })
        }
        ExtractionResult::Failed(reason) => {
            log::warn!("Accessibility capture failed: {}", reason);
            Err(format!("Capture failed: {}", reason))
        }
        ExtractionResult::TooShort => Err("Captured text too short to be useful".to_string()),
        ExtractionResult::LowQuality(score) => Err(format!(
            "Captured text quality too low: {:.1}%",
            score * 100.0
        )),
        ExtractionResult::Disabled => Err("Text extraction is disabled".to_string()),
    }
}

/// Result of capturing a snapshot
#[derive(serde::Serialize)]
pub struct CapturedSnapshotResult {
    pub success: bool,
    pub text_preview: String,
    pub word_count: i32,
    pub source: String,
    pub snapshot_id: String,
}

/// Chat with TheBrain API using specified model
#[tauri::command(rename_all = "camelCase")]
pub async fn thebrain_chat(
    message: String,
    model: String,
    _state: State<'_, AppState>,
) -> Result<String, String> {
    if !crate::vlm_client::vlm_is_authenticated() {
        return Err("Not authenticated with TheBrain. Please login in Settings.".to_string());
    }

    log::info!(
        "🧠 TheBrain chat: model={}, message_len={}",
        model,
        message.len()
    );

    // Use streaming endpoint for better response
    crate::vlm_client::vlm_chat_stream(&message, &model).await
}

/// RAG Chat Response with context and citations
#[derive(serde::Serialize)]
pub struct RagChatResponse {
    pub response: String,
    pub context_used: Vec<ContextItem>,
    pub model: String,
}

#[derive(serde::Serialize)]
pub struct ContextItem {
    pub id: String,
    pub score: f32,
    pub summary: String,
    pub timestamp: Option<String>,
    pub category: Option<String>,
}

/// Chat with TheBrain using RAG - retrieves relevant context before answering
#[tauri::command(rename_all = "camelCase")]
pub async fn thebrain_rag_chat(
    message: String,
    model: String,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<RagChatResponse, String> {
    if !crate::vlm_client::vlm_is_authenticated() {
        return Err("Not authenticated with TheBrain. Please login in Settings.".to_string());
    }

    let search_count = top_k.unwrap_or(5);
    log::info!(
        "🧠 RAG Chat: searching {} items, model={}",
        search_count,
        model
    );

    // Get config before async operations (avoid holding RwLock guard across await)
    let pinecone_config = state.pinecone_client.read().get_config();

    // Step 1: Search Pinecone for relevant context
    let context_items = match pinecone_config {
        Some(config) => {
            match crate::pinecone_client::pinecone_search(&config, &message, search_count).await {
                Ok(matches) => matches
                    .into_iter()
                    .filter(|m| m.score > 0.5) // Only include good matches
                    .map(|m| {
                        let metadata = m.metadata.as_ref();
                        ContextItem {
                            id: m.id,
                            score: m.score,
                            summary: metadata
                                .and_then(|md| md.get("summary").or_else(|| md.get("text")))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            timestamp: metadata
                                .and_then(|md| md.get("timestamp"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            category: metadata
                                .and_then(|md| md.get("category"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        }
                    })
                    .collect::<Vec<_>>(),
                Err(e) => {
                    log::warn!("Pinecone search failed, proceeding without context: {}", e);
                    vec![]
                }
            }
        }
        None => {
            log::info!("Pinecone not configured, proceeding without context");
            vec![]
        }
    };

    // Step 2: Build augmented prompt with context
    let augmented_prompt = if !context_items.is_empty() {
        let context_text = context_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                format!(
                    "[{}] {} (relevance: {:.0}%)\n   {}",
                    i + 1,
                    item.timestamp.as_deref().unwrap_or("Unknown time"),
                    item.score * 100.0,
                    item.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"You are an intelligent assistant with access to the user's activity history and meeting data.

RELEVANT CONTEXT FROM USER'S HISTORY:
{}

USER QUESTION: {}

Instructions:
- Use the context above to inform your answer when relevant
- If the context doesn't contain relevant information, say so and answer based on general knowledge
- Reference specific items from the context when applicable (e.g., "Based on your meeting on [date]...")
- Be concise and actionable"#,
            context_text, message
        )
    } else {
        format!(
            r#"You are an intelligent assistant helping with daily operations.

USER QUESTION: {}

Note: No relevant context was found in the user's history for this query. Answer based on general knowledge."#,
            message
        )
    };

    // Step 3: Call TheBrain with augmented prompt
    let response = crate::vlm_client::vlm_chat_stream(&augmented_prompt, &model).await?;

    log::info!(
        "🧠 RAG Chat complete: {} context items used",
        context_items.len()
    );

    Ok(RagChatResponse {
        response,
        context_used: context_items,
        model,
    })
}

// ============================================================================
// Conversation Storage Commands (Phase 2)
// ============================================================================

/// Conversation record for storage
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ConversationRecord {
    pub id: String,
    pub timestamp: String,
    pub user_query: String,
    pub assistant_response: String,
    pub model_used: String,
    pub context_refs: Vec<String>, // IDs of context items used
}

/// Store a conversation to both Supabase and Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn store_conversation(
    user_query: String,
    assistant_response: String,
    model_used: String,
    context_refs: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();

    log::info!("💾 Storing conversation: {}", id);

    // Get configs before async operations (avoid RwLock guard across await)
    let pinecone_config = state.pinecone_client.read().get_config();
    let supabase_pool = state.supabase_client.read().get_pool();

    // Store to Pinecone for semantic search of past conversations
    if let Some(config) = pinecone_config {
        // Create searchable text combining Q&A
        let searchable_text = format!("Q: {}\nA: {}", user_query, assistant_response);

        let metadata = crate::pinecone_client::ActivityMetadata {
            timestamp: timestamp.clone(),
            category: "conversation".to_string(),
            app_name: Some("thebrain-chat".to_string()),
            focus_area: None,
            summary: format!(
                "User asked about: {}...",
                &user_query.chars().take(100).collect::<String>()
            ),
        };

        if let Err(e) =
            crate::pinecone_client::pinecone_upsert(&config, &id, &searchable_text, &metadata).await
        {
            log::warn!("Failed to store conversation to Pinecone: {}", e);
        } else {
            log::info!("📌 Conversation stored to Pinecone: {}", id);
        }
    }

    // Store to Supabase if connected
    if let Some(pool) = supabase_pool {
        let query = r#"
            INSERT INTO conversations (id, timestamp, user_query, assistant_response, model_used, context_refs)
            VALUES ($1::uuid, $2::timestamptz, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
        "#;

        match sqlx::query(query)
            .bind(&id)
            .bind(&timestamp)
            .bind(&user_query)
            .bind(&assistant_response)
            .bind(&model_used)
            .bind(serde_json::to_value(&context_refs).unwrap_or_default())
            .execute(&pool)
            .await
        {
            Ok(_) => log::info!("📦 Conversation stored to Supabase: {}", id),
            Err(e) => log::warn!("Failed to store conversation to Supabase: {}", e),
        }
    }

    Ok(id)
}

/// Get recent conversation history
#[tauri::command(rename_all = "camelCase")]
pub async fn get_conversation_history(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<ConversationRecord>, String> {
    let max_count = limit.unwrap_or(20);

    // Get pool before async operations (avoid RwLock guard across await)
    let supabase_pool = state.supabase_client.read().get_pool();

    // Try to get from Supabase first
    if let Some(pool) = supabase_pool {
        let query = r#"
            SELECT id, timestamp, user_query, assistant_response, model_used, context_refs
            FROM conversations
            ORDER BY timestamp DESC
            LIMIT $1
        "#;

        match sqlx::query_as::<_, (String, String, String, String, String, serde_json::Value)>(
            query,
        )
        .bind(max_count)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => {
                let records = rows
                    .into_iter()
                    .map(
                        |(
                            id,
                            timestamp,
                            user_query,
                            assistant_response,
                            model_used,
                            context_refs,
                        )| {
                            ConversationRecord {
                                id,
                                timestamp,
                                user_query,
                                assistant_response,
                                model_used,
                                context_refs: context_refs
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            }
                        },
                    )
                    .collect();
                return Ok(records);
            }
            Err(e) => {
                log::warn!("Failed to fetch conversations from Supabase: {}", e);
            }
        }
    }

    // If Supabase not available, return empty
    Ok(vec![])
}

/// Combined RAG chat with automatic conversation storage
#[tauri::command(rename_all = "camelCase")]
pub async fn thebrain_rag_chat_with_memory(
    message: String,
    model: String,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<RagChatResponse, String> {
    // First do the RAG chat
    let response = thebrain_rag_chat(message.clone(), model.clone(), top_k, state.clone()).await?;

    // Then store the conversation for future retrieval
    let context_refs: Vec<String> = response.context_used.iter().map(|c| c.id.clone()).collect();

    if let Err(e) = store_conversation(
        message,
        response.response.clone(),
        model,
        context_refs,
        state,
    )
    .await
    {
        log::warn!("Failed to store conversation: {}", e);
    }

    Ok(response)
}

/// Configure Supabase connection
#[tauri::command(rename_all = "camelCase")]
pub async fn configure_supabase(
    connection_string: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Set connection string (sync), drop guard
    state
        .supabase_client
        .read()
        .set_connection_string(connection_string.clone());
    // Connect using standalone function (no guard held across await)
    let pool = crate::supabase_client::supabase_connect_pool(&connection_string).await?;
    // Store pool in client
    state.supabase_client.read().set_pool(pool);
    Ok(())
}

/// Check Supabase connection
#[tauri::command(rename_all = "camelCase")]
pub async fn check_supabase(state: State<'_, AppState>) -> Result<bool, String> {
    let client = state.supabase_client.read();
    Ok(client.is_connected())
}

/// Sync an activity to Supabase
#[tauri::command(rename_all = "camelCase")]
pub async fn sync_activity_to_supabase(
    activity: Activity,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let pool = state
        .supabase_client
        .read()
        .get_pool()
        .ok_or("Supabase not connected")?;
    crate::supabase_client::supabase_insert_activity(&pool, &activity).await
}

/// Query activities by time range
#[tauri::command(rename_all = "camelCase")]
pub async fn query_activities(
    start_iso: String,
    end_iso: String,
    state: State<'_, AppState>,
) -> Result<Vec<Activity>, String> {
    use chrono::{DateTime, Utc};

    let start: DateTime<Utc> = start_iso
        .parse()
        .map_err(|e| format!("Invalid start time: {}", e))?;
    let end: DateTime<Utc> = end_iso
        .parse()
        .map_err(|e| format!("Invalid end time: {}", e))?;

    let pool = state
        .supabase_client
        .read()
        .get_pool()
        .ok_or("Supabase not connected")?;
    crate::supabase_client::supabase_query_activities(&pool, start, end).await
}

/// Configure Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn configure_pinecone(
    api_key: String,
    index_host: String,
    namespace: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let client = state.pinecone_client.read();
    client.configure(api_key, index_host, namespace);
    Ok(())
}

/// Check if Pinecone is configured
#[tauri::command(rename_all = "camelCase")]
pub async fn check_pinecone(state: State<'_, AppState>) -> Result<bool, String> {
    let client = state.pinecone_client.read();
    Ok(client.is_configured())
}

/// Upsert activity to Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn upsert_to_pinecone(
    id: String,
    text: String,
    metadata: ActivityMetadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = state
        .pinecone_client
        .read()
        .get_config()
        .ok_or("Pinecone not configured")?;
    crate::pinecone_client::pinecone_upsert(&config, &id, &text, &metadata).await
}

/// Semantic search in Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn semantic_search(
    query: String,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<VectorMatch>, String> {
    let k = top_k.unwrap_or(10);
    let config = state
        .pinecone_client
        .read()
        .get_config()
        .ok_or("Pinecone not configured")?;

    crate::pinecone_client::pinecone_search(&config, &query, k).await
}

/// Get Pinecone index stats
#[tauri::command(rename_all = "camelCase")]
pub async fn get_pinecone_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state
        .pinecone_client
        .read()
        .get_config()
        .ok_or("Pinecone not configured")?;

    crate::pinecone_client::pinecone_stats(&config).await
}

/// Index result for transcript embedding
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptIndexResult {
    pub meeting_id: String,
    pub transcripts_indexed: usize,
    pub errors: Vec<String>,
}

/// Index all transcripts from a meeting to Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn index_meeting_transcripts(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<TranscriptIndexResult, String> {
    // Get Pinecone config
    let config = state.pinecone_client.read().get_config().ok_or(
        "Pinecone not configured. Please configure Pinecone in Settings → Knowledge Base.",
    )?;

    // Get all transcripts for this meeting
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    if transcripts.is_empty() {
        return Ok(TranscriptIndexResult {
            meeting_id,
            transcripts_indexed: 0,
            errors: vec!["No transcripts found for this meeting".to_string()],
        });
    }

    // Get meeting info for metadata
    let meeting_title = match state.database.get_meeting(&meeting_id).await {
        Ok(Some(m)) => m.title,
        _ => "Unknown Meeting".to_string(),
    };

    let mut indexed = 0;
    let mut errors = Vec::new();

    // Batch transcripts for efficiency (group by 5 for embedding)
    for (i, transcript) in transcripts.iter().enumerate() {
        // Only index final transcripts
        if !transcript.is_final {
            continue;
        }

        let id = format!("transcript_{}_{}", meeting_id, transcript.id);
        let text = &transcript.text;

        // Build metadata for the vector
        let metadata = serde_json::json!({
            "type": "transcript",
            "meeting_id": meeting_id,
            "meeting_title": meeting_title,
            "transcript_id": transcript.id,
            "speaker": transcript.speaker.as_deref().unwrap_or("Unknown"),
            "timestamp": transcript.timestamp.to_rfc3339(),
            "text": text,
            "index": i,
        });

        match crate::pinecone_client::pinecone_upsert_generic(&config, &id, text, &metadata).await {
            Ok(_) => {
                indexed += 1;
                if indexed % 10 == 0 {
                    log::info!("📌 Indexed {} transcripts to Pinecone", indexed);
                }
            }
            Err(e) => {
                errors.push(format!(
                    "Failed to index transcript {}: {}",
                    transcript.id, e
                ));
            }
        }
    }

    log::info!(
        "✅ Indexed {} transcripts from meeting '{}' to Pinecone",
        indexed,
        meeting_title
    );

    Ok(TranscriptIndexResult {
        meeting_id,
        transcripts_indexed: indexed,
        errors,
    })
}

/// Index all meetings' transcripts to Pinecone
#[tauri::command(rename_all = "camelCase")]
pub async fn index_all_transcripts_to_pinecone(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<TranscriptIndexResult>, String> {
    // Get Pinecone config
    let config = state.pinecone_client.read().get_config().ok_or(
        "Pinecone not configured. Please configure Pinecone in Settings → Knowledge Base.",
    )?;

    // Get all meetings
    let meetings = state
        .database
        .list_meetings(limit.unwrap_or(50))
        .await
        .map_err(|e| format!("Failed to list meetings: {}", e))?;

    let mut results = Vec::new();

    for meeting in meetings {
        let meeting_id = meeting.id.clone();

        // Get transcripts
        let transcripts = match state.database.get_transcripts(&meeting_id).await {
            Ok(t) => t,
            Err(e) => {
                results.push(TranscriptIndexResult {
                    meeting_id: meeting_id.clone(),
                    transcripts_indexed: 0,
                    errors: vec![format!("Failed to get transcripts: {}", e)],
                });
                continue;
            }
        };

        if transcripts.is_empty() {
            continue;
        }

        let meeting_title = meeting.title.clone();
        let mut indexed = 0;
        let mut errors = Vec::new();

        for (i, transcript) in transcripts.iter().enumerate() {
            if !transcript.is_final {
                continue;
            }

            let id = format!("transcript_{}_{}", meeting_id, transcript.id);
            let metadata = serde_json::json!({
                "type": "transcript",
                "meeting_id": meeting_id,
                "meeting_title": meeting_title,
                "transcript_id": transcript.id,
                "speaker": transcript.speaker.as_deref().unwrap_or("Unknown"),
                "timestamp": transcript.timestamp.to_rfc3339(),
                "text": transcript.text,
                "index": i,
            });

            match crate::pinecone_client::pinecone_upsert_generic(
                &config,
                &id,
                &transcript.text,
                &metadata,
            )
            .await
            {
                Ok(_) => indexed += 1,
                Err(e) => errors.push(format!("transcript {} failed: {}", transcript.id, e)),
            }
        }

        if indexed > 0 {
            log::info!(
                "📌 Indexed {} transcripts from meeting '{}'",
                indexed,
                meeting_title
            );
        }

        results.push(TranscriptIndexResult {
            meeting_id,
            transcripts_indexed: indexed,
            errors,
        });
    }

    let total: usize = results.iter().map(|r| r.transcripts_indexed).sum();
    log::info!(
        "✅ Total: Indexed {} transcripts across {} meetings",
        total,
        results.len()
    );

    Ok(results)
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

    // Sort by timestamp
    entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(MeetingTimeline {
        meeting_id: meeting.id,
        title: meeting.title,
        started_at: meeting.started_at.to_rfc3339(),
        ended_at: meeting.ended_at.map(|e| e.to_rfc3339()),
        transcript_count: transcripts.iter().filter(|t| t.is_final).count(),
        accessibility_count: acc_snapshots.len(),
        screenshot_count: frames.iter().filter(|f| f.file_path.is_some()).count(),
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

// ============================================
// Knowledge Base Configuration Commands
// ============================================

/// Configure all knowledge base settings at once
#[tauri::command(rename_all = "camelCase")]
pub async fn configure_knowledge_base(
    supabase_connection: Option<String>,
    pinecone_api_key: Option<String>,
    pinecone_index_host: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Save settings
    if let Some(ref conn) = supabase_connection {
        state
            .settings
            .set_supabase_connection(conn)
            .await
            .map_err(|e| format!("Failed to save Supabase setting: {}", e))?;
        // Connect to Supabase
        state
            .supabase_client
            .read()
            .set_connection_string(conn.clone());
    }

    if let (Some(ref key), Some(ref host)) = (&pinecone_api_key, &pinecone_index_host) {
        state
            .settings
            .set_pinecone_api_key(key)
            .await
            .map_err(|e| format!("Failed to save Pinecone API key: {}", e))?;
        state
            .settings
            .set_pinecone_index_host(host)
            .await
            .map_err(|e| format!("Failed to save Pinecone host: {}", e))?;
        // Configure Pinecone client
        state
            .pinecone_client
            .read()
            .configure(key.clone(), host.clone(), None);
    }

    Ok(())
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
        "supabase_configured": settings.supabase_connection_string.is_some(),
        "pinecone_configured": settings.pinecone_api_key.is_some() && settings.pinecone_index_host.is_some(),
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
        return Err("VLM API is not available. Please check SSH tunnel and token.".to_string());
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
                    pinecone_id: None,
                    supabase_id: None,
                    synced_at: None,
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

/// Get unsynced activities
#[tauri::command(rename_all = "camelCase")]
pub async fn get_unsynced_activities(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<ActivityLogEntry>, String> {
    state
        .database
        .get_unsynced_activities(limit.unwrap_or(50))
        .await
        .map_err(|e| format!("Failed to get activities: {}", e))
}

/// Sync result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncResult {
    pub activities_synced: usize,
    pub pinecone_upserts: usize,
    pub supabase_inserts: usize,
    pub errors: Vec<String>,
}

/// Sync activities to cloud (Pinecone + Supabase)
#[tauri::command(rename_all = "camelCase")]
pub async fn sync_to_cloud(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<SyncResult, String> {
    let limit = limit.unwrap_or(20);

    // Get unsynced activities
    let activities = state
        .database
        .get_unsynced_activities(limit)
        .await
        .map_err(|e| format!("Failed to get unsynced activities: {}", e))?;

    if activities.is_empty() {
        return Ok(SyncResult {
            activities_synced: 0,
            pinecone_upserts: 0,
            supabase_inserts: 0,
            errors: vec![],
        });
    }

    // Check configuration status upfront and get configs (drop guards immediately)
    let pinecone_config = state.pinecone_client.read().get_config();
    let _pinecone_configured = pinecone_config.is_some();
    let supabase_pool = state.supabase_client.read().get_pool();
    let _supabase_connected = supabase_pool.is_some();

    let mut activities_synced = 0;
    let mut pinecone_upserts = 0;
    let mut supabase_inserts = 0;
    let mut errors = Vec::new();

    for activity in activities {
        let activity_id = match &activity.id {
            Some(id) => *id,
            None => continue,
        };

        let mut pinecone_id: Option<String> = None;
        let mut supabase_id: Option<String> = None;

        // Sync to Pinecone (if configured)
        if let Some(ref config) = pinecone_config {
            let id = format!("activity_{}", activity_id);
            let text = format!(
                "{} - {} - {}",
                activity.category,
                activity.summary,
                activity.focus_area.as_deref().unwrap_or("")
            );

            let metadata = ActivityMetadata {
                timestamp: activity.start_time.to_rfc3339(),
                category: activity.category.clone(),
                app_name: activity.app_name.clone(),
                focus_area: activity.focus_area.clone(),
                summary: activity.summary.clone(),
            };

            // Use standalone function (no guard held across await)
            match crate::pinecone_client::pinecone_upsert(config, &id, &text, &metadata).await {
                Ok(_) => {
                    pinecone_id = Some(id);
                    pinecone_upserts += 1;
                }
                Err(e) => {
                    errors.push(format!("Pinecone sync failed: {}", e));
                }
            }
        }

        // Sync to Supabase (if connected)
        if let Some(ref pool) = supabase_pool {
            let supabase_activity = Activity {
                id: None,
                start_time: activity.start_time,
                end_time: activity.end_time,
                duration_seconds: activity.duration_seconds,
                app_name: activity.app_name.clone(),
                window_title: activity.window_title.clone(),
                category: activity.category.clone(),
                summary: activity.summary.clone(),
                focus_area: activity.focus_area.clone(),
                pinecone_id: pinecone_id.clone(),
                created_at: None,
            };

            // Use standalone function (no guard held across await)
            match crate::supabase_client::supabase_insert_activity(pool, &supabase_activity).await {
                Ok(id) => {
                    supabase_id = Some(id);
                    supabase_inserts += 1;
                }
                Err(e) => {
                    errors.push(format!("Supabase sync failed: {}", e));
                }
            }
        }

        // Mark as synced if at least one succeeded
        if pinecone_id.is_some() || supabase_id.is_some() {
            let _ = state
                .database
                .mark_activity_synced(activity_id, pinecone_id.as_deref(), supabase_id.as_deref())
                .await;
            activities_synced += 1;
        }
    }

    log::info!(
        "☁️ Cloud Sync: {} activities synced ({} Pinecone, {} Supabase)",
        activities_synced,
        pinecone_upserts,
        supabase_inserts
    );

    Ok(SyncResult {
        activities_synced,
        pinecone_upserts,
        supabase_inserts,
        errors,
    })
}

// ============================================
// Search Commands (Phase 6)
// ============================================

/// Unified search result combining local, Supabase, and Pinecone results
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KBSearchResult {
    pub id: String,
    pub source: String, // "local", "supabase", "pinecone"
    pub timestamp: Option<String>,
    pub app_name: Option<String>,
    pub category: Option<String>,
    pub summary: String,
    pub score: Option<f32>,
}

/// Search options
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchOptions {
    pub query: Option<String>,        // Semantic query for Pinecone
    pub start_date: Option<String>,   // ISO date for time range
    pub end_date: Option<String>,     // ISO date for time range
    pub category: Option<String>,     // Filter by category
    pub limit: Option<u32>,           // Max results
    pub sources: Option<Vec<String>>, // ["local", "pinecone", "supabase"]
}

/// Combined search across local SQLite, Pinecone, and Supabase
#[tauri::command(rename_all = "camelCase")]
pub async fn search_knowledge_base(
    options: SearchOptions,
    state: State<'_, AppState>,
) -> Result<Vec<KBSearchResult>, String> {
    let mut results = Vec::new();
    let limit = options.limit.unwrap_or(20) as i32;
    let sources = options
        .sources
        .clone()
        .unwrap_or_else(|| vec!["local".to_string()]);

    // Search local SQLite activity_log
    if sources.contains(&"local".to_string()) {
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
    }

    // Semantic search via Pinecone
    if sources.contains(&"pinecone".to_string()) {
        if let Some(ref query) = options.query {
            let config = state.pinecone_client.read().get_config();
            if let Some(config) = config {
                if let Ok(matches) =
                    crate::pinecone_client::pinecone_search(&config, query, limit as u32).await
                {
                    for m in matches {
                        results.push(KBSearchResult {
                            id: m.id,
                            source: "pinecone".to_string(),
                            timestamp: m.metadata.as_ref().and_then(|m| {
                                m.get("timestamp")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            }),
                            app_name: m.metadata.as_ref().and_then(|m| {
                                m.get("app_name")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            }),
                            category: m.metadata.as_ref().and_then(|m| {
                                m.get("category")
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                            }),
                            summary: m
                                .metadata
                                .as_ref()
                                .and_then(|m| {
                                    m.get("summary")
                                        .or_else(|| m.get("text"))
                                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                                })
                                .unwrap_or_default(),
                            score: Some(m.score),
                        });
                    }
                }
            }
        }
    }

    // Time-based query via Supabase
    if sources.contains(&"supabase".to_string()) {
        if let (Some(ref start), Some(ref end)) = (&options.start_date, &options.end_date) {
            use chrono::{DateTime, Utc};
            if let (Ok(start), Ok(end)) =
                (start.parse::<DateTime<Utc>>(), end.parse::<DateTime<Utc>>())
            {
                let pool = state.supabase_client.read().get_pool();
                if let Some(pool) = pool {
                    if let Ok(activities) =
                        crate::supabase_client::supabase_query_activities(&pool, start, end).await
                    {
                        for activity in activities {
                            // Filter by category if provided
                            if let Some(ref cat) = options.category {
                                if activity.category != *cat {
                                    continue;
                                }
                            }

                            results.push(KBSearchResult {
                                id: activity.id.unwrap_or_default(),
                                source: "supabase".to_string(),
                                timestamp: Some(activity.start_time.to_rfc3339()),
                                app_name: activity.app_name,
                                category: Some(activity.category),
                                summary: activity.summary,
                                score: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort by score (Pinecone results first), then by timestamp
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

/// Quick semantic search (just Pinecone)
#[tauri::command(rename_all = "camelCase")]
pub async fn quick_semantic_search(
    query: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<KBSearchResult>, String> {
    let options = SearchOptions {
        query: Some(query),
        start_date: None,
        end_date: None,
        category: None,
        limit,
        sources: Some(vec!["pinecone".to_string()]),
    };
    search_knowledge_base(options, state).await
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

        meetings_array.push(serde_json::json!({
            "id": meeting.id,
            "title": meeting.title,
            "started_at": meeting.started_at,
            "ended_at": meeting.ended_at,
            "duration_seconds": meeting.duration_seconds,
            "transcripts": transcripts,
            "frame_count": frames.len(),
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
// Prompt Management Commands
// ============================================

/// List all prompts, optionally filtered by category
#[tauri::command(rename_all = "camelCase")]
pub async fn list_prompts(
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .list_prompts(category.as_deref())
        .await
        .map_err(|e| format!("Failed to list prompts: {}", e))
}

/// Get a single prompt by ID
#[tauri::command(rename_all = "camelCase")]
pub async fn get_prompt(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .get_prompt(&id)
        .await
        .map_err(|e| format!("Failed to get prompt: {}", e))
}

/// Create a new prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_prompt(
    input: crate::prompt_manager::PromptCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::Prompt, String> {
    state
        .prompt_manager
        .create_prompt(input)
        .await
        .map_err(|e| format!("Failed to create prompt: {}", e))
}

/// Update an existing prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn update_prompt(
    id: String,
    updates: crate::prompt_manager::PromptUpdate,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .update_prompt(&id, updates)
        .await
        .map_err(|e| format!("Failed to update prompt: {}", e))
}

/// Delete a prompt (only non-builtin prompts can be deleted)
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_prompt(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .prompt_manager
        .delete_prompt(&id)
        .await
        .map_err(|e| format!("Failed to delete prompt: {}", e))
}

/// Duplicate a prompt with a new name
#[tauri::command(rename_all = "camelCase")]
pub async fn duplicate_prompt(
    id: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .duplicate_prompt(&id, &new_name)
        .await
        .map_err(|e| format!("Failed to duplicate prompt: {}", e))
}

/// Export all custom prompts as JSON
#[tauri::command(rename_all = "camelCase")]
pub async fn export_prompts(state: State<'_, AppState>) -> Result<String, String> {
    state
        .prompt_manager
        .export_prompts()
        .await
        .map_err(|e| format!("Failed to export prompts: {}", e))
}

/// Import prompts from JSON
#[tauri::command(rename_all = "camelCase")]
pub async fn import_prompts(
    json: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .import_prompts(&json)
        .await
        .map_err(|e| format!("Failed to import prompts: {}", e))
}

// ============================================
// Model Configuration Commands
// ============================================

/// List all model configurations
#[tauri::command(rename_all = "camelCase")]
pub async fn list_model_configs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to list model configs: {}", e))
}

/// Get a model config by ID
#[tauri::command(rename_all = "camelCase")]
pub async fn get_model_config(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ModelConfig>, String> {
    state
        .prompt_manager
        .get_model_config(&id)
        .await
        .map_err(|e| format!("Failed to get model config: {}", e))
}

/// Create a new model configuration
#[tauri::command(rename_all = "camelCase")]
pub async fn create_model_config(
    input: crate::prompt_manager::ModelConfigCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::ModelConfig, String> {
    state
        .prompt_manager
        .create_model_config(input)
        .await
        .map_err(|e| format!("Failed to create model config: {}", e))
}

/// Refresh model availability by checking Ollama
#[tauri::command(rename_all = "camelCase")]
pub async fn refresh_model_availability(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    // Get all models from centralized API - use scoped block to release guard before async
    let (base_url, auth) = {
        let vlm = state.vlm_client.read();
        (vlm.get_base_url(), vlm.get_auth_header())
    };

    let client = reqwest::Client::new();
    let mut request = client.get(format!("{}/api/tags", base_url));
    if let Some(auth_header) = auth {
        request = request.header("Authorization", auth_header);
    }
    let ollama_models = request
        .send()
        .await
        .map_err(|_| "VLM API not available".to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let model_names: Vec<String> = ollama_models
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    // Update availability for each configured model
    let configs = state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to list configs: {}", e))?;

    for config in &configs {
        let is_available = model_names.iter().any(|n| n.starts_with(&config.name));
        let _ = state
            .prompt_manager
            .update_model_availability(&config.name, is_available)
            .await;
    }

    // Return updated list
    state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to refresh model configs: {}", e))
}

/// List available models from VLM API
#[tauri::command(rename_all = "camelCase")]
pub async fn list_ollama_models(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    // Use scoped block to release guard before async
    let (base_url, auth) = {
        let vlm = state.vlm_client.read();
        (vlm.get_base_url(), vlm.get_auth_header())
    };

    let client = reqwest::Client::new();
    let mut request = client.get(format!("{}/api/tags", base_url));
    if let Some(auth_header) = auth {
        request = request.header("Authorization", auth_header);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "VLM API not available".to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let models = response
        .get("models")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(models)
}

// ============================================
// Use Case Mapping Commands
// ============================================

/// List all use case mappings
#[tauri::command(rename_all = "camelCase")]
pub async fn list_use_cases(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::UseCase>, String> {
    state
        .prompt_manager
        .list_use_cases()
        .await
        .map_err(|e| format!("Failed to list use cases: {}", e))
}

/// Get a specific use case with resolved prompt and model
#[tauri::command(rename_all = "camelCase")]
pub async fn get_resolved_use_case(
    use_case: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ResolvedUseCase>, String> {
    state
        .prompt_manager
        .get_resolved_use_case(&use_case)
        .await
        .map_err(|e| format!("Failed to get use case: {}", e))
}

/// Update use case mapping
#[tauri::command(rename_all = "camelCase")]
pub async fn update_use_case_mapping(
    use_case: String,
    prompt_id: Option<String>,
    model_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::UseCase>, String> {
    state
        .prompt_manager
        .update_use_case_mapping(&use_case, prompt_id.as_deref(), model_id.as_deref())
        .await
        .map_err(|e| format!("Failed to update use case: {}", e))
}

/// Test a prompt with sample input
#[tauri::command(rename_all = "camelCase")]
pub async fn test_prompt(
    prompt_id: String,
    test_input: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get the prompt
    let prompt = state
        .prompt_manager
        .get_prompt(&prompt_id)
        .await
        .map_err(|e| format!("Failed to get prompt: {}", e))?
        .ok_or_else(|| "Prompt not found".to_string())?;

    // Get model config if specified
    let model_name = if let Some(ref model_id) = prompt.model_id {
        state
            .prompt_manager
            .get_model_config(model_id)
            .await
            .map_err(|e| format!("Failed to get model: {}", e))?
            .map(|m| m.name)
            .unwrap_or_else(|| "qwen2.5vl:7b".to_string())
    } else {
        "qwen2.5vl:7b".to_string()
    };

    // Get VLM API config - use scoped block to release guard before async
    let (base_url, auth) = {
        let vlm = state.vlm_client.read();
        (vlm.get_base_url(), vlm.get_auth_header())
    };

    // Call centralized API
    let client = reqwest::Client::new();
    let mut request = client
        .post(format!("{}/api/generate", base_url))
        .json(&serde_json::json!({
            "model": model_name,
            "prompt": format!("{}\n\nUser: {}", prompt.system_prompt, test_input),
            "stream": false,
            "options": {
                "temperature": prompt.temperature,
            }
        }));

    if let Some(auth_header) = auth {
        request = request.header("Authorization", auth_header);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to call VLM API: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(response
        .get("response")
        .and_then(|r| r.as_str())
        .unwrap_or("(No response)")
        .to_string())
}

// ============================================================================
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

    agent
        .generate(&segments, &metadata, minutes_since_start, None)
        .await
}

/// Get live insights stream for current recording
#[tauri::command(rename_all = "camelCase")]
pub async fn get_live_insights(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<LiveInsightEvent>, String> {
    // Get recent transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    // Process through live intel agent
    let mut agent = LiveIntelAgent::new();

    for transcript in transcripts.iter().rev().take(50).rev() {
        let segment = TranscriptSegment {
            id: transcript.id.to_string(),
            timestamp_ms: transcript.timestamp.timestamp_millis(),
            speaker: transcript.speaker.clone(),
            text: transcript.text.clone(),
        };
        agent.process_segment(segment);
    }

    Ok(agent.get_all_events().to_vec())
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

use crate::chunk_manager::{ChunkManager, StorageStats};
use crate::frame_extractor::{ExtractedFrame, FrameExtractor};
use crate::video_recorder::{PinMoment, RecordingSession, VideoRecorder};

// Lazy static for video recorder (global instance)
use std::sync::OnceLock;
static VIDEO_RECORDER: OnceLock<parking_lot::RwLock<VideoRecorder>> = OnceLock::new();
static FRAME_EXTRACTOR: OnceLock<FrameExtractor> = OnceLock::new();
static CHUNK_MANAGER: OnceLock<ChunkManager> = OnceLock::new();

fn get_video_recorder() -> &'static parking_lot::RwLock<VideoRecorder> {
    VIDEO_RECORDER.get_or_init(|| parking_lot::RwLock::new(VideoRecorder::default()))
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

/// Set AI provider configuration
#[tauri::command(rename_all = "camelCase")]
pub async fn set_ai_provider_settings(
    provider: String,
    url: Option<String>,
    key: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set_ai_provider(&provider)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(u) = url {
        state
            .settings
            .set_ai_remote_url(&u)
            .await
            .map_err(|e| e.to_string())?;
    }

    if let Some(k) = key {
        state
            .settings
            .set_ai_remote_key(&k)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Get AI provider configuration
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ai_provider_settings(
    state: State<'_, AppState>,
) -> Result<(String, Option<String>, Option<String>), String> {
    let settings = state.settings.get_all().await.map_err(|e| e.to_string())?;
    Ok((
        settings.ai_provider,
        settings.ai_remote_url,
        settings.ai_remote_key,
    ))
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
        state.accessibility_capture.start(
            state.database.clone(),
            state.settings.clone(),
            state.pinecone_client.clone(),
        )?;
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

// ============================================
// Activity Theme Commands
// ============================================

#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_theme(theme: String, state: State<'_, AppState>) -> Result<(), String> {
    // Validate theme name
    let valid_themes = [
        "prospecting",
        "fundraising",
        "product_dev",
        "admin",
        "personal",
    ];
    if !valid_themes.contains(&theme.as_str()) {
        return Err(format!(
            "Invalid theme: {}. Must be one of: {:?}",
            theme, valid_themes
        ));
    }

    // End any open theme session first
    if let Ok(Some(session_id)) = state.database.get_last_open_session().await {
        let _ = state.database.end_theme_session(session_id).await; // Best effort
    }

    // Save to settings
    state
        .settings
        .set_active_theme(&theme)
        .await
        .map_err(|e| format!("Failed to set theme: {}", e))?;

    // Apply theme-specific settings
    let interval_ms = state
        .settings
        .get_theme_interval(&theme)
        .await
        .map_err(|e| format!("Failed to get theme interval: {}", e))?;

    // Update capture engine interval (release lock immediately)
    {
        let engine = state.capture_engine.write();
        engine.set_frame_interval(interval_ms);
    } // Lock dropped here

    // Auto-enable mic transcription for prospecting/fundraising (meeting-heavy themes)
    let enable_mic = matches!(theme.as_str(), "prospecting" | "fundraising");
    state
        .settings
        .set_capture_microphone(enable_mic)
        .await
        .map_err(|e| format!("Failed to set mic capture: {}", e))?;

    // Start new theme session
    state
        .database
        .start_theme_session(&theme)
        .await
        .map_err(|e| format!("Failed to start theme session: {}", e))?;

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_recent_entities(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(50);
    state
        .database
        .get_recent_entities(limit as i32)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_active_theme(state: State<'_, AppState>) -> Result<String, String> {
    state
        .settings
        .get_active_theme()
        .await
        .map_err(|e| format!("Failed to get active theme: {}", e))
}

#[derive(serde::Serialize)]
pub struct ThemeSettings {
    active_theme: String,
    prospecting_interval_ms: u32,
    fundraising_interval_ms: u32,
    product_dev_interval_ms: u32,
    admin_interval_ms: u32,
    personal_interval_ms: u32,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_theme_settings(state: State<'_, AppState>) -> Result<ThemeSettings, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))?;

    Ok(ThemeSettings {
        active_theme: settings.active_theme,
        prospecting_interval_ms: settings.prospecting_interval_ms,
        fundraising_interval_ms: settings.fundraising_interval_ms,
        product_dev_interval_ms: settings.product_dev_interval_ms,
        admin_interval_ms: settings.admin_interval_ms,
        personal_interval_ms: settings.personal_interval_ms,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_theme_interval(
    theme: String,
    interval_ms: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Save to settings
    state
        .settings
        .set_theme_interval(&theme, interval_ms)
        .await
        .map_err(|e| format!("Failed to set interval: {}", e))?;

    // If this is the active theme, update capture engine immediately
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .map_err(|e| format!("Failed to get active theme: {}", e))?;

    if active_theme == theme {
        let engine = state.capture_engine.write();
        engine.set_frame_interval(interval_ms);
    }

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_theme_time_today(
    theme: String,
    state: State<'_, AppState>,
) -> Result<f64, String> {
    let seconds = state
        .database
        .get_theme_time_today(&theme)
        .await
        .map_err(|e| format!("Failed to get theme time: {}", e))?;

    // Convert to hours with 1 decimal place
    Ok((seconds as f64) / 3600.0)
}
// ============================================
// Phase 2: Theme-Specific Prompt Management Commands
// ============================================

use crate::prompt_manager::{Prompt, PromptUpdate};

/// List all prompts for a specific theme
#[tauri::command(rename_all = "camelCase")]
pub async fn list_prompts_by_theme(
    theme: String,
    state: State<'_, AppState>,
) -> Result<Vec<Prompt>, String> {
    state
        .prompt_manager
        .list_prompts_by_theme(&theme)
        .await
        .map_err(|e| format!("Failed to list prompts by theme: {}", e))
}

/// Get the latest version of a prompt by name and optional theme
#[tauri::command(rename_all = "camelCase")]
pub async fn get_latest_prompt(
    name: String,
    theme: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<Prompt>, String> {
    state
        .prompt_manager
        .get_latest_prompt(&name, theme.as_deref())
        .await
        .map_err(|e| format!("Failed to get latest prompt: {}", e))
}

/// Get all versions of a prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn get_prompt_versions(
    name: String,
    theme: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Prompt>, String> {
    state
        .prompt_manager
        .get_prompt_versions(&name, theme.as_deref())
        .await
        .map_err(|e| format!("Failed to get prompt versions: {}", e))
}

/// Create a new version of an existing prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_prompt_version(
    prompt_id: String,
    updates: PromptUpdate,
    state: State<'_, AppState>,
) -> Result<Prompt, String> {
    state
        .prompt_manager
        .create_prompt_version(&prompt_id, updates)
        .await
        .map_err(|e| format!("Failed to create prompt version: {}", e))
}

/// Create a theme-specific prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_theme_prompt(
    theme: String,
    name: String,
    description: String,
    category: String,
    system_prompt: String,
    model_id: Option<String>,
    temperature: Option<f32>,
    state: State<'_, AppState>,
) -> Result<Prompt, String> {
    state
        .prompt_manager
        .create_theme_prompt(
            &theme,
            &name,
            &description,
            &category,
            &system_prompt,
            model_id.as_deref(),
            temperature,
        )
        .await
        .map_err(|e| format!("Failed to create theme prompt: {}", e))
}
// ============================================================================
// Intelligence Pipeline Commands
// ============================================================================

/// Set enable ingest flag
#[tauri::command(rename_all = "camelCase")]
pub async fn set_enable_ingest(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set("enable_ingest", &enabled.to_string())
        .await
        .map_err(|e| format!("Failed to set enable_ingest: {}", e))
}

/// Set ingest configuration (base URL and bearer token)
#[tauri::command(rename_all = "camelCase")]
pub async fn set_ingest_config(
    base_url: String,
    bearer_token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .set("ingest_base_url", &base_url)
        .await
        .map_err(|e| format!("Failed to set ingest_base_url: {}", e))?;
    state
        .settings
        .set("ingest_bearer_token", &bearer_token)
        .await
        .map_err(|e| format!("Failed to set ingest_bearer_token: {}", e))?;
    Ok(())
}

/// Get ingest queue statistics
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ingest_queue_stats(state: State<'_, AppState>) -> Result<(usize, usize), String> {
    let queue = state.ingest_queue.lock();
    queue.get_stats().map_err(|e| e.to_string())
}

/// Test ingest connection
#[tauri::command(rename_all = "camelCase")]
pub async fn test_ingest_connection(state: State<'_, AppState>) -> Result<bool, String> {
    if let Some(ref client) = state.ingest_client {
        client.health_check().await.map_err(|e| e.to_string())
    } else {
        Err("Ingest client not initialized".to_string())
    }
}

/// Trigger manual ingest of a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn trigger_meeting_ingest(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = state
        .ingest_client
        .as_ref()
        .ok_or_else(|| "Ingest client not initialized".to_string())?;

    // Get meeting details
    let meeting = state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Meeting not found".to_string())?;

    // Create session start request
    let started_at = meeting.started_at.to_rfc3339();
    let metadata = serde_json::json!({
        "title": meeting.title,
        "meeting_id": meeting.id,
        "source": "nofriction_meetings",
        "manual_trigger": true
    });

    // Start session
    log::info!("Starting ingest session for meeting {}", meeting_id);
    let session_id = client
        .start_session(None, started_at, metadata)
        .await
        .map_err(|e| format!("Failed to start session: {}", e))?;

    // Get transcripts
    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    // Convert transcripts
    let segments: Vec<crate::ingest_client::TranscriptSegment> = transcripts
        .into_iter()
        .map(|t| crate::ingest_client::TranscriptSegment {
            start_at: t.timestamp.to_rfc3339(),
            end_at: t.timestamp.to_rfc3339(),
            text: t.text,
            speaker: t.speaker,
            confidence: Some(t.confidence as f64),
        })
        .collect();

    let segment_count = segments.len();

    if !segments.is_empty() {
        log::info!("Uploading {} transcript segments...", segment_count);
        client
            .upload_transcript(session_id, segments)
            .await
            .map_err(|e| format!("Failed to upload transcripts: {}", e))?;
    }

    // Get frames (limit to avoid overload)
    let frames = state
        .database
        .get_frames(&meeting_id, 200)
        .await
        .map_err(|e| e.to_string())?;
    log::info!("Found {} frames to upload...", frames.len());

    let mut success_frames = 0;
    for frame in frames {
        if let Some(path_str) = frame.file_path {
            let path = std::path::PathBuf::from(path_str);
            if path.exists() {
                match client
                    .upload_frame(session_id, frame.timestamp.to_rfc3339(), &path, None)
                    .await
                {
                    Ok(_) => success_frames += 1,
                    Err(e) => log::warn!("Failed to upload frame {}: {}", frame.id, e),
                }
            }
        }
    }

    // End session
    let ended_at = meeting.ended_at.unwrap_or(chrono::Utc::now()).to_rfc3339();
    client
        .end_session(session_id, ended_at)
        .await
        .map_err(|e| format!("Failed to end session: {}", e))?;

    Ok(format!(
        "Ingest complete. Uploaded {} frames and {} transcripts.",
        success_frames, segment_count
    ))
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

    // Load API key from settings and set it on the provider
    let provider_type = tm.get_provider_type();
    let api_key = match provider_type {
        ProviderType::Deepgram => state.settings.get_deepgram_api_key().await.ok().flatten(),
        ProviderType::Gemini => state.settings.get_gemini_api_key().await.ok().flatten(),
        ProviderType::Gladia => state.settings.get_gladia_api_key().await.ok().flatten(),
        ProviderType::GoogleSTT => state.settings.get_google_stt_key().await.ok().flatten(),
    };

    if let Some(key) = api_key {
        tm.set_api_key(key);
        log::info!("Loaded API key for {:?} from settings", provider_type);
    } else {
        log::warn!(
            "No API key found in settings for {:?} - transcription will fail",
            provider_type
        );
        return Err(format!("No API key configured for {:?}", provider_type));
    }

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

// ============================================
// Always-On Recording Commands
// ============================================

/// Get current capture mode
#[tauri::command(rename_all = "camelCase")]
pub async fn get_capture_mode(state: State<'_, AppState>) -> Result<String, String> {
    let engine = state.capture_engine.read();
    let mode = engine.get_mode();
    Ok(format!("{:?}", mode))
}

/// Start ambient capture (screen only, 30s intervals, no audio)
#[tauri::command(rename_all = "camelCase")]
pub async fn start_ambient_capture(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("🌙 Starting ambient capture mode");
    let engine = state.capture_engine.read();
    engine.start_ambient(app)?;

    // Prevent sleep
    let _ = state
        .power_manager
        .prevent_sleep("Ambient Capture Active")
        .map_err(|e| log::warn!("Failed to prevent sleep: {}", e));
    Ok(())
}

/// Start meeting capture (full audio + screen, 2s intervals)
#[tauri::command(rename_all = "camelCase")]
pub async fn start_meeting_capture(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("🎙️ Starting meeting capture mode");
    let engine = state.capture_engine.read();
    engine.start_meeting(app)?;

    // Prevent sleep with higher priority? (Using same IOPM assertion for now)
    let _ = state
        .power_manager
        .prevent_sleep("Meeting Capture Active")
        .map_err(|e| log::warn!("Failed to prevent sleep: {}", e));
    Ok(())
}

/// Pause capture (stop all without ending session)
#[tauri::command(rename_all = "camelCase")]
pub async fn pause_capture(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("⏸️ Pausing capture");
    let engine = state.capture_engine.read();
    let _ = engine.pause();
    state.power_manager.release_assertion();
    Ok(())
}

/// Get Always-On settings
#[derive(serde::Serialize)]
pub struct AlwaysOnSettings {
    pub enabled: bool,
    pub idle_timeout_mins: u32,
    pub ambient_interval_secs: u32,
    pub meeting_interval_secs: u32,
    pub retention_hours: u32,
    pub calendar_detection: bool,
    pub app_detection: bool,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_always_on_settings() -> Result<AlwaysOnSettings, String> {
    // TODO: Load from persistent settings
    Ok(AlwaysOnSettings {
        enabled: false,
        idle_timeout_mins: 5,
        ambient_interval_secs: 30,
        meeting_interval_secs: 2,
        retention_hours: 24,
        calendar_detection: true,
        app_detection: true,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_always_on_enabled(enabled: bool) -> Result<(), String> {
    log::info!("Setting Always-On enabled: {}", enabled);
    // TODO: Persist and actually start/stop services
    Ok(())
}

/// Get all running meeting apps
#[tauri::command(rename_all = "camelCase")]
pub async fn get_running_meeting_apps() -> Result<Vec<String>, String> {
    use crate::meeting_trigger::MeetingTriggerEngine;

    let default_apps = vec![
        "zoom.us".to_string(),
        "Zoom".to_string(),
        "Google Meet".to_string(),
        "Microsoft Teams".to_string(),
        "Teams".to_string(),
        "Slack".to_string(),
        "Discord".to_string(),
        "FaceTime".to_string(),
        "Webex".to_string(),
    ];

    Ok(MeetingTriggerEngine::get_running_meeting_apps(
        &default_apps,
    ))
}

/// Check if audio is being used (microphone active)
#[tauri::command(rename_all = "camelCase")]
pub async fn check_audio_usage() -> Result<bool, String> {
    use crate::meeting_trigger::MeetingTriggerEngine;
    Ok(MeetingTriggerEngine::check_audio_usage())
}

/// Dismiss a meeting detection suggestion
#[tauri::command(rename_all = "camelCase")]
pub async fn dismiss_meeting_detection(
    state: State<'_, AppState>,
    detection_id: String,
) -> Result<(), String> {
    state.meeting_trigger.dismiss_detection(&detection_id);
    Ok(())
}

/// Transform window between Insight Deck and Genie mode
#[tauri::command(rename_all = "camelCase")]
pub async fn set_genie_mode(window: Window, is_genie: bool) -> Result<(), String> {
    log::info!("Setting Genie mode: {}", is_genie);

    if is_genie {
        // Genie Mode: Minimized, always on top, no decorations
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_decorations(false).map_err(|e| e.to_string())?;
        window.set_always_on_top(true).map_err(|e| e.to_string())?;
        window.set_resizable(false).map_err(|e| e.to_string())?;

        // Set size to a compact bubble
        let genie_size = LogicalSize::new(320.0, 400.0);
        window.set_size(genie_size).map_err(|e| e.to_string())?;

        // Position in bottom right
        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_size = monitor.size();
            let scale_factor = window.scale_factor().unwrap_or(1.0);

            // Calculate bottom-right position (with 40px margin)
            let x = (monitor_size.width as f64 / scale_factor) - 320.0 - 40.0;
            let y = (monitor_size.height as f64 / scale_factor) - 400.0 - 40.0;

            window
                .set_position(LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
    } else {
        // Insight Deck Mode: Full size, decorated
        window.set_decorations(true).map_err(|e| e.to_string())?;
        window.set_always_on_top(false).map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;

        // Restore to default large size
        let deck_size = LogicalSize::new(1400.0, 900.0);
        window.set_size(deck_size).map_err(|e| e.to_string())?;

        // Center the window
        window.center().map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dork Mode (Study Mode) Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Set session mode ("standard" or "dork")
#[tauri::command(rename_all = "camelCase")]
pub async fn set_session_mode(state: State<'_, AppState>, mode: String) -> Result<(), String> {
    // Validate mode
    if mode != "standard" && mode != "dork" {
        return Err(format!(
            "Invalid session mode: {}. Use 'standard' or 'dork'",
            mode
        ));
    }

    // Save to settings
    state
        .settings
        .set_session_mode(&mode)
        .await
        .map_err(|e| format!("Failed to set session mode: {}", e))?;

    log::info!("📚 Session mode set to: {}", mode);
    Ok(())
}

/// Get current session mode
#[tauri::command(rename_all = "camelCase")]
pub async fn get_session_mode(state: State<'_, AppState>) -> Result<String, String> {
    state
        .settings
        .get_session_mode()
        .await
        .map_err(|e| format!("Failed to get session mode: {}", e))
}

/// Start a dork mode study session
#[tauri::command(rename_all = "camelCase")]
pub async fn start_dork_session(state: State<'_, AppState>) -> Result<String, String> {
    let session = crate::dork_mode::DorkModeSession::new(uuid::Uuid::new_v4().to_string());
    let session_id = session.session_id.clone();

    *state.dork_mode_session.write() = Some(session);

    log::info!("📚 Dork Mode session started: {}", session_id);
    Ok(session_id)
}

/// Add content to the current dork mode session
#[tauri::command(rename_all = "camelCase")]
pub async fn add_dork_content(state: State<'_, AppState>, content: String) -> Result<(), String> {
    let session_guard = state.dork_mode_session.read();
    if let Some(session) = session_guard.as_ref() {
        session.accumulate_content(&content);
        Ok(())
    } else {
        Err("No active dork mode session".to_string())
    }
}

/// End dork mode session and generate study materials
#[tauri::command(rename_all = "camelCase")]
pub async fn end_dork_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::dork_mode::StudyMaterials, String> {
    // Get and end the session
    let (session_id, content) = {
        let session_guard = state.dork_mode_session.read();
        if let Some(session) = session_guard.as_ref() {
            session.end_session();
            (session.session_id.clone(), session.get_all_content())
        } else {
            return Err("No active dork mode session".to_string());
        }
    };

    if content.trim().is_empty() {
        return Err("No content captured during session".to_string());
    }

    log::info!(
        "📚 Generating study materials for session {} ({} chars)",
        session_id,
        content.len()
    );

    // Emit progress event
    let _ = app.emit(
        "dork:generating",
        serde_json::json!({
            "session_id": session_id,
            "content_length": content.len()
        }),
    );

    // Generate study materials using AI
    // Clone the client to avoid holding the RwLockReadGuard across await
    let ai_client = state.ai_client.read().clone();
    let materials =
        crate::dork_mode::generate_study_materials(&ai_client, &session_id, &content).await?;

    // Save to database (if we add the table)
    // TODO: state.database.save_study_materials(...).await?;

    // Emit completion event
    let _ = app.emit("dork:materials_ready", &materials);

    // Clear the session
    *state.dork_mode_session.write() = None;

    log::info!("📚 Study materials generated for session {}", session_id);
    Ok(materials)
}

/// Get study materials for a previous session
#[tauri::command(rename_all = "camelCase")]
pub async fn get_study_materials(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<crate::dork_mode::StudyMaterials>, String> {
    // TODO: Implement database retrieval
    let _ = state;
    let _ = meeting_id;
    Ok(None)
}

// ═══════════════════════════════════════════════════════════════════════════
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

/// Add a comment to a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn add_meeting_comment(
    state: State<'_, AppState>,
    meeting_id: String,
    comment: String,
    comment_type: Option<String>,
    timestamp_ref: Option<f64>,
    parent_id: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();

    state
        .database
        .add_meeting_comment(
            &id,
            &meeting_id,
            &comment,
            comment_type.as_deref(),
            timestamp_ref,
            parent_id.as_deref(),
        )
        .await
        .map_err(|e| format!("Failed to add comment: {}", e))?;

    Ok(id)
}

/// Get comments for a meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_comments(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<crate::database::MeetingComment>, String> {
    state
        .database
        .get_meeting_comments(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get comments: {}", e))
}

/// Get full meeting analysis (notes + comments + transcripts)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_analysis(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<serde_json::Value, String> {
    let meeting = state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?
        .ok_or("Meeting not found")?;

    let transcripts = state
        .database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    let notes = state
        .database
        .get_meeting_notes(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get notes: {}", e))?;

    let comments = state
        .database
        .get_meeting_comments(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get comments: {}", e))?;

    Ok(serde_json::json!({
        "meeting": meeting,
        "transcripts": transcripts,
        "notes": notes,
        "comments": comments,
        "transcript_count": transcripts.len(),
        "comment_count": comments.len(),
        "has_notes": notes.is_some(),
    }))
}

// ============================================================================
// v3.0.0: Obsidian Vault Commands
// ============================================================================

/// Get vault configuration status
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let status = state.vault_manager.get_status().await;
    serde_json::to_value(&status).map_err(|e| e.to_string())
}

/// List all topics in the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn list_vault_topics(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let topics = state.vault_manager.list_topics().await?;
    serde_json::to_value(&topics).map_err(|e| e.to_string())
}

/// Get details for a single topic
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vault_topic(
    state: State<'_, AppState>,
    topic_name: String,
) -> Result<serde_json::Value, String> {
    let topic = state.vault_manager.get_topic(&topic_name).await?;
    serde_json::to_value(&topic).map_err(|e| e.to_string())
}

/// Create a new topic
#[tauri::command(rename_all = "camelCase")]
pub async fn create_vault_topic(
    state: State<'_, AppState>,
    name: String,
    tags: Vec<String>,
) -> Result<serde_json::Value, String> {
    let topic = state.vault_manager.create_topic(&name, tags).await?;
    serde_json::to_value(&topic).map_err(|e| e.to_string())
}

/// Export an existing meeting to the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn export_meeting_to_vault(
    state: State<'_, AppState>,
    topic_name: String,
    meeting_id: String,
) -> Result<String, String> {
    internal_export_meeting(
        state.database.clone(),
        state.vault_manager.clone(),
        topic_name,
        meeting_id,
    )
    .await
}

/// Internal helper for exporting a meeting to the vault
pub async fn internal_export_meeting(
    database: Arc<crate::database::DatabaseManager>,
    vault_manager: Arc<crate::obsidian_vault::VaultManager>,
    topic_name: String,
    meeting_id: String,
) -> Result<String, String> {
    // Get meeting data from database
    let meeting = database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?
        .ok_or_else(|| format!("Meeting {} not found", meeting_id))?;

    let transcripts = database
        .get_transcripts(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;

    // Get meeting notes if available
    let notes = database
        .get_meeting_notes(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get notes: {}", e))?;

    // Get frames for screenshot paths
    let frames = database
        .get_frames(&meeting_id, 1000)
        .await
        .map_err(|e| format!("Failed to get frames: {}", e))?;

    let transcript_tuples: Vec<(String, Option<String>, String)> = transcripts
        .iter()
        .map(|t| (t.text.clone(), t.speaker.clone(), t.timestamp.to_rfc3339()))
        .collect();

    let screenshot_paths: Vec<String> = frames.iter().filter_map(|f| f.file_path.clone()).collect();

    let summary = notes.as_ref().and_then(|n| n.summary.as_deref());
    let key_topics = notes.as_ref().and_then(|n| n.key_topics.as_deref());
    let action_items = notes.as_ref().and_then(|n| n.action_items.as_deref());

    // Generate AI Intelligence from transcripts
    let mut intel_agent = LiveIntelAgent::new();
    for transcript in transcripts.iter() {
        let segment = TranscriptSegment {
            id: transcript.id.to_string(),
            timestamp_ms: transcript.timestamp.timestamp_millis(),
            speaker: transcript.speaker.clone(),
            text: transcript.text.clone(),
        };
        intel_agent.process_segment(segment);
    }
    let insights = intel_agent.get_all_events().to_vec();

    // Categorize insights into markdown sections
    let mut ai_action_items = Vec::new();
    let mut ai_decisions = Vec::new();
    let mut ai_risks = Vec::new();
    let mut ai_commitments = Vec::new();
    let mut ai_questions = Vec::new();
    let mut ai_topic_shifts = Vec::new();

    for insight in &insights {
        match insight {
            LiveInsightEvent::ActionItem { text, assignee, .. } => {
                if let Some(a) = assignee {
                    ai_action_items.push(format!("- {} *(assigned: {})*", text, a));
                } else {
                    ai_action_items.push(format!("- {}", text));
                }
            }
            LiveInsightEvent::Decision { text, .. } => {
                ai_decisions.push(format!("- {}", text));
            }
            LiveInsightEvent::RiskSignal { text, .. } => {
                ai_risks.push(format!("- ⚠️ {}", text));
            }
            LiveInsightEvent::Commitment { text, .. } => {
                ai_commitments.push(format!("- 🤝 {}", text));
            }
            LiveInsightEvent::QuestionSuggestion { text, .. } => {
                ai_questions.push(format!("- ❓ {}", text));
            }
            LiveInsightEvent::TopicShift {
                from_topic,
                to_topic,
                ..
            } => {
                ai_topic_shifts.push(format!("- 🎯 {} → {}", from_topic, to_topic));
            }
        }
    }

    let mut intelligence_md = String::new();
    if !ai_action_items.is_empty() {
        intelligence_md.push_str("### Action Items\n\n");
        intelligence_md.push_str(&ai_action_items.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_decisions.is_empty() {
        intelligence_md.push_str("### Decisions\n\n");
        intelligence_md.push_str(&ai_decisions.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_risks.is_empty() {
        intelligence_md.push_str("### Risk Signals\n\n");
        intelligence_md.push_str(&ai_risks.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_commitments.is_empty() {
        intelligence_md.push_str("### Commitments\n\n");
        intelligence_md.push_str(&ai_commitments.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_questions.is_empty() {
        intelligence_md.push_str("### Questions & Suggestions\n\n");
        intelligence_md.push_str(&ai_questions.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_topic_shifts.is_empty() {
        intelligence_md.push_str("### Topic Shifts\n\n");
        intelligence_md.push_str(&ai_topic_shifts.join("\n"));
        intelligence_md.push_str("\n\n");
    }

    let intelligence = if intelligence_md.is_empty() {
        None
    } else {
        Some(intelligence_md.as_str())
    };

    vault_manager
        .export_meeting(
            &topic_name,
            &meeting_id,
            &meeting.title,
            &meeting.started_at.to_rfc3339(),
            meeting.duration_seconds,
            &transcript_tuples,
            summary,
            key_topics,
            action_items,
            intelligence,
            &screenshot_paths,
        )
        .await
}

/// Read a file from the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn read_vault_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<serde_json::Value, String> {
    let content = state.vault_manager.read_file(&file_path).await?;
    serde_json::to_value(&content).map_err(|e| e.to_string())
}

/// Write a note to a topic
#[tauri::command(rename_all = "camelCase")]
pub async fn write_vault_note(
    state: State<'_, AppState>,
    topic_name: String,
    file_name: String,
    content: String,
) -> Result<String, String> {
    state
        .vault_manager
        .write_note(&topic_name, &file_name, &content)
        .await
}

/// Upload a file to a topic
#[tauri::command(rename_all = "camelCase")]
pub async fn upload_to_vault(
    state: State<'_, AppState>,
    topic_name: String,
    source_path: String,
    dest_name: Option<String>,
) -> Result<String, String> {
    state
        .vault_manager
        .upload_file(&topic_name, &source_path, dest_name.as_deref())
        .await
}

/// List files in the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn list_vault_files(
    state: State<'_, AppState>,
    sub_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let files = state.vault_manager.list_files(sub_path.as_deref()).await?;
    serde_json::to_value(&files).map_err(|e| e.to_string())
}

/// Search the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn search_vault(
    state: State<'_, AppState>,
    query: String,
) -> Result<serde_json::Value, String> {
    let results = state.vault_manager.search(&query).await?;
    serde_json::to_value(&results).map_err(|e| e.to_string())
}

/// Get the vault tree structure
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vault_tree(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let tree = state.vault_manager.get_tree().await?;
    serde_json::to_value(&tree).map_err(|e| e.to_string())
}

/// Delete a file or folder from the vault
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_vault_item(
    state: State<'_, AppState>,
    item_path: String,
) -> Result<(), String> {
    state.vault_manager.delete_item(&item_path).await
}

/// Set the vault path and persist to settings
#[tauri::command(rename_all = "camelCase")]
pub async fn set_vault_path(state: State<'_, AppState>, vault_path: String) -> Result<(), String> {
    // Validate the path exists
    let path = std::path::Path::new(&vault_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!(
            "Invalid vault path: {} (must be an existing directory)",
            vault_path
        ));
    }

    // Update the vault manager
    state.vault_manager.set_vault_path(vault_path.clone());

    // Ensure folder structure
    state.vault_manager.ensure_structure().await?;

    // Persist to settings
    state
        .settings
        .set_vault_path(&vault_path)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Obsidian Knowledge Management Commands
// ═══════════════════════════════════════════════════════════════════

/// Get backlinks pointing to a specific file
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vault_backlinks(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<serde_json::Value, String> {
    let result = state.vault_manager.get_backlinks(&file_path).await?;
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// List all tags in the vault with file counts
#[tauri::command(rename_all = "camelCase")]
pub async fn list_vault_tags(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let tags = state.vault_manager.list_tags().await?;
    serde_json::to_value(&tags).map_err(|e| e.to_string())
}

/// Get files with a specific tag
#[tauri::command(rename_all = "camelCase")]
pub async fn get_files_by_tag(
    state: State<'_, AppState>,
    tag: String,
) -> Result<serde_json::Value, String> {
    let files = state.vault_manager.get_files_by_tag(&tag).await?;
    serde_json::to_value(&files).map_err(|e| e.to_string())
}

/// Build the knowledge graph for visualization
#[tauri::command(rename_all = "camelCase")]
pub async fn get_vault_graph(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let graph = state.vault_manager.build_graph().await?;
    serde_json::to_value(&graph).map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════
// v3.1.0: Calendar Intelligence Commands
// ═══════════════════════════════════════════════════════════════════

/// Generate meeting intelligence — AI briefings for all attendees and their companies
#[tauri::command(rename_all = "camelCase")]
pub async fn generate_meeting_intel(
    state: State<'_, AppState>,
    event_id: String,
    topic_name: String,
) -> Result<serde_json::Value, String> {
    use crate::attendee_intel;

    // Fetch calendar events to find the target event
    let event = {
        let client = state.calendar_client.read();
        let events = client
            .fetch_events()
            .map_err(|e| format!("Calendar error: {}", e))?;
        events
            .into_iter()
            .find(|e| e.event_id == event_id)
            .ok_or_else(|| format!("Calendar event '{}' not found", event_id))?
    };

    if event.attendees.is_empty() {
        return Err("No attendees found for this calendar event".to_string());
    }

    // Generate AI intelligence for all attendees
    let ai_client = state.ai_client.read().clone();
    let intel_package =
        attendee_intel::generate_meeting_intel(&ai_client, &event.title, &event.attendees).await?;

    // Ensure vault structure
    state.vault_manager.ensure_structure().await?;

    // Write person notes to vault
    let meeting_link = event.title.clone();
    for profile in &intel_package.attendees {
        state
            .vault_manager
            .write_person_note(
                &profile.name,
                &profile.email,
                &profile.company,
                &profile.briefing,
                &[meeting_link.clone()],
            )
            .await?;
    }

    // Write company notes to vault
    for company in &intel_package.companies {
        let people_in_company: Vec<String> = intel_package
            .attendees
            .iter()
            .filter(|a| a.company_domain == company.domain)
            .map(|a| a.name.clone())
            .collect();

        state
            .vault_manager
            .write_company_note(
                &company.name,
                &company.domain,
                &company.briefing,
                &people_in_company,
            )
            .await?;
    }

    // Write meeting prep to vault
    let attendee_names: Vec<String> = intel_package
        .attendees
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let event_date = event.start_time.to_rfc3339();
    state
        .vault_manager
        .write_meeting_prep(
            &topic_name,
            &event.title,
            &event_date,
            &attendee_names,
            &intel_package.meeting_prep,
        )
        .await?;

    // Return summary
    let summary = serde_json::json!({
        "event_title": event.title,
        "attendees_count": intel_package.attendees.len(),
        "companies_count": intel_package.companies.len(),
        "attendees": intel_package.attendees.iter().map(|a| serde_json::json!({
            "name": a.name,
            "email": a.email,
            "company": a.company,
        })).collect::<Vec<_>>(),
        "companies": intel_package.companies.iter().map(|c| serde_json::json!({
            "name": c.name,
            "domain": c.domain,
        })).collect::<Vec<_>>(),
    });

    Ok(summary)
}

/// Get calendar events enriched with parsed attendee names and company info
#[tauri::command(rename_all = "camelCase")]
pub async fn get_enriched_calendar_events(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    use crate::attendee_intel;

    let events = {
        let client = state.calendar_client.read();
        client
            .fetch_events()
            .map_err(|e| format!("Calendar error: {}", e))?
    };

    let enriched: Vec<serde_json::Value> = events
        .iter()
        .filter(|e| !e.is_all_day) // Skip all-day events
        .map(|event| {
            let enriched_attendees: Vec<serde_json::Value> = event
                .attendees
                .iter()
                .map(|email| {
                    let name = attendee_intel::extract_name_from_email(email);
                    let (domain, company) = attendee_intel::extract_company_from_email(email);
                    serde_json::json!({
                        "email": email,
                        "name": name,
                        "company": company,
                        "domain": domain,
                    })
                })
                .collect();

            serde_json::json!({
                "event_id": event.event_id,
                "title": event.title,
                "start_time": event.start_time.to_rfc3339(),
                "end_time": event.end_time.to_rfc3339(),
                "location": event.location,
                "meeting_url": event.meeting_url,
                "calendar_name": event.calendar_name,
                "attendees": enriched_attendees,
                "attendee_count": event.attendees.len(),
            })
        })
        .collect();

    Ok(serde_json::json!(enriched))
}
