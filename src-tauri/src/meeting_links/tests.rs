//! Tests for Links & References (docs/LINKS.md): detection (shared cases
//! with iOS), normalization, the scheme allowlist, the list, hidden hashes,
//! and that every purge removes browser-address rows and references.

use super::*;
use crate::database::DatabaseManager;
use crate::redaction::{self, RedactionEnv, WordTarget};
use std::path::PathBuf;
use std::sync::Arc;

// ─── Shared detection cases (same file as the iOS tests) ─────────────────

#[derive(serde::Deserialize)]
struct Cases {
    detect: Vec<DetectCase>,
    normalize: Vec<NormalizeCase>,
}

#[derive(serde::Deserialize)]
struct DetectCase {
    text: String,
    spoken: bool,
    keys: Vec<String>,
}

#[derive(serde::Deserialize)]
struct NormalizeCase {
    input: String,
    key: Option<String>,
    url: Option<String>,
}

fn cases() -> Cases {
    serde_json::from_str(include_str!("detection_cases.json")).expect("detection_cases.json")
}

#[test]
fn shared_detection_cases() {
    let mut failures = Vec::new();
    for c in cases().detect {
        let got: Vec<String> = detect::detect(&c.text, c.spoken).into_iter().map(|f| f.link.key).collect();
        if got != c.keys {
            failures.push(format!("{:?} (spoken {}): expected {:?}, got {:?}", c.text, c.spoken, c.keys, got));
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn shared_normalize_cases() {
    let mut failures = Vec::new();
    for c in cases().normalize {
        let got = detect::normalize(&c.input);
        let (key, url) = (got.as_ref().map(|n| n.key.clone()), got.as_ref().map(|n| n.url.clone()));
        if key != c.key || url != c.url {
            failures.push(format!("{:?}: expected {:?} {:?}, got {:?} {:?}", c.input, c.key, c.url, key, url));
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn detection_keeps_every_mention_in_order() {
    let found = detect::detect("example.com then test dot org then https://example.com/", true);
    let keys: Vec<&str> = found.iter().map(|f| f.link.key.as_str()).collect();
    assert_eq!(keys, ["example.com", "test.org", "example.com"]);
    assert!(found.windows(2).all(|w| w[0].pos < w[1].pos));
}

#[test]
fn normalization_merges_scheme_www_trailing_slash_and_tracking() {
    let forms = [
        "https://www.example.com/a/",
        "http://example.com/a",
        "example.com/a?utm_source=news&fbclid=1",
        "WWW.EXAMPLE.COM/a?gclid=z",
        "https://example.com/a#section",
    ];
    for f in forms {
        assert_eq!(detect::normalize(f).unwrap().key, "example.com/a", "{}", f);
    }
    // Paths are case-sensitive; other query parameters are kept in order
    assert_ne!(detect::normalize("example.com/A").unwrap().key, "example.com/a");
    assert_eq!(detect::normalize("example.com/s?q=2&a=1").unwrap().key, "example.com/s?q=2&a=1");
}

#[test]
fn scheme_allowlist_for_open() {
    for ok in ["https://example.com", "http://example.org/a?b=1", "HTTPS://EXAMPLE.COM/X", "https://localhost:3000/"] {
        assert!(detect::is_openable(ok), "{}", ok);
    }
    for bad in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "file:///etc/passwd",
        "data:text/html,<script>alert(1)</script>",
        "vbscript:msgbox(1)",
        "mailto:jane@example.com",
        "ftp://example.com/file",
        "about:blank",
        "example.com",
        " https://example.com",
        "https://example.com\n",
        "https://user:pw@example.com/",
        "https://",
        "",
    ] {
        assert!(!detect::is_openable(bad), "{:?} must not open", bad);
    }
}

#[test]
fn reference_urls_are_web_addresses_only() {
    assert_eq!(reference_url("example.com/syllabus").unwrap(), "https://example.com/syllabus");
    assert_eq!(reference_url("  http://example.edu/a  ").unwrap(), "http://example.edu/a");
    // As typed: the user's own address keeps its parameters
    assert_eq!(reference_url("https://example.com/r?utm_source=x").unwrap(), "https://example.com/r?utm_source=x");
    for bad in ["javascript:alert(1)", "file:///Users/me/a.pdf", "mailto:a@b.co", "not a link", "", "ftp://x.org"] {
        assert!(reference_url(bad).is_err(), "{:?}", bad);
    }
}

// ─── Database fixture ────────────────────────────────────────────────────

struct Fixture {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
    t0: DateTime<Utc>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn setup() -> Fixture {
    let dir = std::env::temp_dir().join(format!("nf-links-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db = DatabaseManager::new(&data.join("nofriction_meetings.db")).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Biology 101").await.unwrap();
    db.create_meeting("m2", "Other").await.unwrap();
    let t0 = parse_ts(
        &sqlx::query_scalar::<_, String>("SELECT started_at FROM meetings WHERE id = 'm1'")
            .fetch_one(db.pool())
            .await
            .unwrap(),
    )
    .unwrap();
    let env = RedactionEnv {
        app_data_dir: data,
        cache_dir: dir.join("cache"),
        video_enabled: false,
        recording_meetings: Vec::new(),
    };
    Fixture { db: Arc::new(db), dir, env, t0 }
}

fn at(f: &Fixture, ms: i64) -> DateTime<Utc> {
    f.t0 + chrono::Duration::milliseconds(ms)
}

async fn line(f: &Fixture, text: &str, ms: i64) -> i64 {
    f.db.add_transcript_full("m1", text, Some("Prof"), true, 0.9, at(f, ms), None).await.unwrap()
}

/// A browser-address row exactly as the capture writes it.
async fn browser_row(f: &Fixture, id: &str, url: &str, title: &str, ms: i64) {
    f.db.add_text_snapshot_full(id, None, None, Some("m1"), at(f, ms), url, "h", 1.0, BROWSER_URL_SOURCE, Some("Safari"), Some(title))
        .await
        .unwrap();
}

/// A screen (with its keyframe file in the fixture folder) shown from `ms` to `end_ms`.
async fn screen(f: &Fixture, name: &str, ms: i64, end_ms: i64) -> String {
    let frames = f.env.app_data_dir.join("frames").join("m1");
    std::fs::create_dir_all(&frames).unwrap();
    let id = format!("{}-{}", name, uuid::Uuid::new_v4());
    let path = frames.join(format!("state_{}.jpg", id));
    std::fs::write(&path, b"jpeg").unwrap();
    f.db.add_screen_state(&id, "m1", at(f, ms), Some(at(f, end_ms)), "", 0.0, Some(&path.to_string_lossy()), "other", "{}")
        .await
        .unwrap();
    id
}

async fn count(f: &Fixture, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(f.db.pool()).await.unwrap()
}

async fn browser_rows(f: &Fixture) -> i64 {
    count(f, "SELECT COUNT(*) FROM text_snapshots WHERE source = 'browser_url'").await
}

async fn keys(f: &Fixture) -> Vec<String> {
    list_links(f.db.pool(), "m1").await.unwrap().links.into_iter().map(|l| l.key).collect()
}

fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}

// ─── The list ────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_merges_said_screen_and_added_with_counts_and_first_time() {
    let f = setup().await;
    line(&f, "Welcome. The syllabus is at example dot edu slash bio101", 5_000).await;
    line(&f, "Again: example.edu/bio101, and read openstax.org", 65_000).await;
    browser_row(&f, "b1", "https://www.khanacademy.org/science/biology", "Biology | Khan Academy", 30_000).await;
    browser_row(&f, "b2", "https://example.edu/bio101", "BIO 101 Syllabus", 2_000).await;
    // OCR of a slide, and a window title naming a site
    f.db.add_text_snapshot_full("o1", None, None, Some("m1"), at(&f, 40_000), "Slides at openstax.org/books and openstax.org/books", "h", 0.5, "ocr", Some("Keynote"), Some("Lecture 1"))
        .await
        .unwrap();
    f.db.add_text_snapshot_full("o2", None, None, Some("m1"), at(&f, 50_000), "", "h", 0.5, "accessibility", Some("Chrome"), Some("quizlet.com/flashcards"))
        .await
        .unwrap();
    // Another meeting's links never show here
    f.db.add_transcript("m2", "see other-meeting.com", None, true, 0.9).await.unwrap();
    let r = add_reference(f.db.pool(), "m1", "https://www.khanacademy.org/science/biology?utm_source=x", Some("Khan unit"), Some("Ch. 1–3"))
        .await
        .unwrap();

    let list = list_links(f.db.pool(), "m1").await.unwrap();
    let k: Vec<&str> = list.links.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(
        k,
        ["khanacademy.org/science/biology", "example.edu/bio101", "openstax.org/books", "quizlet.com/flashcards", "openstax.org"],
        "added first, then by first time"
    );
    let added = &list.links[0];
    assert_eq!(added.sources, ["added", "screen"]);
    assert_eq!(added.reference_id.as_deref(), Some(r.id.as_str()));
    assert_eq!(added.title.as_deref(), Some("Khan unit"));
    assert_eq!(added.note.as_deref(), Some("Ch. 1–3"));
    assert_eq!(added.url, "https://www.khanacademy.org/science/biology?utm_source=x", "the address as the user gave it");
    assert_eq!(added.first_ms, Some(30_000));

    let syllabus = &list.links[1];
    assert_eq!(syllabus.sources, ["said", "screen"]);
    assert_eq!((syllabus.said_count, syllabus.screen_count), (2, 1));
    assert_eq!(syllabus.first_ms, Some(2_000));
    assert_eq!(syllabus.first_source, Some("screen"));
    assert_eq!(syllabus.title.as_deref(), Some("BIO 101 Syllabus"), "window title of the browser page");
    assert_eq!(syllabus.url, "https://example.edu/bio101");
    assert_eq!((syllabus.host.as_str(), syllabus.path.as_str()), ("example.edu", "/bio101"));

    let books = &list.links[2];
    assert_eq!((books.said_count, books.screen_count), (0, 1), "counted once per capture");
    assert_eq!(list.links[3].sources, ["screen"]);
    assert_eq!(list.links[4].sources, ["said"]);
    assert_eq!(list.links[4].first_ms, Some(65_000));
    assert!(list.hidden.is_empty());
}

#[tokio::test]
async fn http_only_mentions_open_with_http_and_mixed_open_https() {
    let f = setup().await;
    line(&f, "http://intranet.example.org/a and http://mixed.example.org", 1_000).await;
    line(&f, "mixed.example.org again", 2_000).await;
    let list = list_links(f.db.pool(), "m1").await.unwrap();
    assert_eq!(list.links[0].url, "http://intranet.example.org/a");
    assert_eq!(list.links[1].url, "https://mixed.example.org");
}

#[tokio::test]
async fn references_add_edit_delete() {
    let f = setup().await;
    let r = add_reference(f.db.pool(), "m1", "openstax.org/details/books/biology-2e", Some(" Textbook "), None).await.unwrap();
    assert_eq!(r.url, "https://openstax.org/details/books/biology-2e");
    assert_eq!(r.title.as_deref(), Some("Textbook"));
    assert!(add_reference(f.db.pool(), "m1", "javascript:alert(1)", None, None).await.is_err());
    assert!(add_reference(f.db.pool(), "nope", "https://example.com", None, None).await.is_err());
    assert!(add_reference(f.db.pool(), "m1", "https://example.com", Some(&"x".repeat(MAX_TITLE_CHARS + 1)), None).await.is_err());

    let u = update_reference(f.db.pool(), &r.id, "https://example.edu/reading.pdf", None, Some("pages 4-9")).await.unwrap();
    assert_eq!((u.url.as_str(), u.title.as_deref(), u.note.as_deref()), ("https://example.edu/reading.pdf", None, Some("pages 4-9")));
    assert!(update_reference(f.db.pool(), &r.id, "file:///etc/hosts", None, None).await.is_err());
    assert_eq!(keys(&f).await, ["example.edu/reading.pdf"]);

    delete_reference(f.db.pool(), &r.id).await.unwrap();
    assert!(keys(&f).await.is_empty());
    assert!(update_reference(f.db.pool(), &r.id, "https://example.com", None, None).await.is_err());
}

// ─── Hidden links: hashes only ───────────────────────────────────────────

#[tokio::test]
async fn hiding_stores_only_a_salted_hash() {
    let f = setup().await;
    line(&f, "go to example.com and quizlet.com", 1_000).await;
    set_hidden(f.db.pool(), "m1", "quizlet.com", true).await.unwrap();
    set_hidden(f.db.pool(), "m1", "quizlet.com", true).await.unwrap(); // idempotent

    let list = list_links(f.db.pool(), "m1").await.unwrap();
    assert_eq!(list.links.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["example.com"]);
    assert_eq!(list.hidden.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["quizlet.com"]);

    let rows: Vec<(String, String)> = sqlx::query_as("SELECT meeting_id, url_hash FROM meeting_link_hidden")
        .fetch_all(f.db.pool())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let h = &rows[0].1;
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!h.contains("quizlet"));
    assert_eq!(h, &hidden_hash("m1", "quizlet.com"));
    // Salted per meeting: the same site hashes differently elsewhere
    assert_ne!(hidden_hash("m1", "quizlet.com"), hidden_hash("m2", "quizlet.com"));
    // The table has no column that could hold the address
    let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('meeting_link_hidden')")
        .fetch_all(f.db.pool())
        .await
        .unwrap();
    assert_eq!(cols, ["meeting_id", "url_hash"]);

    set_hidden(f.db.pool(), "m1", "quizlet.com", false).await.unwrap();
    assert_eq!(keys(&f).await, ["example.com", "quizlet.com"]);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_link_hidden").await, 0);
}

#[tokio::test]
async fn striking_a_hidden_link_removes_its_hash_too() {
    let f = setup().await;
    let text = "the answer key is at secret-answers.com today";
    let id = line(&f, text, 1_000).await;
    line(&f, "and example.com", 2_000).await;
    set_hidden(f.db.pool(), "m1", "secret-answers.com", true).await.unwrap();
    set_hidden(f.db.pool(), "m1", "example.com", true).await.unwrap();
    let b = text.find("secret-answers.com").unwrap();
    let target = WordTarget {
        meeting_id: "m1".into(),
        transcript_id: id,
        start: utf16(&text[..b]),
        end: utf16(&text[..b]) + utf16("secret-answers.com"),
        expected_text: None,
        whole_line: false,
    };
    redaction::strike_words(f.db.pool(), &f.env, &target, None).await.unwrap();
    let list = list_links(f.db.pool(), "m1").await.unwrap();
    assert!(list.links.is_empty() && list.hidden.iter().map(|l| l.key.as_str()).eq(["example.com"]));
    let left: Vec<String> = sqlx::query_scalar("SELECT url_hash FROM meeting_link_hidden").fetch_all(f.db.pool()).await.unwrap();
    assert_eq!(left, [hidden_hash("m1", "example.com")], "only the link still in the meeting keeps its hash");
}

// ─── Derived links follow the text they come from ────────────────────────

#[tokio::test]
async fn deleting_or_striking_words_removes_said_links() {
    let f = setup().await;
    let text = "for practice go to quizlet dot com slash bio today";
    let id = line(&f, text, 1_000).await;
    assert_eq!(keys(&f).await, ["quizlet.com/bio"]);
    let b = text.find("quizlet").unwrap();
    let e = text.find(" today").unwrap();
    let target = WordTarget {
        meeting_id: "m1".into(),
        transcript_id: id,
        start: utf16(&text[..b]),
        end: utf16(&text[..e]),
        expected_text: None,
        whole_line: false,
    };
    let p = redaction::request_delete_words(f.db.pool(), &f.env, &target).await.unwrap();
    redaction::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert!(keys(&f).await.is_empty());

    // Strike only "dot": the rest can't be joined across the marker
    let text2 = "the site is example dot org for notes";
    let id2 = line(&f, text2, 3_000).await;
    let b = text2.find("dot").unwrap();
    let target = WordTarget {
        meeting_id: "m1".into(),
        transcript_id: id2,
        start: utf16(&text2[..b]),
        end: utf16(&text2[..b]) + 3,
        expected_text: None,
        whole_line: false,
    };
    redaction::strike_words(f.db.pool(), &f.env, &target, None).await.unwrap();
    assert!(keys(&f).await.is_empty(), "no link built across a strike marker");
}

// ─── Browser-address rows: every purge covers them ───────────────────────

#[tokio::test]
async fn browser_rows_go_with_meeting_delete_and_references_too() {
    let f = setup().await;
    browser_row(&f, "b1", "https://example.edu/syllabus", "Syllabus", 1_000).await;
    add_reference(f.db.pool(), "m1", "https://example.edu/reading", Some("Reading"), Some("note")).await.unwrap();
    set_hidden(f.db.pool(), "m1", "example.edu/syllabus", true).await.unwrap();
    // Another meeting keeps its own
    add_reference(f.db.pool(), "m2", "https://example.org", None, None).await.unwrap();
    f.db.add_text_snapshot_full("b2", None, None, Some("m2"), Utc::now(), "https://example.org/x", "h", 1.0, BROWSER_URL_SOURCE, None, None)
        .await
        .unwrap();

    f.db.delete_meeting("m1").await.unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM text_snapshots WHERE meeting_id = 'm1'").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_references WHERE meeting_id = 'm1'").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_link_hidden WHERE meeting_id = 'm1'").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_references").await, 1);
    assert_eq!(browser_rows(&f).await, 1);
    assert!(list_links(f.db.pool(), "m1").await.is_err());
}

#[tokio::test]
async fn references_are_purged_even_without_the_cascade() {
    // purge_meeting deletes them itself (not only ON DELETE CASCADE)
    let f = setup().await;
    add_reference(f.db.pool(), "m1", "https://example.edu/a", None, None).await.unwrap();
    set_hidden(f.db.pool(), "m1", "x.org", true).await.unwrap();
    purge_meeting(f.db.pool(), "m1").await.unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_references").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meeting_link_hidden").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM meetings WHERE id = 'm1'").await, 1, "the meeting itself is the caller's");
}

#[tokio::test]
async fn browser_rows_go_with_a_struck_screen() {
    let f = setup().await;
    let s = screen(&f, "slide", 10_000, 20_000).await;
    browser_row(&f, "inside", "https://example.edu/quiz", "Quiz", 15_000).await;
    browser_row(&f, "outside", "https://example.edu/later", "Later", 40_000).await;
    redaction::strike_screens(f.db.pool(), &f.env, "m1", &[s], None).await.unwrap();
    let ids: Vec<String> = sqlx::query_scalar("SELECT snapshot_id FROM text_snapshots").fetch_all(f.db.pool()).await.unwrap();
    assert_eq!(ids, ["outside"]);
    assert_eq!(keys(&f).await, ["example.edu/later"]);
}

#[tokio::test]
async fn browser_rows_go_with_a_deleted_screen() {
    let f = setup().await;
    let s = screen(&f, "slide", 10_000, 20_000).await;
    browser_row(&f, "inside", "https://example.edu/quiz", "Quiz", 12_000).await;
    let p = redaction::request_delete_screens(f.db.pool(), &f.env, "m1", &[s]).await.unwrap();
    assert_eq!(browser_rows(&f).await, 1, "kept during the undo window");
    redaction::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert_eq!(browser_rows(&f).await, 0);
    assert!(keys(&f).await.is_empty());
}

#[tokio::test]
async fn browser_rows_go_with_a_time_range_delete_and_strike() {
    let f = setup().await;
    line(&f, "first part", 5_000).await;
    browser_row(&f, "r1", "https://example.edu/one", "One", 30_000).await;
    browser_row(&f, "r2", "https://example.edu/two", "Two", 90_000).await;
    browser_row(&f, "r3", "https://example.edu/three", "Three", 150_000).await;
    line(&f, "inside the first range", 31_000).await;
    line(&f, "inside the second range", 91_000).await;

    let p = redaction::time_range::preview_time_range(f.db.pool(), &f.env, "m1", 20_000, 40_000).await.unwrap();
    assert_eq!(p.screen_text_snapshots, 1, "the preview counts the browser address");
    let pending = redaction::time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", 20_000, 40_000, None)
        .await
        .unwrap();
    redaction::commit_pending(f.db.pool(), &f.env, &pending.id).await.unwrap();
    assert_eq!(keys(&f).await, ["example.edu/two", "example.edu/three"]);

    redaction::time_range::strike_time_range(f.db.pool(), &f.env, "m1", 80_000, 100_000, None, None).await.unwrap();
    let ids: Vec<String> = sqlx::query_scalar("SELECT snapshot_id FROM text_snapshots").fetch_all(f.db.pool()).await.unwrap();
    assert_eq!(ids, ["r3"]);
    assert_eq!(keys(&f).await, ["example.edu/three"]);
}

#[cfg(not(feature = "mas"))]
#[tokio::test]
async fn captured_browser_rows_are_plain_screen_text_rows() {
    let f = setup().await;
    let mut t = browser_url::Tracker::default();
    let page = browser_url::BrowserPage {
        app_name: "Arc".into(),
        url: "https://www.example.edu/syllabus?utm_campaign=x&access_token=secret".into(),
        title: Some("Syllabus".into()),
    };
    let cap = t.next(&page).unwrap();
    browser_url::store(&f.db, "m1", &cap, at(&f, 7_000)).await.unwrap();
    let (text, source, app, title): (String, String, String, String) = sqlx::query_as(
        "SELECT text, source, app_name, window_title FROM text_snapshots WHERE meeting_id = 'm1'",
    )
    .fetch_one(f.db.pool())
    .await
    .unwrap();
    assert_eq!(text, "https://www.example.edu/syllabus", "tracking and credential parameters never stored");
    assert_eq!((source.as_str(), app.as_str(), title.as_str()), (BROWSER_URL_SOURCE, "Arc", "Syllabus"));
    let list = list_links(f.db.pool(), "m1").await.unwrap();
    assert_eq!(list.links[0].key, "example.edu/syllabus");
    assert_eq!(list.links[0].first_ms, Some(7_000));
    assert_eq!(list.links[0].sources, ["screen"]);
}
