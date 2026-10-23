//! Purge of recording-derived (`generated`) lesson sets (docs/LESSONS.md
//! §11.4). A generated set paraphrases a recording's transcript, so on any
//! Delete/Strike of that transcript text it is DELETED, not rewritten — exactly
//! like a study guide. Seed and imported sets are NEVER touched (the
//! `WHERE provenance = 'generated'` guard). Generated sets are written by
//! Make lesson (`lessons::generate`, §13).
//!
//! Ordering (idempotent, keyed by meeting_id): resolve dir_paths first, delete
//! learner rows then lesson_sets rows, remove on-disk dirs last tolerating
//! ENOENT — and only once the rows' deletion is durable:
//! - `purge_for_meeting` (rows, then dirs right away) is for a caller that is
//!   NOT inside a transaction (`DatabaseManager::delete_meeting`, autocommit).
//! - Inside a transaction (a Delete/Strike, a backup purge) use
//!   `purge_rows_for_meeting` only; after the live commit
//!   `redaction::post_commit_purge` calls `remove_unreferenced_dirs_for_meeting`,
//!   so a rollback never leaves a row whose files are gone.
//! - `sweep_orphans` (at startup) removes what a crash left: staging dirs and
//!   set folders no catalog row points to.

use sqlx::sqlite::SqliteConnection;
use sqlx::{Pool, Row, Sqlite};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

/// The folder names the catalog points to (row ids and the last component of
/// each `dir_path`), or `None` when there is no catalog table yet (then no
/// folder can be judged unreferenced).
async fn referenced_folders(pool: &Pool<Sqlite>) -> Result<Option<HashSet<String>>, String> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lesson_sets'")
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("Lessons: couldn't read the catalog: {}", e))?;
    let mut out = HashSet::new();
    if exists.is_none() {
        return Ok(None);
    }
    let rows = sqlx::query("SELECT id, dir_path FROM lesson_sets")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Lessons: couldn't read the catalog: {}", e))?;
    for r in rows {
        out.insert(r.get::<String, _>("id"));
        let dir: String = r.get("dir_path");
        if let Some(name) = Path::new(&dir).file_name() {
            out.insert(name.to_string_lossy().to_string());
        }
    }
    Ok(Some(out))
}

/// The set folders under `root` and, separately, its `.tmp-*` staging dirs
/// (directories only).
fn folders(root: &Path) -> (Vec<(String, PathBuf)>, Vec<PathBuf>) {
    let mut sets = Vec::new();
    let mut staging = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return (sets, staging);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(".tmp-") {
            staging.push(path);
        } else {
            sets.push((name, path));
        }
    }
    (sets, staging)
}

fn remove_folder(path: &Path, what: &str) -> bool {
    match std::fs::remove_dir_all(path) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            log::warn!("Lessons: couldn't remove {}: {}", what, e);
            false
        }
    }
}

/// After a Delete/Strike has committed: remove the folders of `meeting_id`'s
/// generated sets that no catalog row points to any more. A folder counts as
/// that recording's generated set when its `lessonset.json` says so
/// (`provenance: "generated"`, `sourceRecordingId`). Staging dirs and folders
/// a row still points to are never touched, so a rolled-back purge keeps its
/// files and an import or Make lesson in flight is safe. Returns how many
/// folders were removed.
pub async fn remove_unreferenced_dirs_for_meeting(
    pool: &Pool<Sqlite>,
    root: &Path,
    meeting_id: &str,
) -> Result<usize, String> {
    // No catalog table: no row was deleted, so there is nothing to remove
    let Some(referenced) = referenced_folders(pool).await? else { return Ok(0) };
    let mut removed = 0;
    for (name, path) in folders(root).0 {
        if referenced.contains(&name) {
            continue;
        }
        let Ok(bytes) = std::fs::read(path.join("lessonset.json")) else { continue };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
        let generated = v.get("provenance").and_then(|p| p.as_str()) == Some("generated");
        let from_it = v.get("sourceRecordingId").and_then(|s| s.as_str()) == Some(meeting_id);
        if generated && from_it && remove_folder(&path, "a purged lesson's folder") {
            removed += 1;
        }
    }
    Ok(removed)
}

/// At startup, before anything can import or make a lesson: remove what an
/// interrupted import, Make lesson or purge left behind — every `.tmp-*`
/// staging dir, and every set folder no catalog row points to (a crash
/// between moving files into place and the commit, or between a commit and
/// removing the replaced or purged set's folder). The seed course is imported
/// through the catalog like any bundle, so its folder has a row; a seed folder
/// without one is garbage, and the seed loader (which runs after this)
/// imports the course again when it should. Returns how many were removed.
pub async fn sweep_orphans(pool: &Pool<Sqlite>, root: &Path) -> Result<usize, String> {
    let referenced = referenced_folders(pool).await?;
    let (sets, staging) = folders(root);
    let mut removed = 0;
    for path in staging {
        if remove_folder(&path, "a staging folder") {
            removed += 1;
        }
    }
    // Without a catalog table every set folder would look orphaned, imports
    // included: keep them all rather than guess (only staging dirs go)
    let Some(referenced) = referenced else { return Ok(removed) };
    for (name, path) in sets {
        if !referenced.contains(&name) && remove_folder(&path, "an orphaned lesson folder") {
            removed += 1;
        }
    }
    Ok(removed)
}
