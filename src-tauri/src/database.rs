// noFriction Meetings - Database Manager
// SQLite storage for meetings, transcripts, and full-text search

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Row, Sqlite};
use std::path::Path;

/// Meeting record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub calendar_event_id: Option<String>,
    /// Planned length in minutes (timed recording); None = no limit
    #[serde(default)]
    pub planned_minutes: Option<i64>,
    /// The recording's notebook (notebooks.rs). The column keeps its old
    /// name `class_name`; None = no notebook
    #[serde(default)]
    pub class_name: Option<String>,
    /// "meeting" | "class" | "personal" (recording_kind.rs); a NULL column
    /// reads as "meeting"
    #[serde(default = "default_recording_kind")]
    pub recording_kind: String,
}

fn default_recording_kind() -> String {
    crate::recording_kind::RecordingKind::default().as_str().to_string()
}

/// Columns read into [`Meeting`] by [`meeting_from_row`].
pub(crate) const MEETING_COLUMNS: &str =
    "id, title, started_at, ended_at, duration_seconds, planned_minutes, class_name, recording_kind";

/// A `meetings` row selected with [`MEETING_COLUMNS`].
pub(crate) fn meeting_from_row(r: &sqlx::sqlite::SqliteRow) -> Meeting {
    Meeting {
        id: r.get("id"),
        title: r.get("title"),
        started_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at"))
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        ended_at: r
            .get::<Option<String>, _>("ended_at")
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        duration_seconds: r.get("duration_seconds"),
        calendar_event_id: None,
        planned_minutes: r.get("planned_minutes"),
        class_name: r.get("class_name"),
        recording_kind: crate::recording_kind::RecordingKind::from_stored(
            r.get::<Option<String>, _>("recording_kind").as_deref(),
        )
        .as_str()
        .to_string(),
    }
}

/// Meeting attendee record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingAttendee {
    pub id: i64,
    pub meeting_id: String,
    pub name: String,
    pub email: String,
    pub company: Option<String>,
    pub role: String,
}

/// Transcript record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: i64,
    pub meeting_id: String,
    pub text: String,
    pub speaker: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub is_final: bool,
    pub confidence: f32,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub meeting_id: String,
    pub meeting_title: String,
    pub transcript_text: String,
    pub timestamp: DateTime<Utc>,
    pub relevance: f64,
}

/// A ranked transcript match used as chat ("ask your meetings") context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptContextHit {
    pub transcript_id: i64,
    pub meeting_id: String,
    pub meeting_title: String,
    pub meeting_started_at: String,
    pub timestamp: String,
    pub speaker: Option<String>,
    pub snippet: String,
    /// bm25 score: lower (more negative) is more relevant
    pub relevance: f64,
}

/// Words too common to be useful search terms.
const SEARCH_STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "had", "her", "was",
    "one", "our", "out", "has", "have", "him", "his", "how", "its", "may", "who", "did", "get",
    "what", "when", "where", "which", "why", "with", "this", "that", "they", "them", "then",
    "there", "their", "from", "about", "into", "would", "could", "should", "been", "were",
    "will", "your", "does", "tell", "said", "say",
];

/// Meaningful lowercase search terms from free text (max 8, deduplicated).
pub fn search_terms(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric()) {
        let w = w.to_lowercase();
        if w.chars().count() > 2 && !SEARCH_STOPWORDS.contains(&w.as_str()) && !out.contains(&w) {
            out.push(w);
            if out.len() == 8 {
                break;
            }
        }
    }
    out
}

/// A safe FTS5 query (quoted terms OR-ed together) from free text, or None
/// if the text has no usable terms. Raw user text can't go to MATCH: FTS5
/// treats punctuation and words like AND/NEAR as syntax.
pub fn fts_or_query(text: &str) -> Option<String> {
    let terms = search_terms(text);
    if terms.is_empty() {
        None
    } else {
        Some(
            terms
                .iter()
                .map(|w| format!("\"{}\"", w))
                .collect::<Vec<_>>()
                .join(" OR "),
        )
    }
}

/// AI-generated meeting notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingNotes {
    pub id: String,
    pub meeting_id: String,
    pub summary: Option<String>,
    pub key_topics: Option<String>,
    pub decisions: Option<String>,
    pub action_items: Option<String>,
    pub participants: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub model_used: Option<String>,
    /// Generated before the transcript/screens were edited: offer "regenerate?"
    #[serde(default)]
    pub stale_after_edit: bool,
}

/// User comment on a meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingComment {
    pub id: String,
    pub meeting_id: String,
    pub user_id: Option<String>,
    pub comment: String,
    pub comment_type: String,
    pub timestamp_ref: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub parent_id: Option<String>,
}

/// Study materials record (Dork Mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyMaterialsRecord {
    pub id: String,
    pub meeting_id: String,
    pub summary: Option<String>,
    pub key_concepts: Option<String>,
    pub quiz_questions: Option<String>,
    pub flashcards: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub model_used: Option<String>,
}

/// Normalized-text fingerprint used by the 30s echo dedupe. Recomputed
/// whenever a line is edited so it never fingerprints removed words.
pub fn transcript_text_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let normalized_text = text.trim().to_lowercase();
    let mut hasher = DefaultHasher::new();
    normalized_text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Add each column a table is missing. SQLite has no
/// `ADD COLUMN IF NOT EXISTS`, and `CREATE TABLE IF NOT EXISTS` never alters
/// a table an older version created, so every column added after a table
/// first shipped must go through here (checked with `pragma_table_info`).
/// Unlike the old `let _ = ALTER ...` pattern, a real failure is returned,
/// not swallowed. `columns` are `(name, declaration)` pairs. Returns the
/// names that were added. Runs on the caller's connection (migrations use
/// exactly one, see [`DatabaseManager::run_migrations`]).
pub async fn ensure_columns(
    conn: &mut sqlx::SqliteConnection,
    table: &str,
    columns: &[(&str, &str)],
) -> Result<Vec<String>, sqlx::Error> {
    let existing: std::collections::HashSet<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
    if existing.is_empty() {
        return Err(sqlx::Error::Protocol(format!(
            "ensure_columns: table {} does not exist",
            table
        )));
    }
    let mut added = Vec::new();
    for (name, decl) in columns {
        if !existing.contains(*name) {
            sqlx::query(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, name, decl))
                .execute(&mut *conn)
                .await?;
            added.push(name.to_string());
        }
    }
    Ok(added)
}

/// Database manager
pub struct DatabaseManager {
    pool: Pool<Sqlite>,
}

impl DatabaseManager {
    /// Get a reference to the connection pool
    pub fn get_pool(&self) -> std::sync::Arc<Pool<Sqlite>> {
        std::sync::Arc::new(self.pool.clone())
    }

    /// A manager over an existing pool (sync applies a deleted recording
    /// through `delete_meeting`)
    pub fn from_pool(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Borrow the pool (redaction runs its own transactions on it)
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Create a new database manager
    pub async fn new(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        // WAL lets the UI read while transcripts and screenshots are being
        // written (the old rollback journal made readers wait on writers);
        // busy_timeout rides out brief write contention instead of erroring.
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(10))
            // Overwrite deleted content instead of leaving it in free pages
            // (docs/REDACTION.md, purge step 8). Applies to every pooled
            // connection; costs extra writes only when rows are deleted.
            .pragma("secure_delete", "ON");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Every migration runs on ONE connection. The pool releases
        // connections asynchronously, so back-to-back statements on
        // `&self.pool` open extra connections mid-migration, and one of those
        // can keep a stale schema afterwards ("no such table: transcripts" on
        // a fresh install). Pool connections opened after this see the full
        // schema. Reproduced ~30% under parallel tests; 0% with this.
        let mut conn = self.pool.acquire().await?;
        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transcripts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                text TEXT NOT NULL,
                speaker TEXT,
                timestamp TEXT NOT NULL,
                is_final INTEGER NOT NULL DEFAULT 1,
                confidence REAL NOT NULL DEFAULT 0.0
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS frames (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                frame_number INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL,
                file_path TEXT,
                ocr_text TEXT
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Columns added after these tables first shipped
        ensure_columns(&mut conn, "transcripts", &[("text_hash", "TEXT")]).await?;
        ensure_columns(
            &mut conn,
            "frames",
            &[("frame_number", "INTEGER DEFAULT 0"), ("file_path", "TEXT")],
        )
        .await?;

        // Lookup index for the 30s echo-dedupe window in add_transcript_at.
        // Deliberately NOT unique: people legitimately repeat short phrases
        // ("Yeah.", "Okay.") across a meeting, and a unique index made every
        // later repeat fail to persist.
        let _ = sqlx::query("DROP INDEX IF EXISTS idx_transcripts_hash")
            .execute(&mut *conn)
            .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_hash ON transcripts(meeting_id, text_hash)",
        )
        .execute(&mut *conn)
        .await;


        // Create full-text search virtual tables
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts 
            USING fts5(text, meeting_id, content='transcripts', content_rowid='id')
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Create triggers to keep FTS in sync
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS transcripts_ai AFTER INSERT ON transcripts BEGIN
                INSERT INTO transcripts_fts(rowid, text, meeting_id) 
                VALUES (new.id, new.text, new.meeting_id);
            END
        "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS transcripts_ad AFTER DELETE ON transcripts BEGIN
                INSERT INTO transcripts_fts(transcripts_fts, rowid, text, meeting_id) 
                VALUES ('delete', old.id, old.text, old.meeting_id);
            END
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Keep FTS in sync when a line is edited (word delete / strike).
        // Every UPDATE of transcripts.text must go through this trigger.
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS transcripts_au AFTER UPDATE OF text, meeting_id ON transcripts BEGIN
                INSERT INTO transcripts_fts(transcripts_fts, rowid, text, meeting_id)
                VALUES ('delete', old.id, old.text, old.meeting_id);
                INSERT INTO transcripts_fts(rowid, text, meeting_id)
                VALUES (new.id, new.text, new.meeting_id);
            END
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Word timings (JSON, nullable): [{"s":utf16_start,"e":utf16_end,"t0":ms,"t1":ms}]
        // relative to the line's text and timestamp. Offsets only, never words.
        ensure_columns(&mut conn, "transcripts", &[("word_timings", "TEXT")]).await?;

        // Create indexes
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id)
        "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings(started_at)
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Knowledge Base tables - frame_queue for VLM analysis
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS frame_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                frame_path TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                analyzed INTEGER NOT NULL DEFAULT 0,
                synced INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Knowledge Base tables - activity_log for analyzed activities
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS activity_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_seconds INTEGER,
                app_name TEXT,
                window_title TEXT,
                category TEXT NOT NULL DEFAULT 'other',
                summary TEXT NOT NULL,
                focus_area TEXT,
                visible_files TEXT,
                confidence REAL DEFAULT 0.0,
                frame_ids TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // Indexes for new tables
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_frame_queue_analyzed ON frame_queue(analyzed)",
        )
        .execute(&mut *conn)
        .await;
        let _ =
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_frame_queue_synced ON frame_queue(synced)")
                .execute(&mut *conn)
                .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_activity_log_start ON activity_log(start_time)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_activity_log_category ON activity_log(category)",
        )
        .execute(&mut *conn)
        .await;

        // Theme activity tracking table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS theme_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                theme TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_theme_sessions_theme ON theme_sessions(theme)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_theme_sessions_started ON theme_sessions(started_at)",
        )
        .execute(&mut *conn)
        .await;

        // Phase 3: Entities table for structured entity extraction
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                activity_id INTEGER NOT NULL,
                entity_type TEXT NOT NULL,
                name TEXT NOT NULL,
                metadata TEXT,
                confidence REAL DEFAULT 0.5,
                theme TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (activity_id) REFERENCES activity_log(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ =
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)")
                .execute(&mut *conn)
                .await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_theme ON entities(theme)")
            .execute(&mut *conn)
            .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_entities_activity ON entities(activity_id)",
        )
        .execute(&mut *conn)
        .await;

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 1: Stateful Screen Ingest - ScreenStates table
        // ═══════════════════════════════════════════════════════════════════════
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS screen_states (
                state_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                app_name TEXT,
                window_title TEXT,
                phash TEXT NOT NULL,
                delta_score REAL DEFAULT 0.0,
                keyframe_path TEXT,
                state_type TEXT DEFAULT 'other',
                flags TEXT DEFAULT '{}',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_screen_states_meeting ON screen_states(meeting_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_screen_states_start ON screen_states(start_ts)",
        )
        .execute(&mut *conn)
        .await;
        // Which display/window a state came from (multi-source capture)
        ensure_columns(&mut conn, "screen_states", &[("source_key", "TEXT")]).await?;

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 2: Stateful Screen Ingest - Episodes & Text Snapshots
        // ═══════════════════════════════════════════════════════════════════════

        // DocumentEpisode: Continuous focus on app/window
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS document_episodes (
                episode_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                app_name TEXT,
                window_title TEXT,
                document_fingerprint TEXT,
                state_count INTEGER DEFAULT 0,
                total_duration_ms INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_episodes_meeting ON document_episodes(meeting_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_episodes_app ON document_episodes(app_name)",
        )
        .execute(&mut *conn)
        .await;

        // Episode-State junction table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS episode_states (
                episode_id TEXT NOT NULL,
                state_id TEXT NOT NULL,
                sequence_num INTEGER DEFAULT 0,
                PRIMARY KEY (episode_id, state_id),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE,
                FOREIGN KEY (state_id) REFERENCES screen_states(state_id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // TextSnapshot: Text at meaningful boundaries
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS text_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                episode_id TEXT,
                state_id TEXT,
                meeting_id TEXT,
                ts TEXT NOT NULL,
                text TEXT NOT NULL,
                text_hash TEXT NOT NULL,
                quality_score REAL DEFAULT 0.0,
                source TEXT DEFAULT 'ocr',
                word_count INTEGER DEFAULT 0,
                app_name TEXT,
                window_title TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE,
                FOREIGN KEY (state_id) REFERENCES screen_states(state_id) ON DELETE CASCADE,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        // meeting_id / app_name / window_title were added after text_snapshots
        // first shipped. Databases created before that kept the old table
        // (CREATE TABLE IF NOT EXISTS never alters it), so every snapshot
        // insert and the screen purge failed there with "no such column:
        // meeting_id". Add them and backfill what the old rows imply, in one
        // transaction so a crash can't leave the columns half-filled.
        {
            let mut tx = sqlx::Connection::begin(&mut *conn).await?;
            let added = ensure_columns(
                &mut tx,
                "text_snapshots",
                &[
                    ("meeting_id", "TEXT REFERENCES meetings(id) ON DELETE CASCADE"),
                    ("app_name", "TEXT"),
                    ("window_title", "TEXT"),
                ],
            )
            .await?;
            if added.iter().any(|c| c == "meeting_id") {
                sqlx::query(
                    "UPDATE text_snapshots SET meeting_id = \
                     (SELECT s.meeting_id FROM screen_states s WHERE s.state_id = text_snapshots.state_id) \
                     WHERE meeting_id IS NULL AND state_id IS NOT NULL",
                )
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE text_snapshots SET meeting_id = \
                     (SELECT e.meeting_id FROM document_episodes e WHERE e.episode_id = text_snapshots.episode_id) \
                     WHERE meeting_id IS NULL AND episode_id IS NOT NULL",
                )
                .execute(&mut *tx)
                .await?;
            }
            if added.iter().any(|c| c == "app_name" || c == "window_title") {
                for col in ["app_name", "window_title"] {
                    sqlx::query(&format!(
                        "UPDATE text_snapshots SET {0} = COALESCE(\
                         (SELECT s.{0} FROM screen_states s WHERE s.state_id = text_snapshots.state_id), \
                         (SELECT e.{0} FROM document_episodes e WHERE e.episode_id = text_snapshots.episode_id)) \
                         WHERE {0} IS NULL",
                        col
                    ))
                    .execute(&mut *tx)
                    .await?;
                }
            }
            tx.commit().await?;
        }

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_episode ON text_snapshots(episode_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_hash ON text_snapshots(text_hash)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_meeting ON text_snapshots(meeting_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_snapshots_ts ON text_snapshots(ts)")
            .execute(&mut *conn)
            .await;

        // TextPatch: Diff between snapshots
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS text_patches (
                patch_id TEXT PRIMARY KEY,
                episode_id TEXT NOT NULL,
                from_snapshot_id TEXT,
                to_snapshot_id TEXT,
                from_text_hash TEXT NOT NULL,
                to_text_hash TEXT NOT NULL,
                ts TEXT NOT NULL,
                unified_diff TEXT NOT NULL,
                lines_added INTEGER DEFAULT 0,
                lines_removed INTEGER DEFAULT 0,
                change_summary TEXT,
                change_type TEXT DEFAULT 'content_changed',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_patches_episode ON text_patches(episode_id)",
        )
        .execute(&mut *conn)
        .await;

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 3: Stateful Screen Ingest - Timeline Events
        // ═══════════════════════════════════════════════════════════════════════

        // MeetingTimelineEvent: Timeline events for meetings
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meeting_timeline_events (
                event_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                ts TEXT NOT NULL,
                event_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                app_name TEXT,
                window_title TEXT,
                duration_ms INTEGER,
                episode_id TEXT,
                state_id TEXT,
                topic TEXT,
                importance REAL DEFAULT 0.5,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE SET NULL
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_timeline_meeting ON meeting_timeline_events(meeting_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_timeline_ts ON meeting_timeline_events(ts)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_timeline_topic ON meeting_timeline_events(topic)",
        )
        .execute(&mut *conn)
        .await;

        // TopicCluster: Topic groupings for meetings
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS topic_clusters (
                topic_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                event_count INTEGER DEFAULT 0,
                total_duration_ms INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_topics_meeting ON topic_clusters(meeting_id)",
        )
        .execute(&mut *conn)
        .await;

        // ═══════════════════════════════════════════════════════════════════════
        // v2.1.0: Management Suite - Audit Log & Data Versioning
        // ═══════════════════════════════════════════════════════════════════════

        // Audit log for all admin actions
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                details TEXT,
                bytes_affected INTEGER DEFAULT 0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action)")
            .execute(&mut *conn)
            .await;
        let _ =
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp)")
                .execute(&mut *conn)
                .await;

        // Versioned edits for learned data
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS data_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                field_name TEXT NOT NULL,
                previous_value TEXT,
                new_value TEXT,
                diff TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_versions_entity ON data_versions(entity_type, entity_id)",
        )
        .execute(&mut *conn)
        .await;

        // ═══════════════════════════════════════════════════════════════════════
        // v2.2.0: Meeting Intelligence System
        // ═══════════════════════════════════════════════════════════════════════

        // AI-generated meeting notes
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meeting_notes (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                summary TEXT,
                key_topics TEXT,       -- JSON array of topics
                decisions TEXT,        -- JSON array of decisions
                action_items TEXT,     -- JSON array of action items
                participants TEXT,     -- JSON array of detected participants
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                model_used TEXT,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_meeting_notes_meeting ON meeting_notes(meeting_id)",
        )
        .execute(&mut *conn)
        .await;

        // User comments and annotations on meetings
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meeting_comments (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                user_id TEXT,
                comment TEXT NOT NULL,
                comment_type TEXT DEFAULT 'note',  -- 'note', 'decision', 'action', 'question'
                timestamp_ref REAL,                -- Optional: reference to transcript timestamp
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT,
                parent_id TEXT,                    -- For threaded comments
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_id) REFERENCES meeting_comments(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_meeting_comments_meeting ON meeting_comments(meeting_id)",
        )
        .execute(&mut *conn)
        .await;

        // Study materials (Dork Mode output)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS study_materials (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                summary TEXT,
                key_concepts TEXT,     -- JSON array of {term, definition}
                quiz_questions TEXT,   -- JSON array of quiz questions
                flashcards TEXT,       -- JSON array of flashcards
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                model_used TEXT,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_study_materials_meeting ON study_materials(meeting_id)",
        )
        .execute(&mut *conn)
        .await;

        // Transcript clusters for grouping segments into logical meetings
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transcript_clusters (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                cluster_name TEXT,
                start_time TEXT,
                end_time TEXT,
                transcript_ids TEXT,   -- JSON array of transcript IDs in this cluster
                auto_generated INTEGER DEFAULT 1,
                confidence REAL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transcript_clusters_meeting ON transcript_clusters(meeting_id)",
        )
        .execute(&mut *conn)
        .await;

        // ═══════════════════════════════════════════════════════════════════════
        // v3.0.0: Calendar Integration — Meeting Attendees
        // ═══════════════════════════════════════════════════════════════════════

        // Add calendar_event_id to meetings table
        ensure_columns(&mut conn, "meetings", &[("calendar_event_id", "TEXT")]).await?;

        // Meeting attendees table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meeting_attendees (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                company TEXT,
                role TEXT NOT NULL DEFAULT 'attendee',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_attendees_meeting ON meeting_attendees(meeting_id)",
        )
        .execute(&mut *conn)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_attendees_email ON meeting_attendees(email)",
        )
        .execute(&mut *conn)
        .await;

        // Assistant chat history (local; replaces the old Supabase table)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS assistant_conversations (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                user_query TEXT NOT NULL,
                assistant_response TEXT NOT NULL,
                model_used TEXT NOT NULL DEFAULT '',
                context_refs TEXT NOT NULL DEFAULT '[]'
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_assistant_conversations_ts ON assistant_conversations(timestamp)",
        )
        .execute(&mut *conn)
        .await;

        // People + calendar meeting details (owned by people.rs)
        crate::people::ensure_schema(&mut conn).await?;

        // Timed recording + notebooks: meetings.planned_minutes / class_name (notebooks.rs)
        crate::notebooks::ensure_schema(&mut conn).await?;
        // Recording type: meetings.recording_kind, backfilled once from class_name
        crate::recording_kind::ensure_schema(&mut conn).await?;

        // Editing + "Strike from the record" (docs/REDACTION.md)
        crate::redaction::ensure_schema(&mut conn).await?;

        // Links & References: added references, hidden-link hashes (docs/LINKS.md)
        crate::meeting_links::ensure_schema(&mut conn).await?;
        // Moment markers and study guides (docs/STUDY_TOOLS.md)
        crate::markers::ensure_schema(&mut conn).await?;
        crate::study::ensure_schema(&mut conn).await?;
        // Topics and chat with your recordings (docs/TOPICS_AND_CHAT.md)
        crate::topics::ensure_schema(&mut conn).await?;
        crate::chat::ensure_schema(&mut conn).await?;
        // Sync change tracking: triggers on every synced table, so it runs
        // after all of them exist (docs/SYNC.md)
        crate::sync::store::ensure_schema(&mut conn).await?;

        log::info!("Database migrations completed (v3.0 - Calendar Integration)");
        Ok(())
    }

    /// Create a new meeting
    pub async fn create_meeting(&self, id: &str, title: &str) -> Result<Meeting, sqlx::Error> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query("INSERT INTO meetings (id, title, started_at) VALUES (?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind(&now_str)
            .execute(&self.pool)
            .await?;

        Ok(Meeting {
            id: id.to_string(),
            title: title.to_string(),
            started_at: now,
            ended_at: None,
            duration_seconds: None,
            calendar_event_id: None,
            planned_minutes: None,
            class_name: None,
            recording_kind: default_recording_kind(),
        })
    }

    /// Update a meeting's title
    pub async fn update_meeting_title(&self, id: &str, title: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE meetings SET title = ? WHERE id = ?")
            .bind(title)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Set calendar event ID on a meeting
    pub async fn set_meeting_calendar_event(
        &self,
        id: &str,
        calendar_event_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE meetings SET calendar_event_id = ? WHERE id = ?")
            .bind(calendar_event_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Add an attendee to a meeting
    pub async fn add_meeting_attendee(
        &self,
        meeting_id: &str,
        name: &str,
        email: &str,
        company: Option<&str>,
        role: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR IGNORE INTO meeting_attendees (meeting_id, name, email, company, role) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(meeting_id)
        .bind(name)
        .bind(email)
        .bind(company)
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get all attendees for a meeting
    pub async fn get_meeting_attendees(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<MeetingAttendee>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, meeting_id, name, email, company, role FROM meeting_attendees WHERE meeting_id = ? ORDER BY role, name"
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| MeetingAttendee {
                id: row.get("id"),
                meeting_id: row.get("meeting_id"),
                name: row.get("name"),
                email: row.get("email"),
                company: row.get("company"),
                role: row.get("role"),
            })
            .collect())
    }

    /// End a meeting
    pub async fn end_meeting(&self, id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Get the start time to calculate duration
        let row: (String,) = sqlx::query_as("SELECT started_at FROM meetings WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        let started_at = DateTime::parse_from_rfc3339(&row.0)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);

        let duration = (now - started_at).num_seconds();

        sqlx::query("UPDATE meetings SET ended_at = ?, duration_seconds = ? WHERE id = ?")
            .bind(&now_str)
            .bind(duration)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Close meetings left open by a previous run (app quit or crashed
    /// mid-recording, or recorded before stop marked meetings ended).
    /// `ended_at` becomes the last captured moment — latest transcript,
    /// screen state or frame — falling back to `started_at`. Only rows with
    /// `ended_at IS NULL` are touched; no content is deleted. Call at startup
    /// before any recording can begin. Returns how many were closed.
    pub async fn close_stale_meetings(&self) -> Result<usize, sqlx::Error> {
        let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> = match sqlx::query_as(
            "SELECT m.id, m.started_at,
                    (SELECT MAX(timestamp) FROM transcripts t WHERE t.meeting_id = m.id),
                    (SELECT MAX(COALESCE(end_ts, start_ts)) FROM screen_states s WHERE s.meeting_id = m.id),
                    (SELECT MAX(timestamp) FROM frames f WHERE f.meeting_id = m.id)
             FROM meetings m WHERE m.ended_at IS NULL",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(r) => r,
            // Older schema without screen_states: transcripts + frames only
            Err(_) => sqlx::query_as(
                "SELECT m.id, m.started_at,
                        (SELECT MAX(timestamp) FROM transcripts t WHERE t.meeting_id = m.id),
                        NULL,
                        (SELECT MAX(timestamp) FROM frames f WHERE f.meeting_id = m.id)
                 FROM meetings m WHERE m.ended_at IS NULL",
            )
            .fetch_all(&self.pool)
            .await?,
        };

        let parse = |s: &Option<String>| {
            s.as_deref()
                .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
                .map(|d| d.with_timezone(&Utc))
        };
        let mut closed = 0;
        for (id, started, t, ss, f) in &rows {
            let Some(started_at) = parse(&Some(started.clone())) else { continue };
            let ended_at = [parse(t), parse(ss), parse(f)]
                .into_iter()
                .flatten()
                .fold(started_at, |a, b| a.max(b));
            let duration = (ended_at - started_at).num_seconds().max(0);
            let res = sqlx::query(
                "UPDATE meetings SET ended_at = ?, duration_seconds = ? WHERE id = ? AND ended_at IS NULL",
            )
            .bind(ended_at.to_rfc3339())
            .bind(duration)
            .bind(id)
            .execute(&self.pool)
            .await?;
            closed += res.rows_affected() as usize;
        }
        Ok(closed)
    }

    /// Delete transcript lines of `meeting_id` timestamped after `after`
    /// whose text matches `is_junk`. Used to drop hallucinated filler
    /// recorded after a detected meeting end. Returns how many were deleted.
    pub async fn delete_transcripts_after_matching(
        &self,
        meeting_id: &str,
        after: DateTime<Utc>,
        is_junk: impl Fn(&str) -> bool,
    ) -> Result<usize, sqlx::Error> {
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT id, text, timestamp FROM transcripts WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&self.pool)
                .await?;
        let mut deleted = 0;
        for (id, text, ts) in rows {
            let Ok(ts) = DateTime::parse_from_rfc3339(&ts) else { continue };
            if ts.with_timezone(&Utc) <= after || !is_junk(&text) {
                continue;
            }
            deleted += sqlx::query("DELETE FROM transcripts WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected() as usize;
        }
        Ok(deleted)
    }

    /// Get a meeting by ID
    pub async fn get_meeting(&self, id: &str) -> Result<Option<Meeting>, sqlx::Error> {
        let row = sqlx::query(&format!("SELECT {} FROM meetings WHERE id = ?", MEETING_COLUMNS))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.as_ref().map(meeting_from_row))
    }

    /// List all meetings
    pub async fn list_meetings(&self, limit: i32) -> Result<Vec<Meeting>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {} FROM meetings ORDER BY started_at DESC LIMIT ?",
            MEETING_COLUMNS
        ))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(meeting_from_row).collect())
    }

    /// Recordings in one notebook (matched ignoring case), newest first
    pub async fn list_meetings_in_notebook(&self, notebook: &str, limit: i32) -> Result<Vec<Meeting>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {} FROM meetings WHERE class_name = ? COLLATE NOCASE ORDER BY started_at DESC LIMIT ?",
            MEETING_COLUMNS
        ))
        .bind(notebook.trim())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(meeting_from_row).collect())
    }

    /// Delete a meeting and its transcripts
    pub async fn delete_meeting(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Added references and hidden-link hashes (docs/REDACTION.md checklist)
        crate::meeting_links::purge_meeting(&self.pool, id).await?;

        // Its topics, and the chat answers that drew on it (their threads are
        // flagged); explicit, so none of it depends on the cascade
        {
            let mut conn = self.pool.acquire().await?;
            crate::topics::purge_for_meeting(&mut conn, id).await?;
            crate::chat::purge_for_meeting(&mut conn, id).await?;
        }

        sqlx::query("DELETE FROM meetings WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Add a transcript with smart deduplication (skips duplicates within 30 seconds)
    pub async fn add_transcript(
        &self,
        meeting_id: &str,
        text: &str,
        speaker: Option<&str>,
        is_final: bool,
        confidence: f32,
    ) -> Result<i64, sqlx::Error> {
        self.add_transcript_at(meeting_id, text, speaker, is_final, confidence, Utc::now())
            .await
    }

    /// Add a transcript stamped with the actual speech time (not insert time),
    /// so rewind alignment against frames is exact.
    pub async fn add_transcript_at(
        &self,
        meeting_id: &str,
        text: &str,
        speaker: Option<&str>,
        is_final: bool,
        confidence: f32,
        timestamp: DateTime<Utc>,
    ) -> Result<i64, sqlx::Error> {
        self.add_transcript_full(meeting_id, text, speaker, is_final, confidence, timestamp, None)
            .await
    }

    /// Like [`add_transcript_at`], plus optional word timings JSON
    /// (see `redaction::WordTiming`: UTF-16 offsets + ms, never the words).
    #[allow(clippy::too_many_arguments)]
    pub async fn add_transcript_full(
        &self,
        meeting_id: &str,
        text: &str,
        speaker: Option<&str>,
        is_final: bool,
        confidence: f32,
        timestamp: DateTime<Utc>,
        word_timings: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = timestamp;
        let now_str = now.to_rfc3339();

        // Only deduplicate final transcripts
        if is_final && !text.trim().is_empty() {
            let text_hash = transcript_text_hash(text);

            // Check if this exact transcript already exists within last 30 seconds
            let thirty_secs_ago = (now - chrono::Duration::seconds(30)).to_rfc3339();

            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM transcripts 
                 WHERE meeting_id = ? AND text_hash = ? AND timestamp > ?
                 LIMIT 1",
            )
            .bind(meeting_id)
            .bind(&text_hash)
            .bind(&thirty_secs_ago)
            .fetch_optional(&self.pool)
            .await?;

            if let Some((existing_id,)) = existing {
                log::debug!("Skipping duplicate transcript: {}", text);
                return Ok(existing_id);
            }

            // Insert with hash
            let result = sqlx::query(
                "INSERT INTO transcripts (meeting_id, text, speaker, timestamp, is_final, confidence, text_hash, word_timings) 
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(meeting_id)
            .bind(text)
            .bind(speaker)
            .bind(&now_str)
            .bind(is_final as i32)
            .bind(confidence)
            .bind(&text_hash)
            .bind(word_timings)
            .execute(&self.pool)
            .await?;

            return Ok(result.last_insert_rowid());
        }

        // Non-final (interim) transcripts - no deduplication needed
        let result = sqlx::query(
            "INSERT INTO transcripts (meeting_id, text, speaker, timestamp, is_final, confidence) 
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind(text)
        .bind(speaker)
        .bind(&now_str)
        .bind(is_final as i32)
        .bind(confidence)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get transcripts for a meeting, with stricken spans rendered as
    /// `[stricken from the record]`. This is what AI prompts, exports and
    /// every other plain-text consumer must use.
    pub async fn get_transcripts(&self, meeting_id: &str) -> Result<Vec<Transcript>, sqlx::Error> {
        let mut rows = self.get_transcripts_marked(meeting_id).await?;
        for t in rows.iter_mut() {
            t.text = crate::redaction::render_plain(&t.text);
        }
        Ok(rows)
    }

    /// Transcripts with raw strike-marker tokens (`⟦strickenid…⟧`) left in
    /// place, for UI views that render the marker bar. Tokens carry only the
    /// redaction id, never content.
    pub async fn get_transcripts_marked(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<Transcript>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, meeting_id, text, speaker, timestamp, is_final, confidence 
             FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC",
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Transcript {
                id: r.get("id"),
                meeting_id: r.get("meeting_id"),
                text: r.get("text"),
                speaker: r.get("speaker"),
                timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                is_final: r.get::<i32, _>("is_final") == 1,
                confidence: r.get("confidence"),
            })
            .collect())
    }

    /// Search transcripts using FTS5
    pub async fn search_transcripts(&self, query: &str) -> Result<Vec<SearchResult>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT 
                t.meeting_id,
                m.title as meeting_title,
                t.text as transcript_text,
                t.timestamp,
                bm25(transcripts_fts) as relevance
            FROM transcripts_fts
            JOIN transcripts t ON transcripts_fts.rowid = t.id
            JOIN meetings m ON t.meeting_id = m.id
            WHERE transcripts_fts MATCH ?
            ORDER BY relevance
            LIMIT 50
            "#,
        )
        .bind(query)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SearchResult {
                meeting_id: r.get("meeting_id"),
                meeting_title: r.get("meeting_title"),
                transcript_text: crate::redaction::render_plain(&r.get::<String, _>("transcript_text")),
                timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                relevance: r.get("relevance"),
            })
            .collect())
    }

    /// Ranked transcript search for chat context: FTS5 + bm25, with a short
    /// snippet around the match plus the meeting title and start time.
    /// `query` must already be a valid FTS5 expression (see [`fts_or_query`]).
    pub async fn search_transcript_context(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<TranscriptContextHit>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                t.id AS transcript_id,
                t.meeting_id,
                m.title AS meeting_title,
                m.started_at AS meeting_started_at,
                t.timestamp,
                t.speaker,
                snippet(transcripts_fts, 0, '', '', '…', 32) AS snippet,
                bm25(transcripts_fts) AS relevance
            FROM transcripts_fts
            JOIN transcripts t ON transcripts_fts.rowid = t.id
            JOIN meetings m ON t.meeting_id = m.id
            WHERE transcripts_fts MATCH ?
            ORDER BY relevance
            LIMIT ?
            "#,
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TranscriptContextHit {
                transcript_id: r.get("transcript_id"),
                meeting_id: r.get("meeting_id"),
                meeting_title: r.get("meeting_title"),
                meeting_started_at: r.get("meeting_started_at"),
                timestamp: r.get("timestamp"),
                speaker: r.get("speaker"),
                snippet: crate::redaction::render_plain(&r.get::<String, _>("snippet")),
                relevance: r.get("relevance"),
            })
            .collect())
    }

    /// Plain-text search over analyzed activities and captured screen text
    /// (no FTS index exists for these; LIKE over the most recent rows).
    /// Returns (id, timestamp, source label, text), newest first.
    pub async fn search_activity_text(
        &self,
        terms: &[String],
        limit: i64,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();

        let cond = terms
            .iter()
            .map(|_| "(summary LIKE ? OR focus_area LIKE ? OR window_title LIKE ?)")
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT id, start_time, category, app_name, summary FROM activity_log \
             WHERE {} ORDER BY start_time DESC LIMIT ?",
            cond
        );
        let mut q = sqlx::query(&sql);
        for t in terms {
            let pat = format!("%{}%", t);
            q = q.bind(pat.clone()).bind(pat.clone()).bind(pat);
        }
        for r in q.bind(limit).fetch_all(&self.pool).await? {
            let app: Option<String> = r.get("app_name");
            let category: String = r.get("category");
            out.push((
                format!("activity-{}", r.get::<i64, _>("id")),
                r.get::<String, _>("start_time"),
                format!("activity: {}{}", category, app.map(|a| format!(" / {}", a)).unwrap_or_default()),
                r.get::<String, _>("summary"),
            ));
        }

        let cond = terms.iter().map(|_| "text LIKE ?").collect::<Vec<_>>().join(" OR ");
        let sql = format!(
            "SELECT snapshot_id, ts, app_name, window_title, text FROM text_snapshots \
             WHERE {} ORDER BY ts DESC LIMIT ?",
            cond
        );
        let mut q = sqlx::query(&sql);
        for t in terms {
            q = q.bind(format!("%{}%", t));
        }
        for r in q.bind(limit).fetch_all(&self.pool).await? {
            let app: Option<String> = r.get("app_name");
            let title: Option<String> = r.get("window_title");
            let label = match (app, title) {
                (Some(a), Some(t)) => format!("screen text: {} — {}", a, t),
                (Some(a), None) => format!("screen text: {}", a),
                (None, Some(t)) => format!("screen text: {}", t),
                (None, None) => "screen text".to_string(),
            };
            out.push((
                format!("snapshot-{}", r.get::<String, _>("snapshot_id")),
                r.get::<String, _>("ts"),
                label,
                r.get::<String, _>("text"),
            ));
        }
        Ok(out)
    }

    /// Save one assistant Q&A exchange locally.
    pub async fn add_assistant_conversation(
        &self,
        id: &str,
        timestamp: &str,
        user_query: &str,
        assistant_response: &str,
        model_used: &str,
        context_refs: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR IGNORE INTO assistant_conversations \
             (id, timestamp, user_query, assistant_response, model_used, context_refs) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(timestamp)
        .bind(user_query)
        .bind(assistant_response)
        .bind(model_used)
        .bind(serde_json::to_string(context_refs).unwrap_or_else(|_| "[]".to_string()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Most recent assistant exchanges, newest first:
    /// (id, timestamp, user_query, assistant_response, model_used, context_refs)
    #[allow(clippy::type_complexity)]
    pub async fn list_assistant_conversations(
        &self,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, String, Vec<String>)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, timestamp, user_query, assistant_response, model_used, context_refs \
             FROM assistant_conversations ORDER BY timestamp DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let refs: String = r.get("context_refs");
                (
                    r.get("id"),
                    r.get("timestamp"),
                    r.get("user_query"),
                    r.get("assistant_response"),
                    r.get("model_used"),
                    serde_json::from_str(&refs).unwrap_or_default(),
                )
            })
            .collect())
    }

    /// Add a frame to the database (for rewind functionality)
    pub async fn add_frame(
        &self,
        meeting_id: &str,
        timestamp: DateTime<Utc>,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let timestamp_str = timestamp.to_rfc3339();

        // Get next frame number for this meeting
        let frame_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM frames WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let result = sqlx::query(
            "INSERT INTO frames (meeting_id, frame_number, timestamp, file_path, ocr_text) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(meeting_id)
        .bind(frame_count)
        .bind(&timestamp_str)
        .bind(file_path)
        .bind(ocr_text)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get frames for a meeting (for rewind timeline)
    pub async fn get_frames(
        &self,
        meeting_id: &str,
        limit: i32,
    ) -> Result<Vec<Frame>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, meeting_id, frame_number, timestamp, file_path, ocr_text 
             FROM frames WHERE meeting_id = ? ORDER BY timestamp ASC LIMIT ?",
        )
        .bind(meeting_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Frame {
                id: r.get("id"),
                meeting_id: r.get("meeting_id"),
                frame_number: r.try_get("frame_number").unwrap_or(0),
                timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                file_path: r.try_get("file_path").ok(),
                ocr_text: r.try_get("ocr_text").ok(),
            })
            .collect())
    }

    /// Get frames in a time range (for rewind scrubbing)
    pub async fn get_frames_in_range(
        &self,
        meeting_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Frame>, sqlx::Error> {
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        let rows = sqlx::query(
            "SELECT id, meeting_id, frame_number, timestamp, file_path, ocr_text 
             FROM frames WHERE meeting_id = ? AND timestamp >= ? AND timestamp <= ?
             ORDER BY timestamp ASC",
        )
        .bind(meeting_id)
        .bind(&start_str)
        .bind(&end_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Frame {
                id: r.get("id"),
                meeting_id: r.get("meeting_id"),
                frame_number: r.try_get("frame_number").unwrap_or(0),
                timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                file_path: r.try_get("file_path").ok(),
                ocr_text: r.try_get("ocr_text").ok(),
            })
            .collect())
    }

    /// Get the most recent frame for a meeting
    pub async fn get_latest_frame(&self, meeting_id: &str) -> Result<Option<Frame>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, meeting_id, frame_number, timestamp, file_path, ocr_text 
             FROM frames WHERE meeting_id = ? ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(meeting_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Frame {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            frame_number: r.try_get("frame_number").unwrap_or(0),
            timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            file_path: r.try_get("file_path").ok(),
            ocr_text: r.try_get("ocr_text").ok(),
        }))
    }

    /// Count frames for a meeting
    pub async fn count_frames(&self, meeting_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM frames WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 1: Stateful Screen Ingest - ScreenState methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a new screen state
    pub async fn add_screen_state(
        &self,
        state_id: &str,
        meeting_id: &str,
        start_ts: DateTime<Utc>,
        end_ts: Option<DateTime<Utc>>,
        phash: &str,
        delta_score: f32,
        keyframe_path: Option<&str>,
        state_type: &str,
        flags_json: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO screen_states 
            (state_id, meeting_id, start_ts, end_ts, phash, delta_score, keyframe_path, state_type, flags)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(state_id)
        .bind(meeting_id)
        .bind(start_ts.to_rfc3339())
        .bind(end_ts.map(|ts| ts.to_rfc3339()))
        .bind(phash)
        .bind(delta_score)
        .bind(keyframe_path)
        .bind(state_type)
        .bind(flags_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record which display/window a screen state was captured from
    pub async fn set_screen_state_source(
        &self,
        state_id: &str,
        source_key: &str,
        label: &str,
        app_name: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE screen_states SET source_key = ?, window_title = ?, app_name = ? WHERE state_id = ?",
        )
        .bind(source_key)
        .bind(label)
        .bind(app_name)
        .bind(state_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update screen state end timestamp (extend duration)
    pub async fn extend_screen_state(
        &self,
        state_id: &str,
        end_ts: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE screen_states SET end_ts = ? WHERE state_id = ?")
            .bind(end_ts.to_rfc3339())
            .bind(state_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Update screen state with keyframe path after saving
    pub async fn update_screen_state_keyframe(
        &self,
        state_id: &str,
        keyframe_path: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE screen_states SET keyframe_path = ? WHERE state_id = ?")
            .bind(keyframe_path)
            .bind(state_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get screen states for a meeting
    pub async fn get_screen_states(
        &self,
        meeting_id: &str,
        limit: i32,
    ) -> Result<Vec<ScreenStateRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT state_id, meeting_id, start_ts, end_ts, app_name, window_title,
                   phash, delta_score, keyframe_path, state_type, flags, created_at
            FROM screen_states 
            WHERE meeting_id = ?
            ORDER BY start_ts ASC
            LIMIT ?
            "#,
        )
        .bind(meeting_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ScreenStateRecord {
                state_id: r.get("state_id"),
                meeting_id: r.get("meeting_id"),
                start_ts: r.get("start_ts"),
                end_ts: r.try_get("end_ts").ok(),
                app_name: r.try_get("app_name").ok(),
                window_title: r.try_get("window_title").ok(),
                phash: r.get("phash"),
                delta_score: r.try_get("delta_score").unwrap_or(0.0),
                keyframe_path: r.try_get("keyframe_path").ok(),
                state_type: r
                    .try_get("state_type")
                    .unwrap_or_else(|_| "other".to_string()),
                flags: r.try_get("flags").unwrap_or_else(|_| "{}".to_string()),
            })
            .collect())
    }

    /// Count screen states for a meeting
    pub async fn count_screen_states(&self, meeting_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM screen_states WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0)
    }

    /// Get the latest screen state for a meeting
    pub async fn get_latest_screen_state(
        &self,
        meeting_id: &str,
    ) -> Result<Option<ScreenStateRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT state_id, meeting_id, start_ts, end_ts, app_name, window_title,
                   phash, delta_score, keyframe_path, state_type, flags, created_at
            FROM screen_states 
            WHERE meeting_id = ?
            ORDER BY start_ts DESC
            LIMIT 1
            "#,
        )
        .bind(meeting_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ScreenStateRecord {
            state_id: r.get("state_id"),
            meeting_id: r.get("meeting_id"),
            start_ts: r.get("start_ts"),
            end_ts: r.try_get("end_ts").ok(),
            app_name: r.try_get("app_name").ok(),
            window_title: r.try_get("window_title").ok(),
            phash: r.get("phash"),
            delta_score: r.try_get("delta_score").unwrap_or(0.0),
            keyframe_path: r.try_get("keyframe_path").ok(),
            state_type: r
                .try_get("state_type")
                .unwrap_or_else(|_| "other".to_string()),
            flags: r.try_get("flags").unwrap_or_else(|_| "{}".to_string()),
        }))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 2: Document Episodes & Text Snapshots CRUD
    // ═══════════════════════════════════════════════════════════════════════════

    /// Create a new document episode
    pub async fn create_episode(
        &self,
        episode_id: &str,
        meeting_id: &str,
        start_ts: DateTime<Utc>,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO document_episodes 
            (episode_id, meeting_id, start_ts, app_name, window_title)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(episode_id)
        .bind(meeting_id)
        .bind(start_ts.to_rfc3339())
        .bind(app_name)
        .bind(window_title)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update episode end time and stats
    pub async fn update_episode(
        &self,
        episode_id: &str,
        end_ts: DateTime<Utc>,
        state_count: i32,
        total_duration_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE document_episodes 
            SET end_ts = ?, state_count = ?, total_duration_ms = ?
            WHERE episode_id = ?
            "#,
        )
        .bind(end_ts.to_rfc3339())
        .bind(state_count)
        .bind(total_duration_ms)
        .bind(episode_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Link a state to an episode
    pub async fn link_state_to_episode(
        &self,
        episode_id: &str,
        state_id: &str,
        sequence_num: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO episode_states (episode_id, state_id, sequence_num)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(episode_id)
        .bind(state_id)
        .bind(sequence_num)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get episodes for a meeting
    pub async fn get_episodes(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<DocumentEpisodeRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT episode_id, meeting_id, start_ts, end_ts, app_name, window_title,
                   document_fingerprint, state_count, total_duration_ms
            FROM document_episodes 
            WHERE meeting_id = ?
            ORDER BY start_ts ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DocumentEpisodeRecord {
                episode_id: r.get("episode_id"),
                meeting_id: r.get("meeting_id"),
                start_ts: r.get("start_ts"),
                end_ts: r.try_get("end_ts").ok(),
                app_name: r.try_get("app_name").ok(),
                window_title: r.try_get("window_title").ok(),
                document_fingerprint: r.try_get("document_fingerprint").ok(),
                state_count: r.try_get("state_count").unwrap_or(0),
                total_duration_ms: r.try_get("total_duration_ms").unwrap_or(0),
            })
            .collect())
    }

    /// Add a text snapshot (legacy signature for backwards compatibility)
    pub async fn add_text_snapshot(
        &self,
        snapshot_id: &str,
        episode_id: Option<&str>,
        state_id: Option<&str>,
        ts: DateTime<Utc>,
        text: &str,
        text_hash: &str,
        quality_score: f32,
        source: &str,
    ) -> Result<(), sqlx::Error> {
        self.add_text_snapshot_full(
            snapshot_id,
            episode_id,
            state_id,
            None,
            ts,
            text,
            text_hash,
            quality_score,
            source,
            None,
            None,
        )
        .await
    }

    /// Add a text snapshot with full metadata including meeting and app context
    pub async fn add_text_snapshot_full(
        &self,
        snapshot_id: &str,
        episode_id: Option<&str>,
        state_id: Option<&str>,
        meeting_id: Option<&str>,
        ts: DateTime<Utc>,
        text: &str,
        text_hash: &str,
        quality_score: f32,
        source: &str,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let word_count = text.split_whitespace().count() as i32;

        sqlx::query(
            r#"
            INSERT INTO text_snapshots 
            (snapshot_id, episode_id, state_id, meeting_id, ts, text, text_hash, quality_score, source, word_count, app_name, window_title)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(snapshot_id)
        .bind(episode_id)
        .bind(state_id)
        .bind(meeting_id)
        .bind(ts.to_rfc3339())
        .bind(text)
        .bind(text_hash)
        .bind(quality_score)
        .bind(source)
        .bind(word_count)
        .bind(app_name)
        .bind(window_title)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get latest snapshot for an episode
    pub async fn get_latest_snapshot(
        &self,
        episode_id: &str,
    ) -> Result<Option<TextSnapshotRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_id, episode_id, state_id, meeting_id, ts, text, text_hash, 
                   quality_score, source, word_count, app_name, window_title
            FROM text_snapshots 
            WHERE episode_id = ?
            ORDER BY ts DESC
            LIMIT 1
            "#,
        )
        .bind(episode_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| TextSnapshotRecord {
            snapshot_id: r.get("snapshot_id"),
            episode_id: r.try_get("episode_id").ok(),
            state_id: r.try_get("state_id").ok(),
            meeting_id: r.try_get("meeting_id").ok(),
            ts: r.get("ts"),
            text: r.get("text"),
            text_hash: r.get("text_hash"),
            quality_score: r.try_get("quality_score").unwrap_or(0.0),
            source: r.try_get("source").unwrap_or_else(|_| "ocr".to_string()),
            word_count: r.try_get("word_count").unwrap_or(0),
            app_name: r.try_get("app_name").ok(),
            window_title: r.try_get("window_title").ok(),
        }))
    }

    /// Get all text snapshots for a meeting (ordered by timestamp)
    pub async fn get_text_snapshots_by_meeting(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<TextSnapshotRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT snapshot_id, episode_id, state_id, meeting_id, ts, text, text_hash, 
                   quality_score, source, word_count, app_name, window_title
            FROM text_snapshots 
            WHERE meeting_id = ?
            ORDER BY ts ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TextSnapshotRecord {
                snapshot_id: r.get("snapshot_id"),
                episode_id: r.try_get("episode_id").ok(),
                state_id: r.try_get("state_id").ok(),
                meeting_id: r.try_get("meeting_id").ok(),
                ts: r.get("ts"),
                text: r.get("text"),
                text_hash: r.get("text_hash"),
                quality_score: r.try_get("quality_score").unwrap_or(0.0),
                source: r.try_get("source").unwrap_or_else(|_| "ocr".to_string()),
                word_count: r.try_get("word_count").unwrap_or(0),
                app_name: r.try_get("app_name").ok(),
                window_title: r.try_get("window_title").ok(),
            })
            .collect())
    }

    /// Add a text patch (diff between snapshots)
    pub async fn add_text_patch(
        &self,
        patch_id: &str,
        episode_id: &str,
        from_snapshot_id: Option<&str>,
        to_snapshot_id: Option<&str>,
        from_text_hash: &str,
        to_text_hash: &str,
        ts: DateTime<Utc>,
        unified_diff: &str,
        lines_added: i32,
        lines_removed: i32,
        change_summary: Option<&str>,
        change_type: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO text_patches 
            (patch_id, episode_id, from_snapshot_id, to_snapshot_id, from_text_hash, 
             to_text_hash, ts, unified_diff, lines_added, lines_removed, change_summary, change_type)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(patch_id)
        .bind(episode_id)
        .bind(from_snapshot_id)
        .bind(to_snapshot_id)
        .bind(from_text_hash)
        .bind(to_text_hash)
        .bind(ts.to_rfc3339())
        .bind(unified_diff)
        .bind(lines_added)
        .bind(lines_removed)
        .bind(change_summary)
        .bind(change_type)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get patches for an episode
    pub async fn get_patches(&self, episode_id: &str) -> Result<Vec<TextPatchRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT patch_id, episode_id, from_snapshot_id, to_snapshot_id,
                   from_text_hash, to_text_hash, ts, unified_diff,
                   lines_added, lines_removed, change_summary, change_type
            FROM text_patches 
            WHERE episode_id = ?
            ORDER BY ts ASC
            "#,
        )
        .bind(episode_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TextPatchRecord {
                patch_id: r.get("patch_id"),
                episode_id: r.get("episode_id"),
                from_snapshot_id: r.try_get("from_snapshot_id").ok(),
                to_snapshot_id: r.try_get("to_snapshot_id").ok(),
                from_text_hash: r.get("from_text_hash"),
                to_text_hash: r.get("to_text_hash"),
                ts: r.get("ts"),
                unified_diff: r.get("unified_diff"),
                lines_added: r.try_get("lines_added").unwrap_or(0),
                lines_removed: r.try_get("lines_removed").unwrap_or(0),
                change_summary: r.try_get("change_summary").ok(),
                change_type: r
                    .try_get("change_type")
                    .unwrap_or_else(|_| "content_changed".to_string()),
            })
            .collect())
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 3: Timeline Events CRUD
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a timeline event
    pub async fn add_timeline_event(
        &self,
        event_id: &str,
        meeting_id: &str,
        ts: DateTime<Utc>,
        event_type: &str,
        title: &str,
        description: Option<&str>,
        app_name: Option<&str>,
        window_title: Option<&str>,
        duration_ms: Option<i64>,
        episode_id: Option<&str>,
        state_id: Option<&str>,
        topic: Option<&str>,
        importance: f32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO meeting_timeline_events 
            (event_id, meeting_id, ts, event_type, title, description, 
             app_name, window_title, duration_ms, episode_id, state_id, topic, importance)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event_id)
        .bind(meeting_id)
        .bind(ts.to_rfc3339())
        .bind(event_type)
        .bind(title)
        .bind(description)
        .bind(app_name)
        .bind(window_title)
        .bind(duration_ms)
        .bind(episode_id)
        .bind(state_id)
        .bind(topic)
        .bind(importance)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get timeline events for a meeting
    pub async fn get_timeline_events(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<TimelineEventRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT event_id, meeting_id, ts, event_type, title, description,
                   app_name, window_title, duration_ms, episode_id, state_id, topic, importance
            FROM meeting_timeline_events 
            WHERE meeting_id = ?
            ORDER BY ts ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TimelineEventRecord {
                event_id: r.get("event_id"),
                meeting_id: r.get("meeting_id"),
                ts: r.get("ts"),
                event_type: r.get("event_type"),
                title: r.get("title"),
                description: r.try_get("description").ok(),
                app_name: r.try_get("app_name").ok(),
                window_title: r.try_get("window_title").ok(),
                duration_ms: r.try_get("duration_ms").ok(),
                episode_id: r.try_get("episode_id").ok(),
                state_id: r.try_get("state_id").ok(),
                topic: r.try_get("topic").ok(),
                importance: r.try_get("importance").unwrap_or(0.5),
            })
            .collect())
    }

    /// Add a topic cluster
    pub async fn add_topic_cluster(
        &self,
        topic_id: &str,
        meeting_id: &str,
        name: &str,
        description: Option<&str>,
        start_ts: DateTime<Utc>,
        end_ts: Option<DateTime<Utc>>,
        event_count: i32,
        total_duration_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO topic_clusters 
            (topic_id, meeting_id, name, description, start_ts, end_ts, event_count, total_duration_ms)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(topic_id)
        .bind(meeting_id)
        .bind(name)
        .bind(description)
        .bind(start_ts.to_rfc3339())
        .bind(end_ts.map(|ts| ts.to_rfc3339()))
        .bind(event_count)
        .bind(total_duration_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get topic clusters for a meeting
    pub async fn get_topic_clusters(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<TopicClusterRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT topic_id, meeting_id, name, description, start_ts, end_ts,
                   event_count, total_duration_ms
            FROM topic_clusters 
            WHERE meeting_id = ?
            ORDER BY start_ts ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TopicClusterRecord {
                topic_id: r.get("topic_id"),
                meeting_id: r.get("meeting_id"),
                name: r.get("name"),
                description: r.try_get("description").ok(),
                start_ts: r.get("start_ts"),
                end_ts: r.try_get("end_ts").ok(),
                event_count: r.try_get("event_count").unwrap_or(0),
                total_duration_ms: r.try_get("total_duration_ms").unwrap_or(0),
            })
            .collect())
    }
}

/// Screen state database record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenStateRecord {
    pub state_id: String,
    pub meeting_id: String,
    pub start_ts: String,
    pub end_ts: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub phash: String,
    pub delta_score: f32,
    pub keyframe_path: Option<String>,
    pub state_type: String,
    pub flags: String,
}

/// Document episode database record (Phase 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEpisodeRecord {
    pub episode_id: String,
    pub meeting_id: String,
    pub start_ts: String,
    pub end_ts: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub document_fingerprint: Option<String>,
    pub state_count: i32,
    pub total_duration_ms: i64,
}

/// Text snapshot database record (Phase 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSnapshotRecord {
    pub snapshot_id: String,
    pub episode_id: Option<String>,
    pub state_id: Option<String>,
    pub meeting_id: Option<String>,
    pub ts: String,
    pub text: String,
    pub text_hash: String,
    pub quality_score: f32,
    pub source: String,
    pub word_count: i32,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

/// Text patch database record (Phase 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPatchRecord {
    pub patch_id: String,
    pub episode_id: String,
    pub from_snapshot_id: Option<String>,
    pub to_snapshot_id: Option<String>,
    pub from_text_hash: String,
    pub to_text_hash: String,
    pub ts: String,
    pub unified_diff: String,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub change_summary: Option<String>,
    pub change_type: String,
}

/// Timeline event database record (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEventRecord {
    pub event_id: String,
    pub meeting_id: String,
    pub ts: String,
    pub event_type: String,
    pub title: String,
    pub description: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub duration_ms: Option<i64>,
    pub episode_id: Option<String>,
    pub state_id: Option<String>,
    pub topic: Option<String>,
    pub importance: f32,
}

/// Topic cluster database record (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicClusterRecord {
    pub topic_id: String,
    pub meeting_id: String,
    pub name: String,
    pub description: Option<String>,
    pub start_ts: String,
    pub end_ts: Option<String>,
    pub event_count: i32,
    pub total_duration_ms: i64,
}

/// Frame record (for rewind timeline)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub id: i64,
    pub meeting_id: String,
    pub frame_number: i64,
    pub timestamp: DateTime<Utc>,
    pub file_path: Option<String>,
    pub ocr_text: Option<String>,
}

/// Synced timeline data (frames + transcripts aligned by timestamp)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedTimeline {
    pub meeting_id: String,
    pub meeting_title: String,
    /// RFC3339 meeting start, to place markers (whose times are wall-clock)
    #[serde(default)]
    pub started_at: String,
    pub duration_seconds: i64,
    pub frames: Vec<TimelineFrame>,
    pub transcripts: Vec<TimelineTranscript>,
    /// "Stricken from the record" markers (no content) for this meeting
    #[serde(default)]
    pub redactions: Vec<crate::redaction::RedactionRecord>,
}

/// Frame on the timeline (simplified for UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineFrame {
    pub id: String,
    pub frame_number: i64,
    pub timestamp_ms: i64, // Milliseconds from start of meeting
    pub thumbnail_path: Option<String>,
    /// When the screen stopped being shown (ms from the start), if known.
    /// Lets the UI turn a screen selection into a time range.
    #[serde(default)]
    pub end_ms: Option<i64>,
}

/// Transcript on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTranscript {
    pub id: String,
    pub timestamp_ms: i64, // Milliseconds from start of meeting
    pub text: String,
    pub speaker: Option<String>,
    pub is_final: bool,
    pub duration_seconds: f64,
    /// End of the line's time span (ms from the start), as a time-range
    /// Delete/Strike reads it: its last word's end (word timings), else its
    /// length estimated from the word count (bounded by the next line).
    /// `None` when its timestamp can't be read (no time range reaches it).
    /// Lets the UI link a transcript selection to screens by time.
    #[serde(default)]
    pub end_ms: Option<i64>,
    /// Middle of each word's time (ms from the start), in text order, when
    /// word timings are stored: a time range removes exactly the words whose
    /// middle is inside it. Times only, no content.
    #[serde(default)]
    pub word_mids_ms: Option<Vec<i64>>,
}

impl DatabaseManager {
    /// Get synced timeline for a meeting (frames + transcripts aligned)
    /// Supports both legacy frames table and new screen_states table
    pub async fn get_synced_timeline(
        &self,
        meeting_id: &str,
    ) -> Result<Option<SyncedTimeline>, sqlx::Error> {
        eprintln!(
            "🔍 get_synced_timeline CALLED with meeting_id: {}",
            meeting_id
        );

        // Get meeting info
        let meeting = match self.get_meeting(meeting_id).await? {
            Some(m) => m,
            None => {
                eprintln!("❌ Meeting not found: {}", meeting_id);
                return Ok(None);
            }
        };

        let start_time = meeting.started_at;
        let duration = meeting.duration_seconds.unwrap_or(0);

        // Try to get frames from legacy frames table first
        let legacy_frames = self.get_frames(meeting_id, 10000).await?;
        eprintln!(
            "📊 Legacy frames count: {} for meeting: {}",
            legacy_frames.len(),
            meeting_id
        );

        // Both sources, in time order: legacy frames, and screen states with
        // a picture (stateful capture, Capture screen, and photos and
        // screens synced from the iPhone, docs/SYNC.md)
        let mut timeline_frames: Vec<TimelineFrame> = legacy_frames
            .into_iter()
            .map(|f| {
                let ms = (f.timestamp - start_time).num_milliseconds();
                TimelineFrame {
                    id: f.id.to_string(),
                    frame_number: f.frame_number,
                    timestamp_ms: ms.max(0),
                    thumbnail_path: f.file_path,
                    end_ms: None,
                }
            })
            .collect();
        let screen_states = self.get_screen_states(meeting_id, 10000).await?;
        timeline_frames.extend(
            screen_states
                .into_iter()
                .filter(|s| s.keyframe_path.is_some()) // Only use states with keyframes
                .map(|s| {
                    let state_ts = DateTime::parse_from_rfc3339(&s.start_ts)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(start_time);
                    let ms = (state_ts - start_time).num_milliseconds();
                    let end_ms = s
                        .end_ts
                        .as_deref()
                        .and_then(|e| DateTime::parse_from_rfc3339(e).ok())
                        .map(|e| (e.with_timezone(&Utc) - start_time).num_milliseconds().max(ms.max(0)));
                    TimelineFrame {
                        id: s.state_id,
                        frame_number: 0,
                        timestamp_ms: ms.max(0),
                        thumbnail_path: s.keyframe_path,
                        end_ms,
                    }
                }),
        );
        timeline_frames.sort_by_key(|f| f.timestamp_ms);
        for (i, f) in timeline_frames.iter_mut().enumerate() {
            f.frame_number = i as i64;
        }

        // Get transcripts (UI view: keep strike-marker tokens for the bar)
        let transcripts = self.get_transcripts_marked(meeting_id).await?;
        let redactions = crate::redaction::list_strikes(&self.pool, meeting_id).await?;
        let mut extents = crate::redaction::time_range::line_extents(&self.pool, meeting_id, start_time).await?;
        let timeline_transcripts: Vec<TimelineTranscript> = transcripts
            .into_iter()
            .map(|t| {
                let ms = (t.timestamp - start_time).num_milliseconds();
                // A line before the meeting start shows at 0:00, where no
                // time range from the timeline can reach it: no span
                let extent = extents.remove(&t.id).filter(|_| ms >= 0);
                TimelineTranscript {
                    id: t.id.to_string(),
                    timestamp_ms: ms.max(0),
                    text: t.text,
                    speaker: t.speaker,
                    is_final: t.is_final,
                    duration_seconds: 0.0, // TODO: Store actual duration from Deepgram
                    end_ms: extent.as_ref().map(|e| e.end_ms.max(ms)),
                    word_mids_ms: extent.and_then(|e| e.word_mids_ms),
                }
            })
            .collect();

        Ok(Some(SyncedTimeline {
            meeting_id: meeting_id.to_string(),
            meeting_title: meeting.title,
            started_at: start_time.to_rfc3339(),
            duration_seconds: duration,
            frames: timeline_frames,
            transcripts: timeline_transcripts,
            redactions,
        }))
    }
}

// ============================================
// Knowledge Base Data Structures
// ============================================

/// Frame queued for VLM analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameQueueItem {
    pub id: i64,
    pub frame_id: Option<i64>,
    pub frame_path: String,
    pub captured_at: DateTime<Utc>,
    pub analyzed: bool,
    pub synced: bool,
}

/// Activity log entry (from VLM analysis)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub id: Option<i64>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub category: String,
    pub summary: String,
    pub focus_area: Option<String>,
    pub visible_files: Option<String>,
    pub confidence: Option<f32>,
    pub frame_ids: Option<String>,
}

/// Entity extracted from VLM analysis (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Option<i64>,
    pub activity_id: i64,
    pub entity_type: String, // "person", "company", "feature", "task", etc.
    pub name: String,
    pub metadata: Option<String>, // JSON
    pub confidence: f32,
    pub theme: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl DatabaseManager {
    // ============================================
    // Entity Methods (Phase 3)
    // ============================================

    /// Add an extracted entity
    pub async fn add_entity(
        &self,
        activity_id: i64,
        entity_type: &str,
        name: &str,
        metadata: Option<&serde_json::Value>,
        confidence: f32,
        theme: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let metadata_str = metadata.map(|v| v.to_string());

        let result = sqlx::query(
            "INSERT INTO entities (activity_id, entity_type, name, metadata, confidence, theme) 
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(activity_id)
        .bind(entity_type)
        .bind(name)
        .bind(metadata_str)
        .bind(confidence)
        .bind(theme)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get entities for an activity
    pub async fn get_entities(&self, activity_id: i64) -> Result<Vec<Entity>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, activity_id, entity_type, name, metadata, confidence, theme, created_at 
             FROM entities WHERE activity_id = ?",
        )
        .bind(activity_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Entity {
                id: Some(r.get("id")),
                activity_id: r.get("activity_id"),
                entity_type: r.get("entity_type"),
                name: r.get("name"),
                metadata: r.get("metadata"),
                confidence: r.get("confidence"),
                theme: r.get("theme"),
                created_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("created_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
    }

    /// Get recent extracted entities (filtered by theme/type)
    pub async fn get_recent_entities(
        &self,
        limit: i32,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let sql = r#"
            SELECT 
                e.id, e.entity_type, e.name, e.confidence, e.activity_id,
                e.theme, e.created_at, e.metadata,
                a.app_name, a.window_title, a.start_time
            FROM entities e
            JOIN activity_log a ON e.activity_id = a.id
            ORDER BY a.start_time DESC
            LIMIT ?
        "#;

        let rows = sqlx::query(sql).bind(limit).fetch_all(&self.pool).await?;

        let result = rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let activity_id: i64 = row.get("activity_id");
                let entity_type: String = row.get("entity_type");
                let name: String = row.get("name");
                let confidence: f32 = row.get("confidence");
                let theme: Option<String> = row.try_get("theme").ok();
                let created_at: String = row.get("created_at");
                let metadata_str: Option<String> = row.try_get("metadata").ok();
                let app_name: Option<String> = row.try_get("app_name").ok();
                let window_title: Option<String> = row.try_get("window_title").ok();
                let start_time: String = row.get("start_time");

                // Parse metadata string to JSON object if present
                let metadata_obj: Option<serde_json::Value> =
                    metadata_str.and_then(|s| serde_json::from_str(&s).ok());

                serde_json::json!({
                    "id": id,
                    "activity_id": activity_id,
                    "entity_type": entity_type,
                    "name": name,
                    "metadata": metadata_obj,
                    "confidence": confidence,
                    "theme": theme,
                    "created_at": created_at,
                    "source": {
                        "app_name": app_name,
                        "window_title": window_title,
                        "timestamp": start_time
                    }
                })
            })
            .collect();

        Ok(result)
    }

    /// Add a frame to the analysis queue
    pub async fn queue_frame(
        &self,
        frame_id: Option<i64>,
        frame_path: &str,
        captured_at: DateTime<Utc>,
    ) -> Result<i64, sqlx::Error> {
        let captured_str = captured_at.to_rfc3339();

        let result = sqlx::query(
            "INSERT INTO frame_queue (frame_id, frame_path, captured_at) VALUES (?, ?, ?)",
        )
        .bind(frame_id)
        .bind(frame_path)
        .bind(&captured_str)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get pending frames for analysis
    pub async fn get_pending_frames(&self, limit: i32) -> Result<Vec<FrameQueueItem>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, frame_id, frame_path, captured_at, analyzed, synced
             FROM frame_queue WHERE analyzed = 0 ORDER BY captured_at ASC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| FrameQueueItem {
                id: r.get("id"),
                frame_id: r.get("frame_id"),
                frame_path: r.get("frame_path"),
                captured_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("captured_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                analyzed: r.get::<i32, _>("analyzed") == 1,
                synced: r.get::<i32, _>("synced") == 1,
            })
            .collect())
    }

    /// Mark frame as analyzed
    pub async fn mark_frame_analyzed(&self, queue_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE frame_queue SET analyzed = 1 WHERE id = ?")
            .bind(queue_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Mark frame as synced
    pub async fn mark_frame_synced(&self, queue_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE frame_queue SET synced = 1 WHERE id = ?")
            .bind(queue_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get unsynced frames count
    pub async fn count_unsynced_frames(&self) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM frame_queue WHERE synced = 0")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    // ============================================
    // Activity Log Methods
    // ============================================

    /// Add an activity log entry
    pub async fn add_activity(&self, activity: &ActivityLogEntry) -> Result<i64, sqlx::Error> {
        let start_str = activity.start_time.to_rfc3339();
        let end_str = activity.end_time.map(|dt| dt.to_rfc3339());

        let result = sqlx::query(
            r#"INSERT INTO activity_log 
               (start_time, end_time, duration_seconds, app_name, window_title, 
                category, summary, focus_area, visible_files, confidence, frame_ids)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&start_str)
        .bind(&end_str)
        .bind(&activity.duration_seconds)
        .bind(&activity.app_name)
        .bind(&activity.window_title)
        .bind(&activity.category)
        .bind(&activity.summary)
        .bind(&activity.focus_area)
        .bind(&activity.visible_files)
        .bind(&activity.confidence)
        .bind(&activity.frame_ids)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get activities by time range
    pub async fn get_activities(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ActivityLogEntry>, sqlx::Error> {
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        let rows = sqlx::query(
            "SELECT * FROM activity_log WHERE start_time >= ? AND start_time <= ? ORDER BY start_time ASC"
        )
        .bind(&start_str)
        .bind(&end_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ActivityLogEntry {
                id: Some(r.get("id")),
                start_time: DateTime::parse_from_rfc3339(&r.get::<String, _>("start_time"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                end_time: r
                    .get::<Option<String>, _>("end_time")
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                duration_seconds: r.get("duration_seconds"),
                app_name: r.get("app_name"),
                window_title: r.get("window_title"),
                category: r.get("category"),
                summary: r.get("summary"),
                focus_area: r.get("focus_area"),
                visible_files: r.get("visible_files"),
                confidence: r.get("confidence"),
                frame_ids: r.get("frame_ids"),
            })
            .collect())
    }

    /// Get activity stats by category for a date
    pub async fn get_activity_stats(&self, date: &str) -> Result<serde_json::Value, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT category, 
                      COUNT(*) as count, 
                      SUM(duration_seconds) as total_seconds
               FROM activity_log 
               WHERE DATE(start_time) = ?
               GROUP BY category
               ORDER BY total_seconds DESC"#,
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        let mut stats = serde_json::Map::new();
        for row in rows {
            let category: String = row.get("category");
            let count: i64 = row.get("count");
            let seconds: Option<i64> = row.get("total_seconds");
            stats.insert(
                category,
                serde_json::json!({
                    "count": count,
                    "total_seconds": seconds.unwrap_or(0)
                }),
            );
        }

        Ok(serde_json::Value::Object(stats))
    }

    /// Get activities with flexible filtering (for search commands)
    pub async fn get_activities_filtered(
        &self,
        start_date: Option<&str>,
        end_date: Option<&str>,
        category: Option<&str>,
        limit: i32,
    ) -> Result<Vec<ActivityLogEntry>, sqlx::Error> {
        // Build dynamic query based on provided filters
        let mut conditions = Vec::new();
        let mut query_str = String::from("SELECT * FROM activity_log WHERE 1=1");

        if start_date.is_some() {
            conditions.push("start_time >= ?");
        }
        if end_date.is_some() {
            conditions.push("start_time <= ?");
        }
        if category.is_some() {
            conditions.push("category = ?");
        }

        for cond in &conditions {
            query_str.push_str(" AND ");
            query_str.push_str(cond);
        }
        query_str.push_str(" ORDER BY start_time DESC LIMIT ?");

        // Build query with bindings
        let mut query = sqlx::query(&query_str);

        if let Some(start) = start_date {
            query = query.bind(start);
        }
        if let Some(end) = end_date {
            query = query.bind(end);
        }
        if let Some(cat) = category {
            query = query.bind(cat);
        }
        query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|r| ActivityLogEntry {
                id: Some(r.get("id")),
                start_time: DateTime::parse_from_rfc3339(&r.get::<String, _>("start_time"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                end_time: r
                    .get::<Option<String>, _>("end_time")
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                duration_seconds: r.get("duration_seconds"),
                app_name: r.get("app_name"),
                window_title: r.get("window_title"),
                category: r.get("category"),
                summary: r.get("summary"),
                focus_area: r.get("focus_area"),
                visible_files: r.get("visible_files"),
                confidence: r.get("confidence"),
                frame_ids: r.get("frame_ids"),
            })
            .collect())
    }

    /// Clear the frame queue (pending VLM analysis)
    pub async fn clear_frame_queue(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM frame_queue")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Clear the activity log
    pub async fn clear_activity_log(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM activity_log")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ============================================
    // Theme Session Tracking
    // ============================================

    /// Start a new theme session
    pub async fn start_theme_session(&self, theme: &str) -> Result<i64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query("INSERT INTO theme_sessions (theme, started_at) VALUES (?, ?)")
            .bind(theme)
            .bind(&now)
            .execute(&self.pool)
            .await?;

        Ok(result.last_insert_rowid())
    }

    /// End the current theme session
    pub async fn end_theme_session(&self, session_id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        // Get start time to calculate duration
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT started_at FROM theme_sessions WHERE id = ? AND ended_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((started_at_str,)) = row {
            if let Ok(started_at) = DateTime::parse_from_rfc3339(&started_at_str) {
                let duration = (now.timestamp() - started_at.timestamp()) as i32;

                sqlx::query(
                    "UPDATE theme_sessions SET ended_at = ?, duration_seconds = ? WHERE id = ?",
                )
                .bind(now.to_rfc3339())
                .bind(duration)
                .bind(session_id)
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get total time in a theme for today (in seconds)
    pub async fn get_theme_time_today(&self, theme: &str) -> Result<i64, sqlx::Error> {
        let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_str =
            DateTime::<Utc>::from_naive_utc_and_offset(today_start, Utc).to_rfc3339();

        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(duration_seconds) FROM theme_sessions WHERE theme = ? AND started_at >= ? AND ended_at IS NOT NULL"
        )
        .bind(theme)
        .bind(&today_start_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0.unwrap_or(0))
    }

    /// Get the last open session ID for cleanup
    pub async fn get_last_open_session(&self) -> Result<Option<i64>, sqlx::Error> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM theme_sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0))
    }

    // ============================================
    // Phase 3: Entity Methods
    // ============================================

    /// Insert an entity extracted from VLM analysis
    pub async fn insert_entity(
        &self,
        activity_id: i64,
        entity_type: &str,
        name: &str,
        metadata: Option<&str>,
        confidence: f32,
        theme: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            "INSERT INTO entities (activity_id, entity_type, name, metadata, confidence, theme, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(activity_id)
        .bind(entity_type)
        .bind(name)
        .bind(metadata)
        .bind(confidence)
        .bind(theme)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// List all entities with optional filters
    pub async fn list_entities(
        &self,
        limit: Option<i32>,
        entity_type: Option<&str>,
        theme: Option<&str>,
    ) -> Result<Vec<Entity>, sqlx::Error> {
        let limit = limit.unwrap_or(100);

        let mut query = "SELECT id, activity_id, entity_type, name, metadata, confidence, theme, created_at FROM entities WHERE 1=1".to_string();

        if entity_type.is_some() {
            query.push_str(" AND entity_type = ?");
        }
        if theme.is_some() {
            query.push_str(" AND theme = ?");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ?");

        let mut query_builder = sqlx::query_as::<
            _,
            (
                Option<i64>,
                i64,
                String,
                String,
                Option<String>,
                f32,
                Option<String>,
                String,
            ),
        >(&query);

        if let Some(et) = entity_type {
            query_builder = query_builder.bind(et);
        }
        if let Some(t) = theme {
            query_builder = query_builder.bind(t);
        }

        query_builder = query_builder.bind(limit);

        let rows = query_builder.fetch_all(&self.pool).await?;

        let entities: Vec<Entity> = rows
            .into_iter()
            .map(
                |(
                    id,
                    activity_id,
                    entity_type,
                    name,
                    metadata,
                    confidence,
                    theme,
                    created_at_str,
                )| {
                    Entity {
                        id,
                        activity_id,
                        entity_type,
                        name,
                        metadata,
                        confidence,
                        theme,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    }
                },
            )
            .collect();

        Ok(entities)
    }

    /// Get entities by type
    pub async fn get_entities_by_type(
        &self,
        entity_type: &str,
        limit: i32,
    ) -> Result<Vec<Entity>, sqlx::Error> {
        self.list_entities(Some(limit), Some(entity_type), None)
            .await
    }

    /// Get entities for a specific activity
    pub async fn get_entities_for_activity(
        &self,
        activity_id: i64,
    ) -> Result<Vec<Entity>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (Option<i64>, i64, String, String, Option<String>, f32, Option<String>, String)>(
            "SELECT id, activity_id, entity_type, name, metadata, confidence, theme, created_at FROM entities WHERE activity_id = ? ORDER BY created_at DESC"
        )
        .bind(activity_id)
        .fetch_all(&self.pool)
        .await?;

        let entities: Vec<Entity> = rows
            .into_iter()
            .map(
                |(
                    id,
                    activity_id,
                    entity_type,
                    name,
                    metadata,
                    confidence,
                    theme,
                    created_at_str,
                )| {
                    Entity {
                        id,
                        activity_id,
                        entity_type,
                        name,
                        metadata,
                        confidence,
                        theme,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    }
                },
            )
            .collect();

        Ok(entities)
    }

    // ============================================
    // Meeting Intelligence System Methods
    // ============================================

    /// Save AI-generated meeting notes
    pub async fn save_meeting_notes(
        &self,
        id: &str,
        meeting_id: &str,
        summary: Option<&str>,
        key_topics: Option<&str>,
        decisions: Option<&str>,
        action_items: Option<&str>,
        participants: Option<&str>,
        model_used: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO meeting_notes 
            (id, meeting_id, summary, key_topics, decisions, action_items, participants, model_used, generated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(id)
        .bind(meeting_id)
        .bind(summary)
        .bind(key_topics)
        .bind(decisions)
        .bind(action_items)
        .bind(participants)
        .bind(model_used)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get meeting notes by meeting ID
    pub async fn get_meeting_notes(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MeetingNotes>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String, Option<String>, i64)>(
            "SELECT id, meeting_id, summary, key_topics, decisions, action_items, participants, generated_at, model_used, stale_after_edit FROM meeting_notes WHERE meeting_id = ? ORDER BY generated_at DESC LIMIT 1"
        )
        .bind(meeting_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                meeting_id,
                summary,
                key_topics,
                decisions,
                action_items,
                participants,
                generated_at,
                model_used,
                stale,
            )| {
                MeetingNotes {
                    id,
                    meeting_id,
                    summary,
                    key_topics,
                    decisions,
                    action_items,
                    participants,
                    generated_at: DateTime::parse_from_rfc3339(&generated_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    model_used,
                    stale_after_edit: stale != 0,
                }
            },
        ))
    }

    /// Add a comment to a meeting
    pub async fn add_meeting_comment(
        &self,
        id: &str,
        meeting_id: &str,
        comment: &str,
        comment_type: Option<&str>,
        timestamp_ref: Option<f64>,
        parent_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO meeting_comments (id, meeting_id, comment, comment_type, timestamp_ref, parent_id)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(meeting_id)
        .bind(comment)
        .bind(comment_type.unwrap_or("note"))
        .bind(timestamp_ref)
        .bind(parent_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get comments for a meeting
    pub async fn get_meeting_comments(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<MeetingComment>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String, Option<f64>, String, Option<String>, Option<String>)>(
            "SELECT id, meeting_id, user_id, comment, comment_type, timestamp_ref, created_at, updated_at, parent_id FROM meeting_comments WHERE meeting_id = ? ORDER BY created_at ASC"
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    meeting_id,
                    user_id,
                    comment,
                    comment_type,
                    timestamp_ref,
                    created_at,
                    updated_at,
                    parent_id,
                )| {
                    MeetingComment {
                        id,
                        meeting_id,
                        user_id,
                        comment,
                        comment_type,
                        timestamp_ref,
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        updated_at: updated_at.and_then(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        }),
                        parent_id,
                    }
                },
            )
            .collect())
    }

    /// Save study materials (Dork Mode)
    pub async fn save_study_materials(
        &self,
        id: &str,
        meeting_id: &str,
        summary: Option<&str>,
        key_concepts: Option<&str>,
        quiz_questions: Option<&str>,
        flashcards: Option<&str>,
        model_used: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO study_materials 
            (id, meeting_id, summary, key_concepts, quiz_questions, flashcards, model_used, generated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(id)
        .bind(meeting_id)
        .bind(summary)
        .bind(key_concepts)
        .bind(quiz_questions)
        .bind(flashcards)
        .bind(model_used)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get study materials for a meeting
    pub async fn get_study_materials(
        &self,
        meeting_id: &str,
    ) -> Result<Option<StudyMaterialsRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Option<String>, String, Option<String>)>(
            "SELECT id, meeting_id, summary, key_concepts, quiz_questions, flashcards, generated_at, model_used FROM study_materials WHERE meeting_id = ? ORDER BY generated_at DESC LIMIT 1"
        )
        .bind(meeting_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                meeting_id,
                summary,
                key_concepts,
                quiz_questions,
                flashcards,
                generated_at,
                model_used,
            )| {
                StudyMaterialsRecord {
                    id,
                    meeting_id,
                    summary,
                    key_concepts,
                    quiz_questions,
                    flashcards,
                    generated_at: DateTime::parse_from_rfc3339(&generated_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    model_used,
                }
            },
        ))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod schema_drift_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Create a temporary database for testing
    async fn test_db() -> (DatabaseManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!("nf-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");
        let db = DatabaseManager::new(&db_path).await.unwrap();
        db.run_migrations().await.unwrap();
        (db, dir)
    }

    /// Clean up temp directory
    fn cleanup(dir: PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    // ─── Initialization ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_database_creates_and_migrates() {
        let (db, dir) = test_db().await;
        // If we get here, creation and migrations succeeded
        // Verify by listing meetings (should return empty)
        let meetings = db.list_meetings(10).await.unwrap();
        assert!(meetings.is_empty());
        cleanup(dir);
    }

    // ─── Meeting CRUD ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_and_get_meeting() {
        let (db, dir) = test_db().await;

        let meeting = db.create_meeting("test-1", "Daily Standup").await.unwrap();
        assert_eq!(meeting.id, "test-1");
        assert_eq!(meeting.title, "Daily Standup");
        assert!(meeting.ended_at.is_none());

        let fetched = db.get_meeting("test-1").await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().title, "Daily Standup");

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_get_nonexistent_meeting_returns_none() {
        let (db, dir) = test_db().await;
        let result = db.get_meeting("nonexistent").await.unwrap();
        assert!(result.is_none());
        cleanup(dir);
    }

    #[tokio::test]
    async fn test_list_meetings_returns_recent_first() {
        let (db, dir) = test_db().await;

        db.create_meeting("m1", "First Meeting").await.unwrap();
        // Small delay to ensure ordering
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.create_meeting("m2", "Second Meeting").await.unwrap();

        let meetings = db.list_meetings(10).await.unwrap();
        assert_eq!(meetings.len(), 2);
        // Most recent first
        assert_eq!(meetings[0].id, "m2");
        assert_eq!(meetings[1].id, "m1");

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_update_meeting_title() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Original").await.unwrap();
        db.update_meeting_title("m1", "Updated Title").await.unwrap();

        let meeting = db.get_meeting("m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Updated Title");

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_end_meeting_sets_timestamp_and_duration() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Test").await.unwrap();

        // Wait a brief moment so duration > 0
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        db.end_meeting("m1").await.unwrap();

        let meeting = db.get_meeting("m1").await.unwrap().unwrap();
        assert!(meeting.ended_at.is_some());
        assert!(meeting.duration_seconds.is_some());
        assert!(meeting.duration_seconds.unwrap() >= 0);

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_delete_meeting() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Doomed").await.unwrap();
        db.delete_meeting("m1").await.unwrap();

        let result = db.get_meeting("m1").await.unwrap();
        assert!(result.is_none());

        cleanup(dir);
    }

    // ─── Transcripts ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_and_get_transcripts() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Test").await.unwrap();

        db.add_transcript("m1", "Hello world", Some("Alice"), true, 0.95)
            .await
            .unwrap();
        db.add_transcript("m1", "Hi there", Some("Bob"), true, 0.88)
            .await
            .unwrap();

        let transcripts = db.get_transcripts("m1").await.unwrap();
        assert_eq!(transcripts.len(), 2);
        assert_eq!(transcripts[0].text, "Hello world");
        assert_eq!(transcripts[0].speaker.as_deref(), Some("Alice"));
        assert!(transcripts[0].is_final);

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_search_transcripts() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Design Review").await.unwrap();

        db.add_transcript("m1", "We should use Kubernetes for the deployment", None, true, 0.9)
            .await
            .unwrap();
        db.add_transcript("m1", "The frontend needs a complete redesign", None, true, 0.9)
            .await
            .unwrap();

        let results = db.search_transcripts("kubernetes").await.unwrap();
        assert!(
            !results.is_empty(),
            "Search should find transcript containing 'kubernetes'"
        );

        cleanup(dir);
    }

    #[tokio::test]
    async fn test_search_transcript_context_ranks_and_snips() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Infra Sync").await.unwrap();
        db.create_meeting("m2", "Design Review").await.unwrap();
        db.add_transcript("m1", "We will migrate the cluster to Kubernetes next quarter", None, true, 0.9)
            .await
            .unwrap();
        db.add_transcript("m2", "The frontend needs a complete redesign", None, true, 0.9)
            .await
            .unwrap();

        let q = fts_or_query("What did we decide about Kubernetes?").unwrap();
        assert_eq!(q, "\"decide\" OR \"kubernetes\"");
        let hits = db.search_transcript_context(&q, 5).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].meeting_title, "Infra Sync");
        assert!(hits[0].snippet.to_lowercase().contains("kubernetes"));

        assert!(fts_or_query("the and?!").is_none());

        db.add_assistant_conversation("c1", "2026-01-01T00:00:00Z", "q", "a", "m", &["x".into()])
            .await
            .unwrap();
        let convs = db.list_assistant_conversations(10).await.unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].5, vec!["x".to_string()]);

        cleanup(dir);
    }

    // ─── Frames ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_and_count_frames() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Test").await.unwrap();

        db.add_frame("m1", Utc::now(), None, None).await.unwrap();
        db.add_frame("m1", Utc::now(), Some("/tmp/frame2.png"), None).await.unwrap();

        let count = db.count_frames("m1").await.unwrap();
        assert_eq!(count, 2);

        let frames = db.get_frames("m1", 10).await.unwrap();
        assert_eq!(frames.len(), 2);

        cleanup(dir);
    }

    // ─── Meeting Notes ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_save_and_get_meeting_notes() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Test").await.unwrap();

        db.save_meeting_notes(
            "notes-1",
            "m1",
            Some("This was a productive meeting"),
            Some("[\"architecture\",\"deployment\"]"),
            Some("[\"Use K8s\"]"),
            Some("[\"Update docs\"]"),
            Some("[\"Alice\",\"Bob\"]"),
            None,
        )
        .await
        .unwrap();

        let notes = db.get_meeting_notes("m1").await.unwrap();
        assert!(notes.is_some());
        let notes = notes.unwrap();
        assert_eq!(notes.summary.as_deref(), Some("This was a productive meeting"));
        assert!(notes.key_topics.is_some());

        cleanup(dir);
    }

    // ─── Meeting Comments ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_and_get_comments() {
        let (db, dir) = test_db().await;
        db.create_meeting("m1", "Test").await.unwrap();

        db.add_meeting_comment("c1", "m1", "Great point about security", Some("note"), None, None)
            .await
            .unwrap();

        let comments = db.get_meeting_comments("m1").await.unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].comment, "Great point about security");
        assert_eq!(comments[0].comment_type, "note");

        cleanup(dir);
    }

    // ─── Meeting-end housekeeping ───────────────────────────────────────

    #[tokio::test]
    async fn test_close_stale_meetings_uses_last_capture() {
        let (db, dir) = test_db().await;
        db.create_meeting("open", "Left open").await.unwrap();
        db.create_meeting("empty", "No content").await.unwrap();
        db.create_meeting("done", "Closed").await.unwrap();
        db.end_meeting("done").await.unwrap();
        let start = db.get_meeting("open").await.unwrap().unwrap().started_at;
        let last = start + chrono::Duration::minutes(42);
        db.add_transcript_at("open", "real words", None, true, 0.9, start + chrono::Duration::minutes(5))
            .await
            .unwrap();
        db.add_transcript_at("open", "later words", None, true, 0.9, last).await.unwrap();

        assert_eq!(db.close_stale_meetings().await.unwrap(), 2);
        let m = db.get_meeting("open").await.unwrap().unwrap();
        assert_eq!(m.ended_at.map(|e| e.timestamp()), Some(last.timestamp()));
        assert_eq!(m.duration_seconds, Some(42 * 60));
        let e = db.get_meeting("empty").await.unwrap().unwrap();
        assert_eq!(e.duration_seconds, Some(0));
        // Content untouched, and a second run is a no-op
        assert_eq!(db.get_transcripts("open").await.unwrap().len(), 2);
        assert_eq!(db.close_stale_meetings().await.unwrap(), 0);
        cleanup(dir);
    }

    #[tokio::test]
    async fn test_delete_junk_after_end_point_only() {
        let (db, dir) = test_db().await;
        db.create_meeting("m", "M").await.unwrap();
        let t = Utc::now();
        let s = chrono::Duration::seconds;
        db.add_transcript_at("m", "Bye-bye.", None, true, 0.9, t - s(60)).await.unwrap(); // before end
        db.add_transcript_at("m", "Thanks all, talk Thursday.", None, true, 0.9, t + s(5)).await.unwrap();
        db.add_transcript_at("m", "Bye-bye. Bye-bye.", None, true, 0.9, t + s(40)).await.unwrap();
        db.add_transcript_at("m", "you you you", None, true, 0.9, t + s(80)).await.unwrap();
        let n = db
            .delete_transcripts_after_matching("m", t, crate::transcription::filter::is_junk)
            .await
            .unwrap();
        assert_eq!(n, 2);
        let left: Vec<String> = db.get_transcripts("m").await.unwrap().into_iter().map(|t| t.text).collect();
        assert_eq!(left, vec!["Bye-bye.".to_string(), "Thanks all, talk Thursday.".to_string()]);
        cleanup(dir);
    }
}
