// ============================================================================
// Intelligence Pipeline Commands
// ============================================================================

/// Set enable ingest flag
#[tauri::command]
pub async fn set_enable_ingest(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings
        .set("enable_ingest", &enabled.to_string())
        .await
        .map_err(|e| format!("Failed to set enable_ingest: {}", e))
}

/// Set ingest configuration (base URL and bearer token)
#[tauri::command]
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
#[tauri::command]
pub async fn get_ingest_queue_stats(state: State<'_, AppState>) -> Result<(usize, usize), String> {
    let queue = state.ingest_queue.lock();
    queue.get_stats().map_err(|e| e.to_string())
}

/// Test ingest connection
#[tauri::command]
    if let Some(ref client) = state.ingest_client {
        client.health_check().await.map_err(|e| e.to_string())
    } else {
        Err("Ingest client not initialized".to_string())
    }
}

/// Trigger manual ingest of a meeting
#[tauri::command]
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
            end_at: t.timestamp.to_rfc3339(), // Approximate duration if needed
            text: t.text,
            speaker: t.speaker,
            confidence: Some(t.confidence as f64),
        })
        .collect();

    if !segments.is_empty() {
        log::info!("Uploading {} transcript segments...", segments.len());
        client
            .upload_transcript(session_id, segments)
            .await
            .map_err(|e| format!("Failed to upload transcripts: {}", e))?;
    }

    // Get frames
    let frames = state.database.get_frames(&meeting_id, 1000).await.map_err(|e| e.to_string())?;
    log::info!("Found {} frames to upload...", frames.len());

    let mut success_frames = 0;
    for frame in frames {
        if let Some(path_str) = frame.file_path {
            let path = std::path::PathBuf::from(path_str);
            if path.exists() {
                match client.upload_frame(
                    session_id,
                    frame.timestamp.to_rfc3339(),
                    &path,
                    None
                ).await {
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

    Ok(format!("Ingest complete. Uploaded {} frames and transcripts.", success_frames))
}
