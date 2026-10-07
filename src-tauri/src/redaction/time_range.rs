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
//! - the **screen video** for the span (DMG), blanked by the background job;
//! - **moment markers** placed in the span, with their notes (markers.rs).
//!   Like screen text they are found at commit, so a Delete's undo window
//!   keeps them.
//!
//! The plan is resolved up front (ids, UTF-16 offsets, text hashes; never
//! content) so the preview's counts are exactly what is removed: a Delete
//! stores the plan in its pending row and is dropped if anything in it
//! changed during the undo window.
//!
//! One action can cover **several disjoint ranges** (a linked selection in
//! the Recordings view: "17 screens · 42 lines · 2 groups"). They are
//! planned, previewed, undone and committed together: one pending row, one
//! undo. A Strike leaves markers for each range.

use super::*;
use std::collections::HashMap;

/// Lines without word timings: estimated length per word, and bounds.
pub const EST_MS_PER_WORD: i64 = 400;
pub const EST_MIN_MS: i64 = 1_000;
pub const EST_MAX_MS: i64 = 30_000;

/// One span of meeting time from the UI: ms offsets from the meeting start,
/// both ends included.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MsRange {
    pub start_ms: i64,
    pub end_ms: i64,
}

type Span = (DateTime<Utc>, DateTime<Utc>);

/// A range end from the timeline covers its whole millisecond (see
/// [`resolve`]); markers record the end as selected, without it.
const MS_EDGE: chrono::Duration = chrono::Duration::nanoseconds(999_999);

/// Sort ranges and merge the ones that overlap or touch (≤ 1 ms apart), so
/// the result is disjoint and in time order.
pub fn merge_spans(ranges: &[Span]) -> Vec<Span> {
    let mut v: Vec<Span> = ranges.to_vec();
    v.sort();
    let mut out: Vec<Span> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(last) if a <= last.1 + chrono::Duration::milliseconds(1) => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// One transcript line in a range plan. A line cut by more than one range
/// (word timings, words kept in between) has one entry per removed run.
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
    /// Which of the action's ranges these words fall in (index into the
    /// plan's ranges; a Strike puts each range's marker in the first entry
    /// that falls in it). Empty in plans made before multi-range: range 0.
    #[serde(default)]
    pub ranges: Vec<usize>,
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

/// A run of consecutive words removed from one line (word timings).
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// UTF-16 [start, end)
    pub start: usize,
    pub end: usize,
    /// Indexes of the ranges its words fall in
    pub ranges: Vec<usize>,
}

/// How one line relates to a set of disjoint ranges (pure, see [`line_cuts`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LineCuts {
    Outside,
    Whole { estimated: bool, ranges: Vec<usize> },
    /// Runs of consecutive words (word timings), in text order
    Words(Vec<Cut>),
    KeptEstimate,
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|t| !t.contains("strickenid")).count()
}

fn parse_timings(timings_json: Option<&str>) -> Vec<WordTiming> {
    timings_json
        .and_then(|j| serde_json::from_str::<Vec<WordTiming>>(j).ok())
        .unwrap_or_default()
}

/// Estimated length of a line without word timings: its word count at
/// [`EST_MS_PER_WORD`], 1–30 s, and never past the next line's start.
fn estimated_ms(text: &str, line_ts: DateTime<Utc>, next_ts: Option<DateTime<Utc>>) -> i64 {
    let mut dur = (word_count(text) as i64 * EST_MS_PER_WORD).clamp(EST_MIN_MS, EST_MAX_MS);
    if let Some(n) = next_ts {
        let gap = (n - line_ts).num_milliseconds();
        if gap > 0 {
            dur = dur.min(gap);
        }
    }
    dur
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
    match line_cuts(text, timings_json, line_ts, next_ts, &[(start, end)]) {
        LineCuts::Outside => LineCut::Outside,
        LineCuts::KeptEstimate => LineCut::KeptEstimate,
        LineCuts::Whole { estimated, .. } => LineCut::Whole { estimated },
        LineCuts::Words(cuts) => LineCut::Words {
            start: cuts.iter().map(|c| c.start).min().unwrap_or(0),
            end: cuts.iter().map(|c| c.end).max().unwrap_or(0),
        },
    }
}

/// Which parts of a line a set of disjoint, sorted ranges removes.
///
/// - With word timings, a word goes when the middle of its time is inside
///   any range; the words that go form runs (in text order), and words kept
///   between two ranges split the line into several cuts.
/// - Without them, the line's span is estimated from its word count and it
///   goes whole only if at least half of it is inside the ranges together.
pub fn line_cuts(
    text: &str,
    timings_json: Option<&str>,
    line_ts: DateTime<Utc>,
    next_ts: Option<DateTime<Utc>>,
    ranges: &[Span],
) -> LineCuts {
    let range_of = |t: DateTime<Utc>| ranges.iter().position(|(a, b)| t >= *a && t <= *b);
    let mut timings = parse_timings(timings_json);
    if !timings.is_empty() {
        timings.sort_by_key(|w| (w.s, w.e, w.t0));
        let hits: Vec<Option<usize>> = timings
            .iter()
            .map(|w| range_of(line_ts + chrono::Duration::milliseconds((w.t0 + w.t1) / 2)))
            .collect();
        if hits.iter().all(Option::is_none) {
            return LineCuts::Outside;
        }
        if hits.iter().all(Option::is_some) {
            let mut r: Vec<usize> = hits.iter().flatten().copied().collect();
            r.sort_unstable();
            r.dedup();
            return LineCuts::Whole { estimated: false, ranges: r };
        }
        let mut cuts: Vec<Cut> = Vec::new();
        let mut open: Option<Cut> = None;
        for (w, hit) in timings.iter().zip(&hits) {
            match (hit, open.as_mut()) {
                (Some(k), Some(c)) => {
                    c.start = c.start.min(w.s);
                    c.end = c.end.max(w.e);
                    if !c.ranges.contains(k) {
                        c.ranges.push(*k);
                    }
                }
                (Some(k), None) => open = Some(Cut { start: w.s, end: w.e, ranges: vec![*k] }),
                (None, _) => cuts.extend(open.take()),
            }
        }
        cuts.extend(open);
        for c in &mut cuts {
            c.ranges.sort_unstable();
        }
        return LineCuts::Words(cuts);
    }
    // No timings: estimate the line's span from its word count
    let dur = estimated_ms(text, line_ts, next_ts);
    let line_end = line_ts + chrono::Duration::milliseconds(dur);
    let mut total = 0i64;
    let mut touched = Vec::new();
    for (k, (a, b)) in ranges.iter().enumerate() {
        let overlap = ((*b).min(line_end) - (*a).max(line_ts)).num_milliseconds();
        if overlap < 0 || (overlap == 0 && !(line_ts >= *a && line_ts <= *b)) {
            continue;
        }
        total += overlap;
        touched.push(k);
    }
    if touched.is_empty() {
        LineCuts::Outside
    } else if total * 2 >= dur {
        LineCuts::Whole { estimated: true, ranges: touched }
    } else {
        LineCuts::KeptEstimate
    }
}

/// A line's time span as a time-range action reads it, for the timeline UI
/// (linked selection of screens and lines): ms from the meeting start.
#[derive(Debug, Clone, PartialEq)]
pub struct LineExtent {
    /// End of the span, rounded up: its last word's end (word timings), else
    /// its estimated length (bounded by the next line)
    pub end_ms: i64,
    /// Middle of each word's time, in text order, when word timings are
    /// stored (rounded down, like the line's own start). Times only.
    pub word_mids_ms: Option<Vec<i64>>,
}

fn ms_floor(d: chrono::Duration) -> i64 {
    let ms = d.num_milliseconds();
    if chrono::Duration::milliseconds(ms) > d {
        ms - 1
    } else {
        ms
    }
}

fn ms_ceil(d: chrono::Duration) -> i64 {
    let ms = d.num_milliseconds();
    if chrono::Duration::milliseconds(ms) < d {
        ms + 1
    } else {
        ms
    }
}

/// Pure: one line's extent (see [`LineExtent`]).
pub fn line_extent(
    text: &str,
    timings_json: Option<&str>,
    meeting_start: DateTime<Utc>,
    line_ts: DateTime<Utc>,
    next_ts: Option<DateTime<Utc>>,
) -> LineExtent {
    let mut timings = parse_timings(timings_json);
    if !timings.is_empty() {
        timings.sort_by_key(|w| (w.s, w.e, w.t0));
        let last = timings.iter().map(|w| w.t1.max(w.t0)).max().unwrap_or(0).max(0);
        let mids = timings
            .iter()
            .map(|w| ms_floor(line_ts + chrono::Duration::milliseconds((w.t0 + w.t1) / 2) - meeting_start))
            .collect();
        return LineExtent {
            end_ms: ms_ceil(line_ts + chrono::Duration::milliseconds(last) - meeting_start),
            word_mids_ms: Some(mids),
        };
    }
    let dur = estimated_ms(text, line_ts, next_ts);
    LineExtent { end_ms: ms_ceil(line_ts + chrono::Duration::milliseconds(dur) - meeting_start), word_mids_ms: None }
}

/// Every line's extent in a meeting, by transcript id. A line whose
/// timestamp can't be read is left out (no time range can reach it).
pub async fn line_extents(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    meeting_start: DateTime<Utc>,
) -> Result<HashMap<i64, LineExtent>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, text, word_timings, timestamp FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC, id ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let parsed: Vec<(i64, String, Option<String>, Option<DateTime<Utc>>)> = rows
        .iter()
        .map(|r| (r.get("id"), r.get("text"), r.get("word_timings"), parse_ts(&r.get::<String, _>("timestamp"))))
        .collect();
    let mut out = HashMap::new();
    for (i, (id, text, timings, ts)) in parsed.iter().enumerate() {
        let Some(ts) = *ts else { continue };
        let next = parsed[i + 1..].iter().filter_map(|p| p.3).find(|n| *n > ts);
        out.insert(*id, line_extent(text, timings.as_deref(), meeting_start, ts, next));
    }
    Ok(out)
}

/// Screen text, AI screen-activity summaries and timeline entries captured
/// inside a span that no removed screen owns, and the moment markers
/// (with their notes) placed inside it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangeExtras {
    pub snapshot_ids: Vec<String>,
    pub activity_ids: Vec<i64>,
    pub event_ids: Vec<String>,
    /// `meeting_markers` ids (markers.rs)
    pub marker_ids: Vec<String>,
}

impl RangeExtras {
    pub fn is_empty(&self) -> bool {
        self.snapshot_ids.is_empty()
            && self.activity_ids.is_empty()
            && self.event_ids.is_empty()
            && self.marker_ids.is_empty()
    }
}

fn in_spans(ts: Option<DateTime<Utc>>, ranges: &[Span]) -> bool {
    ts.map_or(false, |t| ranges.iter().any(|(a, b)| t >= *a && t <= *b))
}

/// Schema-tolerant (also runs against older app backups).
pub(crate) async fn find_range_extras(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    ranges: &[Span],
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
            if in_spans(parse_ts(&r.get::<String, _>("ts")), ranges) {
                out.snapshot_ids.push(r.get("snapshot_id"));
            }
        }
    }
    if tables.contains("activity_log") {
        for r in sqlx::query("SELECT id, start_time FROM activity_log").fetch_all(&mut *conn).await? {
            if in_spans(parse_ts(&r.get::<String, _>("start_time")), ranges) {
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
            if in_spans(parse_ts(&r.get::<String, _>("ts")), ranges) {
                out.event_ids.push(r.get("event_id"));
            }
        }
    }
    if tables.contains("meeting_markers") {
        for r in sqlx::query("SELECT id, ts FROM meeting_markers WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await?
        {
            if in_spans(parse_ts(&r.get::<String, _>("ts")), ranges) {
                out.marker_ids.push(r.get("id"));
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
    if has("meeting_markers") {
        for id in &x.marker_ids {
            sqlx::query("DELETE FROM meeting_markers WHERE id = ?").bind(id).execute(&mut *conn).await?;
        }
    }
    Ok(())
}

/// Everything a set of ranges removes, resolved up front.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangePlan {
    pub meeting_id: String,
    /// First range's start and last range's end
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// The ranges, merged: disjoint and in time order
    pub ranges: Vec<Span>,
    pub meeting_start: DateTime<Utc>,
    pub screen_ids: Vec<String>,
    pub image_files: usize,
    pub lines: Vec<RangeLine>,
    /// Lines without word timings that overlap the ranges by less than half
    pub estimated_excluded: usize,
    pub extras: RangeExtras,
}

impl RangePlan {
    /// Counts lines, not cuts: a line cut by two ranges is one split line.
    pub fn counts(&self) -> RangeCounts {
        let whole: HashSet<i64> = self.lines.iter().filter(|l| l.whole).map(|l| l.transcript_id).collect();
        let split: HashSet<i64> = self
            .lines
            .iter()
            .filter(|l| !l.whole && !whole.contains(&l.transcript_id))
            .map(|l| l.transcript_id)
            .collect();
        RangeCounts { screens: self.screen_ids.len(), lines_whole: whole.len(), lines_split: split.len() }
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
    plan_ranges(conn, meeting_id, &[(start, end)]).await
}

/// Resolve several ranges of a meeting (wall clock) into what they remove,
/// together. Overlapping or touching ranges are merged first.
pub async fn plan_ranges(conn: &mut SqliteConnection, meeting_id: &str, ranges: &[Span]) -> Result<RangePlan, String> {
    if ranges.is_empty() {
        return Err("No time range selected".into());
    }
    if ranges.iter().any(|(a, b)| b <= a) {
        return Err("The end of the time range must be after its start".into());
    }
    let ranges = merge_spans(ranges);
    let meeting_start = meeting_start(conn, meeting_id).await?;
    let tables = table_names(conn).await.map_err(err("Failed to read schema"))?;
    let mut plan = RangePlan {
        meeting_id: meeting_id.into(),
        start: ranges[0].0,
        end: ranges[ranges.len() - 1].1,
        ranges: ranges.clone(),
        meeting_start,
        ..Default::default()
    };

    // Screens: legacy frames, and screen states that start in a range
    if tables.contains("frames") {
        for r in sqlx::query("SELECT id, timestamp, file_path FROM frames WHERE meeting_id = ? ORDER BY timestamp, id")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(err("Failed to read screens"))?
        {
            if in_spans(parse_ts(&r.get::<String, _>("timestamp")), &ranges) {
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
            if in_spans(parse_ts(&r.get::<String, _>("start_ts")), &ranges) {
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
        let (cuts, estimated): (Vec<(usize, usize, Vec<usize>)>, bool) =
            match line_cuts(text, timings.as_deref(), ts, next, &ranges) {
                LineCuts::Outside => continue,
                LineCuts::KeptEstimate => {
                    plan.estimated_excluded += 1;
                    continue;
                }
                LineCuts::Whole { estimated, ranges } => (vec![(0, len16, ranges)], estimated),
                LineCuts::Words(cuts) => (cuts.into_iter().map(|c| (c.start, c.end, c.ranges)).collect(), false),
            };
        let hash = crate::database::transcript_text_hash(text);
        for (s, e, in_ranges) in cuts {
            // Same edit the action will make (snaps to whole words; a line
            // that holds only strike markers has nothing left to remove)
            let Ok(edit) = apply_word_edit(text, timings.as_deref(), s, e, None) else { continue };
            let whole = edit.whole_line;
            plan.lines.push(RangeLine {
                transcript_id: *id,
                start: if whole { 0 } else { s },
                end: if whole { len16 } else { e },
                whole,
                text_hash: hash.clone(),
                estimated,
                words: edit.removed_plain.split_whitespace().count(),
                ranges: in_ranges,
            });
        }
    }

    // Loose screen text / activity / timeline entries. Rows a removed screen
    // owns go with the screen; they aren't counted twice in the preview.
    let mut extras = find_range_extras(conn, meeting_id, &ranges).await.map_err(err("Failed to read screen text"))?;
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
    /// The ranges after merging (disjoint, in time order); one for a
    /// single time range
    pub ranges: Vec<MsRange>,
    pub counts: RangeCounts,
    pub screen_ids: Vec<String>,
    pub image_files: usize,
    /// Lines touched: id + UTF-16 range (the UI hides them during the undo
    /// window). Whole lines cover the full text. A line cut by two ranges
    /// appears twice.
    pub lines: Vec<PreviewLine>,
    pub words_removed: usize,
    pub estimated_included: usize,
    pub estimated_excluded: usize,
    pub screen_text_snapshots: usize,
    pub activity_summaries: usize,
    pub timeline_entries: usize,
    /// Moment markers inside the ranges (removed with them, at commit)
    pub moment_markers: usize,
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
    let est_in = plan.lines.iter().filter(|l| l.estimated).map(|l| l.transcript_id).collect::<HashSet<_>>().len();
    let mut items = Vec::new();
    if plan.ranges.len() > 1 {
        items.push(format!("Everything below, across {} separate time spans", plan.ranges.len()));
    }
    if !plan.screen_ids.is_empty() {
        items.push(format!(
            "{} and {} (with the text read from them, their AI analysis and cached frames)",
            n_s(c.screens, "screen", "screens"),
            n_s(plan.image_files, "image file", "image files")
        ));
    }
    let x = &plan.extras;
    if !x.marker_ids.is_empty() {
        items.push(format!(
            "{} you placed in that span (★ / ? / ✎, with {} notes)",
            n_s(x.marker_ids.len(), "moment marker", "moment markers"),
            if x.marker_ids.len() == 1 { "its" } else { "their" }
        ));
    }
    if !(x.snapshot_ids.is_empty() && x.activity_ids.is_empty() && x.event_ids.is_empty()) {
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
    items.extend(video_preview(env, &plan.meeting_id, &plan.ranges));
    items.extend(common_preview(env, &plan.meeting_id));
    items.push(NO_AUDIO_NOTICE.into());
    let ms = |t: DateTime<Utc>| (t - plan.meeting_start).num_milliseconds();
    TimeRangePreview {
        start: plan.start.to_rfc3339(),
        end: plan.end.to_rfc3339(),
        start_ms: ms(plan.start),
        end_ms: ms(plan.end),
        ranges: plan.ranges.iter().map(|(a, b)| MsRange { start_ms: ms(*a), end_ms: ms(*b) }).collect(),
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
        moment_markers: x.marker_ids.len(),
        nothing: plan.is_empty(),
        items,
    }
}

/// Which range a screen belongs to (by its capture time). Screens in a plan
/// were picked because they start inside one; an unreadable time counts as
/// the first range.
fn range_index(ranges: &[Span], t: Option<DateTime<Utc>>) -> usize {
    t.and_then(|t| ranges.iter().position(|(a, b)| t >= *a && t <= *b)).unwrap_or(0)
}

/// Apply resolved ranges under the lock, in one transaction: transcript
/// lines, screens, loose screen text; strike markers or the pending row;
/// the screen video jobs. Then the shared post-commit purge.
///
/// A Strike leaves, per range: one marker in the transcript (in the first
/// line that range touches; the rest closes up) if it removed words, and one
/// in the screen strip if it removed screens or no words.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_range_locked(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[Span],
    screen_ids: &[String],
    lines: &[RangeLine],
    action: Action,
    reason: Option<&str>,
    record_id: &str,
) -> Result<ActionOutcome, String> {
    env.ensure_not_recording(meeting_id)?;
    if ranges.is_empty() {
        return Err("No time range selected".into());
    }
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    let strike = action == Action::Strike;

    // Which ranges each cut falls in (plans from before multi-range: range 0)
    let seg_ranges: Vec<Vec<usize>> = lines
        .iter()
        .map(|l| {
            let r: Vec<usize> = l.ranges.iter().copied().filter(|k| *k < ranges.len()).collect();
            if r.is_empty() {
                vec![0]
            } else {
                r
            }
        })
        .collect();
    // Strike: one transcript marker per range, in the first cut that falls in it
    let line_rec_ids: Vec<String> =
        (0..ranges.len()).map(|k| if k == 0 { record_id.to_string() } else { new_id() }).collect();
    let mut host: Vec<Option<usize>> = vec![None; ranges.len()];
    for (i, rs) in seg_ranges.iter().enumerate() {
        for k in rs {
            if host[*k].is_none() {
                host[*k] = Some(i);
            }
        }
    }
    let seg_marker = |i: usize| -> Option<String> {
        if !strike {
            return None;
        }
        let ms: Vec<String> =
            (0..ranges.len()).filter(|k| host[*k] == Some(i)).map(|k| marker_token(&line_rec_ids[k])).collect();
        (!ms.is_empty()).then(|| ms.join(" "))
    };

    // Compute every line edit first (so the reason can be checked against
    // all removed words before anything is written). A line cut more than
    // once is edited from its end backwards, so earlier offsets stay valid.
    let mut order: Vec<i64> = Vec::new();
    let mut by_line: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        by_line
            .entry(l.transcript_id)
            .or_insert_with(|| {
                order.push(l.transcript_id);
                Vec::new()
            })
            .push(i);
    }
    let mut edits: Vec<(i64, String, TextEdit)> = Vec::new();
    for tid in &order {
        let line = load_line(&mut tx, meeting_id, *tid).await?;
        let segs = &by_line[tid];
        if segs.iter().any(|i| crate::database::transcript_text_hash(&line.text) != lines[*i].text_hash) {
            return Err("The transcript in that time range changed. Review the range again.".into());
        }
        let mut segs = segs.clone();
        segs.sort_by_key(|i| std::cmp::Reverse(lines[*i].start));
        let mut text = line.text.clone();
        let mut timings = line.timings.clone();
        let mut removed: Vec<String> = Vec::new();
        let mut removed_ms: Option<(i64, i64)> = None;
        for i in segs {
            let m = seg_marker(i);
            let e = apply_word_edit(&text, timings.as_deref(), lines[i].start, lines[i].end, m.as_deref())?;
            removed.insert(0, e.removed_plain);
            if let Some((a, b)) = e.removed_ms {
                removed_ms = Some(removed_ms.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
            }
            text = e.new_text;
            timings = e.new_timings;
        }
        let whole_line = word_count(&text) == 0;
        edits.push((
            *tid,
            line.text,
            TextEdit { new_text: text, removed_plain: removed.join(" "), new_timings: timings, removed_ms, whole_line },
        ));
    }
    let reason = if strike {
        let mut r = validate_reason(reason, None)?;
        for (_, _, e) in &edits {
            r = validate_reason(r.as_deref(), Some(&e.removed_plain))?;
        }
        r
    } else {
        None
    };
    let mut changes = Vec::new();
    for (tid, original, edit) in &edits {
        write_line_edit(&mut tx, meeting_id, *tid, edit, action.replacement()).await?;
        changes.push(LineChange {
            transcript_id: *tid,
            original: original.clone(),
            new_text: edit.new_text.clone(),
            removed: edit.removed_plain.clone(),
        });
    }

    // Screens + loose screen text in the spans
    let screens = resolve_screens(&mut tx, meeting_id, screen_ids, true).await?;
    if screens.len() < screen_ids.iter().collect::<HashSet<_>>().len() {
        return Err("Some screens in that time range were already removed. Review the range again.".into());
    }
    purge_screen_rows(&mut tx, meeting_id, &screens).await.map_err(err("Failed to remove screens"))?;
    let extras = find_range_extras(&mut tx, meeting_id, ranges).await.map_err(err("Failed to read screen text"))?;
    purge_range_extras(&mut tx, &extras).await.map_err(err("Failed to remove screen text"))?;

    let mut out = ActionOutcome::default();
    // (A pending Delete whose content is already gone just finishes.)
    if strike && changes.is_empty() && screens.is_empty() && extras.is_empty() {
        return Err("There's nothing to remove in that time range".into());
    }
    let video = has_screen_video(env, meeting_id);
    match action {
        Action::Strike => {
            let now = Utc::now().to_rfc3339();
            let mut screens_in: Vec<usize> = vec![0; ranges.len()];
            for sc in &screens {
                screens_in[range_index(ranges, sc.start)] += 1;
            }
            for (k, (a, b)) in ranges.iter().enumerate() {
                let line_marker = host[k].is_some();
                // The span as selected (whole ms), and never before its start
                let shown_end = (*b - MS_EDGE).max(*a);
                let mut recs = Vec::new();
                if line_marker {
                    let cuts: Vec<usize> = (0..lines.len()).filter(|i| seg_ranges[*i].contains(&k)).collect();
                    let touched: HashSet<i64> = cuts.iter().map(|i| lines[*i].transcript_id).collect();
                    recs.push(RedactionRecord {
                        id: line_rec_ids[k].clone(),
                        meeting_id: meeting_id.to_string(),
                        kind: if cuts.iter().all(|i| lines[*i].whole) { "line" } else { "words" }.into(),
                        action: "strike".into(),
                        media_start: Some(a.to_rfc3339()),
                        media_end: Some(shown_end.to_rfc3339()),
                        created_at: now.clone(),
                        reason: reason.clone(),
                        transcript_id: host[k].map(|i| lines[i].transcript_id),
                        item_count: touched.len() as i64,
                        video_pending: false,
                    });
                }
                if screens_in[k] > 0 || !line_marker {
                    recs.push(RedactionRecord {
                        id: new_id(),
                        meeting_id: meeting_id.to_string(),
                        kind: "screen".into(),
                        action: "strike".into(),
                        media_start: Some(a.to_rfc3339()),
                        media_end: Some(shown_end.to_rfc3339()),
                        created_at: now.clone(),
                        reason: reason.clone(),
                        transcript_id: None,
                        item_count: screens_in[k] as i64,
                        video_pending: false,
                    });
                }
                // The marker that shows "video pending": the screen strip's, if any
                if video {
                    let owner = recs.len() - 1;
                    let queued = video_jobs::enqueue(&mut tx, meeting_id, &[(*a, *b)], Some(recs[owner].id.as_str()))
                        .await
                        .map_err(err("Failed to queue the screen video blanking"))?;
                    out.video_jobs_queued += queued;
                    recs[owner].video_pending = queued > 0;
                }
                for rec in recs {
                    insert_record(&mut tx, &rec, None).await?;
                    out.records.push(rec);
                }
            }
            out.record = out.records.iter().find(|r| r.kind != "screen").or(out.records.first()).cloned();
        }
        Action::Delete => {
            if video {
                out.video_jobs_queued = video_jobs::enqueue(&mut tx, meeting_id, ranges, None)
                    .await
                    .map_err(err("Failed to queue the screen video blanking"))?;
            }
            sqlx::query("DELETE FROM redactions WHERE id = ? AND action = 'delete'")
                .bind(record_id)
                .execute(&mut *tx)
                .await
                .map_err(err("Failed to finish the delete"))?;
        }
    }
    tx.commit().await.map_err(err("Failed to save the edit"))?;

    let file_errors = remove_screen_files(env, meeting_id, &screens);
    let mut jobs = vec![BackupJob::RangeExtras { ranges: ranges.to_vec() }];
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

/// `ranges` are offsets from the meeting start in whole milliseconds, as
/// the timeline shows them (`timestamp_ms`, rounded down). The end covers
/// its whole millisecond, so a screen the timeline puts at 1234 ms (really
/// 1234.6 ms) is inside a range that ends at 1234: the UI and the backend
/// agree on every item at the edge.
async fn resolve(conn: &mut SqliteConnection, meeting_id: &str, ranges: &[MsRange]) -> Result<RangePlan, String> {
    if ranges.iter().any(|r| r.end_ms <= r.start_ms) {
        return Err("The end of the time range must be after its start".into());
    }
    let ms0 = meeting_start(conn, meeting_id).await?;
    let at = |ms: i64| ms0 + chrono::Duration::milliseconds(ms);
    let spans: Vec<Span> = ranges
        .iter()
        .map(|r| (at(r.start_ms), at(r.end_ms) + MS_EDGE))
        .collect();
    plan_ranges(conn, meeting_id, &spans).await
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

fn one(start_ms: i64, end_ms: i64) -> [MsRange; 1] {
    [MsRange { start_ms, end_ms }]
}

/// What a time range removes (for the confirmation). Counts and offsets only.
pub async fn preview_time_range(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<TimeRangePreview, String> {
    preview_time_ranges(pool, env, meeting_id, &one(start_ms, end_ms)).await
}

/// What several time ranges remove together: exact totals per kind.
pub async fn preview_time_ranges(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[MsRange],
) -> Result<TimeRangePreview, String> {
    env.ensure_not_recording(meeting_id)?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let plan = resolve(&mut conn, meeting_id, ranges).await?;
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
    request_delete_time_ranges(pool, env, meeting_id, &one(start_ms, end_ms), expected).await
}

/// Delete several time ranges as one action: one pending row, one undo.
pub async fn request_delete_time_ranges(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[MsRange],
    expected: Option<RangeCounts>,
) -> Result<PendingDelete, String> {
    let _g = LOCK.lock().await;
    env.ensure_not_recording(meeting_id)?;
    flush_overlapping_locked(pool, env, meeting_id, Scope::All).await?;
    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let plan = resolve(&mut conn, meeting_id, ranges).await?;
    check_expected(&plan, expected)?;
    let id = new_id();
    let payload = serde_json::to_string(&PendingPayload::TimeRange {
        start: plan.start.to_rfc3339(),
        end: plan.end.to_rfc3339(),
        screen_ids: plan.screen_ids.clone(),
        lines: plan.lines.clone(),
        ranges: plan.ranges.iter().map(|(a, b)| (a.to_rfc3339(), b.to_rfc3339())).collect(),
    })
    .map_err(err("Failed to queue the delete"))?;
    let kind = if !plan.screen_ids.is_empty() { "screen" } else { "line" };
    let c = plan.counts();
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
        item_count: (c.screens + c.lines_whole + c.lines_split).max(1) as i64,
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
    strike_time_ranges(pool, env, meeting_id, &one(start_ms, end_ms), reason, expected).await
}

/// Strike several time ranges as one action (no undo): markers for each
/// range, as [`strike_time_range`] leaves for one.
pub async fn strike_time_ranges(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[MsRange],
    reason: Option<&str>,
    expected: Option<RangeCounts>,
) -> Result<ActionOutcome, String> {
    let _g = LOCK.lock().await;
    env.ensure_not_recording(meeting_id)?;
    flush_overlapping_locked(pool, env, meeting_id, Scope::All).await?;
    let plan = {
        let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
        resolve(&mut conn, meeting_id, ranges).await?
    };
    check_expected(&plan, expected)?;
    apply_range_locked(
        pool,
        env,
        meeting_id,
        &plan.ranges,
        &plan.screen_ids,
        &plan.lines,
        Action::Strike,
        reason,
        &new_id(),
    )
    .await
}
