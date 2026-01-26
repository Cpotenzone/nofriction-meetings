// noFriction Meetings - Ingest Queue
// Local durable queue for upload retries

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueuedItem {
    Frame {
        session_id: Uuid,
        captured_at: String,
        image_path: PathBuf,
        sha256: Option<String>,
    },
    Transcript {
        session_id: Uuid,
        segments_json: String,
    },
}

#[derive(Debug)]
pub struct IngestQueue {
    conn: Arc<Mutex<Connection>>,
}

impl IngestQueue {
    pub fn new(db_path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ingest_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                retries INTEGER DEFAULT 0,
                max_retries INTEGER DEFAULT 3,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                last_attempt_at TEXT,
                last_error TEXT
            )",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Enqueue a frame for upload
    pub fn enqueue_frame(
        &self,
        session_id: Uuid,
        captured_at: String,
        image_path: PathBuf,
        sha256: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let item = QueuedItem::Frame {
            session_id,
            captured_at,
            image_path,
            sha256,
        };

        let payload = serde_json::to_string(&item)?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ingest_queue (item_type, payload) VALUES (?1, ?2)",
            params!["frame", payload],
        )?;

        Ok(())
    }

    /// Enqueue transcript segments for upload
    pub fn enqueue_transcript(
        &self,
        session_id: Uuid,
        segments_json: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let item = QueuedItem::Transcript {
            session_id,
            segments_json,
        };

        let payload = serde_json::to_string(&item)?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ingest_queue (item_type, payload) VALUES (?1, ?2)",
            params!["transcript", payload],
        )?;

        Ok(())
    }

    /// Get next pending item
    pub fn get_next_pending(
        &self,
    ) -> Result<Option<(i64, QueuedItem)>, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, payload FROM ingest_queue 
             WHERE retries < max_retries 
             ORDER BY created_at ASC 
             LIMIT 1",
        )?;

        let result = stmt.query_row([], |row| {
            let id: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            Ok((id, payload))
        });

        match result {
            Ok((id, payload)) => {
                let item: QueuedItem = serde_json::from_str(&payload)?;
                Ok(Some((id, item)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Mark item as successfully processed
    pub fn mark_completed(&self, id: i64) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM ingest_queue WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Mark item as failed and increment retry count
    pub fn mark_failed(&self, id: i64, error: &str) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE ingest_queue 
             SET retries = retries + 1, 
                 last_attempt_at = CURRENT_TIMESTAMP,
                 last_error = ?1
             WHERE id = ?2",
            params![error, id],
        )?;
        Ok(())
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();

        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM ingest_queue WHERE retries < max_retries",
            [],
            |row| row.get(0),
        )?;

        let failed: usize = conn.query_row(
            "SELECT COUNT(*) FROM ingest_queue WHERE retries >= max_retries",
            [],
            |row| row.get(0),
        )?;

        Ok((pending, failed))
    }

    /// Clear all failed items
    pub fn clear_failed(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM ingest_queue WHERE retries >= max_retries", [])?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_queue_operations() {
        let temp_file = NamedTempFile::new().unwrap();
        let queue = IngestQueue::new(&temp_file.path().to_path_buf()).unwrap();

        // Enqueue a frame
        let session_id = Uuid::new_v4();
        queue
            .enqueue_frame(
                session_id,
                "2024-01-01T00:00:00Z".to_string(),
                PathBuf::from("/tmp/test.jpg"),
                Some("abc123".to_string()),
            )
            .unwrap();

        // Get stats
        let (pending, failed) = queue.get_stats().unwrap();
        assert_eq!(pending, 1);
        assert_eq!(failed, 0);

        // Get next item
        let item = queue.get_next_pending().unwrap();
        assert!(item.is_some());

        let (id, _) = item.unwrap();

        // Mark completed
        queue.mark_completed(id).unwrap();

        let (pending, _) = queue.get_stats().unwrap();
        assert_eq!(pending, 0);
    }
}
