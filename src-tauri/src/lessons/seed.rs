//! Seed loading on first run (docs/LESSONS.md §5.4). `ensure_seeded` checks a
//! `lessons_seed_version` value in the app settings table; if the shipped seed
//! version is newer than what was loaded, it imports the shipped
//! `resources/seed/stoichiometry.lesson` via the ordinary `import_bundle` path
//! with provenance `seed`, idempotently (keyed by content fingerprint). A
//! deliberate removal leaves a tombstone so re-seeding does not resurrect it.
//!
//! The bundle is compiled into the binary (`SEED_BUNDLE`), so every build has
//! it; tests drive the loader through `ensure_seeded_from_bytes`.

use crate::lessons::bundle;
use sqlx::{Pool, Sqlite};

/// Bump this when the shipped seed content changes so a new version re-seeds.
pub const SEED_VERSION: i64 = 1;

const KEY_VERSION: &str = "lessons_seed_version";
const KEY_TOMBSTONE: &str = "lessons_seed_removed";
/// The shipped seed set id (stable so re-seeding is idempotent).
pub const SEED_SET_ID: &str = "stoichiometry-seed";

async fn get_setting(pool: &Pool<Sqlite>, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

async fn set_setting(pool: &Pool<Sqlite>, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record that the user deliberately removed the seed set, so it is not
/// re-seeded on the next launch. Called by the remove-set command when the
/// removed set is the seed set.
pub async fn mark_removed(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    set_setting(pool, KEY_TOMBSTONE, "1").await
}

/// True if the settings table exists (it is created by the core migration
/// before lessons; a very old backup may lack it).
async fn settings_table_exists(pool: &Pool<Sqlite>) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// Seed from explicit bundle bytes (used by tests and by the shipped-resource
/// path). Idempotent: skips if the tombstone is set or the loaded version is
/// already current. Returns true if it imported.
pub async fn ensure_seeded_from_bytes(pool: &Pool<Sqlite>, bytes: &[u8]) -> Result<bool, String> {
    if !settings_table_exists(pool).await {
        return Ok(false);
    }
    if get_setting(pool, KEY_TOMBSTONE).await.as_deref() == Some("1") {
        return Ok(false);
    }
    let loaded: i64 = get_setting(pool, KEY_VERSION)
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if loaded >= SEED_VERSION {
        return Ok(false);
    }
    let prepared = bundle::validate_bytes(bytes)?;
    bundle::commit_bundle(pool, &prepared, "seed", None).await?;
    set_setting(pool, KEY_VERSION, &SEED_VERSION.to_string())
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// The shipped seed course, compiled into the binary. A runtime resource lookup
/// found it only in development (beside `cargo run`'s working directory): the
/// bundled .app had no copy, so the store builds started with an empty Lessons
/// tab. Embedding it works the same in the DMG and the sandboxed Mac App Store
/// build, with no bundle-resource configuration to forget.
pub const SEED_BUNDLE: &[u8] = include_bytes!("../../resources/seed/stoichiometry.lesson");

/// Seed the shipped course on startup (not a command). Idempotent: see
/// `ensure_seeded_from_bytes`.
pub async fn ensure_seeded(pool: &Pool<Sqlite>) -> Result<bool, String> {
    ensure_seeded_from_bytes(pool, SEED_BUNDLE).await
}
