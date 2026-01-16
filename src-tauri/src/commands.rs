// noFriction Meetings - Tauri Commands
// Frontend-callable commands for recording, transcription, frames, and settings

use crate::capture_engine::{AudioBuffer, AudioDevice, MonitorInfo, RecordingStatus, CapturedFrame};
use crate::database::{Meeting, SearchResult, Transcript, Frame, SyncedTimeline};
use crate::settings::AppSettings;
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, State, Manager};
use base64::Engine;

/// Start recording with frame capture and live transcription
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Generate a new meeting ID
    let meeting_id = uuid::Uuid::new_v4().to_string();
    let title = format!("Meeting {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));

    // Create meeting in database
    state.database.create_meeting(&meeting_id, &title).await
        .map_err(|e| format!("Failed to create meeting: {}", e))?;

    // Get app data directory for frame storage
    let frames_dir = app.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("frames")
        .join(&meeting_id);
    
    std::fs::create_dir_all(&frames_dir)
        .map_err(|e| format!("Failed to create frames directory: {}", e))?;

    // Set up Deepgram connection if API key is configured
    {
        let client = state.deepgram_client.read();
        if client.has_api_key() {
            log::info!("Setting up Deepgram connection for live transcription...");
            client.set_app_handle(app.clone());
            client.set_database(state.database.clone());
            client.set_meeting_id(meeting_id.clone());
            client.start_connection();
        } else {
            log::warn!("No Deepgram API key configured - recording without transcription");
        }
    }

    // Set up audio callback to stream to Deepgram
    let deepgram = state.deepgram_client.clone();
    
    let audio_callback: Arc<dyn Fn(AudioBuffer) + Send + Sync> = Arc::new(move |buffer| {
        if buffer.samples.is_empty() {
            return;
        }
        
        // Queue audio to Deepgram (non-blocking)
        let client = deepgram.read();
        client.queue_audio(&buffer.samples, buffer.sample_rate);
    });

    // Set up frame callback to store screenshots
    let db_for_frames = state.database.clone();
    let meeting_id_for_frames = meeting_id.clone();
    let frames_dir_clone = frames_dir.clone();
    
    let frame_callback: Arc<dyn Fn(CapturedFrame) + Send + Sync> = Arc::new(move |frame| {
        let db = db_for_frames.clone();
        let mid = meeting_id_for_frames.clone();
        let dir = frames_dir_clone.clone();
        
        // Save frame asynchronously
        tokio::spawn(async move {
            // Generate thumbnail path
            let filename = format!("frame_{}.jpg", frame.frame_number);
            let thumbnail_path = dir.join(&filename);
            
            // Save as JPEG thumbnail
            if let Err(e) = frame.image.to_rgb8().save(&thumbnail_path) {
                log::warn!("Failed to save frame thumbnail: {}", e);
                return;
            }
            
            // Store in database
            if let Err(e) = db.add_frame(
                &mid,
                frame.timestamp,
                Some(thumbnail_path.to_str().unwrap_or("")),
                None, // OCR text - could be added later
            ).await {
                log::warn!("Failed to save frame to database: {}", e);
            }
        });
    });

    // Set callbacks and start capture
    {
        let engine = state.capture_engine.read();
        engine.set_audio_callback(audio_callback);
        engine.set_frame_callback(frame_callback);
    }

    {
        let engine = state.capture_engine.read();
        engine.start(app)?;
    }

    log::info!("Recording started: {} (frames saved to {:?})", meeting_id, frames_dir);
    Ok(meeting_id)
}

/// Stop recording
#[tauri::command]
pub async fn stop_recording(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let was_recording = {
        let engine = state.capture_engine.read();
        engine.get_status().is_recording
    };

    // Stop capture engine
    {
        let engine = state.capture_engine.read();
        engine.stop()?;
    }

    // Disconnect from Deepgram (synchronous flag set)
    {
        let client = state.deepgram_client.read();
        client.set_disconnected();
    }

    if was_recording {
        log::info!("Recording stopped successfully");
    }
    
    Ok(())
}

/// Get recording status
#[tauri::command]
pub async fn get_recording_status(
    state: State<'_, AppState>,
) -> Result<RecordingStatus, String> {
    let engine = state.capture_engine.read();
    Ok(engine.get_status())
}

/// Capture a single screenshot (for preview)
#[tauri::command]
pub async fn capture_screenshot(
    monitor_id: Option<u32>,
) -> Result<String, String> {
    let image = crate::capture_engine::CaptureEngine::capture_screenshot(monitor_id)?;
    
    // Convert to base64 JPEG for frontend display
    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);
    image.to_rgb8().write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image: {}", e))?;
    
    let base64 = base64::engine::general_purpose::STANDARD.encode(&buffer);
    Ok(format!("data:image/jpeg;base64,{}", base64))
}

/// Get transcripts for a meeting
#[tauri::command]
pub async fn get_transcripts(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Transcript>, String> {
    state.database.get_transcripts(&meeting_id).await
        .map_err(|e| format!("Failed to get transcripts: {}", e))
}

/// Search transcripts across all meetings
#[tauri::command]
pub async fn search_transcripts(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    state.database.search_transcripts(&query).await
        .map_err(|e| format!("Failed to search: {}", e))
}

/// Get frames for a meeting (rewind timeline)
#[tauri::command]
pub async fn get_frames(
    meeting_id: String,
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Frame>, String> {
    let limit = limit.unwrap_or(1000);
    state.database.get_frames(&meeting_id, limit).await
        .map_err(|e| format!("Failed to get frames: {}", e))
}

/// Get frame count for a meeting
#[tauri::command]
pub async fn get_frame_count(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    state.database.count_frames(&meeting_id).await
        .map_err(|e| format!("Failed to count frames: {}", e))
}

/// Get a frame thumbnail as base64
#[tauri::command]
pub async fn get_frame_thumbnail(
    frame_id: String,
    _thumbnail: bool,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // Parse frame_id (handles both integer and string IDs)
    let id: i64 = frame_id.parse().unwrap_or(0);
    
    // Get frame from database - search through recent meetings
    let meetings = state.database.list_meetings(100).await
        .map_err(|e| format!("Failed to get meetings: {}", e))?;
    
    for meeting in meetings {
        let frames = state.database.get_frames(&meeting.id, 10000).await
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
    
    Ok(None)
}

/// Get available audio devices
#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    crate::capture_engine::CaptureEngine::list_audio_devices()
}

/// Set the audio input device (persisted)
#[tauri::command]
pub async fn set_audio_device(
    device_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let engine = state.capture_engine.read();
        engine.set_microphone(device_id.clone());
    }
    
    state.settings.set_selected_microphone(&device_id).await
        .map_err(|e| format!("Failed to save setting: {}", e))?;
    
    log::info!("Microphone set to: {}", device_id);
    Ok(())
}

/// Get available monitors
#[tauri::command]
pub async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    crate::capture_engine::CaptureEngine::list_monitors()
}

/// Set the monitor (persisted)
#[tauri::command]
pub async fn set_monitor(
    monitor_id: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let engine = state.capture_engine.read();
        engine.set_monitor(monitor_id);
    }
    
    state.settings.set_selected_monitor(monitor_id).await
        .map_err(|e| format!("Failed to save setting: {}", e))?;
    
    log::info!("Monitor set to: {}", monitor_id);
    Ok(())
}

/// Set the Deepgram API key (persisted)
#[tauri::command]
pub async fn set_deepgram_api_key(
    api_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let client = state.deepgram_client.read();
        client.set_api_key(api_key.clone());
    }
    
    state.settings.set_deepgram_api_key(&api_key).await
        .map_err(|e| format!("Failed to save API key: {}", e))?;
    
    log::info!("Deepgram API key saved");
    Ok(())
}

/// Get the Deepgram API key (masked for display)
#[tauri::command]
pub async fn get_deepgram_api_key(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let key = state.settings.get_deepgram_api_key().await
        .map_err(|e| format!("Failed to get API key: {}", e))?;
    
    Ok(key.map(|k| {
        if k.len() > 8 {
            format!("{}...{}", &k[..4], &k[k.len()-4..])
        } else {
            "****".to_string()
        }
    }))
}

/// Get all settings
#[tauri::command]
pub async fn get_settings(
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    state.settings.get_all().await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

/// Get a single setting value
#[tauri::command]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state.settings.get(&key).await
        .map_err(|e| format!("Failed to get setting: {}", e))
}

/// Get all meetings
#[tauri::command]
pub async fn get_meetings(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Meeting>, String> {
    let limit = limit.unwrap_or(50);
    state.database.list_meetings(limit).await
        .map_err(|e| format!("Failed to list meetings: {}", e))
}

/// Get a single meeting
#[tauri::command]
pub async fn get_meeting(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Option<Meeting>, String> {
    state.database.get_meeting(&meeting_id).await
        .map_err(|e| format!("Failed to get meeting: {}", e))
}

/// Delete a meeting
#[tauri::command]
pub async fn delete_meeting(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.database.delete_meeting(&meeting_id).await
        .map_err(|e| format!("Failed to delete meeting: {}", e))?;
    
    log::info!("Meeting deleted: {}", meeting_id);
    Ok(())
}

/// Get synced timeline for rewind (frames + transcripts aligned by timestamp)
#[tauri::command]
pub async fn get_synced_timeline(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SyncedTimeline>, String> {
    state.database.get_synced_timeline(&meeting_id).await
        .map_err(|e| format!("Failed to get synced timeline: {}", e))
}

// ============================================
// AI Commands (Ollama Integration)
// ============================================

use crate::ai_client::{AIClient, AIPreset, ChatMessage, OllamaModel};

/// Check if Ollama is available
#[tauri::command]
pub async fn check_ollama() -> Result<bool, String> {
    let client = AIClient::new();
    Ok(client.is_available().await)
}

/// Get available Ollama models
#[tauri::command]
pub async fn get_ollama_models() -> Result<Vec<OllamaModel>, String> {
    let client = AIClient::new();
    client.list_models().await
}

/// Get AI presets
#[tauri::command]
pub async fn get_ai_presets() -> Result<Vec<AIPreset>, String> {
    Ok(AIPreset::get_all_presets())
}

/// Chat with AI using a preset
#[tauri::command]
pub async fn ai_chat(
    preset_id: String,
    message: String,
    meeting_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();
    
    // Get the preset
    let presets = AIPreset::get_all_presets();
    let preset = presets.iter()
        .find(|p| p.id == preset_id)
        .cloned()
        .unwrap_or_else(AIPreset::qa);
    
    // Build context from meeting if provided
    let context = if let Some(ref id) = meeting_id {
        let transcripts = state.database.get_transcripts(id).await
            .map_err(|e| format!("Failed to get transcripts: {}", e))?;
        
        let transcript_text: String = transcripts.iter()
            .filter(|t| t.is_final)
            .map(|t| format!("[{}] {}: {}", 
                t.timestamp.format("%H:%M:%S"),
                t.speaker.as_deref().unwrap_or("Speaker"),
                t.text
            ))
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
#[tauri::command]
pub async fn summarize_meeting(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();
    
    // Get transcripts
    let transcripts = state.database.get_transcripts(&meeting_id).await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;
    
    if transcripts.is_empty() {
        return Err("No transcripts found for this meeting".to_string());
    }
    
    let content: String = transcripts.iter()
        .filter(|t| t.is_final)
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    
    client.summarize(&content).await
}

/// Extract action items from a meeting
#[tauri::command]
pub async fn extract_action_items(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = AIClient::new();
    
    // Get transcripts
    let transcripts = state.database.get_transcripts(&meeting_id).await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;
    
    if transcripts.is_empty() {
        return Err("No transcripts found for this meeting".to_string());
    }
    
    let content: String = transcripts.iter()
        .filter(|t| t.is_final)
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    
    client.extract_action_items(&content).await
}

// ============================================
// Knowledge Base Commands (VLM, Supabase, Pinecone)
// ============================================

use crate::vlm_client::ActivityContext;
use crate::supabase_client::Activity;
use crate::pinecone_client::{VectorMatch, ActivityMetadata};

/// Check if Ollama VLM is available
#[tauri::command]
pub async fn check_vlm(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let (base_url, _) = state.vlm_client.read().get_url_and_model();
    Ok(crate::vlm_client::vlm_is_available(&base_url).await)
}

/// Check if VLM has vision model
#[tauri::command]
pub async fn check_vlm_vision(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let (base_url, model) = state.vlm_client.read().get_url_and_model();
    crate::vlm_client::vlm_has_vision_model(&base_url, &model).await
}

/// Analyze a frame with VLM
#[tauri::command]
pub async fn analyze_frame(
    frame_path: String,
    state: State<'_, AppState>,
) -> Result<ActivityContext, String> {
    let (base_url, model) = state.vlm_client.read().get_url_and_model();
    crate::vlm_client::vlm_analyze_frame(&base_url, &model, &frame_path).await
}

/// Analyze multiple frames (batch)
#[tauri::command]
pub async fn analyze_frames_batch(
    frame_paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ActivityContext>, String> {
    let (base_url, model) = state.vlm_client.read().get_url_and_model();
    crate::vlm_client::vlm_analyze_frames_batch(&base_url, &model, &frame_paths).await
}

/// Configure Supabase connection
#[tauri::command]
pub async fn configure_supabase(
    connection_string: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Set connection string (sync), drop guard
    state.supabase_client.read().set_connection_string(connection_string.clone());
    // Connect using standalone function (no guard held across await)
    let pool = crate::supabase_client::supabase_connect_pool(&connection_string).await?;
    // Store pool in client
    state.supabase_client.read().set_pool(pool);
    Ok(())
}

/// Check Supabase connection
#[tauri::command]
pub async fn check_supabase(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let client = state.supabase_client.read();
    Ok(client.is_connected())
}

/// Sync an activity to Supabase
#[tauri::command]
pub async fn sync_activity_to_supabase(
    activity: Activity,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let pool = state.supabase_client.read().get_pool()
        .ok_or("Supabase not connected")?;
    crate::supabase_client::supabase_insert_activity(&pool, &activity).await
}

/// Query activities by time range
#[tauri::command]
pub async fn query_activities(
    start_iso: String,
    end_iso: String,
    state: State<'_, AppState>,
) -> Result<Vec<Activity>, String> {
    use chrono::{DateTime, Utc};
    
    let start: DateTime<Utc> = start_iso.parse()
        .map_err(|e| format!("Invalid start time: {}", e))?;
    let end: DateTime<Utc> = end_iso.parse()
        .map_err(|e| format!("Invalid end time: {}", e))?;
    
    let pool = state.supabase_client.read().get_pool()
        .ok_or("Supabase not connected")?;
    crate::supabase_client::supabase_query_activities(&pool, start, end).await
}

/// Configure Pinecone
#[tauri::command]
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
#[tauri::command]
pub async fn check_pinecone(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let client = state.pinecone_client.read();
    Ok(client.is_configured())
}

/// Upsert activity to Pinecone
#[tauri::command]
pub async fn upsert_to_pinecone(
    id: String,
    text: String,
    metadata: ActivityMetadata,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = state.pinecone_client.read().get_config()
        .ok_or("Pinecone not configured")?;
    crate::pinecone_client::pinecone_upsert(&config, &id, &text, &metadata).await
}

/// Semantic search in Pinecone
#[tauri::command]
pub async fn semantic_search(
    query: String,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<VectorMatch>, String> {
    let k = top_k.unwrap_or(10);
    let config = state.pinecone_client.read().get_config()
        .ok_or("Pinecone not configured")?;
    
    crate::pinecone_client::pinecone_search(&config, &query, k).await
}

/// Get Pinecone index stats
#[tauri::command]
pub async fn get_pinecone_stats(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let config = state.pinecone_client.read().get_config()
        .ok_or("Pinecone not configured")?;
    
    crate::pinecone_client::pinecone_stats(&config).await
}

// ============================================
// Capture Mode Settings Commands
// ============================================

/// Set capture microphone toggle
#[tauri::command]
pub async fn set_capture_microphone(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_capture_microphone(enabled).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set capture system audio toggle
#[tauri::command]
pub async fn set_capture_system_audio(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_capture_system_audio(enabled).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set capture screen toggle
#[tauri::command]
pub async fn set_capture_screen(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_capture_screen(enabled).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set always-on capture toggle
#[tauri::command]
pub async fn set_always_on_capture(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_always_on_capture(enabled).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set queue frames for VLM toggle
#[tauri::command]
pub async fn set_queue_frames_for_vlm(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_queue_frames_for_vlm(enabled).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

/// Set frame capture interval (ms)
#[tauri::command]
pub async fn set_frame_capture_interval(
    interval_ms: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.settings.set_frame_capture_interval(interval_ms).await
        .map_err(|e| format!("Failed to save setting: {}", e))
}

// ============================================
// Knowledge Base Configuration Commands
// ============================================

/// Configure all knowledge base settings at once
#[tauri::command]
pub async fn configure_knowledge_base(
    supabase_connection: Option<String>,
    pinecone_api_key: Option<String>,
    pinecone_index_host: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Save settings
    if let Some(ref conn) = supabase_connection {
        state.settings.set_supabase_connection(conn).await
            .map_err(|e| format!("Failed to save Supabase setting: {}", e))?;
        // Connect to Supabase
        state.supabase_client.read().set_connection_string(conn.clone());
    }
    
    if let (Some(ref key), Some(ref host)) = (&pinecone_api_key, &pinecone_index_host) {
        state.settings.set_pinecone_api_key(key).await
            .map_err(|e| format!("Failed to save Pinecone API key: {}", e))?;
        state.settings.set_pinecone_index_host(host).await
            .map_err(|e| format!("Failed to save Pinecone host: {}", e))?;
        // Configure Pinecone client
        state.pinecone_client.read().configure(key.clone(), host.clone(), None);
    }
    
    Ok(())
}

/// Get all capture settings
#[tauri::command]
pub async fn get_capture_settings(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let settings = state.settings.get_all().await
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

use crate::database::{FrameQueueItem, ActivityLogEntry};

/// Result of VLM analysis batch
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisResult {
    pub frames_processed: usize,
    pub activities_created: usize,
    pub errors: Vec<String>,
}

/// Analyze pending frames with VLM
#[tauri::command]
pub async fn analyze_pending_frames(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<AnalysisResult, String> {
    let limit = limit.unwrap_or(10);
    
    // Get VLM config (guard released before await)
    let (base_url, model) = state.vlm_client.read().get_url_and_model();
    
    // Check if VLM is available
    if !crate::vlm_client::vlm_is_available(&base_url).await {
        return Err("Ollama VLM is not available. Please start Ollama first.".to_string());
    }
    
    // Get pending frames
    let pending = state.database.get_pending_frames(limit).await
        .map_err(|e| format!("Failed to get pending frames: {}", e))?;
    
    if pending.is_empty() {
        return Ok(AnalysisResult {
            frames_processed: 0,
            activities_created: 0,
            errors: vec![],
        });
    }
    
    let mut frames_processed = 0;
    let mut activities_created = 0;
    let mut errors = Vec::new();
    
    for frame in pending {
        // Analyze frame with VLM (standalone function - no guard held)
        match crate::vlm_client::vlm_analyze_frame(&base_url, &model, &frame.frame_path).await {
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
                    Ok(_id) => {
                        activities_created += 1;
                        // Mark frame as analyzed
                        let _ = state.database.mark_frame_analyzed(frame.id).await;
                    }
                    Err(e) => {
                        errors.push(format!("Failed to store activity: {}", e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!("VLM analysis failed for {}: {}", frame.frame_path, e));
            }
        }
    }
    
    log::info!("🔍 VLM Analysis: {} frames processed, {} activities created", 
        frames_processed, activities_created);
    
    Ok(AnalysisResult {
        frames_processed,
        activities_created,
        errors,
    })
}

/// Get pending frame count
#[tauri::command]
pub async fn get_pending_frame_count(
    state: State<'_, AppState>,
) -> Result<i64, String> {
    state.database.count_unsynced_frames().await
        .map_err(|e| format!("Failed to count frames: {}", e))
}

/// Get activity stats for today
#[tauri::command]
pub async fn get_activity_stats(
    date: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let date = date.unwrap_or_else(|| {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    });
    
    state.database.get_activity_stats(&date).await
        .map_err(|e| format!("Failed to get stats: {}", e))
}

/// Get unsynced activities
#[tauri::command]
pub async fn get_unsynced_activities(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<ActivityLogEntry>, String> {
    state.database.get_unsynced_activities(limit.unwrap_or(50)).await
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
#[tauri::command]
pub async fn sync_to_cloud(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<SyncResult, String> {
    let limit = limit.unwrap_or(20);
    
    // Get unsynced activities
    let activities = state.database.get_unsynced_activities(limit).await
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
    let pinecone_configured = pinecone_config.is_some();
    let supabase_pool = state.supabase_client.read().get_pool();
    let supabase_connected = supabase_pool.is_some();
    
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
            let text = format!("{} - {} - {}", 
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
            let _ = state.database.mark_activity_synced(
                activity_id,
                pinecone_id.as_deref(),
                supabase_id.as_deref(),
            ).await;
            activities_synced += 1;
        }
    }
    
    log::info!("☁️ Cloud Sync: {} activities synced ({} Pinecone, {} Supabase)", 
        activities_synced, pinecone_upserts, supabase_inserts);
    
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
    pub source: String,  // "local", "supabase", "pinecone"
    pub timestamp: Option<String>,
    pub app_name: Option<String>,
    pub category: Option<String>,
    pub summary: String,
    pub score: Option<f32>,
}

/// Search options
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchOptions {
    pub query: Option<String>,           // Semantic query for Pinecone
    pub start_date: Option<String>,      // ISO date for time range
    pub end_date: Option<String>,        // ISO date for time range
    pub category: Option<String>,        // Filter by category
    pub limit: Option<u32>,              // Max results
    pub sources: Option<Vec<String>>,    // ["local", "pinecone", "supabase"]
}

/// Combined search across local SQLite, Pinecone, and Supabase
#[tauri::command]
pub async fn search_knowledge_base(
    options: SearchOptions,
    state: State<'_, AppState>,
) -> Result<Vec<KBSearchResult>, String> {
    let mut results = Vec::new();
    let limit = options.limit.unwrap_or(20) as i32;
    let sources = options.sources.clone().unwrap_or_else(|| vec!["local".to_string()]);
    
    // Search local SQLite activity_log
    if sources.contains(&"local".to_string()) {
        let local_activities = state.database.get_activities_filtered(
            options.start_date.as_deref(),
            options.end_date.as_deref(),
            options.category.as_deref(),
            limit,
        ).await.unwrap_or_default();
        
        for activity in local_activities {
            // Filter by query if provided (simple text match)
            if let Some(ref query) = options.query {
                let query_lower = query.to_lowercase();
                let matches = activity.summary.to_lowercase().contains(&query_lower)
                    || activity.category.to_lowercase().contains(&query_lower)
                    || activity.focus_area.as_ref().map(|f| f.to_lowercase().contains(&query_lower)).unwrap_or(false);
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
                if let Ok(matches) = crate::pinecone_client::pinecone_search(&config, query, limit as u32).await {
                    for m in matches {
                        results.push(KBSearchResult {
                            id: m.id,
                            source: "pinecone".to_string(),
                            timestamp: m.metadata.as_ref().and_then(|m| m.get("timestamp").and_then(|v| v.as_str().map(|s| s.to_string()))),
                            app_name: m.metadata.as_ref().and_then(|m| m.get("app_name").and_then(|v| v.as_str().map(|s| s.to_string()))),
                            category: m.metadata.as_ref().and_then(|m| m.get("category").and_then(|v| v.as_str().map(|s| s.to_string()))),
                            summary: m.metadata.as_ref().and_then(|m| m.get("summary").and_then(|v| v.as_str().map(|s| s.to_string()))).unwrap_or_default(),
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
            if let (Ok(start), Ok(end)) = (start.parse::<DateTime<Utc>>(), end.parse::<DateTime<Utc>>()) {
                let pool = state.supabase_client.read().get_pool();
                if let Some(pool) = pool {
                    if let Ok(activities) = crate::supabase_client::supabase_query_activities(&pool, start, end).await {
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
    results.sort_by(|a, b| {
        match (&b.score, &a.score) {
            (Some(bs), Some(as_)) => bs.partial_cmp(as_).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.timestamp.cmp(&a.timestamp),
        }
    });
    
    // Limit results
    results.truncate(limit as usize);
    
    log::info!("🔍 Knowledge base search: {} results", results.len());
    Ok(results)
}

/// Quick semantic search (just Pinecone)
#[tauri::command]
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
#[tauri::command]
pub async fn get_local_activities(
    start_date: Option<String>,
    end_date: Option<String>,
    category: Option<String>,
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::ActivityLogEntry>, String> {
    state.database.get_activities_filtered(
        start_date.as_deref(),
        end_date.as_deref(),
        category.as_deref(),
        limit.unwrap_or(50),
    ).await.map_err(|e| format!("Failed to get activities: {}", e))
}

/// Clear cache - remove pending frames and temporary data
#[tauri::command]
pub async fn clear_cache(
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Clear frame queue
    state.database.clear_frame_queue().await
        .map_err(|e| format!("Failed to clear frame queue: {}", e))?;
    
    // Clear activity log (optional - could be configurable)
    state.database.clear_activity_log().await
        .map_err(|e| format!("Failed to clear activity log: {}", e))?;

    log::info!("Cache cleared successfully");
    Ok(())
}

/// Export all data as JSON
#[tauri::command]
pub async fn export_data(
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get all meetings
    let meetings = state.database.list_meetings(1000).await
        .map_err(|e| format!("Failed to get meetings: {}", e))?;;
    
    // Get transcripts for each meeting
    let mut export_data = serde_json::json!({
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "version": "1.0.0",
        "meetings": []
    });
    
    let meetings_array = export_data["meetings"].as_array_mut().unwrap();
    
    for meeting in meetings {
        let transcripts = state.database.get_transcripts(&meeting.id).await
            .unwrap_or_default();
        let frames = state.database.get_frames(&meeting.id, 1000).await
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
    let activities = state.database.get_activities_filtered(None, None, None, 1000).await
        .unwrap_or_default();
    export_data["activities"] = serde_json::to_value(activities)
        .unwrap_or(serde_json::json!([]));
    
    serde_json::to_string_pretty(&export_data)
        .map_err(|e| format!("Failed to serialize export data: {}", e))
}

// ============================================
// Prompt Management Commands
// ============================================

/// List all prompts, optionally filtered by category
#[tauri::command]
pub async fn list_prompts(
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state.prompt_manager.list_prompts(category.as_deref()).await
        .map_err(|e| format!("Failed to list prompts: {}", e))
}

/// Get a single prompt by ID
#[tauri::command]
pub async fn get_prompt(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state.prompt_manager.get_prompt(&id).await
        .map_err(|e| format!("Failed to get prompt: {}", e))
}

/// Create a new prompt
#[tauri::command]
pub async fn create_prompt(
    input: crate::prompt_manager::PromptCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::Prompt, String> {
    state.prompt_manager.create_prompt(input).await
        .map_err(|e| format!("Failed to create prompt: {}", e))
}

/// Update an existing prompt
#[tauri::command]
pub async fn update_prompt(
    id: String,
    updates: crate::prompt_manager::PromptUpdate,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state.prompt_manager.update_prompt(&id, updates).await
        .map_err(|e| format!("Failed to update prompt: {}", e))
}

/// Delete a prompt (only non-builtin prompts can be deleted)
#[tauri::command]
pub async fn delete_prompt(
    id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.prompt_manager.delete_prompt(&id).await
        .map_err(|e| format!("Failed to delete prompt: {}", e))
}

/// Duplicate a prompt with a new name
#[tauri::command]
pub async fn duplicate_prompt(
    id: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state.prompt_manager.duplicate_prompt(&id, &new_name).await
        .map_err(|e| format!("Failed to duplicate prompt: {}", e))
}

/// Export all custom prompts as JSON
#[tauri::command]
pub async fn export_prompts(
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.prompt_manager.export_prompts().await
        .map_err(|e| format!("Failed to export prompts: {}", e))
}

/// Import prompts from JSON
#[tauri::command]
pub async fn import_prompts(
    json: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state.prompt_manager.import_prompts(&json).await
        .map_err(|e| format!("Failed to import prompts: {}", e))
}

// ============================================
// Model Configuration Commands
// ============================================

/// List all model configurations
#[tauri::command]
pub async fn list_model_configs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    state.prompt_manager.list_model_configs().await
        .map_err(|e| format!("Failed to list model configs: {}", e))
}

/// Get a model config by ID
#[tauri::command]
pub async fn get_model_config(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ModelConfig>, String> {
    state.prompt_manager.get_model_config(&id).await
        .map_err(|e| format!("Failed to get model config: {}", e))
}

/// Create a new model configuration
#[tauri::command]
pub async fn create_model_config(
    input: crate::prompt_manager::ModelConfigCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::ModelConfig, String> {
    state.prompt_manager.create_model_config(input).await
        .map_err(|e| format!("Failed to create model config: {}", e))
}

/// Refresh model availability by checking Ollama
#[tauri::command]
pub async fn refresh_model_availability(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    // Get all models from Ollama
    let client = reqwest::Client::new();
    let ollama_models = client.get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|_| "Ollama not available".to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let model_names: Vec<String> = ollama_models.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter()
            .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
            .map(String::from)
            .collect())
        .unwrap_or_default();

    // Update availability for each configured model
    let configs = state.prompt_manager.list_model_configs().await
        .map_err(|e| format!("Failed to list configs: {}", e))?;

    for config in &configs {
        let is_available = model_names.iter().any(|n| n.starts_with(&config.name));
        let _ = state.prompt_manager.update_model_availability(&config.name, is_available).await;
    }

    // Return updated list
    state.prompt_manager.list_model_configs().await
        .map_err(|e| format!("Failed to refresh model configs: {}", e))
}

/// List available models from Ollama
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<serde_json::Value>, String> {
    let client = reqwest::Client::new();
    let response = client.get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|_| "Ollama not available".to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let models = response.get("models")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(models)
}

// ============================================
// Use Case Mapping Commands
// ============================================

/// List all use case mappings
#[tauri::command]
pub async fn list_use_cases(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::UseCase>, String> {
    state.prompt_manager.list_use_cases().await
        .map_err(|e| format!("Failed to list use cases: {}", e))
}

/// Get a specific use case with resolved prompt and model
#[tauri::command]
pub async fn get_resolved_use_case(
    use_case: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ResolvedUseCase>, String> {
    state.prompt_manager.get_resolved_use_case(&use_case).await
        .map_err(|e| format!("Failed to get use case: {}", e))
}

/// Update use case mapping
#[tauri::command]
pub async fn update_use_case_mapping(
    use_case: String,
    prompt_id: Option<String>,
    model_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::UseCase>, String> {
    state.prompt_manager.update_use_case_mapping(
        &use_case,
        prompt_id.as_deref(),
        model_id.as_deref(),
    ).await
        .map_err(|e| format!("Failed to update use case: {}", e))
}

/// Test a prompt with sample input
#[tauri::command]
pub async fn test_prompt(
    prompt_id: String,
    test_input: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get the prompt
    let prompt = state.prompt_manager.get_prompt(&prompt_id).await
        .map_err(|e| format!("Failed to get prompt: {}", e))?
        .ok_or_else(|| "Prompt not found".to_string())?;

    // Get model config if specified
    let model_name = if let Some(ref model_id) = prompt.model_id {
        state.prompt_manager.get_model_config(model_id).await
            .map_err(|e| format!("Failed to get model: {}", e))?
            .map(|m| m.name)
            .unwrap_or_else(|| "llama3.2".to_string())
    } else {
        "llama3.2".to_string()
    };

    // Call Ollama
    let client = reqwest::Client::new();
    let response = client.post("http://localhost:11434/api/generate")
        .json(&serde_json::json!({
            "model": model_name,
            "prompt": format!("{}\n\nUser: {}", prompt.system_prompt, test_input),
            "stream": false,
            "options": {
                "temperature": prompt.temperature,
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to call Ollama: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(response.get("response")
        .and_then(|r| r.as_str())
        .unwrap_or("(No response)")
        .to_string())
}

// ============================================================================
// Meeting Intelligence Commands
// ============================================================================

use crate::meeting_intel::{MeetingState, MeetingStateResolver, CalendarEvent};
use crate::catch_up_agent::{CatchUpCapsule, CatchUpAgent, TranscriptSegment, MeetingMetadata};
use crate::live_intel_agent::{LiveInsightEvent, LiveIntelAgent};

/// Get current meeting state (mode, timing, confidence)
#[tauri::command]
pub async fn get_meeting_state(
    state: State<'_, AppState>,
) -> Result<MeetingState, String> {
    let resolver = MeetingStateResolver::new();
    let now = chrono::Utc::now();
    
    // Get recording status to check if transcript is running
    let is_transcribing = {
        let engine = state.capture_engine.read();
        engine.get_status().is_recording
    };
    
    // TODO: Integrate with calendar API
    // For now, use empty calendar events
    let calendar_events: Vec<CalendarEvent> = Vec::new();
    
    // TODO: Get active window from system
    let active_window: Option<&str> = None;
    
    // TODO: Check if audio is active
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
#[tauri::command]
pub async fn generate_catch_up(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<CatchUpCapsule, String> {
    // Get transcripts for the meeting
    let transcripts = state.database.get_transcripts(&meeting_id).await
        .map_err(|e| format!("Failed to get transcripts: {}", e))?;
    
    if transcripts.is_empty() {
        return Ok(CatchUpCapsule::default());
    }
    
    // Convert to segments
    let segments: Vec<TranscriptSegment> = transcripts.iter().map(|t| {
        TranscriptSegment {
            id: t.id.to_string(),
            timestamp_ms: t.timestamp.timestamp_millis(),
            speaker: t.speaker.clone(),
            text: t.text.clone(),
        }
    }).collect();
    
    // Get meeting info
    let meeting = state.database.get_meeting(&meeting_id).await
        .map_err(|e| format!("Failed to get meeting: {}", e))?;
    
    let metadata = MeetingMetadata {
        title: meeting.as_ref().map(|m| m.title.clone()).unwrap_or_default(),
        description: None,
        attendees: Vec::new(), // TODO: Get from calendar
        scheduled_duration_min: None,
    };
    
    // Calculate minutes since start
    let meeting_start = meeting.as_ref()
        .map(|m| m.started_at)
        .unwrap_or_else(chrono::Utc::now);
    let duration = chrono::Utc::now().signed_duration_since(meeting_start);
    let minutes_since_start = duration.num_minutes() as i32;
    
    // Create agent and generate catch-up
    let ai_client = crate::ai_client::AIClient::new();
    let agent = CatchUpAgent::new(ai_client);
    
    agent.generate(&segments, &metadata, minutes_since_start, None).await
}

/// Get live insights stream for current recording
#[tauri::command]
pub async fn get_live_insights(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<LiveInsightEvent>, String> {
    // Get recent transcripts
    let transcripts = state.database.get_transcripts(&meeting_id).await
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
#[tauri::command]
pub async fn pin_insight(
    meeting_id: String,
    insight_type: String,
    insight_text: String,
    timestamp_ms: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Store pinned insight in database
    // TODO: Add pinned_insights table
    log::info!("Pinning insight for meeting {}: {} - {}", meeting_id, insight_type, insight_text);
    Ok(())
}

/// Mark a decision point explicitly
#[tauri::command]
pub async fn mark_decision(
    meeting_id: String,
    decision_text: String,
    context: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Store decision in database
    // TODO: Add decisions table
    log::info!("Marking decision for meeting {}: {}", meeting_id, decision_text);
    Ok(())
}

// ============================================================================
// Video Recording Commands
// ============================================================================

use crate::video_recorder::{VideoRecorder, RecordingSession, PinMoment};
use crate::frame_extractor::{FrameExtractor, ExtractedFrame};
use crate::chunk_manager::{ChunkManager, StorageStats, RetentionPolicy};

// Lazy static for video recorder (global instance)
use std::sync::OnceLock;
static VIDEO_RECORDER: OnceLock<parking_lot::RwLock<VideoRecorder>> = OnceLock::new();
static FRAME_EXTRACTOR: OnceLock<FrameExtractor> = OnceLock::new();
static CHUNK_MANAGER: OnceLock<ChunkManager> = OnceLock::new();

fn get_video_recorder() -> &'static parking_lot::RwLock<VideoRecorder> {
    VIDEO_RECORDER.get_or_init(|| {
        parking_lot::RwLock::new(VideoRecorder::default())
    })
}

fn get_frame_extractor() -> &'static FrameExtractor {
    FRAME_EXTRACTOR.get_or_init(FrameExtractor::default)
}

fn get_chunk_manager() -> &'static ChunkManager {
    CHUNK_MANAGER.get_or_init(ChunkManager::default)
}

/// Start video recording for a meeting
#[tauri::command]
pub async fn start_video_recording(
    meeting_id: String,
) -> Result<(), String> {
    let recorder = get_video_recorder();
    recorder.write().start(&meeting_id)
}

/// Stop video recording
#[tauri::command]
pub async fn stop_video_recording() -> Result<RecordingSession, String> {
    let recorder = get_video_recorder();
    recorder.write().stop()
}

/// Get current video recording status
#[tauri::command]
pub async fn get_video_recording_status() -> Result<Option<RecordingSession>, String> {
    let recorder = get_video_recorder();
    Ok(recorder.read().get_status())
}

/// Pin the current moment in recording
#[tauri::command]
pub async fn video_pin_moment(
    label: Option<String>,
) -> Result<PinMoment, String> {
    let recorder = get_video_recorder();
    recorder.read().pin_moment(label)
}

/// Extract a frame at a specific timestamp
#[tauri::command]
pub async fn extract_frame_at(
    meeting_id: String,
    chunk_number: u32,
    timestamp_secs: f64,
) -> Result<ExtractedFrame, String> {
    let chunk_manager = get_chunk_manager();
    let chunks = chunk_manager.get_chunks(&meeting_id)?;
    
    let chunk = chunks.iter()
        .find(|c| c.chunk_number == chunk_number)
        .ok_or_else(|| format!("Chunk {} not found", chunk_number))?;
    
    let extractor = get_frame_extractor();
    extractor.extract_at(&chunk.path, timestamp_secs, &meeting_id)
}

/// Extract thumbnail for timeline view
#[tauri::command]
pub async fn extract_thumbnail(
    meeting_id: String,
    chunk_number: u32,
    timestamp_secs: f64,
    size: Option<u32>,
) -> Result<String, String> {
    let chunk_manager = get_chunk_manager();
    let chunks = chunk_manager.get_chunks(&meeting_id)?;
    
    let chunk = chunks.iter()
        .find(|c| c.chunk_number == chunk_number)
        .ok_or_else(|| format!("Chunk {} not found", chunk_number))?;
    
    let extractor = get_frame_extractor();
    let thumb_path = extractor.extract_thumbnail(
        &chunk.path, 
        timestamp_secs, 
        &meeting_id, 
        size.unwrap_or(200)
    )?;
    
    Ok(thumb_path.to_string_lossy().to_string())
}

/// Get storage statistics
#[tauri::command]
pub async fn get_storage_stats() -> Result<StorageStats, String> {
    let manager = get_chunk_manager();
    manager.get_stats()
}

/// Apply retention policies
#[tauri::command]
pub async fn apply_retention() -> Result<(u32, u64), String> {
    let manager = get_chunk_manager();
    manager.apply_retention()
}

/// Delete a meeting's video storage
#[tauri::command]
pub async fn delete_video_storage(
    meeting_id: String,
) -> Result<u64, String> {
    let manager = get_chunk_manager();
    manager.delete_meeting(&meeting_id)
}
