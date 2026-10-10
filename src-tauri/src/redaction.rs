//! Editing transcripts/screens and "Strike from the record".
//!
//! Shared spec with the iOS app: `docs/REDACTION.md`. Two actions:
//!
//! - **Delete**: removes the content everywhere and leaves no trace. It sits in
//!   a 5-second undo window first. The pending delete is persisted as a
//!   `redactions` row (`pending_payload` holds ids/offsets, never content) so a
//!   quit or crash inside the window commits it on the next exit/launch
//!   instead of silently dropping it.
//! - **Strike**: removes the content everywhere immediately, with no undo, and
//!   leaves an immutable marker record (when / where / optional reason, never
//!   what). In transcript text the marker is the token `⟦strickenid<32 hex>⟧`;
//!   plain-text consumers (AI prompts, exports, search) see
//!   `[stricken from the record]` via [`render_plain`].
//!
//! Purge checklist coverage on the Mac (see the spec for the full list):
//! transcript text + word timings, FTS (update trigger + `optimize`), screens
//! (image files, OCR/accessibility snapshots, VLM activity rows, timeline
//! rows, cached video frames/thumbnails), DMG screen video (blanked with black
//! frames via ffmpeg by a durable background job, see [`video_jobs`]), AI
//! outputs (redacted + "made before an edit" flag), app log files, app
//! `backups/` databases (purged or deleted), and freed space
//! (`secure_delete` + `wal_checkpoint(TRUNCATE)`). The Mac stores no meeting
//! audio, so there is nothing to silence.
//!
//! The app-wide [`LOCK`] covers database work only. Screen video blanking
//! (minutes of ffmpeg for a long meeting) runs in the job worker outside it,
//! so a video job never makes another delete, undo or strike wait.
//!
//! Time ranges ("delete everything from 10:41 to 10:53", or several at
//! once from a linked selection of screens and lines) are in [`time_range`].

#[cfg(not(feature = "mas"))]
pub mod video_blank;
pub mod time_range;
pub mod video_jobs;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{ConnectOptions, Connection, Pool, Row, Sqlite};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const STRICKEN_PLACEHOLDER: &str = "[stricken from the record]";
pub const SCREEN_STRICKEN_PLACEHOLDER: &str = "[screen stricken from the record]";
/// What a Delete leaves in AI outputs that quoted the removed words.
pub const DELETED_PLACEHOLDER: &str = "[removed]";
pub const UNDO_WINDOW_SECS: u64 = 5;
pub const RECORDING_SCREENS_ERROR: &str = "Stop the recording to edit its screens.";
pub const MAX_REASON_CHARS: usize = 120;

/// Lines users see in the Strike confirmation regardless of what's selected.
pub const EXPORTS_NOTICE: &str =
    "Files already exported outside the app (Obsidian, Markdown, shares) can't be recalled.";
pub const DEVICE_BACKUP_NOTICE: &str =
    "Time Machine and other device backups are outside the app's control.";
pub const NO_AUDIO_NOTICE: &str =
    "No meeting audio is stored on this Mac, so there is no recording to silence.";

static MARKER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"⟦?strickenid([0-9a-f]{32})⟧?").expect("marker regex"));
static FULL_MARKER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^⟦strickenid([0-9a-f]{32})⟧$").expect("marker regex"));

/// Serializes every redaction action (requests, commits, undo) app-wide so a
/// timer commit can't race an undo or a second edit of the same line.
static LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// The token spliced into transcript text where words were stricken. It is
/// one FTS token (no separators inside) and carries only the record id.
pub fn marker_token(id: &str) -> String {
    format!("⟦strickenid{}⟧", id)
}

/// Replace strike markers with `[stricken from the record]` for every
/// plain-text consumer (AI prompts, exports, search results).
pub fn render_plain(text: &str) -> String {
    if !text.contains("strickenid") {
        return text.to_string();
    }
    MARKER_RE.replace_all(text, STRICKEN_PLACEHOLDER).into_owned()
}

/// Record ids of the strike markers in a line, in order.
pub fn marker_ids(text: &str) -> Vec<String> {
    MARKER_RE
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS redactions (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK (kind IN ('words', 'line', 'screen')),
            action TEXT NOT NULL CHECK (action IN ('delete', 'strike')),
            media_start TEXT,
            media_end TEXT,
            created_at TEXT NOT NULL,
            reason TEXT,
            -- Mac extras: which line holds the marker; how many screens
            transcript_id INTEGER,
            item_count INTEGER NOT NULL DEFAULT 1,
            -- Delete only, during the 5s undo window: ids/offsets (never
            -- content). The row is removed when the delete commits.
            pending_payload TEXT
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_redactions_meeting ON redactions(meeting_id)")
        .execute(&mut *conn)
        .await?;
    // Strike records are never editable, and only go away with their meeting
    // (the FK cascade runs after the parent row is gone, so EXISTS is false).
    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS redactions_strike_no_update
        BEFORE UPDATE ON redactions WHEN old.action = 'strike'
        BEGIN SELECT RAISE(ABORT, 'stricken-from-the-record markers cannot be edited'); END
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS redactions_strike_no_delete
        BEFORE DELETE ON redactions
        WHEN old.action = 'strike' AND EXISTS (SELECT 1 FROM meetings WHERE id = old.meeting_id)
        BEGIN SELECT RAISE(ABORT, 'stricken-from-the-record markers cannot be removed'); END
        "#,
    )
    .execute(&mut *conn)
    .await?;
    // A pending Delete that failed for a transient reason (database busy,
    // the meeting is recording) stays pending, marked failed with a reason,
    // and is retried on the next commit and at launch.
    crate::database::ensure_columns(conn, "redactions", &[("failed_at", "TEXT"), ("failure", "TEXT")]).await?;
    // "Made before an edit, regenerate?" flag on saved AI outputs
    for table in ["meeting_notes", "study_materials", "assistant_conversations"] {
        crate::database::ensure_columns(conn, table, &[("stale_after_edit", "INTEGER NOT NULL DEFAULT 0")]).await?;
    }
    // Screen video blanking queue (runs after the database purge)
    video_jobs::ensure_schema(conn).await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Records
// ═══════════════════════════════════════════════════════════════════════════

/// A marker record. Holds when/where/why, never what.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RedactionRecord {
    pub id: String,
    pub meeting_id: String,
    pub kind: String,
    pub action: String,
    pub media_start: Option<String>,
    pub media_end: Option<String>,
    pub created_at: String,
    pub reason: Option<String>,
    pub transcript_id: Option<i64>,
    pub item_count: i64,
    /// The screen video for this span is still being blanked in the
    /// background (derived from `video_blank_jobs`; strike records
    /// themselves are immutable).
    #[serde(default)]
    pub video_pending: bool,
}

fn record_from_row(r: &sqlx::sqlite::SqliteRow) -> RedactionRecord {
    RedactionRecord {
        id: r.get("id"),
        meeting_id: r.get("meeting_id"),
        kind: r.get("kind"),
        action: r.get("action"),
        media_start: r.get("media_start"),
        media_end: r.get("media_end"),
        created_at: r.get("created_at"),
        reason: r.get("reason"),
        transcript_id: r.get("transcript_id"),
        item_count: r.get("item_count"),
        video_pending: r.try_get::<i64, _>("video_pending").map(|v| v != 0).unwrap_or(false),
    }
}

/// Strike markers for a meeting, oldest first. Committed deletes leave no
/// record, and pending ones are not markers, so neither is listed.
pub async fn list_strikes(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
) -> Result<Vec<RedactionRecord>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, kind, action, media_start, media_end, created_at, reason, \
         transcript_id, item_count, \
         EXISTS (SELECT 1 FROM video_blank_jobs j WHERE j.redaction_id = redactions.id) AS video_pending \
         FROM redactions WHERE meeting_id = ? AND action = 'strike' ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(record_from_row).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Delete,
    Strike,
}

impl Action {
    fn replacement(self) -> &'static str {
        match self {
            Action::Delete => DELETED_PLACEHOLDER,
            Action::Strike => STRICKEN_PLACEHOLDER,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Word-range text edits (pure)
// ═══════════════════════════════════════════════════════════════════════════

/// One word's timing: UTF-16 offsets into the line text and milliseconds
/// relative to the line timestamp. No word text, so timings never hold
/// removed content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WordTiming {
    pub s: usize,
    pub e: usize,
    pub t0: i64,
    pub t1: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    pub new_text: String,
    /// The removed words (for redacting AI outputs/backups/logs). Held in
    /// memory only for the duration of the action.
    pub removed_plain: String,
    pub new_timings: Option<String>,
    /// (min t0, max t1) of the removed words, if timings were stored
    pub removed_ms: Option<(i64, i64)>,
    pub whole_line: bool,
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// UTF-16 code-unit offset (what JS string indexes are) → byte offset.
fn utf16_to_byte(text: &str, off: usize) -> Option<usize> {
    let mut u = 0usize;
    for (b, c) in text.char_indices() {
        if u == off {
            return Some(b);
        }
        u += c.len_utf16();
        if u > off {
            return None;
        }
    }
    (u == off).then_some(text.len())
}

/// Remove the words covering UTF-16 range `[start, end)` from `text`. The
/// range snaps outward to whole whitespace-delimited words, so no fragment
/// of a word survives. Existing strike markers inside the range are kept
/// (strikes can't be undone by a later edit). With `marker`, it is spliced
/// in where the removed words were. Whitespace closes up to single spaces.
pub fn apply_word_edit(
    text: &str,
    timings_json: Option<&str>,
    start: usize,
    end: usize,
    marker: Option<&str>,
) -> Result<TextEdit, String> {
    let mut s = utf16_to_byte(text, start).ok_or("Selection start is outside the line")?;
    let mut e = utf16_to_byte(text, end).ok_or("Selection end is outside the line")?;
    if s >= e {
        return Err("Nothing selected".into());
    }
    while s > 0 {
        let prev = text[..s].chars().next_back().unwrap_or(' ');
        if prev.is_whitespace() {
            break;
        }
        s -= prev.len_utf8();
    }
    while e < text.len() {
        let next = text[e..].chars().next().unwrap_or(' ');
        if next.is_whitespace() {
            break;
        }
        e += next.len_utf8();
    }
    let inner = &text[s..e];
    s += inner.len() - inner.trim_start().len();
    e -= inner.len() - inner.trim_end().len();
    if s >= e {
        return Err("The selection has no words".into());
    }

    let mut middle: Vec<String> = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    let mut placed = false;
    for tok in text[s..e].split_whitespace() {
        if FULL_MARKER_RE.is_match(tok) {
            middle.push(tok.to_string());
        } else {
            words.push(tok);
            if let (Some(m), false) = (marker, placed) {
                middle.push(m.to_string());
                placed = true;
            }
        }
    }
    if words.is_empty() {
        return Err("Those words are already stricken from the record".into());
    }
    let removed_plain = words.join(" ");

    let left = text[..s].trim_end();
    let right = text[e..].trim_start();
    let mid = middle.join(" ");
    let new_text = [left, mid.as_str(), right]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let whole_line = left.is_empty() && right.is_empty();

    // Word timings: drop removed words, shift the ones after the edit
    let mut new_timings = None;
    let mut removed_ms: Option<(i64, i64)> = None;
    if let Some(list) = timings_json.and_then(|j| serde_json::from_str::<Vec<WordTiming>>(j).ok()) {
        let s16 = utf16_len(&text[..s]);
        let e16 = utf16_len(&text[..e]);
        let left16 = utf16_len(left);
        let right_old16 = utf16_len(&text[..text.len() - right.len()]);
        let right_new16 = utf16_len(&new_text) - utf16_len(right);
        let mut kept = Vec::new();
        for w in list {
            if w.e <= s16 && w.e <= left16 {
                kept.push(w);
            } else if w.s >= e16 && w.s >= right_old16 {
                kept.push(WordTiming {
                    s: w.s - right_old16 + right_new16,
                    e: w.e - right_old16 + right_new16,
                    ..w
                });
            } else {
                removed_ms = Some(match removed_ms {
                    None => (w.t0, w.t1),
                    Some((a, b)) => (a.min(w.t0), b.max(w.t1)),
                });
            }
        }
        if !kept.is_empty() {
            new_timings = serde_json::to_string(&kept).ok();
        }
    }

    Ok(TextEdit { new_text, removed_plain, new_timings, removed_ms, whole_line })
}

/// Case-insensitive, whole-word matcher for removed text in AI outputs,
/// backups and logs. Edge punctuation is ignored ("plan." matches "plan").
pub fn phrase_regex(removed: &str) -> Option<Regex> {
    let p = removed.trim_matches(|c: char| !c.is_alphanumeric());
    if p.is_empty() {
        return None;
    }
    let parts: Vec<String> = p.split_whitespace().map(regex::escape).collect();
    Regex::new(&format!(r"(?i)\b{}\b", parts.join(r"\s+"))).ok()
}

/// Common long words that would still over-match as a single removed word.
const LONG_STOPWORDS: &[&str] = &[
    "across", "actually", "against", "almost", "already", "always", "another", "anybody", "anyone",
    "anything", "anyway", "around", "basically", "because", "become", "before", "behind", "believe",
    "better", "between", "beyond", "certain", "certainly", "change", "changes", "coming", "company",
    "couldn't", "definitely", "different", "doesn't", "during", "either", "enough", "especially",
    "everybody", "everyone", "everything", "exactly", "follow", "getting", "having", "honestly", "however",
    "important", "inside", "instead", "itself", "keeping", "literally", "little", "looking", "making",
    "meeting", "meetings", "minute", "minutes", "moment", "myself", "nobody", "nothing", "number",
    "obviously", "online", "others", "outside", "people", "perhaps", "please", "pretty", "probably",
    "problem", "question", "questions", "rather", "really", "reason", "saying", "second", "seconds",
    "should", "shouldn't", "similar", "simply", "something", "sometimes", "somewhere", "started", "talking",
    "thanks", "things", "thinking", "though", "thought", "through", "together", "tomorrow", "totally",
    "toward", "towards", "trying", "understand", "unless", "usually", "whatever", "whether", "within",
    "without", "wouldn't", "yesterday", "yourself", "themselves", "ourselves", "himself", "herself",
    "therefore", "although",
];

/// Whether removed text is specific enough to scrub *elsewhere* (AI outputs,
/// logs, backup verification). Rewriting every whole-word match of a common
/// word ("plan", "team", "because") across a meeting's notes would mangle
/// unrelated text, so only distinctive removals propagate: two or more
/// words, or one word of 6+ characters that isn't a common word. Anything
/// else only marks the AI outputs "made before an edit". Same rule on iOS
/// (`RedactionText.isDistinctive`), see docs/REDACTION.md.
pub fn is_distinctive(removed: &str) -> bool {
    let words: Vec<String> = removed
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [] => false,
        [w] => w.chars().count() >= 6 && !LONG_STOPWORDS.contains(&w.as_str()),
        _ => true,
    }
}

/// [`phrase_regex`], but only for distinctive removals (see [`is_distinctive`]).
pub fn distinctive_phrase_regex(removed: &str) -> Option<Regex> {
    if is_distinctive(removed) {
        phrase_regex(removed)
    } else {
        None
    }
}

fn redact_str(s: &str, re: &Regex, replacement: &str) -> Option<String> {
    if re.is_match(s) {
        Some(re.replace_all(s, regex::NoExpand(replacement)).into_owned())
    } else {
        None
    }
}

fn redact_json(v: &mut serde_json::Value, re: &Regex, replacement: &str) -> bool {
    match v {
        serde_json::Value::String(s) => match redact_str(s, re, replacement) {
            Some(n) => {
                *s = n;
                true
            }
            None => false,
        },
        serde_json::Value::Array(a) => {
            let mut changed = false;
            for x in a.iter_mut() {
                changed |= redact_json(x, re, replacement);
            }
            changed
        }
        serde_json::Value::Object(o) => {
            let mut changed = false;
            for (_, x) in o.iter_mut() {
                changed |= redact_json(x, re, replacement);
            }
            changed
        }
        _ => false,
    }
}

/// Redact a stored value that may be JSON (arrays/objects of strings) or prose.
fn redact_value(s: &str, re: &Regex, replacement: &str) -> Option<String> {
    let t = s.trim_start();
    if t.starts_with('[') || t.starts_with('{') {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(s) {
            return redact_json(&mut v, re, replacement).then(|| v.to_string());
        }
    }
    redact_str(s, re, replacement)
}

/// The reason is shown on the marker, so it must not carry the removed text.
pub fn validate_reason(reason: Option<&str>, removed: Option<&str>) -> Result<Option<String>, String> {
    let Some(r) = reason.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    if r.chars().count() > MAX_REASON_CHARS {
        return Err(format!("Keep the reason under {} characters", MAX_REASON_CHARS));
    }
    if let Some(removed) = removed {
        let hit = phrase_regex(removed).map(|re| re.is_match(r)).unwrap_or(false)
            || removed
                .split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|w| w.chars().count() >= 4)
                .any(|w| phrase_regex(w).map(|re| re.is_match(r)).unwrap_or(false));
        if hit {
            return Err("The reason can't contain the words being removed".into());
        }
    }
    Ok(Some(r.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Environment (paths), so the purge is testable without a running app
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct RedactionEnv {
    /// `paths::app_data_dir()`: frames/, backups/, logs/, <meeting>/video/
    pub app_data_dir: PathBuf,
    /// `paths::app_cache_dir()`: <meeting>/frames, <meeting>/thumbnails
    pub cache_dir: PathBuf,
    /// DMG only: blank screen video chunks with ffmpeg
    pub video_enabled: bool,
    /// Meetings being recorded right now (screens still being captured, and
    /// on the DMG the screen video still being written). Their screens can't
    /// be deleted or stricken until the recording stops.
    pub recording_meetings: Vec<String>,
}

impl RedactionEnv {
    pub fn for_app() -> Self {
        Self {
            app_data_dir: crate::paths::app_data_dir(),
            cache_dir: crate::paths::app_cache_dir(),
            video_enabled: cfg!(not(feature = "mas")),
            recording_meetings: Vec::new(),
        }
    }
    fn ensure_not_recording(&self, meeting_id: &str) -> Result<(), String> {
        if self.recording_meetings.iter().any(|m| m == meeting_id) {
            return Err(RECORDING_SCREENS_ERROR.into());
        }
        Ok(())
    }
    fn backups_dir(&self) -> PathBuf {
        self.app_data_dir.join("backups")
    }
    fn logs_dir(&self) -> PathBuf {
        self.app_data_dir.join("logs")
    }
}

/// Result of a committed action. `warnings` are post-commit steps that
/// could not fully complete (shown to the user, never swallowed).
#[derive(Debug, Clone, Serialize, Default)]
pub struct ActionOutcome {
    /// The Strike marker (for a time range: the transcript marker if any
    /// lines were stricken, else the screen marker)
    pub record: Option<RedactionRecord>,
    /// Every marker the action created (a time range can make two: one in
    /// the transcript, one in the screen strip)
    pub records: Vec<RedactionRecord>,
    pub warnings: Vec<String>,
    pub backups_purged: usize,
    pub backups_deleted: usize,
    /// Screen video ranges queued for background blanking (DMG). The
    /// database content is already gone; the video follows.
    pub video_jobs_queued: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingDelete {
    pub id: String,
    pub meeting_id: String,
    pub kind: String,
    pub undo_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PendingPayload {
    Words { transcript_id: i64, start: usize, end: usize, text_hash: String },
    Screens { ids: Vec<String> },
    /// Blocks of meeting time, resolved when requested (ids/offsets/hashes,
    /// never content) so exactly what the preview counted is removed.
    /// `start`/`end` span them all; `ranges` lists each one (empty in rows
    /// from before multi-range: the one range is `start`–`end`).
    TimeRange {
        start: String,
        end: String,
        screen_ids: Vec<String>,
        lines: Vec<time_range::RangeLine>,
        #[serde(default)]
        ranges: Vec<(String, String)>,
    },
}

fn err<E: std::fmt::Display>(ctx: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{}: {}", ctx, e)
}

// ═══════════════════════════════════════════════════════════════════════════
// DB helpers (schema-tolerant so the same code purges older backup files)
// ═══════════════════════════════════════════════════════════════════════════

async fn table_names(conn: &mut SqliteConnection) -> Result<HashSet<String>, sqlx::Error> {
    let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type IN ('table', 'view')")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("name")).collect())
}

async fn has_column(conn: &mut SqliteConnection, table: &str, column: &str) -> bool {
    sqlx::query("SELECT 1 FROM pragma_table_info(?) WHERE name = ?")
        .bind(table)
        .bind(column)
        .fetch_optional(&mut *conn)
        .await
        .ok()
        .flatten()
        .is_some()
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// AI outputs that were generated from (or quote) a meeting.
struct AiTable {
    table: &'static str,
    key: &'static str,
    cols: &'static [&'static str],
    flag: bool,
}

const AI_TABLES: &[AiTable] = &[
    AiTable {
        table: "meeting_notes",
        key: "id",
        cols: &["summary", "key_topics", "decisions", "action_items", "participants"],
        flag: true,
    },
    // study_materials are deleted outright (study::purge_for_meeting, in
    // redact_ai_outputs): a study guide paraphrases the lecture, so
    // matching the removed words can't clean it.
    // meeting_comments are deliberately absent: they're user-authored, so
    // an edit never rewrites them (the user can edit their own comment).
    AiTable {
        table: "meeting_timeline_events",
        key: "event_id",
        cols: &["title", "description", "topic"],
        flag: false,
    },
    AiTable { table: "topic_clusters", key: "topic_id", cols: &["name", "description"], flag: false },
];

/// Purge step 5: redact every occurrence of the removed words (whole-word,
/// case-insensitive) in this meeting's saved AI outputs and the assistant
/// chats that used it, and set the "made before an edit" flag on them.
/// Only distinctive removals are rewritten ([`is_distinctive`]); for a
/// common word the outputs are only flagged. The meeting's study materials
/// are deleted (any removal). User comments are never rewritten. Returns
/// how many stored values changed.
pub async fn redact_ai_outputs(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    removed: &str,
    replacement: &str,
) -> Result<u64, sqlx::Error> {
    let mut changed = crate::study::purge_for_meeting(conn, meeting_id).await?;
    // AI topics name what the transcript was about, and chat answers quote
    // it: both are deleted outright; the user's own topics stay
    // (docs/TOPICS_AND_CHAT.md)
    changed += crate::topics::purge_ai_for_meeting(conn, meeting_id).await?;
    changed += crate::chat::purge_for_meeting(conn, meeting_id).await?;
    let tables = table_names(conn).await?;
    let re = distinctive_phrase_regex(removed);

    let mut targets: Vec<(&str, &str, Vec<&str>, String, bool)> = Vec::new();
    for t in AI_TABLES {
        if tables.contains(t.table) {
            targets.push((t.table, t.key, t.cols.to_vec(), "meeting_id = ?".into(), t.flag));
        }
    }
    if tables.contains("assistant_conversations") {
        targets.push((
            "assistant_conversations",
            "id",
            vec!["user_query", "assistant_response"],
            "context_refs LIKE ?".into(),
            true,
        ));
    }

    for (table, key, cols, filter, flag) in targets {
        let bind_val = if table == "assistant_conversations" {
            format!("%transcript-{}-%", meeting_id)
        } else {
            meeting_id.to_string()
        };
        let mut present = Vec::new();
        for c in &cols {
            if has_column(conn, table, c).await {
                present.push(*c);
            }
        }
        if let Some(re) = &re {
            if !present.is_empty() {
                let sql = format!(
                    "SELECT CAST({} AS TEXT) AS k, {} FROM {} WHERE {}",
                    key,
                    present.iter().map(|c| format!("CAST({0} AS TEXT) AS {0}", c)).collect::<Vec<_>>().join(", "),
                    table,
                    filter
                );
                let rows = sqlx::query(&sql).bind(&bind_val).fetch_all(&mut *conn).await?;
                for r in rows {
                    let k: String = r.get("k");
                    for c in &present {
                        let v: Option<String> = r.get(*c);
                        if let Some(new) = v.as_deref().and_then(|v| redact_value(v, re, replacement)) {
                            sqlx::query(&format!("UPDATE {} SET {} = ? WHERE CAST({} AS TEXT) = ?", table, c, key))
                                .bind(new)
                                .bind(&k)
                                .execute(&mut *conn)
                                .await?;
                            changed += 1;
                        }
                    }
                }
            }
        }
        if flag && has_column(conn, table, "stale_after_edit").await {
            sqlx::query(&format!("UPDATE {} SET stale_after_edit = 1 WHERE {}", table, filter))
                .bind(&bind_val)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(changed)
}

/// Lines in the meeting whose text still contains the removed phrase.
async fn count_phrase_in_meeting(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    re: &Regex,
) -> Result<usize, sqlx::Error> {
    let rows = sqlx::query("SELECT text FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.iter().filter(|r| re.is_match(&r.get::<String, _>("text"))).count())
}

// ── Screens ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ScreenSource {
    Frame(i64),
    State(String),
}

#[derive(Debug, Clone)]
struct ScreenInfo {
    source: ScreenSource,
    file: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    start_raw: Option<String>,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

/// Resolve timeline ids (frames.id as a number, or screen_states.state_id)
/// within one meeting. Unknown ids are an error unless `lenient` (backups).
async fn resolve_screens(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    ids: &[String],
    lenient: bool,
) -> Result<Vec<ScreenInfo>, String> {
    let tables = table_names(conn).await.map_err(err("Failed to read schema"))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let (Ok(n), true) = (id.parse::<i64>(), tables.contains("frames")) {
            let row = sqlx::query("SELECT file_path, timestamp FROM frames WHERE id = ? AND meeting_id = ?")
                .bind(n)
                .bind(meeting_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(err("Failed to look up screen"))?;
            if let Some(r) = row {
                let ts: String = r.get("timestamp");
                out.push(ScreenInfo {
                    source: ScreenSource::Frame(n),
                    file: r.get("file_path"),
                    start: parse_ts(&ts),
                    end: None,
                    start_raw: Some(ts),
                });
                continue;
            }
        }
        if tables.contains("screen_states") {
            let row = sqlx::query(
                "SELECT keyframe_path, start_ts, end_ts FROM screen_states WHERE state_id = ? AND meeting_id = ?",
            )
            .bind(id)
            .bind(meeting_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(err("Failed to look up screen"))?;
            if let Some(r) = row {
                let st: String = r.get("start_ts");
                let en: Option<String> = r.get("end_ts");
                out.push(ScreenInfo {
                    source: ScreenSource::State(id.clone()),
                    file: r.get("keyframe_path"),
                    start: parse_ts(&st),
                    end: en.as_deref().and_then(parse_ts),
                    start_raw: Some(st),
                });
                continue;
            }
        }
        if !lenient {
            return Err(format!("Screen {} isn't in this recording (already removed?)", id));
        }
    }
    Ok(out)
}

/// Purge step 4 (database side): the screen rows and everything derived
/// from them. Returns snapshot ids removed (to flag chats that cited them).
async fn purge_screen_rows(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    screens: &[ScreenInfo],
) -> Result<Vec<String>, sqlx::Error> {
    let tables = table_names(conn).await?;
    let has = |t: &str| tables.contains(t);
    // App backups can predate text_snapshots.meeting_id: find a meeting's
    // loose snapshots through their episode there.
    let snapshots_have_meeting = has("text_snapshots") && has_column(conn, "text_snapshots", "meeting_id").await;
    let loose_snapshots_sql = if snapshots_have_meeting {
        "SELECT snapshot_id, ts FROM text_snapshots WHERE state_id IS NULL AND (meeting_id = ?1 \
         OR episode_id IN (SELECT episode_id FROM document_episodes WHERE meeting_id = ?1))"
    } else {
        "SELECT snapshot_id, ts FROM text_snapshots WHERE state_id IS NULL \
         AND episode_id IN (SELECT episode_id FROM document_episodes WHERE meeting_id = ?1)"
    };
    let mut removed_snapshots = Vec::new();
    // data_versions keys (the data editor's history of these rows holds
    // the old text): ids and rowids of removed snapshots and episodes.
    let mut version_snapshot_keys: Vec<String> = Vec::new();
    let mut version_episode_keys: Vec<String> = Vec::new();

    for sc in screens {
        // VLM analysis: frame_queue → activity_log.frame_ids (= queue id)
        let mut queue_ids: Vec<i64> = Vec::new();
        if has("frame_queue") {
            let frame_id = match &sc.source {
                ScreenSource::Frame(n) => Some(*n),
                ScreenSource::State(_) => None,
            };
            let rows = sqlx::query("SELECT id FROM frame_queue WHERE frame_id = ? OR (frame_path = ? AND frame_path <> '')")
                .bind(frame_id)
                .bind(sc.file.as_deref().unwrap_or(""))
                .fetch_all(&mut *conn)
                .await?;
            queue_ids = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
        }
        if has("activity_log") {
            let mut activity_ids: Vec<i64> = Vec::new();
            if !queue_ids.is_empty() {
                let sql = format!(
                    "SELECT id FROM activity_log WHERE frame_ids IN ({})",
                    placeholders(queue_ids.len())
                );
                let mut q = sqlx::query(&sql);
                for id in &queue_ids {
                    q = q.bind(id.to_string());
                }
                activity_ids.extend(q.fetch_all(&mut *conn).await?.iter().map(|r| r.get::<i64, _>("id")));
            }
            if let Some(raw) = &sc.start_raw {
                let rows = sqlx::query("SELECT id FROM activity_log WHERE start_time = ?")
                    .bind(raw)
                    .fetch_all(&mut *conn)
                    .await?;
                activity_ids.extend(rows.iter().map(|r| r.get::<i64, _>("id")));
            }
            for aid in activity_ids {
                if has("entities") {
                    sqlx::query("DELETE FROM entities WHERE activity_id = ?").bind(aid).execute(&mut *conn).await?;
                }
                if has("data_versions") {
                    sqlx::query("DELETE FROM data_versions WHERE entity_id = ? AND entity_type LIKE '%activit%'")
                        .bind(aid.to_string())
                        .execute(&mut *conn)
                        .await?;
                }
                sqlx::query("DELETE FROM activity_log WHERE id = ?").bind(aid).execute(&mut *conn).await?;
            }
        }
        for qid in &queue_ids {
            sqlx::query("DELETE FROM frame_queue WHERE id = ?").bind(qid).execute(&mut *conn).await?;
        }

        match &sc.source {
            ScreenSource::Frame(n) => {
                sqlx::query("DELETE FROM frames WHERE id = ?").bind(n).execute(&mut *conn).await?;
            }
            ScreenSource::State(state_id) => {
                // OCR / accessibility text captured with (or during) this screen
                let mut snap_ids: Vec<String> = Vec::new();
                if has("text_snapshots") {
                    let rows = sqlx::query("SELECT snapshot_id FROM text_snapshots WHERE state_id = ?")
                        .bind(state_id)
                        .fetch_all(&mut *conn)
                        .await?;
                    snap_ids.extend(rows.iter().map(|r| r.get::<String, _>("snapshot_id")));
                    if let Some(start) = sc.start {
                        let end = sc.end.unwrap_or(start).max(start);
                        let rows = sqlx::query(loose_snapshots_sql).bind(meeting_id).fetch_all(&mut *conn).await?;
                        for r in rows {
                            if let Some(ts) = parse_ts(&r.get::<String, _>("ts")) {
                                if ts >= start && ts <= end {
                                    snap_ids.push(r.get("snapshot_id"));
                                }
                            }
                        }
                    }
                }
                let episodes: Vec<String> = if has("episode_states") {
                    sqlx::query("SELECT episode_id FROM episode_states WHERE state_id = ?")
                        .bind(state_id)
                        .fetch_all(&mut *conn)
                        .await?
                        .iter()
                        .map(|r| r.get::<String, _>("episode_id"))
                        .collect()
                } else {
                    Vec::new()
                };
                for sid in &snap_ids {
                    version_snapshot_keys.push(sid.clone());
                    if let Some(rowid) = sqlx::query_scalar::<_, String>(
                        "SELECT CAST(rowid AS TEXT) FROM text_snapshots WHERE snapshot_id = ?",
                    )
                    .bind(sid)
                    .fetch_optional(&mut *conn)
                    .await?
                    {
                        version_snapshot_keys.push(rowid);
                    }
                    if has("text_patches") {
                        sqlx::query("DELETE FROM text_patches WHERE from_snapshot_id = ? OR to_snapshot_id = ?")
                            .bind(sid)
                            .bind(sid)
                            .execute(&mut *conn)
                            .await?;
                    }
                    sqlx::query("DELETE FROM text_snapshots WHERE snapshot_id = ?").bind(sid).execute(&mut *conn).await?;
                }
                removed_snapshots.extend(snap_ids);
                if has("episode_states") {
                    sqlx::query("DELETE FROM episode_states WHERE state_id = ?").bind(state_id).execute(&mut *conn).await?;
                }
                if has("meeting_timeline_events") {
                    sqlx::query("DELETE FROM meeting_timeline_events WHERE state_id = ?")
                        .bind(state_id)
                        .execute(&mut *conn)
                        .await?;
                }
                sqlx::query("DELETE FROM screen_states WHERE state_id = ?").bind(state_id).execute(&mut *conn).await?;
                // Episodes left with no screens: their text/diffs came from it
                for ep in episodes {
                    let left: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM episode_states WHERE episode_id = ? LIMIT 1")
                        .bind(&ep)
                        .fetch_optional(&mut *conn)
                        .await?;
                    if left.is_some() {
                        continue;
                    }
                    version_episode_keys.push(ep.clone());
                    if let Some(rowid) = sqlx::query_scalar::<_, String>(
                        "SELECT CAST(rowid AS TEXT) FROM document_episodes WHERE episode_id = ?",
                    )
                    .bind(&ep)
                    .fetch_optional(&mut *conn)
                    .await?
                    {
                        version_episode_keys.push(rowid);
                    }
                    if has("text_snapshots") {
                        let rows = sqlx::query(
                            "SELECT snapshot_id, CAST(rowid AS TEXT) AS rid FROM text_snapshots WHERE episode_id = ?",
                        )
                        .bind(&ep)
                        .fetch_all(&mut *conn)
                        .await?;
                        for r in &rows {
                            version_snapshot_keys.push(r.get::<String, _>("snapshot_id"));
                            version_snapshot_keys.push(r.get::<String, _>("rid"));
                        }
                        removed_snapshots.extend(rows.iter().map(|r| r.get::<String, _>("snapshot_id")));
                        sqlx::query("DELETE FROM text_snapshots WHERE episode_id = ?").bind(&ep).execute(&mut *conn).await?;
                    }
                    if has("text_patches") {
                        sqlx::query("DELETE FROM text_patches WHERE episode_id = ?").bind(&ep).execute(&mut *conn).await?;
                    }
                    if has("meeting_timeline_events") {
                        sqlx::query("DELETE FROM meeting_timeline_events WHERE episode_id = ?")
                            .bind(&ep)
                            .execute(&mut *conn)
                            .await?;
                    }
                    sqlx::query("DELETE FROM document_episodes WHERE episode_id = ?").bind(&ep).execute(&mut *conn).await?;
                }
            }
        }
    }

    // Edit history of the removed snapshots/episodes (old values live there)
    if has("data_versions") {
        for (entity_type, keys) in [("text_snapshot", &version_snapshot_keys), ("episode", &version_episode_keys)] {
            for chunk in keys.chunks(200) {
                let sql = format!(
                    "DELETE FROM data_versions WHERE entity_type = ? AND entity_id IN ({})",
                    placeholders(chunk.len())
                );
                let mut q = sqlx::query(&sql).bind(entity_type);
                for k in chunk {
                    q = q.bind(k);
                }
                q.execute(&mut *conn).await?;
            }
        }
    }

    // Assistant chats that cited the removed screen text
    if !removed_snapshots.is_empty()
        && has("assistant_conversations")
        && has_column(conn, "assistant_conversations", "stale_after_edit").await
    {
        for sid in &removed_snapshots {
            sqlx::query("UPDATE assistant_conversations SET stale_after_edit = 1 WHERE context_refs LIKE ?")
                .bind(format!("%snapshot-{}%", sid))
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(removed_snapshots)
}

fn screen_range(sc: &ScreenInfo) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = sc.start?;
    let end = sc.end.unwrap_or(start).max(start + chrono::Duration::seconds(1));
    Some((start, end))
}

// ═══════════════════════════════════════════════════════════════════════════
// Post-commit steps: freed space, logs, backups, files
// ═══════════════════════════════════════════════════════════════════════════

/// FTS5 keeps deleted terms in old segments until they merge; `optimize`
/// rewrites the index into one segment without them.
async fn fts_optimize(pool: &Pool<Sqlite>) -> Result<(), String> {
    sqlx::query("INSERT INTO transcripts_fts(transcripts_fts) VALUES('optimize')")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(err("Search index cleanup failed"))
}

/// Push everything out of the WAL and truncate it, so no pre-edit page
/// copy survives there. Retries while readers hold old snapshots.
pub async fn wal_checkpoint_truncate(pool: &Pool<Sqlite>) -> Result<(), String> {
    for attempt in 0..8 {
        let row = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(pool)
            .await
            .map_err(err("WAL checkpoint failed"))?;
        let busy: i64 = row.get(0);
        if busy == 0 {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt + 1))).await;
    }
    Err("The database was busy, so the write-ahead log couldn't be cleared yet; it clears on the next checkpoint".into())
}

/// Remove the removed words from the app's own log files (older builds
/// logged transcript lines).
fn redact_logs(dir: &Path, re: &Regex, replacement: &str) -> Result<usize, String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(0) };
    let mut n = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        if let Some(new) = redact_str(&text, re, replacement) {
            std::fs::write(&path, new).map_err(|e| format!("Couldn't clean log {}: {}", path.display(), e))?;
            n += 1;
        }
    }
    Ok(n)
}

fn is_sqlite_file(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .map(|_| &buf == b"SQLite format 3\0")
        .unwrap_or(false)
}

fn find_backup_dbs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let p = entry.path();
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() && is_sqlite_file(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn delete_db_files(path: &Path) -> Result<(), String> {
    let mut first_err = None;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let p = PathBuf::from(format!("{}{}", path.display(), suffix));
        if let Err(e) = std::fs::remove_file(&p) {
            if e.kind() != std::io::ErrorKind::NotFound && first_err.is_none() {
                first_err = Some(format!("{}: {}", p.display(), e));
            }
        }
    }
    first_err.map_or(Ok(()), Err)
}

enum BackupJob {
    Words {
        transcript_id: i64,
        original: String,
        new_text: String,
        removed: String,
        replacement: &'static str,
        /// Lines in the live meeting still matching the removed phrase;
        /// `None` when the removal isn't distinctive (no meeting-wide check).
        live_count: Option<usize>,
    },
    Screens { ids: Vec<String> },
    /// Screen text, AI screen-activity summaries and timeline entries
    /// captured inside time ranges (no screen id ties them to a screen)
    RangeExtras { ranges: Vec<(DateTime<Utc>, DateTime<Utc>)> },
}

#[derive(Default)]
struct BackupReport {
    purged: usize,
    deleted: usize,
    errors: Vec<String>,
}

/// Purge step 7: purge the content from every app backup database, or
/// delete the backup if it can't be purged and verified. All of an
/// action's jobs run in one transaction (and one VACUUM) per backup.
async fn purge_backups(env: &RedactionEnv, meeting_id: &str, jobs: &[BackupJob]) -> BackupReport {
    let mut report = BackupReport::default();
    if jobs.is_empty() {
        return report;
    }
    for path in find_backup_dbs(&env.backups_dir()) {
        match purge_one_backup(&path, meeting_id, jobs).await {
            Ok(true) => report.purged += 1,
            Ok(false) => {}
            Err(e) => {
                log::warn!("Backup {} couldn't be purged ({}); deleting it", path.display(), e);
                match delete_db_files(&path) {
                    Ok(()) => report.deleted += 1,
                    Err(de) => report.errors.push(format!(
                        "A backup still contains this content and couldn't be deleted: {}",
                        de
                    )),
                }
            }
        }
    }
    report
}

/// Ok(false): backup doesn't contain the meeting. Ok(true): purged and
/// verified. Err: caller deletes the backup.
async fn purge_one_backup(path: &Path, meeting_id: &str, jobs: &[BackupJob]) -> Result<bool, String> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true)
        .pragma("secure_delete", "ON")
        .disable_statement_logging();
    let mut conn = SqliteConnection::connect_with(&opts).await.map_err(err("open"))?;
    let tables = table_names(&mut conn).await.map_err(err("schema"))?;

    let mut contains = false;
    if tables.contains("meetings") {
        contains |= sqlx::query("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut conn)
            .await
            .map_err(err("read"))?
            .is_some();
    }
    for t in ["transcripts", "frames", "screen_states"] {
        if !contains && tables.contains(t) {
            contains |= sqlx::query(&format!("SELECT 1 FROM {} WHERE meeting_id = ? LIMIT 1", t))
                .bind(meeting_id)
                .fetch_optional(&mut conn)
                .await
                .map_err(err("read"))?
                .is_some();
        }
    }
    if !contains {
        let _ = conn.close().await;
        return Ok(false);
    }

    let mut tx = conn.begin().await.map_err(err("begin"))?;
    for job in jobs {
        match job {
            BackupJob::Words { transcript_id, original, new_text, removed, replacement, live_count } => {
                if tables.contains("transcripts") {
                    let row = sqlx::query("SELECT text FROM transcripts WHERE id = ? AND meeting_id = ?")
                        .bind(transcript_id)
                        .bind(meeting_id)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(err("read"))?;
                    if let Some(r) = row {
                        if r.get::<String, _>("text") == *original {
                            if new_text.trim().is_empty() {
                                sqlx::query("DELETE FROM transcripts WHERE id = ?")
                                    .bind(transcript_id)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err(err("write"))?;
                            } else {
                                sqlx::query("UPDATE transcripts SET text = ? WHERE id = ?")
                                    .bind(new_text)
                                    .bind(transcript_id)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err(err("write"))?;
                                if has_column(&mut tx, "transcripts", "word_timings").await {
                                    sqlx::query("UPDATE transcripts SET word_timings = NULL WHERE id = ?")
                                        .bind(transcript_id)
                                        .execute(&mut *tx)
                                        .await
                                        .map_err(err("write"))?;
                                }
                            }
                        }
                    }
                }
                redact_ai_outputs(&mut tx, meeting_id, removed, replacement).await.map_err(err("ai outputs"))?;
                if tables.contains("transcripts") {
                    // The edited line itself: if the backup holds a different
                    // version of it that still has the words, it can't be purged.
                    if let Some(re) = phrase_regex(removed) {
                        let row: Option<String> = sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ? AND meeting_id = ?")
                            .bind(transcript_id)
                            .bind(meeting_id)
                            .fetch_optional(&mut *tx)
                            .await
                            .map_err(err("verify"))?;
                        if row.map_or(false, |t| t != *new_text && re.is_match(&t)) {
                            return Err("backup holds another version of the edited line".into());
                        }
                    }
                    // Meeting-wide copies: only checkable for distinctive phrases
                    // (a common word legitimately appears in other lines).
                    if let (Some(re), Some(live)) = (distinctive_phrase_regex(removed), live_count) {
                        let n = count_phrase_in_meeting(&mut tx, meeting_id, &re).await.map_err(err("verify"))?;
                        if n > *live {
                            return Err("backup holds other copies of the removed words".into());
                        }
                    }
                }
            }
            BackupJob::Screens { ids } => {
                let screens = resolve_screens(&mut tx, meeting_id, ids, true).await?;
                purge_screen_rows(&mut tx, meeting_id, &screens).await.map_err(err("screens"))?;
                let left = resolve_screens(&mut tx, meeting_id, ids, true).await?;
                if !left.is_empty() {
                    return Err("screens still present after purge".into());
                }
            }
            BackupJob::RangeExtras { ranges } => {
                let found = time_range::find_range_extras(&mut tx, meeting_id, ranges).await.map_err(err("range"))?;
                time_range::purge_range_extras(&mut tx, &found).await.map_err(err("range"))?;
                let left = time_range::find_range_extras(&mut tx, meeting_id, ranges).await.map_err(err("range"))?;
                if !left.is_empty() {
                    return Err("screen text still present after purge".into());
                }
            }
        }
    }
    tx.commit().await.map_err(err("commit"))?;
    if tables.contains("transcripts_fts") {
        sqlx::query("INSERT INTO transcripts_fts(transcripts_fts) VALUES('rebuild')")
            .execute(&mut conn)
            .await
            .map_err(err("fts rebuild"))?;
    }
    sqlx::query("VACUUM").execute(&mut conn).await.map_err(err("vacuum"))?;
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut conn).await;
    conn.close().await.map_err(err("close"))?;
    Ok(true)
}

fn remove_screen_files(env: &RedactionEnv, meeting_id: &str, screens: &[ScreenInfo]) -> Vec<String> {
    let mut errors = Vec::new();
    for sc in screens {
        if let Some(f) = sc.file.as_deref().filter(|f| !f.is_empty()) {
            if let Err(e) = std::fs::remove_file(f) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("{}: {}", f, e));
                }
            }
        }
    }
    // Cached frames/thumbnails extracted from this meeting's video: caches,
    // regenerated on demand, so drop them all rather than guess which match.
    for sub in ["frames", "thumbnails"] {
        let d = env.cache_dir.join(meeting_id).join(sub);
        if d.exists() {
            if let Err(e) = std::fs::remove_dir_all(&d) {
                errors.push(format!("{}: {}", d.display(), e));
            }
        }
    }
    errors
}

// ═══════════════════════════════════════════════════════════════════════════
// DMG screen video: blanked by the background job worker (video_jobs.rs,
// video_blank.rs), never inline and never under LOCK
// ═══════════════════════════════════════════════════════════════════════════

/// Does this meeting have screen video that a purge must blank? (DMG only;
/// the Mac App Store build records screenshots, not video.)
fn has_screen_video(env: &RedactionEnv, meeting_id: &str) -> bool {
    #[cfg(not(feature = "mas"))]
    if env.video_enabled {
        return !video_blank::list_chunks(&video_blank::video_dir(env, meeting_id)).is_empty();
    }
    let _ = (env, meeting_id);
    false
}

// ═══════════════════════════════════════════════════════════════════════════
// Actions
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct WordTarget {
    pub meeting_id: String,
    pub transcript_id: i64,
    /// UTF-16 offsets into the line text (JS string indexes)
    pub start: usize,
    pub end: usize,
    /// If given, the line must still read exactly this (stale-view guard)
    pub expected_text: Option<String>,
    /// Target the whole line; `start`/`end` are ignored and resolved from
    /// the current text *inside* the redaction lock, after overlapping
    /// pending deletes have been flushed (so the range can't go stale).
    pub whole_line: bool,
}

struct Line {
    text: String,
    timings: Option<String>,
    timestamp: Option<DateTime<Utc>>,
}

async fn load_line(conn: &mut SqliteConnection, meeting_id: &str, transcript_id: i64) -> Result<Line, String> {
    let row = sqlx::query("SELECT text, word_timings, timestamp FROM transcripts WHERE id = ? AND meeting_id = ?")
        .bind(transcript_id)
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Failed to load the line"))?
        .ok_or("That transcript line no longer exists")?;
    Ok(Line {
        text: row.get("text"),
        timings: row.get("word_timings"),
        timestamp: parse_ts(&row.get::<String, _>("timestamp")),
    })
}

fn word_range(t: &WordTarget) -> (usize, usize) {
    (t.start, t.end)
}

/// A whole-line target with its range set from the line's current text.
/// Call under the lock, after `flush_overlapping_locked`.
async fn resolve_target(conn: &mut SqliteConnection, t: &WordTarget) -> Result<WordTarget, String> {
    if !t.whole_line {
        return Ok(t.clone());
    }
    let line = load_line(conn, &t.meeting_id, t.transcript_id).await?;
    Ok(WordTarget { start: 0, end: utf16_len(&line.text), whole_line: false, ..t.clone() })
}

/// One transcript line changed by an action: what the post-commit purge
/// needs. Held in memory only for the duration of the action.
struct LineChange {
    transcript_id: i64,
    original: String,
    new_text: String,
    removed: String,
}

/// Write one line edit inside the action's transaction: text + FTS (the
/// transcripts_au / transcripts_ad triggers) and AI outputs (purge steps
/// 1, 2 and 5). A line left with no text is removed.
async fn write_line_edit(
    tx: &mut SqliteConnection,
    meeting_id: &str,
    transcript_id: i64,
    edit: &TextEdit,
    replacement: &str,
) -> Result<(), String> {
    if edit.new_text.trim().is_empty() {
        sqlx::query("DELETE FROM transcripts WHERE id = ?")
            .bind(transcript_id)
            .execute(&mut *tx)
            .await
            .map_err(err("Failed to remove the line"))?;
    } else {
        sqlx::query("UPDATE transcripts SET text = ?, text_hash = ?, word_timings = ? WHERE id = ?")
            .bind(&edit.new_text)
            .bind(crate::database::transcript_text_hash(&edit.new_text))
            .bind(&edit.new_timings)
            .bind(transcript_id)
            .execute(&mut *tx)
            .await
            .map_err(err("Failed to update the line"))?;
    }
    redact_ai_outputs(tx, meeting_id, &edit.removed_plain, replacement)
        .await
        .map_err(err("Failed to redact AI outputs"))?;
    Ok(())
}

/// Post-commit purge shared by every action: search index, app logs, app
/// backups (one pass per backup for all of the action's jobs), freed space.
/// Failures become warnings, shown to the user, never swallowed.
async fn post_commit_purge(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    lines: &[LineChange],
    mut jobs: Vec<BackupJob>,
    replacement: &'static str,
    out: &mut ActionOutcome,
) {
    if !lines.is_empty() {
        if let Err(e) = fts_optimize(pool).await {
            out.warnings.push(e);
        }
    }
    for l in lines {
        // Logs and the meeting-wide backup check only for distinctive
        // removals (a common word would rewrite unrelated log lines).
        let re = distinctive_phrase_regex(&l.removed);
        let mut live_count = None;
        if let Some(re) = &re {
            if let Err(e) = redact_logs(&env.logs_dir(), re, replacement) {
                out.warnings.push(e);
            }
            live_count = Some(match pool.acquire().await {
                Ok(mut conn) => count_phrase_in_meeting(&mut conn, meeting_id, re).await.unwrap_or(usize::MAX),
                Err(_) => usize::MAX,
            });
        }
        jobs.push(BackupJob::Words {
            transcript_id: l.transcript_id,
            original: l.original.clone(),
            new_text: l.new_text.clone(),
            removed: l.removed.clone(),
            replacement,
            live_count,
        });
    }
    let report = purge_backups(env, meeting_id, &jobs).await;
    out.backups_purged = report.purged;
    out.backups_deleted = report.deleted;
    out.warnings.extend(report.errors);
    // Links (meeting_links.rs) are derived from the text just removed; a
    // hidden link's hash goes once the link no longer appears anywhere
    if let Err(e) = crate::meeting_links::prune_hidden(pool, meeting_id).await {
        out.warnings.push(e);
    }
    if let Err(e) = wal_checkpoint_truncate(pool).await {
        out.warnings.push(e);
    }
}

async fn apply_words_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    target: &WordTarget,
    action: Action,
    reason: Option<&str>,
    record_id: &str,
    expected_hash: Option<&str>,
) -> Result<ActionOutcome, String> {
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    let line = load_line(&mut tx, &target.meeting_id, target.transcript_id).await?;
    if let Some(exp) = &target.expected_text {
        if *exp != line.text {
            return Err("This line changed since you selected it. Reload and try again.".into());
        }
    }
    if let Some(h) = expected_hash {
        if crate::database::transcript_text_hash(&line.text) != h {
            return Err("This line changed during the undo window, so the delete was not applied.".into());
        }
    }
    let marker = (action == Action::Strike).then(|| marker_token(record_id));
    let (start, end) = word_range(target);
    let edit = apply_word_edit(&line.text, line.timings.as_deref(), start, end, marker.as_deref())?;
    let reason = if action == Action::Strike {
        validate_reason(reason, Some(&edit.removed_plain))?
    } else {
        None
    };

    write_line_edit(&mut tx, &target.meeting_id, target.transcript_id, &edit, action.replacement()).await?;

    let (media_start, media_end) = match (line.timestamp, edit.removed_ms) {
        (Some(ts), Some((a, b))) => (
            Some(ts + chrono::Duration::milliseconds(a)),
            Some(ts + chrono::Duration::milliseconds(b)),
        ),
        (Some(ts), None) => (Some(ts), Some(ts)),
        _ => (None, None),
    };
    let kind = if edit.whole_line { "line" } else { "words" };

    let mut out = ActionOutcome::default();
    match action {
        Action::Strike => {
            let rec = RedactionRecord {
                id: record_id.to_string(),
                meeting_id: target.meeting_id.clone(),
                kind: kind.into(),
                action: "strike".into(),
                media_start: media_start.map(|d| d.to_rfc3339()),
                media_end: media_end.map(|d| d.to_rfc3339()),
                created_at: Utc::now().to_rfc3339(),
                reason,
                transcript_id: Some(target.transcript_id),
                item_count: 1,
                video_pending: false,
            };
            insert_record(&mut tx, &rec, None).await?;
            out.record = Some(rec.clone());
            out.records.push(rec);
        }
        Action::Delete => {
            // The pending row (if any) goes away: Delete leaves no trace
            sqlx::query("DELETE FROM redactions WHERE id = ? AND action = 'delete'")
                .bind(record_id)
                .execute(&mut *tx)
                .await
                .map_err(err("Failed to finish the delete"))?;
        }
    }
    tx.commit().await.map_err(err("Failed to save the edit"))?;

    let change = LineChange {
        transcript_id: target.transcript_id,
        original: line.text,
        new_text: edit.new_text,
        removed: edit.removed_plain,
    };
    post_commit_purge(pool, env, &target.meeting_id, &[change], Vec::new(), action.replacement(), &mut out).await;
    Ok(out)
}

async fn insert_record(
    conn: &mut SqliteConnection,
    rec: &RedactionRecord,
    pending_payload: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO redactions (id, meeting_id, kind, action, media_start, media_end, created_at, reason, \
         transcript_id, item_count, pending_payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.id)
    .bind(&rec.meeting_id)
    .bind(&rec.kind)
    .bind(&rec.action)
    .bind(&rec.media_start)
    .bind(&rec.media_end)
    .bind(&rec.created_at)
    .bind(&rec.reason)
    .bind(rec.transcript_id)
    .bind(rec.item_count)
    .bind(pending_payload)
    .execute(&mut *conn)
    .await
    .map_err(err("Failed to save the record"))?;
    Ok(())
}

/// Screens: the database purge (rows, derived rows) commits first, so the
/// screens are gone at once and for good. On the DMG build the screen video
/// for those moments is queued for blanking in the same transaction and
/// blanked by the background worker afterwards, outside the lock.
async fn apply_screens_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ids: &[String],
    action: Action,
    reason: Option<&str>,
    record_id: &str,
) -> Result<ActionOutcome, String> {
    if ids.is_empty() {
        return Err("No screens selected".into());
    }
    env.ensure_not_recording(meeting_id)?;
    let reason = if action == Action::Strike { validate_reason(reason, None)? } else { None };

    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    let screens = resolve_screens(&mut tx, meeting_id, ids, false).await?;
    let ranges: Vec<(DateTime<Utc>, DateTime<Utc>)> = screens.iter().filter_map(screen_range).collect();
    purge_screen_rows(&mut tx, meeting_id, &screens).await.map_err(err("Failed to remove screens"))?;

    let mut out = ActionOutcome::default();
    if has_screen_video(env, meeting_id) && !ranges.is_empty() {
        out.video_jobs_queued = video_jobs::enqueue(
            &mut tx,
            meeting_id,
            &ranges,
            (action == Action::Strike).then_some(record_id),
        )
        .await
        .map_err(err("Failed to queue the screen video blanking"))?;
    }

    let media_start = ranges.iter().map(|r| r.0).min();
    let media_end = ranges.iter().map(|r| r.1).max();
    match action {
        Action::Strike => {
            let rec = RedactionRecord {
                id: record_id.to_string(),
                meeting_id: meeting_id.to_string(),
                kind: "screen".into(),
                action: "strike".into(),
                media_start: media_start.map(|d| d.to_rfc3339()),
                media_end: media_end.map(|d| d.to_rfc3339()),
                created_at: Utc::now().to_rfc3339(),
                reason,
                transcript_id: None,
                item_count: screens.len() as i64,
                video_pending: out.video_jobs_queued > 0,
            };
            insert_record(&mut tx, &rec, None).await?;
            out.record = Some(rec.clone());
            out.records.push(rec);
        }
        Action::Delete => {
            sqlx::query("DELETE FROM redactions WHERE id = ? AND action = 'delete'")
                .bind(record_id)
                .execute(&mut *tx)
                .await
                .map_err(err("Failed to finish the delete"))?;
        }
    }
    tx.commit().await.map_err(err("Failed to save the edit"))?;

    let file_errors = remove_screen_files(env, meeting_id, &screens);
    post_commit_purge(
        pool,
        env,
        meeting_id,
        &[],
        vec![BackupJob::Screens { ids: ids.to_vec() }],
        action.replacement(),
        &mut out,
    )
    .await;
    if !file_errors.is_empty() {
        // Loud: the rows are gone but an image file is still on disk
        return Err(format!(
            "The screens were removed from the recording, but these files could not be deleted: {}",
            file_errors.join("; ")
        ));
    }
    Ok(out)
}

/// What a new action touches, so overlapping pending deletes commit first.
enum Scope {
    /// One transcript line: its pending deletes hold offsets into it
    Line(i64),
    /// Screens: a pending screen delete could hold the same ids
    Screens,
    /// A block of time: anything pending in the meeting may overlap it
    All,
}

/// Commit pending deletes that overlap a new action (same line, or any
/// screens of the meeting), so offsets and ids stay valid. Other pending
/// deletes keep their undo window (e.g. several lines deleted at once).
/// Errors are returned, never swallowed. Database work only: screen video
/// is blanked later by the job worker, so this never waits on ffmpeg.
async fn flush_overlapping_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    scope: Scope,
) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT id, pending_payload FROM redactions WHERE meeting_id = ? AND action = 'delete' \
         AND pending_payload IS NOT NULL ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(err("Database busy"))?;
    for r in rows {
        let payload = serde_json::from_str::<PendingPayload>(&r.get::<String, _>("pending_payload")).ok();
        let overlaps = match (&scope, &payload) {
            (Scope::All, _) | (_, None) => true,
            (Scope::Line(id), Some(PendingPayload::Words { transcript_id, .. })) => transcript_id == id,
            (Scope::Line(id), Some(PendingPayload::TimeRange { lines, .. })) => {
                lines.iter().any(|l| l.transcript_id == *id)
            }
            (Scope::Line(_), Some(PendingPayload::Screens { .. })) => false,
            (Scope::Screens, Some(PendingPayload::Words { .. })) => false,
            (Scope::Screens, Some(_)) => true,
        };
        if overlaps {
            commit_locked(pool, env, &r.get::<String, _>("id")).await?;
        }
    }
    Ok(())
}

/// Why a pending Delete didn't apply.
enum CommitFailure {
    /// Nothing about the target changed (database busy, recording in
    /// progress): keep the pending row, mark it failed, retry later.
    Transient(String),
    /// The target itself changed (line edited/removed, screens gone): the
    /// stored offsets/ids no longer mean the same content, so it can never
    /// apply. The row is dropped and the user is told.
    Permanent(String),
}

/// Before applying: is the pending delete's target still exactly what the
/// user selected? Only a definite "no" is permanent; read errors are not.
async fn check_target_unchanged(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    payload: &PendingPayload,
) -> Result<(), CommitFailure> {
    let mut conn = pool.acquire().await.map_err(|e| CommitFailure::Transient(format!("Database busy: {}", e)))?;
    let busy = |e: sqlx::Error| CommitFailure::Transient(format!("Database busy: {}", e));
    match payload {
        PendingPayload::Words { transcript_id, text_hash, .. } => {
            let text: Option<String> = sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ? AND meeting_id = ?")
                .bind(transcript_id)
                .bind(meeting_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(busy)?;
            match text {
                None => Err(CommitFailure::Permanent("That transcript line no longer exists".into())),
                Some(t) if crate::database::transcript_text_hash(&t) != *text_hash => Err(CommitFailure::Permanent(
                    "This line changed during the undo window, so the delete was not applied.".into(),
                )),
                Some(_) => Ok(()),
            }
        }
        PendingPayload::Screens { ids } => {
            let wanted: HashSet<&String> = ids.iter().collect();
            let found = resolve_screens(&mut conn, meeting_id, ids, true).await.map_err(CommitFailure::Transient)?;
            if found.len() < wanted.len() {
                Err(CommitFailure::Permanent(
                    "Some of those screens were already removed, so the delete was not applied.".into(),
                ))
            } else {
                Ok(())
            }
        }
        PendingPayload::TimeRange { screen_ids, lines, .. } => {
            for l in lines {
                let text: Option<String> =
                    sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ? AND meeting_id = ?")
                        .bind(l.transcript_id)
                        .bind(meeting_id)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(busy)?;
                if text.map_or(true, |t| crate::database::transcript_text_hash(&t) != l.text_hash) {
                    return Err(CommitFailure::Permanent(
                        "The transcript in that time range changed during the undo window, so the delete was not applied."
                            .into(),
                    ));
                }
            }
            let wanted: HashSet<&String> = screen_ids.iter().collect();
            let found = resolve_screens(&mut conn, meeting_id, screen_ids, true)
                .await
                .map_err(CommitFailure::Transient)?;
            if found.len() < wanted.len() {
                return Err(CommitFailure::Permanent(
                    "Some screens in that time range were already removed, so the delete was not applied.".into(),
                ));
            }
            Ok(())
        }
    }
}

async fn commit_locked(pool: &Pool<Sqlite>, env: &RedactionEnv, id: &str) -> Result<Option<ActionOutcome>, String> {
    let row = sqlx::query(
        "SELECT meeting_id, pending_payload FROM redactions WHERE id = ? AND action = 'delete' AND pending_payload IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(err("Database busy"))?;
    let Some(row) = row else { return Ok(None) };
    let meeting_id: String = row.get("meeting_id");
    let payload: String = row.get("pending_payload");
    let result: Result<ActionOutcome, CommitFailure> = match serde_json::from_str::<PendingPayload>(&payload) {
        Err(e) => Err(CommitFailure::Permanent(format!("Unreadable pending delete: {}", e))),
        Ok(p) => match check_target_unchanged(pool, &meeting_id, &p).await {
            Err(f) => Err(f),
            Ok(()) => match p {
                PendingPayload::Words { transcript_id, start, end, text_hash } => {
                    let target = WordTarget {
                        meeting_id: meeting_id.clone(),
                        transcript_id,
                        start,
                        end,
                        expected_text: None,
                        whole_line: false,
                    };
                    apply_words_locked(pool, env, &target, Action::Delete, None, id, Some(&text_hash))
                        .await
                        .map_err(CommitFailure::Transient)
                }
                PendingPayload::Screens { ids } => {
                    apply_screens_locked(pool, env, &meeting_id, &ids, Action::Delete, None, id)
                        .await
                        .map_err(CommitFailure::Transient)
                }
                PendingPayload::TimeRange { start, end, screen_ids, lines, ranges } => {
                    let raw = if ranges.is_empty() { vec![(start, end)] } else { ranges };
                    let spans: Option<Vec<(DateTime<Utc>, DateTime<Utc>)>> =
                        raw.iter().map(|(a, b)| Some((parse_ts(a)?, parse_ts(b)?))).collect();
                    match spans {
                        Some(spans) if !spans.is_empty() => time_range::apply_range_locked(
                            pool,
                            env,
                            &meeting_id,
                            &spans,
                            &screen_ids,
                            &lines,
                            Action::Delete,
                            None,
                            id,
                        )
                        .await
                        .map_err(CommitFailure::Transient),
                        _ => Err(CommitFailure::Permanent("Unreadable pending delete".into())),
                    }
                }
            },
        },
    };
    match result {
        Ok(o) => Ok(Some(o)),
        Err(CommitFailure::Permanent(msg)) => {
            let _ = sqlx::query("DELETE FROM redactions WHERE id = ? AND action = 'delete'")
                .bind(id)
                .execute(pool)
                .await;
            Err(msg)
        }
        Err(CommitFailure::Transient(msg)) => {
            // Keep it pending (it is retried), but marked failed with a
            // reason so the meeting view can show it. If the row is already
            // gone the delete itself applied and only a later step failed.
            let _ = sqlx::query(
                "UPDATE redactions SET failed_at = ?, failure = ? \
                 WHERE id = ? AND action = 'delete' AND pending_payload IS NOT NULL",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(&msg)
            .bind(id)
            .execute(pool)
            .await;
            Err(msg)
        }
    }
}

/// Retry this meeting's failed pending deletes (all meetings when `None`).
/// Returns the errors of the ones that failed again (still kept pending).
async fn retry_failed_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: Option<&str>,
    except: Option<&str>,
) -> Vec<String> {
    let ids: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM redactions WHERE action = 'delete' AND pending_payload IS NOT NULL          AND failed_at IS NOT NULL AND (?1 IS NULL OR meeting_id = ?1) ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    {
        Ok(ids) => ids,
        Err(e) => return vec![format!("Couldn't read failed deletes: {}", e)],
    };
    let mut errors = Vec::new();
    for id in ids {
        if except == Some(id.as_str()) {
            continue;
        }
        if let Err(e) = commit_locked(pool, env, &id).await {
            log::warn!("Retried delete {} failed again: {}", id, e);
            errors.push(e);
        }
    }
    errors
}

/// A Delete whose undo window ended but which couldn't be applied yet.
/// Holds no content (ids/offsets stay server-side).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FailedDelete {
    pub id: String,
    pub meeting_id: String,
    pub kind: String,
    pub item_count: i64,
    pub created_at: String,
    pub failed_at: String,
    pub failure: String,
}

pub async fn list_failed_deletes(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<Vec<FailedDelete>, String> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, kind, item_count, created_at, failed_at, failure FROM redactions          WHERE meeting_id = ? AND action = 'delete' AND pending_payload IS NOT NULL AND failed_at IS NOT NULL          ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(err("Database busy"))?;
    Ok(rows
        .iter()
        .map(|r| FailedDelete {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            kind: r.get("kind"),
            item_count: r.get("item_count"),
            created_at: r.get("created_at"),
            failed_at: r.get("failed_at"),
            failure: r.get::<Option<String>, _>("failure").unwrap_or_default(),
        })
        .collect())
}

/// Retry button: try this meeting's failed deletes again.
pub async fn retry_failed_deletes(pool: &Pool<Sqlite>, env: &RedactionEnv, meeting_id: &str) -> Vec<String> {
    let _g = LOCK.lock().await;
    retry_failed_locked(pool, env, Some(meeting_id), None).await
}

/// Still pending (in its undo window, or failed and waiting for a retry)?
pub async fn is_pending(pool: &Pool<Sqlite>, id: &str) -> bool {
    sqlx::query("SELECT 1 FROM redactions WHERE id = ? AND action = 'delete' AND pending_payload IS NOT NULL")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

// ── Public entry points (each takes the app-wide lock) ────────────────────

/// Delete words: validated now, committed after the undo window.
pub async fn request_delete_words(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    target: &WordTarget,
) -> Result<PendingDelete, String> {
    let _g = LOCK.lock().await;
    flush_overlapping_locked(pool, env, &target.meeting_id, Scope::Line(target.transcript_id)).await?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let target = &resolve_target(&mut conn, target).await?;
    let line = load_line(&mut conn, &target.meeting_id, target.transcript_id).await?;
    if let Some(exp) = &target.expected_text {
        if *exp != line.text {
            return Err("This line changed since you selected it. Reload and try again.".into());
        }
    }
    let edit = apply_word_edit(&line.text, line.timings.as_deref(), target.start, target.end, None)?;
    let id = new_id();
    let payload = serde_json::to_string(&PendingPayload::Words {
        transcript_id: target.transcript_id,
        start: target.start,
        end: target.end,
        text_hash: crate::database::transcript_text_hash(&line.text),
    })
    .map_err(err("Failed to queue the delete"))?;
    let kind = if edit.whole_line { "line" } else { "words" };
    let rec = RedactionRecord {
        id: id.clone(),
        meeting_id: target.meeting_id.clone(),
        kind: kind.into(),
        action: "delete".into(),
        media_start: None,
        media_end: None,
        created_at: Utc::now().to_rfc3339(),
        reason: None,
        transcript_id: Some(target.transcript_id),
        item_count: 1,
        video_pending: false,
    };
    insert_record(&mut conn, &rec, Some(&payload)).await?;
    Ok(PendingDelete { id, meeting_id: target.meeting_id.clone(), kind: kind.into(), undo_seconds: UNDO_WINDOW_SECS })
}

pub async fn request_delete_screens(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ids: &[String],
) -> Result<PendingDelete, String> {
    let _g = LOCK.lock().await;
    if ids.is_empty() {
        return Err("No screens selected".into());
    }
    env.ensure_not_recording(meeting_id)?;
    flush_overlapping_locked(pool, env, meeting_id, Scope::Screens).await?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let screens = resolve_screens(&mut conn, meeting_id, ids, false).await?;
    let id = new_id();
    let payload = serde_json::to_string(&PendingPayload::Screens { ids: ids.to_vec() })
        .map_err(err("Failed to queue the delete"))?;
    let rec = RedactionRecord {
        id: id.clone(),
        meeting_id: meeting_id.to_string(),
        kind: "screen".into(),
        action: "delete".into(),
        media_start: None,
        media_end: None,
        created_at: Utc::now().to_rfc3339(),
        reason: None,
        transcript_id: None,
        item_count: screens.len() as i64,
        video_pending: false,
    };
    insert_record(&mut conn, &rec, Some(&payload)).await?;
    Ok(PendingDelete { id, meeting_id: meeting_id.to_string(), kind: "screen".into(), undo_seconds: UNDO_WINDOW_SECS })
}

/// Undo a pending Delete. Strikes have no undo.
pub async fn undo_delete(pool: &Pool<Sqlite>, id: &str) -> Result<(), String> {
    let _g = LOCK.lock().await;
    let action: Option<String> = sqlx::query_scalar("SELECT action FROM redactions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(err("Database busy"))?;
    match action.as_deref() {
        Some("strike") => Err("Striking from the record can't be undone".into()),
        None => Err("Too late to undo: the delete was already applied".into()),
        _ => {
            sqlx::query("DELETE FROM redactions WHERE id = ? AND action = 'delete' AND pending_payload IS NOT NULL")
                .bind(id)
                .execute(pool)
                .await
                .map_err(err("Failed to undo"))?;
            Ok(())
        }
    }
}

/// Commit one pending Delete now (timer fired, or the user dismissed the
/// toast). Ok(None) if it was already committed or undone.
pub async fn commit_pending(pool: &Pool<Sqlite>, env: &RedactionEnv, id: &str) -> Result<Option<ActionOutcome>, String> {
    let _g = LOCK.lock().await;
    let meeting_id: Option<String> = sqlx::query_scalar("SELECT meeting_id FROM redactions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let result = commit_locked(pool, env, id).await;
    // Next commit attempt = a retry of this meeting's earlier failures
    if let Some(mid) = meeting_id {
        retry_failed_locked(pool, env, Some(&mid), Some(id)).await;
    }
    result
}

/// Commit every pending Delete (app exit, and launch after a crash), which
/// includes retrying the ones that failed earlier. Returns the errors; each
/// one is also logged, and transient failures stay pending (marked failed).
pub async fn commit_all_pending(pool: &Pool<Sqlite>, env: &RedactionEnv) -> Vec<String> {
    let _g = LOCK.lock().await;
    let ids: Vec<String> = match sqlx::query(
        "SELECT id FROM redactions WHERE action = 'delete' AND pending_payload IS NOT NULL ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows.iter().map(|r| r.get("id")).collect(),
        Err(e) => return vec![format!("Couldn't read pending deletes: {}", e)],
    };
    let mut errors = Vec::new();
    for id in ids {
        if let Err(e) = commit_locked(pool, env, &id).await {
            log::error!("Pending delete {} failed to commit: {}", id, e);
            errors.push(e);
        }
    }
    errors
}

pub async fn strike_words(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    target: &WordTarget,
    reason: Option<&str>,
) -> Result<ActionOutcome, String> {
    let _g = LOCK.lock().await;
    flush_overlapping_locked(pool, env, &target.meeting_id, Scope::Line(target.transcript_id)).await?;
    let target = {
        let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
        resolve_target(&mut conn, target).await?
    };
    apply_words_locked(pool, env, &target, Action::Strike, reason, &new_id(), None).await
}

pub async fn strike_screens(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ids: &[String],
    reason: Option<&str>,
) -> Result<ActionOutcome, String> {
    let _g = LOCK.lock().await;
    flush_overlapping_locked(pool, env, meeting_id, Scope::Screens).await?;
    apply_screens_locked(pool, env, meeting_id, ids, Action::Strike, reason, &new_id()).await
}

// ── Sync with your iPhone (docs/SYNC.md) ──────────────────────────────────

/// Another device deleted or struck these screens (`ids` as the timeline
/// names them: a `frames` id or a `screen_states` id): the same purge as a
/// local Delete of screens (rows, derived rows, files, and on the DMG build
/// a screen video blanking job), with no marker of its own (a strike's
/// marker arrives as its own record). Refused while the recording runs.
pub async fn delete_screens_synced(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ids: &[String],
) -> Result<ActionOutcome, String> {
    let _g = LOCK.lock().await;
    flush_overlapping_locked(pool, env, meeting_id, Scope::Screens).await?;
    apply_screens_locked(pool, env, meeting_id, ids, Action::Delete, None, &new_id()).await
}

/// One change another device made to a line, in this Mac's UTF-16 offsets
/// (planned by `sync::merge` from the line's current text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncedOp {
    /// Remove the words in `[start16, end16)`. `marker` (32-hex record id):
    /// put that strike marker where they were. `strike`: AI outputs get the
    /// stricken placeholder; else the delete placeholder.
    Remove { start16: usize, end16: usize, marker: Option<String>, strike: bool },
    /// Insert strike markers whose words were already gone here
    Insert { at16: usize, markers: Vec<String> },
}

/// Apply another device's removals to one line through the same purge as a
/// local Delete/Strike (text and search index, AI outputs, review guide,
/// AI topics and chat answers, app backups and logs, links, WAL). `plan` gets
/// the line's current text *inside* the redaction lock, after pending
/// deletes on the line were committed, and returns ops from the end of the
/// line backwards. `src` = (device id, time): the change is logged as coming
/// from that device, so it isn't sent back to it. Returns whether anything
/// changed. The removed words exist only in this call's locals.
pub async fn edit_line_synced<F>(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    transcript_id: i64,
    src: Option<(&str, i64)>,
    plan: F,
) -> Result<bool, String>
where
    F: FnOnce(&str) -> Vec<SyncedOp>,
{
    let _g = LOCK.lock().await;
    flush_overlapping_locked(pool, env, meeting_id, Scope::Line(transcript_id)).await?;
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    let line = match load_line(&mut tx, meeting_id, transcript_id).await {
        Ok(l) => l,
        Err(_) => return Ok(false),
    };
    let ops = plan(&line.text);
    if ops.is_empty() {
        return Ok(false);
    }
    let mut text = line.text.clone();
    let mut timings = line.timings.clone();
    // (removed words, struck?)
    let mut removed: Vec<(String, bool)> = Vec::new();
    for op in ops {
        match op {
            SyncedOp::Remove { start16, end16, marker, strike } => {
                let token = marker.as_deref().map(marker_token);
                match apply_word_edit(&text, timings.as_deref(), start16, end16, token.as_deref()) {
                    Ok(edit) => {
                        removed.push((edit.removed_plain, strike));
                        text = edit.new_text;
                        timings = edit.new_timings;
                    }
                    // Already gone here (or only markers left): nothing to remove
                    Err(_) => {}
                }
            }
            SyncedOp::Insert { at16, markers } => {
                let tokens: Vec<String> = markers
                    .iter()
                    .filter(|m| !text.contains(&marker_token(m)))
                    .map(|m| marker_token(m))
                    .collect();
                if tokens.is_empty() {
                    continue;
                }
                let before16 = utf16_len(&text);
                let new_text = crate::sync::merge::insert_markers(&text, at16, &tokens);
                let delta = utf16_len(&new_text) as i64 - before16 as i64;
                if let Some(list) = timings.as_deref().and_then(|j| serde_json::from_str::<Vec<WordTiming>>(j).ok()) {
                    let shifted: Vec<WordTiming> = list
                        .into_iter()
                        .map(|w| {
                            if w.s >= at16 {
                                WordTiming { s: (w.s as i64 + delta).max(0) as usize, e: (w.e as i64 + delta).max(0) as usize, ..w }
                            } else {
                                w
                            }
                        })
                        .collect();
                    timings = serde_json::to_string(&shifted).ok();
                }
                text = new_text;
            }
        }
    }
    if text == line.text {
        return Ok(false);
    }
    if let Some((device, at)) = src {
        sqlx::query("INSERT OR REPLACE INTO sync_apply (key, value) VALUES ('src', ?), ('at', ?)")
            .bind(device)
            .bind(at.to_string())
            .execute(&mut *tx)
            .await
            .map_err(err("Database busy"))?;
    }
    let edit = TextEdit {
        new_text: text.clone(),
        removed_plain: String::new(),
        new_timings: timings,
        removed_ms: None,
        whole_line: false,
    };
    // Text + search index (and the row goes if no text is left)
    write_line_edit(&mut tx, meeting_id, transcript_id, &edit, DELETED_PLACEHOLDER).await?;
    for (words, strike) in &removed {
        let replacement = if *strike { STRICKEN_PLACEHOLDER } else { DELETED_PLACEHOLDER };
        redact_ai_outputs(&mut tx, meeting_id, words, replacement)
            .await
            .map_err(err("Failed to redact AI outputs"))?;
    }
    if src.is_some() {
        sqlx::query("DELETE FROM sync_apply").execute(&mut *tx).await.map_err(err("Database busy"))?;
    }
    tx.commit().await.map_err(err("Failed to save the edit"))?;

    let any_strike = removed.iter().any(|r| r.1);
    let changes: Vec<LineChange> = removed
        .into_iter()
        .map(|(words, _)| LineChange {
            transcript_id,
            original: line.text.clone(),
            new_text: text.clone(),
            removed: words,
        })
        .collect();
    let mut out = ActionOutcome::default();
    post_commit_purge(
        pool,
        env,
        meeting_id,
        &changes,
        Vec::new(),
        if any_strike { STRICKEN_PLACEHOLDER } else { DELETED_PLACEHOLDER },
        &mut out,
    )
    .await;
    for w in out.warnings {
        log::warn!("Sync edit purge: {}", w);
    }
    Ok(true)
}

/// Exactly what a Strike will destroy, for the confirmation dialog. Counts
/// only; the selected words are shown by the UI that selected them.
pub async fn preview_words(pool: &Pool<Sqlite>, env: &RedactionEnv, target: &WordTarget) -> Result<Vec<String>, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let target = &resolve_target(&mut conn, target).await?;
    let line = load_line(&mut conn, &target.meeting_id, target.transcript_id).await?;
    let edit = apply_word_edit(&line.text, line.timings.as_deref(), target.start, target.end, Some("x"))?;
    let n_words = edit.removed_plain.split_whitespace().count();
    let mut items = vec![format!(
        "{} {} from this transcript line{}, and from search",
        n_words,
        if n_words == 1 { "word" } else { "words" },
        if edit.whole_line { " (the whole line)" } else { "" }
    )];
    if edit.removed_ms.is_some() {
        items.push("The word timings for those words".into());
    }
    items.extend(ai_preview(&mut conn, &target.meeting_id).await);
    items.push("Any copies in the app's log files".into());
    items.extend(common_preview(env, &target.meeting_id));
    items.push(NO_AUDIO_NOTICE.into());
    Ok(items)
}

pub async fn preview_screens(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ids: &[String],
) -> Result<Vec<String>, String> {
    env.ensure_not_recording(meeting_id)?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let screens = resolve_screens(&mut conn, meeting_id, ids, false).await?;
    let n = screens.len();
    let files = screens.iter().filter(|s| s.file.as_deref().map_or(false, |f| !f.is_empty())).count();
    let mut items = vec![format!(
        "{} {} and {} image {}",
        n,
        if n == 1 { "screen" } else { "screens" },
        files,
        if files == 1 { "file" } else { "files" }
    )];
    let state_ids: Vec<&String> = screens
        .iter()
        .filter_map(|s| match &s.source {
            ScreenSource::State(id) => Some(id),
            _ => None,
        })
        .collect();
    if !state_ids.is_empty() {
        let sql = format!(
            "SELECT COUNT(*) FROM text_snapshots WHERE state_id IN ({})",
            placeholders(state_ids.len())
        );
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for id in &state_ids {
            q = q.bind(*id);
        }
        let snaps = q.fetch_one(&mut *conn).await.unwrap_or(0);
        items.push(format!(
            "The text read from {} ({} OCR/accessibility snapshot{} and nearby screen text)",
            if n == 1 { "it" } else { "them" },
            snaps,
            if snaps == 1 { "" } else { "s" }
        ));
    }
    items.push("AI (VLM) analysis of these screens and timeline entries built from them".into());
    items.push("Cached video frames and thumbnails for this recording".into());
    let ranges: Vec<_> = screens.iter().filter_map(screen_range).collect();
    items.extend(video_preview(env, meeting_id, &ranges));
    items.extend(common_preview(env, meeting_id));
    Ok(items)
}

async fn ai_preview(conn: &mut SqliteConnection, meeting_id: &str) -> Vec<String> {
    let mut items = Vec::new();
    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);
    let study = crate::study::count_for_meeting(conn, meeting_id).await;
    let chats: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assistant_conversations WHERE context_refs LIKE ?")
        .bind(format!("%transcript-{}-%", meeting_id))
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0);
    let total = notes + chats;
    if total > 0 {
        items.push(format!(
            "Mentions in {} saved AI output{} (notes, assistant chats); they'll be marked \"made before an edit\"",
            total,
            if total == 1 { "" } else { "s" }
        ));
    }
    if study > 0 {
        items.push(
            "The guide in REVIEW (summary, key terms, flashcards, quiz, questions): deleted; make it again after the edit"
                .into(),
        );
    }
    let topics = crate::topics::count_for_meeting(conn, meeting_id).await;
    if topics > 0 {
        items.push(format!(
            "{} AI topic{} on this recording: deleted; find them again after the edit (topics you added or renamed stay)",
            topics,
            if topics == 1 { "" } else { "s" }
        ));
    }
    let answers = crate::chat::count_for_meeting(conn, meeting_id).await;
    if answers > 0 {
        items.push(format!(
            "{} CHAT answer{} that drew on this recording: deleted (the chat is marked)",
            answers,
            if answers == 1 { "" } else { "s" }
        ));
    }
    items.push("Mentions in timeline entries for this recording (comments you wrote are left as they are)".into());
    items
}

/// The screen video line of a preview (DMG): which chunks cover the span,
/// and that the black frames are written in the background afterwards.
#[cfg(feature = "mas")]
fn video_preview(_env: &RedactionEnv, _meeting_id: &str, _ranges: &[(DateTime<Utc>, DateTime<Utc>)]) -> Vec<String> {
    Vec::new()
}

#[cfg(not(feature = "mas"))]
fn video_preview(env: &RedactionEnv, meeting_id: &str, ranges: &[(DateTime<Utc>, DateTime<Utc>)]) -> Vec<String> {
    if ranges.is_empty() || !has_screen_video(env, meeting_id) {
        return Vec::new();
    }
    let mut items = Vec::new();
    match video_blank::plan(env, meeting_id, ranges) {
        Ok(p) if !p.is_empty() => items.push(format!(
            "That span of the screen video ({} chunk{}): replaced with black frames in the background \
             right after (the database content goes first; the meeting view shows progress)",
            p.len(),
            if p.len() == 1 { "" } else { "s" }
        )),
        Ok(_) => {}
        Err(e) => items.push(format!(
            "Screen video: {} (the video is blanked in the background once it can be read; until then it \
             still holds that span, and the meeting view says so)",
            e
        )),
    }
    items
}

fn common_preview(env: &RedactionEnv, meeting_id: &str) -> Vec<String> {
    let _ = meeting_id;
    let mut items = Vec::new();
    let backups = find_backup_dbs(&env.backups_dir()).len();
    if backups > 0 {
        items.push(format!(
            "The same content in the app's {} backup database{} (purged, or the backup is deleted if it can't be)",
            backups,
            if backups == 1 { "" } else { "s" }
        ));
    }
    items.push("Freed database space is overwritten (secure delete, write-ahead log cleared)".into());
    items.push(EXPORTS_NOTICE.into());
    items.push(DEVICE_BACKUP_NOTICE.into());
    items
}

/// Plain-text lines for exports: one per screen strike.
pub fn screen_strike_lines(records: &[RedactionRecord]) -> Vec<(String, String)> {
    records
        .iter()
        .filter(|r| r.kind == "screen" && r.action == "strike")
        .map(|r| (r.media_start.clone().unwrap_or_else(|| r.created_at.clone()), SCREEN_STRICKEN_PLACEHOLDER.to_string()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri commands
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use tauri::{AppHandle, Emitter, Manager, State};

    /// Meetings being recorded now: the open capture session, plus (DMG)
    /// the meeting whose screen video is being written.
    fn recording_meetings(state: &AppState) -> Vec<String> {
        let mut out = Vec::new();
        if state.capture_engine.read().is_recording() {
            if let Some(m) = state.state_builder.read().current_meeting_id() {
                out.push(m);
            }
        }
        #[cfg(not(feature = "mas"))]
        if let Some(m) = crate::commands::video_recording_meeting() {
            out.push(m);
        }
        out
    }

    fn env_for(state: &AppState) -> RedactionEnv {
        RedactionEnv { recording_meetings: recording_meetings(state), ..RedactionEnv::for_app() }
    }

    pub(crate) fn env_from(app: &AppHandle) -> RedactionEnv {
        match app.try_state::<AppState>() {
            Some(st) => env_for(&st),
            None => RedactionEnv::for_app(),
        }
    }

    fn schedule_commit(app: AppHandle, state: &State<'_, AppState>, pending: &PendingDelete) {
        let db = state.database.clone();
        let id = pending.id.clone();
        let meeting_id = pending.meeting_id.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(UNDO_WINDOW_SECS * 1000 + 250)).await;
            let result = commit_pending(db.pool(), &env_from(&app), &id).await;
            emit_commit_result(&app, db.pool(), &id, &meeting_id, result).await;
            kick_video_jobs(&app);
        });
    }

    /// Start or wake the screen video worker (DMG; a no-op in the Mac App
    /// Store build, which records no video). It runs outside the redaction
    /// lock and reports `video_blank_progress` events to the UI.
    pub fn kick_video_jobs(app: &AppHandle) {
        #[cfg(not(feature = "mas"))]
        {
            let Some(state) = app.try_state::<AppState>() else { return };
            let pool = state.database.pool().clone();
            let app_env = app.clone();
            let env: std::sync::Arc<dyn Fn() -> RedactionEnv + Send + Sync> =
                std::sync::Arc::new(move || env_from(&app_env));
            let app_ev = app.clone();
            let report: video_jobs::Reporter = std::sync::Arc::new(move |ev: video_jobs::JobEvent| {
                let _ = app_ev.emit("video_blank_progress", ev);
            });
            video_jobs::kick(pool, env, std::sync::Arc::new(video_blank::FfmpegOps), report);
        }
        #[cfg(feature = "mas")]
        let _ = app;
    }

    /// At launch, once the app state is managed: resume interrupted jobs,
    /// give failed ones a fresh attempt, and start the worker.
    pub fn start_video_worker(app: &AppHandle) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(state) = app.try_state::<AppState>() {
                if let Err(e) = video_jobs::prepare_for_launch(state.database.pool()).await {
                    log::warn!("{}", e);
                }
            }
            kick_video_jobs(&app);
        });
    }

    /// `redaction_committed`, or `redaction_failed` with `retryable` (the
    /// delete is still pending and listed by `list_failed_redactions`) or not
    /// (the target changed; the delete was dropped).
    async fn emit_commit_result(
        app: &AppHandle,
        pool: &Pool<Sqlite>,
        id: &str,
        meeting_id: &str,
        result: Result<Option<ActionOutcome>, String>,
    ) {
        match result {
            Ok(Some(outcome)) => {
                let _ = app.emit(
                    "redaction_committed",
                    serde_json::json!({ "id": id, "meeting_id": meeting_id, "warnings": outcome.warnings }),
                );
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("Delete {} failed: {}", id, e);
                let retryable = is_pending(pool, id).await;
                let _ = app.emit(
                    "redaction_failed",
                    serde_json::json!({ "id": id, "meeting_id": meeting_id, "error": e, "retryable": retryable }),
                );
            }
        }
    }

    fn target(
        meeting_id: String,
        transcript_id: i64,
        start_char: usize,
        end_char: usize,
        expected_text: Option<String>,
    ) -> WordTarget {
        WordTarget { meeting_id, transcript_id, start: start_char, end: end_char, expected_text, whole_line: false }
    }

    /// The whole line; its range is resolved inside the redaction lock.
    fn whole_line(meeting_id: &str, transcript_id: i64) -> WordTarget {
        WordTarget {
            meeting_id: meeting_id.to_string(),
            transcript_id,
            start: 0,
            end: 0,
            expected_text: None,
            whole_line: true,
        }
    }

    /// Delete words (UTF-16 offsets into the line). Returns a pending delete
    /// with a 5s undo window; it commits on its own afterwards.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_transcript_words(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        transcript_id: i64,
        start_char: usize,
        end_char: usize,
        expected_text: Option<String>,
    ) -> Result<PendingDelete, String> {
        let t = target(meeting_id, transcript_id, start_char, end_char, expected_text);
        let pending = request_delete_words(state.database.pool(), &env_for(&state), &t).await?;
        schedule_commit(app, &state, &pending);
        Ok(pending)
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn strike_transcript_words(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        transcript_id: i64,
        start_char: usize,
        end_char: usize,
        reason: Option<String>,
        expected_text: Option<String>,
    ) -> Result<ActionOutcome, String> {
        let t = target(meeting_id, transcript_id, start_char, end_char, expected_text);
        let out = strike_words(state.database.pool(), &env_for(&state), &t, reason.as_deref()).await;
        kick_video_jobs(&app);
        out
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_transcript_line(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        transcript_id: i64,
    ) -> Result<PendingDelete, String> {
        let t = whole_line(&meeting_id, transcript_id);
        let pending = request_delete_words(state.database.pool(), &env_for(&state), &t).await?;
        schedule_commit(app, &state, &pending);
        Ok(pending)
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn strike_transcript_line(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        transcript_id: i64,
        reason: Option<String>,
    ) -> Result<ActionOutcome, String> {
        let t = whole_line(&meeting_id, transcript_id);
        let out = strike_words(state.database.pool(), &env_for(&state), &t, reason.as_deref()).await;
        kick_video_jobs(&app);
        out
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_screens(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        ids: Vec<String>,
    ) -> Result<PendingDelete, String> {
        let pending = request_delete_screens(state.database.pool(), &env_for(&state), &meeting_id, &ids).await?;
        schedule_commit(app, &state, &pending);
        Ok(pending)
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn strike_screens(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        ids: Vec<String>,
        reason: Option<String>,
    ) -> Result<ActionOutcome, String> {
        let out =
            super::strike_screens(state.database.pool(), &env_for(&state), &meeting_id, &ids, reason.as_deref()).await;
        kick_video_jobs(&app);
        out
    }

    /// What deleting/striking a block of time removes (exact counts).
    /// `startMs`/`endMs` are offsets from the meeting start.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn preview_time_range(
        state: State<'_, AppState>,
        meeting_id: String,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<time_range::TimeRangePreview, String> {
        time_range::preview_time_range(state.database.pool(), &env_for(&state), &meeting_id, start_ms, end_ms).await
    }

    /// Delete a block of time (5-second undo, like every Delete). `expected`
    /// is the preview's counts; the delete is refused if they changed.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_time_range(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        start_ms: i64,
        end_ms: i64,
        expected: Option<time_range::RangeCounts>,
    ) -> Result<PendingDelete, String> {
        let pending = time_range::request_delete_time_range(
            state.database.pool(),
            &env_for(&state),
            &meeting_id,
            start_ms,
            end_ms,
            expected,
        )
        .await?;
        schedule_commit(app, &state, &pending);
        Ok(pending)
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn strike_time_range(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        start_ms: i64,
        end_ms: i64,
        reason: Option<String>,
        expected: Option<time_range::RangeCounts>,
    ) -> Result<ActionOutcome, String> {
        let out = time_range::strike_time_range(
            state.database.pool(),
            &env_for(&state),
            &meeting_id,
            start_ms,
            end_ms,
            reason.as_deref(),
            expected,
        )
        .await;
        kick_video_jobs(&app);
        out
    }

    /// What deleting/striking several blocks of time removes, together
    /// (a linked selection of screens and transcript lines): exact totals.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn preview_time_ranges(
        state: State<'_, AppState>,
        meeting_id: String,
        ranges: Vec<time_range::MsRange>,
    ) -> Result<time_range::TimeRangePreview, String> {
        time_range::preview_time_ranges(state.database.pool(), &env_for(&state), &meeting_id, &ranges).await
    }

    /// Delete several blocks of time as one action: one pending delete, one
    /// 5-second undo. `expected` is the preview's counts.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_time_ranges(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        ranges: Vec<time_range::MsRange>,
        expected: Option<time_range::RangeCounts>,
    ) -> Result<PendingDelete, String> {
        let pending = time_range::request_delete_time_ranges(
            state.database.pool(),
            &env_for(&state),
            &meeting_id,
            &ranges,
            expected,
        )
        .await?;
        schedule_commit(app, &state, &pending);
        Ok(pending)
    }

    /// Strike several blocks of time as one action (no undo); markers for
    /// each range.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn strike_time_ranges(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        ranges: Vec<time_range::MsRange>,
        reason: Option<String>,
        expected: Option<time_range::RangeCounts>,
    ) -> Result<ActionOutcome, String> {
        let out = time_range::strike_time_ranges(
            state.database.pool(),
            &env_for(&state),
            &meeting_id,
            &ranges,
            reason.as_deref(),
            expected,
        )
        .await;
        kick_video_jobs(&app);
        out
    }

    /// Screen video ranges still being blanked (or failed) for a meeting.
    /// Times only.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_video_blank_jobs(
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<Vec<video_jobs::VideoJobInfo>, String> {
        video_jobs::list(state.database.pool(), Some(&meeting_id)).await
    }

    /// Retry button for the screen video banner.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn retry_video_blank_jobs(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<Vec<video_jobs::VideoJobInfo>, String> {
        video_jobs::retry_now(state.database.pool(), Some(&meeting_id)).await?;
        kick_video_jobs(&app);
        video_jobs::list(state.database.pool(), Some(&meeting_id)).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn undo_redaction(state: State<'_, AppState>, id: String) -> Result<(), String> {
        undo_delete(state.database.pool(), &id).await
    }

    /// Apply a pending Delete now instead of waiting out the undo window.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn commit_redaction(
        app: AppHandle,
        state: State<'_, AppState>,
        id: String,
    ) -> Result<Option<ActionOutcome>, String> {
        let out = commit_pending(state.database.pool(), &env_for(&state), &id).await;
        kick_video_jobs(&app);
        out
    }

    /// Deletes whose undo window ended but that couldn't be applied yet
    /// (shown in the meeting view with a Retry button).
    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_failed_redactions(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<FailedDelete>, String> {
        list_failed_deletes(state.database.pool(), &meeting_id).await
    }

    /// Retry this meeting's failed deletes. Returns the ones still failing.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn retry_failed_redactions(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<Vec<FailedDelete>, String> {
        let errors = retry_failed_deletes(state.database.pool(), &env_for(&state), &meeting_id).await;
        kick_video_jobs(&app);
        for e in &errors {
            log::warn!("Delete retry failed: {}", e);
        }
        list_failed_deletes(state.database.pool(), &meeting_id).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_redactions(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<RedactionRecord>, String> {
        list_strikes(state.database.pool(), &meeting_id).await.map_err(err("Failed to load markers"))
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn preview_strike_words(
        state: State<'_, AppState>,
        meeting_id: String,
        transcript_id: i64,
        start_char: Option<usize>,
        end_char: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let t = match (start_char, end_char) {
            (Some(s), Some(e)) => target(meeting_id, transcript_id, s, e, None),
            _ => whole_line(&meeting_id, transcript_id),
        };
        preview_words(state.database.pool(), &env_for(&state), &t).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn preview_strike_screens(
        state: State<'_, AppState>,
        meeting_id: String,
        ids: Vec<String>,
    ) -> Result<Vec<String>, String> {
        preview_screens(state.database.pool(), &env_for(&state), &meeting_id, &ids).await
    }

    /// Whether this meeting's saved AI outputs predate an edit.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn get_meeting_ai_status(state: State<'_, AppState>, meeting_id: String) -> Result<serde_json::Value, String> {
        let pool = state.database.pool();
        let notes: Option<i64> = sqlx::query_scalar(
            "SELECT stale_after_edit FROM meeting_notes WHERE meeting_id = ? ORDER BY generated_at DESC LIMIT 1",
        )
        .bind(&meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(err("Database busy"))?;
        let study: Option<i64> = sqlx::query_scalar(
            "SELECT stale_after_edit FROM study_materials WHERE meeting_id = ? ORDER BY generated_at DESC LIMIT 1",
        )
        .bind(&meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(err("Database busy"))?;
        Ok(serde_json::json!({
            "has_notes": notes.is_some(),
            "notes_stale": notes.unwrap_or(0) != 0,
            "has_study": study.is_some(),
            "study_stale": study.unwrap_or(0) != 0,
        }))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests;
