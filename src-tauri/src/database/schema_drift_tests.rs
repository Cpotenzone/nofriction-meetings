//! Schema drift: a database created by an older build must end up with
//! every column the current code uses. `CREATE TABLE IF NOT EXISTS` never
//! alters an existing table, so columns added later need
//! [`super::ensure_columns`]. `old_schema_2026_10.sql` is the real schema
//! (DDL only) of such a database: its `text_snapshots` had no `meeting_id`,
//! so every screen delete failed with "no such column: meeting_id".

use super::*;
use crate::redaction::{self, RedactionEnv, WordTarget};
use sqlx::sqlite::SqliteConnection;
use sqlx::{Connection, Executor};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const OLD_SCHEMA: &str = include_str!("old_schema_2026_10.sql");

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nf-drift-{}-{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// table → columns, for every ordinary table (FTS shadow tables excluded)
async fn columns(pool: &Pool<Sqlite>) -> BTreeMap<String, BTreeSet<String>> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
         AND name NOT LIKE 'transcripts_fts%'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut out = BTreeMap::new();
    for t in tables {
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&t)
            .fetch_all(pool)
            .await
            .unwrap();
        out.insert(t, cols.into_iter().collect());
    }
    out
}

/// Every migration that creates tables at launch: the database's own, plus
/// the settings and prompt-library managers (same file).
async fn migrate_all(db: &DatabaseManager) {
    db.run_migrations().await.unwrap();
    crate::settings::SettingsManager::new(db.get_pool()).init().await.unwrap();
    crate::prompt_manager::PromptManager::new(db.pool().clone()).run_migrations().await.unwrap();
}

#[tokio::test]
async fn ensure_columns_adds_only_missing_columns_and_fails_loudly() {
    let dir = tmp_dir("cols");
    let mut conn = SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(dir.join("t.db")).create_if_missing(true),
    )
    .await
    .unwrap();
    conn.execute("CREATE TABLE t (a TEXT)").await.unwrap();
    let added = ensure_columns(&mut conn, "t", &[("a", "TEXT"), ("b", "TEXT"), ("c", "INTEGER NOT NULL DEFAULT 0")])
        .await
        .unwrap();
    assert_eq!(added, vec!["b".to_string(), "c".to_string()]);
    // Idempotent: nothing to add the second time
    assert!(ensure_columns(&mut conn, "t", &[("b", "TEXT"), ("c", "INTEGER")]).await.unwrap().is_empty());
    // A real failure is returned, not swallowed (unlike `let _ = ALTER ...`)
    assert!(ensure_columns(&mut conn, "missing_table", &[("x", "TEXT")]).await.is_err());
    conn.execute("INSERT INTO t (a) VALUES ('row')").await.unwrap();
    assert!(ensure_columns(&mut conn, "t", &[("d", "TEXT NOT NULL")]).await.is_err(), "NOT NULL without default");
    let _ = std::fs::remove_dir_all(dir);
}

/// Build the old database, with synthetic rows shaped like real ones
/// (a screen with its OCR snapshot, a loose accessibility snapshot, a
/// transcript line, and a screen delete left pending by the failed purge).
async fn old_database(dir: &Path) -> PathBuf {
    let path = dir.join("nofriction_meetings.db");
    let mut conn = SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path).create_if_missing(true),
    )
    .await
    .unwrap();
    conn.execute(OLD_SCHEMA).await.expect("old schema DDL runs");
    let t0 = Utc::now() - chrono::Duration::minutes(30);
    let ts = |s: i64| (t0 + chrono::Duration::seconds(s)).to_rfc3339();
    conn.execute(
        format!(
            "INSERT INTO meetings (id, title, started_at) VALUES ('m1', 'Old meeting', '{}');
             INSERT INTO document_episodes (episode_id, meeting_id, start_ts, app_name, window_title)
                 VALUES ('e1', 'm1', '{}', 'Mail', 'Inbox');
             INSERT INTO screen_states (state_id, meeting_id, start_ts, end_ts, app_name, window_title, phash, keyframe_path)
                 VALUES ('s1', 'm1', '{}', '{}', 'Mail', 'Inbox', 'p', NULL),
                        ('s2', 'm1', '{}', '{}', 'Notes', 'Plan', 'p', NULL);
             INSERT INTO episode_states (episode_id, state_id, sequence_num) VALUES ('e1', 's1', 0);
             INSERT INTO text_snapshots (snapshot_id, episode_id, state_id, ts, text, text_hash, source)
                 VALUES ('snap-s1', 'e1', 's1', '{}', 'ocr text', 'h1', 'ocr'),
                        ('snap-s2', NULL, 's2', '{}', 'ocr text two', 'h2', 'ocr'),
                        ('snap-loose', 'e1', NULL, '{}', 'accessibility text', 'h3', 'accessibility');
             INSERT INTO transcripts (meeting_id, text, timestamp, is_final, confidence)
                 VALUES ('m1', 'keep these words please', '{}', 1, 0.9);",
            ts(0),
            ts(10),
            ts(60),
            ts(70),
            ts(120),
            ts(130),
            ts(60),
            ts(120),
            ts(65),
            ts(200),
        )
        .as_str(),
    )
    .await
    .unwrap();
    // The owner's situation: a screen delete whose purge failed on the old
    // schema, kept pending (failed) to be retried at launch
    sqlx::query(
        "INSERT INTO redactions (id, meeting_id, kind, action, created_at, item_count, pending_payload, failed_at, failure) \
         VALUES ('pending1', 'm1', 'screen', 'delete', ?, 1, ?, ?, 'no such column: meeting_id')",
    )
    .bind(ts(300))
    .bind(r#"{"type":"screens","ids":["s2"]}"#)
    .bind(ts(301))
    .execute(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();
    path
}

#[tokio::test]
async fn old_schema_migrates_to_every_current_column_and_screen_delete_works() {
    // What the current code creates on a fresh install
    let fresh_dir = tmp_dir("fresh");
    let fresh = DatabaseManager::new(&fresh_dir.join("nofriction_meetings.db")).await.unwrap();
    migrate_all(&fresh).await;
    let want = columns(fresh.pool()).await;

    // The old database, migrated by the current code
    let dir = tmp_dir("old");
    let path = old_database(&dir).await;
    let db = DatabaseManager::new(&path).await.unwrap();
    migrate_all(&db).await;
    let have = columns(db.pool()).await;

    let mut missing = Vec::new();
    for (table, cols) in &want {
        match have.get(table) {
            None => missing.push(format!("table {}", table)),
            Some(h) => missing.extend(cols.difference(h).map(|c| format!("{}.{}", table, c))),
        }
    }
    assert!(missing.is_empty(), "columns the code uses but an old database lacks: {:?}", missing);
    let idx: Option<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_snapshots_meeting'")
            .fetch_optional(db.pool())
            .await
            .unwrap();
    assert!(idx.is_some(), "index on the added column exists");

    // Backfill: meeting from the snapshot's screen (or its episode)
    let rows: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT snapshot_id, meeting_id, app_name FROM text_snapshots ORDER BY snapshot_id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            ("snap-loose".into(), Some("m1".into()), Some("Mail".into())),
            ("snap-s1".into(), Some("m1".into()), Some("Mail".into())),
            ("snap-s2".into(), Some("m1".into()), Some("Notes".into())),
        ]
    );
    // Running the migrations again changes nothing and doesn't fail
    migrate_all(&db).await;

    // The insert that failed on the old schema now works
    db.add_text_snapshot_full("snap-new", None, None, Some("m1"), Utc::now(), "t", "h", 0.5, "ocr", None, None)
        .await
        .unwrap();

    let env = RedactionEnv {
        app_data_dir: dir.join("data"),
        cache_dir: dir.join("cache"),
        video_enabled: false,
        recording_meetings: Vec::new(),
    };
    // Launch: the pending screen delete that failed before now applies
    let errors = redaction::commit_all_pending(db.pool(), &env).await;
    assert!(errors.is_empty(), "{:?}", errors);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM screen_states WHERE state_id = 's2'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM redactions").fetch_one(db.pool()).await.unwrap();
    assert_eq!(n, 0, "Delete leaves no trace");

    // A new screen delete (purge) on the migrated schema
    let p = redaction::request_delete_screens(db.pool(), &env, "m1", &["s1".into()]).await.unwrap();
    redaction::commit_pending(db.pool(), &env, &p.id).await.unwrap().unwrap();
    for (sql, what) in [
        ("SELECT COUNT(*) FROM screen_states", "screens"),
        ("SELECT COUNT(*) FROM text_snapshots WHERE snapshot_id IN ('snap-s1', 'snap-s2', 'snap-loose')", "screen text"),
    ] {
        let n: i64 = sqlx::query_scalar(sql).fetch_one(db.pool()).await.unwrap();
        assert_eq!(n, 0, "{} left after the purge", what);
    }

    // And a transcript delete
    let id: i64 = sqlx::query_scalar("SELECT id FROM transcripts").fetch_one(db.pool()).await.unwrap();
    let t = WordTarget {
        meeting_id: "m1".into(),
        transcript_id: id,
        start: 5,
        end: 10,
        expected_text: None,
        whole_line: false,
    };
    let p = redaction::request_delete_words(db.pool(), &env, &t).await.unwrap();
    redaction::commit_pending(db.pool(), &env, &p.id).await.unwrap().unwrap();
    let text: String = sqlx::query_scalar("SELECT text FROM transcripts").fetch_one(db.pool()).await.unwrap();
    assert_eq!(text, "keep words please");

    let _ = std::fs::remove_dir_all(fresh_dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Real-data check, run by hand against a COPY of an app database:
///   NF_DRIFT_DB=/path/to/copy.db cargo test --lib owner_db_copy -- --ignored --nocapture
/// The copy is copied again into a temp dir, its file paths are pointed into
/// that temp dir (so nothing outside it can be deleted), then migrated and
/// the launch-time commit of pending deletes runs. Prints counts only.
#[tokio::test]
#[ignore]
async fn owner_db_copy_migrates_and_commits_pending_deletes() {
    let Ok(src) = std::env::var("NF_DRIFT_DB") else {
        eprintln!("NF_DRIFT_DB not set");
        return;
    };
    let dir = tmp_dir("copy");
    let path = dir.join("nofriction_meetings.db");
    std::fs::copy(&src, &path).unwrap();
    let count = |sql: &'static str| {
        let p = path.clone();
        async move {
            let mut c = SqliteConnection::connect_with(&sqlx::sqlite::SqliteConnectOptions::new().filename(&p))
                .await
                .unwrap();
            let n: i64 = sqlx::query_scalar(sql).fetch_one(&mut c).await.unwrap_or(-1);
            c.close().await.unwrap();
            n
        }
    };
    let before_cols: i64 =
        count("SELECT COUNT(*) FROM pragma_table_info('text_snapshots') WHERE name IN ('meeting_id','app_name','window_title')").await;
    let pending_before = count("SELECT COUNT(*) FROM redactions WHERE action = 'delete' AND pending_payload IS NOT NULL").await;
    let screens_before = count("SELECT COUNT(*) FROM screen_states").await;
    let snaps_before = count("SELECT COUNT(*) FROM text_snapshots").await;
    {
        // Point every stored file path into the temp dir
        let mut c = SqliteConnection::connect_with(&sqlx::sqlite::SqliteConnectOptions::new().filename(&path))
            .await
            .unwrap();
        let fake = dir.join("files").to_string_lossy().to_string();
        for sql in [
            "UPDATE screen_states SET keyframe_path = ?1 || '/' || state_id || '.jpg' WHERE keyframe_path IS NOT NULL",
            "UPDATE frames SET file_path = ?1 || '/' || id || '.jpg' WHERE file_path IS NOT NULL",
            "UPDATE frame_queue SET frame_path = ?1 || '/q' || id || '.jpg'",
        ] {
            sqlx::query(sql).bind(&fake).execute(&mut c).await.unwrap();
        }
        c.close().await.unwrap();
    }
    let db = DatabaseManager::new(&path).await.unwrap();
    let t = std::time::Instant::now();
    migrate_all(&db).await;
    let migrate_ms = t.elapsed().as_millis();
    let after_cols: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('text_snapshots') WHERE name IN ('meeting_id','app_name','window_title')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let backfilled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_snapshots WHERE meeting_id IS NOT NULL")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let env = RedactionEnv {
        app_data_dir: dir.join("data"),
        cache_dir: dir.join("cache"),
        video_enabled: false,
        recording_meetings: Vec::new(),
    };
    let t = std::time::Instant::now();
    let errors = redaction::commit_all_pending(db.pool(), &env).await;
    let commit_ms = t.elapsed().as_millis();
    let pool = db.pool().clone();
    let q = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await.unwrap() }
    };
    let pending_after = q("SELECT COUNT(*) FROM redactions WHERE action = 'delete' AND pending_payload IS NOT NULL").await;
    let screens_after = q("SELECT COUNT(*) FROM screen_states").await;
    let snaps_after = q("SELECT COUNT(*) FROM text_snapshots").await;
    println!("text_snapshots drift columns present: before {} / after {} (of 3)", before_cols, after_cols);
    println!("snapshots with meeting_id after backfill: {} of {}", backfilled, snaps_before);
    println!("migrations: {} ms; commit_all_pending: {} ms, {} error(s)", migrate_ms, commit_ms, errors.len());
    for e in &errors {
        // Error strings are app messages (no content); shown for diagnosis
        println!("  error: {}", e.chars().take(160).collect::<String>());
    }
    println!("pending deletes: before {} / after {}", pending_before, pending_after);
    println!("screen states: before {} / after {}", screens_before, screens_after);
    println!("text snapshots: before {} / after {}", snaps_before, snaps_after);
    drop(pool);
    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}
