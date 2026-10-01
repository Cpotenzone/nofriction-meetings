// Capture source selection: which displays and windows get screenshotted
// during a meeting, plus an on-demand "snap this now" action.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::capture_engine::{CaptureEngine, CaptureSource, CaptureTarget};
use crate::AppState;

const TARGETS_SETTING_KEY: &str = "capture_targets";

/// Displays + visible windows for the picker (with small previews).
#[tauri::command(rename_all = "camelCase")]
pub async fn list_capture_sources(with_thumbnails: Option<bool>) -> Result<Vec<CaptureSource>, String> {
    let thumbs = with_thumbnails.unwrap_or(true);
    tokio::task::spawn_blocking(move || CaptureEngine::list_capture_sources(thumbs))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_capture_targets(state: State<'_, AppState>) -> Result<Vec<CaptureTarget>, String> {
    Ok(state.capture_engine.read().capture_targets())
}

/// Set (and persist) the capture targets. An empty list means "the main
/// display". Applies immediately, including mid-recording.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_capture_targets(
    targets: Vec<CaptureTarget>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.capture_engine.read().set_capture_targets(targets.clone());
    let json = serde_json::to_string(&targets).map_err(|e| e.to_string())?;
    state
        .settings
        .set(TARGETS_SETTING_KEY, &json)
        .await
        .map_err(|e| e.to_string())
}

/// Restore persisted targets at startup. Window ids don't survive app
/// restarts, so only displays are restored.
pub async fn load_persisted_targets(settings: &crate::settings::SettingsManager) -> Vec<CaptureTarget> {
    settings
        .get(TARGETS_SETTING_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<Vec<CaptureTarget>>(&j).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|t| matches!(t, CaptureTarget::Display { .. }))
        .collect()
}

#[derive(serde::Serialize)]
pub struct Snapshot {
    pub path: String,
    pub label: String,
    pub state_id: Option<String>,
    pub meeting_id: Option<String>,
}

/// Capture one display/window right now at full resolution. During a
/// recording it's filed into the meeting's timeline; otherwise it's saved
/// to the snapshots folder.
#[tauri::command(rename_all = "camelCase")]
pub async fn snap_capture_target(
    app: AppHandle,
    target: CaptureTarget,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    let t = target.clone();
    let (image, label, app_name, _) =
        tokio::task::spawn_blocking(move || CaptureEngine::capture_target(&t))
            .await
            .map_err(|e| e.to_string())??;

    let now = chrono::Utc::now();
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let meeting_id = if state.capture_engine.read().is_recording() {
        state.state_builder.read().current_meeting_id()
    } else {
        None
    };

    let state_id = uuid::Uuid::new_v4().to_string();
    let dir = match &meeting_id {
        Some(mid) => data_dir.join("frames").join(mid),
        None => data_dir.join("snapshots"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("snap_{}.jpg", state_id));
    let save_path = path.clone();
    tokio::task::spawn_blocking(move || image.to_rgb8().save(&save_path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Failed to save snapshot: {}", e))?;

    let path_str = path.to_string_lossy().to_string();
    let mut recorded_state = None;
    if let Some(mid) = &meeting_id {
        state
            .database
            .add_screen_state(&state_id, mid, now, Some(now), "", 0.0, Some(&path_str), "snapshot", "{}")
            .await
            .map_err(|e| e.to_string())?;
        let _ = state
            .database
            .set_screen_state_source(&state_id, &target.key(), &label, app_name.as_deref())
            .await;
        recorded_state = Some(state_id.clone());
    }

    let _ = app.emit(
        "frame_captured",
        serde_json::json!({
            "state_id": recorded_state,
            "meeting_id": meeting_id,
            "path": path_str,
            "source": target.key(),
            "label": label,
            "timestamp": now.to_rfc3339(),
            "manual": true,
        }),
    );
    log::info!("📸 Snapshot of {} → {}", label, path_str);

    Ok(Snapshot {
        path: path_str,
        label,
        state_id: recorded_state,
        meeting_id,
    })
}

/// Pause the active recording: audio and screenshots are dropped until
/// resumed. The meeting, transcript and timeline stay open.
#[tauri::command(rename_all = "camelCase")]
pub async fn pause_recording(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.capture_engine.read();
    if !engine.is_recording() {
        return Err("Not recording".into());
    }
    engine.set_paused(true);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn resume_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.capture_engine.read().set_paused(false);
    Ok(())
}
