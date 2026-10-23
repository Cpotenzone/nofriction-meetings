//! Make lesson (docs/LESSONS.md §13) with a mock AI: no endpoint is ever
//! called. Covers the generate → validate → store → load → purge round trip,
//! per-part retry and reporting, concept-graph and reference repair, small
//! context condensing, the run-stopping errors, the save-time transcript
//! re-check, and the generated validation profile (text and MC only, no
//! model numbers, no figures).

use super::parse::{self, ConceptDraft, ConceptLookup, LessonScope};
use super::*;
use crate::ai::{AiError, Msg, Opts};
use crate::database::DatabaseManager;
use crate::lessons::schema::{self, Profile};
use crate::redaction::{self as rd, RedactionEnv, WordTarget};
use crate::study::prompt::{StudyInput, StudyLine, CHARS_PER_TOKEN};
use crate::study::{Completer, GenError, Progress};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// The mock AI
// ═══════════════════════════════════════════════════════════════════════════

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

/// Which part a request asks for, from its system prompt.
fn part_of(system: &str) -> &'static str {
    if system.starts_with("You condense") {
        "condense"
    } else if system.contains("Name the concepts") {
        "concepts"
    } else if system.contains("Plan the lessons") {
        "outline"
    } else if system.contains("Write the explanations") {
        "explanations"
    } else if system.contains("Write worked examples") {
        "examples"
    } else if system.contains("common mistakes a student") {
        "mistakes"
    } else if system.contains("final exam for the whole lecture") {
        "exam"
    } else if system.contains("checkpoint quiz") {
        "checkpoint"
    } else {
        "unknown"
    }
}

fn mc(q: &str, answer: Value, concepts: &[&str]) -> Value {
    json!({
        "question": q,
        "choices": ["Prophase", "Metaphase", "Anaphase", "Telophase"],
        "answer": answer,
        "explanation": "Said in the lecture.",
        "concepts": concepts,
    })
}

fn good(part: &str) -> String {
    match part {
        "condense" => "[0:10] the cell cycle has interphase and mitosis\n[0:20] mitosis starts with prophase".into(),
        "concepts" => json!({
            "title": "The cell cycle",
            "concepts": [
                {"id": "c1", "name": "Cell cycle", "summary": "Interphase then mitosis.", "prerequisites": []},
                {"id": "c2", "name": "Mitosis", "summary": "Division of the nucleus.", "prerequisites": ["c1"]},
                {"id": "c3", "name": "Prophase", "summary": "Chromosomes condense.", "prerequisites": ["c2"]}
            ]
        })
        .to_string(),
        "outline" => json!({
            "lessons": [
                {"title": "The cycle", "objectives": ["Describe the cell cycle"], "concepts": ["c1"]},
                {"title": "Mitosis", "objectives": ["Order the phases of mitosis"], "concepts": ["c2", "c3"]}
            ]
        })
        .to_string(),
        "explanations" => json!({
            "explanations": [
                {"title": "What it is", "concepts": ["c1"], "paragraphs": ["The cycle has interphase and mitosis.", "Interphase is the longest part."]},
                {"title": "Order of events", "paragraphs": "Mitosis begins with prophase."}
            ]
        })
        .to_string(),
        "examples" => json!({
            "examples": [
                {"prompt": "A cell finished interphase. What comes next?",
                 "steps": ["Interphase ends.", {"text": "Mitosis begins with prophase.", "result": "42 g"}],
                 "answer": "It enters prophase.", "final_answer": 42, "concepts": ["c1"]}
            ]
        })
        .to_string(),
        "mistakes" => json!({
            "mistakes": [
                {"claim": "Interphase is a resting phase", "why_wrong": "The cell grows and copies its DNA", "correct": "Interphase is busy"},
                {"claim": "Mitosis is the whole cycle", "why_wrong": "It is one part", "correct": "Interphase comes first"}
            ]
        })
        .to_string(),
        "checkpoint" => json!({
            "questions": [
                mc("Which phase comes first?", json!(0), &["c1"]),
                mc("Which phase lines chromosomes up?", json!("B"), &["c2"]),
                mc("Which phase pulls chromatids apart?", json!("Anaphase"), &["c3"]),
                mc("Which phase ends mitosis?", json!(3), &[]),
                mc("Where do chromosomes condense?", json!("a)"), &["nope"]),
                mc("What follows prophase?", json!(1), &["c2"])
            ]
        })
        .to_string(),
        "exam" => json!({
            "questions": [
                mc("Exam: first phase?", json!(0), &["c1"]),
                mc("Exam: alignment phase?", json!(1), &["c2"]),
                mc("Exam: separation phase?", json!(2), &["c3"]),
                mc("Exam: last phase?", json!(3), &["c2", "c3"]),
                mc("Exam: condensation phase?", json!(0), &["c3"])
            ]
        })
        .to_string(),
        _ => "{}".into(),
    }
}

struct Mock {
    ctx: usize,
    /// Each request and its answer budget
    calls: Mutex<Vec<(Vec<Msg>, u32)>>,
    /// Bad answers to give first, per part
    bad_first: Mutex<Vec<(&'static str, usize)>>,
    /// A different good answer for a part
    answers: Mutex<HashMap<&'static str, String>>,
    error: Option<AiError>,
}

impl Mock {
    fn new(ctx: usize) -> Self {
        Self {
            ctx,
            calls: Mutex::new(Vec::new()),
            bad_first: Mutex::new(Vec::new()),
            answers: Mutex::new(HashMap::new()),
            error: None,
        }
    }
    fn texts(&self) -> Vec<String> {
        self.calls.lock().iter().map(|(m, _)| m.iter().map(text_of).collect::<Vec<_>>().join("\n")).collect()
    }
    fn count(&self, part: &str) -> usize {
        self.calls.lock().iter().filter(|(m, _)| part_of(&text_of(&m[0])) == part).count()
    }
}

#[async_trait::async_trait]
impl Completer for Mock {
    async fn complete(&self, msgs: Vec<Msg>, opts: Opts) -> Result<String, AiError> {
        self.calls.lock().push((msgs.clone(), opts.max_tokens));
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let part = part_of(&text_of(&msgs[0]));
        if let Some(entry) = self.bad_first.lock().iter_mut().find(|(k, n)| *k == part && *n > 0) {
            entry.1 -= 1;
            return Ok("Sure! Here is your lesson: it is about cells.".into());
        }
        if let Some(a) = self.answers.lock().get(part) {
            return Ok(a.clone());
        }
        Ok(good(part))
    }
    fn context_tokens(&self) -> usize {
        self.ctx
    }
}

fn no_progress() -> impl Fn(Progress) + Send + Sync {
    |_p: Progress| {}
}

fn class_input(lines: &[(i64, &str)]) -> StudyInput {
    StudyInput {
        title: "Biology 101: the cell cycle".into(),
        kind: RecordingKind::Class,
        notebook: Some("BIO 101".into()),
        duration_ms: lines.last().map(|l| l.0).unwrap_or(0),
        lines: lines.iter().map(|(ms, t)| StudyLine { ms: *ms, text: t.to_string() }).collect(),
        marks: Vec::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Generation
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn short_lecture_makes_a_validated_text_and_mc_lesson_set() {
    let m = Mock::new(32_768);
    let input = class_input(&[(10_000, "the cell cycle has interphase and mitosis"), (20_000, "mitosis starts with prophase")]);
    let made = generate(&m, &input, "m1", "set-1", &no_progress()).await.unwrap();
    assert!(made.failed.is_empty(), "{:?}", made.failed);
    assert_eq!(m.count("condense"), 0, "a short lecture isn't condensed");
    // concepts + outline + 2 lessons × 4 parts + exam
    assert_eq!(m.calls.lock().len(), 2 + 2 * 4 + 1);

    let v = &made.validated;
    assert_eq!(v.set.provenance, "generated");
    assert_eq!(v.set.source_recording_id.as_deref(), Some("m1"));
    assert_eq!(v.set.transcript_fingerprint.as_deref(), Some(input.fingerprint().as_str()));
    assert_eq!(v.set.title, "The cell cycle");
    assert_eq!(v.set.level, "BIO 101");
    assert!(v.figures.is_empty());
    assert_eq!(v.lessons.len(), 2);
    assert_eq!(v.set.concepts.iter().map(|c| c.lesson_ids.clone()).collect::<Vec<_>>(), vec![vec!["l1"], vec!["l2"], vec!["l2"]]);
    for l in &v.lessons {
        assert!(l.explainers.iter().all(|e| e.kind == "text"), "only text explainers");
        assert!(l.explainers.iter().all(|e| e.data["paragraphs"].as_array().is_some_and(|p| !p.is_empty())));
        // The model's number is never stored: no finalAnswer, no step result
        for w in &l.worked_examples {
            assert!(w.final_answer.is_none());
            assert_eq!(w.answer_text.as_deref(), Some("It enters prophase."));
            assert!(w.steps.iter().all(|s| s.result.is_none() && s.expression.is_none() && s.unit.is_none()));
        }
        assert_eq!(l.misconceptions.items.len(), 2);
        assert!(l.trainer.generators.is_empty());
        assert!(l.checkpoint.items.iter().all(|q| q.kind == "mc" && q.answer.is_none() && q.tolerance.is_none()));
        assert_eq!(l.checkpoint.present_count as usize, l.checkpoint.items.len().min(CHECKPOINT_QUESTIONS));
        // Every tag names one of the lesson's own concepts
        for q in &l.checkpoint.items {
            assert!(q.concept_ids.iter().all(|c| l.concept_ids.contains(c)), "{:?} in {:?}", q.concept_ids, l.concept_ids);
        }
    }
    // MC answers were resolved into a stored index: index, letter, choice text
    let cp = &v.lessons[1].checkpoint.items;
    let idx = |p: &str| cp.iter().find(|q| q.prompt == p).unwrap().answer_index.unwrap();
    assert_eq!(idx("Which phase comes first?"), 0);
    assert_eq!(idx("Which phase lines chromosomes up?"), 1);
    assert_eq!(idx("Which phase pulls chromatids apart?"), 2);
    assert_eq!(idx("Where do chromosomes condense?"), 0);
    // The exam: its own questions plus copies of the checkpoint questions
    let e = &v.set.exam;
    assert!(e.items.iter().all(|q| q.kind == "mc"));
    assert_eq!(e.mix.mc, e.question_count);
    assert_eq!((e.mix.numeric, e.mix.balance, e.mix.multistep), (0, 0, 0));
    assert_eq!(e.pass_pct, 80);
    assert!(e.items.iter().any(|q| q.id.starts_with("exam-q")) && e.items.iter().any(|q| q.id.starts_with("x-")));
    let ids: std::collections::HashSet<&str> = e.items.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids.len(), e.items.len(), "exam ids are unique");
    assert!(v.set.diagnostic.is_none());
}

#[tokio::test]
async fn bad_part_is_retried_once_then_reported_for_that_part() {
    let m = Mock::new(32_768);
    m.bad_first.lock().push(("checkpoint", 1)); // fixed on the retry
    m.bad_first.lock().push(("mistakes", 2)); // unusable twice, for lesson 1
    let input = class_input(&[(10_000, "the cell cycle")]);
    let made = generate(&m, &input, "m1", "set-2", &no_progress()).await.unwrap();
    assert_eq!(made.failed.len(), 1, "{:?}", made.failed);
    let f = &made.failed[0];
    assert_eq!((f.part.as_str(), f.lesson), ("mistakes", Some(1)));
    assert!(f.error.contains("common mistakes for lesson 1") && f.error.contains("asked twice"), "{}", f.error);
    // The other parts and lessons go on
    assert_eq!(made.validated.lessons.len(), 2);
    assert!(made.validated.lessons[0].misconceptions.items.is_empty());
    assert_eq!(made.validated.lessons[1].misconceptions.items.len(), 2);
    assert_eq!(m.count("checkpoint"), 2 + 1);
    assert_eq!(m.count("mistakes"), 2 + 1);
    // The retry says why, without echoing the bad answer
    let retry = m.texts().into_iter().find(|t| t.contains("couldn't be used")).unwrap();
    assert!(!retry.contains("Here is your lesson"));
}

#[tokio::test]
async fn a_lesson_without_explanations_or_a_checkpoint_is_left_out_and_reported() {
    let m = Mock::new(32_768);
    // Lesson 1's checkpoint is unusable twice; lesson 2's is fine
    m.bad_first.lock().push(("checkpoint", 2));
    let input = class_input(&[(10_000, "the cell cycle")]);
    let made = generate(&m, &input, "m1", "set-3", &no_progress()).await.unwrap();
    assert_eq!(made.failed.iter().map(|f| (f.part.as_str(), f.lesson)).collect::<Vec<_>>(), vec![("checkpoint", Some(1))]);
    let v = &made.validated;
    assert_eq!(v.lessons.len(), 1);
    assert_eq!((v.lessons[0].id.as_str(), v.lessons[0].index, v.lessons[0].title.as_str()), ("l1", 1, "Mitosis"));
    // The left-out lesson's concept goes with it; nothing points at it
    assert_eq!(v.set.concepts.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["c2", "c3"]);
    assert!(v.set.concepts.iter().all(|c| c.prerequisites.iter().all(|p| p != "c1")));
    assert!(v.set.exam.items.iter().all(|q| !q.concept_ids.contains(&"c1".to_string())));

    // With every lesson's explanations unusable, nothing is made
    let m = Mock::new(32_768);
    m.bad_first.lock().push(("explanations", 4));
    match generate(&m, &input, "m1", "set-4", &no_progress()).await {
        Err(GenError::Failed(e)) => assert!(e.contains("Couldn't make any lesson") && e.contains("explanations"), "{}", e),
        other => panic!("{:?}", other.map(|_| ())),
    }
    // The concepts and the plan are required
    let m = Mock::new(32_768);
    m.bad_first.lock().push(("outline", 2));
    assert!(matches!(generate(&m, &input, "m1", "set-5", &no_progress()).await, Err(GenError::Failed(e)) if e.contains("lesson plan")));
}

#[tokio::test]
async fn a_failed_exam_part_still_leaves_a_cumulative_exam() {
    let m = Mock::new(32_768);
    m.bad_first.lock().push(("exam", 2));
    let made = generate(&m, &class_input(&[(10_000, "the cell cycle")]), "m1", "set-6", &no_progress()).await.unwrap();
    assert_eq!(made.failed.iter().map(|f| f.part.as_str()).collect::<Vec<_>>(), vec!["exam"]);
    let e = &made.validated.set.exam;
    assert!(e.items.iter().all(|q| q.id.starts_with("x-")), "drawn from the checkpoints");
    assert!(e.question_count as usize >= schema::GEN_MIN_EXAM_BANK);
}

#[tokio::test]
async fn long_lecture_is_condensed_to_fit_a_small_window() {
    let m = Mock::new(4_096);
    let lines: Vec<(i64, String)> =
        (0..600).map(|i| (i as i64 * 6_000, format!("sentence {} about the cell cycle and the phases of mitosis", i))).collect();
    let refs: Vec<(i64, &str)> = lines.iter().map(|(a, b)| (*a, b.as_str())).collect();
    let input = class_input(&refs);
    let progress = Arc::new(Mutex::new(Vec::<Progress>::new()));
    let p2 = progress.clone();
    let made = generate(&m, &input, "m1", "set-7", &move |p: Progress| p2.lock().push(p)).await.unwrap();
    assert!(made.failed.is_empty(), "{:?}", made.failed);
    assert!(m.count("condense") >= 2, "condensed in chunks ({} calls)", m.count("condense"));
    // Every request leaves room for its own answer in the 4K window by our
    // estimate, so the client never has to trim the middle out
    for (msgs, max_tokens) in m.calls.lock().iter() {
        let chars: usize = msgs.iter().map(|x| text_of(x).chars().count()).sum();
        let room = ((4_096 - *max_tokens as usize - 512) as f64 * CHARS_PER_TOKEN) as usize;
        assert!(chars < room, "{} chars, room for {} ({})", chars, room, part_of(&text_of(&msgs[0])));
    }
    // The parts are written from the condensed notes
    let written: Vec<String> = m.texts().into_iter().filter(|t| !t.starts_with("You condense")).collect();
    assert!(written.iter().all(|t| t.contains("LECTURE NOTES")), "parts use the notes");
    let p = progress.lock();
    assert!(p.windows(2).all(|w| w[0].done <= w[1].done), "progress only moves forward");
    assert_eq!(p.last().unwrap().label, "Done");
    assert!(p.iter().any(|x| x.label.starts_with("Reading the recording")));
    assert!(p.iter().any(|x| x.label.starts_with("Lesson 2 of 2: writing the checkpoint")));
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
        match generate(&m, &class_input(&[(10_000, "cells")]), "m1", "s", &no_progress()).await {
            Err(GenError::Ai(got)) => assert_eq!(got, e),
            other => panic!("{:?}", other.map(|_| ())),
        }
        assert_eq!(m.calls.lock().len(), 1, "stopped at the first request");
    }
    // Only a Class recording makes a lesson, and an empty transcript never calls the AI
    let m = Mock::new(32_768);
    let mut meeting = class_input(&[(10_000, "the budget")]);
    meeting.kind = RecordingKind::Meeting;
    assert!(matches!(generate(&m, &meeting, "m1", "s", &no_progress()).await, Err(GenError::Failed(e)) if e == NOT_A_CLASS));
    let empty = class_input(&[(0, "[stricken from the record]")]);
    assert!(matches!(generate(&m, &empty, "m1", "s", &no_progress()).await, Err(GenError::Failed(e)) if e == NO_TRANSCRIPT));
    assert!(m.calls.lock().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Parsing and repair (untrusted model output)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cyclic_concept_graph_is_repaired_into_a_dag() {
    let raw = json!({"concepts": [
        {"id": "a", "name": "Moles", "prerequisites": ["b"]},           // forward edge that closes a cycle
        {"id": "b", "name": "Molar mass", "prerequisites": ["a", "b"]},  // keeps a; self-loop dropped
        {"id": "c", "name": "Mole ratio", "prerequisites": ["Molar mass", "zzz"]}, // by name; unknown dropped
        {"id": "d", "name": "Yield", "prerequisites": ["e"]},            // forward edge, no cycle: kept
        {"id": "e", "name": "Limiting reagent", "prerequisites": ["c"]},
        {"id": "f", "name": "moles"}                                      // the same concept again: dropped
    ]})
    .to_string();
    let d = parse::concepts(&raw).unwrap();
    let ids: Vec<&str> = d.concepts.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["c1", "c2", "c3", "c4", "c5"]);
    let pre = |i: usize| d.concepts[i].prerequisites.clone();
    assert!(pre(0).is_empty(), "the edge closing the cycle is dropped");
    assert_eq!(pre(1), vec!["c1"]);
    assert_eq!(pre(2), vec!["c2"]);
    assert_eq!(pre(3), vec!["c5"]);
    assert_eq!(pre(4), vec!["c3"]);
    assert_eq!(d.repaired_edges, 3);
    // What is left passes the strict DAG check the bundle validator uses
    let as_schema: Vec<schema::Concept> = d
        .concepts
        .iter()
        .map(|c| schema::Concept { id: c.id.clone(), name: c.name.clone(), summary: String::new(), prerequisites: c.prerequisites.clone(), lesson_ids: vec![] })
        .collect();
    assert!(schema::validate_concept_dag(&as_schema).is_ok());

    // A longer cycle through three concepts
    let mut cs: Vec<ConceptDraft> = ["c1", "c2", "c3"]
        .iter()
        .map(|id| ConceptDraft { id: id.to_string(), name: id.to_string(), summary: String::new(), prerequisites: vec![] })
        .collect();
    cs[0].prerequisites = vec!["c3".into()];
    cs[1].prerequisites = vec!["c1".into()];
    cs[2].prerequisites = vec!["c2".into()];
    assert_eq!(parse::break_cycles(&mut cs), 1);
    assert!(cs[0].prerequisites.is_empty());
    assert_eq!((cs[1].prerequisites.clone(), cs[2].prerequisites.clone()), (vec!["c1".to_string()], vec!["c2".to_string()]));
}

#[tokio::test]
async fn a_model_made_cyclic_graph_still_makes_a_valid_set() {
    let m = Mock::new(32_768);
    m.answers.lock().insert(
        "concepts",
        json!({"title": "Cells", "concepts": [
            {"id": "c1", "name": "Cell cycle", "prerequisites": ["c3"]},
            {"id": "c2", "name": "Mitosis", "prerequisites": ["c1"]},
            {"id": "c3", "name": "Prophase", "prerequisites": ["c2"]}
        ]})
        .to_string(),
    );
    let made = generate(&m, &class_input(&[(10_000, "cells")]), "m1", "set-8", &no_progress()).await.unwrap();
    let c1 = made.validated.set.concepts.iter().find(|c| c.id == "c1").unwrap();
    assert!(c1.prerequisites.is_empty(), "never stored cyclic");
}

#[test]
fn dangling_references_are_dropped_or_the_part_fails() {
    let concepts: Vec<ConceptDraft> = ["Cell cycle", "Mitosis", "Prophase"]
        .iter()
        .enumerate()
        .map(|(i, n)| ConceptDraft { id: format!("c{}", i + 1), name: n.to_string(), summary: String::new(), prerequisites: vec![] })
        .collect();
    // A lesson naming only unknown concepts is dropped; a concept listed
    // twice stays in its first lesson; an unlisted concept joins the last
    let raw = json!({"lessons": [
        {"title": "Ghost", "concepts": ["c9", "nothing"]},
        {"title": "Start", "objectives": ["- Describe it", ""], "concepts": ["Cell cycle"]},
        {"title": "Next", "concepts": ["c1", "c2"]}
    ]})
    .to_string();
    let o = parse::outline(&raw, &concepts).unwrap();
    assert_eq!(o.iter().map(|l| l.title.as_str()).collect::<Vec<_>>(), vec!["Start", "Next"]);
    assert_eq!(o[0].concept_ids, vec!["c1"]);
    assert_eq!(o[0].objectives, vec!["Describe it"]);
    assert_eq!(o[1].concept_ids, vec!["c2", "c3"]);
    // Nothing usable: the part fails (and is asked for again)
    assert!(parse::outline(r#"{"lessons": [{"title": "Ghost", "concepts": ["c9"]}]}"#, &concepts).is_err());

    // Element references outside the lesson are dropped; the lesson's
    // concepts are the fallback tag
    let lookup = ConceptLookup::new(&concepts);
    let lesson_concepts = vec!["c2".to_string(), "c3".to_string()];
    let scope = LessonScope { lesson_id: "l2", concept_ids: &lesson_concepts, lookup: &lookup };
    let x = parse::explanations(
        r#"{"explanations": [{"title": "A", "concepts": ["c1"], "paragraphs": ["p"]}, {"title": "B", "concepts": ["Prophase", "c9"], "text": "q"}]}"#,
        &scope,
    )
    .unwrap();
    assert_eq!(x[0].concept_ids, vec!["c2", "c3"]);
    assert_eq!(x[1].concept_ids, vec!["c3"]);
    assert_eq!(x[1].data, json!({"paragraphs": ["q"]}));
    assert_eq!((x[0].id.as_str(), x[1].id.as_str()), ("l2-x1", "l2-x2"));
}

#[test]
fn mc_answers_resolve_from_index_letter_or_text_and_bad_ones_are_dropped() {
    let concepts = vec![ConceptDraft { id: "c1".into(), name: "Phases".into(), summary: String::new(), prerequisites: vec![] }];
    let lookup = ConceptLookup::new(&concepts);
    let allowed = vec!["c1".to_string()];
    let raw = r#"Here you go:
```json
{"questions": [
  {"question": "Index", "choices": ["a", "b", "c", "d"], "answer": 2},
  {"question": "Letter", "choices": ["a", "b", "c", "d"], "answer": "D."},
  {"question": "Text", "options": ["Red", "Green"], "correct": "green"},
  {"question": "Out of range", "choices": ["a", "b"], "answer": 5},
  {"question": "No answer", "choices": ["a", "b"]},
  {"question": "Duplicate choices", "choices": ["same", "Same"], "answer": 0},
  {"question": "Empty choice", "choices": ["a", ""], "answer": 0},
  {"question": "Index", "choices": ["x", "y"], "answer": 0},
  {"question": "<b>Tags</b> stay text", "choices": ["<i>a</i>", "b"], "answer": 0, "explanation": "<script>x</script>"},
]}
```"#;
    let qs = parse::questions(raw, "q", &allowed, &lookup, 1).unwrap();
    let got: Vec<(&str, u32)> = qs.iter().map(|q| (q.prompt.as_str(), q.answer_index.unwrap())).collect();
    assert_eq!(got, vec![("Index", 2), ("Letter", 3), ("Text", 1), ("<b>Tags</b> stay text", 0)]);
    // Kept as plain text: the UI renders it as a text node, never HTML
    assert_eq!(qs[3].explanation, "<script>x</script>");
    assert!(qs.iter().all(|q| q.kind == "mc" && q.concept_ids == allowed));
    // Too few usable questions is an error for the part
    assert!(parse::questions(raw, "q", &allowed, &lookup, 5).unwrap_err().contains("Only 4 usable questions"));
    for bad in ["", "no json here", "{", "[1, 2]", r#"{"questions": "none"}"#, "null"] {
        assert!(parse::questions(bad, "q", &allowed, &lookup, 1).is_err(), "{:?}", bad);
        assert!(parse::concepts(bad).is_err(), "{:?}", bad);
    }
    // Deep nesting is rejected, not overflowed
    assert!(parse::concepts(&"[".repeat(5000)).is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// The generated validation profile
// ═══════════════════════════════════════════════════════════════════════════

fn gen_mc(id: &str) -> Value {
    json!({"id": id, "conceptIds": ["c1"], "kind": "mc", "prompt": format!("Question {}", id),
           "choices": ["a", "b", "c"], "answerIndex": 1, "explanation": "why"})
}

fn gen_lesson() -> Value {
    json!({
        "id": "l1", "index": 1, "title": "Cells",
        "objectives": [{"text": "Describe cells", "conceptIds": ["c1"]}],
        "conceptIds": ["c1"],
        "explainers": [
            {"id": "x1", "title": "Cells", "conceptIds": ["c1"], "kind": "text", "data": {"paragraphs": ["Cells divide."], "html": "<b>x</b>"}},
            {"id": "x2", "title": "Balance it", "conceptIds": ["c1"], "kind": "equation_balancer",
             "data": {"equation": {"reactants": [{"formula": "H2"}, {"formula": "O2"}], "products": [{"formula": "H2O"}]}, "answerCoefficients": [2, 1, 2]}}
        ],
        "workedExamples": [
            {"id": "w1", "prompt": "Divide a cell", "conceptIds": ["c1"],
             "steps": [{"text": "Copy DNA", "expression": "2 × 23", "result": "46", "unit": "chromosomes"}],
             "finalAnswer": {"value": 46.0}, "answerText": "Two cells."}
        ],
        "misconceptions": {"items": []},
        "trainer": {"id": "t1", "conceptIds": ["c1"], "generators": [{"template": "mole_mass", "conceptIds": ["c1"]}], "staticItems": []},
        "checkpoint": {"presentCount": 3, "items": [gen_mc("q1"), gen_mc("q2"), gen_mc("q3"), gen_mc("q4"),
            {"id": "q5", "conceptIds": ["c1"], "kind": "numeric", "prompt": "How many?", "answer": 46.0, "unit": "", "tolerance": {"kind": "exact", "value": 0.0}}]}
    })
}

fn gen_set(provenance: &str) -> (schema::LessonSet, HashMap<String, String>) {
    let set: schema::LessonSet = serde_json::from_value(json!({
        "id": "g", "schemaMajor": 1, "title": "Cells", "level": "", "provenance": provenance,
        "sourceRecordingId": "m1", "lessons": ["l1"],
        "concepts": [{"id": "c1", "name": "Cell", "summary": "s", "prerequisites": [], "lessonIds": ["l1"]}],
        "exam": {"items": [gen_mc("e1"), gen_mc("e2"), gen_mc("e3"), gen_mc("e4"), gen_mc("e5")],
                 "questionCount": 5, "passPct": 80, "mix": {"mc": 5, "numeric": 0, "balance": 0, "multistep": 0}}
    }))
    .unwrap();
    let mut lessons = HashMap::new();
    lessons.insert("l1".to_string(), gen_lesson().to_string());
    (set, lessons)
}

#[test]
fn generated_profile_allows_a_figure_free_text_lesson_and_strips_model_numbers() {
    let (set, lessons) = gen_set("generated");
    let v = schema::validate_set(set, vec![], &lessons).unwrap();
    let l = &v.lessons[0];
    // Only the text explainer is kept; the closed-form one is dropped
    assert_eq!(l.explainers.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), vec!["text"]);
    assert_eq!(l.explainers[0].data, json!({"paragraphs": ["Cells divide."]}), "nothing else in data is kept");
    // Worked example: words only, no number as the key
    let w = &l.worked_examples[0];
    assert!(w.final_answer.is_none());
    assert_eq!(w.answer_text.as_deref(), Some("Two cells."));
    assert_eq!((w.steps[0].expression.clone(), w.steps[0].result.clone(), w.steps[0].unit.clone()), (None, None, None));
    // Multiple choice only, no generator-backed drills
    assert_eq!(l.checkpoint.items.len(), 4);
    assert!(l.checkpoint.items.iter().all(|q| q.kind == "mc"));
    assert!(l.trainer.generators.is_empty());

    // The same content claiming an authored provenance fails the authored
    // floors (D4 figure floor first)
    for p in ["imported", "seed"] {
        let (set, lessons) = gen_set(p);
        let e = schema::validate_set(set, vec![], &lessons).unwrap_err();
        assert!(e.contains("figure"), "{}: {}", p, e);
    }
    assert_eq!(Profile::for_provenance("generated"), Profile::Generated);
    assert_eq!(Profile::for_provenance("imported"), Profile::Authored);
}

#[test]
fn generated_profile_rejects_figures_numbers_and_thin_banks() {
    // Figures: no runtime image generation, so a generated set has none
    let (set, lessons) = gen_set("generated");
    let fig = schema::Figure { id: "f".into(), path: "figures/f.png".into(), sha256: "x".into(), width: 1, height: 1, alt: "a".into(), reviewed_by: String::new() };
    assert!(schema::validate_set(set, vec![fig], &lessons).unwrap_err().contains("figures"));

    // A non-MC exam mix
    let (mut set, lessons) = gen_set("generated");
    set.exam.mix = schema::ExamMix { mc: 3, numeric: 2, balance: 0, multistep: 0 };
    assert!(schema::validate_set(set, vec![], &lessons).is_err());

    // Too few checkpoint questions (bank < 4)
    let (set, _) = gen_set("generated");
    let mut lesson = gen_lesson();
    lesson["checkpoint"]["items"] = json!([gen_mc("q1"), gen_mc("q2"), gen_mc("q3")]);
    let mut lessons = HashMap::new();
    lessons.insert("l1".to_string(), lesson.to_string());
    assert!(schema::validate_set(set, vec![], &lessons).unwrap_err().contains("checkpoint bank"));

    // No text left after the closed-form explainer is dropped
    let (set, _) = gen_set("generated");
    let mut lesson = gen_lesson();
    lesson["explainers"] = json!([lesson["explainers"][1].clone()]);
    let mut lessons = HashMap::new();
    lessons.insert("l1".to_string(), lesson.to_string());
    assert!(schema::validate_set(set, vec![], &lessons).unwrap_err().contains("explainers"));
}

#[test]
fn text_explainers_are_cleaned_and_bounded_in_authored_sets_too() {
    let concepts: std::collections::HashSet<String> = ["c1".to_string()].into_iter().collect();
    let mut e: schema::Explainer = serde_json::from_value(json!({
        "id": "x", "title": "  T \u{7}", "conceptIds": ["c1", "c9"], "kind": "text",
        "data": {"paragraphs": ["  one\n two ", 3, {"no": 1}, "", "a", "b", "c", "d", "e", "f"]}
    }))
    .unwrap();
    schema::validate_explainer(&mut e, &concepts, &Default::default(), Profile::Authored).unwrap();
    assert_eq!(e.title, "T");
    assert_eq!(e.concept_ids, vec!["c1"]);
    let ps = e.data["paragraphs"].as_array().unwrap();
    assert_eq!(ps.len(), schema::MAX_TEXT_PARAGRAPHS);
    assert_eq!(ps[0], "one two");
    assert_eq!(ps[1], "3");
    let mut empty: schema::Explainer =
        serde_json::from_value(json!({"id": "y", "title": "T", "kind": "text", "data": {"paragraphs": []}})).unwrap();
    assert!(schema::validate_explainer(&mut empty, &concepts, &Default::default(), Profile::Authored).is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// Storage and the purge
// ═══════════════════════════════════════════════════════════════════════════

struct Fx {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
    root: PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn fx() -> Fx {
    let dir = std::env::temp_dir().join(format!("nf-make-lesson-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db = DatabaseManager::new(&data.join("nofriction_meetings.db")).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Biology 101").await.unwrap();
    sqlx::query("UPDATE meetings SET recording_kind = 'class' WHERE id = 'm1'").execute(db.pool()).await.unwrap();
    let env = RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() };
    let root = dir.join("lessons");
    Fx { db: Arc::new(db), dir, env, root }
}

async fn line_at(f: &Fx, secs: i64, text: &str) -> i64 {
    let started: String = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = 'm1'").fetch_one(f.db.pool()).await.unwrap();
    let t0 = chrono::DateTime::parse_from_rfc3339(&started).unwrap().with_timezone(&chrono::Utc);
    f.db.add_transcript_at("m1", text, None, true, 0.9, t0 + chrono::Duration::seconds(secs)).await.unwrap()
}

async fn make(f: &Fx, set_id: &str) -> Result<String, String> {
    let input = study::load_input(f.db.pool(), "m1").await?;
    let made = generate(&Mock::new(32_768), &input, "m1", set_id, &no_progress()).await.map_err(String::from)?;
    save_generated(f.db.pool(), &f.root, "m1", &input.fingerprint(), &made.validated).await
}

async fn generated_rows(f: &Fx) -> Vec<(String, String)> {
    sqlx::query_as("SELECT id, dir_path FROM lesson_sets WHERE source_recording_id = 'm1' AND provenance = 'generated'")
        .fetch_all(f.db.pool())
        .await
        .unwrap()
}

fn utf16_span(hay: &str, needle: &str) -> (usize, usize) {
    let b = hay.find(needle).unwrap();
    let s = hay[..b].encode_utf16().count();
    (s, s + needle.encode_utf16().count())
}

#[tokio::test]
async fn generate_validate_store_load_purge_round_trip() {
    let f = fx().await;
    let text = "the cell cycle has interphase and mitosis";
    let id = line_at(&f, 10, text).await;
    line_at(&f, 20, "mitosis starts with prophase").await;

    // Make → stored as an ordinary catalog entry with its files on disk
    let set_id = make(&f, "gen-a").await.unwrap();
    assert_eq!(set_id, "gen-a");
    let rows = generated_rows(&f).await;
    assert_eq!(rows.len(), 1);
    let dir = PathBuf::from(&rows[0].1);
    assert!(dir.starts_with(&f.root) && dir.join("lessonset.json").is_file() && dir.join("lessons/l1.json").is_file());
    let fp: String = sqlx::query_scalar("SELECT transcript_fingerprint FROM lesson_sets WHERE id = 'gen-a'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(fp, study::load_input(f.db.pool(), "m1").await.unwrap().fingerprint());
    let content: String = sqlx::query_scalar("SELECT content_fingerprint FROM lesson_sets WHERE id = 'gen-a'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(content.len(), 64);

    // Load it back the way the player does, and it re-validates to itself
    let set = crate::lessons::commands::load_set(&dir).unwrap();
    assert_eq!((set.provenance.as_str(), set.source_recording_id.as_deref()), ("generated", Some("m1")));
    let mut lesson_json = HashMap::new();
    for lid in &set.lessons {
        let l = crate::lessons::commands::load_lesson(&dir, lid).unwrap();
        assert!(l.explainers.iter().all(|e| e.kind == "text"));
        lesson_json.insert(lid.clone(), serde_json::to_string(&l).unwrap());
    }
    let again = schema::validate_set(set.clone(), vec![], &lesson_json).unwrap();
    assert_eq!(again.set, set, "stored data is already clean");
    // The stored JSON never carries a worked-example number
    let raw = std::fs::read_to_string(dir.join("lessons/l1.json")).unwrap();
    assert!(!raw.contains("finalAnswer") && !raw.contains("42"), "{}", raw);

    // Learner progress on it, then Make again: the old set (and its progress) is replaced
    sqlx::query("INSERT INTO mastery_records (lesson_set_id, concept_id, updated_at) VALUES ('gen-a', 'c1', 'now')")
        .execute(f.db.pool())
        .await
        .unwrap();
    make(&f, "gen-b").await.unwrap();
    let rows = generated_rows(&f).await;
    assert_eq!(rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["gen-b"]);
    assert!(!dir.exists(), "the previous set's files are gone");
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mastery_records WHERE lesson_set_id = 'gen-a'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(left, 0);
    let dir_b = PathBuf::from(&rows[0].1);
    assert!(dir_b.is_dir());

    // An imported or seed set made from nothing is never touched by it
    sqlx::query(
        "INSERT INTO lesson_sets (id, title, provenance, source_recording_id, content_fingerprint, schema_major, dir_path, imported_at) \
         VALUES ('imp', 'I', 'imported', 'm1', 'fp', 1, '', 'now')",
    )
    .execute(f.db.pool())
    .await
    .unwrap();

    // Strike words → the generated set is deleted (rows and files), the imported one stays
    let (s, e) = utf16_span(text, "interphase");
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    assert!(generated_rows(&f).await.is_empty(), "strike purges the generated set");
    assert!(!dir_b.exists(), "and its files");
    let imp: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lesson_sets WHERE id = 'imp'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(imp, 1);
}

#[tokio::test]
async fn line_delete_and_whole_recording_delete_purge_it_too() {
    let f = fx().await;
    let id = line_at(&f, 10, "osmosis moves water").await;
    line_at(&f, 20, "diffusion is passive").await;
    make(&f, "gen-c").await.unwrap();
    // The confirmation says the lesson goes too
    let target = WordTarget { meeting_id: "m1".into(), transcript_id: id, start: 0, end: 0, expected_text: None, whole_line: true };
    let preview = rd::preview_words(f.db.pool(), &f.env, &target).await.unwrap();
    assert!(preview.iter().any(|i| i.contains("lesson made from this recording")), "{:?}", preview);
    // A whole-line Delete: kept through the undo window, deleted at commit
    let p = rd::request_delete_words(f.db.pool(), &f.env, &target).await.unwrap();
    assert_eq!(generated_rows(&f).await.len(), 1, "the undo window keeps it");
    // …and Make lesson refuses to save while that Delete is pending
    let e = make(&f, "gen-d").await.unwrap_err();
    assert_eq!(e, TRANSCRIPT_CHANGED);
    rd::commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert!(generated_rows(&f).await.is_empty(), "line delete purges it");

    // Deleting the whole recording cascades to its lesson
    make(&f, "gen-e").await.unwrap();
    let dir = PathBuf::from(&generated_rows(&f).await[0].1);
    f.db.delete_meeting("m1").await.unwrap();
    assert!(generated_rows(&f).await.is_empty());
    assert!(!dir.exists());
}

#[tokio::test]
async fn a_lesson_made_before_an_edit_is_never_saved_after_it() {
    let f = fx().await;
    let text = "the Krebs cycle runs in the matrix";
    let id = line_at(&f, 10, text).await;
    let input = study::load_input(f.db.pool(), "m1").await.unwrap();
    let made = generate(&Mock::new(32_768), &input, "m1", "gen-f", &no_progress()).await.unwrap();
    // Meanwhile the student strikes words
    let (s, e) = utf16_span(text, "Krebs cycle");
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    let err = save_generated(f.db.pool(), &f.root, "m1", &input.fingerprint(), &made.validated).await.unwrap_err();
    assert_eq!(err, TRANSCRIPT_CHANGED);
    assert!(generated_rows(&f).await.is_empty());
    // Nothing is left on disk either (no staging dir, no set dir)
    let leftovers = std::fs::read_dir(&f.root).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftovers, 0);
    // A set made for another recording is refused
    let err = save_generated(f.db.pool(), &f.root, "m2", &input.fingerprint(), &made.validated).await.unwrap_err();
    assert!(err.contains("wasn't made from this recording"));
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
    let input = study::load_input(f.db.pool(), "m1").await.unwrap();
    let m = Mock::new(32_768);
    generate(&m, &input, "m1", "gen-g", &no_progress()).await.unwrap();
    for t in m.texts() {
        assert!(!t.contains("Zanzibar") && !t.contains("light"), "{}", t);
        assert!(!t.contains("strickenid"), "marker ids never leave");
        assert!(t.contains("[stricken from the record]"));
    }
}
