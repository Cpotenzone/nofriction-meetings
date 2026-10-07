//! Notebooks: an optional grouping for a recording of any type, such as
//! "Acme project", "BIO 101" or "Health". See
//! docs/TIMED_RECORDING_AND_NOTEBOOKS.md. (This was "classes" before the
//! recording type existed.)
//!
//! - Stored as `meetings.class_name` (nullable; the column keeps its old
//!   name, no data migration: it is the notebook), next to
//!   `meetings.planned_minutes` (timed recording). Both are added to older
//!   databases by `ensure_schema` (`database::ensure_columns`).
//! - Recent notebooks are read from the recordings that have one; no
//!   separate list is kept, so deleting a recording deletes its notebook
//!   name with it.
//! - A notebook never changes how notes are written: the recording's type
//!   does (recording_kind.rs).

use crate::database::{ensure_columns, DatabaseManager};
use crate::AppState;
use sqlx::Row;
use tauri::State;

/// Recent-notebook chips offered in the Record sheet.
pub const RECENT_LIMIT: i64 = 12;
/// Longest notebook name kept (characters).
pub const MAX_LEN: usize = 80;

/// Columns this feature adds to `meetings` (owned here, created at launch).
/// `class_name` is the notebook.
pub async fn ensure_schema(conn: &mut sqlx::SqliteConnection) -> Result<(), sqlx::Error> {
    ensure_columns(conn, "meetings", &[("planned_minutes", "INTEGER"), ("class_name", "TEXT")]).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_meetings_class ON meetings(class_name)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Trim, collapse runs of whitespace (newlines included) to one space, drop
/// control characters, cap at `MAX_LEN` characters. Empty → None.
pub fn normalize(input: &str) -> Option<String> {
    let cleaned: String = input
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let collapsed = cleaned.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_LEN).collect();
    let capped = capped.trim_end().to_string();
    (!capped.is_empty()).then_some(capped)
}

/// The name to store: normalized, and spelled like an existing notebook
/// that matches it ignoring case ("bio 101" joins "BIO 101").
pub fn canonical(input: &str, existing: &[String]) -> Option<String> {
    let name = normalize(input)?;
    let lower = name.to_lowercase();
    Some(
        existing
            .iter()
            .find(|e| e.to_lowercase() == lower)
            .cloned()
            .unwrap_or(name),
    )
}

impl DatabaseManager {
    pub async fn get_meeting_notebook(&self, meeting_id: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT class_name FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("class_name")))
    }

    /// Set (or clear, with None) a recording's notebook. Returns the stored name.
    pub async fn set_meeting_notebook(&self, meeting_id: &str, notebook: Option<&str>) -> Result<Option<String>, sqlx::Error> {
        let name = match notebook {
            Some(input) => {
                let existing = self.recent_notebooks(500).await?;
                canonical(input, &existing)
            }
            None => None,
        };
        sqlx::query("UPDATE meetings SET class_name = ? WHERE id = ?")
            .bind(&name)
            .bind(meeting_id)
            .execute(self.pool())
            .await?;
        Ok(name)
    }

    pub async fn set_meeting_planned_minutes(&self, meeting_id: &str, minutes: Option<i64>) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE meetings SET planned_minutes = ? WHERE id = ?")
            .bind(minutes)
            .bind(meeting_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Notebooks of saved recordings, most recently recorded first, one per
    /// name ignoring case (the most recent spelling wins).
    pub async fn recent_notebooks(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT class_name FROM meetings WHERE class_name IS NOT NULL AND class_name != '' \
             ORDER BY started_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .into_iter()
            .filter(|c| seen.insert(c.to_lowercase()))
            .take(limit.max(0) as usize)
            .collect())
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// Set or clear the notebook of a recording (Record sheet, recording detail).
#[tauri::command(rename_all = "camelCase")]
pub async fn set_meeting_notebook(
    meeting_id: String,
    notebook: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state
        .database
        .set_meeting_notebook(&meeting_id, notebook.as_deref())
        .await
        .map_err(|e| format!("Failed to save the notebook: {}", e))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_recent_notebooks(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .database
        .recent_notebooks(RECENT_LIMIT)
        .await
        .map_err(|e| format!("Failed to list notebooks: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cleans_and_caps() {
        assert_eq!(normalize("  BIO 101 —  Cell\n Biology "), Some("BIO 101 — Cell Biology".to_string()));
        assert_eq!(normalize("\t\n "), None);
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("CHEM\u{0007}201"), Some("CHEM201".to_string()));
        let long = "x".repeat(200);
        assert_eq!(normalize(&long).unwrap().chars().count(), MAX_LEN);
        // Cap never leaves a trailing space
        let spaced = format!("{} y", "x".repeat(MAX_LEN - 1));
        assert_eq!(normalize(&spaced), Some("x".repeat(MAX_LEN - 1)));
    }

    #[test]
    fn canonical_reuses_existing_spelling() {
        let existing = vec!["BIO 101 — Cell Biology".to_string(), "Acme project".to_string()];
        assert_eq!(canonical("bio 101 — cell biology", &existing), Some("BIO 101 — Cell Biology".to_string()));
        assert_eq!(canonical(" acme  PROJECT ", &existing), Some("Acme project".to_string()));
        assert_eq!(canonical("Health", &existing), Some("Health".to_string()));
        assert_eq!(canonical("   ", &existing), None);
    }

    async fn db() -> (DatabaseManager, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nf-notebooks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = DatabaseManager::new(&dir.join("t.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn notebook_is_stored_listed_filtered_and_deleted_with_the_recording() {
        let (db, dir) = db().await;
        db.create_meeting("m1", "Lecture 1").await.unwrap();
        db.create_meeting("m2", "Standup").await.unwrap();
        db.create_meeting("m3", "Lecture 2").await.unwrap();
        assert_eq!(db.set_meeting_notebook("m1", Some(" BIO 101 ")).await.unwrap(), Some("BIO 101".into()));
        // Typed differently: joins the existing notebook
        assert_eq!(db.set_meeting_notebook("m3", Some("bio 101")).await.unwrap(), Some("BIO 101".into()));
        db.set_meeting_planned_minutes("m3", Some(90)).await.unwrap();

        let m3 = db.get_meeting("m3").await.unwrap().unwrap();
        assert_eq!((m3.class_name.as_deref(), m3.planned_minutes), (Some("BIO 101"), Some(90)));
        let m2 = db.get_meeting("m2").await.unwrap().unwrap();
        assert_eq!((m2.class_name, m2.planned_minutes), (None, None));
        assert_eq!(db.recent_notebooks(RECENT_LIMIT).await.unwrap(), vec!["BIO 101".to_string()]);

        let in_notebook = db.list_meetings_in_notebook("bio 101", 50).await.unwrap();
        let mut ids: Vec<_> = in_notebook.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["m1", "m3"]);

        // Clearing works
        assert_eq!(db.set_meeting_notebook("m1", None).await.unwrap(), None);
        assert_eq!(db.get_meeting_notebook("m1").await.unwrap(), None);

        // Deleting the last recording of a notebook removes the name everywhere
        db.delete_meeting("m3").await.unwrap();
        assert!(db.recent_notebooks(RECENT_LIMIT).await.unwrap().is_empty());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings WHERE class_name IS NOT NULL")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
