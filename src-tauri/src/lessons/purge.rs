//! Purge of recording-derived (`generated`) lesson sets (docs/LESSONS.md
//! §11.4). A generated set paraphrases a recording's transcript, so on any
//! Delete/Strike of that transcript text it is DELETED, not rewritten — exactly
//! like a study guide. Seed and imported sets are NEVER touched (the
//! `WHERE provenance = 'generated'` guard). Generated sets are written by
//! Make lesson (`lessons::generate`, §13).
//!
//! Ordering (idempotent, keyed by meeting_id, safe under the non-transactional
//! `delete_meeting`): resolve dir_paths first, delete learner rows then
//! lesson_sets rows, remove on-disk dirs last tolerating ENOENT.

use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

/// Rows a purge would delete (for the Delete/Strike preview). Schema-tolerant.
pub async fn count_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> i64 {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lesson_sets'")
            .fetch_optional(&mut *conn)
            .await
            .unwrap_or(None);
    if exists.is_none() {
        return 0;
    }
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM lesson_sets WHERE source_recording_id = ? AND provenance = 'generated'",
    )
    .bind(meeting_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap_or(0)
}

/// Delete every `generated` lesson set derived from `meeting_id`: its learner
/// records and catalog rows in the DB, then its on-disk dirs. Runs on the
/// caller's connection/transaction (live hook, backup hook, delete cascade).
/// Returns the number of DB rows affected. Schema-tolerant: an old backup may
/// lack the tables.
pub async fn purge_for_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<u64, sqlx::Error> {
    let (changed, dirs) = purge_rows_for_meeting(conn, meeting_id).await?;
    remove_dirs(&dirs);
    Ok(changed)
}

/// Steps 1 and 2 of the purge: resolve the generated sets' dir paths, then
/// delete their learner rows and catalog rows. Returns (rows affected, the
/// on-disk dirs to remove once the caller's transaction is safe). The lesson
/// maker uses this inside its save transaction to replace the previous
/// generated set for a recording, removing the old dirs only after commit.
pub(crate) async fn purge_rows_for_meeting(
    conn: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<(u64, Vec<String>), sqlx::Error> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lesson_sets'")
            .fetch_optional(&mut *conn)
            .await?;
    if exists.is_none() {
        return Ok((0, Vec::new()));
    }

    // 1. Resolve first: ids + dir paths, BEFORE deleting any row.
    let rows =
        sqlx::query("SELECT id, dir_path FROM lesson_sets WHERE source_recording_id = ? AND provenance = 'generated'")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await?;
    if rows.is_empty() {
        return Ok((0, Vec::new()));
    }
    let targets: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("dir_path")))
        .collect();

    // 2. Delete learner rows then the catalog rows (keyed by set id).
    let mut changed = 0u64;
    for (id, _) in &targets {
        for table in ["lesson_progress", "mastery_records", "exam_attempts", "certificates"] {
            // Tables all exist via ensure_schema; a stale backup may not.
            let has: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?")
                    .bind(table)
                    .fetch_optional(&mut *conn)
                    .await?;
            if has.is_none() {
                continue;
            }
            changed += sqlx::query(&format!("DELETE FROM {} WHERE lesson_set_id = ?", table))
                .bind(id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
        }
    }
    changed += sqlx::query("DELETE FROM lesson_sets WHERE source_recording_id = ? AND provenance = 'generated'")
        .bind(meeting_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();

    Ok((changed, targets.into_iter().map(|(_, dir)| dir).collect()))
}

/// Step 3: remove on-disk dirs last, tolerating ENOENT. A dir-removal failure
/// after the DB delete is logged-and-ignored (orphaned bytes are harmless).
pub(crate) fn remove_dirs(dirs: &[String]) {
    for dir_path in dirs {
        if dir_path.is_empty() {
            continue;
        }
        match std::fs::remove_dir_all(dir_path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("Lessons: couldn't remove on-disk dir after purge: {}", e),
        }
    }
}
