//! Delete / Strike a block of meeting time ("everything from 10:41 to
//! 10:53"), docs/REDACTION.md "Time ranges".
//!
//! Within `[start, end]` this removes, with every purge-checklist step:
//! - **screens** captured in the span (legacy `frames` rows and
//!   `screen_states` that start in it) and everything derived from them;
//! - **screen text** captured in the span that no removed screen owns
//!   (OCR/accessibility snapshots), AI screen-activity summaries and
//!   timeline entries from the span;
//! - **transcript lines**: a line with stored word timings is split
//!   exactly (a word goes when the middle of its time falls inside the
//!   range); a line without them goes whole only if at least half of it
//!   falls inside (its length is estimated from its word count), and the
//!   preview says so;
//! - the **screen video** for the span (DMG), blanked by the background job.
//!
//! The plan is resolved up front (ids, UTF-16 offsets, text hashes; never
//! content) so the preview's counts are exactly what is removed: a Delete
//! stores the plan in its pending row and is dropped if anything in it
//! changed during the undo window.

use super::*;

/// Lines without word timings: estimated length per word, and bounds.
pub const EST_MS_PER_WORD: i64 = 400;
pub const EST_MIN_MS: i64 = 1_000;
pub const EST_MAX_MS: i64 = 30_000;

/// One transcript line in a range plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RangeLine {
    pub transcript_id: i64,
    /// UTF-16 [start, end) of the words removed (whole line: all of it)
    pub start: usize,
    pub end: usize,
    pub whole: bool,
    /// Line hash when planned; the action is refused if the line changed
    pub text_hash: String,
    /// Had no word timings: included because ≥ 50% of its estimated span
    /// is inside the range
    #[serde(default)]
    pub estimated: bool,
    #[serde(default)]
    pub words: usize,
}

/// How one line relates to a range (pure, see [`line_cut`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LineCut {
    /// Nothing of the line is inside
    Outside,
    /// The whole line goes
    Whole { estimated: bool },
    /// Exactly these words go (word timings): UTF-16 [start, end)
    Words { start: usize, end: usize },
    /// No word timings and less than half of it is inside: kept
    KeptEstimate,
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|t| !t.contains("strickenid")).count()
}

/// Which part of a line a range removes. `line_ts` is the line's start,
/// `next_ts` the next line's start (bounds the estimate for lines without
/// word timings). Times in the timings are ms from `line_ts`.
pub fn line_cut(
    text: &str,
    timings_json: Option<&str>,
    line_ts: DateTime<Utc>,
    next_ts: Option<DateTime<Utc>>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> LineCut {
    let timings: Vec<WordTiming> = timings_json
        .and_then(|j| serde_json::from_str::<Vec<WordTiming>>(j).ok())
        .unwrap_or_default();
    if !timings.is_empty() {
        let mut ts = timings;
        ts.sort_by_key(|w| (w.t0, w.s));
        let inside: Vec<&WordTiming> = ts
            .iter()
            .filter(|w| {
                let mid = line_ts + chrono::Duration::milliseconds((w.t0 + w.t1) / 2);
                mid >= start && mid <= end
            })
            .collect();
        if inside.is_empty() {
            return LineCut::Outside;
        }
        if inside.len() == ts.len() {
            return LineCut::Whole { estimated: false };
        }
        let s = inside.iter().map(|w| w.s).min().unwrap_or(0);
        let e = inside.iter().map(|w| w.e).max().unwrap_or(0);
        return LineCut::Words { start: s, end: e };
    }
    // No timings: estimate the line's span from its word count
    let mut dur = (word_count(text) as i64 * EST_MS_PER_WORD).clamp(EST_MIN_MS, EST_MAX_MS);
    if let Some(n) = next_ts {
        let gap = (n - line_ts).num_milliseconds();
        if gap > 0 {
            dur = dur.min(gap);
        }
    }
    let line_end = line_ts + chrono::Duration::milliseconds(dur);
    let overlap = (end.min(line_end) - start.max(line_ts)).num_milliseconds();
    if overlap < 0 || (overlap == 0 && !(line_ts >= start && line_ts <= end)) {
        return LineCut::Outside;
    }
    if overlap * 2 >= dur {
        LineCut::Whole { estimated: true }
    } else {
        LineCut::KeptEstimate
    }
}

/// Screen text, AI screen-activity summaries and timeline entries captured
/// inside a span that no removed screen owns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangeExtras {
    pub snapshot_ids: Vec<String>,
    pub activity_ids: Vec<i64>,
    pub event_ids: Vec<String>,
}

impl RangeExtras {
    pub fn is_empty(&self) -> bool {
        self.snapshot_ids.is_empty() && self.activity_ids.is_empty() && self.event_ids.is_empty()
    }
}

fn in_span(ts: Option<DateTime<Utc>>, start: DateTime<Utc>, end: DateTime<Utc>) -> bool {
    ts.map_or(false, |t| t >= start && t <= end)
}

/// Schema-tolerant (also runs against older app backups).
pub(crate) async fn find_range_extras(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<RangeExtras, sqlx::Error> {
    let tables = table_names(conn).await?;
    let mut out = RangeExtras::default();
    if tables.contains("text_snapshots") {
        let by_meeting = has_column(conn, "text_snapshots", "meeting_id").await;
        let sql = format!(
            "SELECT snapshot_id, ts FROM text_snapshots WHERE {} \
             state_id IN (SELECT state_id FROM screen_states WHERE meeting_id = ?1) \
             OR episode_id IN (SELECT episode_id FROM document_episodes WHERE meeting_id = ?1)",
            if by_meeting { "meeting_id = ?1 OR" } else { "" }
        );
        for r in sqlx::query(&sql).bind(meeting_id).fetch_all(&mut *conn).await? {
            if in_span(parse_ts(&r.get::<String, _>("ts")), start, end) {
                out.snapshot_ids.push(r.get("snapshot_id"));
            }
        }
    }
    if tables.contains("activity_log") {
        for r in sqlx::query("SELECT id, start_time FROM activity_log").fetch_all(&mut *conn).await? {
            if in_span(parse_ts(&r.get::<String, _>("start_time")), start, end) {
                out.activity_ids.push(r.get("id"));
            }
        }
    }
    if tables.contains("meeting_timeline_events") {
        for r in sqlx::query("SELECT event_id, ts FROM meeting_timeline_events WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await?
        {
            if in_span(parse_ts(&r.get::<String, _>("ts")), start, end) {
                out.event_ids.push(r.get("event_id"));
            }
        }
    }
    Ok(out)
}

/// Delete [`RangeExtras`] and what derives from them (text patches, the data
/// editor's history, VLM entities); flag assistant chats that cited them.
pub(crate) async fn purge_range_extras(conn: &mut SqliteConnection, x: &RangeExtras) -> Result<(), sqlx::Error> {
    let tables = table_names(conn).await?;
    let has = |t: &str| tables.contains(t);
    for sid in &x.snapshot_ids {
        let rowid: Option<String> =
            sqlx::query_scalar("SELECT CAST(rowid AS TEXT) FROM text_snapshots WHERE snapshot_id = ?")
                .bind(sid)
                .fetch_optional(&mut *conn)
                .await?;
        if has("data_versions") {
            for key in std::iter::once(sid.clone()).chain(rowid) {
                sqlx::query("DELETE FROM data_versions WHERE entity_type = 'text_snapshot' AND entity_id = ?")
                    .bind(key)
                    .execute(&mut *conn)
                    .await?;
            }
        }
        if has("text_patches") {
            sqlx::query("DELETE FROM text_patches WHERE from_snapshot_id = ? OR to_snapshot_id = ?")
                .bind(sid)
                .bind(sid)
                .execute(&mut *conn)
                .await?;
        }
        sqlx::query("DELETE FROM text_snapshots WHERE snapshot_id = ?").bind(sid).execute(&mut *conn).await?;
        if has("assistant_conversations") && has_column(conn, "assistant_conversations", "stale_after_edit").await {
            sqlx::query("UPDATE assistant_conversations SET stale_after_edit = 1 WHERE context_refs LIKE ?")
                .bind(format!("%snapshot-{}%", sid))
                .execute(&mut *conn)
                .await?;
        }
    }
    for aid in &x.activity_ids {
        if has("entities") {
            sqlx::query("DELETE FROM entities WHERE activity_id = ?").bind(aid).execute(&mut *conn).await?;
        }
        if has("data_versions") {
            sqlx::query("DELETE FROM data_versions WHERE entity_id = ? AND entity_type LIKE '%activit%'")
                .bind(aid.to_string())
                .execute(&mut *conn)
                .await?;
        }
        if has("frame_queue") {
            // The VLM queue row that produced it (frame_ids = queue id)
            sqlx::query("DELETE FROM frame_queue WHERE CAST(id AS TEXT) IN (SELECT frame_ids FROM activity_log WHERE id = ?)")
                .bind(aid)
                .execute(&mut *conn)
                .await?;
        }
        sqlx::query("DELETE FROM activity_log WHERE id = ?").bind(aid).execute(&mut *conn).await?;
    }
    for ev in &x.event_ids {
        sqlx::query("DELETE FROM meeting_timeline_events WHERE event_id = ?").bind(ev).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Everything a range removes, resolved up front.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangePlan {
    pub meeting_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub meeting_start: DateTime<Utc>,
    pub screen_ids: Vec<String>,
    pub image_files: usize,
    pub lines: Vec<RangeLine>,
    /// Lines without word timings that overlap the range by less than half
    pub estimated_excluded: usize,
    pub extras: RangeExtras,
}

impl RangePlan {
    pub fn counts(&self) -> RangeCounts {
        RangeCounts {
            screens: self.screen_ids.len(),
            lines_whole: self.lines.iter().filter(|l| l.whole).count(),
            lines_split: self.lines.iter().filter(|l| !l.whole).count(),
        }
    }
}

/// What the confirmation showed; the action is refused if the range now
/// resolves to something else (the user sees exactly what is removed).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RangeCounts {
    pub screens: usize,
    pub lines_whole: usize,
    pub lines_split: usize,
}

async fn meeting_start(conn: &mut SqliteConnection, meeting_id: &str) -> Result<DateTime<Utc>, String> {
    let s: Option<String> = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Database busy"))?;
    s.as_deref().and_then(parse_ts).ok_or_else(|| "That meeting no longer exists".to_string())
}

/// Resolve a range of a meeting (wall clock) into what it removes.
pub async fn plan_range(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<RangePlan, String> {
    if end <= start {
        return Err("The end of the time range must be after its start".into());
    }
    let meeting_start = meeting_start(conn, meeting_id).await?;
    let tables = table_names(conn).await.map_err(err("Failed to read schema"))?;
    let mut plan = RangePlan { meeting_id: meeting_id.into(), start, end, meeting_start, ..Default::default() };

    // Screens: legacy frames, and screen states that start in the range
    if tables.contains("frames") {
        for r in sqlx::query("SELECT id, timestamp, file_path FROM frames WHERE meeting_id = ? ORDER BY timestamp, id")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(err("Failed to read screens"))?
        {
            if in_span(parse_ts(&r.get::<String, _>("timestamp")), start, end) {
                plan.screen_ids.push(r.get::<i64, _>("id").to_string());
                if r.get::<Option<String>, _>("file_path").map_or(false, |f| !f.is_empty()) {
                    plan.image_files += 1;
                }
            }
        }
    }
    if tables.contains("screen_states") {
        for r in sqlx::query(
            "SELECT state_id, start_ts, keyframe_path FROM screen_states WHERE meeting_id = ? ORDER BY start_ts, state_id",
        )
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(err("Failed to read screens"))?
        {
            if in_span(parse_ts(&r.get::<String, _>("start_ts")), start, end) {
                plan.screen_ids.push(r.get("state_id"));
                if r.get::<Option<String>, _>("keyframe_path").map_or(false, |f| !f.is_empty()) {
                    plan.image_files += 1;
                }
            }
        }
    }

    // Transcript lines, in spoken order
    let rows = sqlx::query(
        "SELECT id, text, word_timings, timestamp FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC, id ASC",
    )
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Failed to read the transcript"))?;
    let parsed: Vec<(i64, String, Option<String>, Option<DateTime<Utc>>)> = rows
        .iter()
        .map(|r| (r.get("id"), r.get("text"), r.get("word_timings"), parse_ts(&r.get::<String, _>("timestamp"))))
        .collect();
    for (i, (id, text, timings, ts)) in parsed.iter().enumerate() {
        let Some(ts) = *ts else { continue };
        let next = parsed[i + 1..].iter().filter_map(|p| p.3).find(|n| *n > ts);
        let len16 = utf16_len(text);
        let (s, e, estimated) = match line_cut(text, timings.as_deref(), ts, next, start, end) {
            LineCut::Outside => continue,
            LineCut::KeptEstimate => {
                plan.estimated_excluded += 1;
                continue;
            }
            LineCut::Whole { estimated } => (0, len16, estimated),
            LineCut::Words { start: s, end: e } => (s, e, false),
        };
        // Same edit the action will make (snaps to whole words; a line that
        // holds only strike markers has nothing left to remove)
        let Ok(edit) = apply_word_edit(text, timings.as_deref(), s, e, None) else { continue };
        let whole = edit.whole_line;
        plan.lines.push(RangeLine {
            transcript_id: *id,
            start: if whole { 0 } else { s },
            end: if whole { len16 } else { e },
            whole,
            text_hash: crate::database::transcript_text_hash(text),
            estimated,
            words: edit.removed_plain.split_whitespace().count(),
        });
    }

    // Loose screen text / activity / timeline entries. Rows a removed screen
    // owns go with the screen; they aren't counted twice in the preview.
    let mut extras = find_range_extras(conn, meeting_id, start, end).await.map_err(err("Failed to read screen text"))?;
    if !plan.screen_ids.is_empty() {
        let screens = resolve_screens(conn, meeting_id, &plan.screen_ids, true).await?;
        let mut owned_snaps: HashSet<String> = HashSet::new();
        let mut owned_events: HashSet<String> = HashSet::new();
        let mut owned_activity: HashSet<i64> = HashSet::new();
        for sc in &screens {
            if let ScreenSource::State(state_id) = &sc.source {
                if tables.contains("text_snapshots") {
                    let ids: Vec<String> = sqlx::query_scalar("SELECT snapshot_id FROM text_snapshots WHERE state_id = ?")
                        .bind(state_id)
                        .fetch_all(&mut *conn)
                        .await
                        .map_err(err("Failed to read screen text"))?;
                    owned_snaps.extend(ids);
                }
                if tables.contains("meeting_timeline_events") {
                    let ids: Vec<String> =
                        sqlx::query_scalar("SELECT event_id FROM meeting_timeline_events WHERE state_id = ?")
                            .bind(state_id)
                            .fetch_all(&mut *conn)
                            .await
                            .map_err(err("Failed to read timeline"))?;
                    owned_events.extend(ids);
                }
            }
            if tables.contains("activity_log") {
                let frame_id = match &sc.source {
                    ScreenSource::Frame(n) => Some(*n),
                    ScreenSource::State(_) => None,
                };
                let ids: Vec<i64> = sqlx::query_scalar(
                    "SELECT a.id FROM activity_log a WHERE a.start_time = ?1 OR a.frame_ids IN \
                     (SELECT CAST(q.id AS TEXT) FROM frame_queue q WHERE q.frame_id = ?2 OR (q.frame_path = ?3 AND q.frame_path <> ''))",
                )
                .bind(sc.start_raw.as_deref().unwrap_or(""))
                .bind(frame_id)
                .bind(sc.file.as_deref().unwrap_or(""))
                .fetch_all(&mut *conn)
                .await
                .map_err(err("Failed to read screen activity"))?;
                owned_activity.extend(ids);
            }
        }
        extras.snapshot_ids.retain(|s| !owned_snaps.contains(s));
        extras.event_ids.retain(|e| !owned_events.contains(e));
        extras.activity_ids.retain(|a| !owned_activity.contains(a));
    }
    plan.extras = extras;
    Ok(plan)
}

/// The confirmation's numbers and lines. No content: counts and offsets.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimeRangePreview {
    pub start: String,
    pub end: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub counts: RangeCounts,
    pub screen_ids: Vec<String>,
    pub image_files: usize,
    /// Lines touched: id + UTF-16 range (the UI hides them during the undo
    /// window). Whole lines cover the full text.
    pub lines: Vec<PreviewLine>,
    pub words_removed: usize,
    pub estimated_included: usize,
    pub estimated_excluded: usize,
    pub screen_text_snapshots: usize,
    pub activity_summaries: usize,
    pub timeline_entries: usize,
    pub nothing: bool,
    /// What will be destroyed, one line each (same style as the Strike
    /// confirmation for words/screens)
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreviewLine {
    pub transcript_id: i64,
    pub start: usize,
    pub end: usize,
    pub whole: bool,
}

fn n_s(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

impl RangePlan {
    fn is_empty(&self) -> bool {
        self.screen_ids.is_empty() && self.lines.is_empty() && self.extras.is_empty()
    }
}

pub(crate) async fn build_preview(
    conn: &mut SqliteConnection,
    env: &RedactionEnv,
    plan: &RangePlan,
) -> TimeRangePreview {
    let c = plan.counts();
    let words: usize = plan.lines.iter().map(|l| l.words).sum();
    let est_in = plan.lines.iter().filter(|l| l.estimated).count();
    let mut items = Vec::new();
    if !plan.screen_ids.is_empty() {
        items.push(format!(
            "{} and {} (with the text read from them, their AI analysis and cached frames)",
            n_s(c.screens, "screen", "screens"),
            n_s(plan.image_files, "image file", "image files")
        ));
    }
    let x = &plan.extras;
    if !x.is_empty() {
        let mut parts = Vec::new();
        if !x.snapshot_ids.is_empty() {
            parts.push(n_s(x.snapshot_ids.len(), "screen-text snapshot", "screen-text snapshots"));
        }
        if !x.activity_ids.is_empty() {
            parts.push(n_s(x.activity_ids.len(), "AI screen-activity summary", "AI screen-activity summaries"));
        }
        if !x.event_ids.is_empty() {
            parts.push(n_s(x.event_ids.len(), "timeline entry", "timeline entries"));
        }
        items.push(format!("Also captured in that span: {}", parts.join(", ")));
    }
    if !plan.lines.is_empty() {
        let mut s = format!(
            "{} from the transcript and search ({} removed whole",
            n_s(c.lines_whole + c.lines_split, "line", "lines"),
            c.lines_whole
        );
        if c.lines_split > 0 {
            s.push_str(&format!(
                ", {} split at the edge of the range using word timings",
                c.lines_split
            ));
        }
        s.push_str(&format!("; {} in all)", n_s(words, "word", "words")));
        items.push(s);
    }
    if est_in > 0 {
        items.push(format!(
            "{} without word timings {} included because at least half of {} falls inside the range (length estimated from the word count)",
            n_s(est_in, "line", "lines"),
            if est_in == 1 { "is" } else { "are" },
            if est_in == 1 { "it" } else { "each" }
        ));
    }
    if plan.estimated_excluded > 0 {
        items.push(format!(
            "{} without word timings {} kept because less than half of {} falls inside the range",
            n_s(plan.estimated_excluded, "line", "lines"),
            if plan.estimated_excluded == 1 { "is" } else { "are" },
            if plan.estimated_excluded == 1 { "it" } else { "each" }
        ));
    }
    if !plan.lines.is_empty() {
        items.extend(ai_preview(conn, &plan.meeting_id).await);
        items.push("Any copies in the app's log files".into());
    }
    items.extend(video_preview(env, &plan.meeting_id, &[(plan.start, plan.end)]));
    items.extend(common_preview(env, &plan.meeting_id));
    items.push(NO_AUDIO_NOTICE.into());
    let ms = |t: DateTime<Utc>| (t - plan.meeting_start).num_milliseconds();
    TimeRangePreview {
        start: plan.start.to_rfc3339(),
        end: plan.end.to_rfc3339(),
        start_ms: ms(plan.start),
        end_ms: ms(plan.end),
        counts: c,
        screen_ids: plan.screen_ids.clone(),
        image_files: plan.image_files,
        lines: plan
            .lines
            .iter()
            .map(|l| PreviewLine { transcript_id: l.transcript_id, start: l.start, end: l.end, whole: l.whole })
            .collect(),
        words_removed: words,
        estimated_included: est_in,
        estimated_excluded: plan.estimated_excluded,
        screen_text_snapshots: x.snapshot_ids.len(),
        activity_summaries: x.activity_ids.len(),
        timeline_entries: x.event_ids.len(),
        nothing: plan.is_empty(),
        items,
    }
}

/// Apply a resolved range under the lock, in one transaction: transcript
/// lines, screens, loose screen text; strike markers or the pending row;
/// the screen video job. Then the shared post-commit purge.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_range_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    (start, end): (DateTime<Utc>, DateTime<Utc>),
    screen_ids: &[String],
    lines: &[RangeLine],
    action: Action,
    reason: Option<&str>,
    record_id: &str,
) -> Result<ActionOutcome, String> {
    env.ensure_not_recording(meeting_id)?;
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;

    // Compute every line edit first (so the reason can be checked against
    // all removed words before anything is written)
    let marker = (action == Action::Strike).then(|| marker_token(record_id));
    let mut edits: Vec<(&RangeLine, String, TextEdit)> = Vec::new();
    for l in lines {
        let line = load_line(&mut tx, meeting_id, l.transcript_id).await?;
        if crate::database::transcript_text_hash(&line.text) != l.text_hash {
            return Err("The transcript in that time range changed. Review the range again.".into());
        }
        // A strike leaves one marker for the whole span, in its first line
        let m = if edits.is_empty() { marker.as_deref() } else { None };
        let edit = apply_word_edit(&line.text, line.timings.as_deref(), l.start, l.end, m)?;
        edits.push((l, line.text, edit));
    }
    let reason = if action == Action::Strike {
        let mut r = validate_reason(reason, None)?;
        for (_, _, e) in &edits {
            r = validate_reason(r.as_deref(), Some(&e.removed_plain))?;
        }
        r
    } else {
        None
    };
    let mut changes = Vec::new();
    for (l, original, edit) in &edits {
        write_line_edit(&mut tx, meeting_id, l.transcript_id, edit, action.replacement()).await?;
        changes.push(LineChange {
            transcript_id: l.transcript_id,
            original: original.clone(),
            new_text: edit.new_text.clone(),
            removed: edit.removed_plain.clone(),
        });
    }

    // Screens + loose screen text in the span
    let screens = resolve_screens(&mut tx, meeting_id, screen_ids, true).await?;
    if screens.len() < screen_ids.iter().collect::<HashSet<_>>().len() {
        return Err("Some screens in that time range were already removed. Review the range again.".into());
    }
    purge_screen_rows(&mut tx, meeting_id, &screens).await.map_err(err("Failed to remove screens"))?;
    let extras = find_range_extras(&mut tx, meeting_id, start, end).await.map_err(err("Failed to read screen text"))?;
    purge_range_extras(&mut tx, &extras).await.map_err(err("Failed to remove screen text"))?;

    let mut out = ActionOutcome::default();
    // (A pending Delete whose content is already gone just finishes.)
    if action == Action::Strike && changes.is_empty() && screens.is_empty() && extras.is_empty() {
        return Err("There's nothing to remove in that time range".into());
    }
    let screen_record_id = new_id();
    // The marker that shows "video pending": the screen strip's, if any
    let video_owner: String =
        if !screens.is_empty() || changes.is_empty() { screen_record_id.clone() } else { record_id.to_string() };
    if has_screen_video(env, meeting_id) {
        out.video_jobs_queued = video_jobs::enqueue(
            &mut tx,
            meeting_id,
            &[(start, end)],
            (action == Action::Strike).then_some(video_owner.as_str()),
        )
        .await
        .map_err(err("Failed to queue the screen video blanking"))?;
    }
    match action {
        Action::Strike => {
            let now = Utc::now().to_rfc3339();
            if !changes.is_empty() {
                let rec = RedactionRecord {
                    id: record_id.to_string(),
                    meeting_id: meeting_id.to_string(),
                    kind: if lines.iter().all(|l| l.whole) { "line" } else { "words" }.into(),
                    action: "strike".into(),
                    media_start: Some(start.to_rfc3339()),
                    media_end: Some(end.to_rfc3339()),
                    created_at: now.clone(),
                    reason: reason.clone(),
                    transcript_id: Some(lines[0].transcript_id),
                    item_count: changes.len() as i64,
                    video_pending: out.video_jobs_queued > 0 && video_owner == record_id,
                };
                insert_record(&mut tx, &rec, None).await?;
                out.records.push(rec);
            }
            if !screens.is_empty() || changes.is_empty() {
                let rec = RedactionRecord {
                    id: screen_record_id.clone(),
                    meeting_id: meeting_id.to_string(),
                    kind: "screen".into(),
                    action: "strike".into(),
                    media_start: Some(start.to_rfc3339()),
                    media_end: Some(end.to_rfc3339()),
                    created_at: now,
                    reason,
                    transcript_id: None,
                    item_count: screens.len() as i64,
                    video_pending: out.video_jobs_queued > 0 && video_owner == screen_record_id,
                };
                insert_record(&mut tx, &rec, None).await?;
                out.records.push(rec);
            }
            out.record = out.records.first().cloned();
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
    let mut jobs = vec![BackupJob::RangeExtras { start, end }];
    if !screen_ids.is_empty() {
        jobs.push(BackupJob::Screens { ids: screen_ids.to_vec() });
    }
    post_commit_purge(pool, env, meeting_id, &changes, jobs, action.replacement(), &mut out).await;
    if !file_errors.is_empty() {
        return Err(format!(
            "The time range was removed from the meeting, but these files could not be deleted: {}",
            file_errors.join("; ")
        ));
    }
    Ok(out)
}

/// `start_ms`/`end_ms` are offsets from the meeting start (the timeline's
/// `timestamp_ms`).
async fn resolve(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<RangePlan, String> {
    let ms0 = meeting_start(conn, meeting_id).await?;
    let start = ms0 + chrono::Duration::milliseconds(start_ms);
    let end = ms0 + chrono::Duration::milliseconds(end_ms);
    plan_range(conn, meeting_id, start, end).await
}

fn check_expected(plan: &RangePlan, expected: Option<RangeCounts>) -> Result<(), String> {
    if plan.is_empty() {
        return Err("There's nothing to remove in that time range".into());
    }
    match expected {
        Some(e) if e != plan.counts() => {
            Err("That time range now covers different content than the preview showed. Review it again.".into())
        }
        _ => Ok(()),
    }
}

/// What a time range removes (for the confirmation). Counts and offsets only.
pub async fn preview_time_range(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<TimeRangePreview, String> {
    env.ensure_not_recording(meeting_id)?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let plan = resolve(&mut conn, meeting_id, start_ms, end_ms).await?;
    Ok(build_preview(&mut conn, env, &plan).await)
}

/// Delete a time range: validated and resolved now, committed after the
/// 5-second undo window (like every Delete).
pub async fn request_delete_time_range(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    start_ms: i64,
    end_ms: i64,
    expected: Option<RangeCounts>,
) -> Result<PendingDelete, String> {
    let _g = LOCK.lock().await;
    env.ensure_not_recording(meeting_id)?;
    flush_overlapping_locked(pool, env, meeting_id, Scope::All).await?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let plan = resolve(&mut conn, meeting_id, start_ms, end_ms).await?;
    check_expected(&plan, expected)?;
    let id = new_id();
    let payload = serde_json::to_string(&PendingPayload::TimeRange {
        start: plan.start.to_rfc3339(),
        end: plan.end.to_rfc3339(),
        screen_ids: plan.screen_ids.clone(),
        lines: plan.lines.clone(),
    })
    .map_err(err("Failed to queue the delete"))?;
    let kind = if !plan.screen_ids.is_empty() { "screen" } else { "line" };
    let rec = RedactionRecord {
        id: id.clone(),
        meeting_id: meeting_id.to_string(),
        kind: kind.into(),
        action: "delete".into(),
        media_start: None,
        media_end: None,
        created_at: Utc::now().to_rfc3339(),
        reason: None,
        transcript_id: None,
        item_count: (plan.screen_ids.len() + plan.lines.len()).max(1) as i64,
        video_pending: false,
    };
    insert_record(&mut conn, &rec, Some(&payload)).await?;
    Ok(PendingDelete { id, meeting_id: meeting_id.to_string(), kind: kind.into(), undo_seconds: UNDO_WINDOW_SECS })
}

/// Strike a time range: no undo; one marker in the transcript (if it had
/// lines) and one in the screen strip (if it had screens), both covering the
/// span and carrying the reason.
pub async fn strike_time_range(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    start_ms: i64,
    end_ms: i64,
    reason: Option<&str>,
    expected: Option<RangeCounts>,
) -> Result<ActionOutcome, String> {
    let _g = LOCK.lock().await;
    env.ensure_not_recording(meeting_id)?;
    flush_overlapping_locked(pool, env, meeting_id, Scope::All).await?;
    let plan = {
        let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
        resolve(&mut conn, meeting_id, start_ms, end_ms).await?
    };
    check_expected(&plan, expected)?;
    apply_range_locked(
        pool,
        env,
        meeting_id,
        (plan.start, plan.end),
        &plan.screen_ids,
        &plan.lines,
        Action::Strike,
        reason,
        &new_id(),
    )
    .await
}
