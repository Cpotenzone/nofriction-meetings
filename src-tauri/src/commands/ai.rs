// noFriction Meetings - AI & Knowledge Base Commands
// AI chat/summaries (via the active provider), vision, local conversation
// storage, and RAG over the local SQLite index (no server-side services)

use crate::AppState;
use tauri::State;

// ============================================
// Intelligence / Meeting State Commands
// ============================================


// ============================================
// AI Commands (active provider from Settings → AI Engine)
// ============================================

use crate::ai_client::{AIClient, AIPreset, ChatMessage};

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
// Knowledge Base Commands (VLM)
// ============================================

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

/// Free-form assistant chat on the active text provider. `model` is
/// accepted for compatibility and ignored (the model is chosen in Settings).
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_chat(
    message: String,
    model: Option<String>,
    _state: State<'_, AppState>,
) -> Result<String, String> {
    let _ = model;
    log::info!("🤖 Assistant chat: message_len={}", message.len());
    AIClient::new()
        .complete_with(
            Some("You are a concise, helpful assistant for a meeting-notes app."),
            &message,
            crate::ai_client::DEFAULT_MAX_TOKENS,
            0.5,
        )
        .await
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

/// Upper bound on retrieved context sent to the model (characters).
const RAG_CONTEXT_CHAR_BUDGET: usize = 6000;
/// Upper bound per context item (characters).
const RAG_ITEM_CHAR_LIMIT: usize = 600;

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// Retrieve chat context from the local SQLite index only: meeting
/// transcripts via FTS5 (bm25-ranked, with snippets, meeting title and time),
/// then analyzed activities / captured screen text via plain LIKE matching.
/// Capped at `top_k` transcript hits, `top_k / 2` activity hits and
/// [`RAG_CONTEXT_CHAR_BUDGET`] characters in total.
async fn retrieve_local_context(
    state: &State<'_, AppState>,
    message: &str,
    top_k: u32,
) -> Vec<ContextItem> {
    let top_k = top_k.clamp(1, 20) as i64;
    let mut items = Vec::new();

    if let Some(q) = crate::database::fts_or_query(message) {
        match state.database.search_transcript_context(&q, top_k).await {
            Ok(hits) => {
                for hit in hits {
                    let speaker = hit
                        .speaker
                        .as_deref()
                        .map(|s| format!("{}: ", s))
                        .unwrap_or_default();
                    items.push(ContextItem {
                        id: format!("transcript-{}-{}", hit.meeting_id, hit.transcript_id),
                        // bm25 is negative-is-better; map to a rough 0-1 score
                        score: (1.0 / (1.0 + hit.relevance.abs())) as f32,
                        summary: clip(
                            &format!(
                                "Meeting \"{}\" (started {}): {}{}",
                                hit.meeting_title, hit.meeting_started_at, speaker, hit.snippet
                            ),
                            RAG_ITEM_CHAR_LIMIT,
                        ),
                        timestamp: Some(hit.timestamp),
                        category: Some("transcript".to_string()),
                    });
                }
            }
            Err(e) => log::warn!("Local transcript search failed: {}", e),
        }
    }

    let terms = crate::database::search_terms(message);
    let activity_limit = (top_k / 2).max(1);
    match state.database.search_activity_text(&terms, activity_limit).await {
        Ok(rows) => {
            for (id, ts, label, text) in rows.into_iter().take(activity_limit as usize) {
                items.push(ContextItem {
                    id,
                    score: 0.0,
                    summary: clip(&text, RAG_ITEM_CHAR_LIMIT),
                    timestamp: Some(ts),
                    category: Some(label),
                });
            }
        }
        Err(e) => log::warn!("Local activity search failed: {}", e),
    }

    // Enforce the total context budget (items are already in priority order)
    let mut used = 0;
    items
        .into_iter()
        .take_while(|item| {
            used += item.summary.len();
            used <= RAG_CONTEXT_CHAR_BUDGET
        })
        .collect()
}

/// Assistant chat with RAG - retrieves relevant context before answering
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_rag_chat(
    message: String,
    model: Option<String>,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<RagChatResponse, String> {
    let _ = model;
    let model = crate::ai::config::snapshot()
        .text
        .map(|s| format!("{}/{}", s.provider, s.model))
        .unwrap_or_default();

    let search_count = top_k.unwrap_or(5);
    log::info!(
        "🧠 RAG Chat: searching {} items, model={}",
        search_count,
        model
    );

    // Step 1: Retrieve relevant context from the local SQLite index
    let context_items = retrieve_local_context(&state, &message, search_count).await;

    // Step 2: Build augmented prompt with context
    let augmented_prompt = if !context_items.is_empty() {
        let context_text = context_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                format!(
                    "[{}] {} ({})\n   {}",
                    i + 1,
                    item.timestamp.as_deref().unwrap_or("Unknown time"),
                    item.category.as_deref().unwrap_or("note"),
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

    // Step 3: Call the active text provider with the augmented prompt
    let response = AIClient::new()
        .complete_with(None, &augmented_prompt, crate::ai_client::DEFAULT_MAX_TOKENS, 0.5)
        .await?;

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

/// Store a conversation in the local database
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

    if let Err(e) = state
        .database
        .add_assistant_conversation(
            &id,
            &timestamp,
            &user_query,
            &assistant_response,
            &model_used,
            &context_refs,
        )
        .await
    {
        log::warn!("Failed to store conversation locally: {}", e);
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

    let rows = state
        .database
        .list_assistant_conversations(max_count as i64)
        .await
        .map_err(|e| format!("Failed to load conversation history: {}", e))?;

    Ok(rows
        .into_iter()
        .map(
            |(id, timestamp, user_query, assistant_response, model_used, context_refs)| {
                ConversationRecord {
                    id,
                    timestamp,
                    user_query,
                    assistant_response,
                    model_used,
                    context_refs,
                }
            },
        )
        .collect())
}

/// Combined RAG chat with automatic conversation storage
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_rag_chat_with_memory(
    message: String,
    model: Option<String>,
    top_k: Option<u32>,
    state: State<'_, AppState>,
) -> Result<RagChatResponse, String> {
    // First do the RAG chat
    let response = assistant_rag_chat(message.clone(), model, top_k, state.clone()).await?;
    let model = response.model.clone();

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
