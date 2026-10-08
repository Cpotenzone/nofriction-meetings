//! Chat with your recordings (docs/TOPICS_AND_CHAT.md): a conversation
//! that answers from the user's own recordings, with citations that jump
//! to the moment.
//!
//! - Scope: all recordings, one Notebook, one Topic or one recording. The
//!   scope is resolved to a SQL filter (`retrieval::Filter`) and named in
//!   every answer.
//! - Retrieval, local only: FTS5 over the transcripts (bm25), plus the saved
//!   notes and marker notes matched by the question's terms, all read from
//!   the app's SQLite database. The best passages that fit the model's
//!   window are sent, each with the recording's title, date, type, Notebook
//!   and time. No embeddings, no index service.
//! - AI: one request through the same `Completer` the Review guide uses
//!   (`ai::complete_text`: consent, endpoint policy and, in the Mac App
//!   Store build, noFriction Pro). The client has no streaming; the UI
//!   shows a thinking state.
//! - Answer: Markdown with `[n]` citations; `citations` maps each number to
//!   the passage (recording, time, excerpt). The UI renders the text as
//!   text, never HTML.
//! - Memory: the thread's last `HISTORY_TURNS` messages go into the prompt.
//!   Threads and messages are stored locally (`chat_threads`,
//!   `chat_messages`, `chat_message_sources`), listable and deletable.
//! - Purge: when a recording is deleted or its transcript edited, every
//!   assistant message whose sources included it is deleted and the thread
//!   flagged (`purge_for_meeting`, in `redaction::redact_ai_outputs` and
//!   `DatabaseManager::delete_meeting`). See docs/REDACTION.md. Transcript
//!   text and model output are never logged.

pub mod prompt;
pub mod retrieval;
#[cfg(test)]
mod tests;

use crate::ai::{AiError, Msg, Opts};
use crate::study::{Completer, LiveCompleter};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnection;
use sqlx::{Acquire, Pool, Row, Sqlite};

pub use retrieval::{Passage, Scope, ScopeKind};

/// Messages of the thread sent back to the model with each question.
pub const HISTORY_TURNS: usize = 8;
/// Passages at most per answer.
pub const MAX_PASSAGES: usize = 12;
/// Shared with iOS.
const TITLE_CHARS: usize = 48;

pub const FLAG_REMOVED: &str =
    "Some answers were removed from this chat because a recording they drew on was deleted or edited.";

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_threads (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            scope_kind TEXT NOT NULL,
            scope_value TEXT,
            flag TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_messages (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
            role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
            content TEXT NOT NULL,
            citations TEXT NOT NULL DEFAULT '[]',
            scope_label TEXT,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chat_messages_thread ON chat_messages(thread_id, created_at)")
        .execute(&mut *conn)
        .await?;
    // Every recording whose passages were sent for an answer (cited or not)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_message_sources (
            message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
            meeting_id TEXT NOT NULL,
            PRIMARY KEY (message_id, meeting_id)
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chat_message_sources_meeting ON chat_message_sources(meeting_id)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn has_table(conn: &mut SqliteConnection, name: &str) -> Result<bool, sqlx::Error> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(exists.is_some())
}

/// Purge: delete every assistant answer that drew on this meeting (its
/// passages were in the prompt, cited or not) and flag their threads, and
/// flag threads scoped to the meeting. Called for every Delete/Strike of
/// transcript text, in the action's transaction, on the live database and
/// on each app backup (`redaction::redact_ai_outputs`), and when the meeting
/// is deleted. Schema-tolerant: an old backup may not have the tables.
/// Returns the number of messages deleted.
pub async fn purge_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<u64, sqlx::Error> {
    if !has_table(conn, "chat_message_sources").await? || !has_table(conn, "chat_messages").await? {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE chat_threads SET flag = ? WHERE id IN (SELECT thread_id FROM chat_messages WHERE id IN \
         (SELECT message_id FROM chat_message_sources WHERE meeting_id = ?)) \
         OR (scope_kind = 'meeting' AND scope_value = ?)",
    )
    .bind(FLAG_REMOVED)
    .bind(meeting_id)
    .bind(meeting_id)
    .execute(&mut *conn)
    .await?;
    let n = sqlx::query(
        "DELETE FROM chat_messages WHERE id IN (SELECT message_id FROM chat_message_sources WHERE meeting_id = ?)",
    )
    .bind(meeting_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM chat_message_sources WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await?;
    Ok(n)
}

/// Answers a purge would delete (for the Delete/Strike preview).
pub async fn count_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(DISTINCT message_id) FROM chat_message_sources WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════════════════════
// Records
// ═══════════════════════════════════════════════════════════════════════════

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatThread {
    pub id: String,
    pub title: String,
    pub scope: Scope,
    /// Set when answers were removed by a purge (`FLAG_REMOVED`)
    pub flag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One passage the answer may cite: `[n]` in the text → this.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Citation {
    pub n: usize,
    pub meeting_id: String,
    pub title: String,
    /// ms from the recording's start; None for notes (no time)
    pub timestamp_ms: Option<i64>,
    pub excerpt: String,
    /// "transcript" | "notes" | "marker"
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub thread_id: String,
    /// "user" | "assistant"
    pub role: String,
    /// Markdown (assistant) or the user's text; shown as text, never HTML
    pub content: String,
    pub citations: Vec<Citation>,
    /// The scope the answer was drawn from ("All recordings", "Notebook BIO 101"…)
    pub scope_label: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatThreadDetail {
    pub thread: ChatThread,
    pub messages: Vec<ChatMessage>,
}

/// What `ask` returns: the thread (new or existing) and the two messages.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatTurn {
    pub thread: ChatThread,
    pub user: ChatMessage,
    pub assistant: ChatMessage,
}

fn thread_from_row(r: &sqlx::sqlite::SqliteRow) -> ChatThread {
    ChatThread {
        id: r.get("id"),
        title: r.get("title"),
        scope: Scope::from_stored(&r.get::<String, _>("scope_kind"), r.get::<Option<String>, _>("scope_value")),
        flag: r.get("flag"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn message_from_row(r: &sqlx::sqlite::SqliteRow) -> ChatMessage {
    let citations: Vec<Citation> = serde_json::from_str(&r.get::<String, _>("citations")).unwrap_or_default();
    ChatMessage {
        id: r.get("id"),
        thread_id: r.get("thread_id"),
        role: r.get("role"),
        content: r.get("content"),
        citations,
        scope_label: r.get("scope_label"),
        created_at: r.get("created_at"),
    }
}

const THREAD_COLS: &str = "id, title, scope_kind, scope_value, flag, created_at, updated_at";
const MESSAGE_COLS: &str = "id, thread_id, role, content, citations, scope_label, created_at";

pub async fn list_threads(pool: &Pool<Sqlite>) -> Result<Vec<ChatThread>, String> {
    let rows = sqlx::query(&format!("SELECT {} FROM chat_threads ORDER BY updated_at DESC LIMIT 200", THREAD_COLS))
        .fetch_all(pool)
        .await
        .map_err(err("Couldn't read the chats"))?;
    Ok(rows.iter().map(thread_from_row).collect())
}

async fn get_thread_conn(conn: &mut SqliteConnection, id: &str) -> Result<ChatThread, String> {
    let row = sqlx::query(&format!("SELECT {} FROM chat_threads WHERE id = ?", THREAD_COLS))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read the chat"))?
        .ok_or("That chat no longer exists")?;
    Ok(thread_from_row(&row))
}

pub async fn get_thread(pool: &Pool<Sqlite>, id: &str) -> Result<ChatThreadDetail, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let thread = get_thread_conn(&mut conn, id).await?;
    let rows = sqlx::query(&format!(
        "SELECT {} FROM chat_messages WHERE thread_id = ? ORDER BY created_at ASC, rowid ASC",
        MESSAGE_COLS
    ))
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read the chat"))?;
    Ok(ChatThreadDetail { thread, messages: rows.iter().map(message_from_row).collect() })
}

pub async fn delete_thread(pool: &Pool<Sqlite>, id: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    // Explicit, so the delete never depends on foreign-key enforcement
    sqlx::query("DELETE FROM chat_message_sources WHERE message_id IN (SELECT id FROM chat_messages WHERE thread_id = ?)")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't delete the chat"))?;
    sqlx::query("DELETE FROM chat_messages WHERE thread_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't delete the chat"))?;
    sqlx::query("DELETE FROM chat_threads WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't delete the chat"))?;
    tx.commit().await.map_err(err("Couldn't delete the chat"))
}

/// The thread's last `HISTORY_TURNS` messages, oldest first.
async fn history(conn: &mut SqliteConnection, thread_id: &str) -> Result<Vec<(String, String)>, String> {
    let rows = sqlx::query(
        "SELECT role, content FROM chat_messages WHERE thread_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?",
    )
    .bind(thread_id)
    .bind(HISTORY_TURNS as i64)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read the chat"))?;
    Ok(rows.iter().rev().map(|r| (r.get("role"), r.get("content"))).collect())
}

/// A thread title from the first question.
pub fn title_of(question: &str) -> String {
    let one = question.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= TITLE_CHARS {
        one
    } else {
        let cut: String = one.chars().take(TITLE_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Asking
// ═══════════════════════════════════════════════════════════════════════════

/// Everything an answer is made of, before and after the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub content: String,
    pub passages: Vec<Passage>,
    pub scope_label: String,
}

impl Answer {
    pub fn citations(&self) -> Vec<Citation> {
        self.passages
            .iter()
            .map(|p| Citation {
                n: p.n,
                meeting_id: p.meeting_id.clone(),
                title: p.title.clone(),
                timestamp_ms: p.timestamp_ms,
                excerpt: p.excerpt.clone(),
                source: p.source.to_string(),
            })
            .collect()
    }
}

/// The citation numbers an answer uses, in order of first use.
pub fn cited_numbers(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '[' {
            let mut j = i + 1;
            let mut nums: Vec<usize> = Vec::new();
            let mut cur = String::new();
            let mut ok = true;
            while j < bytes.len() && bytes[j] != ']' {
                let c = bytes[j];
                if c.is_ascii_digit() {
                    cur.push(c);
                } else if c == ',' || c == ' ' {
                    if !cur.is_empty() {
                        nums.push(cur.parse().unwrap_or(0));
                        cur.clear();
                    }
                } else {
                    ok = false;
                    break;
                }
                j += 1;
            }
            if ok && j < bytes.len() {
                if !cur.is_empty() {
                    nums.push(cur.parse().unwrap_or(0));
                }
                for n in nums {
                    if n > 0 && !out.contains(&n) {
                        out.push(n);
                    }
                }
                i = j;
            }
        }
        i += 1;
    }
    out
}

/// Answer `question` within `scope` with `c`, using `history` (role, text)
/// as memory. Retrieval and the prompt are local; only the request goes to
/// the user's AI.
pub async fn answer(
    conn: &mut SqliteConnection,
    c: &dyn Completer,
    scope: &Scope,
    history: &[(String, String)],
    question: &str,
) -> Result<Answer, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Ask something first.".into());
    }
    let info = retrieval::resolve(conn, scope).await?;
    let ctx = c.context_tokens();
    let max_tokens = prompt::max_tokens(ctx);
    let hist = prompt::history_messages(history, ctx);
    let fixed = prompt::user_message(&info, &[], question) + &hist.iter().map(|m| m.text()).collect::<Vec<_>>().join("\n");
    let budget = prompt::passage_budget(ctx, max_tokens, &prompt::system_prompt(), &fixed);
    let passages = retrieval::retrieve(conn, &info, question, budget, MAX_PASSAGES).await?;
    let mut msgs = vec![Msg::system(prompt::system_prompt())];
    msgs.extend(hist);
    msgs.push(Msg::user(prompt::user_message(&info, &passages, question)));
    let opts = Opts { max_tokens, temperature: Some(0.2) };
    let content = match c.complete(msgs, opts).await {
        Ok(raw) => prompt::clean_answer(&raw),
        Err(AiError::Truncated) => return Err(AiError::Truncated.to_string()),
        Err(e) => return Err(e.to_string()),
    };
    if content.is_empty() {
        return Err("The model sent an empty answer. Try again.".into());
    }
    Ok(Answer { content, passages, scope_label: info.label })
}

/// Ask in a thread (a new one when `thread_id` is None) and store both
/// messages. The thread's scope is the one asked with (it can change
/// between questions; every answer names its own).
pub async fn ask(
    pool: &Pool<Sqlite>,
    c: &dyn Completer,
    thread_id: Option<&str>,
    scope: &Scope,
    question: &str,
) -> Result<ChatTurn, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let (existing, hist) = match thread_id {
        Some(id) => {
            let t = get_thread_conn(&mut conn, id).await?;
            let h = history(&mut conn, id).await?;
            (Some(t), h)
        }
        None => (None, Vec::new()),
    };
    let ans = answer(&mut conn, c, scope, &hist, question).await?;
    let now = Utc::now().to_rfc3339();
    let mut tx = conn.begin().await.map_err(err("Database busy"))?;
    let thread = match existing {
        Some(mut t) => {
            sqlx::query("UPDATE chat_threads SET scope_kind = ?, scope_value = ?, updated_at = ? WHERE id = ?")
                .bind(scope.kind.as_str())
                .bind(&scope.value)
                .bind(&now)
                .bind(&t.id)
                .execute(&mut *tx)
                .await
                .map_err(err("Couldn't save the chat"))?;
            t.scope = scope.clone();
            t.updated_at = now.clone();
            t
        }
        None => {
            let t = ChatThread {
                id: uuid::Uuid::new_v4().simple().to_string(),
                title: title_of(question),
                scope: scope.clone(),
                flag: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            sqlx::query(
                "INSERT INTO chat_threads (id, title, scope_kind, scope_value, flag, created_at, updated_at) VALUES (?, ?, ?, ?, NULL, ?, ?)",
            )
            .bind(&t.id)
            .bind(&t.title)
            .bind(scope.kind.as_str())
            .bind(&scope.value)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save the chat"))?;
            t
        }
    };
    let user = ChatMessage {
        id: uuid::Uuid::new_v4().simple().to_string(),
        thread_id: thread.id.clone(),
        role: "user".into(),
        content: question.trim().to_string(),
        citations: Vec::new(),
        scope_label: None,
        created_at: now.clone(),
    };
    // The answer sorts after the question even within one clock tick
    let later = (Utc::now() + chrono::Duration::milliseconds(1)).to_rfc3339();
    let assistant = ChatMessage {
        id: uuid::Uuid::new_v4().simple().to_string(),
        thread_id: thread.id.clone(),
        role: "assistant".into(),
        content: ans.content.clone(),
        citations: ans.citations(),
        scope_label: Some(ans.scope_label.clone()),
        created_at: later,
    };
    for m in [&user, &assistant] {
        sqlx::query(
            "INSERT INTO chat_messages (id, thread_id, role, content, citations, scope_label, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&m.id)
        .bind(&m.thread_id)
        .bind(&m.role)
        .bind(&m.content)
        .bind(serde_json::to_string(&m.citations).unwrap_or_else(|_| "[]".into()))
        .bind(&m.scope_label)
        .bind(&m.created_at)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save the chat"))?;
    }
    let mut seen: Vec<&str> = Vec::new();
    for p in &ans.passages {
        if seen.contains(&p.meeting_id.as_str()) {
            continue;
        }
        seen.push(&p.meeting_id);
        sqlx::query("INSERT OR IGNORE INTO chat_message_sources (message_id, meeting_id) VALUES (?, ?)")
            .bind(&assistant.id)
            .bind(&p.meeting_id)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save the chat"))?;
    }
    tx.commit().await.map_err(err("Couldn't save the chat"))?;
    Ok(ChatTurn { thread, user, assistant })
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri commands
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use tauri::State;

    /// Ask a question in a thread (None starts a new one). Consent and Pro
    /// are checked by `ai::complete_text`; the UI turns their errors into
    /// the dialogs.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn chat_ask(
        state: State<'_, AppState>,
        thread_id: Option<String>,
        scope: Scope,
        message: String,
    ) -> Result<ChatTurn, String> {
        log::info!("Chat: question of {} chars, scope {}", message.chars().count(), scope.kind.as_str());
        ask(state.database.pool(), &LiveCompleter, thread_id.as_deref(), &scope, &message).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_chat_threads(state: State<'_, AppState>) -> Result<Vec<ChatThread>, String> {
        list_threads(state.database.pool()).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn get_chat_thread(state: State<'_, AppState>, thread_id: String) -> Result<ChatThreadDetail, String> {
        get_thread(state.database.pool(), &thread_id).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_chat_thread(state: State<'_, AppState>, thread_id: String) -> Result<(), String> {
        delete_thread(state.database.pool(), &thread_id).await
    }

    /// What a scope covers, for the scope picker and the suggested
    /// questions (built locally, no AI call).
    #[tauri::command(rename_all = "camelCase")]
    pub async fn chat_scope_summary(state: State<'_, AppState>, scope: Scope) -> Result<retrieval::ScopeSummary, String> {
        let mut conn = state.database.pool().acquire().await.map_err(err("Database busy"))?;
        retrieval::summary(&mut conn, &scope).await
    }
}
