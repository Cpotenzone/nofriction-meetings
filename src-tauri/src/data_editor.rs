// Data Editor Module
// Provides versioned CRUD operations for learned data (text_snapshots, entities, activity_log)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

// =============================================================================
// Types
// =============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LearnedDataItem {
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub content: String,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataVersion {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub field_name: String,
    pub previous_value: Option<String>,
    pub new_value: Option<String>,
    pub diff: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditResult {
    pub success: bool,
    pub version_id: i64,
    pub message: String,
}

// =============================================================================
// Data Editor
// =============================================================================

pub struct DataEditor {
    pool: Arc<SqlitePool>,
    max_versions: u32,
}

impl DataEditor {
    pub fn new(pool: Arc<SqlitePool>, max_versions: u32) -> Self {
        Self { pool, max_versions }
    }

    /// List learned data by entity type
    pub async fn list_learned_data(
        &self,
        entity_type: &str,
        limit: i32,
        offset: i32,
        search: Option<&str>,
    ) -> Result<Vec<LearnedDataItem>, sqlx::Error> {
        match entity_type {
            "text_snapshot" => self.list_text_snapshots(limit, offset, search).await,
            "entity" => self.list_entities(limit, offset, search).await,
            "activity_log" => self.list_activity_log(limit, offset, search).await,
            "episode" => self.list_episodes(limit, offset, search).await,
            _ => Ok(vec![]),
        }
    }

    /// Get count of learned data by entity type
    pub async fn count_learned_data(
        &self,
        entity_type: &str,
        search: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let (table, search_col) = match entity_type {
            "text_snapshot" => ("text_snapshots", "content"),
            "entity" => ("entities", "entity_value"),
            "activity_log" => ("activity_log", "action"),
            "episode" => ("document_episodes", "document_path"),
            _ => return Ok(0),
        };

        let row = if let Some(s) = search {
            let query_str = format!(
                "SELECT COUNT(*) as count FROM {} WHERE {} LIKE ?",
                table, search_col
            );
            sqlx::query(&query_str)
                .bind(format!("%{}%", s))
                .fetch_one(self.pool.as_ref())
                .await?
        } else {
            let query_str = format!("SELECT COUNT(*) as count FROM {}", table);
            sqlx::query(&query_str)
                .fetch_one(self.pool.as_ref())
                .await?
        };

        Ok(row.get("count"))
    }

    /// Edit a learned data field with versioning
    pub async fn edit_learned_data(
        &self,
        entity_type: &str,
        entity_id: &str,
        field_name: &str,
        new_value: &str,
    ) -> Result<EditResult, sqlx::Error> {
        // Get current value
        let previous_value = self
            .get_field_value(entity_type, entity_id, field_name)
            .await?;

        // Create version record
        let version_id = self
            .create_version(
                entity_type,
                entity_id,
                field_name,
                previous_value.as_deref(),
                new_value,
            )
            .await?;

        // Update the actual record
        self.update_field_value(entity_type, entity_id, field_name, new_value)
            .await?;

        // Prune old versions if needed
        self.prune_versions(entity_type, entity_id).await?;

        Ok(EditResult {
            success: true,
            version_id,
            message: format!("Updated {}.{}", entity_type, field_name),
        })
    }

    /// Get version history for an entity
    pub async fn get_versions(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<DataVersion>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, entity_type, entity_id, field_name, previous_value, new_value, diff, timestamp
             FROM data_versions
             WHERE entity_type = ? AND entity_id = ?
             ORDER BY timestamp DESC
             LIMIT 50",
        )
        .bind(entity_type)
        .bind(entity_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DataVersion {
                id: row.get("id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                field_name: row.get("field_name"),
                previous_value: row.get("previous_value"),
                new_value: row.get("new_value"),
                diff: row.get("diff"),
                timestamp: row.get("timestamp"),
            })
            .collect())
    }

    /// Restore a previous version
    pub async fn restore_version(&self, version_id: i64) -> Result<EditResult, sqlx::Error> {
        // Get the version record
        let row = sqlx::query(
            "SELECT entity_type, entity_id, field_name, previous_value
             FROM data_versions WHERE id = ?",
        )
        .bind(version_id)
        .fetch_one(self.pool.as_ref())
        .await?;

        let entity_type: String = row.get("entity_type");
        let entity_id: String = row.get("entity_id");
        let field_name: String = row.get("field_name");
        let previous_value: Option<String> = row.get("previous_value");

        if let Some(value) = previous_value {
            // This will create a new version and update the field
            self.edit_learned_data(&entity_type, &entity_id, &field_name, &value)
                .await
        } else {
            Ok(EditResult {
                success: false,
                version_id: 0,
                message: "No previous value to restore".to_string(),
            })
        }
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    async fn list_text_snapshots(
        &self,
        limit: i32,
        offset: i32,
        search: Option<&str>,
    ) -> Result<Vec<LearnedDataItem>, sqlx::Error> {
        let query = if let Some(s) = search {
            sqlx::query(
                "SELECT snapshot_id, text, created_at, source as metadata
                 FROM text_snapshots
                 WHERE text LIKE ?
                 ORDER BY created_at DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(format!("%{}%", s))
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query(
                "SELECT snapshot_id, text, created_at, source as metadata
                 FROM text_snapshots
                 ORDER BY created_at DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
        };

        let rows = query.fetch_all(self.pool.as_ref()).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id: String = row.get("snapshot_id");
                let content: String = row.get("text");
                let created_at: String = row.get("created_at");
                LearnedDataItem {
                    entity_type: "text_snapshot".to_string(),
                    entity_id: id.clone(),
                    title: format!("Text Snapshot {}", &id[..8.min(id.len())]),
                    content: content.chars().take(200).collect(),
                    metadata: row.get("metadata"),
                    created_at: created_at.clone(),
                    updated_at: created_at,
                }
            })
            .collect())
    }

    async fn list_entities(
        &self,
        limit: i32,
        offset: i32,
        search: Option<&str>,
    ) -> Result<Vec<LearnedDataItem>, sqlx::Error> {
        let query = if let Some(s) = search {
            sqlx::query(
                "SELECT id, entity_type, entity_value, first_seen, last_seen, metadata
                 FROM entities
                 WHERE entity_value LIKE ? OR entity_type LIKE ?
                 ORDER BY last_seen DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(format!("%{}%", s))
            .bind(format!("%{}%", s))
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query(
                "SELECT id, entity_type, entity_value, first_seen, last_seen, metadata
                 FROM entities
                 ORDER BY last_seen DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
        };

        let rows = query.fetch_all(self.pool.as_ref()).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let entity_type_val: String = row.get("entity_type");
                let entity_value: String = row.get("entity_value");
                let first_seen: String = row.get("first_seen");
                let last_seen: String = row.get("last_seen");
                LearnedDataItem {
                    entity_type: "entity".to_string(),
                    entity_id: id.to_string(),
                    title: format!("[{}] {}", entity_type_val, entity_value),
                    content: entity_value,
                    metadata: row.get("metadata"),
                    created_at: first_seen,
                    updated_at: last_seen,
                }
            })
            .collect())
    }

    async fn list_activity_log(
        &self,
        limit: i32,
        offset: i32,
        search: Option<&str>,
    ) -> Result<Vec<LearnedDataItem>, sqlx::Error> {
        let query = if let Some(s) = search {
            sqlx::query(
                "SELECT id, action, metadata, timestamp
                 FROM activity_log
                 WHERE action LIKE ?
                 ORDER BY timestamp DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(format!("%{}%", s))
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query(
                "SELECT id, action, metadata, timestamp
                 FROM activity_log
                 ORDER BY timestamp DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
        };

        let rows = query.fetch_all(self.pool.as_ref()).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let action: String = row.get("action");
                let timestamp: String = row.get("timestamp");
                LearnedDataItem {
                    entity_type: "activity_log".to_string(),
                    entity_id: id.to_string(),
                    title: action.clone(),
                    content: action,
                    metadata: row.get("metadata"),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                }
            })
            .collect())
    }

    async fn list_episodes(
        &self,
        limit: i32,
        offset: i32,
        search: Option<&str>,
    ) -> Result<Vec<LearnedDataItem>, sqlx::Error> {
        let query = if let Some(s) = search {
            sqlx::query(
                "SELECT id, document_path, session_id, started_at, ended_at
                 FROM document_episodes
                 WHERE document_path LIKE ?
                 ORDER BY started_at DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(format!("%{}%", s))
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query(
                "SELECT id, document_path, session_id, started_at, ended_at
                 FROM document_episodes
                 ORDER BY started_at DESC
                 LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
        };

        let rows = query.fetch_all(self.pool.as_ref()).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let document_path: String = row.get("document_path");
                let started_at: String = row.get("started_at");
                let ended_at: Option<String> = row.get("ended_at");
                LearnedDataItem {
                    entity_type: "episode".to_string(),
                    entity_id: id.to_string(),
                    title: document_path
                        .split('/')
                        .last()
                        .unwrap_or(&document_path)
                        .to_string(),
                    content: document_path,
                    metadata: None,
                    created_at: started_at,
                    updated_at: ended_at.unwrap_or_default(),
                }
            })
            .collect())
    }

    async fn get_field_value(
        &self,
        entity_type: &str,
        entity_id: &str,
        field_name: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let (table, id_field) = match entity_type {
            "text_snapshot" => ("text_snapshots", "id"),
            "entity" => ("entities", "id"),
            "activity_log" => ("activity_log", "id"),
            "episode" => ("document_episodes", "id"),
            _ => return Ok(None),
        };

        // Validate field name to prevent SQL injection
        let valid_fields = match entity_type {
            "text_snapshot" => vec!["content"],
            "entity" => vec!["entity_value", "entity_type", "metadata"],
            "activity_log" => vec!["action", "metadata"],
            "episode" => vec!["document_path"],
            _ => vec![],
        };

        if !valid_fields.contains(&field_name) {
            return Ok(None);
        }

        let query = format!(
            "SELECT {} FROM {} WHERE {} = ?",
            field_name, table, id_field
        );
        let row = sqlx::query(&query)
            .bind(entity_id)
            .fetch_optional(self.pool.as_ref())
            .await?;

        Ok(row.map(|r| r.get::<String, _>(field_name)))
    }

    async fn update_field_value(
        &self,
        entity_type: &str,
        entity_id: &str,
        field_name: &str,
        new_value: &str,
    ) -> Result<(), sqlx::Error> {
        let (table, id_field) = match entity_type {
            "text_snapshot" => ("text_snapshots", "id"),
            "entity" => ("entities", "id"),
            "activity_log" => ("activity_log", "id"),
            "episode" => ("document_episodes", "id"),
            _ => return Ok(()),
        };

        // Validate field name
        let valid_fields = match entity_type {
            "text_snapshot" => vec!["content"],
            "entity" => vec!["entity_value", "entity_type", "metadata"],
            "activity_log" => vec!["action", "metadata"],
            "episode" => vec!["document_path"],
            _ => vec![],
        };

        if !valid_fields.contains(&field_name) {
            return Ok(());
        }

        let query = format!(
            "UPDATE {} SET {} = ? WHERE {} = ?",
            table, field_name, id_field
        );
        sqlx::query(&query)
            .bind(new_value)
            .bind(entity_id)
            .execute(self.pool.as_ref())
            .await?;

        Ok(())
    }

    async fn create_version(
        &self,
        entity_type: &str,
        entity_id: &str,
        field_name: &str,
        previous_value: Option<&str>,
        new_value: &str,
    ) -> Result<i64, sqlx::Error> {
        // Create a simple diff string
        let diff = if let Some(prev) = previous_value {
            format!(
                "-{}\n+{}",
                prev.chars().take(100).collect::<String>(),
                new_value.chars().take(100).collect::<String>()
            )
        } else {
            format!("+{}", new_value.chars().take(100).collect::<String>())
        };

        let result = sqlx::query(
            "INSERT INTO data_versions (entity_type, entity_id, field_name, previous_value, new_value, diff)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(field_name)
        .bind(previous_value)
        .bind(new_value)
        .bind(diff)
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn prune_versions(&self, entity_type: &str, entity_id: &str) -> Result<(), sqlx::Error> {
        // Keep only the last N versions
        sqlx::query(
            "DELETE FROM data_versions
             WHERE entity_type = ? AND entity_id = ?
             AND id NOT IN (
                 SELECT id FROM data_versions
                 WHERE entity_type = ? AND entity_id = ?
                 ORDER BY timestamp DESC
                 LIMIT ?
             )",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(self.max_versions as i32)
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }
}
