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
pub async fn test_ingest_connection(state: State<'_, AppState>) -> Result<bool, String> {
    if let Some(ref client) = state.ingest_client {
        client.health_check().await.map_err(|e| e.to_string())
    } else {
        Err("Ingest client not initialized".to_string())
    }
}
