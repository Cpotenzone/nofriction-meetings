//! Study tools (docs/STUDY_TOOLS.md): a study guide made from a lecture's
//! transcript with the user's configured AI — lecture-notes summary, key
//! terms, flashcards, a practice quiz (answers point back to the transcript
//! time) and questions to ask the instructor. Moment markers (✎ On the test,
//! ★ Important, ? Question) steer it.
//!
//! - AI: every request goes through `ai::complete_text` (Apple on-device or
//!   the one user-entered endpoint). Consent and, in the Mac App Store
//!   build, the Pro check happen there; nothing here adds or skips a gate.
//! - Input: the stored transcript (Whisper-filtered at transcription time,
//!   so filtered text is never fed back), stricken spans as
//!   `[stricken from the record]`, deleted words gone. Long lectures are
//!   condensed chunk by chunk to fit small context windows.
//! - Output: strict JSON, validated and cleaned (`parse`); one retry; a
//!   clear error otherwise. Only the cleaned material is stored.
//! - Storage: `study_materials` (meeting_id, kind, json,
//!   transcript_fingerprint, created_at), one row per part. Any Delete or
//!   Strike of transcript text in the meeting deletes them (live database
//!   and app backups), see `purge_for_meeting` and docs/REDACTION.md.
//!   Transcript text and model output are never logged.

pub mod export;
pub mod parse;
pub mod prompt;
#[cfg(test)]
mod tests;

use crate::ai::{AiError, Msg, Opts};
use chrono::{DateTime, Utc};
pub use parse::StudyKind;
use prompt::{StudyInput, StudyLine, StudyMark};
use serde::Serialize;
use serde_json::Value;
use sqlx::sqlite::SqliteConnection;
use sqlx::{Pool, Row, Sqlite};
use std::collections::BTreeMap;

pub const TRANSCRIPT_CHANGED: &str =
    "The transcript changed while the study guide was being made (it was edited), so it wasn't saved. Generate it again.";
pub const PENDING_EDIT: &str =
    "An edit to this transcript is still in its 5-second undo window. Try again in a moment.";
pub const NO_TRANSCRIPT: &str = "This recording has no transcript to make a study guide from.";

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

/// `study_materials` first shipped, unused, for the old Dork Mode (summary,
/// key_concepts, quiz_questions, flashcards columns; created in
/// database.rs). This feature's columns are added here, schema-drift safe.
pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    crate::database::ensure_columns(
        conn,
        "study_materials",
        &[("kind", "TEXT"), ("json", "TEXT"), ("transcript_fingerprint", "TEXT"), ("created_at", "TEXT")],
    )
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_study_materials_kind ON study_materials(meeting_id, kind)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Purge: delete every study material of a meeting (this feature's rows and
/// any old Dork Mode rows). Called for every Delete/Strike of transcript
/// text, in the action's transaction, on the live database and on each app
/// backup (`redaction::redact_ai_outputs`). Schema-tolerant: an old backup
/// may not have the table. A guide paraphrases the lecture, so matching the
/// removed words can't clean it; it is deleted and can be made again.
pub async fn purge_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<u64, sqlx::Error> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'study_materials'")
            .fetch_optional(&mut *conn)
            .await?;
    if exists.is_none() {
        return Ok(0);
    }
    Ok(sqlx::query("DELETE FROM study_materials WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await?
        .rows_affected())
}

/// Rows a purge would delete (for the Delete/Strike preview).
pub async fn count_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM study_materials WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════════════════════
// Input
// ═══════════════════════════════════════════════════════════════════════════

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

struct MeetingInfo {
    title: String,
    class_name: Option<String>,
    started_at: DateTime<Utc>,
    duration_seconds: Option<i64>,
}

async fn meeting_info(conn: &mut SqliteConnection, meeting_id: &str) -> Result<MeetingInfo, String> {
    let row = sqlx::query("SELECT title, started_at, duration_seconds, class_name FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Database busy"))?
        .ok_or("That recording no longer exists")?;
    Ok(MeetingInfo {
        title: row.get("title"),
        class_name: row.get("class_name"),
        started_at: parse_ts(&row.get::<String, _>("started_at")).ok_or("That recording has no start time")?,
        duration_seconds: row.get("duration_seconds"),
    })
}

/// The prompt input for a meeting, read on one connection (so a save can
/// check it inside its own transaction).
pub async fn load_input_conn(conn: &mut SqliteConnection, meeting_id: &str) -> Result<StudyInput, String> {
    let m = meeting_info(conn, meeting_id).await?;
    let rows = sqlx::query("SELECT text, timestamp FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC, id ASC")
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(err("Couldn't read the transcript"))?;
    let lines: Vec<StudyLine> = rows
        .iter()
        .map(|r| StudyLine {
            ms: parse_ts(&r.get::<String, _>("timestamp"))
                .map(|t| (t - m.started_at).num_milliseconds().max(0))
                .unwrap_or(0),
            // Stricken spans → "[stricken from the record]"; marker ids never leave
            text: crate::redaction::render_plain(&r.get::<String, _>("text")),
        })
        .collect();
    let marks: Vec<StudyMark> = sqlx::query("SELECT ts, kind, note FROM meeting_markers WHERE meeting_id = ? ORDER BY ts ASC")
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(err("Couldn't read the markers"))?
        .iter()
        .map(|r| StudyMark {
            ms: parse_ts(&r.get::<String, _>("ts")).map(|t| (t - m.started_at).num_milliseconds().max(0)).unwrap_or(0),
            kind: r.get("kind"),
            note: r.get("note"),
        })
        .collect();
    let last = lines.iter().map(|l| l.ms).chain(marks.iter().map(|k| k.ms)).max().unwrap_or(0);
    let duration_ms = m.duration_seconds.map(|s| s * 1000).filter(|d| *d > 0).unwrap_or(0).max(last);
    Ok(StudyInput { title: m.title, class_name: m.class_name, duration_ms, lines, marks })
}

pub async fn load_input(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<StudyInput, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    load_input_conn(&mut conn, meeting_id).await
}

async fn has_pending_delete(conn: &mut SqliteConnection, meeting_id: &str) -> Result<bool, String> {
    let n: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM redactions WHERE meeting_id = ? AND action = 'delete' AND pending_payload IS NOT NULL LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err("Database busy"))?;
    Ok(n.is_some())
}

// ═══════════════════════════════════════════════════════════════════════════
// Stored material
// ═══════════════════════════════════════════════════════════════════════════

/// Save generated parts, replacing earlier ones of the same kinds. Inside
/// one `BEGIN IMMEDIATE` transaction it first checks the transcript still
/// reads exactly as it did when generation started (`fingerprint`) and has
/// no Delete in its undo window: material made from text that has since
/// been deleted or stricken must never be written after the purge ran.
pub async fn save_materials(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    fingerprint: &str,
    items: &[(StudyKind, Value)],
) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    // IMMEDIATE takes the write lock before the check, so no edit can commit
    // between the check and the insert (a deferred transaction could only
    // fail late, or spuriously on unrelated writes)
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await.map_err(err("Database busy"))?;
    let result = save_locked(&mut conn, meeting_id, fingerprint, items).await;
    let end = if result.is_ok() { "COMMIT" } else { "ROLLBACK" };
    match sqlx::query(end).execute(&mut *conn).await {
        Ok(_) => result,
        Err(e) => {
            // Never hand a connection with an open transaction back to the pool
            let _ = conn.close().await;
            result.and(Err(format!("Couldn't save the study guide: {}", e)))
        }
    }
}

async fn save_locked(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    fingerprint: &str,
    items: &[(StudyKind, Value)],
) -> Result<(), String> {
    if has_pending_delete(conn, meeting_id).await? {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let now_input = load_input_conn(conn, meeting_id).await?;
    if now_input.fingerprint() != fingerprint {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let now = Utc::now().to_rfc3339();
    for (kind, data) in items {
        sqlx::query("DELETE FROM study_materials WHERE meeting_id = ? AND kind = ?")
            .bind(meeting_id)
            .bind(kind.as_str())
            .execute(&mut *conn)
            .await
            .map_err(err("Couldn't save the study guide"))?;
        sqlx::query(
            "INSERT INTO study_materials (id, meeting_id, kind, json, transcript_fingerprint, created_at, generated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().simple().to_string())
        .bind(meeting_id)
        .bind(kind.as_str())
        .bind(data.to_string())
        .bind(fingerprint)
        .bind(&now)
        .bind(&now)
        .execute(&mut *conn)
        .await
        .map_err(err("Couldn't save the study guide"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredMaterial {
    pub kind: String,
    pub data: Value,
    pub created_at: String,
    /// Made from a transcript that reads differently now
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudyGuide {
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    pub duration_ms: i64,
    pub has_transcript: bool,
    /// By kind ("summary", "terms", "flashcards", "quiz", "questions")
    pub materials: BTreeMap<String, StoredMaterial>,
    pub markers: Vec<crate::markers::Marker>,
}

/// A meeting's study guide: stored parts (re-validated on the way out) and
/// its markers.
pub async fn load_guide(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<StudyGuide, String> {
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let info = meeting_info(&mut conn, meeting_id).await?;
    let input = load_input_conn(&mut conn, meeting_id).await?;
    let current = input.fingerprint();
    let rows = sqlx::query(
        "SELECT kind, json, transcript_fingerprint, created_at FROM study_materials \
         WHERE meeting_id = ? AND kind IS NOT NULL AND json IS NOT NULL ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read the study guide"))?;
    drop(conn);
    let mut materials = BTreeMap::new();
    for r in rows {
        let Some(kind) = StudyKind::parse(&r.get::<String, _>("kind")) else { continue };
        let Some(data) = parse::revalidate(kind, &r.get::<String, _>("json"), input.duration_ms) else { continue };
        let fp: Option<String> = r.get("transcript_fingerprint");
        materials.insert(
            kind.as_str().to_string(),
            StoredMaterial {
                kind: kind.as_str().to_string(),
                data,
                created_at: r.get::<Option<String>, _>("created_at").unwrap_or_default(),
                stale: fp.as_deref() != Some(current.as_str()),
            },
        );
    }
    Ok(StudyGuide {
        meeting_id: meeting_id.to_string(),
        title: info.title,
        started_at: info.started_at.to_rfc3339(),
        duration_ms: input.duration_ms,
        has_transcript: input.has_transcript(),
        materials,
        markers: crate::markers::list(pool, meeting_id).await?,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Generation
// ═══════════════════════════════════════════════════════════════════════════

/// The AI call, abstracted so tests use a mock (never a real endpoint).
#[async_trait::async_trait]
pub trait Completer: Send + Sync {
    async fn complete(&self, msgs: Vec<Msg>, opts: Opts) -> Result<String, AiError>;
    /// The model's context window in tokens (prompt + answer)
    fn context_tokens(&self) -> usize;
}

/// The user's configured text AI. `ai::complete_text` checks consent, the
/// endpoint policy and (Mac App Store build) noFriction Pro.
pub struct LiveCompleter;

#[async_trait::async_trait]
impl Completer for LiveCompleter {
    async fn complete(&self, msgs: Vec<Msg>, opts: Opts) -> Result<String, AiError> {
        crate::ai::complete_text(msgs, opts).await
    }
    fn context_tokens(&self) -> usize {
        let cfg = crate::ai::config::snapshot();
        match cfg.selection(crate::ai::Kind::Text) {
            Some(sel) => crate::ai::providers::context_window(
                &sel.provider,
                &sel.model,
                cfg.model_info(&sel.provider, &sel.model).and_then(|i| i.context),
            ),
            // Unconfigured: the call will say so; plan for the smallest window
            None => crate::ai::providers::APPLE_CONTEXT_TOKENS,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

#[derive(Debug)]
pub enum GenError {
    /// AI access: consent, Pro, no provider, wrong key, unreachable… The
    /// whole run stops (the UI turns consent/Pro into their dialogs).
    Ai(AiError),
    Failed(String),
}

impl From<GenError> for String {
    fn from(e: GenError) -> String {
        match e {
            GenError::Ai(a) => a.to_string(),
            GenError::Failed(s) => s,
        }
    }
}

/// Errors that are about the answer, not about reaching the AI: retried,
/// then reported for that part only.
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

/// Ask for one part; validate; on a bad answer ask once more.
async fn ask_json(
    c: &dyn Completer,
    kind: StudyKind,
    user: &str,
    max_tokens: u32,
    duration_ms: i64,
) -> Result<Result<Value, String>, GenError> {
    let system = prompt::system_for(kind);
    let mut why = String::new();
    for attempt in 0..2 {
        let text = if attempt == 0 { user.to_string() } else { format!("{}\n\n{}", user, prompt::retry_note(kind, &why)) };
        let opts = Opts { max_tokens, temperature: Some(if attempt == 0 { 0.2 } else { 0.3 }) };
        match c.complete(vec![Msg::system(system.clone()), Msg::user(text)], opts).await {
            Ok(raw) => match parse::validate(kind, &raw, duration_ms) {
                Ok(v) => return Ok(Ok(v)),
                Err(e) => why = e,
            },
            Err(e) if answer_error(&e) => why = short(&e),
            Err(e) => return Err(GenError::Ai(e)),
        }
    }
    log::warn!("Study guide: the {} answer was unusable twice", kind.as_str());
    Ok(Err(format!(
        "Couldn't make the {}: {} (asked twice). Try again, or choose a larger model in Settings → AI Engine.",
        kind.label(),
        why
    )))
}

/// Condense lines that don't fit into notes that do (map step), up to
/// three rounds. Returns the material to send and whether it is condensed.
async fn fit_material(
    c: &dyn Completer,
    input: &StudyInput,
    kinds: &[StudyKind],
    total: &mut usize,
    done: &mut usize,
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(String, bool), GenError> {
    let ctx = c.context_tokens();
    let fixed = prompt::user_message(input, true, "");
    let final_budget = kinds
        .iter()
        .map(|k| prompt::body_budget(ctx, prompt::max_tokens(*k, ctx), &prompt::system_for(*k), &fixed))
        .min()
        .unwrap_or(600);
    let mut lines = input.transcript_lines();
    let mut condensed = false;
    for round in 0..3 {
        if lines.iter().map(|l| l.chars().count() + 1).sum::<usize>() <= final_budget {
            break;
        }
        let cmax = prompt::condense_max_tokens(ctx);
        let fixed_c = prompt::condense_message(input, "", 99, 99) + &input.marks_block(i64::MIN, i64::MAX);
        let chunk_budget = prompt::body_budget(ctx, cmax, prompt::CONDENSE_SYSTEM, &fixed_c);
        let chunks = prompt::chunk_lines(&lines, chunk_budget);
        if round > 0 && chunks.len() <= 1 && condensed {
            break; // condensing again wouldn't shrink it; the client trims the rest
        }
        *total += chunks.len();
        let mut notes: Vec<String> = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            progress(Progress {
                done: *done,
                total: *total,
                label: format!("Reading the lecture ({} of {})…", i + 1, chunks.len()),
            });
            let msg = prompt::condense_message(input, chunk, i + 1, chunks.len());
            let mut got = None;
            for attempt in 0..2 {
                let opts = Opts { max_tokens: cmax, temperature: Some(if attempt == 0 { 0.2 } else { 0.3 }) };
                match c.complete(vec![Msg::system(prompt::CONDENSE_SYSTEM), Msg::user(msg.clone())], opts).await {
                    Ok(raw) => {
                        if let Ok(ls) = parse::condensed_lines(&raw, 20) {
                            got = Some(ls);
                            break;
                        }
                    }
                    Err(e) if answer_error(&e) => {}
                    Err(e) => return Err(GenError::Ai(e)),
                }
            }
            let Some(ls) = got else {
                return Err(GenError::Failed(format!(
                    "Couldn't condense part {} of {} of the lecture (asked twice). Try again, or choose a model with a larger context window.",
                    i + 1,
                    chunks.len()
                )));
            };
            notes.extend(ls);
            *done += 1;
        }
        lines = notes;
        condensed = true;
    }
    Ok((lines.join("\n"), condensed))
}

/// Make the requested parts. AI access errors stop the run; a part whose
/// answer is unusable twice is reported on its own and the rest go on.
pub async fn generate(
    c: &dyn Completer,
    input: &StudyInput,
    kinds: &[StudyKind],
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Vec<(StudyKind, Result<Value, String>)>, GenError> {
    if !input.has_transcript() {
        return Err(GenError::Failed(NO_TRANSCRIPT.into()));
    }
    let mut total = kinds.len();
    let mut done = 0usize;
    let (body, condensed) = fit_material(c, input, kinds, &mut total, &mut done, progress).await?;
    let user = prompt::user_message(input, condensed, &body);
    let ctx = c.context_tokens();
    let mut out = Vec::new();
    for k in kinds {
        progress(Progress { done, total, label: format!("Writing the {}…", k.label()) });
        let r = ask_json(c, *k, &user, prompt::max_tokens(*k, ctx), input.duration_ms).await?;
        out.push((*k, r));
        done += 1;
    }
    progress(Progress { done: total, total, label: "Done".into() });
    Ok(out)
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
    use tauri::{AppHandle, Emitter, State};

    pub const PROGRESS_EVENT: &str = "study_progress";

    /// Meetings with a generation running (one at a time per meeting).
    static RUNNING: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

    struct RunGuard(String);
    impl Drop for RunGuard {
        fn drop(&mut self) {
            RUNNING.lock().remove(&self.0);
        }
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct PartError {
        pub kind: String,
        pub error: String,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct GenerateResult {
        pub saved: Vec<String>,
        pub failed: Vec<PartError>,
        pub guide: StudyGuide,
    }

    fn recording_meeting(state: &AppState) -> Option<String> {
        if !state.capture_engine.read().is_recording() {
            return None;
        }
        state.state_builder.read().current_meeting_id()
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn get_study_guide(state: State<'_, AppState>, meeting_id: String) -> Result<StudyGuide, String> {
        load_guide(state.database.pool(), &meeting_id).await
    }

    /// Make (or remake) the study guide, or some parts of it (`kinds`).
    /// Progress arrives as `study_progress` events.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn generate_study_guide(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
        kinds: Option<Vec<String>>,
    ) -> Result<GenerateResult, String> {
        let kinds: Vec<StudyKind> = match kinds {
            Some(list) if !list.is_empty() => {
                list.iter().map(|k| StudyKind::parse(k).ok_or("Unknown study guide part")).collect::<Result<_, _>>()?
            }
            _ => StudyKind::ALL.to_vec(),
        };
        if recording_meeting(&state).as_deref() == Some(meeting_id.as_str()) {
            return Err("Stop the recording first: the study guide is made from the whole lecture.".into());
        }
        if !RUNNING.lock().insert(meeting_id.clone()) {
            return Err("A study guide for this recording is already being made.".into());
        }
        let _guard = RunGuard(meeting_id.clone());
        let pool = state.database.pool().clone();
        {
            let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
            if has_pending_delete(&mut conn, &meeting_id).await? {
                return Err(PENDING_EDIT.into());
            }
        }
        let input = load_input(&pool, &meeting_id).await?;
        let fingerprint = input.fingerprint();
        log::info!(
            "Study guide: {} part(s) for a {}-line transcript",
            kinds.len(),
            input.lines.len()
        );
        let mid = meeting_id.clone();
        let app_ev = app.clone();
        let report = move |p: Progress| {
            let _ = app_ev.emit(
                PROGRESS_EVENT,
                serde_json::json!({ "meeting_id": mid, "done": p.done, "total": p.total, "label": p.label }),
            );
        };
        let outcomes = generate(&LiveCompleter, &input, &kinds, &report).await.map_err(String::from)?;
        let ok: Vec<(StudyKind, Value)> =
            outcomes.iter().filter_map(|(k, r)| r.as_ref().ok().map(|v| (*k, v.clone()))).collect();
        save_materials(&pool, &meeting_id, &fingerprint, &ok).await?;
        let failed: Vec<PartError> = outcomes
            .iter()
            .filter_map(|(k, r)| r.as_ref().err().map(|e| PartError { kind: k.as_str().into(), error: e.clone() }))
            .collect();
        Ok(GenerateResult {
            saved: ok.iter().map(|(k, _)| k.as_str().to_string()).collect(),
            failed,
            guide: load_guide(&pool, &meeting_id).await?,
        })
    }

    /// Show the save dialog and write `contents` where the user chose. In the
    /// Mac App Store sandbox the save panel grants write access to that file
    /// (`files.user-selected.read-write`). Ok(None) when cancelled.
    async fn save_with_dialog(
        app: &AppHandle,
        default_name: &str,
        filter: (&str, &[&str]),
        contents: String,
    ) -> Result<Option<String>, String> {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_file_name(default_name)
            .add_filter(filter.0, filter.1)
            .save_file(move |p| {
                let _ = tx.send(p);
            });
        let Some(chosen) = rx.await.map_err(|_| "The save dialog closed unexpectedly".to_string())? else {
            return Ok(None);
        };
        let path = chosen.into_path().map_err(err("That location can't be written"))?;
        std::fs::write(&path, contents).map_err(err("Couldn't save the file"))?;
        Ok(Some(path.display().to_string()))
    }

    /// Flashcards as CSV (front,back) for Anki or Quizlet.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn export_study_flashcards(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<Option<String>, String> {
        let guide = load_guide(state.database.pool(), &meeting_id).await?;
        let cards = guide.materials.get("flashcards").map(|m| export::cards_of(&m.data)).unwrap_or_default();
        if cards.is_empty() {
            return Err("Make the flashcards first.".into());
        }
        let name = format!("{} flashcards.csv", export::file_stem(&guide.title));
        save_with_dialog(&app, &name, ("CSV", &["csv"]), export::flashcards_csv(&cards)).await
    }

    /// The study guide as Markdown.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn export_study_guide(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<Option<String>, String> {
        let guide = load_guide(state.database.pool(), &meeting_id).await?;
        let md = guide_markdown_of(&guide);
        let name = format!("{} study guide.md", export::file_stem(&guide.title));
        save_with_dialog(&app, &name, ("Markdown", &["md"]), md).await
    }

    pub fn guide_markdown_of(guide: &StudyGuide) -> String {
        let marks: Vec<export::GuideMark> = guide
            .markers
            .iter()
            .map(|m| export::GuideMark { ms: m.offset_ms, kind: m.kind.clone(), note: m.note.clone() })
            .collect();
        let when = parse_ts(&guide.started_at)
            .map(|t| t.with_timezone(&chrono::Local).format("%A %-d %B %Y, %H:%M").to_string())
            .unwrap_or_default();
        let get = |k: &str| guide.materials.get(k).map(|m| &m.data);
        export::guide_markdown(&export::GuideParts {
            title: &guide.title,
            when: &when,
            summary: get("summary"),
            terms: get("terms"),
            flashcards: get("flashcards"),
            quiz: get("quiz"),
            questions: get("questions"),
            marks: &marks,
        })
    }
}
