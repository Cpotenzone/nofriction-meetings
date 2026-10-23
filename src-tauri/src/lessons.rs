//! Lessons (docs/LESSONS.md): noFriction's interactive-education subsystem.
//! A **lesson set** is a self-contained, 100%-offline unit of study material —
//! a concept graph, deterministic data-driven explainers (never model HTML),
//! AI-authored static figure PNGs embedded in the bundle, per-lesson checkpoint
//! quizzes, per-concept mastery, spaced-repetition review, adaptive trainers, a
//! cumulative exam, and a locally-issued certificate.
//!
//! This module is the Rust backend (FEAT-001). It mirrors the study subsystem's
//! architecture exactly: **untrusted structured content → tolerant parse →
//! strict validation + cleaning → store only cleaned structured data → render as
//! plain text / typed components, never HTML.** Correctness and grading are
//! authoritative here in Rust (shared with a future Swift layer); the TS mirror
//! is preview-only and never decides pass/fail.
//!
//! Invariants (docs/LESSONS.md §11): taking a present lesson is 100% offline and
//! free — no AI, no image, no font/CDN fetch, no telemetry. The only network is
//! a user-initiated URL import (`bundle::import_from_url`); no default URL ships.
//! Generated-provenance sets join the Delete/Strike purge (`purge`); seed and
//! imported sets are exempt. **Make lesson** (`generate`, §13) turns a Class
//! recording's transcript into a generated set with the user's own AI
//! (noFriction Pro, through `ai::client::complete`).

pub mod bundle;
pub mod commands;
pub mod exam;
pub mod explain;
pub mod generate;
pub mod generators;
pub mod grade;
pub mod mastery;
pub mod parse;
pub mod purge;
pub mod review;
pub mod schema;
pub mod seed;
pub mod trainer;

#[cfg(test)]
mod tests;

use sqlx::sqlite::SqliteConnection;

pub use purge::{count_for_meeting, purge_for_meeting};

/// The bundle schema major this build reads. Import rejects an unknown major.
pub const SCHEMA_MAJOR: u32 = 1;
/// The bundle schema minor this build writes. A newer minor is read tolerantly.
/// Minor 1 (Make lesson) adds the `text` explainer kind, the optional
/// `WorkedExample.answerText`, and an optional `finalAnswer` (required for
/// authored courses, absent from generated ones).
pub const SCHEMA_MINOR: u32 = 1;

/// `<app data>/lessons/` — the on-disk catalog of imported/seeded bundles.
pub fn lessons_root() -> std::path::PathBuf {
    crate::paths::app_data_dir().join("lessons")
}

/// Additive schema: the catalog table and the learner-local records. Mirrors
/// `study::ensure_schema` — only `CREATE TABLE IF NOT EXISTS` and
/// `ensure_columns`, never an `ALTER` of a shared table. Registered in
/// `database.rs::run_migrations` next to `study::ensure_schema`.
pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    // The catalog: one row per present lesson set (bundle files live on disk).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lesson_sets (\
            id TEXT PRIMARY KEY, \
            title TEXT NOT NULL, \
            subtitle TEXT, \
            level TEXT, \
            provenance TEXT NOT NULL, \
            source_recording_id TEXT, \
            transcript_fingerprint TEXT, \
            content_fingerprint TEXT NOT NULL, \
            schema_major INTEGER NOT NULL, \
            dir_path TEXT NOT NULL, \
            imported_at TEXT NOT NULL\
        )",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lesson_sets_recording \
         ON lesson_sets(source_recording_id, provenance)",
    )
    .execute(&mut *conn)
    .await?;

    // Learner-local: per-lesson progress.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lesson_progress (\
            lesson_set_id TEXT NOT NULL, \
            lesson_id TEXT NOT NULL, \
            explainers_seen TEXT NOT NULL DEFAULT '[]', \
            worked_examples_seen TEXT NOT NULL DEFAULT '[]', \
            checkpoint_best_pct REAL NOT NULL DEFAULT 0, \
            checkpoint_passed INTEGER NOT NULL DEFAULT 0, \
            last_viewed_explainer_id TEXT, \
            started_at TEXT, \
            updated_at TEXT NOT NULL, \
            PRIMARY KEY (lesson_set_id, lesson_id)\
        )",
    )
    .execute(&mut *conn)
    .await?;

    // Learner-local: per-concept mastery (EMA) + SM-2-lite review state.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mastery_records (\
            lesson_set_id TEXT NOT NULL, \
            concept_id TEXT NOT NULL, \
            score REAL NOT NULL DEFAULT 0, \
            attempts INTEGER NOT NULL DEFAULT 0, \
            correct_attempts INTEGER NOT NULL DEFAULT 0, \
            first_try_correct INTEGER NOT NULL DEFAULT 0, \
            mastered INTEGER NOT NULL DEFAULT 0, \
            ease REAL NOT NULL DEFAULT 2.5, \
            interval_days REAL NOT NULL DEFAULT 0, \
            reviews INTEGER NOT NULL DEFAULT 0, \
            due_at TEXT, \
            last_reviewed_at TEXT, \
            updated_at TEXT NOT NULL, \
            PRIMARY KEY (lesson_set_id, concept_id)\
        )",
    )
    .execute(&mut *conn)
    .await?;
    // `reviews` shipped after the table first existed, so an older DB needs the
    // column added explicitly (CREATE TABLE IF NOT EXISTS never alters). It
    // persists the SM-2-lite review count so the interval walks 1 → 3 →
    // interval·ease across sessions instead of resetting to 1 day every grade.
    crate::database::ensure_columns(
        &mut *conn,
        "mastery_records",
        &[("reviews", "INTEGER NOT NULL DEFAULT 0")],
    )
    .await?;

    // Learner-local: exam attempts. `selected_item_ids` persists the 30 item
    // ids drawn at start so the submit scores against the server-known
    // selection (unanswered → zero), not whatever subset the client posts.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS exam_attempts (\
            id TEXT PRIMARY KEY, \
            lesson_set_id TEXT NOT NULL, \
            started_at TEXT NOT NULL, \
            submitted_at TEXT, \
            answers TEXT NOT NULL DEFAULT '[]', \
            selected_item_ids TEXT NOT NULL DEFAULT '[]', \
            score_pct REAL NOT NULL DEFAULT 0, \
            passed INTEGER NOT NULL DEFAULT 0\
        )",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_exam_attempts_set ON exam_attempts(lesson_set_id)",
    )
    .execute(&mut *conn)
    .await?;
    // `selected_item_ids` shipped after the table first existed, so an older DB
    // needs the column added explicitly (CREATE TABLE IF NOT EXISTS never
    // alters it in).
    crate::database::ensure_columns(
        &mut *conn,
        "exam_attempts",
        &[("selected_item_ids", "TEXT NOT NULL DEFAULT '[]'")],
    )
    .await?;

    // Learner-local: issued certificates.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS certificates (\
            id TEXT PRIMARY KEY, \
            lesson_set_id TEXT NOT NULL, \
            exam_attempt_id TEXT NOT NULL, \
            course_title TEXT NOT NULL, \
            level TEXT, \
            score_pct REAL NOT NULL, \
            issued_at TEXT NOT NULL, \
            provenance TEXT NOT NULL, \
            result_hash TEXT NOT NULL\
        )",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_certificates_set ON certificates(lesson_set_id)",
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}
