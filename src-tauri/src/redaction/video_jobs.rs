//! Durable background jobs that blank removed moments out of the DMG screen
//! video (docs/REDACTION.md, purge step 4).
//!
//! A screen or time-range Delete/Strike removes the database content first
//! and, in the same transaction, queues one job per covered wall-clock range
//! here. A single worker blanks them afterwards, **outside** the redaction
//! `LOCK`, so no other edit ever waits behind ffmpeg. Jobs survive quits and
//! crashes (`running` rows are resumed at launch), failures are retried with
//! backoff (never in a tight loop), and a job row is removed once its range
//! is black in every covering chunk. Rows hold times only, never content.
//!
//! The ffmpeg work sits behind [`VideoOps`] so tests can fake slow or
//! failing runs; the real implementation is `video_blank::FfmpegOps` (DMG).

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// After this many failed automatic attempts a job waits for the next launch
/// or the Retry button instead of being retried on a timer.
pub const MAX_AUTO_ATTEMPTS: i64 = 8;
/// First retry delay; doubles per attempt up to [`MAX_BACKOFF_SECS`].
pub const BASE_BACKOFF_SECS: i64 = 30;
pub const MAX_BACKOFF_SECS: i64 = 3600;
/// `next_attempt_at` for a job that only retries at launch / on Retry.
const PARKED: &str = "9999-12-31T00:00:00+00:00";

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS video_blank_jobs (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            -- Wall-clock range to blank (RFC3339, unpadded). Times only.
            start_at TEXT NOT NULL,
            end_at TEXT NOT NULL,
            -- The Strike record whose marker shows "video pending" until
            -- this row is gone. NULL for a Delete (which leaves no trace
            -- once the job is done).
            redaction_id TEXT,
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'running', 'failed')),
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            tool_missing INTEGER NOT NULL DEFAULT 0,
            next_attempt_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_video_blank_jobs_meeting ON video_blank_jobs(meeting_id)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Sort and merge overlapping/touching ranges (within 1 s).
pub fn merge_ranges(ranges: &[(DateTime<Utc>, DateTime<Utc>)]) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut v: Vec<(DateTime<Utc>, DateTime<Utc>)> = ranges.iter().map(|(a, b)| (*a.min(b), *a.max(b))).collect();
    v.sort();
    let mut out: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(last) if a <= last.1 + chrono::Duration::seconds(1) => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Queue blanking of `ranges` for a meeting. Call inside the purge
/// transaction so the job exists if and only if the content was removed.
pub async fn enqueue(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    ranges: &[(DateTime<Utc>, DateTime<Utc>)],
    redaction_id: Option<&str>,
) -> Result<usize, sqlx::Error> {
    let merged = merge_ranges(ranges);
    let now = Utc::now().to_rfc3339();
    for (a, b) in &merged {
        sqlx::query(
            "INSERT INTO video_blank_jobs (id, meeting_id, start_at, end_at, redaction_id, status, attempts, created_at) \
             VALUES (?, ?, ?, ?, ?, 'pending', 0, ?)",
        )
        .bind(new_id())
        .bind(meeting_id)
        .bind(a.to_rfc3339())
        .bind(b.to_rfc3339())
        .bind(redaction_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(merged.len())
}

/// A queued/failed job, for the meeting-view banner. Times only.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VideoJobInfo {
    pub id: String,
    pub meeting_id: String,
    pub start_at: String,
    pub end_at: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    /// ffmpeg/ffprobe isn't installed: shown as a persistent warning
    pub tool_missing: bool,
    pub next_attempt_at: Option<String>,
    /// Belongs to a Strike (its marker says the video is still pending)
    pub strike: bool,
}

pub async fn list(pool: &Pool<Sqlite>, meeting_id: Option<&str>) -> Result<Vec<VideoJobInfo>, String> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, start_at, end_at, status, attempts, last_error, tool_missing, next_attempt_at, \
         redaction_id FROM video_blank_jobs WHERE (?1 IS NULL OR meeting_id = ?1) ORDER BY created_at ASC, start_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(err("Couldn't read screen video jobs"))?;
    Ok(rows
        .iter()
        .map(|r| VideoJobInfo {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            start_at: r.get("start_at"),
            end_at: r.get("end_at"),
            status: r.get("status"),
            attempts: r.get("attempts"),
            last_error: r.get("last_error"),
            tool_missing: r.get::<i64, _>("tool_missing") != 0,
            next_attempt_at: r.get::<Option<String>, _>("next_attempt_at").filter(|s| s != PARKED),
            strike: r.get::<Option<String>, _>("redaction_id").is_some(),
        })
        .collect())
}

/// Retry button: make this meeting's failed jobs due now.
pub async fn retry_now(pool: &Pool<Sqlite>, meeting_id: Option<&str>) -> Result<u64, String> {
    sqlx::query(
        "UPDATE video_blank_jobs SET next_attempt_at = ?2, updated_at = ?2 \
         WHERE status = 'failed' AND (?1 IS NULL OR meeting_id = ?1)",
    )
    .bind(meeting_id)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .map_err(err("Couldn't retry screen video jobs"))
}

/// At launch: a job that was `running` when the app quit or crashed is
/// resumed, and every failed job gets one fresh attempt (ffmpeg may have
/// been installed since).
pub async fn prepare_for_launch(pool: &Pool<Sqlite>) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE video_blank_jobs SET status = 'pending', updated_at = ? WHERE status = 'running'")
        .bind(&now)
        .execute(pool)
        .await
        .map_err(err("Couldn't resume screen video jobs"))?;
    sqlx::query("UPDATE video_blank_jobs SET next_attempt_at = ?1, updated_at = ?1 WHERE status = 'failed'")
        .bind(&now)
        .execute(pool)
        .await
        .map_err(err("Couldn't resume screen video jobs"))?;
    Ok(())
}

/// Delay before automatic attempt `attempts + 1`: 30 s, 1 min, 2 min, …,
/// capped at 1 h.
pub fn backoff(attempts: i64) -> chrono::Duration {
    let n = attempts.clamp(1, 20) - 1;
    chrono::Duration::seconds((BASE_BACKOFF_SECS << n).min(MAX_BACKOFF_SECS))
}

/// Result of blanking one meeting's due ranges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlankReport {
    /// Chunks re-encoded (partially or fully)
    pub chunks_rewritten: usize,
    /// Ranges skipped because they were already black (recorded, or found
    /// black by decoding: e.g. blanked by an older build)
    pub ranges_already_blank: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlankError {
    /// ffmpeg/ffprobe isn't installed: kept, shown as a persistent warning
    ToolMissing(String),
    Failed(String),
    /// The app is quitting; the job is resumed at the next launch
    Cancelled,
}

/// The ffmpeg side of a job. `progress` takes 0.0–1.0.
pub trait VideoOps: Send + Sync + 'static {
    fn blank(
        &self,
        env: &RedactionEnv,
        meeting_id: &str,
        ranges: &[(DateTime<Utc>, DateTime<Utc>)],
        progress: &dyn Fn(f32),
        cancel: &AtomicBool,
    ) -> Result<BlankReport, BlankError>;
}

/// Sent to the UI (`video_blank_progress`). Times only, never content.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JobEvent {
    pub meeting_id: String,
    /// "running" | "done" | "failed"
    pub status: String,
    /// 0–100 while running
    pub percent: Option<u32>,
    pub error: Option<String>,
    pub tool_missing: bool,
    /// Jobs (ranges) still queued or failed for this meeting afterwards
    pub remaining: usize,
}

pub type Reporter = Arc<dyn Fn(JobEvent) + Send + Sync>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSummary {
    pub meetings_done: usize,
    pub meetings_failed: usize,
    pub jobs_done: usize,
}

struct DueJob {
    id: String,
    meeting_id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    attempts: i64,
}

async fn due_jobs(pool: &Pool<Sqlite>, now: DateTime<Utc>, skip: &[String]) -> Result<Vec<DueJob>, String> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, start_at, end_at, attempts, next_attempt_at FROM video_blank_jobs \
         WHERE status IN ('pending', 'failed') ORDER BY created_at ASC, start_at ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(err("Couldn't read screen video jobs"))?;
    let mut out = Vec::new();
    for r in rows {
        let meeting_id: String = r.get("meeting_id");
        if skip.contains(&meeting_id) {
            continue;
        }
        let due = match r.get::<Option<String>, _>("next_attempt_at") {
            None => true,
            Some(s) => parse_ts(&s).map_or(false, |t| t <= now),
        };
        let (Some(start), Some(end)) = (
            parse_ts(&r.get::<String, _>("start_at")),
            parse_ts(&r.get::<String, _>("end_at")),
        ) else {
            continue;
        };
        if due {
            out.push(DueJob { id: r.get("id"), meeting_id, start, end, attempts: r.get("attempts") });
        }
    }
    Ok(out)
}

/// When the worker should look again: the earliest scheduled attempt of a
/// job that is still retried automatically (meetings in `skip` excluded).
pub async fn next_wakeup(pool: &Pool<Sqlite>, skip: &[String]) -> Option<DateTime<Utc>> {
    let rows = sqlx::query(
        "SELECT meeting_id, next_attempt_at FROM video_blank_jobs WHERE status IN ('pending', 'failed')",
    )
    .fetch_all(pool)
    .await
    .ok()?;
    rows.iter()
        .filter(|r| !skip.contains(&r.get::<String, _>("meeting_id")))
        .filter_map(|r| match r.get::<Option<String>, _>("next_attempt_at") {
            None => Some(Utc::now()),
            Some(s) if s == PARKED => None,
            Some(s) => parse_ts(&s),
        })
        .min()
}

async fn has_jobs_for(pool: &Pool<Sqlite>, meetings: &[String]) -> bool {
    for m in meetings {
        if remaining(pool, m).await > 0 {
            return true;
        }
    }
    false
}

async fn remaining(pool: &Pool<Sqlite>, meeting_id: &str) -> usize {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM video_blank_jobs WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0) as usize
}

/// Process every job due at `now`, one meeting at a time (all of a
/// meeting's due ranges in one pass per chunk). Meetings in `recording`
/// wait. Never takes the redaction `LOCK`.
pub async fn run_due(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    ops: Arc<dyn VideoOps>,
    now: DateTime<Utc>,
    cancel: Arc<AtomicBool>,
    report: Reporter,
) -> Result<RunSummary, String> {
    let mut summary = RunSummary::default();
    let jobs = due_jobs(pool, now, &env.recording_meetings).await?;
    let mut meetings: Vec<String> = Vec::new();
    for j in &jobs {
        if !meetings.contains(&j.meeting_id) {
            meetings.push(j.meeting_id.clone());
        }
    }
    for meeting_id in meetings {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let group: Vec<&DueJob> = jobs.iter().filter(|j| j.meeting_id == meeting_id).collect();
        let ids: Vec<String> = group.iter().map(|j| j.id.clone()).collect();
        let ranges: Vec<(DateTime<Utc>, DateTime<Utc>)> = group.iter().map(|j| (j.start, j.end)).collect();
        let attempts = group.iter().map(|j| j.attempts).max().unwrap_or(0) + 1;
        set_status(pool, &ids, "running", attempts, None, false, None).await?;
        report(JobEvent {
            meeting_id: meeting_id.clone(),
            status: "running".into(),
            percent: Some(0),
            error: None,
            tool_missing: false,
            remaining: remaining(pool, &meeting_id).await,
        });

        let ops2 = ops.clone();
        let env2 = env.clone();
        let mid = meeting_id.clone();
        let cancel2 = cancel.clone();
        let report2 = report.clone();
        let n_remaining = remaining(pool, &meeting_id).await;
        let result = tokio::task::spawn_blocking(move || {
            let last = std::sync::atomic::AtomicU32::new(0);
            let progress = |f: f32| {
                let pct = (f.clamp(0.0, 1.0) * 100.0).floor() as u32;
                if pct > last.load(Ordering::Relaxed) {
                    last.store(pct, Ordering::Relaxed);
                    report2(JobEvent {
                        meeting_id: mid.clone(),
                        status: "running".into(),
                        percent: Some(pct.min(99)),
                        error: None,
                        tool_missing: false,
                        remaining: n_remaining,
                    });
                }
            };
            ops2.blank(&env2, &mid, &ranges, &progress, &cancel2)
        })
        .await
        .unwrap_or_else(|e| Err(BlankError::Failed(format!("Video blanking crashed: {}", e))));

        match result {
            Ok(_) => {
                for chunk in ids.chunks(200) {
                    let sql = format!("DELETE FROM video_blank_jobs WHERE id IN ({})", placeholders(chunk.len()));
                    let mut q = sqlx::query(&sql);
                    for id in chunk {
                        q = q.bind(id);
                    }
                    q.execute(pool).await.map_err(err("Couldn't finish screen video job"))?;
                }
                summary.meetings_done += 1;
                summary.jobs_done += ids.len();
                report(JobEvent {
                    meeting_id: meeting_id.clone(),
                    status: "done".into(),
                    percent: Some(100),
                    error: None,
                    tool_missing: false,
                    remaining: remaining(pool, &meeting_id).await,
                });
            }
            Err(BlankError::Cancelled) => {
                // Not the job's fault: resumed at the next launch
                set_status(pool, &ids, "pending", attempts - 1, None, false, None).await?;
            }
            Err(e) => {
                let (msg, tool_missing) = match e {
                    BlankError::ToolMissing(m) => (m, true),
                    BlankError::Failed(m) => (m, false),
                    BlankError::Cancelled => unreachable!(),
                };
                let next = if attempts >= MAX_AUTO_ATTEMPTS {
                    PARKED.to_string()
                } else {
                    (Utc::now() + backoff(attempts)).to_rfc3339()
                };
                log::warn!("Screen video blanking failed (attempt {}): {}", attempts, msg);
                set_status(pool, &ids, "failed", attempts, Some(&msg), tool_missing, Some(&next)).await?;
                summary.meetings_failed += 1;
                report(JobEvent {
                    meeting_id: meeting_id.clone(),
                    status: "failed".into(),
                    percent: None,
                    error: Some(msg),
                    tool_missing,
                    remaining: remaining(pool, &meeting_id).await,
                });
            }
        }
    }
    Ok(summary)
}

async fn set_status(
    pool: &Pool<Sqlite>,
    ids: &[String],
    status: &str,
    attempts: i64,
    last_error: Option<&str>,
    tool_missing: bool,
    next_attempt_at: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    for id in ids {
        sqlx::query(
            "UPDATE video_blank_jobs SET status = ?, attempts = ?, last_error = ?, tool_missing = ?, \
             next_attempt_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(attempts.max(0))
        .bind(last_error)
        .bind(tool_missing as i64)
        .bind(next_attempt_at)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(err("Couldn't update screen video job"))?;
    }
    Ok(())
}

// ── The app's single worker ────────────────────────────────────────────

struct Worker {
    running: AtomicBool,
    notify: tokio::sync::Notify,
    cancel: Arc<AtomicBool>,
}

static WORKER: Lazy<Worker> = Lazy::new(|| Worker {
    running: AtomicBool::new(false),
    notify: tokio::sync::Notify::new(),
    cancel: Arc::new(AtomicBool::new(false)),
});

/// Start the worker if it isn't running, or wake it. `env` is re-read on
/// every pass (meetings being recorded wait). Safe to call often.
pub fn kick(
    pool: Pool<Sqlite>,
    env: Arc<dyn Fn() -> RedactionEnv + Send + Sync>,
    ops: Arc<dyn VideoOps>,
    report: Reporter,
) {
    WORKER.notify.notify_one();
    if WORKER.running.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if WORKER.cancel.load(Ordering::SeqCst) {
                break;
            }
            let e = env();
            let mut min_wait = 0;
            if let Err(err) = run_due(&pool, &e, ops.clone(), Utc::now(), WORKER.cancel.clone(), report.clone()).await {
                log::warn!("Screen video worker: {}", err);
                min_wait = 5; // never spin on a database error
            }
            // Jobs of a meeting being recorded wait: look again in a minute
            let waiting_on_recording = has_jobs_for(&pool, &e.recording_meetings).await;
            let next = next_wakeup(&pool, &e.recording_meetings).await;
            let wait = match (next, waiting_on_recording) {
                (None, false) => {
                    // Nothing left to retry automatically. Re-check after
                    // clearing the flag so a kick in between isn't lost.
                    WORKER.running.store(false, Ordering::SeqCst);
                    let again = due_jobs(&pool, Utc::now(), &[]).await.map(|j| !j.is_empty()).unwrap_or(false);
                    if again && !WORKER.running.swap(true, Ordering::SeqCst) {
                        continue;
                    }
                    break;
                }
                (None, true) => 60,
                (Some(t), rec) => {
                    let secs = (t - Utc::now()).num_seconds().clamp(0, 3600);
                    if rec {
                        secs.min(60)
                    } else {
                        secs
                    }
                }
            }
            .max(min_wait);
            if wait > 0 {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(wait as u64)) => {}
                    _ = WORKER.notify.notified() => {}
                }
            }
        }
    });
}

/// App exit: stop the running ffmpeg; the job resumes at the next launch.
pub fn shutdown() {
    WORKER.cancel.store(true, Ordering::SeqCst);
}
