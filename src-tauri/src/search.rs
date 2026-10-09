//! The one search, at the top of Recordings: titles, notebooks, people,
//! topics, and anything said. Everything is looked up on this Mac; the
//! query and the matching lines are never logged.

use crate::database::search_terms;
use serde::Serialize;
use sqlx::{Pool, Row, Sqlite};
use tauri::State;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub meeting_id: String,
    /// "title" | "notebook" | "person" | "topic" | "said"
    pub kind: String,
    /// What matched: the person's name, the topic, or the transcript line (plain text)
    pub label: String,
    /// A line: ms from the recording's start
    pub ms: Option<i64>,
}

/// `LIKE` pattern for "contains", with the user's `%`, `_` and `\` escaped.
pub fn like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');
    for c in query.trim().chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// A safe FTS5 query from free text: every term must match, the last one
/// as a prefix (so typing finds lines as you go). None without usable terms.
pub fn fts_and_query(text: &str) -> Option<String> {
    let terms = search_terms(text);
    if terms.is_empty() {
        return None;
    }
    let n = terms.len();
    Some(
        terms
            .iter()
            .enumerate()
            .map(|(i, w)| if i + 1 == n { format!("\"{}\"*", w) } else { format!("\"{}\"", w) })
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

pub async fn search(pool: &Pool<Sqlite>, query: &str) -> Result<Vec<SearchHit>, sqlx::Error> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let like = like_pattern(q);
    let mut hits: Vec<SearchHit> = Vec::new();

    // Titles and notebooks
    for r in sqlx::query(
        "SELECT id, title, class_name FROM meetings \
         WHERE title LIKE ? ESCAPE '\\' OR class_name LIKE ? ESCAPE '\\' \
         ORDER BY started_at DESC LIMIT 100",
    )
    .bind(&like)
    .bind(&like)
    .fetch_all(pool)
    .await?
    {
        let id: String = r.get("id");
        let title: String = r.get("title");
        let notebook: Option<String> = r.try_get("class_name").ok().flatten();
        if contains_ci(&title, q) {
            hits.push(SearchHit { meeting_id: id.clone(), kind: "title".into(), label: title, ms: None });
        }
        if let Some(nb) = notebook.filter(|n| contains_ci(n, q)) {
            hits.push(SearchHit { meeting_id: id, kind: "notebook".into(), label: nb, ms: None });
        }
    }

    // People on the invite (name, email or company)
    for r in sqlx::query(
        "SELECT a.meeting_id, COALESCE(NULLIF(p.name, ''), NULLIF(a.name, ''), a.email) AS who \
         FROM meeting_attendees a LEFT JOIN people p ON p.id = lower(a.email) \
         WHERE a.name LIKE ? ESCAPE '\\' OR a.email LIKE ? ESCAPE '\\' \
            OR p.name LIKE ? ESCAPE '\\' OR p.company LIKE ? ESCAPE '\\' OR a.company LIKE ? ESCAPE '\\' \
         LIMIT 200",
    )
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .fetch_all(pool)
    .await?
    {
        hits.push(SearchHit { meeting_id: r.get("meeting_id"), kind: "person".into(), label: r.get("who"), ms: None });
    }

    // Topics
    for r in sqlx::query("SELECT meeting_id, topic FROM meeting_topics WHERE topic LIKE ? ESCAPE '\\' LIMIT 200")
        .bind(&like)
        .fetch_all(pool)
        .await?
    {
        hits.push(SearchHit { meeting_id: r.get("meeting_id"), kind: "topic".into(), label: r.get("topic"), ms: None });
    }

    // Anything said (full-text search over the transcripts)
    if let Some(fts) = fts_and_query(q) {
        for r in sqlx::query(
            "SELECT t.meeting_id, t.text, t.timestamp, m.started_at \
             FROM transcripts_fts JOIN transcripts t ON transcripts_fts.rowid = t.id \
             JOIN meetings m ON m.id = t.meeting_id \
             WHERE transcripts_fts MATCH ? ORDER BY bm25(transcripts_fts) LIMIT 80",
        )
        .bind(&fts)
        .fetch_all(pool)
        .await?
        {
            let text: String = r.get("text");
            let plain = crate::redaction::render_plain(&text);
            if plain.trim().is_empty() {
                continue;
            }
            let at: String = r.get("timestamp");
            let started: String = r.get("started_at");
            let ms = match (
                chrono::DateTime::parse_from_rfc3339(&at),
                chrono::DateTime::parse_from_rfc3339(&started),
            ) {
                (Ok(a), Ok(s)) => Some((a - s).num_milliseconds().max(0)),
                _ => None,
            };
            hits.push(SearchHit { meeting_id: r.get("meeting_id"), kind: "said".into(), label: plain, ms });
        }
    }

    Ok(hits)
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_lowercase().contains(&needle.to_lowercase())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_recordings(query: String, state: State<'_, crate::AppState>) -> Result<Vec<SearchHit>, String> {
    search(state.database.pool(), &query)
        .await
        .map_err(|e| format!("Search failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("50% done_"), "%50\\% done\\_%");
        assert_eq!(like_pattern("  acme  "), "%acme%");
    }

    #[test]
    fn fts_query_is_quoted_and_anded() {
        assert_eq!(fts_and_query("launch date"), Some("\"launch\" AND \"date\"*".into()));
        assert_eq!(fts_and_query("midterm"), Some("\"midterm\"*".into()));
        // Only stopwords / short words: nothing to match on
        assert_eq!(fts_and_query("to a"), None);
        // Punctuation and FTS keywords never reach MATCH as syntax
        assert_eq!(fts_and_query("\"NEAR(x)\" roadmap"), Some("\"near\" AND \"roadmap\"*".into()));
    }
}
