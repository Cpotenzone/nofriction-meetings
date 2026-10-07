//! Classes (course tagging): a meeting can belong to a class such as
//! "BIO 101 — Cell Biology". See docs/TIMED_RECORDING_AND_CLASSES.md.
//!
//! - Stored as `meetings.class_name` (nullable), next to
//!   `meetings.planned_minutes` (timed recording). Both are added to older
//!   databases by `ensure_schema` (`database::ensure_columns`).
//! - Recent classes are read from the meetings that have one; no separate
//!   list is kept, so deleting a meeting deletes its class name with it.
//! - A meeting with a class gets lecture notes (key concepts, definitions,
//!   examples, announcements and deadlines) instead of meeting minutes. Same
//!   JSON shape as the meeting report, so storage and the notes view don't
//!   change; only the prompt does. No provider logic here.

use crate::database::{ensure_columns, DatabaseManager};
use crate::AppState;
use sqlx::Row;
use tauri::State;

/// Recent-class chips offered in the Record sheet.
pub const RECENT_LIMIT: i64 = 12;
/// Longest class name kept (characters).
pub const MAX_LEN: usize = 80;
/// Set once the one-time "check your school's policy" notice was shown.
pub const SETTING_NOTICE_SHOWN: &str = "class_recording_notice_shown";
/// `meeting_notes.model_used` for notes written with the lecture prompt
/// (the notes view relabels its sections).
pub const LECTURE_NOTES_LABEL: &str = "lecture-notes";

/// Columns this feature adds to `meetings` (owned here, created at launch).
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

/// The name to store: normalized, and spelled like an existing class that
/// matches it ignoring case ("bio 101" joins "BIO 101").
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

/// Lecture notes, in the meeting report's JSON shape:
/// key_topics = key concepts, decisions = definitions (example in
/// `context`), action_items = announcements / assignments / deadlines the
/// instructor stated. Never tasks invented for students.
pub fn lecture_notes_prompt(class_name: &str) -> String {
    format!(
        r#"You are a careful note-taker for a student. The transcript below is a recording of a class ("{class}"). Write lecture notes, not meeting minutes.

Your output MUST be valid JSON with this exact schema:
{{
  "summary": "2-4 sentence overview of what the lecture covered",
  "key_topics": ["key concept 1", "key concept 2"],
  "decisions": [{{"text": "term: its definition as the instructor explained it", "made_by": null, "context": "an example the instructor used for it (or null)"}}],
  "action_items": [{{"task": "an announcement, assignment, reading or exam the instructor mentioned", "assignee": null, "due_date": "the date or deadline as stated (or null)", "priority": "high/medium/low"}}],
  "participants": ["the instructor's name, only if it is stated"]
}}

Rules:
- key_topics: the main concepts taught, in the order they came up
- decisions: definitions and worked examples; put the example in "context"
- action_items: ONLY announcements, assignments, readings, exams and deadlines the instructor stated. Never write action items for attendees and never invent tasks for students.
- Use "high" priority for exams and graded deadlines
- If there is nothing for a list, return an empty array
- Do NOT hallucinate information not in the transcript
"#,
        class = class_name
    )
}

/// The prompt and notes label for a meeting: lecture notes when it has a
/// class, otherwise the meeting prompt (user's custom report prompt or the
/// default) with `meeting_label`.
pub fn report_prompt<'a>(class_name: Option<&str>, meeting_prompt: &str, meeting_label: &'a str) -> (String, &'a str) {
    match class_name.and_then(normalize) {
        Some(class) => (lecture_notes_prompt(&class), LECTURE_NOTES_LABEL),
        None => (meeting_prompt.to_string(), meeting_label),
    }
}

impl DatabaseManager {
    pub async fn get_meeting_class(&self, meeting_id: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT class_name FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("class_name")))
    }

    /// Set (or clear, with None) a meeting's class. Returns the stored name.
    pub async fn set_meeting_class(&self, meeting_id: &str, class_name: Option<&str>) -> Result<Option<String>, sqlx::Error> {
        let name = match class_name {
            Some(input) => {
                let existing = self.recent_classes(500).await?;
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

    /// Classes of saved meetings, most recently recorded first, one per name
    /// ignoring case (the most recent spelling wins).
    pub async fn recent_classes(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
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

/// Set or clear the class of a meeting (Record sheet, meeting detail).
#[tauri::command(rename_all = "camelCase")]
pub async fn set_meeting_class(
    meeting_id: String,
    class_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state
        .database
        .set_meeting_class(&meeting_id, class_name.as_deref())
        .await
        .map_err(|e| format!("Failed to save the class: {}", e))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_recent_classes(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .database
        .recent_classes(RECENT_LIMIT)
        .await
        .map_err(|e| format!("Failed to list classes: {}", e))
}

/// True exactly once: the first recording with a class shows the "check
/// your school's policy" notice. A reminder, not an attestation.
#[tauri::command(rename_all = "camelCase")]
pub async fn take_class_recording_notice(state: State<'_, AppState>) -> Result<bool, String> {
    let shown = state.settings.get(SETTING_NOTICE_SHOWN).await.ok().flatten().as_deref() == Some("true");
    if shown {
        return Ok(false);
    }
    state
        .settings
        .set(SETTING_NOTICE_SHOWN, "true")
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;
    Ok(true)
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
        let existing = vec!["BIO 101 — Cell Biology".to_string(), "HIST 200".to_string()];
        assert_eq!(canonical("bio 101 — cell biology", &existing), Some("BIO 101 — Cell Biology".to_string()));
        assert_eq!(canonical(" hist  200 ", &existing), Some("HIST 200".to_string()));
        assert_eq!(canonical("MATH 3", &existing), Some("MATH 3".to_string()));
        assert_eq!(canonical("   ", &existing), None);
    }

    #[test]
    fn lecture_prompt_only_for_classes() {
        let (p, label) = report_prompt(Some("BIO 101"), "MEETING PROMPT", "auto-report");
        assert_eq!(label, LECTURE_NOTES_LABEL);
        assert!(p.contains("BIO 101") && p.contains("lecture notes"));
        assert!(p.contains("Never write action items for attendees"));
        // Same JSON keys as the meeting report, so GeneratedNotes parses it
        for key in ["\"summary\"", "\"key_topics\"", "\"decisions\"", "\"action_items\"", "\"participants\""] {
            assert!(p.contains(key), "{}", key);
        }
        assert!(!p.contains("{{"), "format braces resolved");
        let (p, label) = report_prompt(None, "MEETING PROMPT", "auto-report");
        assert_eq!((p.as_str(), label), ("MEETING PROMPT", "auto-report"));
        let (_, label) = report_prompt(Some("  "), "MEETING PROMPT", "default");
        assert_eq!(label, "default", "a blank class is no class");
    }

    async fn db() -> (DatabaseManager, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nf-classes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = DatabaseManager::new(&dir.join("t.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn class_is_stored_listed_filtered_and_deleted_with_the_meeting() {
        let (db, dir) = db().await;
        db.create_meeting("m1", "Lecture 1").await.unwrap();
        db.create_meeting("m2", "Standup").await.unwrap();
        db.create_meeting("m3", "Lecture 2").await.unwrap();
        assert_eq!(db.set_meeting_class("m1", Some(" BIO 101 ")).await.unwrap(), Some("BIO 101".into()));
        // Typed differently: joins the existing class
        assert_eq!(db.set_meeting_class("m3", Some("bio 101")).await.unwrap(), Some("BIO 101".into()));
        db.set_meeting_planned_minutes("m3", Some(90)).await.unwrap();

        let m3 = db.get_meeting("m3").await.unwrap().unwrap();
        assert_eq!((m3.class_name.as_deref(), m3.planned_minutes), (Some("BIO 101"), Some(90)));
        let m2 = db.get_meeting("m2").await.unwrap().unwrap();
        assert_eq!((m2.class_name, m2.planned_minutes), (None, None));
        assert_eq!(db.recent_classes(RECENT_LIMIT).await.unwrap(), vec!["BIO 101".to_string()]);

        let in_class = db.list_meetings_in_class("bio 101", 50).await.unwrap();
        let mut ids: Vec<_> = in_class.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["m1", "m3"]);

        // Clearing works
        assert_eq!(db.set_meeting_class("m1", None).await.unwrap(), None);
        assert_eq!(db.get_meeting_class("m1").await.unwrap(), None);

        // Deleting the last meeting of a class removes the name everywhere
        db.delete_meeting("m3").await.unwrap();
        assert!(db.recent_classes(RECENT_LIMIT).await.unwrap().is_empty());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings WHERE class_name IS NOT NULL")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
