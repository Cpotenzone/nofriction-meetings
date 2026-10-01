import Foundation
import SQLite3
import SwiftData

/// Freed-space cleanup for the SwiftData store after a Delete or Strike
/// (docs/REDACTION.md, purge item 8).
///
/// SwiftData doesn't expose Core Data's store options, so the store can't be
/// opened with `PRAGMA secure_delete = ON`. Instead, after the edit is saved:
/// 1. SwiftData's persistent history is deleted (it records which properties
///    changed — not their values for this schema — but nothing about an edit
///    should outlive it).
/// 2. A second SQLite connection runs `wal_checkpoint(TRUNCATE)`, `VACUUM`
///    (rebuilds the file so no free page still holds the old text) and a final
///    `wal_checkpoint(TRUNCATE)` (empties the WAL, which held the old pages).
///
/// What can't be guaranteed: flash blocks the file system freed are managed
/// by iOS (encrypted with the file's key, not overwritten by the app), and
/// VACUUM can be skipped if another connection is mid-transaction; `scrub`
/// reports that so the UI can say so.
enum StoreHygiene {
    struct Result: Equatable {
        var historyCleared: Bool
        var compacted: Bool
    }

    @MainActor
    static func scrub(_ context: ModelContext) -> Result {
        var history = false
        do {
            try context.deleteHistory(HistoryDescriptor<DefaultHistoryTransaction>())
            history = true
        } catch {
            history = false
        }
        let urls = context.container.configurations.map(\.url).filter { $0.isFileURL && $0.path(percentEncoded: false) != "/dev/null" }
        guard !urls.isEmpty else {
            // In-memory store: nothing on disk to compact
            return Result(historyCleared: history, compacted: true)
        }
        let compacted = urls.allSatisfy { compact(storeAt: $0) }
        return Result(historyCleared: history, compacted: compacted)
    }

    /// Checkpoint + VACUUM + truncate the WAL of the SQLite file at `url`.
    @discardableResult
    static func compact(storeAt url: URL) -> Bool {
        guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else { return false }
        var db: OpaquePointer?
        guard sqlite3_open_v2(url.path(percentEncoded: false), &db, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db else {
            sqlite3_close(db)
            return false
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 3000)
        // secure_delete is per connection: it covers the pages this connection frees
        let steps = [
            "PRAGMA secure_delete = ON;",
            "PRAGMA wal_checkpoint(TRUNCATE);",
            "VACUUM;",
            "PRAGMA wal_checkpoint(TRUNCATE);",
        ]
        var ok = true
        for sql in steps {
            if sqlite3_exec(db, sql, nil, nil, nil) != SQLITE_OK { ok = false }
        }
        // A checkpoint that couldn't finish reports busy in its first result column
        ok = ok && checkpointComplete(db)
        return ok
    }

    private static func checkpointComplete(_ db: OpaquePointer) -> Bool {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, "PRAGMA wal_checkpoint(TRUNCATE);", -1, &stmt, nil) == SQLITE_OK else { return false }
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_step(stmt) == SQLITE_ROW else { return false }
        return sqlite3_column_int(stmt, 0) == 0
    }
}
