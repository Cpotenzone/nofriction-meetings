// noFriction Meetings - Admin Commands
// Tauri commands for the Management Suite
//
// Features:
// - Recordings library with storage enumeration
// - Safe deletion with preview and audit
// - Learned data editing with versioning
// - System health and tools management

use crate::audit_log::{AuditEntry, AuditLog};
use crate::data_editor::{DataEditor, DataVersion, EditResult, LearnedDataItem};
use crate::storage_manager::{DeletePreview, DeleteResult, StorageManager};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

// ═══════════════════════════════════════════════════════════════════════════
// Storage/Recordings Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Recording with storage info for library display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingWithStorage {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub frames_count: u32,
    pub frames_bytes: u64,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    pub total_bytes: u64,
    pub total_bytes_formatted: String,
}

/// List recordings with storage information
#[tauri::command]
pub async fn list_recordings_with_storage(
    app: AppHandle,
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
    search: Option<String>,
) -> Result<Vec<RecordingWithStorage>, String> {
    // Get app data directory
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let storage_manager = StorageManager::new(app_data_dir);

    // Get meetings from database
    let all_meetings = state
        .database
        .list_meetings(limit as i32)
        .await
        .map_err(|e| format!("Failed to list meetings: {}", e))?;

    // Filter by search if provided
    let meetings: Vec<_> = if let Some(query) = search {
        let query_lower = query.to_lowercase();
        all_meetings
            .into_iter()
            .filter(|m| m.title.to_lowercase().contains(&query_lower))
            .skip(offset as usize)
            .collect()
    } else {
        all_meetings.into_iter().skip(offset as usize).collect()
    };

    // Enrich with storage info
    let mut results = Vec::with_capacity(meetings.len());

    for meeting in meetings {
        let storage = storage_manager.get_meeting_storage(&meeting.id).await;

        results.push(RecordingWithStorage {
            id: meeting.id.clone(),
            title: meeting.title,
            started_at: meeting.started_at.to_rfc3339(),
            ended_at: meeting.ended_at.map(|dt| dt.to_rfc3339()),
            duration_seconds: meeting.duration_seconds,
            frames_count: storage.frames_count,
            frames_bytes: storage.frames_bytes,
            video_bytes: storage.video_bytes,
            audio_bytes: storage.audio_bytes,
            total_bytes: storage.total_bytes,
            total_bytes_formatted: StorageManager::format_bytes(storage.total_bytes),
        });
    }

    Ok(results)
}

/// Get total storage stats
#[tauri::command]
pub async fn get_admin_storage_stats(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let storage_manager = StorageManager::new(app_data_dir);

    // Get all meetings
    let meetings = state
        .database
        .list_meetings(10000)
        .await
        .map_err(|e| format!("Failed to get meetings: {}", e))?;

    let mut total_frames_bytes = 0u64;
    let mut total_video_bytes = 0u64;
    let mut total_audio_bytes = 0u64;
    let mut total_frames_count = 0u32;

    for meeting in &meetings {
        let storage = storage_manager.get_meeting_storage(&meeting.id).await;
        total_frames_bytes += storage.frames_bytes;
        total_video_bytes += storage.video_bytes;
        total_audio_bytes += storage.audio_bytes;
        total_frames_count += storage.frames_count;
    }

    let total_bytes = total_frames_bytes + total_video_bytes + total_audio_bytes;

    Ok(serde_json::json!({
        "meetings_count": meetings.len(),
        "frames_count": total_frames_count,
        "frames_bytes": total_frames_bytes,
        "frames_bytes_formatted": StorageManager::format_bytes(total_frames_bytes),
        "video_bytes": total_video_bytes,
        "video_bytes_formatted": StorageManager::format_bytes(total_video_bytes),
        "audio_bytes": total_audio_bytes,
        "audio_bytes_formatted": StorageManager::format_bytes(total_audio_bytes),
        "total_bytes": total_bytes,
        "total_bytes_formatted": StorageManager::format_bytes(total_bytes),
    }))
}

/// Preview deletion - shows what will be deleted without actually deleting
#[tauri::command]
pub async fn preview_delete_recordings(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_ids: Vec<String>,
) -> Result<DeletePreview, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let storage_manager = StorageManager::new(app_data_dir);
    let mut preview = storage_manager.preview_delete(&meeting_ids).await;

    // Check if recording is active
    let recording_status = state.capture_engine.read().get_status();
    if recording_status.is_recording {
        preview.safe_to_delete = false;
        preview
            .warnings
            .push("Recording is currently active".to_string());
    }

    Ok(preview)
}

/// Delete recordings - removes filesystem artifacts and optionally DB records
#[tauri::command]
pub async fn delete_recordings(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_ids: Vec<String>,
    delete_db_records: bool,
) -> Result<DeleteResult, String> {
    // Check if recording is active
    let recording_status = state.capture_engine.read().get_status();
    if recording_status.is_recording {
        return Err("Cannot delete while recording is active".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let storage_manager = StorageManager::new(app_data_dir);

    // Get preview first for audit log
    let preview = storage_manager.preview_delete(&meeting_ids).await;

    // Delete filesystem artifacts
    let mut result = storage_manager.delete_meetings(&meeting_ids).await;

    // Log to audit
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    for meeting_id in &meeting_ids {
        let bytes = preview
            .breakdown
            .get(meeting_id)
            .map(|info| info.total_bytes)
            .unwrap_or(0);

        let details = serde_json::json!({
            "delete_db_records": delete_db_records,
            "files_deleted": preview.file_count,
        });

        if let Err(e) = audit
            .log_deletion("meeting", meeting_id, bytes, Some(details))
            .await
        {
            log::warn!("Failed to audit log deletion: {}", e);
        }
    }

    // Optionally delete database records
    if delete_db_records {
        for meeting_id in &meeting_ids {
            if let Err(e) = state.database.delete_meeting(meeting_id).await {
                result
                    .errors
                    .push(format!("DB delete {}: {}", meeting_id, e));
            }
        }
    }

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════════════════
// Audit Log Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Get audit log entries
#[tauri::command]
pub async fn get_audit_log(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
    action_filter: Option<String>,
) -> Result<Vec<AuditEntry>, String> {
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    audit
        .get_entries(limit, offset, action_filter.as_deref())
        .await
}

/// Get audit log count
#[tauri::command]
pub async fn get_audit_log_count(
    state: State<'_, AppState>,
    action_filter: Option<String>,
) -> Result<u32, String> {
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    audit.count_entries(action_filter.as_deref()).await
}

// ═══════════════════════════════════════════════════════════════════════════
// System Health Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Service health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub name: String,
    pub status: String, // "healthy", "degraded", "error", "unknown"
    pub message: Option<String>,
    pub last_check: String,
}

/// Get system health status for all services
#[tauri::command]
pub async fn get_system_health(state: State<'_, AppState>) -> Result<Vec<ServiceHealth>, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut services = Vec::new();

    // Database health
    services.push(ServiceHealth {
        name: "Database".to_string(),
        status: "healthy".to_string(),
        message: Some("SQLite connected".to_string()),
        last_check: now.clone(),
    });

    // Recording engine
    let recording_status = state.capture_engine.read().get_status();
    services.push(ServiceHealth {
        name: "Recording Engine".to_string(),
        status: "healthy".to_string(),
        message: Some(if recording_status.is_recording {
            format!("Recording ({} frames)", recording_status.video_frames)
        } else {
            "Idle".to_string()
        }),
        last_check: now.clone(),
    });

    // Metrics collector
    if let Some(metrics) = state.metrics_collector.snapshot() {
        services.push(ServiceHealth {
            name: "Metrics Collector".to_string(),
            status: "healthy".to_string(),
            message: Some(format!(
                "{} frames, {} states",
                metrics.frames_in, metrics.states_out
            )),
            last_check: now.clone(),
        });
    } else {
        services.push(ServiceHealth {
            name: "Metrics Collector".to_string(),
            status: "unknown".to_string(),
            message: Some("No active session".to_string()),
            last_check: now.clone(),
        });
    }

    // Ingest queue
    let queue_result = state.ingest_queue.lock().get_stats();
    match queue_result {
        Ok((pending, processing)) => {
            services.push(ServiceHealth {
                name: "Ingest Queue".to_string(),
                status: "healthy".to_string(),
                message: Some(format!("{} pending, {} processing", pending, processing)),
                last_check: now.clone(),
            });
        }
        Err(e) => {
            services.push(ServiceHealth {
                name: "Ingest Queue".to_string(),
                status: "error".to_string(),
                message: Some(format!("Failed to get stats: {}", e)),
                last_check: now.clone(),
            });
        }
    }

    Ok(services)
}

/// Get ingest queue statistics
#[tauri::command]
pub async fn get_admin_queue_stats(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let (pending, processing) = state
        .ingest_queue
        .lock()
        .get_stats()
        .map_err(|e| format!("Failed to get queue stats: {}", e))?;

    Ok(serde_json::json!({
        "pending": pending,
        "processing": processing,
        "completed": 0,  // Not tracked by current API
        "failed": 0,     // Not tracked by current API
        "total_bytes": 0,
        "total_bytes_formatted": "0 B",
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature Flags Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Get all feature flags
#[tauri::command]
pub async fn get_feature_flags(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))?;

    Ok(serde_json::json!({
        "admin_console_enabled": true,  // Always true if we got here
        "dedup_enabled": settings.dedup_enabled.unwrap_or(true),
        "vlm_auto_process": settings.vlm_auto_process,
        "enable_ingest": settings.enable_ingest.unwrap_or(false),
        "queue_frames_for_vlm": settings.queue_frames_for_vlm,
    }))
}

/// Set a feature flag
#[tauri::command]
pub async fn set_feature_flag(
    state: State<'_, AppState>,
    flag: String,
    value: bool,
) -> Result<bool, String> {
    // Update the appropriate flag using dedicated setters
    match flag.as_str() {
        "dedup_enabled" => {
            state
                .settings
                .set("dedup_enabled", &value.to_string())
                .await
                .map_err(|e| format!("Failed to set dedup_enabled: {}", e))?;
        }
        "vlm_auto_process" => {
            state
                .settings
                .set("vlm_auto_process", &value.to_string())
                .await
                .map_err(|e| format!("Failed to set vlm_auto_process: {}", e))?;
        }
        "enable_ingest" => {
            state
                .settings
                .set("enable_ingest", &value.to_string())
                .await
                .map_err(|e| format!("Failed to set enable_ingest: {}", e))?;
        }
        "queue_frames_for_vlm" => {
            state
                .settings
                .set_queue_frames_for_vlm(value)
                .await
                .map_err(|e| format!("Failed to set queue_frames_for_vlm: {}", e))?;
        }
        _ => return Err(format!("Unknown feature flag: {}", flag)),
    }

    // Audit log
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    let _ = audit
        .log_action(crate::audit_log::AuditAction {
            action: "toggle_flag".to_string(),
            target_type: "feature_flag".to_string(),
            target_id: flag.clone(),
            details: Some(serde_json::json!({ "value": value }).to_string()),
            bytes_affected: 0,
        })
        .await;

    log::info!("Feature flag {} set to {}", flag, value);
    Ok(value)
}

// ═══════════════════════════════════════════════════════════════════════════
// Learned Data Commands (M2)
// ═══════════════════════════════════════════════════════════════════════════

/// List learned data by entity type
#[tauri::command]
pub async fn list_learned_data(
    state: State<'_, AppState>,
    entity_type: String,
    limit: u32,
    offset: u32,
    search: Option<String>,
) -> Result<Vec<LearnedDataItem>, String> {
    let editor = DataEditor::new(state.database.get_pool().clone(), 10);

    editor
        .list_learned_data(&entity_type, limit as i32, offset as i32, search.as_deref())
        .await
        .map_err(|e| format!("Failed to list learned data: {}", e))
}

/// Get count of learned data by entity type
#[tauri::command]
pub async fn count_learned_data(
    state: State<'_, AppState>,
    entity_type: String,
    search: Option<String>,
) -> Result<i64, String> {
    let editor = DataEditor::new(state.database.get_pool().clone(), 10);

    editor
        .count_learned_data(&entity_type, search.as_deref())
        .await
        .map_err(|e| format!("Failed to count learned data: {}", e))
}

/// Edit a learned data field with versioning
#[tauri::command]
pub async fn edit_learned_data(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    field_name: String,
    new_value: String,
) -> Result<EditResult, String> {
    let editor = DataEditor::new(state.database.get_pool().clone(), 10);

    let result = editor
        .edit_learned_data(&entity_type, &entity_id, &field_name, &new_value)
        .await
        .map_err(|e| format!("Failed to edit learned data: {}", e))?;

    // Audit log
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    let _ = audit
        .log_action(crate::audit_log::AuditAction {
            action: "edit_learned_data".to_string(),
            target_type: entity_type.clone(),
            target_id: entity_id.clone(),
            details: Some(
                serde_json::json!({
                    "field": field_name,
                    "version_id": result.version_id
                })
                .to_string(),
            ),
            bytes_affected: 0,
        })
        .await;

    Ok(result)
}

/// Get version history for an entity
#[tauri::command]
pub async fn get_data_versions(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
) -> Result<Vec<DataVersion>, String> {
    let editor = DataEditor::new(state.database.get_pool().clone(), 10);

    editor
        .get_versions(&entity_type, &entity_id)
        .await
        .map_err(|e| format!("Failed to get versions: {}", e))
}

/// Restore a previous version
#[tauri::command]
pub async fn restore_data_version(
    state: State<'_, AppState>,
    version_id: i64,
) -> Result<EditResult, String> {
    let editor = DataEditor::new(state.database.get_pool().clone(), 10);

    let result = editor
        .restore_version(version_id)
        .await
        .map_err(|e| format!("Failed to restore version: {}", e))?;

    // Audit log
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    let _ = audit
        .log_action(crate::audit_log::AuditAction {
            action: "restore_version".to_string(),
            target_type: "data_version".to_string(),
            target_id: version_id.to_string(),
            details: Some(
                serde_json::json!({
                    "new_version_id": result.version_id
                })
                .to_string(),
            ),
            bytes_affected: 0,
        })
        .await;

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tools Console Commands (M4)
// ═══════════════════════════════════════════════════════════════════════════

/// Job history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEntry {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub details: Option<String>,
}

/// Get job history from frame queue
#[tauri::command]
pub async fn get_job_history(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<JobEntry>, String> {
    // Get recent processed frames as "jobs" from frame_queue
    let rows = sqlx::query(
        "SELECT id, frame_path, status, queued_at, processed_at, error_message
         FROM frame_queue
         ORDER BY queued_at DESC
         LIMIT ?",
    )
    .bind(limit as i32)
    .fetch_all(state.database.get_pool().as_ref())
    .await
    .map_err(|e| format!("Failed to query job history: {}", e))?;

    use sqlx::Row;
    Ok(rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.get("id");
            let frame_path: String = row.get("frame_path");
            let status: String = row.get("status");
            let queued_at: String = row.get("queued_at");
            let processed_at: Option<String> = row.get("processed_at");
            let error_message: Option<String> = row.get("error_message");

            // Calculate duration if we have both timestamps
            let duration_ms =
                if let (Some(processed), Some(_queued)) = (&processed_at, Some(&queued_at)) {
                    chrono::DateTime::parse_from_rfc3339(processed)
                        .ok()
                        .and_then(|p| {
                            chrono::DateTime::parse_from_rfc3339(&queued_at)
                                .ok()
                                .map(|q| (p - q).num_milliseconds())
                        })
                } else {
                    None
                };

            JobEntry {
                id: id.to_string(),
                job_type: "frame_process".to_string(),
                status,
                started_at: queued_at,
                completed_at: processed_at,
                duration_ms,
                details: Some(format!(
                    "{}{}",
                    frame_path.split('/').last().unwrap_or(&frame_path),
                    error_message
                        .map(|e| format!(" - Error: {}", e))
                        .unwrap_or_default()
                )),
            }
        })
        .collect())
}

/// Pause or resume the ingest queue
#[tauri::command]
pub async fn pause_ingest_queue(state: State<'_, AppState>, paused: bool) -> Result<bool, String> {
    // Toggle the enable_ingest setting
    state
        .settings
        .set("enable_ingest", &(!paused).to_string())
        .await
        .map_err(|e| format!("Failed to set enable_ingest: {}", e))?;

    // Audit log
    let audit = AuditLog::new(state.database.get_pool().as_ref().clone());
    let _ = audit
        .log_action(crate::audit_log::AuditAction {
            action: if paused {
                "pause_queue".to_string()
            } else {
                "resume_queue".to_string()
            },
            target_type: "ingest_queue".to_string(),
            target_id: "main".to_string(),
            details: None,
            bytes_affected: 0,
        })
        .await;

    log::info!("Ingest queue {}", if paused { "paused" } else { "resumed" });
    Ok(!paused) // Return the new enabled state
}

/// Get database statistics
#[tauri::command]
pub async fn get_database_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    use sqlx::Row;

    // Get counts from various tables
    let meetings_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM meetings")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let frames_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM frames")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let transcripts_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM transcripts")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let entities_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM entities")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let frame_queue_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM frame_queue")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let audit_log_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM audit_log")
        .fetch_one(state.database.get_pool().as_ref())
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    Ok(serde_json::json!({
        "meetings": meetings_count,
        "frames": frames_count,
        "transcripts": transcripts_count,
        "entities": entities_count,
        "frame_queue": frame_queue_count,
        "audit_log": audit_log_count,
    }))
}
