// noFriction Meetings - AI & Knowledge Base Commands
// Ollama, TheBrain, Supabase, Pinecone, conversation storage, RAG

use crate::AppState;
use tauri::State;

// ============================================
// Intelligence / Meeting State Commands
// ============================================


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

    // Also configure the AI client so chat/summarize/action-items use the remote endpoint
    state.ai_client.read().set_base_url(url.clone());

    log::info!("✅ VLM + AI API URL updated to: {}", url);
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

