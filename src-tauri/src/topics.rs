//! Topics (docs/TOPICS_AND_CHAT.md): one to four short labels per recording
//! ("Q4 roadmap", "Cell membranes", "Insurance renewal"), found by the
//! user's AI from the transcript and stored locally, so the Recordings list
//! can be filtered and grouped by what was talked about.
//!
//! - AI: every request goes through `ai::complete_text` (Apple on-device or
//!   the one user-entered endpoint), via the same `Completer` the Review
//!   guide uses. Consent and, in the Mac App Store build, the Pro check
//!   happen there; nothing here adds or skips a gate.
//! - Input: the transcript exactly as the Review guide reads it
//!   (`study::load_input`): Whisper-filtered at transcription time, stricken
//!   spans as `[stricken from the record]`, deleted words gone. Long
//!   recordings are read chunk by chunk and the labels merged.
//! - Output: strict JSON, validated and cleaned (`validate`); one retry; the
//!   model's text is never interpreted, only shown.
//! - Normalization: every label gets a `topic_key` (lowercase, punctuation
//!   gone, plurals folded). A new key within one edit of a key another
//!   recording already has adopts that key and its label, so the topic list
//!   stays small.
//! - User edits: `set_for_meeting` stores the user's own labels with
//!   `source = 'user'`; "Find topics" never removes or renames those, it
//!   only fills the remaining slots.
//! - Purge: a recording's topics go with the recording, and any Delete or
//!   Strike of transcript text deletes all of them (`purge_for_meeting`, in
//!   `redaction::redact_ai_outputs`, live database and app backups), like
//!   the Review guide. See docs/REDACTION.md. Transcript text and model
//!   output are never logged.

#[cfg(test)]
mod tests;

use crate::ai::{AiError, Msg, Opts};
use crate::study::prompt::{self as sprompt, StudyInput};
use crate::study::{Completer, LiveCompleter};
use serde::Serialize;
use serde_json::Value;
use sqlx::sqlite::SqliteConnection;
use sqlx::{Acquire, Pool, Row, Sqlite};
use std::collections::BTreeMap;

/// Topics the AI stores per recording (the user can add more, up to `MAX_USER_TOPICS`).
pub const MAX_TOPICS: usize = 4;
/// Topics a user may keep on one recording.
pub const MAX_USER_TOPICS: usize = 8;
/// AI candidates below this confidence aren't stored (shared with iOS).
pub const MIN_CONFIDENCE: f64 = 0.3;
/// Shared with iOS: at most 40 characters and 6 words.
pub const MAX_LABEL_CHARS: usize = 40;
pub const MAX_LABEL_WORDS: usize = 6;
const MIN_LABEL_CHARS: usize = 2;
/// Answer budget: four short labels with confidences.
const MAX_TOKENS: u32 = 300;

pub const NO_TRANSCRIPT: &str = "This recording has no transcript to find topics in.";

/// Labels that name the recording rather than what it was about.
const GENERIC: &[&str] = &[
    "meeting", "meetings", "discussion", "conversation", "recording", "general", "misc", "miscellaneous",
    "other", "various topics", "various", "notes", "topics", "chat", "call", "class", "lecture", "intro",
    "introduction", "update", "updates", "sync", "agenda", "none", "unknown", "n/a", "overview", "summary",
];

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_topics (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            topic TEXT NOT NULL,
            topic_key TEXT NOT NULL,
            confidence REAL,
            source TEXT NOT NULL DEFAULT 'ai' CHECK (source IN ('ai', 'user')),
            created_at TEXT NOT NULL,
            UNIQUE (meeting_id, topic_key)
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_meeting_topics_key ON meeting_topics(topic_key)")
        .execute(&mut *conn)
        .await?;
    // AI topics the user removed from a recording: "Find topics" never
    // brings them back (shared rule with iOS)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_topics_removed (
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            topic_key TEXT NOT NULL,
            PRIMARY KEY (meeting_id, topic_key)
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keys the user removed from this recording.
async fn removed_keys(conn: &mut SqliteConnection, meeting_id: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar("SELECT topic_key FROM meeting_topics_removed WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(err("Couldn't read the topics"))
}

/// With the recording: forget which topics the user removed from it.
pub async fn purge_removed_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), sqlx::Error> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meeting_topics_removed'")
            .fetch_optional(&mut *conn)
            .await?;
    if exists.is_some() {
        sqlx::query("DELETE FROM meeting_topics_removed WHERE meeting_id = ?").bind(meeting_id).execute(&mut *conn).await?;
    }
    Ok(())
}

async fn table_exists(conn: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meeting_topics'")
            .fetch_optional(&mut *conn)
            .await?;
    Ok(exists.is_some())
}

/// Purge: delete the AI's topics of a meeting (a topic paraphrases the
/// transcript); the user's own topics stay, like comments (shared rule
/// with iOS). Called for every Delete/Strike of transcript text, in the
/// action's transaction, on the live database and on each app backup
/// (`redaction::redact_ai_outputs`). Schema-tolerant: an old backup may
/// not have the table.
pub async fn purge_ai_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<u64, sqlx::Error> {
    if !table_exists(conn).await? {
        return Ok(0);
    }
    Ok(sqlx::query("DELETE FROM meeting_topics WHERE meeting_id = ? AND source = 'ai'")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await?
        .rows_affected())
}

/// With the recording: every topic, the AI's and the user's, and the
/// removed-topic memory (`DatabaseManager::delete_meeting`).
pub async fn purge_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<u64, sqlx::Error> {
    if !table_exists(conn).await? {
        return Ok(0);
    }
    let n = sqlx::query("DELETE FROM meeting_topics WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    purge_removed_for_meeting(conn, meeting_id).await?;
    Ok(n)
}

/// AI topics a transcript purge would delete (for the Delete/Strike preview).
pub async fn count_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics WHERE meeting_id = ? AND source = 'ai'")
        .bind(meeting_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════════════════════
// Normalization (pure)
// ═══════════════════════════════════════════════════════════════════════════

/// One word's matching form: lowercase, a trailing plural "s" folded
/// ("membranes" → "membrane", but "class", "bus" and "q4" stay).
fn fold_word(w: &str) -> String {
    let w = w.to_lowercase();
    let n = w.chars().count();
    if n > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") {
        w[..w.len() - 1].to_string()
    } else {
        w
    }
}

/// The matching key of a label: lowercase words (letters and digits only,
/// apostrophes dropped), plurals folded, single spaces. "" for no words.
pub fn topic_key(label: &str) -> String {
    label
        .replace(['\'', '’'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(fold_word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Levenshtein distance on chars, bounded (stops early above `max`).
fn edit_distance_within(a: &str, b: &str, max: usize) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > max {
        return false;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        let mut row_min = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            row_min = row_min.min(cur[j]);
        }
        if row_min > max {
            return false;
        }
        prev = cur;
    }
    prev[b.len()] <= max
}

/// Two keys name the same topic: equal, equal without spaces ("off site"
/// / "offsite"), or (for keys of six characters or more without digits)
/// within one edit of each other ("kubernetes" / "kubernates"). A digit is
/// never a typo: "q4 roadmap" is not "q3 roadmap".
pub fn near(a: &str, b: &str) -> bool {
    if a == b || a.replace(' ', "") == b.replace(' ', "") {
        return true;
    }
    let no_digits = |s: &str| !s.chars().any(|c| c.is_ascii_digit());
    a.chars().count() >= 6 && b.chars().count() >= 6 && no_digits(a) && no_digits(b) && edit_distance_within(a, b, 1)
}

/// The display label for a raw label: cleaned (one line, no control
/// characters, no list markers), trailing period gone, at most
/// `MAX_LABEL_CHARS`, first letter capitalized when the whole label is
/// lowercase ("q4 roadmap" → "Q4 roadmap"; "BIO 101" stays). None when
/// empty, too short, or generic ("meeting").
pub fn display_label(raw: &str) -> Option<String> {
    let s = crate::study::parse::clean_text(&Value::String(raw.to_string()), MAX_LABEL_CHARS)?;
    let s = s
        .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '"' | '“' | '”' | '\'' | '`' | '*'))
        .trim()
        .to_string();
    let key = topic_key(&s);
    let lower = s.to_lowercase();
    if key.chars().count() < MIN_LABEL_CHARS || GENERIC.contains(&key.as_str()) || GENERIC.contains(&lower.as_str()) {
        return None;
    }
    if s.split_whitespace().count() > MAX_LABEL_WORDS {
        return None;
    }
    if s.chars().all(|c| !c.is_uppercase()) {
        let mut cs = s.chars();
        let first = cs.next()?.to_uppercase().collect::<String>();
        return Some(format!("{}{}", first, cs.as_str()));
    }
    Some(s)
}

/// A found topic before storage.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub label: String,
    pub key: String,
    pub confidence: f64,
}

/// Validate a model answer: `{"topics": [{"label", "confidence"}]}` (also a
/// bare array, or an array of strings). Labels are cleaned, generic ones
/// dropped, duplicates (by key) merged keeping the higher confidence; at
/// most `MAX_TOPICS`, highest confidence first. Error when nothing usable.
pub fn validate(raw: &str) -> Result<Vec<Found>, String> {
    let v = crate::study::parse::extract_json(raw)?;
    let list: Vec<Value> = match &v {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("topics")
            .or_else(|| o.get("labels"))
            .and_then(|x| x.as_array())
            .cloned()
            .ok_or("The answer had no topics list")?,
        _ => return Err("The answer had no topics list".into()),
    };
    let mut out: Vec<Found> = Vec::new();
    for item in list.iter().take(40) {
        let (label_v, conf) = match item {
            Value::String(_) | Value::Number(_) => (item.clone(), 0.5),
            Value::Object(o) => {
                let Some(l) = o.get("label").or_else(|| o.get("topic")).or_else(|| o.get("name")) else { continue };
                let c = o.get("confidence").or_else(|| o.get("score")).and_then(|c| c.as_f64()).unwrap_or(0.5);
                (l.clone(), c)
            }
            _ => continue,
        };
        let label = match &label_v {
            Value::String(s) => display_label(s),
            Value::Number(n) => display_label(&n.to_string()),
            _ => None,
        };
        let Some(label) = label else { continue };
        let key = topic_key(&label);
        let conf = if conf.is_finite() { conf.clamp(0.0, 1.0) } else { 0.5 };
        if let Some(existing) = out.iter_mut().find(|f| near(&f.key, &key)) {
            existing.confidence = existing.confidence.max(conf);
            continue;
        }
        out.push(Found { label, key, confidence: conf });
    }
    if out.is_empty() {
        return Err("The answer had no usable topics".into());
    }
    // Stable: equal confidences keep the model's order (most important first)
    out.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(MAX_TOPICS);
    Ok(out)
}

/// Merge the topics found in several parts of one recording: by key, the
/// confidences add up (a topic that comes up in every part ranks first),
/// capped at 1 and `MAX_TOPICS` entries.
pub fn merge_parts(parts: &[Vec<Found>]) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let n = parts.len().max(1) as f64;
    for part in parts {
        for f in part {
            if let Some(e) = out.iter_mut().find(|e| near(&e.key, &f.key)) {
                e.confidence += f.confidence / n;
            } else {
                out.push(Found { label: f.label.clone(), key: f.key.clone(), confidence: f.confidence / n });
            }
        }
    }
    for f in out.iter_mut() {
        f.confidence = f.confidence.min(1.0);
    }
    out.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(MAX_TOPICS);
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// Prompt
// ═══════════════════════════════════════════════════════════════════════════

pub fn system_prompt() -> String {
    format!(
        "You label a transcript with its main topics. Reply with only one JSON object in this shape: \
{{\"topics\": [{{\"label\": \"short noun phrase\", \"confidence\": 0.9}}]}}. 1 to {} topics, the most \
important first; \"confidence\" is 0 to 1. Each label is a short noun phrase of 1 to 4 words naming \
what was talked about (for example \"Q4 roadmap\", \"Cell membranes\", \"Insurance renewal\"), never a \
sentence, and never a generic word such as \"meeting\" or \"discussion\". Use only what the transcript \
says. The transcript comes from speech recognition and may contain errors. Text shown as \
[stricken from the record] was removed by the user: never guess at or mention what it said. No \
Markdown, no code fence, no text before or after the JSON.",
        MAX_TOPICS
    )
}

/// The user message for one part of the transcript.
pub fn user_message(input: &StudyInput, part: usize, parts: usize, chunk: &str) -> String {
    let head = sprompt::header(input);
    if parts > 1 {
        format!("{}Part {} of {}\n\nTRANSCRIPT:\n{}", head, part, parts, chunk)
    } else {
        format!("{}\nTRANSCRIPT:\n{}", head, chunk)
    }
}

fn retry_note(why: &str) -> String {
    format!(
        "Your previous answer couldn't be used ({}). Answer again with only the JSON object in the shape \
described, and nothing else.",
        why
    )
}

fn answer_error(e: &AiError) -> bool {
    matches!(e, AiError::Truncated | AiError::Other(_))
}

fn short(e: &AiError) -> String {
    match e {
        AiError::Truncated => "the answer was cut off".into(),
        AiError::Other(m) => m.chars().take(160).collect(),
        other => other.class().into(),
    }
}

/// Ask for one part; validate; on a bad answer ask once more. AI access
/// errors (consent, Pro, no provider…) come back as their own string so the
/// UI can act on them.
async fn ask(c: &dyn Completer, user: &str, max_tokens: u32) -> Result<Result<Vec<Found>, String>, String> {
    let system = system_prompt();
    let mut why = String::new();
    for attempt in 0..2 {
        let text = if attempt == 0 { user.to_string() } else { format!("{}\n\n{}", user, retry_note(&why)) };
        let opts = Opts { max_tokens, temperature: Some(if attempt == 0 { 0.1 } else { 0.3 }) };
        match c.complete(vec![Msg::system(system.clone()), Msg::user(text)], opts).await {
            Ok(raw) => match validate(&raw) {
                Ok(v) => return Ok(Ok(v)),
                Err(e) => why = e,
            },
            Err(e) if answer_error(&e) => why = short(&e),
            Err(e) => return Err(e.to_string()),
        }
    }
    log::warn!("Topics: the answer was unusable twice");
    Ok(Err(format!("Couldn't find topics: {} (asked twice). Try again, or choose a larger model in Settings → AI Engine.", why)))
}

/// Find the topics of a transcript. A long recording is read in parts that
/// fit the model's window, and the parts' topics merged.
pub async fn generate(c: &dyn Completer, input: &StudyInput) -> Result<Vec<Found>, String> {
    if !input.has_transcript() {
        return Err(NO_TRANSCRIPT.into());
    }
    let ctx = c.context_tokens();
    let max_tokens = MAX_TOKENS.min((ctx / 8).max(120) as u32);
    let fixed = user_message(input, 99, 99, "");
    let budget = sprompt::body_budget(ctx, max_tokens, &system_prompt(), &fixed);
    let lines = input.transcript_lines();
    // Reading more than eight parts adds little: the topics repeat
    let chunks = {
        let mut c = sprompt::chunk_lines(&lines, budget);
        if c.len() > 8 {
            let step = c.len() as f64 / 8.0;
            c = (0..8).map(|i| c[(i as f64 * step) as usize].clone()).collect();
        }
        c
    };
    let mut parts: Vec<Vec<Found>> = Vec::new();
    let mut last_err = None;
    for (i, chunk) in chunks.iter().enumerate() {
        let msg = user_message(input, i + 1, chunks.len(), chunk);
        match ask(c, &msg, max_tokens).await? {
            Ok(found) => parts.push(found),
            Err(e) => last_err = Some(e),
        }
    }
    if parts.is_empty() {
        return Err(last_err.unwrap_or_else(|| "Couldn't find topics".into()));
    }
    Ok(merge_parts(&parts))
}

// ═══════════════════════════════════════════════════════════════════════════
// Storage
// ═══════════════════════════════════════════════════════════════════════════

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeetingTopic {
    pub id: String,
    pub meeting_id: String,
    pub label: String,
    pub key: String,
    pub confidence: Option<f64>,
    /// "ai" | "user"
    pub source: String,
    pub created_at: String,
}

fn topic_from_row(r: &sqlx::sqlite::SqliteRow) -> MeetingTopic {
    MeetingTopic {
        id: r.get("id"),
        meeting_id: r.get("meeting_id"),
        label: r.get("topic"),
        key: r.get("topic_key"),
        confidence: r.get("confidence"),
        source: r.get("source"),
        created_at: r.get("created_at"),
    }
}

const SELECT: &str = "SELECT id, meeting_id, topic, topic_key, confidence, source, created_at FROM meeting_topics";

pub async fn list_for_meeting_conn(conn: &mut SqliteConnection, meeting_id: &str) -> Result<Vec<MeetingTopic>, String> {
    let rows = sqlx::query(&format!(
        "{} WHERE meeting_id = ? ORDER BY CASE source WHEN 'user' THEN 0 ELSE 1 END, confidence DESC, created_at ASC, rowid ASC",
        SELECT
    ))
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read the topics"))?;
    Ok(rows.iter().map(topic_from_row).collect())
}

pub async fn list_for_meeting(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<Vec<MeetingTopic>, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    list_for_meeting_conn(&mut conn, meeting_id).await
}

/// Every distinct key in the store with its most used label.
async fn known_keys(conn: &mut SqliteConnection) -> Result<Vec<(String, String)>, String> {
    let rows = sqlx::query(
        "SELECT topic_key, topic, COUNT(*) AS n FROM meeting_topics GROUP BY topic_key, topic ORDER BY n DESC, topic ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read the topics"))?;
    let mut out: Vec<(String, String)> = Vec::new();
    for r in rows {
        let k: String = r.get("topic_key");
        if !out.iter().any(|(x, _)| *x == k) {
            out.push((k, r.get("topic")));
        }
    }
    Ok(out)
}

/// The stored form of a label: the key and label of a known near-duplicate
/// topic when there is one (so chips merge across recordings), else its own.
pub fn canonical(label: &str, known: &[(String, String)]) -> (String, String) {
    let key = topic_key(label);
    match known.iter().find(|(k, _)| near(k, &key)) {
        Some((k, l)) => (k.clone(), l.clone()),
        None => (key, label.to_string()),
    }
}

async fn insert(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    key: &str,
    label: &str,
    confidence: Option<f64>,
    source: &str,
    now: &str,
) -> Result<bool, String> {
    let r = sqlx::query(
        "INSERT OR IGNORE INTO meeting_topics (id, meeting_id, topic, topic_key, confidence, source, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .bind(meeting_id)
    .bind(label)
    .bind(key)
    .bind(confidence)
    .bind(source)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(err("Couldn't save the topics"))?;
    Ok(r.rows_affected() > 0)
}

/// Store the AI's topics for a meeting, replacing its earlier AI topics and
/// keeping the user's. Inside one `BEGIN IMMEDIATE` transaction it first
/// checks the transcript still reads as it did when generation started
/// (`fingerprint`) and has no Delete in its undo window: labels made from
/// text that has since been deleted or stricken are never written after
/// the purge ran.
pub async fn save_ai_topics(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    fingerprint: &str,
    found: &[Found],
) -> Result<Vec<MeetingTopic>, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await.map_err(err("Database busy"))?;
    let result = save_ai_locked(&mut conn, meeting_id, fingerprint, found).await;
    let end = if result.is_ok() { "COMMIT" } else { "ROLLBACK" };
    match sqlx::query(end).execute(&mut *conn).await {
        Ok(_) => result,
        Err(e) => {
            let _ = conn.close().await;
            result.and(Err(format!("Couldn't save the topics: {}", e)))
        }
    }
}

async fn save_ai_locked(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    fingerprint: &str,
    found: &[Found],
) -> Result<Vec<MeetingTopic>, String> {
    if crate::study::has_pending_delete(conn, meeting_id).await? {
        return Err(crate::study::TRANSCRIPT_CHANGED.into());
    }
    let now_input = crate::study::load_input_conn(conn, meeting_id).await?;
    if now_input.fingerprint() != fingerprint {
        return Err(crate::study::TRANSCRIPT_CHANGED.into());
    }
    sqlx::query("DELETE FROM meeting_topics WHERE meeting_id = ? AND source = 'ai'")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await
        .map_err(err("Couldn't save the topics"))?;
    let user = list_for_meeting_conn(conn, meeting_id).await?;
    let known = known_keys(conn).await?;
    let removed = removed_keys(conn, meeting_id).await?;
    let room = MAX_TOPICS.saturating_sub(user.len());
    let now = chrono::Utc::now().to_rfc3339();
    let mut added = 0;
    for f in found.iter().filter(|f| f.confidence >= MIN_CONFIDENCE) {
        if added >= room {
            break;
        }
        let (key, label) = canonical(&f.label, &known);
        if user.iter().any(|u| near(&u.key, &key)) || removed.iter().any(|r| near(r, &key)) {
            continue;
        }
        if insert(conn, meeting_id, &key, &label, Some(f.confidence), "ai", &now).await? {
            added += 1;
        }
    }
    list_for_meeting_conn(conn, meeting_id).await
}

/// Replace a meeting's topics with the user's own labels (all stored as
/// `source = 'user'`, so "Find topics" leaves them alone). Labels are
/// cleaned like the AI's; empty or generic ones are dropped; at most
/// `MAX_USER_TOPICS`. An empty list clears the recording's topics. AI
/// topics missing from the new list are remembered as removed, so "Find
/// topics" doesn't bring them back; adding one again forgets that.
pub async fn set_for_meeting(pool: &Pool<Sqlite>, meeting_id: &str, labels: &[String]) -> Result<Vec<MeetingTopic>, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Database busy"))?;
    if exists.is_none() {
        return Err("That recording no longer exists".into());
    }
    let known = known_keys(&mut conn).await?;
    let before = list_for_meeting_conn(&mut conn, meeting_id).await?;
    let mut tx = conn.begin().await.map_err(err("Database busy"))?;
    sqlx::query("DELETE FROM meeting_topics WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save the topics"))?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut n = 0;
    let mut kept: Vec<String> = Vec::new();
    for raw in labels {
        if n >= MAX_USER_TOPICS {
            break;
        }
        let Some(label) = display_label(raw) else { continue };
        let (key, label) = canonical(&label, &known);
        if insert(&mut tx, meeting_id, &key, &label, None, "user", &now).await? {
            n += 1;
            kept.push(key);
        }
    }
    for k in &kept {
        sqlx::query("DELETE FROM meeting_topics_removed WHERE meeting_id = ? AND topic_key = ?")
            .bind(meeting_id)
            .bind(k)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save the topics"))?;
    }
    for t in before.iter().filter(|t| !kept.iter().any(|k| near(k, &t.key))) {
        sqlx::query("INSERT OR IGNORE INTO meeting_topics_removed (meeting_id, topic_key) VALUES (?, ?)")
            .bind(meeting_id)
            .bind(&t.key)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save the topics"))?;
    }
    tx.commit().await.map_err(err("Couldn't save the topics"))?;
    list_for_meeting_conn(&mut conn, meeting_id).await
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicSummary {
    pub key: String,
    pub label: String,
    /// Recordings with this topic
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicRef {
    pub key: String,
    pub label: String,
}

/// The topic index: every topic with how many recordings have it (most
/// used first), and each recording's topics.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct TopicIndex {
    pub topics: Vec<TopicSummary>,
    pub by_meeting: BTreeMap<String, Vec<TopicRef>>,
}

pub async fn index(pool: &Pool<Sqlite>) -> Result<TopicIndex, String> {
    let rows = sqlx::query(
        "SELECT meeting_id, topic, topic_key FROM meeting_topics ORDER BY CASE source WHEN 'user' THEN 0 ELSE 1 END, confidence DESC, created_at ASC, rowid ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(err("Couldn't read the topics"))?;
    let mut by_meeting: BTreeMap<String, Vec<TopicRef>> = BTreeMap::new();
    // key → (label → uses)
    let mut labels: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for r in rows {
        let key: String = r.get("topic_key");
        let label: String = r.get("topic");
        *labels.entry(key.clone()).or_default().entry(label.clone()).or_default() += 1;
        *counts.entry(key.clone()).or_default() += 1;
        by_meeting.entry(r.get("meeting_id")).or_default().push(TopicRef { key, label });
    }
    let best: BTreeMap<String, String> = labels
        .into_iter()
        .map(|(k, ls)| {
            let l = ls.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(l, _)| l).unwrap_or_default();
            (k, l)
        })
        .collect();
    for refs in by_meeting.values_mut() {
        for t in refs.iter_mut() {
            if let Some(l) = best.get(&t.key) {
                t.label = l.clone();
            }
        }
    }
    let mut topics: Vec<TopicSummary> = counts
        .into_iter()
        .map(|(key, count)| TopicSummary { label: best.get(&key).cloned().unwrap_or_else(|| key.clone()), key, count })
        .collect();
    topics.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())));
    Ok(TopicIndex { topics, by_meeting })
}

/// Find and store a recording's topics with `c`. Shared by the "Find
/// topics" command and the notes hook.
pub async fn find_and_save(pool: &Pool<Sqlite>, c: &dyn Completer, meeting_id: &str) -> Result<Vec<MeetingTopic>, String> {
    {
        let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
        if crate::study::has_pending_delete(&mut conn, meeting_id).await? {
            return Err(crate::study::PENDING_EDIT.into());
        }
    }
    let input = crate::study::load_input(pool, meeting_id).await?;
    let fingerprint = input.fingerprint();
    let found = generate(c, &input).await?;
    log::info!("Topics: {} found for a {}-line transcript", found.len(), input.lines.len());
    save_ai_topics(pool, meeting_id, &fingerprint, &found).await
}

/// After notes are written for a recording: find its topics in the
/// background with the user's AI (the same gates as the notes). Failures
/// are logged, never surfaced; the notes don't depend on this.
pub fn find_after_notes(db: std::sync::Arc<crate::database::DatabaseManager>, meeting_id: String) {
    tokio::spawn(async move {
        if !commands::begin(&meeting_id) {
            return;
        }
        let _guard = commands::RunGuard(meeting_id.clone());
        match find_and_save(db.pool(), &LiveCompleter, &meeting_id).await {
            Ok(t) => log::info!("Topics: {} stored after notes", t.len()),
            Err(e) => log::warn!("Topics after notes not found: {}", e.chars().take(160).collect::<String>()),
        }
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri commands
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use std::collections::HashSet;
    use tauri::State;

    /// Meetings with a search running (one at a time per meeting).
    static RUNNING: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

    pub(super) fn begin(meeting_id: &str) -> bool {
        RUNNING.lock().insert(meeting_id.to_string())
    }

    pub(super) struct RunGuard(pub(super) String);
    impl Drop for RunGuard {
        fn drop(&mut self) {
            RUNNING.lock().remove(&self.0);
        }
    }

    fn recording_meeting(state: &AppState) -> Option<String> {
        if !state.capture_engine.read().is_recording() {
            return None;
        }
        state.state_builder.read().current_meeting_id()
    }

    /// Every topic with its recording count, and each recording's topics.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_topics(state: State<'_, AppState>) -> Result<TopicIndex, String> {
        index(state.database.pool()).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn get_meeting_topics(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<MeetingTopic>, String> {
        list_for_meeting(state.database.pool(), &meeting_id).await
    }

    /// The user's own topics for a recording (rename, remove, add).
    #[tauri::command(rename_all = "camelCase")]
    pub async fn set_meeting_topics(
        state: State<'_, AppState>,
        meeting_id: String,
        labels: Vec<String>,
    ) -> Result<Vec<MeetingTopic>, String> {
        set_for_meeting(state.database.pool(), &meeting_id, &labels).await
    }

    /// Find (or find again) a recording's topics with the user's AI. Topics
    /// the user edited are kept.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn find_topics(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<MeetingTopic>, String> {
        if recording_meeting(&state).as_deref() == Some(meeting_id.as_str()) {
            return Err("Stop the recording first: topics are found from the whole recording.".into());
        }
        if !begin(&meeting_id) {
            return Err("Topics for this recording are already being found.".into());
        }
        let _guard = RunGuard(meeting_id.clone());
        find_and_save(state.database.pool(), &LiveCompleter, &meeting_id).await
    }
}
