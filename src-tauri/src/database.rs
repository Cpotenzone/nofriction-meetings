// noFriction Meetings - Database Manager
// SQLite storage for meetings, transcripts, and full-text search

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite, Row};
use std::path::Path;

/// Meeting record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
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

/// Database manager
pub struct DatabaseManager {
    pool: Pool<Sqlite>,
}

impl DatabaseManager {
    /// Get a reference to the connection pool
    pub fn get_pool(&self) -> std::sync::Arc<Pool<Sqlite>> {
        std::sync::Arc::new(self.pool.clone())
    }

    /// Create a new database manager
    pub async fn new(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

        Ok(Self { pool })
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create tables
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER
            )
        "#)
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS transcripts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                text TEXT NOT NULL,
                speaker TEXT,
                timestamp TEXT NOT NULL,
                is_final INTEGER NOT NULL DEFAULT 1,
                confidence REAL NOT NULL DEFAULT 0.0
            )
        "#)
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS frames (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                frame_number INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL,
                file_path TEXT,
                ocr_text TEXT
            )
        "#)
        .execute(&self.pool)
        .await?;

        // Add columns if they don't exist (for migration from old schema)
        let _ = sqlx::query("ALTER TABLE transcripts ADD COLUMN text_hash TEXT")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE frames ADD COLUMN frame_number INTEGER DEFAULT 0")
            .execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE frames ADD COLUMN file_path TEXT")
            .execute(&self.pool).await;

        // Create unique index on text_hash for deduplication
        let _ = sqlx::query(r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_transcripts_hash ON transcripts(meeting_id, text_hash)
        "#)
        .execute(&self.pool)
        .await;

        // Create full-text search virtual tables
        sqlx::query(r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts 
            USING fts5(text, meeting_id, content='transcripts', content_rowid='id')
        "#)
        .execute(&self.pool)
        .await?;

        // Create triggers to keep FTS in sync
        sqlx::query(r#"
            CREATE TRIGGER IF NOT EXISTS transcripts_ai AFTER INSERT ON transcripts BEGIN
                INSERT INTO transcripts_fts(rowid, text, meeting_id) 
                VALUES (new.id, new.text, new.meeting_id);
            END
        "#)
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"
            CREATE TRIGGER IF NOT EXISTS transcripts_ad AFTER DELETE ON transcripts BEGIN
                INSERT INTO transcripts_fts(transcripts_fts, rowid, text, meeting_id) 
                VALUES ('delete', old.id, old.text, old.meeting_id);
            END
        "#)
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id)
        "#)
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings(started_at)
        "#)
        .execute(&self.pool)
        .await?;

        // Knowledge Base tables - frame_queue for VLM analysis
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS frame_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                frame_path TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                analyzed INTEGER NOT NULL DEFAULT 0,
                synced INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#)
        .execute(&self.pool)
        .await?;

        // Knowledge Base tables - activity_log for analyzed activities
        sqlx::query(r#"
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
                pinecone_id TEXT,
                supabase_id TEXT,
                synced_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#)
        .execute(&self.pool)
        .await?;

        // Indexes for new tables
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_frame_queue_analyzed ON frame_queue(analyzed)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_frame_queue_synced ON frame_queue(synced)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activity_log_start ON activity_log(start_time)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activity_log_category ON activity_log(category)")
            .execute(&self.pool).await;

        // Theme activity tracking table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS theme_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                theme TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#)
        .execute(&self.pool)
        .await?;

        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_theme_sessions_theme ON theme_sessions(theme)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_theme_sessions_started ON theme_sessions(started_at)")
            .execute(&self.pool).await;

        // Phase 3: Entities table for structured entity extraction
        sqlx::query(r#"
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
        "#)
        .execute(&self.pool)
        .await?;

        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_theme ON entities(theme)")
            .execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_entities_activity ON entities(activity_id)")
            .execute(&self.pool).await;

        log::info!("Database migrations completed");
        Ok(())
    }

    /// Create a new meeting
    pub async fn create_meeting(&self, id: &str, title: &str) -> Result<Meeting, sqlx::Error> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO meetings (id, title, started_at) VALUES (?, ?, ?)"
        )
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
        })
    }

    /// End a meeting
    pub async fn end_meeting(&self, id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Get the start time to calculate duration
        let row: (String,) = sqlx::query_as(
            "SELECT started_at FROM meetings WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        let started_at = DateTime::parse_from_rfc3339(&row.0)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);

        let duration = (now - started_at).num_seconds();

        sqlx::query(
            "UPDATE meetings SET ended_at = ?, duration_seconds = ? WHERE id = ?"
        )
        .bind(&now_str)
        .bind(duration)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a meeting by ID
    pub async fn get_meeting(&self, id: &str) -> Result<Option<Meeting>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, title, started_at, ended_at, duration_seconds FROM meetings WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Meeting {
            id: r.get("id"),
            title: r.get("title"),
            started_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            ended_at: r.get::<Option<String>, _>("ended_at")
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            duration_seconds: r.get("duration_seconds"),
        }))
    }

    /// List all meetings
    pub async fn list_meetings(&self, limit: i32) -> Result<Vec<Meeting>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, title, started_at, ended_at, duration_seconds 
             FROM meetings ORDER BY started_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Meeting {
            id: r.get("id"),
            title: r.get("title"),
            started_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            ended_at: r.get::<Option<String>, _>("ended_at")
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            duration_seconds: r.get("duration_seconds"),
        }).collect())
    }

    /// Delete a meeting and its transcripts
    pub async fn delete_meeting(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

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
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        
        // Only deduplicate final transcripts
        if is_final && !text.trim().is_empty() {
            // Create hash of text content (normalized)
            let normalized_text = text.trim().to_lowercase();
            let mut hasher = DefaultHasher::new();
            normalized_text.hash(&mut hasher);
            let text_hash = format!("{:016x}", hasher.finish());
            
            // Check if this exact transcript already exists within last 30 seconds
            let thirty_secs_ago = (now - chrono::Duration::seconds(30)).to_rfc3339();
            
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM transcripts 
                 WHERE meeting_id = ? AND text_hash = ? AND timestamp > ?
                 LIMIT 1"
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
                "INSERT INTO transcripts (meeting_id, text, speaker, timestamp, is_final, confidence, text_hash) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(meeting_id)
            .bind(text)
            .bind(speaker)
            .bind(&now_str)
            .bind(is_final as i32)
            .bind(confidence)
            .bind(&text_hash)
            .execute(&self.pool)
            .await?;
            
            return Ok(result.last_insert_rowid());
        }
        
        // Non-final (interim) transcripts - no deduplication needed
        let result = sqlx::query(
            "INSERT INTO transcripts (meeting_id, text, speaker, timestamp, is_final, confidence) 
             VALUES (?, ?, ?, ?, ?, ?)"
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

    /// Get transcripts for a meeting
    pub async fn get_transcripts(&self, meeting_id: &str) -> Result<Vec<Transcript>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, meeting_id, text, speaker, timestamp, is_final, confidence 
             FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC"
        )
        .bind(meeting_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Transcript {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            text: r.get("text"),
            speaker: r.get("speaker"),
            timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            is_final: r.get::<i32, _>("is_final") == 1,
            confidence: r.get("confidence"),
        }).collect())
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
            "#
        )
        .bind(query)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| SearchResult {
            meeting_id: r.get("meeting_id"),
            meeting_title: r.get("meeting_title"),
            transcript_text: r.get("transcript_text"),
            timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            relevance: r.get("relevance"),
        }).collect())
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
        let frame_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM frames WHERE meeting_id = ?"
        )
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
    pub async fn get_frames(&self, meeting_id: &str, limit: i32) -> Result<Vec<Frame>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, meeting_id, frame_number, timestamp, file_path, ocr_text 
             FROM frames WHERE meeting_id = ? ORDER BY timestamp ASC LIMIT ?"
        )
        .bind(meeting_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Frame {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            frame_number: r.try_get("frame_number").unwrap_or(0),
            timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            file_path: r.try_get("file_path").ok(),
            ocr_text: r.try_get("ocr_text").ok(),
        }).collect())
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
             ORDER BY timestamp ASC"
        )
        .bind(meeting_id)
        .bind(&start_str)
        .bind(&end_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Frame {
            id: r.get("id"),
            meeting_id: r.get("meeting_id"),
            frame_number: r.try_get("frame_number").unwrap_or(0),
            timestamp: DateTime::parse_from_rfc3339(&r.get::<String, _>("timestamp"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            file_path: r.try_get("file_path").ok(),
            ocr_text: r.try_get("ocr_text").ok(),
        }).collect())
    }

    /// Get the most recent frame for a meeting
    pub async fn get_latest_frame(&self, meeting_id: &str) -> Result<Option<Frame>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, meeting_id, frame_number, timestamp, file_path, ocr_text 
             FROM frames WHERE meeting_id = ? ORDER BY timestamp DESC LIMIT 1"
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
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM frames WHERE meeting_id = ?"
        )
        .bind(meeting_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }
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
    pub duration_seconds: i64,
    pub frames: Vec<TimelineFrame>,
    pub transcripts: Vec<TimelineTranscript>,
}

/// Frame on the timeline (simplified for UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineFrame {
    pub id: String,
    pub frame_number: i64,
    pub timestamp_ms: i64,  // Milliseconds from start of meeting
    pub thumbnail_path: Option<String>,
}

/// Transcript on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTranscript {
    pub id: String,
    pub timestamp_ms: i64,  // Milliseconds from start of meeting
    pub text: String,
    pub speaker: Option<String>,
    pub is_final: bool,
    pub duration_seconds: f64,
}

impl DatabaseManager {
    /// Get synced timeline for a meeting (frames + transcripts aligned)
    pub async fn get_synced_timeline(&self, meeting_id: &str) -> Result<Option<SyncedTimeline>, sqlx::Error> {
        // Get meeting info
        let meeting = match self.get_meeting(meeting_id).await? {
            Some(m) => m,
            None => return Ok(None),
        };

        let start_time = meeting.started_at;
        let duration = meeting.duration_seconds.unwrap_or(0);

        // Get frames
        let frames = self.get_frames(meeting_id, 10000).await?;
        let timeline_frames: Vec<TimelineFrame> = frames
            .into_iter()
            .map(|f| {
                let ms = (f.timestamp - start_time).num_milliseconds();
                TimelineFrame {
                    id: f.id.to_string(),
                    frame_number: f.frame_number,
                    timestamp_ms: ms.max(0),
                    thumbnail_path: f.file_path,
                }
            })
            .collect();

        // Get transcripts
        let transcripts = self.get_transcripts(meeting_id).await?;
        let timeline_transcripts: Vec<TimelineTranscript> = transcripts
            .into_iter()
            .map(|t| {
                let ms = (t.timestamp - start_time).num_milliseconds();
                TimelineTranscript {
                    id: t.id.to_string(),
                    timestamp_ms: ms.max(0),
                    text: t.text,
                    speaker: t.speaker,
                    is_final: t.is_final,
                    duration_seconds: 0.0, // TODO: Store actual duration from Deepgram
                }
            })
            .collect();

        Ok(Some(SyncedTimeline {
            meeting_id: meeting_id.to_string(),
            meeting_title: meeting.title,
            duration_seconds: duration,
            frames: timeline_frames,
            transcripts: timeline_transcripts,
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
    pub pinecone_id: Option<String>,
    pub supabase_id: Option<String>,
    pub synced_at: Option<DateTime<Utc>>,
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
             VALUES (?, ?, ?, ?, ?, ?)"
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
             FROM entities WHERE activity_id = ?"
        )
        .bind(activity_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Entity {
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
        }).collect())
    }

    /// Get recent extracted entities (filtered by theme/type)
    pub async fn get_recent_entities(&self, limit: i32) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let sql = r#"
            SELECT 
                e.id, e.entity_type, e.name, e.confidence, e.activity_id,
                e.theme, e.created_at, e.metadata,
                a.app_name, a.window_title, a.start_time
            FROM entities e
            JOIN activities a ON e.activity_id = a.id
            ORDER BY a.start_time DESC
            LIMIT ?
        "#;

        let rows = sqlx::query(sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let result = rows.into_iter().map(|row| {
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
            let metadata_obj: Option<serde_json::Value> = metadata_str
                .and_then(|s| serde_json::from_str(&s).ok());

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
        }).collect();

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
            "INSERT INTO frame_queue (frame_id, frame_path, captured_at) VALUES (?, ?, ?)"
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
             FROM frame_queue WHERE analyzed = 0 ORDER BY captured_at ASC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| FrameQueueItem {
            id: r.get("id"),
            frame_id: r.get("frame_id"),
            frame_path: r.get("frame_path"),
            captured_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("captured_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            analyzed: r.get::<i32, _>("analyzed") == 1,
            synced: r.get::<i32, _>("synced") == 1,
        }).collect())
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
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM frame_queue WHERE synced = 0"
        )
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
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
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

        Ok(rows.into_iter().map(|r| ActivityLogEntry {
            id: Some(r.get("id")),
            start_time: DateTime::parse_from_rfc3339(&r.get::<String, _>("start_time"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            end_time: r.get::<Option<String>, _>("end_time")
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
            pinecone_id: r.get("pinecone_id"),
            supabase_id: r.get("supabase_id"),
            synced_at: r.get::<Option<String>, _>("synced_at")
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        }).collect())
    }

    /// Get unsynced activities
    pub async fn get_unsynced_activities(&self, limit: i32) -> Result<Vec<ActivityLogEntry>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM activity_log WHERE synced_at IS NULL ORDER BY start_time ASC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| ActivityLogEntry {
            id: Some(r.get("id")),
            start_time: DateTime::parse_from_rfc3339(&r.get::<String, _>("start_time"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            end_time: r.get::<Option<String>, _>("end_time")
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
            pinecone_id: r.get("pinecone_id"),
            supabase_id: r.get("supabase_id"),
            synced_at: None,
        }).collect())
    }

    /// Update activity with sync info
    pub async fn mark_activity_synced(
        &self,
        activity_id: i64,
        pinecone_id: Option<&str>,
        supabase_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let synced_at = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE activity_log SET pinecone_id = ?, supabase_id = ?, synced_at = ? WHERE id = ?"
        )
        .bind(pinecone_id)
        .bind(supabase_id)
        .bind(&synced_at)
        .bind(activity_id)
        .execute(&self.pool)
        .await?;

        Ok(())
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
               ORDER BY total_seconds DESC"#
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        let mut stats = serde_json::Map::new();
        for row in rows {
            let category: String = row.get("category");
            let count: i64 = row.get("count");
            let seconds: Option<i64> = row.get("total_seconds");
            stats.insert(category, serde_json::json!({
                "count": count,
                "total_seconds": seconds.unwrap_or(0)
            }));
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
        
        Ok(rows.into_iter().map(|r| ActivityLogEntry {
            id: Some(r.get("id")),
            start_time: DateTime::parse_from_rfc3339(&r.get::<String, _>("start_time"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            end_time: r.get::<Option<String>, _>("end_time")
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
            pinecone_id: r.get("pinecone_id"),
            supabase_id: r.get("supabase_id"),
            synced_at: r.get::<Option<String>, _>("synced_at")
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        }).collect())
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
            "SELECT started_at FROM theme_sessions WHERE id = ? AND ended_at IS NULL"
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((started_at_str,)) = row {
            if let Ok(started_at) = DateTime::parse_from_rfc3339(&started_at_str) {
                let duration = (now.timestamp() - started_at.timestamp()) as i32;
                
                sqlx::query("UPDATE theme_sessions SET ended_at = ?, duration_seconds = ? WHERE id = ?")
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
        let today_start_str = DateTime::<Utc>::from_naive_utc_and_offset(today_start, Utc).to_rfc3339();
        
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
            "SELECT id FROM theme_sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1"
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

        let mut query_builder = sqlx::query_as::<_, (Option<i64>, i64, String, String, Option<String>, f32, Option<String>, String)>(&query);
        
        if let Some(et) = entity_type {
            query_builder = query_builder.bind(et);
        }
        if let Some(t) = theme {
            query_builder = query_builder.bind(t);
        }
        
        query_builder = query_builder.bind(limit);

        let rows = query_builder.fetch_all(&self.pool).await?;

        let entities: Vec<Entity> = rows.into_iter().map(|(id, activity_id, entity_type, name, metadata, confidence, theme, created_at_str)| {
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
        }).collect();

        Ok(entities)
    }

    /// Get entities by type
    pub async fn get_entities_by_type(&self, entity_type: &str, limit: i32) -> Result<Vec<Entity>, sqlx::Error> {
        self.list_entities(Some(limit), Some(entity_type), None).await
    }

    /// Get entities for a specific activity
    pub async fn get_entities_for_activity(&self, activity_id: i64) -> Result<Vec<Entity>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (Option<i64>, i64, String, String, Option<String>, f32, Option<String>, String)>(
            "SELECT id, activity_id, entity_type, name, metadata, confidence, theme, created_at FROM entities WHERE activity_id = ? ORDER BY created_at DESC"
        )
        .bind(activity_id)
        .fetch_all(&self.pool)
        .await?;

        let entities: Vec<Entity> = rows.into_iter().map(|(id, activity_id, entity_type, name, metadata, confidence, theme, created_at_str)| {
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
        }).collect();

        Ok(entities)
    }
}
