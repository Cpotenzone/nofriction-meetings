// noFriction Meetings - Audit Log
// Persistent audit trail for all admin actions
//
// Features:
// - Append-only action logging
// - Query by action type, target, time range
// - Export capability

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};

/// Represents an entry in the audit log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub action: String,      // e.g., "delete", "edit", "pause", "toggle_flag"
    pub target_type: String, // e.g., "meeting", "text_snapshot", "entity", "queue"
    pub target_id: String,
    pub details: Option<String>, // JSON metadata
    pub bytes_affected: u64,
    pub timestamp: DateTime<Utc>,
}

/// Input for creating a new audit entry
#[derive(Debug, Clone)]
pub struct AuditAction {
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub details: Option<String>,
    pub bytes_affected: u64,
}

/// Audit log manager
pub struct AuditLog {
    pool: Pool<Sqlite>,
}

impl AuditLog {
    /// Create a new audit log manager
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Log an action (append-only)
    pub async fn log_action(&self, action: AuditAction) -> Result<i64, String> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            INSERT INTO audit_log (action, target_type, target_id, details, bytes_affected, timestamp)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&action.action)
        .bind(&action.target_type)
        .bind(&action.target_id)
        .bind(&action.details)
        .bind(action.bytes_affected as i64)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to insert audit log: {}", e))?;

        log::info!(
            "AUDIT: {} {} {} ({})",
            action.action,
            action.target_type,
            action.target_id,
            crate::storage_manager::StorageManager::format_bytes(action.bytes_affected)
        );

        Ok(result.last_insert_rowid())
    }

    /// Log a deletion action
    pub async fn log_deletion(
        &self,
        target_type: &str,
        target_id: &str,
        bytes_freed: u64,
        details: Option<serde_json::Value>,
    ) -> Result<i64, String> {
        self.log_action(AuditAction {
            action: "delete".to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            details: details.map(|d| d.to_string()),
            bytes_affected: bytes_freed,
        })
        .await
    }

    /// Log an edit action
    pub async fn log_edit(
        &self,
        target_type: &str,
        target_id: &str,
        field: &str,
        old_value: Option<&str>,
        new_value: &str,
    ) -> Result<i64, String> {
        let details = serde_json::json!({
            "field": field,
            "old_value": old_value,
            "new_value": new_value,
        });

        self.log_action(AuditAction {
            action: "edit".to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            details: Some(details.to_string()),
            bytes_affected: 0,
        })
        .await
    }

    /// Query audit log entries
    pub async fn get_entries(
        &self,
        limit: u32,
        offset: u32,
        action_filter: Option<&str>,
    ) -> Result<Vec<AuditEntry>, String> {
        let query = if let Some(action) = action_filter {
            sqlx::query(
                r#"
                SELECT id, action, target_type, target_id, details, bytes_affected, timestamp
                FROM audit_log
                WHERE action = ?
                ORDER BY timestamp DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(action)
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query(
                r#"
                SELECT id, action, target_type, target_id, details, bytes_affected, timestamp
                FROM audit_log
                ORDER BY timestamp DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(limit)
            .bind(offset)
        };

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("Failed to query audit log: {}", e))?;

        let entries = rows
            .iter()
            .filter_map(|row| {
                let timestamp_str: String = row.get("timestamp");
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                    .ok()?
                    .with_timezone(&Utc);

                Some(AuditEntry {
                    id: row.get("id"),
                    action: row.get("action"),
                    target_type: row.get("target_type"),
                    target_id: row.get("target_id"),
                    details: row.get("details"),
                    bytes_affected: row.get::<i64, _>("bytes_affected") as u64,
                    timestamp,
                })
            })
            .collect();

        Ok(entries)
    }

    /// Get count of audit entries
    pub async fn count_entries(&self, action_filter: Option<&str>) -> Result<u32, String> {
        let count: i64 = if let Some(action) = action_filter {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = ?")
                .bind(action)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| format!("Failed to count: {}", e))?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| format!("Failed to count: {}", e))?
        };

        Ok(count as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                details TEXT,
                bytes_affected INTEGER DEFAULT 0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_log_and_query() {
        let pool = setup_test_db().await;
        let audit = AuditLog::new(pool);

        // Log a deletion
        let id = audit
            .log_deletion("meeting", "test123", 1000, None)
            .await
            .unwrap();
        assert!(id > 0);

        // Query entries
        let entries = audit.get_entries(10, 0, None).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "delete");
        assert_eq!(entries[0].target_id, "test123");
    }

    #[tokio::test]
    async fn test_filter_by_action() {
        let pool = setup_test_db().await;
        let audit = AuditLog::new(pool);

        // Log different actions
        audit
            .log_deletion("meeting", "m1", 1000, None)
            .await
            .unwrap();
        audit
            .log_edit("text_snapshot", "s1", "text", None, "new text")
            .await
            .unwrap();

        // Filter by action
        let deletes = audit.get_entries(10, 0, Some("delete")).await.unwrap();
        assert_eq!(deletes.len(), 1);

        let edits = audit.get_entries(10, 0, Some("edit")).await.unwrap();
        assert_eq!(edits.len(), 1);
    }
}
