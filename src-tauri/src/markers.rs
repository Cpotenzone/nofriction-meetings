//! Moment markers (docs/STUDY_TOOLS.md): while recording, the user marks a
//! moment as ★ Important, ? Question or ✎ (labelled by the recording's
//! type: On the test for a class, Follow up for a meeting, Remember for
//! personal; stored as `test` for all three), with an optional short note.
//! One tap marks ★; the kind and note can be changed afterwards.
//!
//! Markers live in `meeting_markers` (this module owns the table) and go
//! with their meeting (`ON DELETE CASCADE`). A note is user content: a
//! time-range Delete/Strike removes the markers inside the range
//! (`redaction::time_range::find_range_extras` / `purge_range_extras`, at
//! commit, so the 5-second undo keeps them). Word and line edits leave
//! markers alone, like user comments (docs/REDACTION.md).
//!
//! Notes and marker text are never logged.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnection;
use sqlx::{Pool, Row, Sqlite};

/// Stored kind values, in the order the UI offers them.
pub const KINDS: &[&str] = &["important", "question", "test"];
/// What a one-tap mark is.
pub const DEFAULT_KIND: &str = "important";
/// Long enough for "ask about the second proof", short enough to stay a note.
pub const MAX_NOTE_CHARS: usize = 280;
/// A mark this soon after the previous one in the same meeting is the same
/// mark (the hotkey and the menu accelerator, or a double tap).
pub const DEBOUNCE_MS: i64 = 800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub id: String,
    pub meeting_id: String,
    /// Wall clock, RFC 3339 (same clock as transcript lines and screens)
    pub ts: String,
    /// "important" | "question" | "test"
    pub kind: String,
    pub note: Option<String>,
    pub created_at: String,
    /// ms from the meeting start: the Recordings timeline's clock
    pub offset_ms: i64,
}

/// ★ / ? / ✎, for prompts and exports.
pub fn symbol(kind: &str) -> &'static str {
    match kind {
        "question" => "?",
        "test" => "✎",
        _ => "★",
    }
}

/// The kind's label in a recording of type `rec`: the stored `test` kind is
/// "On the test" in a class, "Follow up" in a meeting, "Remember" in a
/// personal recording.
pub fn label(kind: &str, rec: crate::recording_kind::RecordingKind) -> &'static str {
    match kind {
        "question" => "Question",
        "test" => rec.third_mark_label(),
        _ => "Important",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_markers (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            ts TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('important', 'question', 'test')),
            note TEXT,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_meeting_markers_meeting ON meeting_markers(meeting_id, ts)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Validation (pure)
// ═══════════════════════════════════════════════════════════════════════════

/// `None` → ★ (a one-tap mark). Unknown kinds are refused.
pub fn normalize_kind(kind: Option<&str>) -> Result<&'static str, String> {
    match kind.map(str::trim).filter(|k| !k.is_empty()) {
        None => Ok(DEFAULT_KIND),
        Some(k) => KINDS
            .iter()
            .find(|x| x.eq_ignore_ascii_case(k))
            .copied()
            .ok_or_else(|| "Unknown marker type".to_string()),
    }
}

/// Trimmed, whitespace runs closed up to one space, at most
/// [`MAX_NOTE_CHARS`]; empty → no note.
pub fn clean_note(note: Option<&str>) -> Result<Option<String>, String> {
    let Some(n) = note else { return Ok(None) };
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() {
        return Ok(None);
    }
    if n.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("Keep the note under {} characters", MAX_NOTE_CHARS));
    }
    Ok(Some(n))
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

// ═══════════════════════════════════════════════════════════════════════════
// Store
// ═══════════════════════════════════════════════════════════════════════════

async fn meeting_start(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<DateTime<Utc>, String> {
    let s: Option<String> = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(err("Database busy"))?;
    s.as_deref().and_then(parse_ts).ok_or_else(|| "That meeting no longer exists".to_string())
}

fn from_row(r: &sqlx::sqlite::SqliteRow, start: Option<DateTime<Utc>>) -> Marker {
    let ts: String = r.get("ts");
    let offset_ms = match (parse_ts(&ts), start) {
        (Some(t), Some(s)) => (t - s).num_milliseconds().max(0),
        _ => 0,
    };
    Marker {
        id: r.get("id"),
        meeting_id: r.get("meeting_id"),
        ts,
        kind: r.get("kind"),
        note: r.get("note"),
        created_at: r.get("created_at"),
        offset_ms,
    }
}

async fn get(pool: &Pool<Sqlite>, id: &str) -> Result<Marker, String> {
    let row = sqlx::query(
        "SELECT k.id, k.meeting_id, k.ts, k.kind, k.note, k.created_at, m.started_at \
         FROM meeting_markers k JOIN meetings m ON m.id = k.meeting_id WHERE k.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(err("Database busy"))?
    .ok_or("That marker no longer exists")?;
    let start = parse_ts(&row.get::<String, _>("started_at"));
    Ok(from_row(&row, start))
}

/// Add a marker at `ts` (wall clock). With `debounce`, a mark within
/// [`DEBOUNCE_MS`] after the meeting's latest one returns that one instead.
pub async fn add(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    ts: DateTime<Utc>,
    kind: Option<&str>,
    note: Option<&str>,
    debounce: bool,
) -> Result<Marker, String> {
    let kind = normalize_kind(kind)?;
    let note = clean_note(note)?;
    meeting_start(pool, meeting_id).await?;
    if debounce {
        let last: Option<(String, String)> =
            sqlx::query_as("SELECT id, ts FROM meeting_markers WHERE meeting_id = ? ORDER BY ts DESC LIMIT 1")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await
                .map_err(err("Database busy"))?;
        if let Some((id, last_ts)) = last {
            if parse_ts(&last_ts).map_or(false, |t| (ts - t).num_milliseconds().abs() < DEBOUNCE_MS) {
                return get(pool, &id).await;
            }
        }
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO meeting_markers (id, meeting_id, ts, kind, note, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(meeting_id)
        .bind(ts.to_rfc3339())
        .bind(kind)
        .bind(&note)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .map_err(err("Couldn't save the marker"))?;
    get(pool, &id).await
}

/// Add a marker `offset_ms` after the meeting start (marking while
/// reviewing a recording).
pub async fn add_at_offset(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    offset_ms: i64,
    kind: Option<&str>,
    note: Option<&str>,
) -> Result<Marker, String> {
    let start = meeting_start(pool, meeting_id).await?;
    add(pool, meeting_id, start + chrono::Duration::milliseconds(offset_ms.max(0)), kind, note, false).await
}

/// Change a marker's kind and/or note. `note: Some(None)` (or an empty
/// note) removes the note; `None` keeps it.
pub async fn update(
    pool: &Pool<Sqlite>,
    id: &str,
    kind: Option<&str>,
    note: Option<Option<&str>>,
) -> Result<Marker, String> {
    let current = get(pool, id).await?;
    let kind = match kind {
        Some(k) => normalize_kind(Some(k))?.to_string(),
        None => current.kind.clone(),
    };
    let note = match note {
        Some(n) => clean_note(n)?,
        None => current.note.clone(),
    };
    sqlx::query("UPDATE meeting_markers SET kind = ?, note = ? WHERE id = ?")
        .bind(&kind)
        .bind(&note)
        .bind(id)
        .execute(pool)
        .await
        .map_err(err("Couldn't save the marker"))?;
    get(pool, id).await
}

pub async fn delete(pool: &Pool<Sqlite>, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM meeting_markers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(err("Couldn't delete the marker"))?;
    Ok(())
}

/// A meeting's markers in time order.
pub async fn list(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<Vec<Marker>, String> {
    let start = parse_ts(
        &sqlx::query_scalar::<_, String>("SELECT started_at FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(err("Database busy"))?
            .unwrap_or_default(),
    );
    let rows = sqlx::query(
        "SELECT id, meeting_id, ts, kind, note, created_at FROM meeting_markers WHERE meeting_id = ? ORDER BY ts ASC, created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(err("Database busy"))?;
    Ok(rows.iter().map(|r| from_row(r, start)).collect())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri commands
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use tauri::{AppHandle, Emitter, Manager, State};

    /// Event carrying the new [`Marker`] (CaptureBar shows it and offers the
    /// kind/note chooser; the Recordings view refreshes).
    pub const MARKER_ADDED_EVENT: &str = "marker_added";
    /// Event when a mark was asked for but nothing is being recorded.
    pub const MARKER_FAILED_EVENT: &str = "marker_failed";

    /// The meeting being recorded right now, if any.
    fn recording_meeting(state: &AppState) -> Option<String> {
        if !state.capture_engine.read().is_recording() {
            return None;
        }
        state.state_builder.read().current_meeting_id()
    }

    /// Mark now in the meeting being recorded (Mark button, menu item and
    /// the global hotkey). Debounced, so the hotkey and the menu accelerator
    /// firing for one key press make one marker.
    pub async fn mark_now(
        app: &AppHandle,
        state: &AppState,
        kind: Option<&str>,
        note: Option<&str>,
    ) -> Result<Marker, String> {
        let meeting_id = recording_meeting(state).ok_or("Start a recording to mark a moment.")?;
        let m = add(state.database.pool(), &meeting_id, Utc::now(), kind, note, true).await?;
        let _ = app.emit(MARKER_ADDED_EVENT, &m);
        Ok(m)
    }

    /// Global hotkey / menu: mark ★ in the meeting being recorded.
    pub fn mark_from_shortcut(app: &AppHandle) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Some(state) = app.try_state::<AppState>() else { return };
            if let Err(e) = mark_now(&app, &state, None, None).await {
                let _ = app.emit(MARKER_FAILED_EVENT, e);
            }
        });
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn mark_moment(
        app: AppHandle,
        state: State<'_, AppState>,
        kind: Option<String>,
        note: Option<String>,
    ) -> Result<Marker, String> {
        mark_now(&app, &state, kind.as_deref(), note.as_deref()).await
    }

    /// Mark a moment of a recorded meeting (Recordings view).
    #[tauri::command(rename_all = "camelCase")]
    pub async fn add_marker(
        state: State<'_, AppState>,
        meeting_id: String,
        offset_ms: i64,
        kind: Option<String>,
        note: Option<String>,
    ) -> Result<Marker, String> {
        add_at_offset(state.database.pool(), &meeting_id, offset_ms, kind.as_deref(), note.as_deref()).await
    }

    /// `clearNote: true` removes the note; otherwise `note` replaces it when given.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn update_marker(
        state: State<'_, AppState>,
        id: String,
        kind: Option<String>,
        note: Option<String>,
        clear_note: Option<bool>,
    ) -> Result<Marker, String> {
        let note_arg: Option<Option<&str>> = if clear_note.unwrap_or(false) {
            Some(None)
        } else {
            note.as_deref().map(Some)
        };
        update(state.database.pool(), &id, kind.as_deref(), note_arg).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_marker(state: State<'_, AppState>, id: String) -> Result<(), String> {
        delete(state.database.pool(), &id).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_markers(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<Marker>, String> {
        list(state.database.pool(), &meeting_id).await
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Global hotkey
// ═══════════════════════════════════════════════════════════════════════════

/// ⌃⌥⌘M marks ★ while another app (slides, a video call, a browser) is in
/// front. Chosen not to collide: the app menu uses ⌘, ⌘N ⌘. ⌘1 ⌘2 ⌘⇧I ⌘⇧P
/// ⌘K, the tray ⌘⇧N, the capture bar ⌘⇧S; macOS itself uses ⌥⌘M (minimize
/// all) and VoiceOver ⌃⌥ + letter, and ⌃⌥⌘M is unused. The same key is the
/// File → Mark Moment accelerator when the app is in front (debounced, so
/// the two never make two markers).
pub fn hotkey() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER), Code::KeyM)
}

/// The same key as a menu accelerator.
pub const MENU_ACCELERATOR: &str = "Ctrl+Alt+Cmd+M";

/// Register the global ⌃⌥⌘M at launch. If another app owns the key, the
/// app still starts (File → Mark Moment and the Mark button work); the
/// failure is logged. (Registering inside the plugin builder instead would
/// make a taken key fail the whole app's setup.)
pub fn register_hotkey(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let result = app.global_shortcut().on_shortcut(hotkey(), |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            commands::mark_from_shortcut(app);
        }
    });
    match result {
        Ok(()) => log::info!("Mark hotkey ⌃⌥⌘M registered"),
        Err(e) => log::warn!("Mark hotkey ⌃⌥⌘M not available ({}); File → Mark Moment still works", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseManager;

    async fn db() -> (DatabaseManager, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nf-markers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = DatabaseManager::new(&dir.join("t.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        db.create_meeting("m1", "Biology 101").await.unwrap();
        (db, dir)
    }

    #[test]
    fn kinds_and_notes_are_validated() {
        assert_eq!(normalize_kind(None).unwrap(), "important");
        assert_eq!(normalize_kind(Some("  ")).unwrap(), "important");
        assert_eq!(normalize_kind(Some("TEST")).unwrap(), "test");
        assert_eq!(normalize_kind(Some("question")).unwrap(), "question");
        assert!(normalize_kind(Some("urgent")).is_err());
        assert_eq!(clean_note(Some("  ask   about\n proof 2 ")).unwrap().as_deref(), Some("ask about proof 2"));
        assert_eq!(clean_note(Some("   ")).unwrap(), None);
        assert_eq!(clean_note(None).unwrap(), None);
        assert!(clean_note(Some(&"x".repeat(MAX_NOTE_CHARS + 1))).is_err());
        assert!(clean_note(Some(&"é".repeat(MAX_NOTE_CHARS))).unwrap().is_some(), "counts characters, not bytes");
        assert_eq!((symbol("important"), symbol("question"), symbol("test")), ("★", "?", "✎"));
    }

    #[test]
    fn labels_follow_the_recording_type_and_the_stored_kind_does_not() {
        use crate::recording_kind::RecordingKind::*;
        assert_eq!(label("test", Class), "On the test");
        assert_eq!(label("test", Meeting), "Follow up");
        assert_eq!(label("test", Personal), "Remember");
        for rec in [Class, Meeting, Personal] {
            assert_eq!(label("important", rec), "Important");
            assert_eq!(label("question", rec), "Question");
        }
        // One stored value for the third mark, whatever the type
        assert_eq!(KINDS, &["important", "question", "test"]);
    }

    #[tokio::test]
    async fn marker_crud_and_offsets() {
        let (db, dir) = db().await;
        let pool = db.pool();
        let start = parse_ts(&sqlx::query_scalar::<_, String>("SELECT started_at FROM meetings WHERE id = 'm1'")
            .fetch_one(pool)
            .await
            .unwrap())
        .unwrap();

        // One tap: ★, no note
        let a = add(pool, "m1", start + chrono::Duration::seconds(90), None, None, false).await.unwrap();
        assert_eq!((a.kind.as_str(), a.note.as_deref(), a.offset_ms), ("important", None, 90_000));
        // Typed, with a note; and one added while reviewing, by offset
        let b = add(pool, "m1", start + chrono::Duration::seconds(30), Some("question"), Some("why?"), false)
            .await
            .unwrap();
        let c = add_at_offset(pool, "m1", 600_000, Some("test"), None).await.unwrap();
        assert_eq!(c.offset_ms, 600_000);
        let all = list(pool, "m1").await.unwrap();
        assert_eq!(all.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), vec![b.id.clone(), a.id.clone(), c.id.clone()]);

        // Change the type afterwards; set, keep and clear the note
        let a2 = update(pool, &a.id, Some("test"), None).await.unwrap();
        assert_eq!((a2.kind.as_str(), a2.note.as_deref()), ("test", None));
        let a3 = update(pool, &a.id, None, Some(Some("chapter 4"))).await.unwrap();
        assert_eq!((a3.kind.as_str(), a3.note.as_deref()), ("test", Some("chapter 4")));
        let a4 = update(pool, &a.id, Some("important"), None).await.unwrap();
        assert_eq!(a4.note.as_deref(), Some("chapter 4"), "a kind change keeps the note");
        let a5 = update(pool, &a.id, None, Some(None)).await.unwrap();
        assert_eq!(a5.note, None);
        assert!(update(pool, &a.id, Some("nope"), None).await.is_err());

        delete(pool, &b.id).await.unwrap();
        assert_eq!(list(pool, "m1").await.unwrap().len(), 2);
        assert!(update(pool, &b.id, Some("test"), None).await.is_err());
        // Unknown meeting
        assert!(add(pool, "nope", Utc::now(), None, None, false).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn debounce_folds_a_double_press_into_one_marker() {
        let (db, dir) = db().await;
        let pool = db.pool();
        let t = Utc::now();
        let a = add(pool, "m1", t, None, None, true).await.unwrap();
        let b = add(pool, "m1", t + chrono::Duration::milliseconds(300), None, None, true).await.unwrap();
        assert_eq!(a.id, b.id);
        let c = add(pool, "m1", t + chrono::Duration::milliseconds(DEBOUNCE_MS + 50), None, None, true).await.unwrap();
        assert_ne!(a.id, c.id);
        assert_eq!(list(pool, "m1").await.unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn meeting_delete_removes_its_markers() {
        let (db, dir) = db().await;
        let pool = db.pool();
        db.create_meeting("m2", "Other").await.unwrap();
        add(pool, "m1", Utc::now(), Some("question"), Some("private note"), false).await.unwrap();
        add(pool, "m2", Utc::now(), None, None, false).await.unwrap();
        db.delete_meeting("m1").await.unwrap();
        let left: Vec<String> = sqlx::query_scalar("SELECT meeting_id FROM meeting_markers").fetch_all(pool).await.unwrap();
        assert_eq!(left, vec!["m2".to_string()]);
        let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_markers WHERE note = 'private note'")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(notes, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn schema_rejects_unknown_kinds() {
        let (db, dir) = db().await;
        let r = sqlx::query(
            "INSERT INTO meeting_markers (id, meeting_id, ts, kind, note, created_at) VALUES ('x', 'm1', ?, 'urgent', NULL, ?)",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(db.pool())
        .await;
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
