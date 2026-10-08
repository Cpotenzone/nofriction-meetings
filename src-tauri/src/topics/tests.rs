//! Topics against the purge (docs/TOPICS_AND_CHAT.md, docs/REDACTION.md).
//! The AI is always a mock here; no endpoint is called.

use super::*;
use crate::ai::{AiError, Msg, Opts};
use crate::database::DatabaseManager;
use crate::redaction::{self as rd, RedactionEnv, WordTarget};
use crate::study::prompt::{StudyLine, StudyInput};
use parking_lot::Mutex;
use sqlx::{ConnectOptions, Connection};
use std::path::PathBuf;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// Validation and normalization (pure)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn validates_json_shapes_and_caps_at_four() {
    let raw = "<think>hmm</think>Sure:\n```json\n{\"topics\": [{\"label\": \"q4 roadmap\", \"confidence\": 0.9}, {\"label\": \"Hiring plan\", \"confidence\": 0.7}]}\n```";
    let got = validate(raw).unwrap();
    assert_eq!(got.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), vec!["Q4 roadmap", "Hiring plan"]);
    assert_eq!(got[0].key, "q4 roadmap");
    // Bare arrays, arrays of strings, missing confidences
    let got = validate(r#"["Cell membranes", {"topic": "Osmosis"}, {"name": "Diffusion", "score": 2}]"#).unwrap();
    assert_eq!(got.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), vec!["Diffusion", "Cell membranes", "Osmosis"]);
    assert_eq!(got[0].confidence, 1.0, "clamped");
    assert_eq!(got[1].confidence, 0.5, "default");
    // More than four: the highest confidences win, ties keep the model's order
    let many = r#"{"topics": [{"label": "a1 b", "confidence": 0.3}, {"label": "b2 c", "confidence": 0.9}, {"label": "c3 d", "confidence": 0.5}, {"label": "d4 e", "confidence": 0.5}, {"label": "e5 f", "confidence": 0.5}, {"label": "f6 g", "confidence": 0.1}]}"#;
    let got = validate(many).unwrap();
    assert_eq!(got.len(), MAX_TOPICS);
    assert_eq!(got.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), vec!["B2 c", "C3 d", "D4 e", "E5 f"]);
    // Generic labels and near-duplicates are dropped
    let got = validate(r#"["Meeting", "discussion", "Cell membranes", "cell membrane", "CELL MEMBRANES.", "x"]"#).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].label, "Cell membranes");
}

#[test]
fn malformed_output_is_an_error_never_a_panic() {
    for c in ["", "I can't help", "{", "{\"topics\": \"no\"}", "null", "[1, 2]", "{\"topics\": []}", "[\"\"]", "[\"m\"]"] {
        assert!(validate(c).is_err(), "accepted {:?}", c);
    }
    assert!(validate(&"[".repeat(5000)).is_err());
    assert!(validate(&format!("{}{}", "{\"a\":".repeat(3000), "1")).is_err());
}

#[test]
fn keys_fold_case_punctuation_and_plurals() {
    assert_eq!(topic_key("Cell Membranes"), "cell membrane");
    assert_eq!(topic_key("cell-membrane!"), "cell membrane");
    assert_eq!(topic_key("Q4 Roadmap"), "q4 roadmap");
    assert_eq!(topic_key("Insurance renewals"), "insurance renewal");
    assert_eq!(topic_key("Class notes"), "class note", "'class' keeps its s");
    assert_eq!(topic_key("Bus routes"), "bus route");
    assert_eq!(topic_key("Mitosis"), "mitosis");
    assert_eq!(topic_key("Casey's plan"), "casey plan", "possessives fold too");
    assert_eq!(topic_key("  --  "), "");
    assert!(near("kubernetes", "kubernates"), "one typo");
    assert!(near("q4 roadmap", "q4 roadmap"));
    assert!(!near("q4 roadmap", "q3 roadmap"), "a different quarter is a different topic");
    assert!(!near("atp", "adp"), "short keys must match exactly");
    assert!(!near("cell membrane", "cell membrane osmosis"));
    assert!(near("off site", "offsite"), "equal without spaces");
}

#[test]
fn display_labels_are_cleaned_and_cased() {
    assert_eq!(display_label("q4 roadmap").as_deref(), Some("Q4 roadmap"));
    assert_eq!(display_label("BIO 101 midterm").as_deref(), Some("BIO 101 midterm"));
    assert_eq!(display_label("- \"Insurance renewal.\"").as_deref(), Some("Insurance renewal"));
    assert_eq!(display_label("  cell\n\tmembranes \u{7}").as_deref(), Some("Cell membranes"));
    assert_eq!(display_label("meeting"), None);
    assert_eq!(display_label("Various topics"), None);
    assert_eq!(display_label(""), None);
    assert_eq!(display_label("a"), None);
    assert_eq!(display_label(&"word ".repeat(7)), None, "more than six words");
    let long = display_label("Supercalifragilisticexpialidocious membranes and more").unwrap();
    assert!(long.chars().count() <= MAX_LABEL_CHARS);
    assert!(long.ends_with('…'));
    // Kept as text: the UI renders it as text
    assert_eq!(display_label("<b>Osmosis</b>").as_deref(), Some("<b>Osmosis</b>"));
}

#[test]
fn parts_merge_by_key_and_the_recurring_topic_ranks_first() {
    let f = |l: &str, c: f64| Found { label: l.into(), key: topic_key(l), confidence: c };
    let merged = merge_parts(&[
        vec![f("Osmosis", 0.9), f("Cell membranes", 0.6)],
        vec![f("cell membrane", 0.9), f("Diffusion", 0.8)],
        vec![f("Cell Membranes", 0.7), f("Krebs cycle", 0.9), f("Glycolysis", 0.5), f("Enzymes", 0.4)],
    ]);
    assert_eq!(merged[0].label, "Cell membranes", "{:?}", merged);
    assert!(merged.len() <= MAX_TOPICS);
    assert!(merged.iter().all(|m| m.confidence <= 1.0));
    assert_eq!(merged.iter().filter(|m| m.key == "cell membrane").count(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// Generation with a mock AI
// ═══════════════════════════════════════════════════════════════════════════

struct Mock {
    ctx: usize,
    calls: Mutex<Vec<Vec<Msg>>>,
    /// Bad answers to give before the good one
    bad_first: Mutex<usize>,
    error: Option<AiError>,
}

impl Mock {
    fn new(ctx: usize) -> Self {
        Self { ctx, calls: Mutex::new(Vec::new()), bad_first: Mutex::new(0), error: None }
    }
    fn texts(&self) -> Vec<String> {
        self.calls.lock().iter().map(|m| m.iter().map(|x| x.text()).collect::<Vec<_>>().join("\n")).collect()
    }
}

#[async_trait::async_trait]
impl Completer for Mock {
    async fn complete(&self, msgs: Vec<Msg>, _opts: Opts) -> Result<String, AiError> {
        self.calls.lock().push(msgs.clone());
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let mut bad = self.bad_first.lock();
        if *bad > 0 {
            *bad -= 1;
            return Ok("Here are the topics: cells, ATP".into());
        }
        let user = msgs[1].text();
        if user.contains("Part ") {
            Ok(r#"{"topics": [{"label": "Cell cycle", "confidence": 0.9}, {"label": "Mitosis phases", "confidence": 0.7}]}"#.into())
        } else {
            Ok(r#"{"topics": [{"label": "cell membranes", "confidence": 0.9}, {"label": "ATP synthesis", "confidence": 0.8}]}"#.into())
        }
    }
    fn context_tokens(&self) -> usize {
        self.ctx
    }
}

fn input_with(lines: &[(i64, &str)]) -> StudyInput {
    StudyInput {
        title: "Biology 101".into(),
        kind: crate::recording_kind::RecordingKind::Class,
        notebook: Some("BIO 101".into()),
        duration_ms: lines.iter().map(|l| l.0).max().unwrap_or(0) + 5_000,
        lines: lines.iter().map(|(ms, t)| StudyLine { ms: *ms, text: t.to_string() }).collect(),
        marks: Vec::new(),
    }
}

#[tokio::test]
async fn short_recording_is_one_request() {
    let m = Mock::new(32_768);
    let i = input_with(&[(10_000, "cells have organelles"), (20_000, "mitochondria make ATP")]);
    let got = generate(&m, &i).await.unwrap();
    assert_eq!(got.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), vec!["Cell membranes", "ATP synthesis"]);
    assert_eq!(m.calls.lock().len(), 1);
    let t = &m.texts()[0];
    assert!(t.starts_with(&system_prompt()), "{}", t);
    assert!(t.contains("Lecture: Biology 101\nClass: BIO 101\n") && t.contains("TRANSCRIPT:\n[0:10] cells have organelles"), "{}", t);
    assert!(!t.contains("Part 1 of"));
}

#[tokio::test]
async fn long_recording_is_read_in_parts_and_merged() {
    let m = Mock::new(4_096);
    let lines: Vec<(i64, String)> =
        (0..500).map(|i| (i as i64 * 6_000, format!("sentence {} about the cell cycle and mitosis phases", i))).collect();
    let refs: Vec<(i64, &str)> = lines.iter().map(|(a, b)| (*a, b.as_str())).collect();
    let got = generate(&m, &input_with(&refs)).await.unwrap();
    let calls = m.calls.lock().len();
    assert!((2..=8).contains(&calls), "{} parts", calls);
    assert!(m.texts().iter().all(|t| t.contains("Part ")));
    assert!(got.len() <= MAX_TOPICS);
    assert_eq!(got[0].label, "Cell cycle");
    for c in m.calls.lock().iter() {
        let chars: usize = c.iter().map(|x| x.text().chars().count()).sum();
        assert!(chars < ((4_096 - 300 - 512) as f64 * sprompt::CHARS_PER_TOKEN) as usize, "{} chars", chars);
    }
}

#[tokio::test]
async fn bad_answer_is_retried_once_and_ai_errors_stop() {
    let m = Mock::new(32_768);
    *m.bad_first.lock() = 1;
    let i = input_with(&[(10_000, "cells have organelles")]);
    assert!(generate(&m, &i).await.is_ok());
    assert_eq!(m.calls.lock().len(), 2);
    assert!(m.texts()[1].contains("couldn't be used") && !m.texts()[1].contains("Here are the topics"));
    let m = Mock::new(32_768);
    *m.bad_first.lock() = 2;
    let e = generate(&m, &i).await.unwrap_err();
    assert!(e.contains("asked twice"), "{}", e);
    let mut m = Mock::new(32_768);
    m.error = Some(AiError::ConsentRequired("custom".into()));
    assert_eq!(generate(&m, &i).await.unwrap_err(), "CONSENT_REQUIRED:custom");
    assert!(generate(&m, &input_with(&[])).await.unwrap_err().contains("no transcript"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Storage and the purge
// ═══════════════════════════════════════════════════════════════════════════

struct Fx {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
    db_path: PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn fx() -> Fx {
    let dir = std::env::temp_dir().join(format!("nf-topics-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db_path = data.join("nofriction_meetings.db");
    let db = DatabaseManager::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Biology 101").await.unwrap();
    db.create_meeting("m2", "Biology 102").await.unwrap();
    let env = RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() };
    Fx { db: Arc::new(db), dir, env, db_path }
}

async fn line_at(f: &Fx, meeting: &str, secs: i64, text: &str) -> i64 {
    let t0 = crate::study::load_input(f.db.pool(), meeting).await.map(|_| ()).unwrap();
    let _ = t0;
    let start: String = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = ?").bind(meeting).fetch_one(f.db.pool()).await.unwrap();
    let start = chrono::DateTime::parse_from_rfc3339(&start).unwrap().with_timezone(&chrono::Utc);
    f.db.add_transcript_at(meeting, text, None, true, 0.9, start + chrono::Duration::seconds(secs)).await.unwrap()
}

async fn rows(pool: &Pool<Sqlite>, meeting: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics WHERE meeting_id = ?").bind(meeting).fetch_one(pool).await.unwrap()
}

fn labels(t: &[MeetingTopic]) -> Vec<String> {
    t.iter().map(|x| x.label.clone()).collect()
}

fn utf16_span(hay: &str, needle: &str) -> (usize, usize) {
    let b = hay.find(needle).unwrap();
    let s = hay[..b].encode_utf16().count();
    (s, s + needle.encode_utf16().count())
}

#[tokio::test]
async fn ai_topics_merge_across_recordings_and_user_edits_are_kept() {
    let f = fx().await;
    line_at(&f, "m1", 10, "the cell membrane is selectively permeable").await;
    line_at(&f, "m2", 10, "cell membranes again, and osmosis").await;
    let got = find_and_save(f.db.pool(), &Mock::new(32_768), "m1").await.unwrap();
    assert_eq!(labels(&got), vec!["Cell membranes", "ATP synthesis"]);
    assert!(got.iter().all(|t| t.source == "ai" && t.confidence.is_some()));
    // A second recording's "cell membrane" adopts the first's key and label
    sqlx::query("UPDATE meeting_topics SET topic = 'Cell membranes (lecture)' WHERE meeting_id = 'm1' AND topic_key = 'cell membrane'")
        .execute(f.db.pool())
        .await
        .unwrap();
    let got2 = find_and_save(f.db.pool(), &Mock::new(32_768), "m2").await.unwrap();
    assert_eq!(got2[0].key, "cell membrane");
    assert_eq!(got2[0].label, "Cell membranes (lecture)", "the stored label wins over the model's");
    let idx = index(f.db.pool()).await.unwrap();
    assert_eq!(idx.topics.len(), 2);
    let cm = idx.topics.iter().find(|t| t.key == "cell membrane").unwrap();
    assert_eq!((cm.count, cm.label.as_str()), (2, "Cell membranes (lecture)"));
    assert_eq!(idx.by_meeting["m1"].len(), 2);
    assert_eq!(idx.by_meeting["m2"][0].label, "Cell membranes (lecture)");

    // The user renames, removes and adds on m1: all become user topics
    let user = set_for_meeting(f.db.pool(), "m1", &["osmosis".into(), "Cell membranes".into(), "meeting".into(), "".into()]).await.unwrap();
    assert_eq!(labels(&user), vec!["Osmosis", "Cell membranes (lecture)"], "generic and empty dropped, label merged");
    assert!(user.iter().all(|t| t.source == "user" && t.confidence.is_none()));
    // Finding again keeps them, and never brings back the AI topic the user removed
    let again = find_and_save(f.db.pool(), &Mock::new(32_768), "m1").await.unwrap();
    assert_eq!(labels(&again), vec!["Osmosis", "Cell membranes (lecture)"], "ATP synthesis stays removed: {:?}", again);
    assert_eq!(again.iter().filter(|t| t.source == "user").count(), 2);
    assert_eq!(rows(f.db.pool(), "m1").await, 2);
    // Adding it back by hand forgets the removal
    set_for_meeting(f.db.pool(), "m1", &["Osmosis".into(), "ATP synthesis".into()]).await.unwrap();
    let removed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics_removed WHERE meeting_id = 'm1'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(removed, 1, "only 'cell membrane' is remembered as removed now");
    // Reading back keeps the user's order; finding again brings nothing removed back
    assert_eq!(labels(&list_for_meeting(f.db.pool(), "m1").await.unwrap()), vec!["Osmosis", "ATP synthesis"]);
    assert_eq!(labels(&find_and_save(f.db.pool(), &Mock::new(32_768), "m1").await.unwrap()), vec!["Osmosis", "ATP synthesis"]);
    // An unknown recording is refused
    assert!(set_for_meeting(f.db.pool(), "nope", &["x y".into()]).await.is_err());
    // Clearing
    assert!(set_for_meeting(f.db.pool(), "m1", &[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn topics_made_before_an_edit_are_never_saved_after_it() {
    let f = fx().await;
    let text = "the Krebs cycle runs in the matrix";
    let id = line_at(&f, "m1", 10, text).await;
    let before = crate::study::load_input(f.db.pool(), "m1").await.unwrap().fingerprint();
    let (s, e) = utf16_span(text, "Krebs cycle");
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    let found = vec![Found { label: "Krebs cycle".into(), key: "krebs cycle".into(), confidence: 0.9 }];
    assert_eq!(save_ai_topics(f.db.pool(), "m1", &before, &found).await.unwrap_err(), crate::study::TRANSCRIPT_CHANGED);
    assert_eq!(rows(f.db.pool(), "m1").await, 0);
    // The prompt never carries the stricken words
    let m = Mock::new(32_768);
    find_and_save(f.db.pool(), &m, "m1").await.unwrap();
    for t in m.texts() {
        assert!(!t.contains("Krebs") && t.contains("[stricken from the record]"), "{}", t);
    }
}

#[tokio::test]
async fn transcript_edits_and_meeting_delete_purge_topics_and_backups_tolerate_old_schemas() {
    let f = fx().await;
    let text = "osmosis moves water across membranes";
    let id = line_at(&f, "m1", 10, text).await;
    line_at(&f, "m2", 10, "photosynthesis needs light").await;
    find_and_save(f.db.pool(), &Mock::new(32_768), "m1").await.unwrap();
    find_and_save(f.db.pool(), &Mock::new(32_768), "m2").await.unwrap();
    set_for_meeting(f.db.pool(), "m1", &["Osmosis".into(), "Water".into()]).await.unwrap();
    assert_eq!(rows(f.db.pool(), "m1").await, 2);
    // A backup copy made now
    rd::wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();
    // The preview says so; the strike deletes them (the user's too), live and in the backup
    let (s, e) = utf16_span(text, "water");
    let preview = rd::preview_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }).await.unwrap();
    assert!(preview.iter().any(|i| i.contains("2 topics on this recording")), "{:?}", preview);
    let out = rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    assert_eq!(rows(f.db.pool(), "m1").await, 0, "strike");
    assert_eq!(rows(f.db.pool(), "m2").await, 2, "the other recording keeps its topics");
    assert_eq!((out.backups_purged, out.backups_deleted), (1, 0), "{:?}", out.warnings);
    let mut c = sqlx::sqlite::SqliteConnectOptions::new().filename(&copy).connect().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics WHERE meeting_id = 'm1'").fetch_one(&mut c).await.unwrap();
    assert_eq!(n, 0);
    // An older backup without the table is left alone, not an error
    sqlx::query("DROP TABLE meeting_topics").execute(&mut c).await.unwrap();
    assert_eq!(purge_for_meeting(&mut c, "m1").await.unwrap(), 0);
    assert!(rd::redact_ai_outputs(&mut c, "m2", "photosynthesis", "[stricken from the record]").await.is_ok());
    c.close().await.unwrap();
    // Deleting the recording deletes its topics
    set_for_meeting(f.db.pool(), "m2", &[]).await.unwrap();
    f.db.delete_meeting("m2").await.unwrap();
    assert_eq!(rows(f.db.pool(), "m2").await, 0, "meeting delete");
    let removed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics_removed WHERE meeting_id = 'm2'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(removed, 0, "its removed-topic memory goes with it");
    assert!(index(f.db.pool()).await.unwrap().topics.is_empty());
}
