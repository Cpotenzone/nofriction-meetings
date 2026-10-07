//! Study tools and moment markers against the purge (docs/STUDY_TOOLS.md,
//! docs/REDACTION.md). The AI is always a mock here; no endpoint is called.

use super::export::*;
use super::parse::*;
use super::prompt::*;
use super::*;
use crate::database::DatabaseManager;
use crate::redaction::time_range::{self, MsRange};
use crate::redaction::{self as rd, RedactionEnv, WordTarget};
use parking_lot::Mutex;
use serde_json::json;
use sqlx::{ConnectOptions, Connection};
use std::path::PathBuf;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// Parsing model output (untrusted)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn extracts_json_from_fences_prose_and_think_blocks() {
    let raw = "<think>let me plan {\"cards\": []}</think>Sure! Here you go:\n```json\n{\"cards\": [{\"front\": \"Mitosis\", \"back\": \"Cell division\"}]}\n```\nHope it helps {not json}";
    let v = validate(StudyKind::Flashcards, raw, 0).unwrap();
    assert_eq!(v, json!({"cards": [{"front": "Mitosis", "back": "Cell division"}]}));
    // Trailing commas (common small-model slip), braces inside strings
    let v = validate(StudyKind::Terms, r#"{"terms": [{"term": "a {b}", "definition": "c ] d",},],}"#, 0).unwrap();
    assert_eq!(v["terms"][0]["term"], "a {b}");
    assert_eq!(v["terms"][0]["definition"], "c ] d");
    // A bare array is accepted for list parts
    let v = validate(StudyKind::Flashcards, r#"[{"question": "Q?", "answer": "A"}]"#, 0).unwrap();
    assert_eq!(v["cards"][0], json!({"front": "Q?", "back": "A"}));
}

#[test]
fn malformed_output_is_an_error_never_a_panic() {
    let cases = [
        "",
        "I can't help with that.",
        "{",
        "{\"cards\": [",
        "{\"cards\": [{\"front\": \"x\"",
        "[1, 2, 3]",
        "{\"cards\": \"not a list\"}",
        "{\"cards\": [{\"front\": 5}]}",
        "{\"cards\": [{\"front\": \"\", \"back\": \"\"}]}",
        "{\"cards\": [{\"front\": \"same\", \"back\": \"Same.\"}]}",
        "null",
        "\"just a string\"",
        "{\"summary\": \"plain text summary\"}",
        "{\"sections\": [{\"heading\": \"H\", \"bullets\": []}]}",
        "\u{0}\u{1}{{{{[[[[",
        "</think>{\"terms\": []}",
    ];
    for kind in StudyKind::ALL {
        for c in cases {
            assert!(validate(kind, c, 60_000).is_err(), "{:?} accepted {:?}", kind, c);
        }
    }
    // A quiz question needs at least two choices
    assert!(validate(StudyKind::Quiz, r#"{"questions": [{"question": "Q", "choices": ["a"], "answer": 0}]}"#, 0).is_err());
    // Deeply nested garbage is rejected, not overflowed
    let deep = "[".repeat(5000);
    assert!(validate(StudyKind::Quiz, &deep, 0).is_err());
    let deep_obj = format!("{}{}", "{\"a\":".repeat(3000), "1");
    assert!(validate(StudyKind::Quiz, &deep_obj, 0).is_err());
}

#[test]
fn quiz_answers_are_checked_and_times_bounded() {
    let raw = r#"{"questions": [
        {"question": "What do mitochondria make?", "choices": ["ATP", "DNA", "RNA", "Fat"], "answer": 0, "explanation": "Powerhouse.", "time": "12:34"},
        {"question": "Letter answer", "choices": ["a", "b", "c", "d"], "answer": "C)", "time": "[1:02:03]"},
        {"question": "Text answer", "options": ["red", "green"], "correct": "Green", "t": 95},
        {"question": "Out of range", "choices": ["a", "b"], "answer": 2},
        {"question": "Negative", "choices": ["a", "b"], "answer": -1},
        {"question": "Duplicate choices", "choices": ["same", "Same", "x"], "answer": 0},
        {"question": "Too many", "choices": ["1","2","3","4","5","6","7"], "answer": 0},
        {"question": "Empty choice", "choices": ["a", ""], "answer": 0},
        {"question": "Late time", "choices": ["a", "b"], "answer": 1, "time": "99:00"},
        {"question": "Bad time", "choices": ["a", "b"], "answer": 1, "time": "12:75"}
    ]}"#;
    let v = validate(StudyKind::Quiz, raw, 3_800_000).unwrap();
    let qs = v["questions"].as_array().unwrap();
    let names: Vec<&str> = qs.iter().map(|q| q["question"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["What do mitochondria make?", "Letter answer", "Text answer", "Late time", "Bad time"]);
    assert_eq!(qs[0]["at_ms"], 754_000);
    assert_eq!(qs[1]["answer"], 2);
    assert_eq!(qs[1]["at_ms"], 3_723_000);
    assert_eq!(qs[2]["answer"], 1);
    assert_eq!(qs[2]["at_ms"], 95_000);
    assert!(qs[3]["at_ms"].is_null(), "after the end of the lecture");
    assert!(qs[4]["at_ms"].is_null(), "not a time");
    // Stored material re-validates to itself
    assert_eq!(revalidate(StudyKind::Quiz, &v.to_string(), 3_800_000).unwrap(), v);
}

#[test]
fn text_is_cleaned_capped_and_never_interpreted() {
    let long = "x".repeat(5000);
    let raw = json!({"cards": [
        {"front": "  - 1. What is\n\tosmosis? \u{7}", "back": "<img src=x onerror=alert(1)> water moves"},
        {"front": long, "back": "b"},
        {"front": "What is osmosis?", "back": "duplicate front, dropped"}
    ]})
    .to_string();
    let v = validate(StudyKind::Flashcards, &raw, 0).unwrap();
    let cards = v["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0]["front"], "What is osmosis?");
    // Kept as text: the UI renders it as text (no HTML), exports escape it
    assert_eq!(cards[0]["back"], "<img src=x onerror=alert(1)> water moves");
    assert!(cards[1]["front"].as_str().unwrap().chars().count() <= 400);
    assert!(cards[1]["front"].as_str().unwrap().ends_with('…'));
    // Counts are capped
    let many = json!({"terms": (0..200).map(|i| json!({"term": format!("t{}", i), "definition": "d"})).collect::<Vec<_>>()});
    assert_eq!(validate(StudyKind::Terms, &many.to_string(), 0).unwrap()["terms"].as_array().unwrap().len(), MAX_TERMS);
}

#[test]
fn summary_and_questions_shapes() {
    let v = validate(
        StudyKind::Summary,
        r#"{"title": "Cells", "sections": [{"heading": "Organelles", "bullets": ["Mitochondria make ATP", 3]}, {"heading": "Empty", "bullets": []}, {"bullets": ["no heading"]}]}"#,
        0,
    )
    .unwrap();
    assert_eq!(v, json!({"title": "Cells", "sections": [{"heading": "Organelles", "bullets": ["Mitochondria make ATP", "3"]}]}));
    let v = validate(StudyKind::Questions, r#"{"questions": ["Why ATP?", {"question": "What about osmosis?", "time": "2:00"}, "why atp?"]}"#, 600_000)
        .unwrap();
    assert_eq!(v, json!({"questions": [{"question": "Why ATP?", "at_ms": null}, {"question": "What about osmosis?", "at_ms": 120_000}]}));
}

#[test]
fn clocks_parse_and_print() {
    assert_eq!(clock(0), "0:00");
    assert_eq!(clock(754_000), "12:34");
    assert_eq!(clock(3_725_000), "1:02:05");
    assert_eq!(parse_clock("12:34"), Some(754_000));
    assert_eq!(parse_clock("[1:02:05]"), Some(3_725_000));
    assert_eq!(parse_clock("95"), Some(95_000));
    assert_eq!(parse_clock("1:2:3:4"), None);
    assert_eq!(parse_clock("ab:cd"), None);
    assert_eq!(parse_clock("-5"), None);
    assert_eq!(parse_clock(""), None);
    assert_eq!(parse_clock("NaN"), None);
}

#[test]
fn condensed_lines_keep_times_and_drop_noise() {
    let ls = condensed_lines("```\n[1:00] - Mitosis has 4 phases\n\n* [bad] no time\n[2:30] **Osmosis** defined\n```", 10).unwrap();
    assert_eq!(ls, vec!["[1:00] Mitosis has 4 phases", "[bad] no time", "[2:30] **Osmosis** defined"]);
    assert!(condensed_lines("   \n```\n```", 10).is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// Prompt input and chunking
// ═══════════════════════════════════════════════════════════════════════════

fn input_with(lines: &[(i64, &str)], marks: &[(i64, &str, Option<&str>)]) -> StudyInput {
    StudyInput {
        title: "Biology 101".into(),
        class_name: Some("BIO 101".into()),
        duration_ms: lines.iter().map(|l| l.0).max().unwrap_or(0) + 5_000,
        lines: lines.iter().map(|(ms, t)| StudyLine { ms: *ms, text: t.to_string() }).collect(),
        marks: marks.iter().map(|(ms, k, n)| StudyMark { ms: *ms, kind: k.to_string(), note: n.map(String::from) }).collect(),
    }
}

#[test]
fn prompt_lines_carry_times_and_marks() {
    let i = input_with(
        &[(0, "Welcome."), (61_000, "[stricken from the record]"), (62_000, "[stricken from the record]"), (90_000, "Mitosis.")],
        &[(90_500, "test", Some("phases")), (10_000, "question", None)],
    );
    assert_eq!(i.transcript_lines(), vec!["[0:00] Welcome.", "[1:01] [stricken from the record]", "[1:30] Mitosis."]);
    let m = user_message(&i, false, &i.transcript_lines().join("\n"));
    assert!(m.contains("[1:30] ✎ On the test: phases"), "{}", m);
    assert!(m.contains("[0:10] ? Question"), "{}", m);
    assert!(m.contains("TRANSCRIPT:\n[0:00] Welcome."));
    assert!(m.starts_with("Lecture: Biology 101\nClass: BIO 101\n"), "{}", m);
    assert!(system_for(StudyKind::Quiz).contains("0-based index"));
    assert!(system_for(StudyKind::Summary).contains("never guess at or mention"));
    // The fingerprint follows the text
    let mut j = i.clone();
    assert_eq!(i.fingerprint(), j.fingerprint());
    j.lines[0].text = "Welcome!".into();
    assert_ne!(i.fingerprint(), j.fingerprint());
}

#[test]
fn chunks_pack_whole_lines_within_budget() {
    let lines: Vec<String> = (0..100).map(|i| format!("[{}:00] line number {}", i, i)).collect();
    let chunks = chunk_lines(&lines, 300);
    assert!(chunks.len() > 5);
    assert!(chunks.iter().all(|c| c.chars().count() <= 300));
    assert_eq!(chunks.join("\n"), lines.join("\n"), "nothing lost or reordered");
    let huge = vec!["y".repeat(1000)];
    let c = chunk_lines(&huge, 300);
    assert_eq!(c.len(), 4);
    assert_eq!(c.concat(), huge[0]);
    assert_eq!(chunk_span("[1:00] a\n[3:30] b\nno time"), Some((60_000, 210_000)));
}

// ═══════════════════════════════════════════════════════════════════════════
// Generation with a mock AI
// ═══════════════════════════════════════════════════════════════════════════

/// Answers by the kind named in the system prompt. Records every request.
struct Mock {
    ctx: usize,
    calls: Mutex<Vec<Vec<Msg>>>,
    /// Bad answers to give first, per kind ("summary", …, "condense")
    bad_first: Mutex<Vec<(&'static str, usize)>>,
    error: Option<AiError>,
}

impl Mock {
    fn new(ctx: usize) -> Self {
        Self { ctx, calls: Mutex::new(Vec::new()), bad_first: Mutex::new(Vec::new()), error: None }
    }
    /// Each request's text (system + user)
    fn texts(&self) -> Vec<String> {
        self.calls.lock().iter().map(|m| m.iter().map(text_of).collect::<Vec<_>>().join("\n")).collect()
    }
}

fn text_of(m: &Msg) -> String {
    m.parts
        .iter()
        .filter_map(|p| match p {
            crate::ai::Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn kind_of(system: &str) -> &'static str {
    if system.starts_with("You condense") {
        "condense"
    } else if system.contains("Write lecture notes") {
        "summary"
    } else if system.contains("key terms") {
        "terms"
    } else if system.contains("flashcards") {
        "flashcards"
    } else if system.contains("practice quiz") {
        "quiz"
    } else {
        "questions"
    }
}

fn good(kind: &str) -> String {
    match kind {
        "condense" => "[0:10] cells have organelles\n[0:20] mitochondria make ATP".into(),
        "summary" => r#"{"title": "Cells", "sections": [{"heading": "Organelles", "bullets": ["Mitochondria make ATP"]}]}"#.into(),
        "terms" => r#"{"terms": [{"term": "ATP", "definition": "Energy currency"}]}"#.into(),
        "flashcards" => r#"{"cards": [{"front": "What makes ATP?", "back": "Mitochondria"}]}"#.into(),
        "quiz" => r#"{"questions": [{"question": "What makes ATP?", "choices": ["Mitochondria", "Nucleus", "Ribosome", "Golgi"], "answer": 0, "explanation": "Said at 0:20.", "time": "0:20"}]}"#.into(),
        _ => r#"{"questions": [{"question": "Why ATP?", "time": "0:20"}]}"#.into(),
    }
}

#[async_trait::async_trait]
impl Completer for Mock {
    async fn complete(&self, msgs: Vec<Msg>, _opts: Opts) -> Result<String, AiError> {
        self.calls.lock().push(msgs.clone());
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let kind = kind_of(&text_of(&msgs[0]));
        let mut bad = self.bad_first.lock();
        if let Some(entry) = bad.iter_mut().find(|(k, n)| *k == kind && *n > 0) {
            entry.1 -= 1;
            return Ok("Sorry, here are your flashcards: front: ATP back: energy".into());
        }
        Ok(good(kind))
    }
    fn context_tokens(&self) -> usize {
        self.ctx
    }
}

fn no_progress() -> impl Fn(Progress) + Send + Sync {
    |_p: Progress| {}
}

#[tokio::test]
async fn short_lecture_is_one_request_per_part() {
    let m = Mock::new(32_768);
    let i = input_with(&[(10_000, "cells have organelles"), (20_000, "mitochondria make ATP")], &[(20_000, "test", None)]);
    let out = generate(&m, &i, &StudyKind::ALL, &no_progress()).await.unwrap();
    assert_eq!(out.len(), 5);
    assert!(out.iter().all(|(_, r)| r.is_ok()));
    assert_eq!(m.calls.lock().len(), 5, "no condensing for a short lecture");
    let texts = m.texts();
    assert!(texts.iter().all(|t| t.contains("[0:20] ✎ On the test")), "marks reach every part");
    assert!(texts.iter().all(|t| t.contains("TRANSCRIPT:")));
}

#[tokio::test]
async fn long_lecture_is_condensed_to_fit_a_small_window() {
    let m = Mock::new(4_096);
    let lines: Vec<(i64, String)> =
        (0..600).map(|i| (i as i64 * 6_000, format!("sentence {} about the cell cycle and mitosis phases", i))).collect();
    let refs: Vec<(i64, &str)> = lines.iter().map(|(a, b)| (*a, b.as_str())).collect();
    let i = input_with(&refs, &[]);
    let progress = Arc::new(Mutex::new(Vec::<Progress>::new()));
    let p2 = progress.clone();
    let out = generate(&m, &i, &StudyKind::ALL, &move |p: Progress| p2.lock().push(p)).await.unwrap();
    assert!(out.iter().all(|(_, r)| r.is_ok()));
    let calls = m.calls.lock().clone();
    let condense = calls.iter().filter(|c| kind_of(&text_of(&c[0])) == "condense").count();
    assert!(condense >= 2, "condensed in chunks ({} calls)", condense);
    // Every request leaves room for its answer in the 4K window by our
    // estimate, so the client never has to trim the middle out
    for c in &calls {
        let chars: usize = c.iter().map(|x| text_of(x).chars().count()).sum();
        assert!(chars < ((4_096 - 700 - 512) as f64 * CHARS_PER_TOKEN) as usize, "{} chars", chars);
    }
    let final_texts: Vec<String> = m.texts().into_iter().filter(|t| t.contains("LECTURE NOTES")).collect();
    assert_eq!(final_texts.len(), 5, "the parts use the condensed notes");
    let p = progress.lock();
    assert!(p.windows(2).all(|w| w[0].done <= w[1].done));
    assert_eq!(p.last().unwrap().label, "Done");
}

#[tokio::test]
async fn bad_answer_is_retried_once_then_reported_for_that_part() {
    let m = Mock::new(32_768);
    m.bad_first.lock().push(("flashcards", 1));
    m.bad_first.lock().push(("quiz", 2));
    let i = input_with(&[(10_000, "cells have organelles")], &[]);
    let out = generate(&m, &i, &StudyKind::ALL, &no_progress()).await.unwrap();
    let get = |k: StudyKind| out.iter().find(|(x, _)| *x == k).unwrap().1.clone();
    assert!(get(StudyKind::Flashcards).is_ok(), "fixed on the retry");
    let quiz = get(StudyKind::Quiz).unwrap_err();
    assert!(quiz.contains("practice quiz") && quiz.contains("asked twice"), "{}", quiz);
    assert!(get(StudyKind::Summary).is_ok() && get(StudyKind::Questions).is_ok(), "the other parts go on");
    assert_eq!(m.calls.lock().len(), 5 + 1 + 1);
    // The retry says why, without echoing the bad answer
    let retry = m.texts().into_iter().find(|t| t.contains("couldn't be used")).unwrap();
    assert!(!retry.contains("Sorry, here are your flashcards"));
}

#[tokio::test]
async fn consent_pro_and_access_errors_stop_the_whole_run() {
    for e in [
        AiError::ConsentRequired("custom".into()),
        AiError::ProRequired,
        AiError::NoProvider,
        AiError::Unreachable("down".into()),
    ] {
        let mut m = Mock::new(32_768);
        m.error = Some(e.clone());
        let i = input_with(&[(10_000, "cells")], &[]);
        match generate(&m, &i, &StudyKind::ALL, &no_progress()).await {
            Err(GenError::Ai(got)) => assert_eq!(got, e),
            other => panic!("{:?}", other.map(|_| ())),
        }
        assert_eq!(m.calls.lock().len(), 1, "stopped at the first request");
    }
    // An empty transcript never calls the AI
    let m = Mock::new(32_768);
    let i = input_with(&[(0, "[stricken from the record]")], &[]);
    assert!(matches!(generate(&m, &i, &StudyKind::ALL, &no_progress()).await, Err(GenError::Failed(_))));
    assert!(m.calls.lock().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Exports
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn csv_fields_are_quoted_and_safe() {
    assert_eq!(csv_field("plain"), "\"plain\"");
    assert_eq!(csv_field("a, b"), "\"a, b\"");
    assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_field("line1\nline2\r\nline3"), "\"line1 line2  line3\"");
    assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
    assert_eq!(csv_field("+1"), "\"'+1\"");
    assert_eq!(csv_field("@cmd"), "\"'@cmd\"");
    assert_eq!(csv_field("-5 degrees"), "\"-5 degrees\"", "a negative number is fine");
    assert_eq!(csv_field("-cmd"), "\"'-cmd\"");
    assert_eq!(csv_field("café ✎"), "\"café ✎\"");
    let csv = flashcards_csv(&[("What, why?".into(), "Because \"so\"".into()), ("a".into(), "b".into())]);
    assert_eq!(csv, "\"What, why?\",\"Because \"\"so\"\"\"\r\n\"a\",\"b\"\r\n");
}

#[test]
fn markdown_guide_escapes_model_text_and_lists_marks() {
    let summary = json!({"title": "Cells", "sections": [{"heading": "Org*an*elles", "bullets": ["[click](javascript:alert(1)) <b>x</b>"]}]});
    let quiz = json!({"questions": [{"question": "Q1?", "choices": ["a", "b"], "answer": 1, "explanation": "because", "at_ms": 754_000}]});
    let questions = json!({"questions": [{"question": "Why?", "at_ms": 60_000}]});
    let marks = vec![
        GuideMark { ms: 90_000, kind: "test".into(), note: Some("phases_of *mitosis*".into()) },
        GuideMark { ms: 120_000, kind: "question".into(), note: None },
    ];
    let md = guide_markdown(&GuideParts {
        title: "Bio #1",
        when: "Monday",
        summary: Some(&summary),
        quiz: Some(&quiz),
        questions: Some(&questions),
        marks: &marks,
        ..Default::default()
    });
    assert!(md.starts_with("# Study guide: Bio \\#1\n"));
    assert!(md.contains("### Org\\*an\\*elles"));
    assert!(md.contains("\\[click\\](javascript:alert(1)) \\<b\\>x\\</b\\>"), "{}", md);
    assert!(!md.contains("[click]("));
    assert!(md.contains("- 1:30 ✎ On the test: phases\\_of \\*mitosis\\*"));
    assert!(md.contains("- Why? (1:00)"));
    assert!(md.contains("- You marked 2:00 as confusing"));
    assert!(md.contains("1. Q1?\n   - A) a\n   - B) b"));
    assert!(md.contains("### Answer key\n\n1. B: because (12:34)"));
    assert_eq!(file_stem("Bio: Cells/Part 2"), "Bio Cells Part 2");
    assert_eq!(file_stem("///"), "Lecture");
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
    let dir = std::env::temp_dir().join(format!("nf-study-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db_path = data.join("nofriction_meetings.db");
    let db = DatabaseManager::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Biology 101").await.unwrap();
    let env = RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() };
    Fx { db: Arc::new(db), dir, env, db_path }
}

async fn start(f: &Fx) -> chrono::DateTime<Utc> {
    parse_ts(&sqlx::query_scalar::<_, String>("SELECT started_at FROM meetings WHERE id = 'm1'").fetch_one(f.db.pool()).await.unwrap())
        .unwrap()
}

async fn line_at(f: &Fx, secs: i64, text: &str) -> i64 {
    let t0 = start(f).await;
    f.db.add_transcript_at("m1", text, None, true, 0.9, t0 + chrono::Duration::seconds(secs)).await.unwrap()
}

async fn study_rows(pool: &Pool<Sqlite>) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM study_materials WHERE meeting_id = 'm1'").fetch_one(pool).await.unwrap()
}

async fn make_guide(f: &Fx) {
    let input = load_input(f.db.pool(), "m1").await.unwrap();
    let m = Mock::new(32_768);
    let out = generate(&m, &input, &StudyKind::ALL, &no_progress()).await.unwrap();
    let ok: Vec<(StudyKind, Value)> = out.into_iter().map(|(k, r)| (k, r.unwrap())).collect();
    save_materials(f.db.pool(), "m1", &input.fingerprint(), &ok).await.unwrap();
}

fn utf16_span(hay: &str, needle: &str) -> (usize, usize) {
    let b = hay.find(needle).unwrap();
    let s = hay[..b].encode_utf16().count();
    (s, s + needle.encode_utf16().count())
}

#[tokio::test]
async fn saved_guide_loads_back_validated_and_fresh() {
    let f = fx().await;
    line_at(&f, 10, "cells have organelles").await;
    line_at(&f, 20, "mitochondria make ATP").await;
    make_guide(&f).await;
    let g = load_guide(f.db.pool(), "m1").await.unwrap();
    assert_eq!(g.materials.len(), 5);
    assert!(g.materials.values().all(|m| !m.stale));
    assert_eq!(g.materials["quiz"].data["questions"][0]["at_ms"], 20_000);
    // Remaking one part replaces only that part
    make_guide(&f).await;
    assert_eq!(study_rows(f.db.pool()).await, 5);
    // A row tampered with outside the app never reaches the UI unchecked
    sqlx::query("UPDATE study_materials SET json = '{\"cards\": 5}' WHERE kind = 'flashcards'").execute(f.db.pool()).await.unwrap();
    assert!(!load_guide(f.db.pool(), "m1").await.unwrap().materials.contains_key("flashcards"));
    // The transcript changing later marks the guide as made from older text
    line_at(&f, 30, "a new line").await;
    assert!(load_guide(f.db.pool(), "m1").await.unwrap().materials.values().all(|m| m.stale));
}

#[tokio::test]
async fn a_guide_made_before_an_edit_is_never_saved_after_it() {
    let f = fx().await;
    let text = "the Krebs cycle runs in the matrix";
    let id = line_at(&f, 10, text).await;
    let input = load_input(f.db.pool(), "m1").await.unwrap();
    let before = input.fingerprint();
    // Generation runs… meanwhile the user strikes words
    rd::strike_words(
        f.db.pool(),
        &f.env,
        &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: utf16_span(text, "Krebs cycle").0, end: utf16_span(text, "Krebs cycle").1, expected_text: None, whole_line: false },
        None,
    )
    .await
    .unwrap();
    let e = save_materials(f.db.pool(), "m1", &before, &[(StudyKind::Terms, json!({"terms": [{"term": "Krebs cycle", "definition": "d"}]}))])
        .await
        .unwrap_err();
    assert_eq!(e, TRANSCRIPT_CHANGED);
    assert_eq!(study_rows(f.db.pool()).await, 0);
    // A Delete still in its undo window also blocks the save
    let id2 = line_at(&f, 30, "glycolysis happens in the cytoplasm").await;
    let now = load_input(f.db.pool(), "m1").await.unwrap().fingerprint();
    let p = rd::request_delete_words(
        f.db.pool(),
        &f.env,
        &WordTarget { meeting_id: "m1".into(), transcript_id: id2, start: 0, end: 10, expected_text: None, whole_line: false },
    )
    .await
    .unwrap();
    assert_eq!(
        save_materials(f.db.pool(), "m1", &now, &[(StudyKind::Terms, json!({"terms": [{"term": "x", "definition": "y"}]}))]).await.unwrap_err(),
        TRANSCRIPT_CHANGED
    );
    rd::undo_delete(f.db.pool(), &p.id).await.unwrap();
}

#[tokio::test]
async fn prompts_never_carry_stricken_or_deleted_words() {
    let f = fx().await;
    let a = "the secret exam answer is Zanzibar";
    let id = line_at(&f, 10, a).await;
    let b = "photosynthesis needs light";
    let id2 = line_at(&f, 20, b).await;
    let (s, e) = utf16_span(a, "Zanzibar");
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    let (s, e) = utf16_span(b, "light");
    let p = rd::request_delete_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id2, start: s, end: e, expected_text: None, whole_line: false })
        .await
        .unwrap();
    rd::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    let input = load_input(f.db.pool(), "m1").await.unwrap();
    let m = Mock::new(32_768);
    generate(&m, &input, &StudyKind::ALL, &no_progress()).await.unwrap();
    for t in m.texts() {
        assert!(!t.contains("Zanzibar") && !t.contains("light"), "{}", t);
        assert!(!t.contains("strickenid"), "marker ids never leave");
        assert!(t.contains("[stricken from the record]"));
    }
}

#[tokio::test]
async fn word_line_and_time_range_edits_delete_the_guide() {
    // Strike words
    let f = fx().await;
    let text = "osmosis moves water across membranes";
    let id = line_at(&f, 10, text).await;
    make_guide(&f).await;
    let (s, e) = utf16_span(text, "water");
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 0, "strike");

    // Delete a whole line: kept during the undo window, deleted at commit
    make_guide(&f).await;
    let p = rd::request_delete_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: 0, end: 0, expected_text: None, whole_line: true })
        .await
        .unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 5, "the undo window keeps it");
    rd::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 0, "line delete");

    // Time range delete
    line_at(&f, 60, "diffusion is passive").await;
    make_guide(&f).await;
    let preview = time_range::preview_time_ranges(f.db.pool(), &f.env, "m1", &[MsRange { start_ms: 55_000, end_ms: 70_000 }]).await.unwrap();
    assert!(preview.items.iter().any(|i| i.contains("study guide")), "{:?}", preview.items);
    let p = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &[MsRange { start_ms: 55_000, end_ms: 70_000 }], Some(preview.counts))
        .await
        .unwrap();
    rd::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 0, "time range");
}

#[tokio::test]
async fn backups_lose_the_guide_too() {
    let f = fx().await;
    let text = "the Calvin cycle fixes carbon";
    let id = line_at(&f, 10, text).await;
    make_guide(&f).await;
    rd::wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();
    let (s, e) = utf16_span(text, "Calvin cycle");
    let out = rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    assert_eq!((out.backups_purged, out.backups_deleted), (1, 0), "{:?}", out.warnings);
    let mut c = sqlx::sqlite::SqliteConnectOptions::new().filename(&copy).connect().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM study_materials").fetch_one(&mut c).await.unwrap();
    assert_eq!(n, 0);
    c.close().await.unwrap();
}

#[tokio::test]
async fn screen_only_edits_keep_the_guide() {
    // A guide is made from the transcript; deleting screens doesn't touch it
    let f = fx().await;
    line_at(&f, 10, "the cell membrane").await;
    make_guide(&f).await;
    let t0 = start(&f).await;
    sqlx::query("INSERT INTO frames (meeting_id, timestamp, frame_number, file_path) VALUES ('m1', ?, 1, '')")
        .bind((t0 + chrono::Duration::seconds(100)).to_rfc3339())
        .execute(f.db.pool())
        .await
        .unwrap();
    let preview = time_range::preview_time_ranges(f.db.pool(), &f.env, "m1", &[MsRange { start_ms: 95_000, end_ms: 105_000 }]).await.unwrap();
    assert_eq!(preview.counts.screens, 1);
    assert!(!preview.items.iter().any(|i| i.contains("study guide")));
    let p = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &[MsRange { start_ms: 95_000, end_ms: 105_000 }], Some(preview.counts))
        .await
        .unwrap();
    rd::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 5);
}

// ═══════════════════════════════════════════════════════════════════════════
// Moment markers and time ranges
// ═══════════════════════════════════════════════════════════════════════════

async fn marker_ids(f: &Fx) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM meeting_markers WHERE meeting_id = 'm1' ORDER BY ts").fetch_all(f.db.pool()).await.unwrap()
}

#[tokio::test]
async fn time_range_delete_removes_markers_inside_at_commit_and_undo_keeps_them() {
    let f = fx().await;
    line_at(&f, 10, "intro").await;
    line_at(&f, 120, "the important part").await;
    let before = crate::markers::add_at_offset(f.db.pool(), "m1", 5_000, None, Some("keep me")).await.unwrap();
    let inside = crate::markers::add_at_offset(f.db.pool(), "m1", 121_000, Some("test"), Some("chapter 4")).await.unwrap();
    let after = crate::markers::add_at_offset(f.db.pool(), "m1", 300_000, Some("question"), None).await.unwrap();
    let range = [MsRange { start_ms: 100_000, end_ms: 200_000 }];

    let p = time_range::preview_time_ranges(f.db.pool(), &f.env, "m1", &range).await.unwrap();
    assert_eq!(p.moment_markers, 1);
    assert!(p.items.iter().any(|i| i.contains("1 moment marker")), "{:?}", p.items);

    // Undo: nothing changes
    let pending = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &range, Some(p.counts)).await.unwrap();
    assert_eq!(marker_ids(&f).await.len(), 3, "kept during the undo window");
    rd::undo_delete(f.db.pool(), &pending.id).await.unwrap();
    assert_eq!(marker_ids(&f).await, vec![before.id.clone(), inside.id.clone(), after.id.clone()]);

    // Commit: only the marker inside goes, with its note
    let pending = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &range, Some(p.counts)).await.unwrap();
    rd::commit_pending(f.db.pool(), &f.env, &pending.id).await.unwrap();
    assert_eq!(marker_ids(&f).await, vec![before.id.clone(), after.id.clone()]);
    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_markers WHERE note = 'chapter 4'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(notes, 0);
}

#[tokio::test]
async fn time_range_strike_removes_markers_and_a_range_with_only_markers_still_deletes() {
    let f = fx().await;
    line_at(&f, 10, "intro").await;
    line_at(&f, 120, "the important part").await;
    crate::markers::add_at_offset(f.db.pool(), "m1", 121_000, Some("test"), None).await.unwrap();
    let out = time_range::strike_time_ranges(f.db.pool(), &f.env, "m1", &[MsRange { start_ms: 100_000, end_ms: 200_000 }], None, None)
        .await
        .unwrap();
    assert!(!out.records.is_empty());
    assert!(marker_ids(&f).await.is_empty());

    // Nothing in the range but a marker: the Delete still removes it
    let m = crate::markers::add_at_offset(f.db.pool(), "m1", 400_000, None, Some("lonely")).await.unwrap();
    let range = [MsRange { start_ms: 390_000, end_ms: 410_000 }];
    let p = time_range::preview_time_ranges(f.db.pool(), &f.env, "m1", &range).await.unwrap();
    assert!(!p.nothing);
    let pending = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &range, Some(p.counts)).await.unwrap();
    rd::commit_pending(f.db.pool(), &f.env, &pending.id).await.unwrap();
    assert!(!marker_ids(&f).await.contains(&m.id));
}

#[tokio::test]
async fn time_range_purges_markers_in_app_backups() {
    let f = fx().await;
    line_at(&f, 120, "the important part").await;
    crate::markers::add_at_offset(f.db.pool(), "m1", 121_000, Some("question"), Some("ask about this")).await.unwrap();
    crate::markers::add_at_offset(f.db.pool(), "m1", 500_000, None, None).await.unwrap();
    rd::wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();
    let range = [MsRange { start_ms: 100_000, end_ms: 200_000 }];
    let p = time_range::preview_time_ranges(f.db.pool(), &f.env, "m1", &range).await.unwrap();
    let pending = time_range::request_delete_time_ranges(f.db.pool(), &f.env, "m1", &range, Some(p.counts)).await.unwrap();
    let out = rd::commit_pending(f.db.pool(), &f.env, &pending.id).await.unwrap().unwrap();
    assert_eq!(out.backups_purged, 1, "{:?}", out.warnings);
    let mut c = sqlx::sqlite::SqliteConnectOptions::new().filename(&copy).connect().await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT kind FROM meeting_markers").fetch_all(&mut c).await.unwrap();
    assert_eq!(left, vec!["important".to_string()]);
    c.close().await.unwrap();
}

#[tokio::test]
async fn meeting_delete_removes_markers_and_study_materials() {
    let f = fx().await;
    line_at(&f, 10, "cells").await;
    make_guide(&f).await;
    crate::markers::add_at_offset(f.db.pool(), "m1", 1_000, None, Some("note")).await.unwrap();
    f.db.delete_meeting("m1").await.unwrap();
    assert_eq!(study_rows(f.db.pool()).await, 0);
    assert!(marker_ids(&f).await.is_empty());
}

#[tokio::test]
async fn markdown_export_of_a_saved_guide() {
    let f = fx().await;
    line_at(&f, 10, "cells have organelles").await;
    line_at(&f, 20, "mitochondria make ATP").await;
    crate::markers::add_at_offset(f.db.pool(), "m1", 20_000, Some("test"), Some("ATP")).await.unwrap();
    make_guide(&f).await;
    let g = load_guide(f.db.pool(), "m1").await.unwrap();
    let md = commands::guide_markdown_of(&g);
    for part in ["## Summary", "## Key terms", "## Marked moments", "0:20 ✎ On the test: ATP", "## Questions to ask", "## Flashcards", "## Practice quiz", "### Answer key"] {
        assert!(md.contains(part), "missing {}: {}", part, md);
    }
    let csv = flashcards_csv(&cards_of(&g.materials["flashcards"].data));
    assert_eq!(csv, "\"What makes ATP?\",\"Mitochondria\"\r\n");
}
