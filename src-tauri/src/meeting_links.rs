//! Links & References on every meeting (docs/LINKS.md).
//!
//! Three sources, one list per meeting:
//!
//! - **Said**: URLs and site names in the transcript ("example dot com").
//! - **On screen**: URLs in the meeting's screen text (OCR / accessibility
//!   snapshots and window titles), plus, in the DMG build, the frontmost
//!   browser's address captured while recording ([`browser_url`]). Those are
//!   `text_snapshots` rows with `source = 'browser_url'`, so every screen
//!   purge (Delete, Strike, time ranges, meeting delete) already covers them.
//! - **Added**: references the user adds (syllabus, a reading, slides) in
//!   `meeting_references`.
//!
//! Privacy design: "Said" and "On screen" are derived from the transcript
//! and screen text each time the list is built, never stored. Deleting or
//! striking that text removes the links with it. Hiding a detected link
//! stores only a salted hash of its normalized URL (`meeting_link_hidden`),
//! and hashes whose link no longer appears are pruned after every edit.
//! Nothing here touches the network: no page titles, no favicons.

pub mod detect;
#[cfg(not(feature = "mas"))]
pub mod browser_url;
#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnection;
use sqlx::{Pool, Row, Sqlite};
use std::collections::{HashMap, HashSet};

/// `text_snapshots.source` of a browser address captured while recording.
pub const BROWSER_URL_SOURCE: &str = "browser_url";
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_NOTE_CHARS: usize = 2000;
pub const MAX_URL_CHARS: usize = 2048;
pub const OPEN_REFUSED: &str = "Only web links (http and https) can be opened.";

// ═══════════════════════════════════════════════════════════════════════════
// Schema (runs inside DatabaseManager::run_migrations on its one connection)
// ═══════════════════════════════════════════════════════════════════════════

pub async fn ensure_schema(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_references (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            url TEXT NOT NULL,
            title TEXT,
            note TEXT,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_meeting_references_meeting ON meeting_references(meeting_id)")
        .execute(&mut *conn)
        .await?;
    // Hidden detected links: a salted hash of the normalized URL, never the URL
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_link_hidden (
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            url_hash TEXT NOT NULL,
            PRIMARY KEY (meeting_id, url_hash)
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Meeting delete: the meeting's references and hidden-link hashes. They
/// also go with `ON DELETE CASCADE`; this doesn't depend on it.
pub async fn purge_meeting(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM meeting_references WHERE meeting_id = ?").bind(meeting_id).execute(pool).await?;
    sqlx::query("DELETE FROM meeting_link_hidden WHERE meeting_id = ?").bind(meeting_id).execute(pool).await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// The list
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeetingLink {
    /// Normalized URL without scheme (the dedupe key; what Hide hashes)
    pub key: String,
    /// What Open opens (http or https only)
    pub url: String,
    /// Host without `www.` (with `:port` if not the default)
    pub host: String,
    /// Path, query and `#/route` after the host ("" for a home page)
    pub path: String,
    /// The reference's title, else a browser window title seen with it
    pub title: Option<String>,
    pub note: Option<String>,
    /// "added", "said", "screen", in that order, for each source it has
    pub sources: Vec<&'static str>,
    /// Mentions in the transcript
    pub said_count: u32,
    /// Screen captures it appeared in
    pub screen_count: u32,
    /// First time said or seen, in ms from the meeting start (Recordings time)
    pub first_ms: Option<i64>,
    pub first_at: Option<String>,
    /// "said" or "screen"
    pub first_source: Option<&'static str>,
    /// Set for an added reference
    pub reference_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingLinks {
    pub meeting_id: String,
    pub started_at: String,
    /// Added references first (in the order added), then detected links by
    /// the time they first came up
    pub links: Vec<MeetingLink>,
    /// Detected links the user hid (still derived from the meeting)
    pub hidden: Vec<MeetingLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingReference {
    pub id: String,
    pub meeting_id: String,
    pub url: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

/// Per-meeting salted hash of a normalized URL (the only thing Hide stores).
pub fn hidden_hash(meeting_id: &str, key: &str) -> String {
    let mut h = Sha256::new();
    h.update(meeting_id.as_bytes());
    h.update(b"\n");
    h.update(key.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Default)]
struct Group {
    link: Option<detect::Normalized>,
    any_https: bool,
    title: Option<String>,
    said: u32,
    screen: u32,
    first: Option<(DateTime<Utc>, &'static str)>,
}

impl Group {
    fn add(&mut self, n: &detect::Normalized, at: Option<DateTime<Utc>>, source: &'static str) {
        if self.link.is_none() {
            self.link = Some(n.clone());
        }
        self.any_https |= n.scheme == "https";
        match source {
            "said" => self.said += 1,
            _ => self.screen += 1,
        }
        if let Some(t) = at {
            if self.first.map_or(true, |(f, _)| t < f) {
                self.first = Some((t, source));
            }
        }
    }

    fn into_link(self, started: DateTime<Utc>) -> MeetingLink {
        let n = self.link.expect("group has a link");
        // Seen as both http and https (or without a scheme): open https
        let scheme = if self.any_https { "https" } else { n.scheme };
        let url = format!("{}://{}{}", scheme, if n.www { "www." } else { "" }, n.key);
        let mut sources = Vec::new();
        if self.said > 0 {
            sources.push("said");
        }
        if self.screen > 0 {
            sources.push("screen");
        }
        MeetingLink {
            key: n.key.clone(),
            url,
            host: n.host.clone(),
            path: n.path.clone(),
            title: self.title,
            note: None,
            sources,
            said_count: self.said,
            screen_count: self.screen,
            first_ms: self.first.map(|(t, _)| (t - started).num_milliseconds().max(0)),
            first_at: self.first.map(|(t, _)| t.to_rfc3339()),
            first_source: self.first.map(|(_, s)| s),
            reference_id: None,
            created_at: None,
        }
    }
}

/// Detected links (Said + On screen), grouped by key, in first-seen order.
async fn detect_meeting(
    conn: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<(Vec<String>, HashMap<String, Group>), sqlx::Error> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Group> = HashMap::new();
    let mut put = |n: detect::Normalized, at: Option<DateTime<Utc>>, source: &'static str, title: Option<&str>| {
        let g = groups.entry(n.key.clone()).or_insert_with(|| {
            order.push(n.key.clone());
            Group::default()
        });
        g.add(&n, at, source);
        if g.title.is_none() {
            if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
                g.title = Some(t.chars().take(MAX_TITLE_CHARS).collect());
            }
        }
    };

    // Said: final transcript lines (strike markers are boundaries)
    let rows = sqlx::query(
        "SELECT text, timestamp FROM transcripts WHERE meeting_id = ? AND is_final = 1 ORDER BY timestamp ASC, id ASC",
    )
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await?;
    for r in rows {
        let text: String = r.get("text");
        let at = parse_ts(&r.get::<String, _>("timestamp"));
        for f in detect::detect(&text, true) {
            put(f.link, at, "said", None);
        }
    }

    // On screen: the meeting's screen text (same rows the screen purges
    // remove) and browser addresses. Counted once per capture.
    let rows = sqlx::query(
        "SELECT ts, text, source, window_title FROM text_snapshots WHERE meeting_id = ?1 \
         OR state_id IN (SELECT state_id FROM screen_states WHERE meeting_id = ?1) \
         OR episode_id IN (SELECT episode_id FROM document_episodes WHERE meeting_id = ?1) \
         ORDER BY ts ASC",
    )
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await?;
    for r in rows {
        let text: String = r.get("text");
        let source: Option<String> = r.get("source");
        let title: Option<String> = r.get("window_title");
        let at = parse_ts(&r.get::<String, _>("ts"));
        if source.as_deref() == Some(BROWSER_URL_SOURCE) {
            if let Some(n) = detect::normalize(&text) {
                put(n, at, "screen", title.as_deref());
            }
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        let found = detect::detect(&text, false)
            .into_iter()
            .chain(title.as_deref().map(|t| detect::detect(t, false)).unwrap_or_default());
        for f in found {
            if seen.insert(f.link.key.clone()) {
                put(f.link, at, "screen", None);
            }
        }
    }
    Ok((order, groups))
}

async fn meeting_started(conn: &mut SqliteConnection, meeting_id: &str) -> Result<DateTime<Utc>, String> {
    let s: Option<String> = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| format!("Couldn't read the meeting: {}", e))?;
    let s = s.ok_or_else(|| "This meeting no longer exists".to_string())?;
    parse_ts(&s).ok_or_else(|| "This meeting has no start time".to_string())
}

fn reference_from_row(r: &sqlx::sqlite::SqliteRow) -> MeetingReference {
    MeetingReference {
        id: r.get("id"),
        meeting_id: r.get("meeting_id"),
        url: r.get("url"),
        title: r.get("title"),
        note: r.get("note"),
        created_at: r.get("created_at"),
    }
}

/// The Links list for one meeting.
pub async fn list_links(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<MeetingLinks, String> {
    let mut conn = pool.acquire().await.map_err(|e| format!("Database busy: {}", e))?;
    let started = meeting_started(&mut conn, meeting_id).await?;
    let (order, mut groups) =
        detect_meeting(&mut conn, meeting_id).await.map_err(|e| format!("Couldn't read the meeting: {}", e))?;

    let refs: Vec<MeetingReference> = sqlx::query(
        "SELECT id, meeting_id, url, title, note, created_at FROM meeting_references WHERE meeting_id = ? \
         ORDER BY created_at ASC, rowid ASC",
    )
    .bind(meeting_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("Couldn't read the references: {}", e))?
    .iter()
    .map(reference_from_row)
    .collect();

    let mut links = Vec::new();
    for r in refs {
        let n = detect::normalize(&r.url);
        let key = n.as_ref().map(|n| n.key.clone()).unwrap_or_else(|| r.url.clone());
        // A detected link with the same address joins its reference
        let detected = groups.remove(&key).map(|g| g.into_link(started));
        let mut sources = vec!["added"];
        if let Some(d) = &detected {
            sources.extend(d.sources.iter().copied());
        }
        links.push(MeetingLink {
            key,
            url: r.url.clone(),
            host: n.as_ref().map(|n| n.host.clone()).unwrap_or_default(),
            path: n.as_ref().map(|n| n.path.clone()).unwrap_or_default(),
            title: r.title.clone().or_else(|| detected.as_ref().and_then(|d| d.title.clone())),
            note: r.note.clone(),
            sources,
            said_count: detected.as_ref().map_or(0, |d| d.said_count),
            screen_count: detected.as_ref().map_or(0, |d| d.screen_count),
            first_ms: detected.as_ref().and_then(|d| d.first_ms),
            first_at: detected.as_ref().and_then(|d| d.first_at.clone()),
            first_source: detected.as_ref().and_then(|d| d.first_source),
            reference_id: Some(r.id.clone()),
            created_at: Some(r.created_at.clone()),
        });
    }

    let hidden_hashes: HashSet<String> =
        sqlx::query_scalar("SELECT url_hash FROM meeting_link_hidden WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| format!("Couldn't read hidden links: {}", e))?
            .into_iter()
            .collect();
    let mut detected: Vec<MeetingLink> =
        order.into_iter().filter_map(|k| groups.remove(&k)).map(|g| g.into_link(started)).collect();
    detected.sort_by_key(|l| l.first_ms.unwrap_or(i64::MAX));
    let (hidden, shown): (Vec<MeetingLink>, Vec<MeetingLink>) =
        detected.into_iter().partition(|l| hidden_hashes.contains(&hidden_hash(meeting_id, &l.key)));
    links.extend(shown);
    Ok(MeetingLinks { meeting_id: meeting_id.to_string(), started_at: started.to_rfc3339(), links, hidden })
}

/// After an edit (Delete / Strike / time range), drop hidden-link hashes
/// whose link no longer appears anywhere in the meeting, so not even a hash
/// of a removed link is left. Cheap when nothing is hidden.
pub async fn prune_hidden(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<usize, String> {
    let mut conn = pool.acquire().await.map_err(|e| format!("Database busy: {}", e))?;
    let hashes: Vec<String> = sqlx::query_scalar("SELECT url_hash FROM meeting_link_hidden WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("Couldn't read hidden links: {}", e))?;
    if hashes.is_empty() {
        return Ok(0);
    }
    let (order, _) =
        detect_meeting(&mut conn, meeting_id).await.map_err(|e| format!("Couldn't read the meeting: {}", e))?;
    let live: HashSet<String> = order.iter().map(|k| hidden_hash(meeting_id, k)).collect();
    let mut n = 0;
    for h in hashes.iter().filter(|h| !live.contains(*h)) {
        n += sqlx::query("DELETE FROM meeting_link_hidden WHERE meeting_id = ? AND url_hash = ?")
            .bind(meeting_id)
            .bind(h)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("Couldn't update hidden links: {}", e))?
            .rows_affected() as usize;
    }
    Ok(n)
}

/// Hide (or show again) a detected link. Stores only the salted hash.
pub async fn set_hidden(pool: &Pool<Sqlite>, meeting_id: &str, key: &str, hidden: bool) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > MAX_URL_CHARS {
        return Err("That isn't a link in this meeting".into());
    }
    let h = hidden_hash(meeting_id, key);
    let q = if hidden {
        "INSERT OR IGNORE INTO meeting_link_hidden (meeting_id, url_hash) VALUES (?, ?)"
    } else {
        "DELETE FROM meeting_link_hidden WHERE meeting_id = ? AND url_hash = ?"
    };
    sqlx::query(q)
        .bind(meeting_id)
        .bind(&h)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Couldn't update the link: {}", e))
}

// ═══════════════════════════════════════════════════════════════════════════
// Added references
// ═══════════════════════════════════════════════════════════════════════════

/// A typed address as stored: `https://` added when there's no scheme.
/// Only http(s) web addresses are accepted.
pub fn reference_url(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("Enter a web address".into());
    }
    if s.len() > MAX_URL_CHARS {
        return Err("That address is too long".into());
    }
    if detect::normalize(s).is_none() {
        return Err("Enter a web address that starts with http:// or https:// (like https://example.com)".into());
    }
    let lower = s.to_ascii_lowercase();
    let url = if lower.starts_with("http://") || lower.starts_with("https://") { s.to_string() } else { format!("https://{}", s) };
    if !detect::is_openable(&url) {
        return Err("Enter a web address that starts with http:// or https:// (like https://example.com)".into());
    }
    Ok(url)
}

fn clean_text(v: Option<&str>, max: usize, what: &str) -> Result<Option<String>, String> {
    let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if v.chars().count() > max {
        return Err(format!("The {} can be at most {} characters", what, max));
    }
    Ok(Some(v.to_string()))
}

pub async fn add_reference(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    url: &str,
    title: Option<&str>,
    note: Option<&str>,
) -> Result<MeetingReference, String> {
    let url = reference_url(url)?;
    let title = clean_text(title, MAX_TITLE_CHARS, "title")?;
    let note = clean_text(note, MAX_NOTE_CHARS, "note")?;
    let mut conn = pool.acquire().await.map_err(|e| format!("Database busy: {}", e))?;
    meeting_started(&mut conn, meeting_id).await?;
    let r = MeetingReference {
        id: uuid::Uuid::new_v4().simple().to_string(),
        meeting_id: meeting_id.to_string(),
        url,
        title,
        note,
        created_at: Utc::now().to_rfc3339(),
    };
    sqlx::query("INSERT INTO meeting_references (id, meeting_id, url, title, note, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&r.id)
        .bind(&r.meeting_id)
        .bind(&r.url)
        .bind(&r.title)
        .bind(&r.note)
        .bind(&r.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("Couldn't add the reference: {}", e))?;
    Ok(r)
}

pub async fn update_reference(
    pool: &Pool<Sqlite>,
    id: &str,
    url: &str,
    title: Option<&str>,
    note: Option<&str>,
) -> Result<MeetingReference, String> {
    let url = reference_url(url)?;
    let title = clean_text(title, MAX_TITLE_CHARS, "title")?;
    let note = clean_text(note, MAX_NOTE_CHARS, "note")?;
    let n = sqlx::query("UPDATE meeting_references SET url = ?, title = ?, note = ? WHERE id = ?")
        .bind(&url)
        .bind(&title)
        .bind(&note)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Couldn't save the reference: {}", e))?
        .rows_affected();
    if n == 0 {
        return Err("This reference was already removed".into());
    }
    let row = sqlx::query("SELECT id, meeting_id, url, title, note, created_at FROM meeting_references WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Couldn't read the reference: {}", e))?;
    Ok(reference_from_row(&row))
}

pub async fn delete_reference(pool: &Pool<Sqlite>, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM meeting_references WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Couldn't remove the reference: {}", e))
}

// ═══════════════════════════════════════════════════════════════════════════
// Browser address capture setting (DMG; the `mas` build has no Accessibility)
// ═══════════════════════════════════════════════════════════════════════════

/// Settings key. Absent = on (it only runs with screen capture on and
/// Accessibility already granted).
pub const BROWSER_URL_SETTING: &str = "browser_url_capture";

pub async fn browser_capture_enabled(settings: &crate::settings::SettingsManager) -> bool {
    if cfg!(feature = "mas") {
        return false;
    }
    !matches!(settings.get(BROWSER_URL_SETTING).await, Ok(Some(v)) if v == "false")
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri commands
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use tauri::{AppHandle, State};

    #[tauri::command(rename_all = "camelCase")]
    pub async fn list_meeting_links(state: State<'_, AppState>, meeting_id: String) -> Result<MeetingLinks, String> {
        list_links(state.database.pool(), &meeting_id).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn add_meeting_reference(
        state: State<'_, AppState>,
        meeting_id: String,
        url: String,
        title: Option<String>,
        note: Option<String>,
    ) -> Result<MeetingReference, String> {
        add_reference(state.database.pool(), &meeting_id, &url, title.as_deref(), note.as_deref()).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn update_meeting_reference(
        state: State<'_, AppState>,
        id: String,
        url: String,
        title: Option<String>,
        note: Option<String>,
    ) -> Result<MeetingReference, String> {
        update_reference(state.database.pool(), &id, &url, title.as_deref(), note.as_deref()).await
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn delete_meeting_reference(state: State<'_, AppState>, id: String) -> Result<(), String> {
        delete_reference(state.database.pool(), &id).await
    }

    /// Hide a detected link (`hidden` false shows it again).
    #[tauri::command(rename_all = "camelCase")]
    pub async fn hide_meeting_link(
        state: State<'_, AppState>,
        meeting_id: String,
        key: String,
        hidden: Option<bool>,
    ) -> Result<(), String> {
        set_hidden(state.database.pool(), &meeting_id, &key, hidden.unwrap_or(true)).await
    }

    /// Open in the default browser. http and https only, checked here (the UI
    /// checks too): never `javascript:`, `file:` or any other scheme.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn open_meeting_link(app: AppHandle, url: String) -> Result<(), String> {
        if !detect::is_openable(&url) {
            return Err(OPEN_REFUSED.into());
        }
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(url, None::<&str>).map_err(|e| format!("Couldn't open the link: {}", e))
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn get_browser_url_capture(state: State<'_, AppState>) -> Result<bool, String> {
        Ok(browser_capture_enabled(&state.settings).await)
    }

    #[tauri::command(rename_all = "camelCase")]
    pub async fn set_browser_url_capture(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
        if cfg!(feature = "mas") {
            return Err("Browser addresses aren't recorded in this version of the app".into());
        }
        state
            .settings
            .set(BROWSER_URL_SETTING, if enabled { "true" } else { "false" })
            .await
            .map_err(|e| format!("Couldn't save the setting: {}", e))
    }
}
