//! App data locations and the one-time data-folder migration.
//!
//! Tauri's `app_data_dir()` is `<data dir>/<bundle identifier>`. The
//! identifier changed from `ai.nofriction.meetings` to
//! `com.nofriction.meetings` (App Store bundle id, docs/APP_STORE_RELEASE.md
//! D1), so the Developer ID build moves the old folder once at startup.
//!
//! In the App Sandbox (`mas` feature) `dirs::data_dir()` resolves inside the
//! container (`~/Library/Containers/com.nofriction.meetings/Data/Library/
//! Application Support`), so the same helpers work unchanged there. The
//! sandboxed build cannot see the DMG build's folder, so nothing is migrated
//! into the container automatically.

use std::path::{Path, PathBuf};

/// Must match `identifier` in tauri.conf.json.
pub const BUNDLE_ID: &str = "com.nofriction.meetings";
/// Identifier used by builds up to v3.5.0.
pub const LEGACY_BUNDLE_ID: &str = "ai.nofriction.meetings";

/// Same path Tauri's `app.path().app_data_dir()` returns, usable before the
/// app is built (logging) and from code without an `AppHandle`.
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(BUNDLE_ID)
}

/// Per-app cache folder (thumbnails, extracted frames).
pub fn app_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join(BUNDLE_ID)
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

/// Outcome of [`migrate_legacy_data_dir`], logged once logging is up.
#[derive(Debug, PartialEq, Eq)]
pub enum Migration {
    /// No legacy folder
    NothingToDo,
    /// Both exist: leave both alone (never merge or delete user data)
    BothExist { legacy: PathBuf, current: PathBuf },
    Moved { from: PathBuf, to: PathBuf },
    Failed { from: PathBuf, to: PathBuf, error: String },
}

/// Move `<data>/ai.nofriction.meetings` → `<data>/com.nofriction.meetings`
/// when the destination doesn't exist yet. Never deletes anything: if the
/// rename fails, the old folder stays where it is and the app starts with a
/// fresh folder (the error is logged so the owner can move it by hand).
pub fn migrate_legacy_data_dir() -> Migration {
    // The sandboxed build can't reach the old folder; don't even look.
    if cfg!(feature = "mas") {
        return Migration::NothingToDo;
    }
    let Some(data) = dirs::data_dir() else { return Migration::NothingToDo };
    migrate_between(&data.join(LEGACY_BUNDLE_ID), &data.join(BUNDLE_ID))
}

pub fn migrate_between(legacy: &Path, current: &Path) -> Migration {
    if !legacy.is_dir() {
        return Migration::NothingToDo;
    }
    if current.exists() {
        // An empty destination (e.g. created by an earlier launch that
        // failed to migrate) is safe to replace; anything else is kept.
        let empty = std::fs::read_dir(current).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty || std::fs::remove_dir(current).is_err() {
            return Migration::BothExist { legacy: legacy.to_path_buf(), current: current.to_path_buf() };
        }
    }
    // Same volume, so this is an atomic rename.
    match std::fs::rename(legacy, current) {
        Ok(()) => Migration::Moved { from: legacy.to_path_buf(), to: current.to_path_buf() },
        Err(e) => Migration::Failed {
            from: legacy.to_path_buf(),
            to: current.to_path_buf(),
            error: e.to_string(),
        },
    }
}

pub fn log_migration(m: &Migration) {
    match m {
        Migration::NothingToDo => {}
        Migration::Moved { from, to } => {
            log::info!("📦 Moved app data {} → {}", from.display(), to.display())
        }
        Migration::BothExist { legacy, current } => log::warn!(
            "📦 Both {} and {} exist; using the new folder and leaving the old one untouched",
            legacy.display(),
            current.display()
        ),
        Migration::Failed { from, to, error } => log::error!(
            "📦 Could not move app data {} → {} ({}); old data left in place. \
             Quit the app and move the folder by hand.",
            from.display(),
            to.display(),
            error
        ),
    }
}

/// Database left behind by the removed screen-ingest feature (pre-3.6).
pub const REMOVED_INGEST_QUEUE_DB: &str = "ingest_queue.db";

/// Outcome of [`archive_removed_ingest_queue`].
#[derive(Debug, PartialEq, Eq)]
pub enum IngestQueueArchive {
    NothingToDo,
    Moved { to: PathBuf },
    Failed { error: String },
}

/// Move `<data>/ingest_queue.db` (and any `-wal`/`-shm` sidecars) into
/// `<data>/backups/removed-ingest-queue-<date>.db`. Never deletes: if the
/// name is taken a counter is appended; if the move fails the file stays.
pub fn archive_removed_ingest_queue(data_dir: &Path, date: &str) -> IngestQueueArchive {
    let src = data_dir.join(REMOVED_INGEST_QUEUE_DB);
    if !src.is_file() {
        return IngestQueueArchive::NothingToDo;
    }
    let backups = data_dir.join("backups");
    if let Err(e) = std::fs::create_dir_all(&backups) {
        return IngestQueueArchive::Failed { error: e.to_string() };
    }
    let mut dest = backups.join(format!("removed-ingest-queue-{}.db", date));
    let mut n = 2;
    while dest.exists() {
        dest = backups.join(format!("removed-ingest-queue-{}-{}.db", date, n));
        n += 1;
    }
    if let Err(e) = std::fs::rename(&src, &dest) {
        return IngestQueueArchive::Failed { error: e.to_string() };
    }
    for suffix in ["-wal", "-shm"] {
        let side = data_dir.join(format!("{}{}", REMOVED_INGEST_QUEUE_DB, suffix));
        if side.is_file() {
            let side_dest = PathBuf::from(format!("{}{}", dest.display(), suffix));
            let _ = std::fs::rename(&side, &side_dest);
        }
    }
    IngestQueueArchive::Moved { to: dest }
}

/// Startup hook: archive the removed ingest queue in the app data folder
/// and log the outcome once (silent when there is nothing to do).
pub fn archive_removed_ingest_queue_at_startup() {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    match archive_removed_ingest_queue(&app_data_dir(), &date) {
        IngestQueueArchive::NothingToDo => {}
        IngestQueueArchive::Moved { to } => log::info!(
            "📦 Moved {} from the removed ingest feature to {} (kept, not deleted)",
            REMOVED_INGEST_QUEUE_DB,
            to.display()
        ),
        IngestQueueArchive::Failed { error } => log::warn!(
            "📦 Could not move the old {} into backups ({}); left in place",
            REMOVED_INGEST_QUEUE_DB,
            error
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_removed_ingest_queue_without_deleting() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        // Nothing there: no-op, no backups folder created
        assert_eq!(archive_removed_ingest_queue(data, "2026-10-02"), IngestQueueArchive::NothingToDo);
        assert!(!data.join("backups").exists());

        std::fs::write(data.join("ingest_queue.db"), b"q1").unwrap();
        std::fs::write(data.join("ingest_queue.db-wal"), b"w").unwrap();
        let IngestQueueArchive::Moved { to } = archive_removed_ingest_queue(data, "2026-10-02") else {
            panic!("expected a move")
        };
        assert_eq!(to, data.join("backups/removed-ingest-queue-2026-10-02.db"));
        assert_eq!(std::fs::read(&to).unwrap(), b"q1");
        assert!(data.join("backups/removed-ingest-queue-2026-10-02.db-wal").exists());
        assert!(!data.join("ingest_queue.db").exists());
        assert!(!data.join("ingest_queue.db-wal").exists());

        // Same day again (e.g. restored by hand): never overwrites
        std::fs::write(data.join("ingest_queue.db"), b"q2").unwrap();
        let IngestQueueArchive::Moved { to: to2 } = archive_removed_ingest_queue(data, "2026-10-02") else {
            panic!("expected a move")
        };
        assert_eq!(to2, data.join("backups/removed-ingest-queue-2026-10-02-2.db"));
        assert_eq!(std::fs::read(&to).unwrap(), b"q1");
        assert_eq!(std::fs::read(&to2).unwrap(), b"q2");
        // Second launch: nothing left to do
        assert_eq!(archive_removed_ingest_queue(data, "2026-10-03"), IngestQueueArchive::NothingToDo);
    }

    #[test]
    fn moves_when_destination_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("old");
        let current = tmp.path().join("new");
        std::fs::create_dir_all(legacy.join("logs")).unwrap();
        std::fs::write(legacy.join("nofriction_meetings.db"), b"db").unwrap();
        assert!(matches!(migrate_between(&legacy, &current), Migration::Moved { .. }));
        assert!(!legacy.exists());
        assert_eq!(std::fs::read(current.join("nofriction_meetings.db")).unwrap(), b"db");
    }

    #[test]
    fn replaces_empty_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("old");
        let current = tmp.path().join("new");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("a"), b"1").unwrap();
        std::fs::create_dir_all(&current).unwrap();
        assert!(matches!(migrate_between(&legacy, &current), Migration::Moved { .. }));
        assert!(current.join("a").exists());
    }

    #[test]
    fn never_touches_when_both_have_data() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("old");
        let current = tmp.path().join("new");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("a"), b"old").unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("a"), b"new").unwrap();
        assert!(matches!(migrate_between(&legacy, &current), Migration::BothExist { .. }));
        assert_eq!(std::fs::read(legacy.join("a")).unwrap(), b"old");
        assert_eq!(std::fs::read(current.join("a")).unwrap(), b"new");
    }

    #[test]
    fn nothing_without_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            migrate_between(&tmp.path().join("old"), &tmp.path().join("new")),
            Migration::NothingToDo
        );
    }
}
