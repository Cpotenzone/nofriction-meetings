//! Scope and local retrieval for the chat: which recordings a question may
//! draw on, and the best passages from them (transcript lines by FTS5
//! rank, saved notes and marker notes by term match), packed into a
//! character budget. Everything here reads the app's SQLite database only.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

/// Characters a passage's excerpt may have.
pub const EXCERPT_CHARS: usize = 320;
/// Rough size of a passage's header line in the prompt.
const HEADER_CHARS: usize = 90;
/// Transcript lines fetched by rank before packing.
const FTS_LIMIT: i64 = 40;
/// Passages one recording may take when the scope spans several.
const PER_MEETING: usize = 4;
/// Recordings read for the fallback ("summarize my week") passages.
const RECENT_MEETINGS: i64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    All,
    Notebook,
    Topic,
    Meeting,
}

impl ScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::All => "all",
            ScopeKind::Notebook => "notebook",
            ScopeKind::Topic => "topic",
            ScopeKind::Meeting => "meeting",
        }
    }
    pub fn parse(s: &str) -> Option<ScopeKind> {
        match s.trim().to_lowercase().as_str() {
            "all" => Some(ScopeKind::All),
            "notebook" => Some(ScopeKind::Notebook),
            "topic" => Some(ScopeKind::Topic),
            "meeting" => Some(ScopeKind::Meeting),
            _ => None,
        }
    }
}

/// What a question may draw on. `value`: the Notebook name, the topic key
/// or the meeting id; ignored for `All`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub kind: ScopeKind,
    #[serde(default)]
    pub value: Option<String>,
}

impl Scope {
    pub fn all() -> Scope {
        Scope { kind: ScopeKind::All, value: None }
    }
    pub fn meeting(id: &str) -> Scope {
        Scope { kind: ScopeKind::Meeting, value: Some(id.to_string()) }
    }
    pub fn notebook(name: &str) -> Scope {
        Scope { kind: ScopeKind::Notebook, value: Some(name.to_string()) }
    }
    pub fn topic(key: &str) -> Scope {
        Scope { kind: ScopeKind::Topic, value: Some(key.to_string()) }
    }
    /// A stored scope; anything unknown reads as All.
    pub fn from_stored(kind: &str, value: Option<String>) -> Scope {
        match ScopeKind::parse(kind) {
            Some(ScopeKind::All) | None => Scope::all(),
            Some(k) => Scope { kind: k, value: value.filter(|v| !v.trim().is_empty()) },
        }
    }
    fn value(&self) -> Result<String, String> {
        self.value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
            .ok_or_else(|| format!("Pick a {} to ask about.", self.kind.as_str()))
    }
}

/// The SQL filter a scope resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    All,
    Notebook(String),
    Topic(String),
    Meeting(String),
}

impl Filter {
    /// " AND <col> IN (…)" with one `?` for the value, or "" for All. Put
    /// it last in the query and bind `value()` last.
    pub fn clause(&self, col: &str) -> String {
        match self {
            Filter::All => String::new(),
            Filter::Notebook(_) => format!(" AND {} IN (SELECT id FROM meetings WHERE class_name = ? COLLATE NOCASE)", col),
            Filter::Topic(_) => format!(" AND {} IN (SELECT meeting_id FROM meeting_topics WHERE topic_key = ?)", col),
            Filter::Meeting(_) => format!(" AND {} = ?", col),
        }
    }
    pub fn value(&self) -> Option<&str> {
        match self {
            Filter::All => None,
            Filter::Notebook(v) | Filter::Topic(v) | Filter::Meeting(v) => Some(v),
        }
    }
    pub fn single(&self) -> bool {
        matches!(self, Filter::Meeting(_))
    }
}

/// A resolved scope: its label (shown with every answer), how many
/// recordings it covers, and its filter.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeInfo {
    pub scope: Scope,
    pub label: String,
    pub count: i64,
    pub filter: Filter,
}

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

async fn count_meetings(conn: &mut SqliteConnection, filter: &Filter) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM meetings WHERE 1 = 1{}", filter.clause("id"));
    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    if let Some(v) = filter.value() {
        q = q.bind(v.to_string());
    }
    q.fetch_one(&mut *conn).await.map_err(err("Database busy"))
}

pub async fn resolve(conn: &mut SqliteConnection, scope: &Scope) -> Result<ScopeInfo, String> {
    let (label, filter) = match scope.kind {
        ScopeKind::All => ("All recordings".to_string(), Filter::All),
        ScopeKind::Notebook => {
            let v = scope.value()?;
            (format!("Notebook · {}", v), Filter::Notebook(v))
        }
        ScopeKind::Topic => {
            let key = scope.value()?;
            let label: Option<String> = sqlx::query_scalar(
                "SELECT topic FROM meeting_topics WHERE topic_key = ? GROUP BY topic ORDER BY COUNT(*) DESC, topic ASC LIMIT 1",
            )
            .bind(&key)
            .fetch_optional(&mut *conn)
            .await
            .map_err(err("Database busy"))?;
            (format!("Topic · {}", label.unwrap_or_else(|| key.clone())), Filter::Topic(key))
        }
        ScopeKind::Meeting => {
            let id = scope.value()?;
            let title: Option<String> = sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?")
                .bind(&id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(err("Database busy"))?;
            let title = title.ok_or("That recording no longer exists")?;
            (format!("Recording · {}", title.trim()), Filter::Meeting(id))
        }
    };
    let count = count_meetings(conn, &filter).await?;
    Ok(ScopeInfo { scope: scope.clone(), label, count, filter })
}

/// What a scope covers, for the picker and the suggested questions.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ScopeSummary {
    pub label: String,
    pub count: i64,
    /// Newest first, at most 6
    pub recent_titles: Vec<String>,
    /// Most used first, at most 8
    pub topics: Vec<String>,
    /// Notebooks of the scoped recordings, at most 6
    pub notebooks: Vec<String>,
    /// "meeting" | "class" | "personal" present in the scope
    pub kinds: Vec<String>,
}

pub async fn summary(conn: &mut SqliteConnection, scope: &Scope) -> Result<ScopeSummary, String> {
    let info = resolve(conn, scope).await?;
    let sql = format!(
        "SELECT title, recording_kind, class_name FROM meetings WHERE 1 = 1{} ORDER BY started_at DESC LIMIT 30",
        info.filter.clause("id")
    );
    let mut q = sqlx::query(&sql);
    if let Some(v) = info.filter.value() {
        q = q.bind(v.to_string());
    }
    let rows = q.fetch_all(&mut *conn).await.map_err(err("Database busy"))?;
    let mut recent_titles = Vec::new();
    let mut notebooks: Vec<String> = Vec::new();
    let mut kinds: Vec<String> = Vec::new();
    for r in &rows {
        let title: String = r.get("title");
        if recent_titles.len() < 6 && !title.trim().is_empty() {
            recent_titles.push(title.trim().to_string());
        }
        if let Some(nb) = r.get::<Option<String>, _>("class_name").filter(|n| !n.trim().is_empty()) {
            if notebooks.len() < 6 && !notebooks.iter().any(|x| x.eq_ignore_ascii_case(&nb)) {
                notebooks.push(nb);
            }
        }
        let kind = crate::recording_kind::RecordingKind::from_stored(r.get::<Option<String>, _>("recording_kind").as_deref())
            .as_str()
            .to_string();
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let tsql = format!(
        "SELECT topic, COUNT(*) AS n FROM meeting_topics WHERE 1 = 1{} GROUP BY topic_key, topic ORDER BY n DESC, topic ASC LIMIT 8",
        info.filter.clause("meeting_id")
    );
    let mut tq = sqlx::query(&tsql);
    if let Some(v) = info.filter.value() {
        tq = tq.bind(v.to_string());
    }
    let topics = tq
        .fetch_all(&mut *conn)
        .await
        .map_err(err("Database busy"))?
        .iter()
        .map(|r| r.get::<String, _>("topic"))
        .collect();
    Ok(ScopeSummary { label: info.label, count: info.count, recent_titles, topics, notebooks, kinds })
}

// ═══════════════════════════════════════════════════════════════════════════
// Passages
// ═══════════════════════════════════════════════════════════════════════════

/// One passage sent to the model and offered as a citation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Passage {
    /// 1-based number the answer cites
    pub n: usize,
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    /// "meeting" | "class" | "personal"
    pub kind: String,
    pub notebook: Option<String>,
    /// "transcript" | "notes" | "marker"
    pub source: &'static str,
    /// ms from the recording's start; None for notes
    pub timestamp_ms: Option<i64>,
    pub excerpt: String,
    pub score: f64,
}

impl Passage {
    /// Characters it takes in the prompt.
    pub fn cost(&self) -> usize {
        self.excerpt.chars().count() + HEADER_CHARS
    }
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn offset_ms(start: &str, at: &str) -> Option<i64> {
    Some((parse_ts(at)? - parse_ts(start)?).num_milliseconds().max(0))
}

/// Trimmed, one line, at most `max` characters.
pub fn clip(text: &str, max: usize) -> String {
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= max {
        one
    } else {
        let cut: String = one.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Share of `terms` found in `text` (case-insensitive), 0 to 1.
pub fn term_score(text: &str, terms: &[String]) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let lower = text.to_lowercase();
    let hits = terms.iter().filter(|t| lower.contains(t.as_str())).count();
    hits as f64 / terms.len() as f64
}

struct MeetingHead {
    id: String,
    title: String,
    started_at: String,
    kind: String,
    notebook: Option<String>,
}

fn head_from_row(r: &sqlx::sqlite::SqliteRow, id_col: &str) -> MeetingHead {
    MeetingHead {
        id: r.get(id_col),
        title: r.get("title"),
        started_at: r.get("started_at"),
        kind: crate::recording_kind::RecordingKind::from_stored(r.get::<Option<String>, _>("recording_kind").as_deref())
            .as_str()
            .to_string(),
        notebook: r.get::<Option<String>, _>("class_name").filter(|n| !n.trim().is_empty()),
    }
}

fn passage(h: &MeetingHead, source: &'static str, timestamp_ms: Option<i64>, excerpt: String, score: f64) -> Passage {
    Passage {
        n: 0,
        meeting_id: h.id.clone(),
        title: h.title.clone(),
        started_at: h.started_at.clone(),
        kind: h.kind.clone(),
        notebook: h.notebook.clone(),
        source,
        timestamp_ms,
        excerpt,
        score,
    }
}

fn usable_line(text: &str) -> Option<String> {
    let plain = crate::redaction::render_plain(text);
    let t = plain.trim();
    if t.is_empty() || t == crate::redaction::STRICKEN_PLACEHOLDER {
        return None;
    }
    Some(clip(t, EXCERPT_CHARS))
}

async fn transcript_hits(conn: &mut SqliteConnection, filter: &Filter, question: &str) -> Result<Vec<Passage>, String> {
    let Some(fts) = crate::database::fts_or_query(question) else { return Ok(Vec::new()) };
    let sql = format!(
        "SELECT t.meeting_id, m.title, m.started_at, m.recording_kind, m.class_name, t.timestamp, t.text, \
         bm25(transcripts_fts) AS rank \
         FROM transcripts_fts JOIN transcripts t ON transcripts_fts.rowid = t.id JOIN meetings m ON m.id = t.meeting_id \
         WHERE transcripts_fts MATCH ?{} ORDER BY rank LIMIT ?",
        filter.clause("t.meeting_id")
    );
    let mut q = sqlx::query(&sql).bind(fts);
    if let Some(v) = filter.value() {
        q = q.bind(v.to_string());
    }
    let rows = q.bind(FTS_LIMIT).fetch_all(&mut *conn).await.map_err(err("Couldn't search the transcripts"))?;
    let mut out = Vec::new();
    for r in rows {
        let Some(excerpt) = usable_line(&r.get::<String, _>("text")) else { continue };
        let h = head_from_row(&r, "meeting_id");
        let rank: f64 = r.get("rank");
        let ts = offset_ms(&h.started_at, &r.get::<String, _>("timestamp"));
        // bm25 is negative, and more negative is better; |rank| / (1 + |rank|)
        // keeps that order on a 0..1 scale beside the term scores
        let r = rank.abs();
        out.push(passage(&h, "transcript", ts, excerpt, r / (1.0 + r)));
    }
    Ok(out)
}

/// Texts in a notes JSON list: strings, or objects' "text" / "task".
fn note_items(json: Option<String>) -> Vec<String> {
    let Some(j) = json else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&j) else { return Vec::new() };
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|x| match x {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => o
                .get("text")
                .or_else(|| o.get("task"))
                .or_else(|| o.get("term"))
                .and_then(|t| t.as_str())
                .map(|s| {
                    let who = o.get("assignee").or_else(|| o.get("made_by")).and_then(|a| a.as_str()).filter(|a| !a.trim().is_empty());
                    match who {
                        Some(w) => format!("{} ({})", s, w),
                        None => s.to_string(),
                    }
                }),
            _ => None,
        })
        .filter(|s| !s.trim().is_empty())
        .collect()
}

async fn notes_hits(conn: &mut SqliteConnection, filter: &Filter, terms: &[String]) -> Result<Vec<Passage>, String> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT n.meeting_id, n.summary, n.key_topics, n.decisions, n.action_items, m.title, m.started_at, \
         m.recording_kind, m.class_name FROM meeting_notes n JOIN meetings m ON m.id = n.meeting_id \
         WHERE 1 = 1{} ORDER BY n.generated_at DESC LIMIT 60",
        filter.clause("n.meeting_id")
    );
    let mut q = sqlx::query(&sql);
    if let Some(v) = filter.value() {
        q = q.bind(v.to_string());
    }
    let rows = q.fetch_all(&mut *conn).await.map_err(err("Couldn't read the notes"))?;
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for r in rows {
        let h = head_from_row(&r, "meeting_id");
        if seen.contains(&h.id) {
            continue; // only the newest notes of a recording
        }
        seen.push(h.id.clone());
        let mut candidates: Vec<String> = Vec::new();
        if let Some(s) = r.get::<Option<String>, _>("summary").filter(|s| !s.trim().is_empty()) {
            candidates.push(format!("Summary: {}", s));
        }
        for d in note_items(r.get("decisions")) {
            candidates.push(format!("Decision: {}", d));
        }
        for a in note_items(r.get("action_items")) {
            candidates.push(format!("Action item: {}", a));
        }
        let topics = note_items(r.get("key_topics"));
        if !topics.is_empty() {
            candidates.push(format!("Key points: {}", topics.join("; ")));
        }
        for c in candidates {
            let score = term_score(&c, terms);
            if score > 0.0 {
                out.push(passage(&h, "notes", None, clip(&c, EXCERPT_CHARS), score * 0.9));
            }
        }
    }
    Ok(out)
}

async fn marker_hits(conn: &mut SqliteConnection, filter: &Filter, terms: &[String]) -> Result<Vec<Passage>, String> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT k.meeting_id, k.ts, k.kind, k.note, m.title, m.started_at, m.recording_kind, m.class_name \
         FROM meeting_markers k JOIN meetings m ON m.id = k.meeting_id \
         WHERE k.note IS NOT NULL AND TRIM(k.note) != ''{} ORDER BY k.ts DESC LIMIT 200",
        filter.clause("k.meeting_id")
    );
    let mut q = sqlx::query(&sql);
    if let Some(v) = filter.value() {
        q = q.bind(v.to_string());
    }
    let rows = q.fetch_all(&mut *conn).await.map_err(err("Couldn't read the markers"))?;
    let mut out = Vec::new();
    for r in rows {
        let h = head_from_row(&r, "meeting_id");
        let note: String = r.get::<Option<String>, _>("note").unwrap_or_default();
        let score = term_score(&note, terms);
        if score == 0.0 {
            continue;
        }
        let kind: String = r.get("kind");
        let rec = crate::recording_kind::RecordingKind::from_stored(Some(&h.kind));
        let text = format!("{} {}: {}", crate::markers::symbol(&kind), crate::markers::label(&kind, rec), note.trim());
        let ts = offset_ms(&h.started_at, &r.get::<String, _>("ts"));
        out.push(passage(&h, "marker", ts, clip(&text, EXCERPT_CHARS), score * 0.85));
    }
    Ok(out)
}

/// Passages for a question the index can't match ("Summarize my week"):
/// the newest scoped recordings' notes summary, or lines sampled across
/// their transcript. One recording gets more lines than several.
async fn recent_passages(conn: &mut SqliteConnection, filter: &Filter) -> Result<Vec<Passage>, String> {
    let limit = if filter.single() { 1 } else { RECENT_MEETINGS };
    let sql = format!(
        "SELECT id, title, started_at, recording_kind, class_name FROM meetings WHERE 1 = 1{} ORDER BY started_at DESC LIMIT ?",
        filter.clause("id")
    );
    let mut q = sqlx::query(&sql);
    if let Some(v) = filter.value() {
        q = q.bind(v.to_string());
    }
    let rows = q.bind(limit).fetch_all(&mut *conn).await.map_err(err("Database busy"))?;
    let lines_per = if filter.single() { 12 } else { 3 };
    let mut out = Vec::new();
    for r in rows {
        let h = head_from_row(&r, "id");
        let summary: Option<String> = sqlx::query_scalar(
            "SELECT summary FROM meeting_notes WHERE meeting_id = ? AND summary IS NOT NULL AND TRIM(summary) != '' ORDER BY generated_at DESC LIMIT 1",
        )
        .bind(&h.id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read the notes"))?;
        if let Some(s) = summary {
            out.push(passage(&h, "notes", None, clip(&format!("Summary: {}", s), EXCERPT_CHARS), 0.3));
            if !filter.single() {
                continue;
            }
        }
        let lines = sqlx::query("SELECT timestamp, text FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC, id ASC")
            .bind(&h.id)
            .fetch_all(&mut *conn)
            .await
            .map_err(err("Couldn't read the transcript"))?;
        let usable: Vec<(String, String)> = lines
            .iter()
            .filter_map(|l| usable_line(&l.get::<String, _>("text")).map(|t| (l.get::<String, _>("timestamp"), t)))
            .collect();
        if usable.is_empty() {
            continue;
        }
        let step = (usable.len() as f64 / lines_per as f64).max(1.0);
        let mut i = 0.0;
        while (i as usize) < usable.len() && out.len() < 64 {
            let (ts, text) = &usable[i as usize];
            out.push(passage(&h, "transcript", offset_ms(&h.started_at, ts), text.clone(), 0.2));
            i += step;
        }
    }
    Ok(out)
}

/// The best passages for `question` within the scope, at most `max`, within
/// `char_budget` characters of prompt text, numbered from 1 in rank order.
pub async fn retrieve(
    conn: &mut SqliteConnection,
    info: &ScopeInfo,
    question: &str,
    char_budget: usize,
    max: usize,
) -> Result<Vec<Passage>, String> {
    let terms = crate::database::search_terms(question);
    let mut all = transcript_hits(conn, &info.filter, question).await?;
    all.extend(notes_hits(conn, &info.filter, &terms).await?);
    all.extend(marker_hits(conn, &info.filter, &terms).await?);
    if all.len() < 3 {
        all.extend(recent_passages(conn, &info.filter).await?);
    }
    Ok(pack(all, char_budget, max, if info.filter.single() { max } else { PER_MEETING }))
}

/// Rank, cap per recording, fit the budget, number.
pub fn pack(mut all: Vec<Passage>, char_budget: usize, max: usize, per_meeting: usize) -> Vec<Passage> {
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<Passage> = Vec::new();
    let mut used = 0usize;
    for p in all {
        if out.len() >= max {
            break;
        }
        if out.iter().filter(|o| o.meeting_id == p.meeting_id).count() >= per_meeting {
            continue;
        }
        if out.iter().any(|o| o.meeting_id == p.meeting_id && o.excerpt == p.excerpt) {
            continue;
        }
        let cost = p.cost();
        if used + cost > char_budget {
            if out.is_empty() && cost <= char_budget.saturating_add(EXCERPT_CHARS) {
                // One passage always fits: a tiny window still gets the best line
            } else {
                continue;
            }
        }
        used += cost;
        out.push(p);
    }
    for (i, p) in out.iter_mut().enumerate() {
        p.n = i + 1;
    }
    out
}
