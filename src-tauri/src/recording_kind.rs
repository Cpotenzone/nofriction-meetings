//! "What is it?": a recording's type, **Meeting · Class · Personal**. See
//! docs/TIMED_RECORDING_AND_NOTEBOOKS.md. The same vocabulary is used on iOS.
//!
//! - Stored as `meetings.recording_kind` ('meeting' / 'class' / 'personal').
//!   NULL means meeting: rows from before this column, and anything that
//!   doesn't read as a type. Added by `ensure_schema`, which backfills once:
//!   a recording that had a class (`class_name`, now the notebook) becomes
//!   'class'.
//! - The type picks the notes style (meeting notes, lecture notes, or
//!   personal notes: summary, key points, to-dos and reminders), the third
//!   mark's label (On the test / Follow up / Remember), the guide's name
//!   (Study guide for a class, Review guide otherwise) and the title of an
//!   untitled recording. Only labels and prompts vary: stored values and
//!   JSON shapes are the same for every type, and no provider logic lives
//!   here.
//! - The Record sheet remembers the last type picked
//!   (`recording_default_kind`, Meeting until then). Starts that skip the
//!   sheet (⌘N, tray, command palette) use the remembered type.
//! - The first Class-type recording shows a one-time school-policy notice.

use crate::database::{ensure_columns, DatabaseManager};
use crate::AppState;
use chrono::{DateTime, TimeZone};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

/// Setting key for the remembered type ("meeting" | "class" | "personal").
pub const SETTING_DEFAULT_KIND: &str = "recording_default_kind";
/// Set once the one-time "check your school's policy" notice was shown.
/// (Same key as before: users who saw it for a class don't see it again.)
pub const SETTING_CLASS_NOTICE_SHOWN: &str = "class_recording_notice_shown";
/// Emitted when a Class-type recording starts and the notice hasn't been
/// shown yet. The UI shows it (non-blocking).
pub const CLASS_NOTICE_EVENT: &str = "class-recording-notice";
/// `meeting_notes.model_used` for notes written with the lecture prompt.
pub const LECTURE_NOTES_LABEL: &str = "lecture-notes";
/// `meeting_notes.model_used` for notes written with the personal prompt.
pub const PERSONAL_NOTES_LABEL: &str = "personal-notes";

/// What a recording is. Wire and stored format: "meeting" / "class" / "personal".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingKind {
    #[default]
    Meeting,
    Class,
    Personal,
}

impl RecordingKind {
    /// Picker order: Meeting · Class · Personal.
    pub const ALL: [RecordingKind; 3] = [RecordingKind::Meeting, RecordingKind::Class, RecordingKind::Personal];

    /// "class" / " Personal " → the type; anything else → None.
    pub fn parse(s: &str) -> Option<RecordingKind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "meeting" => Some(RecordingKind::Meeting),
            "class" => Some(RecordingKind::Class),
            "personal" => Some(RecordingKind::Personal),
            _ => None,
        }
    }

    /// The stored column value: NULL or unreadable → Meeting.
    pub fn from_stored(stored: Option<&str>) -> RecordingKind {
        stored.and_then(RecordingKind::parse).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RecordingKind::Meeting => "meeting",
            RecordingKind::Class => "class",
            RecordingKind::Personal => "personal",
        }
    }

    /// Picker label: "Meeting" / "Class" / "Personal".
    pub fn label(self) -> &'static str {
        match self {
            RecordingKind::Meeting => "Meeting",
            RecordingKind::Class => "Class",
            RecordingKind::Personal => "Personal",
        }
    }

    /// The generated guide's name: "Study guide" for a class, "Review guide" otherwise.
    pub fn guide_title(self) -> &'static str {
        match self {
            RecordingKind::Class => "Study guide",
            _ => "Review guide",
        }
    }

    /// Label of the third mark (stored kind `test`, the same for every type).
    pub fn third_mark_label(self) -> &'static str {
        match self {
            RecordingKind::Class => "On the test",
            RecordingKind::Meeting => "Follow up",
            RecordingKind::Personal => "Remember",
        }
    }
}

/// The remembered type from its stored setting. Missing or unreadable → Meeting.
pub fn remembered_from(stored: Option<&str>) -> RecordingKind {
    RecordingKind::from_stored(stored)
}

/// The type a start should use: the plan's, or the remembered one when the
/// plan has none (⌘N, tray, command palette).
pub fn resolve(requested: Option<&str>, remembered: RecordingKind) -> Result<RecordingKind, String> {
    match requested.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(remembered),
        Some(s) => RecordingKind::parse(s).ok_or_else(|| format!("Unknown recording type: {}", s)),
    }
}

// ─── Untitled recordings ─────────────────────────────────────────────────────

/// The title of a recording with no calendar match: the notebook plus the
/// date when it has one ("BIO 101 — Oct 7"), else the type plus the date
/// ("Class — Oct 7", "Personal — Oct 7").
pub fn untitled_title<Tz: TimeZone>(kind: RecordingKind, notebook: Option<&str>, when: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let date = when.format("%b %-d");
    match notebook.and_then(crate::notebooks::normalize) {
        Some(n) => format!("{} — {}", n, date),
        None => format!("{} — {}", kind.label(), date),
    }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A title the app made up (so linking a calendar event may replace it):
/// "Meeting 2026-09-24 09:01" (earlier builds) or "<type or notebook> — Oct 7".
pub fn is_untitled_title(title: &str) -> bool {
    if title.starts_with("Meeting 20") && title.len() <= "Meeting 2026-09-24 09:01".len() {
        return true;
    }
    let Some((head, date)) = title.rsplit_once(" — ") else { return false };
    let mut parts = date.split(' ');
    let (Some(month), Some(day), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    !head.trim().is_empty()
        && MONTHS.contains(&month)
        && (1..=2).contains(&day.len())
        && day.parse::<u32>().map(|d| (1..=31).contains(&d)).unwrap_or(false)
}

// ─── Notes style ─────────────────────────────────────────────────────────────

/// Lecture notes, in the meeting report's JSON shape:
/// key_topics = key concepts, decisions = definitions (example in
/// `context`), action_items = announcements / assignments / deadlines the
/// instructor stated. Never tasks invented for students.
pub fn lecture_notes_prompt(class_name: Option<&str>) -> String {
    let what = match class_name.and_then(crate::notebooks::normalize) {
        Some(c) => format!("a class (\"{}\")", c),
        None => "a class".to_string(),
    };
    format!(
        r#"You are a careful note-taker for a student. The transcript below is a recording of {what}. Write lecture notes, not meeting minutes.

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
        what = what
    )
}

/// Personal notes (a conversation, appointment, talk or idea), in the same
/// JSON shape: key_topics = key points, action_items = to-dos and reminders
/// that were said. `decisions` and `participants` stay empty: no minutes,
/// no attendees, no owners.
pub fn personal_notes_prompt(notebook: Option<&str>) -> String {
    let what = match notebook.and_then(crate::notebooks::normalize) {
        Some(n) => format!("a personal recording (notebook \"{}\")", n),
        None => "a personal recording".to_string(),
    };
    format!(
        r#"You are a careful note-taker. The transcript below is {what}: a conversation, appointment, talk or idea. Write short personal notes in plain language.

Your output MUST be valid JSON with this exact schema:
{{
  "summary": "2-4 sentence overview of what was said",
  "key_topics": ["key point 1", "key point 2"],
  "decisions": [],
  "action_items": [{{"task": "a to-do or reminder that was said", "assignee": null, "due_date": "the date or time as stated (or null)", "priority": "high/medium/low"}}],
  "participants": []
}}

Rules:
- key_topics: the key points worth keeping (facts, numbers, instructions, ideas), in the order they came up
- action_items: ONLY to-dos and reminders that were actually said (things to do, buy, book, take, call or check). Never invent tasks and never assign them to anyone.
- decisions and participants: always empty arrays
- Use "high" priority only for something with a stated date or urgency
- If there is nothing for a list, return an empty array
- Do NOT hallucinate information not in the transcript
"#,
        what = what
    )
}

/// The notes prompt and `model_used` label for a recording: meeting notes
/// (the user's custom report prompt or the default, with `meeting_label`)
/// for a meeting, lecture notes for a class, personal notes for personal.
/// The notebook only names the class or notebook in the prompt.
pub fn report_prompt<'a>(
    kind: RecordingKind,
    notebook: Option<&str>,
    meeting_prompt: &str,
    meeting_label: &'a str,
) -> (String, &'a str) {
    match kind {
        RecordingKind::Meeting => (meeting_prompt.to_string(), meeting_label),
        RecordingKind::Class => (lecture_notes_prompt(notebook), LECTURE_NOTES_LABEL),
        RecordingKind::Personal => (personal_notes_prompt(notebook), PERSONAL_NOTES_LABEL),
    }
}

// ─── Schema ──────────────────────────────────────────────────────────────────

/// Adds `meetings.recording_kind` (nullable; NULL = meeting). When the
/// column is new, recordings that had a class become 'class', in the same
/// transaction, so the backfill runs exactly once. Runs after
/// `notebooks::ensure_schema` (it needs `class_name`).
pub async fn ensure_schema(conn: &mut sqlx::SqliteConnection) -> Result<(), sqlx::Error> {
    use sqlx::Connection;
    let mut tx = conn.begin().await?;
    let added = ensure_columns(&mut tx, "meetings", &[("recording_kind", "TEXT")]).await?;
    if added.iter().any(|c| c == "recording_kind") {
        let n = sqlx::query(
            "UPDATE meetings SET recording_kind = 'class' \
             WHERE class_name IS NOT NULL AND TRIM(class_name) != ''",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        log::info!("Recording types: {} recording(s) with a class marked as Class", n);
    }
    tx.commit().await?;
    Ok(())
}

impl DatabaseManager {
    /// A recording's type and notebook (`class_name`).
    pub async fn get_kind_and_notebook(&self, meeting_id: &str) -> Result<(RecordingKind, Option<String>), sqlx::Error> {
        let row = sqlx::query("SELECT recording_kind, class_name FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(match row {
            Some(r) => (
                RecordingKind::from_stored(r.get::<Option<String>, _>("recording_kind").as_deref()),
                r.get::<Option<String>, _>("class_name"),
            ),
            None => (RecordingKind::default(), None),
        })
    }

    pub async fn set_meeting_kind(&self, meeting_id: &str, kind: RecordingKind) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE meetings SET recording_kind = ? WHERE id = ?")
            .bind(kind.as_str())
            .bind(meeting_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

// ─── Runtime ─────────────────────────────────────────────────────────────────

/// The remembered type (Settings).
pub async fn remembered(settings: &crate::settings::SettingsManager) -> RecordingKind {
    remembered_from(settings.get(SETTING_DEFAULT_KIND).await.ok().flatten().as_deref())
}

pub async fn remember(settings: &crate::settings::SettingsManager, kind: RecordingKind) {
    if let Err(e) = settings.set(SETTING_DEFAULT_KIND, kind.as_str()).await {
        log::warn!("Could not save the recording type: {}", e);
    }
}

/// True exactly once: the first Class-type recording shows the "check your
/// school's policy" notice. A reminder, not an attestation.
pub async fn take_class_notice(settings: &crate::settings::SettingsManager) -> bool {
    let shown = settings.get(SETTING_CLASS_NOTICE_SHOWN).await.ok().flatten().as_deref() == Some("true");
    if shown {
        return false;
    }
    match settings.set(SETTING_CLASS_NOTICE_SHOWN, "true").await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("Could not save the class notice setting: {}", e);
            false
        }
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// Change a recording's type afterwards (REWIND → the recording's Type).
/// Returns the stored value. New notes use the new type's style.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_meeting_recording_kind(
    meeting_id: String,
    kind: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let kind = RecordingKind::parse(&kind).ok_or_else(|| format!("Unknown recording type: {}", kind))?;
    state
        .database
        .set_meeting_kind(&meeting_id, kind)
        .await
        .map_err(|e| format!("Failed to save the type: {}", e))?;
    Ok(kind.as_str().to_string())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn parse_store_and_labels() {
        assert_eq!(RecordingKind::parse(" Class "), Some(RecordingKind::Class));
        assert_eq!(RecordingKind::parse("PERSONAL"), Some(RecordingKind::Personal));
        assert_eq!(RecordingKind::parse("meeting"), Some(RecordingKind::Meeting));
        assert_eq!(RecordingKind::parse("lecture"), None);
        assert_eq!(RecordingKind::from_stored(None), RecordingKind::Meeting, "NULL means meeting");
        assert_eq!(RecordingKind::from_stored(Some("junk")), RecordingKind::Meeting);
        assert_eq!(RecordingKind::from_stored(Some("class")), RecordingKind::Class);
        assert_eq!(
            RecordingKind::ALL.map(|k| k.label()),
            ["Meeting", "Class", "Personal"],
            "picker order and labels"
        );
        assert_eq!(RecordingKind::ALL.map(|k| k.as_str()), ["meeting", "class", "personal"]);
        assert_eq!(serde_json::to_string(&RecordingKind::Personal).unwrap(), "\"personal\"");
        assert_eq!(remembered_from(None), RecordingKind::Meeting, "Meeting until a type is picked");
        assert_eq!(remembered_from(Some("personal")), RecordingKind::Personal);
    }

    #[test]
    fn resolve_uses_the_plan_or_the_remembered_type() {
        assert_eq!(resolve(None, RecordingKind::Class), Ok(RecordingKind::Class), "tray / ⌘N / palette");
        assert_eq!(resolve(Some(""), RecordingKind::Personal), Ok(RecordingKind::Personal));
        assert_eq!(resolve(Some("meeting"), RecordingKind::Class), Ok(RecordingKind::Meeting));
        assert!(resolve(Some("party"), RecordingKind::Class).is_err());
    }

    #[test]
    fn marker_and_guide_labels_by_type() {
        assert_eq!(RecordingKind::Class.third_mark_label(), "On the test");
        assert_eq!(RecordingKind::Meeting.third_mark_label(), "Follow up");
        assert_eq!(RecordingKind::Personal.third_mark_label(), "Remember");
        assert_eq!(RecordingKind::Class.guide_title(), "Study guide");
        assert_eq!(RecordingKind::Meeting.guide_title(), "Review guide");
        assert_eq!(RecordingKind::Personal.guide_title(), "Review guide");
    }

    #[test]
    fn untitled_titles() {
        let oct7 = Utc.with_ymd_and_hms(2026, 10, 7, 9, 30, 0).unwrap();
        assert_eq!(untitled_title(RecordingKind::Class, None, &oct7), "Class — Oct 7");
        assert_eq!(untitled_title(RecordingKind::Personal, None, &oct7), "Personal — Oct 7");
        assert_eq!(untitled_title(RecordingKind::Meeting, None, &oct7), "Meeting — Oct 7");
        assert_eq!(untitled_title(RecordingKind::Class, Some(" BIO  101 "), &oct7), "BIO 101 — Oct 7", "notebook wins");
        assert_eq!(untitled_title(RecordingKind::Meeting, Some("   "), &oct7), "Meeting — Oct 7", "blank notebook");
        let dec25 = Utc.with_ymd_and_hms(2026, 12, 25, 23, 0, 0).unwrap();
        assert_eq!(untitled_title(RecordingKind::Personal, Some("Health"), &dec25), "Health — Dec 25");

        for t in ["Class — Oct 7", "BIO 101 — Dec 25", "Meeting 2026-09-24 09:01", "Acme — project — Jan 31"] {
            assert!(is_untitled_title(t), "{}", t);
        }
        for t in ["Weekly sync", "Class — October 7", "— Oct 7", "Class — Oct 77", "Class — Oct 7 2026", "Class — Foo 7"] {
            assert!(!is_untitled_title(t), "{}", t);
        }
        // Every made-up title is recognised
        for k in RecordingKind::ALL {
            assert!(is_untitled_title(&untitled_title(k, None, &oct7)));
            assert!(is_untitled_title(&untitled_title(k, Some("Health"), &dec25)));
        }
    }

    const KEYS: [&str; 5] = ["\"summary\"", "\"key_topics\"", "\"decisions\"", "\"action_items\"", "\"participants\""];

    #[test]
    fn notes_prompt_follows_the_type_not_the_notebook() {
        // Meeting: the user's report prompt, even with a notebook
        let (p, label) = report_prompt(RecordingKind::Meeting, Some("BIO 101"), "MEETING PROMPT", "auto-report");
        assert_eq!((p.as_str(), label), ("MEETING PROMPT", "auto-report"));

        // Class: lecture notes, with or without a notebook
        let (p, label) = report_prompt(RecordingKind::Class, Some("BIO 101"), "MEETING PROMPT", "auto-report");
        assert_eq!(label, LECTURE_NOTES_LABEL);
        assert!(p.contains("a class (\"BIO 101\")") && p.contains("lecture notes"));
        assert!(p.contains("Never write action items for attendees"));
        let (p, label) = report_prompt(RecordingKind::Class, None, "MEETING PROMPT", "default");
        assert_eq!(label, LECTURE_NOTES_LABEL, "a class without a notebook still gets lecture notes");
        assert!(p.contains("a recording of a class. Write lecture notes"), "{}", p);

        // Personal: summary, key points, to-dos and reminders
        let (p, label) = report_prompt(RecordingKind::Personal, Some("Health"), "MEETING PROMPT", "default");
        assert_eq!(label, PERSONAL_NOTES_LABEL);
        assert!(p.contains("notebook \"Health\"") && p.contains("key point") && p.contains("to-do or reminder"));
        let lower = p.to_lowercase();
        for banned in ["attendee", "minutes", "action item", "meeting", "owner"] {
            assert!(!lower.contains(banned), "personal notes avoid {:?}", banned);
        }

        // Same JSON keys for every type, so GeneratedNotes parses all of them
        for kind in [RecordingKind::Class, RecordingKind::Personal] {
            let (p, _) = report_prompt(kind, Some("X"), "", "");
            for key in KEYS {
                assert!(p.contains(key), "{:?} {}", kind, key);
            }
            assert!(!p.contains("{{"), "format braces resolved");
            assert!(p.contains("Do NOT hallucinate"));
        }
    }

    async fn db() -> (DatabaseManager, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nf-kind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = DatabaseManager::new(&dir.join("t.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn kind_is_stored_read_and_defaults_to_meeting() {
        let (db, dir) = db().await;
        db.create_meeting("m1", "One").await.unwrap();
        db.create_meeting("m2", "Two").await.unwrap();
        assert_eq!(db.get_kind_and_notebook("m1").await.unwrap(), (RecordingKind::Meeting, None), "NULL → meeting");
        db.set_meeting_kind("m1", RecordingKind::Personal).await.unwrap();
        db.set_meeting_notebook("m1", Some("Health")).await.unwrap();
        assert_eq!(
            db.get_kind_and_notebook("m1").await.unwrap(),
            (RecordingKind::Personal, Some("Health".to_string()))
        );
        let m1 = db.get_meeting("m1").await.unwrap().unwrap();
        assert_eq!(m1.recording_kind, "personal");
        let m2 = db.get_meeting("m2").await.unwrap().unwrap();
        assert_eq!(m2.recording_kind, "meeting");
        // A notebook alone never changes the type (no more "class means lecture")
        db.set_meeting_notebook("m2", Some("BIO 101")).await.unwrap();
        assert_eq!(db.get_kind_and_notebook("m2").await.unwrap().0, RecordingKind::Meeting);
        assert_eq!(db.get_kind_and_notebook("missing").await.unwrap(), (RecordingKind::Meeting, None));
        let _ = std::fs::remove_dir_all(dir);
    }
}
