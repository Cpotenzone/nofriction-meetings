// noFriction Meetings - Extended Capture Commands
// Always-on recording, dork mode (study mode), capture metrics

use crate::AppState;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, State, Window};

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

/// Start meeting capture (full audio + screen) at the user's configured
/// screenshot interval (default 1s)
#[tauri::command(rename_all = "camelCase")]
pub async fn start_meeting_capture(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let interval_ms = state
        .settings
        .get_all()
        .await
        .map(|s| s.frame_capture_interval_ms)
        .unwrap_or(1000);

    log::info!("🎙️ Starting meeting capture mode ({}ms frames)", interval_ms);
    let engine = state.capture_engine.read();
    engine.start_meeting(app, interval_ms)?;

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
        // Genie Mode: Compact overlay, always on top, no decorations, resizable
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_decorations(false).map_err(|e| e.to_string())?;
        window.set_always_on_top(true).map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;

        // Set minimum size so it can't be shrunk to nothing
        let min_size = LogicalSize::new(280.0, 300.0);
        window
            .set_min_size(Some(min_size))
            .map_err(|e| e.to_string())?;

        // Set initial compact size
        let genie_size = LogicalSize::new(320.0, 400.0);
        window.set_size(genie_size).map_err(|e| e.to_string())?;

        // Position in bottom right as default starting position
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

        // Clear min-size constraint from genie mode
        window
            .set_min_size(None::<LogicalSize<f64>>)
            .map_err(|e| e.to_string())?;

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
