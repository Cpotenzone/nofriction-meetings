//! The Mac side of sync's data (docs/SYNC.md): change tracking, building
//! outgoing items, applying incoming ones.
//!
//! Change tracking is done by SQLite triggers on every synced table, so no
//! write path can forget it. Each change bumps a counter and stamps
//! `sync_meta (entity, record_id)` with that number (`seq`), the time, and
//! the device it came from (set in `sync_apply` only inside a transaction
//! that applies a peer's items). Deletions stay in `sync_meta` forever as
//! ids, so a deleted record is never re-imported. No content is stored here.

use super::merge;
use super::protocol::{self as p, Item};
use crate::redaction::{self, RedactionEnv};
use sqlx::{Pool, Row, Sqlite, SqliteConnection};
use std::collections::HashSet;

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{}: {}", ctx, e)
}

pub const NOW_MS_SQL: &str = "CAST(ROUND((julianday('now') - 2440587.5) * 86400000) AS INTEGER)";

/// Synced tables and the entity each change is logged as.
struct Tracked {
    table: &'static str,
    entity: &'static str,
    /// Expressions over NEW/OLD (`{row}` is replaced)
    record_id: &'static str,
    rec: &'static str,
    /// AFTER UPDATE OF these columns (None = any column)
    update_of: Option<&'static str>,
    /// A delete of this table's row deletes the record (else it only
    /// touches it, e.g. a meeting's attendee)
    delete_is_gone: bool,
    insert_when: Option<&'static str>,
    delete_when: Option<&'static str>,
}

const TRACKED: &[Tracked] = &[
    Tracked { table: "meetings", entity: "recording", record_id: "{row}.id", rec: "{row}.id", update_of: None, delete_is_gone: true, insert_when: None, delete_when: None },
    Tracked { table: "meeting_details", entity: "recording", record_id: "{row}.meeting_id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: false, insert_when: None, delete_when: None },
    Tracked { table: "meeting_attendees", entity: "recording", record_id: "{row}.meeting_id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: false, insert_when: None, delete_when: None },
    Tracked { table: "transcripts", entity: "line", record_id: "(SELECT sync_id FROM transcripts WHERE id = {row}.id)", rec: "{row}.meeting_id", update_of: Some("text"), delete_is_gone: true, insert_when: None, delete_when: None },
    Tracked { table: "redactions", entity: "strike", record_id: "{row}.id", rec: "{row}.meeting_id", update_of: Some("action"), delete_is_gone: false, insert_when: Some("NEW.action = 'strike'"), delete_when: Some("0") },
    Tracked { table: "meeting_notes", entity: "notes", record_id: "{row}.meeting_id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: true, insert_when: None, delete_when: Some("NOT EXISTS (SELECT 1 FROM meeting_notes n WHERE n.meeting_id = OLD.meeting_id)") },
    Tracked { table: "meeting_markers", entity: "mark", record_id: "{row}.id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: true, insert_when: None, delete_when: None },
    Tracked { table: "meeting_references", entity: "ref", record_id: "{row}.id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: true, insert_when: None, delete_when: None },
    Tracked { table: "meeting_topics", entity: "topic", record_id: "{row}.id", rec: "{row}.meeting_id", update_of: None, delete_is_gone: true, insert_when: None, delete_when: None },
];

/// The statements a trigger runs for one change.
fn touch_sql(entity: &str, record_id: &str, rec: &str, deleted: bool) -> String {
    // Notes come back after a delete (made again); every other id stays gone
    let deleted_merge = if entity == "notes" { "excluded.deleted" } else { "MAX(sync_meta.deleted, excluded.deleted)" };
    format!(
        "UPDATE sync_counter SET n = n + 1 WHERE id = 1; \
         INSERT INTO sync_meta (entity, record_id, rec, origin, created_seq, seq, modified_at, last_src, deleted) \
         SELECT '{entity}', {record_id}, {rec}, (SELECT value FROM sync_apply WHERE key = 'src'), c.n, c.n, \
                COALESCE((SELECT CAST(value AS INTEGER) FROM sync_apply WHERE key = 'at'), {now}), \
                (SELECT value FROM sync_apply WHERE key = 'src'), {deleted} \
         FROM sync_counter c WHERE c.id = 1 AND {record_id} IS NOT NULL \
         ON CONFLICT(entity, record_id) DO UPDATE SET seq = excluded.seq, modified_at = excluded.modified_at, \
            last_src = excluded.last_src, deleted = {deleted_merge}, rec = COALESCE(excluded.rec, sync_meta.rec);",
        entity = entity,
        record_id = record_id,
        rec = rec,
        now = NOW_MS_SQL,
        deleted = deleted as i32,
        deleted_merge = deleted_merge,
    )
}

/// Tables, triggers and the one-time backfill. Runs inside
/// `DatabaseManager::run_migrations` on its one connection, after every
/// synced table exists.
pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    for sql in [
        r#"CREATE TABLE IF NOT EXISTS sync_meta (
            entity TEXT NOT NULL,
            record_id TEXT NOT NULL,
            rec TEXT,
            origin TEXT,
            created_seq INTEGER NOT NULL,
            seq INTEGER NOT NULL,
            modified_at INTEGER NOT NULL,
            last_src TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (entity, record_id)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_sync_meta_seq ON sync_meta(seq)",
        "CREATE TABLE IF NOT EXISTS sync_counter (id INTEGER PRIMARY KEY CHECK (id = 1), n INTEGER NOT NULL)",
        "INSERT OR IGNORE INTO sync_counter (id, n) VALUES (1, 0)",
        // Set only inside a transaction that applies a peer's items
        "CREATE TABLE IF NOT EXISTS sync_apply (key TEXT PRIMARY KEY, value TEXT)",
        "CREATE TABLE IF NOT EXISTS sync_kv (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        r#"CREATE TABLE IF NOT EXISTS sync_devices (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            paired_at TEXT NOT NULL,
            last_sync_at TEXT,
            last_error TEXT
        )"#,
    ] {
        sqlx::query(sql).execute(&mut *conn).await?;
    }
    // Stable cross-device id for transcript lines
    crate::database::ensure_columns(conn, "transcripts", &[("sync_id", "TEXT")]).await?;
    // Where a synced line was heard ("screen" = what was playing during
    // iPhone screen capture); NULL = the microphone. Kept so the line
    // reaches other devices as it was.
    crate::database::ensure_columns(conn, "transcripts", &[("source", "TEXT")]).await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_transcripts_sync_id ON transcripts(sync_id)")
        .execute(&mut *conn)
        .await?;

    // A crash mid-apply can't leave the flag behind (it is only ever
    // written inside a transaction), but clear it anyway
    sqlx::query("DELETE FROM sync_apply").execute(&mut *conn).await?;

    for t in TRACKED {
        let ins = touch_sql(t.entity, &t.record_id.replace("{row}", "NEW"), &t.rec.replace("{row}", "NEW"), false);
        let upd = ins.clone();
        let del = touch_sql(t.entity, &t.record_id.replace("{row}", "OLD"), &t.rec.replace("{row}", "OLD"), t.delete_is_gone);
        let line_id = if t.table == "transcripts" {
            "UPDATE transcripts SET sync_id = lower(hex(randomblob(16))) WHERE id = NEW.id AND sync_id IS NULL; "
        } else {
            ""
        };
        let when = |w: Option<&str>| w.map(|w| format!(" WHEN {}", w)).unwrap_or_default();
        // Deleting a line: the trigger reads OLD.sync_id directly (the row is gone)
        let del = if t.table == "transcripts" { touch_sql(t.entity, "OLD.sync_id", "OLD.meeting_id", true) } else { del };
        let stmts = [
            format!(
                "CREATE TRIGGER IF NOT EXISTS sync_{tb}_ai AFTER INSERT ON {tb}{w} BEGIN {line_id}{body} END",
                tb = t.table,
                w = when(t.insert_when),
                line_id = line_id,
                body = ins
            ),
            format!(
                "CREATE TRIGGER IF NOT EXISTS sync_{tb}_au AFTER UPDATE{of} ON {tb} BEGIN {body} END",
                tb = t.table,
                of = t.update_of.map(|c| format!(" OF {}", c)).unwrap_or_default(),
                body = upd
            ),
            format!(
                "CREATE TRIGGER IF NOT EXISTS sync_{tb}_ad AFTER DELETE ON {tb}{w} BEGIN {body} END",
                tb = t.table,
                w = when(t.delete_when),
                body = del
            ),
        ];
        for s in stmts {
            sqlx::query(&s).execute(&mut *conn).await?;
        }
    }

    // One-time backfill: everything recorded before sync existed is a
    // change (seq 1) the first time a device pulls
    let done: Option<String> = sqlx::query_scalar("SELECT value FROM sync_kv WHERE key = 'backfilled'")
        .fetch_optional(&mut *conn)
        .await?;
    if done.is_none() {
        sqlx::query("UPDATE transcripts SET sync_id = lower(hex(randomblob(16))) WHERE sync_id IS NULL")
            .execute(&mut *conn)
            .await?;
        sqlx::query("UPDATE sync_counter SET n = MAX(n, 1) WHERE id = 1").execute(&mut *conn).await?;
        let started_ms = "COALESCE(CAST(ROUND((julianday(started_at) - 2440587.5) * 86400000) AS INTEGER), 0)";
        for (entity, sql) in [
            ("recording", format!("SELECT id AS r, id AS m, {} AS t FROM meetings", started_ms)),
            ("line", "SELECT sync_id AS r, meeting_id AS m, 0 AS t FROM transcripts WHERE sync_id IS NOT NULL".to_string()),
            ("strike", "SELECT id AS r, meeting_id AS m, 0 AS t FROM redactions WHERE action = 'strike'".to_string()),
            ("notes", "SELECT DISTINCT meeting_id AS r, meeting_id AS m, 0 AS t FROM meeting_notes".to_string()),
            ("mark", "SELECT id AS r, meeting_id AS m, 0 AS t FROM meeting_markers".to_string()),
            ("ref", "SELECT id AS r, meeting_id AS m, 0 AS t FROM meeting_references".to_string()),
            ("topic", "SELECT id AS r, meeting_id AS m, 0 AS t FROM meeting_topics".to_string()),
        ] {
            sqlx::query(&format!(
                "INSERT OR IGNORE INTO sync_meta (entity, record_id, rec, origin, created_seq, seq, modified_at, last_src, deleted) \
                 SELECT ?, s.r, s.m, NULL, 1, 1, s.t, NULL, 0 FROM ({}) s",
                sql
            ))
            .bind(entity)
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query("INSERT OR REPLACE INTO sync_kv (key, value) VALUES ('backfilled', '1')")
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Key/value (sync on/off, this Mac's id)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn kv_get(pool: &Pool<Sqlite>, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM sync_kv WHERE key = ?").bind(key).fetch_optional(pool).await.ok().flatten()
}

pub async fn kv_set(pool: &Pool<Sqlite>, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT OR REPLACE INTO sync_kv (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(err("Couldn't save the sync setting"))
}

/// This Mac's sync device id (made once)
pub async fn device_id(pool: &Pool<Sqlite>) -> Result<String, String> {
    if let Some(id) = kv_get(pool, "device_id").await {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().hyphenated().to_string();
    sqlx::query("INSERT OR IGNORE INTO sync_kv (key, value) VALUES ('device_id', ?)")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(err("Couldn't save the sync id"))?;
    Ok(kv_get(pool, "device_id").await.unwrap_or(id))
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub paired_at: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
}

pub async fn list_devices(pool: &Pool<Sqlite>) -> Result<Vec<Device>, String> {
    let rows = sqlx::query("SELECT id, name, paired_at, last_sync_at, last_error FROM sync_devices ORDER BY paired_at")
        .fetch_all(pool)
        .await
        .map_err(err("Couldn't read paired devices"))?;
    Ok(rows
        .iter()
        .map(|r| Device {
            id: r.get("id"),
            name: r.get("name"),
            paired_at: r.get("paired_at"),
            last_sync_at: r.get("last_sync_at"),
            last_error: r.get("last_error"),
        })
        .collect())
}

pub async fn add_device(pool: &Pool<Sqlite>, id: &str, name: &str) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO sync_devices (id, name, paired_at) VALUES (?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, paired_at = excluded.paired_at, last_error = NULL",
    )
    .bind(id)
    .bind(name)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(err("Couldn't save the device"))
}

pub async fn remove_device(pool: &Pool<Sqlite>, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM sync_devices WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(err("Couldn't forget the device"))
}

pub async fn device_known(pool: &Pool<Sqlite>, id: &str) -> bool {
    sqlx::query("SELECT 1 FROM sync_devices WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

pub async fn mark_synced(pool: &Pool<Sqlite>, id: &str, error: Option<&str>) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = match error {
        None => sqlx::query("UPDATE sync_devices SET last_sync_at = ?, last_error = NULL WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await,
        Some(e) => sqlx::query("UPDATE sync_devices SET last_error = ? WHERE id = ?").bind(e).bind(id).execute(pool).await,
    };
}

// ═══════════════════════════════════════════════════════════════════════════
// Outgoing: everything changed since a device's cursor
// ═══════════════════════════════════════════════════════════════════════════

struct MetaRow {
    entity: String,
    record_id: String,
    rec: Option<String>,
    origin: Option<String>,
    created_seq: i64,
    modified_at: i64,
    deleted: bool,
}

pub async fn current_seq(pool: &Pool<Sqlite>) -> Result<i64, String> {
    sqlx::query_scalar("SELECT n FROM sync_counter WHERE id = 1")
        .fetch_one(pool)
        .await
        .map_err(err("Couldn't read the change counter"))
}

fn opt_ms(s: Option<String>) -> Option<i64> {
    s.as_deref().and_then(p::parse_time_ms)
}

/// Items for `peer`: changes numbered in `(since, upto]` that didn't come
/// from it. Returns the items (removals first) and `upto`.
pub async fn collect(pool: &Pool<Sqlite>, peer: &str, since: i64, token_key: &[u8]) -> Result<(Vec<Item>, i64), String> {
    let upto = current_seq(pool).await?;
    let rows = sqlx::query(
        "SELECT entity, record_id, rec, origin, created_seq, modified_at, deleted FROM sync_meta \
         WHERE seq > ? AND seq <= ? AND (last_src IS NULL OR last_src != ?) ORDER BY seq",
    )
    .bind(since)
    .bind(upto)
    .bind(peer)
    .fetch_all(pool)
    .await
    .map_err(err("Couldn't read changes"))?;
    let rows: Vec<MetaRow> = rows
        .iter()
        .map(|r| MetaRow {
            entity: r.get("entity"),
            record_id: r.get("record_id"),
            rec: r.get("rec"),
            origin: r.get("origin"),
            created_seq: r.get("created_seq"),
            modified_at: r.get("modified_at"),
            deleted: r.get::<i64, _>("deleted") != 0,
        })
        .collect();
    let gone_recordings: HashSet<String> = sqlx::query_scalar("SELECT record_id FROM sync_meta WHERE entity = 'recording' AND deleted = 1")
        .fetch_all(pool)
        .await
        .map_err(err("Couldn't read changes"))?
        .into_iter()
        .collect();

    let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
    let mut items = Vec::new();
    for row in rows {
        let rec_gone = row.rec.as_ref().map(|r| gone_recordings.contains(r)).unwrap_or(false);
        if row.entity != "recording" && rec_gone {
            continue; // the recording's gone covers it
        }
        if row.deleted {
            if let Some(g) = gone_item(&row) {
                items.push(Item::Gone(g));
            }
            continue;
        }
        let built = match row.entity.as_str() {
            "recording" => recording_item(&mut conn, &row.record_id, row.modified_at).await?.map(Item::Recording),
            "line" => {
                let peer_has_it = row.origin.as_deref() == Some(peer) || row.created_seq <= since;
                line_item(&mut conn, &row.record_id, peer_has_it, token_key).await?
            }
            "strike" => strike_item(&mut conn, &row.record_id).await?.map(Item::Strike),
            "notes" => notes_item(&mut conn, &row.record_id, row.modified_at).await?.map(Item::Notes),
            "mark" => mark_item(&mut conn, &row.record_id, row.modified_at).await?.map(Item::Mark),
            "ref" => ref_item(&mut conn, &row.record_id, row.modified_at).await?.map(Item::Ref),
            "topic" => topic_item(&mut conn, &row.record_id).await?.map(Item::Topic),
            _ => None,
        };
        match built {
            Some(item) => items.push(item),
            // Logged but no longer there (a cascade, or a row we don't sync): gone
            None if row.entity != "strike" => {
                if let Some(g) = gone_item(&row) {
                    items.push(Item::Gone(g));
                }
            }
            None => {}
        }
    }
    items.sort_by_key(|i| i.order());
    Ok((items, upto))
}

fn gone_item(row: &MetaRow) -> Option<p::GoneItem> {
    let id = p::wire_id(&row.record_id)?;
    let entity = match row.entity.as_str() {
        "recording" | "line" | "mark" | "ref" | "topic" | "notes" => row.entity.clone(),
        _ => return None,
    };
    Some(p::GoneItem { entity, id, rec: row.rec.as_deref().and_then(p::wire_id) })
}

async fn recording_item(conn: &mut SqliteConnection, id: &str, modified: i64) -> Result<Option<p::RecordingItem>, String> {
    let Some(wire) = p::wire_id(id) else { return Ok(None) };
    let Some(r) = sqlx::query(
        "SELECT id, title, started_at, ended_at, planned_minutes, class_name, recording_kind FROM meetings WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err("Couldn't read a recording"))?
    else {
        return Ok(None);
    };
    let Some(started) = p::parse_time_ms(&r.get::<String, _>("started_at")) else { return Ok(None) };
    let details = sqlx::query(
        "SELECT calendar_event_id, scheduled_start, scheduled_end, location, meeting_url, notes FROM meeting_details WHERE meeting_id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err("Couldn't read a recording"))?;
    let cal = details
        .map(|d| p::Calendar {
            event: d.get("calendar_event_id"),
            start: opt_ms(d.get("scheduled_start")),
            end: opt_ms(d.get("scheduled_end")),
            location: d.get("location"),
            url: d.get("meeting_url"),
            notes: d.get("notes"),
        })
        .filter(|c| !c.is_empty());
    let people = sqlx::query(
        "SELECT a.email, a.name, a.role FROM meeting_attendees a WHERE a.meeting_id = ? \
         ORDER BY (a.role = 'organizer') DESC, lower(a.email)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(err("Couldn't read a recording"))?
    .iter()
    .map(|a| p::PersonEntry {
        email: a.get::<String, _>("email").to_lowercase(),
        name: Some(a.get::<String, _>("name")).filter(|n| !n.trim().is_empty()),
        role: if a.get::<String, _>("role") == "organizer" { "organizer".into() } else { "attendee".into() },
    })
    .collect();
    let kind = crate::recording_kind::RecordingKind::from_stored(r.get::<Option<String>, _>("recording_kind").as_deref())
        .as_str()
        .to_string();
    Ok(Some(p::RecordingItem {
        id: wire,
        title: r.get("title"),
        started,
        ended: opt_ms(r.get("ended_at")),
        kind,
        notebook: r.get::<Option<String>, _>("class_name").filter(|s| !s.trim().is_empty()),
        planned: r.get("planned_minutes"),
        cal,
        people,
        modified,
    }))
}

async fn line_item(conn: &mut SqliteConnection, sync_id: &str, peer_has_it: bool, token_key: &[u8]) -> Result<Option<Item>, String> {
    let Some(id) = p::wire_id(sync_id) else { return Ok(None) };
    let Some(r) = sqlx::query("SELECT meeting_id, text, speaker, timestamp, source FROM transcripts WHERE sync_id = ?")
        .bind(sync_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read a line"))?
    else {
        return Ok(None);
    };
    let Some(rec) = p::wire_id(&r.get::<String, _>("meeting_id")) else { return Ok(None) };
    let wire_text = p::text_to_wire(&r.get::<String, _>("text"));
    if peer_has_it {
        return Ok(Some(Item::Edit(p::EditItem { id, rec, keep: p::keep_list(token_key, &wire_text) })));
    }
    let Some(at) = p::parse_time_ms(&r.get::<String, _>("timestamp")) else { return Ok(None) };
    Ok(Some(Item::Line(p::LineItem {
        id,
        rec,
        text: wire_text,
        at,
        dur: None,
        speaker: r.get("speaker"),
        src: r.get::<Option<String>, _>("source").filter(|s| s == "screen"),
    })))
}

async fn strike_item(conn: &mut SqliteConnection, id: &str) -> Result<Option<p::StrikeItem>, String> {
    let Some(r) = sqlx::query(
        "SELECT r.id, r.meeting_id, r.kind, r.media_start, r.media_end, r.created_at, r.reason, t.sync_id AS line \
         FROM redactions r LEFT JOIN transcripts t ON t.id = r.transcript_id \
         WHERE r.id = ? AND r.action = 'strike' AND r.kind IN ('words', 'line')",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err("Couldn't read a strike"))?
    else {
        return Ok(None);
    };
    let (Some(wid), Some(rec)) = (p::wire_id(id), p::wire_id(&r.get::<String, _>("meeting_id"))) else { return Ok(None) };
    Ok(Some(p::StrikeItem {
        id: wid,
        rec,
        target: r.get("kind"),
        from: opt_ms(r.get("media_start")),
        to: opt_ms(r.get("media_end")),
        created: opt_ms(r.get("created_at")).unwrap_or(0),
        reason: r.get("reason"),
        line: r.get::<Option<String>, _>("line").as_deref().and_then(p::wire_id),
    }))
}

async fn notes_item(conn: &mut SqliteConnection, meeting_id: &str, modified: i64) -> Result<Option<p::NotesItem>, String> {
    let Some(rec) = p::wire_id(meeting_id) else { return Ok(None) };
    let Some(r) = sqlx::query(
        "SELECT summary, key_topics, decisions, action_items, generated_at, model_used, stale_after_edit \
         FROM meeting_notes WHERE meeting_id = ? ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err("Couldn't read notes"))?
    else {
        return Ok(None);
    };
    let md = notes_markdown(
        r.get("summary"),
        r.get("key_topics"),
        r.get("decisions"),
        r.get("action_items"),
        r.get::<Option<String>, _>("model_used").as_deref(),
    );
    Ok(Some(p::NotesItem {
        rec,
        md,
        made: opt_ms(r.get("generated_at")).unwrap_or(modified),
        stale: r.get::<i64, _>("stale_after_edit") != 0,
        modified,
    }))
}

/// Model name stored with notes that came from the iPhone (Markdown in `summary`)
pub const SYNCED_NOTES_MODEL: &str = "synced-markdown";

fn json_list(s: Option<String>) -> Vec<serde_json::Value> {
    s.and_then(|s| serde_json::from_str::<Vec<serde_json::Value>>(&s).ok()).unwrap_or_default()
}

fn field(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string()).filter(|s| !s.trim().is_empty());
    }
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(str::to_string))
        .filter(|s| !s.trim().is_empty())
}

/// The Mac's structured notes as Markdown in the iPhone's style: bold
/// headings, `•` bullets, headings by the notes' layout (as the Mac's
/// notes panel shows them).
pub fn notes_markdown(
    summary: Option<String>,
    key_topics: Option<String>,
    decisions: Option<String>,
    action_items: Option<String>,
    model_used: Option<&str>,
) -> String {
    if model_used == Some(SYNCED_NOTES_MODEL) {
        return summary.unwrap_or_default();
    }
    let topics: Vec<String> = json_list(key_topics).iter().filter_map(|v| field(v, &["topic", "text", "name"])).collect();
    let decisions: Vec<String> = json_list(decisions)
        .iter()
        .filter_map(|v| {
            let text = field(v, &["text", "term", "decision"])?;
            Some(match field(v, &["made_by", "definition", "example"]) {
                Some(by) => format!("{} — {}", text, by),
                None => text,
            })
        })
        .collect();
    let actions: Vec<String> = json_list(action_items)
        .iter()
        .filter_map(|v| {
            let task = field(v, &["task", "text"])?;
            let who = field(v, &["assignee"]);
            let due = field(v, &["due_date"]);
            Some(match (who, due) {
                (Some(w), Some(d)) => format!("{}: {} · {}", w, task, d),
                (Some(w), None) => format!("{}: {}", w, task),
                (None, Some(d)) => format!("{} · {}", task, d),
                (None, None) => task,
            })
        })
        .collect();
    let (h_sum, h_topics, h_dec, h_act) = match model_used {
        Some("lecture-notes") => ("Lecture summary", "Key concepts", "Definitions and examples", "Announcements and deadlines"),
        Some("personal-notes") => ("Summary", "Key points", "", "To-dos and reminders"),
        _ => ("Summary", "Key topics", "Decisions", "Action items"),
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
        parts.push(format!("**{}**\n{}", h_sum, s.trim()));
    }
    for (heading, list) in [(h_topics, &topics), (h_dec, &decisions), (h_act, &actions)] {
        if heading.is_empty() || list.is_empty() {
            continue;
        }
        let bullets: Vec<String> = list.iter().map(|x| format!("• {}", x)).collect();
        parts.push(format!("**{}**\n{}", heading, bullets.join("\n")));
    }
    parts.join("\n\n")
}

async fn mark_item(conn: &mut SqliteConnection, id: &str, modified: i64) -> Result<Option<p::MarkItem>, String> {
    let Some(r) = sqlx::query("SELECT id, meeting_id, ts, kind, note, created_at FROM meeting_markers WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read a mark"))?
    else {
        return Ok(None);
    };
    let (Some(wid), Some(rec), Some(at)) =
        (p::wire_id(id), p::wire_id(&r.get::<String, _>("meeting_id")), p::parse_time_ms(&r.get::<String, _>("ts")))
    else {
        return Ok(None);
    };
    Ok(Some(p::MarkItem {
        id: wid,
        rec,
        at,
        kind: r.get("kind"),
        note: r.get::<Option<String>, _>("note").filter(|n| !n.trim().is_empty()),
        created: opt_ms(r.get("created_at")).unwrap_or(at),
        modified,
    }))
}

async fn ref_item(conn: &mut SqliteConnection, id: &str, modified: i64) -> Result<Option<p::RefItem>, String> {
    let Some(r) = sqlx::query("SELECT meeting_id, url, title, note, created_at FROM meeting_references WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read a link"))?
    else {
        return Ok(None);
    };
    let (Some(wid), Some(rec)) = (p::wire_id(id), p::wire_id(&r.get::<String, _>("meeting_id"))) else { return Ok(None) };
    Ok(Some(p::RefItem {
        id: wid,
        rec,
        url: r.get("url"),
        title: r.get("title"),
        note: r.get("note"),
        created: opt_ms(r.get("created_at")).unwrap_or(0),
        modified,
    }))
}

async fn topic_item(conn: &mut SqliteConnection, id: &str) -> Result<Option<p::TopicItem>, String> {
    let Some(r) = sqlx::query("SELECT meeting_id, topic, topic_key, confidence, source, created_at FROM meeting_topics WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read a topic"))?
    else {
        return Ok(None);
    };
    let (Some(wid), Some(rec)) = (p::wire_id(id), p::wire_id(&r.get::<String, _>("meeting_id"))) else { return Ok(None) };
    let conf: Option<f64> = r.get("confidence");
    Ok(Some(p::TopicItem {
        id: wid,
        rec,
        label: r.get("topic"),
        key: r.get("topic_key"),
        conf: (conf.unwrap_or(1.0).clamp(0.0, 1.0) * 1000.0).round() as i64,
        source: r.get("source"),
        created: opt_ms(r.get("created_at")).unwrap_or(0),
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Incoming: apply a peer's items
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ApplyReport {
    pub applied: usize,
    pub skipped: usize,
    /// Ids the peer should send again next time (couldn't be applied now)
    pub retry: Vec<String>,
    /// Problems (no content), for the device's last error
    pub errors: Vec<String>,
}

async fn set_apply(conn: &mut SqliteConnection, src: &str, at: i64) -> Result<(), String> {
    sqlx::query("INSERT OR REPLACE INTO sync_apply (key, value) VALUES ('src', ?), ('at', ?)")
        .bind(src)
        .bind(at.to_string())
        .execute(&mut *conn)
        .await
        .map(|_| ())
        .map_err(err("Database busy"))
}

async fn clear_apply(conn: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query("DELETE FROM sync_apply").execute(&mut *conn).await.map(|_| ()).map_err(err("Database busy"))
}

async fn meta(conn: &mut SqliteConnection, entity: &str, record_id: &str) -> Result<Option<(i64, bool)>, String> {
    let row = sqlx::query("SELECT modified_at, deleted FROM sync_meta WHERE entity = ? AND record_id = ?")
        .bind(entity)
        .bind(record_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read sync state"))?;
    Ok(row.map(|r| (r.get::<i64, _>("modified_at"), r.get::<i64, _>("deleted") != 0)))
}

async fn is_gone(conn: &mut SqliteConnection, entity: &str, record_id: &str) -> Result<bool, String> {
    Ok(meta(conn, entity, record_id).await?.map(|m| m.1).unwrap_or(false))
}

/// Remember an id as deleted, even if this Mac never had it, so a late copy
/// is never imported.
async fn remember_gone(conn: &mut SqliteConnection, entity: &str, record_id: &str, rec: Option<&str>, src: &str) -> Result<(), String> {
    sqlx::query(&format!(
        "INSERT INTO sync_meta (entity, record_id, rec, origin, created_seq, seq, modified_at, last_src, deleted) \
         SELECT ?, ?, ?, ?, n, n, {now}, ?, 1 FROM sync_counter WHERE id = 1 \
         ON CONFLICT(entity, record_id) DO UPDATE SET deleted = 1",
        now = NOW_MS_SQL
    ))
    .bind(entity)
    .bind(record_id)
    .bind(rec)
    .bind(src)
    .bind(src)
    .execute(&mut *conn)
    .await
    .map(|_| ())
    .map_err(err("Couldn't save sync state"))
}

async fn meeting_exists(conn: &mut SqliteConnection, id: &str) -> Result<bool, String> {
    Ok(sqlx::query("SELECT 1 FROM meetings WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(err("Couldn't read a recording"))?
        .is_some())
}

/// Apply `items` from `peer`. Removals go one at a time through the
/// redaction module's purge path; content goes in one transaction.
pub async fn apply(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    peer: &str,
    token_key: &[u8],
    mut items: Vec<Item>,
) -> ApplyReport {
    items.sort_by_key(|i| i.order());
    let mut report = ApplyReport::default();
    let (removals, content): (Vec<Item>, Vec<Item>) = items.into_iter().partition(|i| i.is_removal());
    for item in removals {
        let id = item_id(&item);
        match apply_removal(pool, env, peer, token_key, &item).await {
            Ok(true) => report.applied += 1,
            Ok(false) => report.skipped += 1,
            Err(e) => {
                report.errors.push(e);
                if let Some(id) = id {
                    report.retry.push(id);
                }
            }
        }
    }
    if !content.is_empty() {
        if let Err(e) = apply_content(pool, peer, &content, &mut report).await {
            report.errors.push(e);
            report.retry.extend(content.iter().filter_map(item_id));
        }
    }
    report
}

fn item_id(item: &Item) -> Option<String> {
    Some(match item {
        Item::Recording(x) => x.id.clone(),
        Item::Line(x) => x.id.clone(),
        Item::Edit(x) => x.id.clone(),
        Item::Strike(x) => x.id.clone(),
        Item::Notes(x) => x.rec.clone(),
        Item::Mark(x) => x.id.clone(),
        Item::Ref(x) => x.id.clone(),
        Item::Topic(x) => x.id.clone(),
        Item::Gone(x) => x.id.clone(),
    })
}

/// Ok(true) applied, Ok(false) nothing to do, Err → retry later
async fn apply_removal(
    pool: &Pool<Sqlite>,
    env: &RedactionEnv,
    peer: &str,
    token_key: &[u8],
    item: &Item,
) -> Result<bool, String> {
    let now = p::ms(chrono::Utc::now());
    match item {
        Item::Strike(s) => {
            let (Some(id), Some(rec)) = (p::simple_id(&s.id), p::wire_id(&s.rec)) else { return Ok(false) };
            if s.target != "words" && s.target != "line" {
                return Ok(false);
            }
            let mut tx = pool.begin().await.map_err(err("Database busy"))?;
            if !meeting_exists(&mut tx, &rec).await? {
                return Ok(false);
            }
            let exists = sqlx::query("SELECT 1 FROM redactions WHERE id = ?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(err("Database busy"))?
                .is_some();
            if exists {
                return Ok(false);
            }
            let line_local: Option<i64> = match s.line.as_deref().and_then(p::simple_id) {
                Some(l) => sqlx::query_scalar("SELECT id FROM transcripts WHERE sync_id = ?")
                    .bind(l)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(err("Database busy"))?,
                None => None,
            };
            set_apply(&mut tx, peer, now).await?;
            sqlx::query(
                "INSERT INTO redactions (id, meeting_id, kind, action, media_start, media_end, created_at, reason, transcript_id, item_count) \
                 VALUES (?, ?, ?, 'strike', ?, ?, ?, ?, ?, 1)",
            )
            .bind(&id)
            .bind(&rec)
            .bind(&s.target)
            .bind(s.from.map(|t| p::from_ms(t).to_rfc3339()))
            .bind(s.to.map(|t| p::from_ms(t).to_rfc3339()))
            .bind(p::from_ms(s.created).to_rfc3339())
            .bind(&s.reason)
            .bind(line_local)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save a strike"))?;
            clear_apply(&mut tx).await?;
            tx.commit().await.map_err(err("Couldn't save a strike"))?;
            Ok(true)
        }
        Item::Edit(e) => {
            let (Some(line), Some(rec)) = (p::simple_id(&e.id), p::wire_id(&e.rec)) else { return Ok(false) };
            let Some(keep) = merge::parse_keep(&e.keep) else { return Ok(false) };
            let tid: Option<i64> = sqlx::query_scalar("SELECT id FROM transcripts WHERE sync_id = ? AND meeting_id = ?")
                .bind(&line)
                .bind(&rec)
                .fetch_optional(pool)
                .await
                .map_err(err("Database busy"))?;
            let Some(tid) = tid else { return Ok(false) };
            let key = token_key.to_vec();
            let outcome = redaction::edit_line_synced(pool, env, &rec, tid, Some((peer, now)), move |text| {
                let local = p::tokens_by(text, p::mac_marker_id);
                plan_to_ops(merge::plan(&local, &keep, &key))
            })
            .await?;
            Ok(outcome)
        }
        Item::Gone(g) => apply_gone(pool, env, peer, g).await,
        _ => Ok(false),
    }
}

fn plan_to_ops(plan: merge::LinePlan) -> Vec<redaction::SyncedOp> {
    plan.ops
        .into_iter()
        .map(|op| match op {
            merge::Op::Remove { start16, end16, marker, strike } => redaction::SyncedOp::Remove {
                start16,
                end16,
                marker: marker.as_deref().and_then(p::simple_id),
                strike,
            },
            merge::Op::Insert { at16, markers } => redaction::SyncedOp::Insert {
                at16,
                markers: markers.iter().filter_map(|m| p::simple_id(m)).collect(),
            },
        })
        .collect()
}

async fn apply_gone(pool: &Pool<Sqlite>, env: &RedactionEnv, peer: &str, g: &p::GoneItem) -> Result<bool, String> {
    let now = p::ms(chrono::Utc::now());
    match g.entity.as_str() {
        "recording" => {
            let Some(id) = p::wire_id(&g.id) else { return Ok(false) };
            if env.recording_meetings.iter().any(|m| *m == id) {
                return Err("That recording is still being recorded on this Mac".into());
            }
            let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
            let exists = meeting_exists(&mut conn, &id).await?;
            remember_gone(&mut conn, "recording", &id, Some(&id), peer).await?;
            drop(conn);
            if !exists {
                return Ok(false);
            }
            delete_recording(pool, env, &id).await?;
            Ok(true)
        }
        "line" => {
            let Some(line) = p::simple_id(&g.id) else { return Ok(false) };
            let row: Option<(i64, String)> = sqlx::query_as("SELECT id, meeting_id FROM transcripts WHERE sync_id = ?")
                .bind(&line)
                .fetch_optional(pool)
                .await
                .map_err(err("Database busy"))?;
            {
                let mut conn = pool.acquire().await.map_err(err("Database busy"))?;
                remember_gone(&mut conn, "line", &line, row.as_ref().map(|r| r.1.as_str()), peer).await?;
            }
            let Some((tid, meeting)) = row else { return Ok(false) };
            // Every word of the line goes, with delete semantics; strike
            // markers already in it stay (strikes are permanent)
            redaction::edit_line_synced(pool, env, &meeting, tid, Some((peer, now)), |text| {
                let toks = p::tokens_by(text, p::mac_marker_id);
                let mut ops = Vec::new();
                let mut run: Option<(usize, usize)> = None;
                for t in &toks {
                    match t {
                        p::Tok::Word { start16, end16, .. } => {
                            run = Some(match run {
                                Some((s, _)) => (s, *end16),
                                None => (*start16, *end16),
                            })
                        }
                        p::Tok::Marker { .. } => {
                            if let Some((s, e)) = run.take() {
                                ops.push(redaction::SyncedOp::Remove { start16: s, end16: e, marker: None, strike: false });
                            }
                        }
                    }
                }
                if let Some((s, e)) = run {
                    ops.push(redaction::SyncedOp::Remove { start16: s, end16: e, marker: None, strike: false });
                }
                ops.reverse();
                ops
            })
            .await
        }
        "mark" | "ref" | "topic" | "notes" => {
            let (table, key) = match g.entity.as_str() {
                "mark" => ("meeting_markers", "id"),
                "ref" => ("meeting_references", "id"),
                "topic" => ("meeting_topics", "id"),
                _ => ("meeting_notes", "meeting_id"),
            };
            let local = if g.entity == "notes" { p::wire_id(&g.id) } else { p::simple_id(&g.id) };
            let Some(local) = local else { return Ok(false) };
            let mut tx = pool.begin().await.map_err(err("Database busy"))?;
            set_apply(&mut tx, peer, now).await?;
            let n = sqlx::query(&format!("DELETE FROM {} WHERE {} = ?", table, key))
                .bind(&local)
                .execute(&mut *tx)
                .await
                .map_err(err("Couldn't remove it"))?
                .rows_affected();
            if g.entity != "notes" {
                remember_gone(&mut tx, &g.entity, &local, g.rec.as_deref().and_then(p::wire_id).as_deref(), peer).await?;
            }
            clear_apply(&mut tx).await?;
            tx.commit().await.map_err(err("Couldn't remove it"))?;
            Ok(n > 0)
        }
        _ => Ok(false),
    }
}

/// Delete a recording the way Delete recording does on this Mac: the
/// database purge (transcript, topics, chat answers, links, markers, notes…)
/// and the recording's file folders.
pub async fn delete_recording(pool: &Pool<Sqlite>, env: &RedactionEnv, id: &str) -> Result<(), String> {
    let db = crate::database::DatabaseManager::from_pool(pool.clone());
    db.delete_meeting(id).await.map_err(err("Couldn't delete the recording"))?;
    for d in crate::commands::meeting_file_dirs(&env.app_data_dir, &env.cache_dir, id).iter().filter(|d| d.exists()) {
        if let Err(e) = std::fs::remove_dir_all(d) {
            log::warn!("Sync: could not remove a deleted recording's folder: {}", e);
        }
    }
    Ok(())
}

async fn apply_content(pool: &Pool<Sqlite>, peer: &str, items: &[Item], report: &mut ApplyReport) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(err("Database busy"))?;
    for item in items {
        let changed = match item {
            Item::Recording(r) => apply_recording(&mut tx, peer, r).await?,
            Item::Line(l) => apply_line(&mut tx, peer, l).await?,
            Item::Notes(n) => apply_notes(&mut tx, peer, n).await?,
            Item::Mark(m) => apply_mark(&mut tx, peer, m).await?,
            Item::Ref(r) => apply_ref(&mut tx, peer, r).await?,
            Item::Topic(t) => apply_topic(&mut tx, peer, t).await?,
            _ => false,
        };
        if changed {
            report.applied += 1;
        } else {
            report.skipped += 1;
        }
    }
    clear_apply(&mut tx).await?;
    tx.commit().await.map_err(err("Couldn't save synced changes"))
}

/// LWW: should an incoming record with `incoming_mod` replace ours?
async fn newer(conn: &mut SqliteConnection, entity: &str, record_id: &str, incoming_mod: i64) -> Result<Option<bool>, String> {
    match meta(conn, entity, record_id).await? {
        Some((_, true)) => Ok(None), // gone: never re-imported
        Some((local_mod, false)) => Ok(Some(incoming_mod > local_mod)),
        None => Ok(Some(true)),
    }
}

async fn apply_recording(tx: &mut SqliteConnection, peer: &str, r: &p::RecordingItem) -> Result<bool, String> {
    let Some(id) = p::wire_id(&r.id) else { return Ok(false) };
    let exists = meeting_exists(tx, &id).await?;
    match newer(tx, "recording", &id, r.modified).await? {
        None => return Ok(false),
        Some(false) if exists => return Ok(false),
        _ => {}
    }
    set_apply(tx, peer, r.modified).await?;
    let started = p::from_ms(r.started).to_rfc3339();
    let ended = r.ended.map(|e| p::from_ms(e).to_rfc3339());
    let duration = r.ended.map(|e| ((e - r.started) / 1000).max(0));
    let kind = crate::recording_kind::RecordingKind::from_stored(Some(&r.kind)).as_str().to_string();
    let notebook = r.notebook.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if exists {
        sqlx::query(
            "UPDATE meetings SET title = ?, started_at = ?, ended_at = ?, duration_seconds = ?, planned_minutes = ?, \
             class_name = ?, recording_kind = ? WHERE id = ?",
        )
        .bind(&r.title)
        .bind(&started)
        .bind(&ended)
        .bind(duration)
        .bind(r.planned)
        .bind(notebook)
        .bind(&kind)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't update a recording"))?;
    } else {
        sqlx::query(
            "INSERT INTO meetings (id, title, started_at, ended_at, duration_seconds, planned_minutes, class_name, recording_kind) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&r.title)
        .bind(&started)
        .bind(&ended)
        .bind(duration)
        .bind(r.planned)
        .bind(notebook)
        .bind(&kind)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't add a recording"))?;
    }
    // Calendar details and people
    match &r.cal {
        Some(c) => {
            sqlx::query(
                "INSERT INTO meeting_details (meeting_id, calendar_event_id, scheduled_start, scheduled_end, location, meeting_url, notes) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(meeting_id) DO UPDATE SET \
                 calendar_event_id = excluded.calendar_event_id, scheduled_start = excluded.scheduled_start, \
                 scheduled_end = excluded.scheduled_end, location = excluded.location, meeting_url = excluded.meeting_url, \
                 notes = excluded.notes",
            )
            .bind(&id)
            .bind(&c.event)
            .bind(c.start.map(|t| p::from_ms(t).to_rfc3339()))
            .bind(c.end.map(|t| p::from_ms(t).to_rfc3339()))
            .bind(&c.location)
            .bind(&c.url)
            .bind(&c.notes)
            .execute(&mut *tx)
            .await
            .map_err(err("Couldn't save calendar details"))?;
            sqlx::query("UPDATE meetings SET calendar_event_id = ? WHERE id = ?")
                .bind(&c.event)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(err("Couldn't save calendar details"))?;
        }
        None => {
            sqlx::query("DELETE FROM meeting_details WHERE meeting_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(err("Couldn't save calendar details"))?;
        }
    }
    let mut keep_emails: Vec<String> = Vec::new();
    for person in &r.people {
        let email = person.email.trim().to_lowercase();
        if email.is_empty() {
            continue;
        }
        let name = person
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| crate::attendee_intel::extract_name_from_email(&email));
        let role = if person.role == "organizer" { "organizer" } else { "attendee" };
        sqlx::query(
            "INSERT INTO people (id, email, name) VALUES (?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET name = COALESCE(people.name, excluded.name)",
        )
        .bind(&email)
        .bind(&email)
        .bind(&name)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save people"))?;
        sqlx::query(
            "INSERT INTO meeting_attendees (meeting_id, name, email, role) VALUES (?, ?, ?, ?) \
             ON CONFLICT(meeting_id, email) DO UPDATE SET name = excluded.name, role = excluded.role",
        )
        .bind(&id)
        .bind(&name)
        .bind(&email)
        .bind(role)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save people"))?;
        keep_emails.push(email);
    }
    let existing: Vec<String> = sqlx::query_scalar("SELECT email FROM meeting_attendees WHERE meeting_id = ?")
        .bind(&id)
        .fetch_all(&mut *tx)
        .await
        .map_err(err("Couldn't save people"))?;
    for e in existing {
        if !keep_emails.contains(&e.to_lowercase()) {
            sqlx::query("DELETE FROM meeting_attendees WHERE meeting_id = ? AND email = ?")
                .bind(&id)
                .bind(&e)
                .execute(&mut *tx)
                .await
                .map_err(err("Couldn't save people"))?;
        }
    }
    // The triggers stamped `now`-ish times inside this transaction with the
    // item's own time; make sure the record carries exactly that
    sqlx::query("UPDATE sync_meta SET modified_at = ? WHERE entity = 'recording' AND record_id = ?")
        .bind(r.modified)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save sync state"))?;
    Ok(true)
}

async fn apply_line(tx: &mut SqliteConnection, peer: &str, l: &p::LineItem) -> Result<bool, String> {
    let (Some(sid), Some(rec)) = (p::simple_id(&l.id), p::wire_id(&l.rec)) else { return Ok(false) };
    if is_gone(tx, "line", &sid).await? || !meeting_exists(tx, &rec).await? {
        return Ok(false);
    }
    let exists = sqlx::query("SELECT 1 FROM transcripts WHERE sync_id = ?")
        .bind(&sid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(err("Database busy"))?
        .is_some();
    if exists {
        return Ok(false); // lines change only by edits
    }
    let text = p::text_from_wire(&l.text);
    if text.trim().is_empty() {
        return Ok(false);
    }
    set_apply(tx, peer, p::ms(chrono::Utc::now())).await?;
    sqlx::query(
        "INSERT INTO transcripts (meeting_id, text, speaker, timestamp, is_final, confidence, text_hash, sync_id, source) \
         VALUES (?, ?, ?, ?, 1, 0.0, ?, ?, ?)",
    )
    .bind(&rec)
    .bind(&text)
    .bind(&l.speaker)
    .bind(p::from_ms(l.at).to_rfc3339())
    .bind(crate::database::transcript_text_hash(&text))
    .bind(&sid)
    .bind(l.src.as_deref().filter(|s| *s == "screen"))
    .execute(&mut *tx)
    .await
    .map_err(err("Couldn't add a line"))?;
    Ok(true)
}

async fn apply_notes(tx: &mut SqliteConnection, peer: &str, n: &p::NotesItem) -> Result<bool, String> {
    let Some(rec) = p::wire_id(&n.rec) else { return Ok(false) };
    if !meeting_exists(tx, &rec).await? || is_gone(tx, "recording", &rec).await? {
        return Ok(false);
    }
    let local_mod = meta(tx, "notes", &rec).await?.filter(|m| !m.1).map(|m| m.0);
    let has_local = sqlx::query("SELECT 1 FROM meeting_notes WHERE meeting_id = ?")
        .bind(&rec)
        .fetch_optional(&mut *tx)
        .await
        .map_err(err("Database busy"))?
        .is_some();
    if has_local && local_mod.map_or(false, |m| n.modified <= m) {
        return Ok(false);
    }
    set_apply(tx, peer, n.modified).await?;
    sqlx::query("DELETE FROM meeting_notes WHERE meeting_id = ?")
        .bind(&rec)
        .execute(&mut *tx)
        .await
        .map_err(err("Couldn't save notes"))?;
    sqlx::query(
        "INSERT INTO meeting_notes (id, meeting_id, summary, generated_at, model_used, stale_after_edit) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .bind(&rec)
    .bind(&n.md)
    .bind(p::from_ms(n.made).format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(SYNCED_NOTES_MODEL)
    .bind(n.stale as i32)
    .execute(&mut *tx)
    .await
    .map_err(err("Couldn't save notes"))?;
    Ok(true)
}

async fn apply_mark(tx: &mut SqliteConnection, peer: &str, m: &p::MarkItem) -> Result<bool, String> {
    let (Some(id), Some(rec)) = (p::simple_id(&m.id), p::wire_id(&m.rec)) else { return Ok(false) };
    if !["important", "question", "test"].contains(&m.kind.as_str()) || !meeting_exists(tx, &rec).await? {
        return Ok(false);
    }
    if newer(tx, "mark", &id, m.modified).await? != Some(true) {
        return Ok(false);
    }
    set_apply(tx, peer, m.modified).await?;
    sqlx::query(
        "INSERT INTO meeting_markers (id, meeting_id, ts, kind, note, created_at) VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET ts = excluded.ts, kind = excluded.kind, note = excluded.note",
    )
    .bind(&id)
    .bind(&rec)
    .bind(p::from_ms(m.at).to_rfc3339())
    .bind(&m.kind)
    .bind(m.note.as_deref().filter(|n| !n.trim().is_empty()))
    .bind(p::from_ms(m.created).to_rfc3339())
    .execute(&mut *tx)
    .await
    .map_err(err("Couldn't save a mark"))?;
    Ok(true)
}

async fn apply_ref(tx: &mut SqliteConnection, peer: &str, r: &p::RefItem) -> Result<bool, String> {
    let (Some(id), Some(rec)) = (p::simple_id(&r.id), p::wire_id(&r.rec)) else { return Ok(false) };
    let lower = r.url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) || !meeting_exists(tx, &rec).await? {
        return Ok(false); // http(s) links only, like a link added by hand
    }
    if newer(tx, "ref", &id, r.modified).await? != Some(true) {
        return Ok(false);
    }
    set_apply(tx, peer, r.modified).await?;
    sqlx::query(
        "INSERT INTO meeting_references (id, meeting_id, url, title, note, created_at) VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET url = excluded.url, title = excluded.title, note = excluded.note",
    )
    .bind(&id)
    .bind(&rec)
    .bind(&r.url)
    .bind(&r.title)
    .bind(&r.note)
    .bind(p::from_ms(r.created).to_rfc3339())
    .execute(&mut *tx)
    .await
    .map_err(err("Couldn't save a link"))?;
    Ok(true)
}

async fn apply_topic(tx: &mut SqliteConnection, peer: &str, t: &p::TopicItem) -> Result<bool, String> {
    let (Some(id), Some(rec)) = (p::simple_id(&t.id), p::wire_id(&t.rec)) else { return Ok(false) };
    if !["ai", "user"].contains(&t.source.as_str()) || !meeting_exists(tx, &rec).await? || is_gone(tx, "topic", &id).await? {
        return Ok(false);
    }
    set_apply(tx, peer, p::ms(chrono::Utc::now())).await?;
    let n = sqlx::query(
        "INSERT OR IGNORE INTO meeting_topics (id, meeting_id, topic, topic_key, confidence, source, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&rec)
    .bind(&t.label)
    .bind(&t.key)
    .bind((t.conf.clamp(0, 1000) as f64) / 1000.0)
    .bind(&t.source)
    .bind(p::from_ms(t.created).to_rfc3339())
    .execute(&mut *tx)
    .await
    .map_err(err("Couldn't save a topic"))?
    .rows_affected();
    Ok(n > 0)
}
