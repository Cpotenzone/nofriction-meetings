// noFriction Meetings - Intelligence & Calendar Commands
// Calendar intelligence, data chatbot, and meeting reports (v3.1.0+)

use crate::AppState;
use super::{search_knowledge_base, SearchOptions};
use tauri::State;


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

/// Get attendees for a meeting (from calendar integration)
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_attendees(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::MeetingAttendee>, String> {
    state
        .database
        .get_meeting_attendees(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get attendees: {}", e))
}

/// Update a meeting's title
#[tauri::command(rename_all = "camelCase")]
pub async fn update_meeting_title(
    meeting_id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .database
        .update_meeting_title(&meeting_id, &title)
        .await
        .map_err(|e| format!("Failed to update title: {}", e))
}

/// AI-powered attendee lookup — generates person + company briefings
#[tauri::command(rename_all = "camelCase")]
pub async fn lookup_attendees(
    event_title: String,
    attendee_emails: Vec<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    use crate::attendee_intel;

    let ai_client = state.ai_client.read().clone();
    let package =
        attendee_intel::generate_meeting_intel(&ai_client, &event_title, &attendee_emails)
            .await
            .map_err(|e| format!("Intel generation failed: {}", e))?;

    Ok(serde_json::json!({
        "event_title": package.event_title,
        "attendees": package.attendees,
        "companies": package.companies,
        "meeting_prep": package.meeting_prep,
    }))
}

/// Match a recording to an overlapping calendar event
#[tauri::command(rename_all = "camelCase")]
pub async fn match_recording_to_calendar(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // Get the meeting
    let meeting = state
        .database
        .get_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?
        .ok_or_else(|| format!("Meeting not found: {}", meeting_id))?;

    // Fetch calendar events
    let events = {
        let client = state.calendar_client.read();
        client
            .fetch_events()
            .map_err(|e| format!("Calendar error: {}", e))?
    };

    let meeting_start = meeting.started_at;
    let meeting_end = meeting.ended_at.unwrap_or(chrono::Utc::now());

    // Find overlapping event
    for event in &events {
        if event.is_all_day {
            continue;
        }
        // Check if time ranges overlap
        if event.start_time < meeting_end && event.end_time > meeting_start {
            let attendee_names: Vec<String> = event
                .attendees
                .iter()
                .map(|e| crate::attendee_intel::extract_name_from_email(e))
                .collect();

            return Ok(serde_json::json!({
                "meeting_id": meeting_id,
                "event_id": event.event_id,
                "event_title": event.title,
                "attendee_count": event.attendees.len(),
                "attendee_names": attendee_names,
                "attendee_emails": event.attendees,
                "start_time": event.start_time.to_rfc3339(),
                "end_time": event.end_time.to_rfc3339(),
            }));
        }
    }

    // No match
    Ok(serde_json::json!(null))
}

// ─── Data Chatbot: RAG-powered conversational interface ─────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatHistoryMessage {
    pub role: String,
    pub content: String,
}

/// RAG chatbot — search knowledge base, inject context, generate AI answer
#[tauri::command(rename_all = "camelCase")]
pub async fn chat_with_data(
    message: String,
    history: Vec<ChatHistoryMessage>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    use crate::ai_client::{AIPreset, ChatMessage};

    // Step 1: Retrieve — search knowledge base for relevant context
    let search_options = SearchOptions {
        query: Some(message.clone()),
        start_date: None,
        end_date: None,
        category: None,
        limit: Some(10),
        sources: Some(vec!["local".to_string(), "pinecone".to_string()]),
    };

    let search_results = search_knowledge_base(search_options, state.clone())
        .await
        .unwrap_or_default();

    // Step 2: Augment — build context string from search results
    let mut context_parts = Vec::new();
    let mut source_citations = Vec::new();

    for (i, result) in search_results.iter().enumerate().take(8) {
        let timestamp = result.timestamp.as_deref().unwrap_or("unknown time");
        let source_label = match result.source.as_str() {
            "pinecone" => "Vector DB",
            "local" => "Local DB",
            "supabase" => "Cloud DB",
            _ => &result.source,
        };
        let app = result.app_name.as_deref().unwrap_or("");

        context_parts.push(format!(
            "--- Source {} ({}, {}) ---\n{}\n{}",
            i + 1,
            source_label,
            timestamp,
            if app.is_empty() {
                String::new()
            } else {
                format!("[App: {}] ", app)
            },
            result.summary
        ));

        source_citations.push(serde_json::json!({
            "id": result.id,
            "summary": result.summary.chars().take(150).collect::<String>(),
            "source": result.source,
            "score": result.score,
            "timestamp": result.timestamp,
            "app_name": result.app_name,
        }));
    }

    let context = if context_parts.is_empty() {
        "No relevant meeting data found in the knowledge base for this query.".to_string()
    } else {
        context_parts.join("\n\n")
    };

    // Step 3: Generate — resolve persona-specific Genie prompt from PromptManager
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .unwrap_or_else(|_| "personal".to_string());

    let genie_prompt_name = format!("genie_system_{}", active_theme);
    let preset = match state
        .prompt_manager
        .get_prompt_by_name(&genie_prompt_name, Some(&active_theme))
        .await
        .ok()
        .flatten()
    {
        Some(db_prompt) => AIPreset::from_prompt(&db_prompt),
        None => AIPreset::qa(), // Fallback to hardcoded preset
    };
    let mut messages: Vec<ChatMessage> = history
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    // Add the current user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: message,
    });

    let ai_client = state.ai_client.read().clone();
    let answer = ai_client
        .chat(&preset, messages, Some(&context))
        .await
        .unwrap_or_else(|e| format!("I wasn't able to process your question: {}", e));

    Ok(serde_json::json!({
        "answer": answer,
        "sources": source_citations,
        "context_count": search_results.len(),
    }))
}

// ============================================
// Meeting Report Prompt Commands
// ============================================

/// Get the current meeting report prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_report_prompt(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to load settings: {}", e))?;
    Ok(settings.meeting_report_prompt)
}

/// Set the meeting report prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn set_meeting_report_prompt(
    state: State<'_, AppState>,
    prompt: String,
) -> Result<(), String> {
    state
        .settings
        .set("meeting_report_prompt", &prompt)
        .await
        .map_err(|e| format!("Failed to save prompt: {}", e))?;
    log::info!("📝 Meeting report prompt updated ({} chars)", prompt.len());
    Ok(())
}

/// Manually generate a meeting report for a specific meeting
#[tauri::command(rename_all = "camelCase")]
pub async fn generate_meeting_report(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<serde_json::Value, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to load settings: {}", e))?;

    // Resolve persona-specific report prompt from PromptManager, fallback to settings
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .unwrap_or_else(|_| "personal".to_string());

    let report_prompt_name = format!("meeting_report_{}", active_theme);
    let prompt_text = match state
        .prompt_manager
        .get_prompt_by_name(&report_prompt_name, Some(&active_theme))
        .await
        .ok()
        .flatten()
    {
        Some(db_prompt) => db_prompt.system_prompt,
        None => settings.meeting_report_prompt.clone(),
    };

    let ai_client = { state.ai_client.read().clone() };
    let generator = crate::meeting_notes::MeetingNotesGenerator::new(ai_client);

    let notes = generator
        .generate_notes_with_prompt(&meeting_id, &state.database, &prompt_text)
        .await?;

    Ok(serde_json::json!({
        "summary": notes.summary,
        "key_topics": notes.key_topics,
        "decisions": notes.decisions,
        "action_items": notes.action_items,
        "participants": notes.participants,
    }))
}
