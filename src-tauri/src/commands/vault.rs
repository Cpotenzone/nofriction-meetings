// noFriction Meetings - Vault Commands
// Obsidian vault integration commands (v3.0.0)
//
// Export to Obsidian is part of noFriction Pro (docs/PRO.md): every command
// that writes to the vault, or picks it, goes through
// `require_pro_feature(ProFeature::Obsidian)` (enforced in the `mas` build
// only). Reading the user's own vault files stays open.

use crate::catch_up_agent::TranscriptSegment;
use crate::entitlement::{require_pro_feature, ProFeature};
use crate::live_intel_agent::{LiveIntelAgent, LiveInsightEvent};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

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
    require_pro_feature(ProFeature::Obsidian).await?;
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
    require_pro_feature(ProFeature::Obsidian).await?;
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

    // Gap Fix: Get accessibility snapshots for screen activity
    let text_snapshots = database
        .get_text_snapshots_by_meeting(&meeting_id)
        .await
        .map_err(|e| format!("Failed to get text snapshots: {}", e))?;

    // Build screen activity markdown from accessibility snapshots
    let mut screen_activity_md = String::new();
    for snapshot in &text_snapshots {
        let time_str = if snapshot.ts.len() >= 19 {
            &snapshot.ts[11..19]
        } else {
            &snapshot.ts
        };
        let app = snapshot.app_name.as_deref().unwrap_or("Unknown");
        let window = snapshot.window_title.as_deref().unwrap_or("");
        let text_preview: String = snapshot.text.chars().take(200).collect();
        screen_activity_md.push_str(&format!(
            "**[{}] {}** — {}\n> {}\n\n",
            time_str, app, window, text_preview
        ));
    }

    let mut transcript_tuples: Vec<(String, Option<String>, String)> = transcripts
        .iter()
        .map(|t| (crate::redaction::render_plain(&t.text), t.speaker.clone(), t.timestamp.to_rfc3339()))
        .collect();
    // Stricken screens render as a marker at the moment they covered
    let strikes = crate::redaction::list_strikes(database.pool(), &meeting_id)
        .await
        .map_err(|e| format!("Failed to get redactions: {}", e))?;
    for (ts, text) in crate::redaction::screen_strike_lines(&strikes) {
        let ts = chrono::DateTime::parse_from_rfc3339(&ts)
            .map(|d| d.with_timezone(&chrono::Utc).to_rfc3339())
            .unwrap_or(ts);
        transcript_tuples.push((text, None, ts));
    }
    transcript_tuples.sort_by_key(|(_, _, ts)| {
        chrono::DateTime::parse_from_rfc3339(ts).map(|d| d.timestamp_micros()).unwrap_or(0)
    });

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
    let mut ai_key_insights = Vec::new();
    let mut ai_deadlines = Vec::new();

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
            LiveInsightEvent::KeyInsight {
                text, importance, ..
            } => {
                ai_key_insights.push(format!("- 💡 {} *(importance: {:.0}/5)*", text, importance));
            }
            LiveInsightEvent::Deadline {
                text,
                deadline_ref,
                owner,
                ..
            } => {
                if let Some(o) = owner {
                    ai_deadlines.push(format!("- 📅 {} — {} *(owner: {})*", text, deadline_ref, o));
                } else {
                    ai_deadlines.push(format!("- 📅 {} — {}", text, deadline_ref));
                }
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
    if !ai_key_insights.is_empty() {
        intelligence_md.push_str("### Key Insights\n\n");
        intelligence_md.push_str(&ai_key_insights.join("\n"));
        intelligence_md.push_str("\n\n");
    }
    if !ai_deadlines.is_empty() {
        intelligence_md.push_str("### Deadlines\n\n");
        intelligence_md.push_str(&ai_deadlines.join("\n"));
        intelligence_md.push_str("\n\n");
    }

    // Append screen activity to the exported content
    let mut full_intelligence = intelligence_md.clone();
    if !screen_activity_md.is_empty() {
        full_intelligence.push_str("### Screen Activity\n\n");
        full_intelligence.push_str(&screen_activity_md);
    }

    let intelligence_final = if full_intelligence.is_empty() {
        None
    } else {
        Some(full_intelligence.as_str())
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
            intelligence_final,
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
    require_pro_feature(ProFeature::Obsidian).await?;
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
    require_pro_feature(ProFeature::Obsidian).await?;
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
    require_pro_feature(ProFeature::Obsidian).await?;
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

    // m6: keep access across launches. The folder came from the open panel,
    // so this launch may bookmark it; later launches resolve the bookmark.
    match crate::bookmarks::create(&vault_path) {
        Ok(b64) => {
            let _ = state.settings.set(VAULT_BOOKMARK_KEY, &b64).await;
        }
        Err(e) => {
            // The sandboxed build can't reopen the vault next launch without it
            let _ = state.settings.delete(VAULT_BOOKMARK_KEY).await;
            if cfg!(feature = "mas") {
                return Err(format!("{} — pick the folder again with Select Folder", e));
            }
            log::warn!("Vault bookmark not created ({}); using the plain path", e);
        }
    }

    Ok(())
}

/// Settings key holding the vault's security-scoped bookmark (base64).
pub const VAULT_BOOKMARK_KEY: &str = "obsidian_vault_bookmark";

/// Startup: resolve the vault bookmark and start accessing it; fall back to
/// the plain saved path (Developer ID build). Returns the path to use.
pub async fn restore_vault_access(settings: &crate::settings::SettingsManager) -> Option<String> {
    let plain = settings.get("obsidian_vault_path").await.ok().flatten();
    if let Ok(Some(b64)) = settings.get(VAULT_BOOKMARK_KEY).await {
        match crate::bookmarks::resolve_and_access(&b64) {
            Ok(r) => {
                let path = r.path.to_string_lossy().into_owned();
                if !r.accessing && cfg!(feature = "mas") {
                    log::warn!("Vault bookmark resolved but access was refused");
                }
                if r.stale || plain.as_deref() != Some(path.as_str()) {
                    if let Ok(fresh) = crate::bookmarks::create(&path) {
                        let _ = settings.set(VAULT_BOOKMARK_KEY, &fresh).await;
                    }
                    let _ = settings.set_vault_path(&path).await;
                }
                return Some(path);
            }
            Err(e) => log::warn!("Vault bookmark unusable ({}); falling back to saved path", e),
        }
    }
    plain
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

#[cfg(test)]
mod tests {
    use crate::entitlement::ProFeature;

    /// Every command that picks the vault or writes to it checks Pro first.
    #[test]
    fn vault_writes_are_pro_gated() {
        let src = include_str!("vault.rs");
        for name in [
            "create_vault_topic",
            "export_meeting_to_vault",
            "write_vault_note",
            "upload_to_vault",
            "set_vault_path",
        ] {
            let start = src.find(&format!("pub async fn {}(", name)).unwrap_or_else(|| panic!("{} missing", name));
            let body = &src[start..];
            let end = body[1..].find("\npub async fn ").map(|i| i + 1).unwrap_or(body.len());
            assert!(
                body[..end].contains("require_pro_feature(ProFeature::Obsidian).await?"),
                "{} must check Pro before touching the vault",
                name
            );
        }
    }

    #[test]
    fn auto_export_setting_needs_pro() {
        use crate::commands::setting_needs_pro;
        assert_eq!(setting_needs_pro("obsidian_auto_export", "true"), Some(ProFeature::Obsidian));
        // Turning it off, and every other setting, stays free
        assert_eq!(setting_needs_pro("obsidian_auto_export", "false"), None);
        assert_eq!(setting_needs_pro("auto_stop_enabled", "true"), None);
    }
}
