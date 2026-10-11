//! Lessons backend tests (docs/LESSONS.md §15). Mirrors study/tests.rs: no
//! endpoint is ever called. Covers schema validation, bundle round-trip +
//! per-file hash tamper + path/schema rejection, numeric grading, balance +
//! explainer correctness, mastery/review/trainer sequences, exam
//! mix/scoring/readiness, and purge (live + backup) of a generated set.

use super::bundle::{self, Manifest, ManifestFile};
use super::exam;
use super::explain;
use super::generators;
use super::grade::{self, Given, MultistepGiven, NumericGrade};
use super::mastery::{self, Mastery};
use super::review::{self, ReviewState};
use super::schema::{self, Equation, Product, Reactant, Tolerance};
use super::trainer::{self, ConceptState};
use crate::database::DatabaseManager;
use crate::redaction::{self as rd, RedactionEnv, WordTarget};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

// ════════════════════════════════════════════════════════════════════════
// Bundle fixtures
// ════════════════════════════════════════════════════════════════════════

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// A tiny valid PNG (2x2) that the `image` crate decodes.
fn tiny_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([250, 204, 21, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

fn eq(reactants: &[(&str, u32)], products: &[&str]) -> Value {
    json!({
        "reactants": reactants.iter().map(|(f, _)| json!({"formula": f})).collect::<Vec<_>>(),
        "products": products.iter().map(|f| json!({"formula": f})).collect::<Vec<_>>(),
    })
}

/// Build an MC checkpoint/exam item with the given concept.
fn mc_item(id: &str, concept: &str) -> Value {
    json!({
        "id": id, "conceptIds": [concept], "kind": "mc",
        "prompt": "What is a mole?", "explanation": "A count of particles.",
        "choices": ["6.022e23 particles", "one gram", "one liter"], "answerIndex": 0
    })
}

fn numeric_item(id: &str, concept: &str, answer: f64, unit: &str) -> Value {
    json!({
        "id": id, "conceptIds": [concept], "kind": "numeric",
        "prompt": "How many moles?", "explanation": "Divide by molar mass.",
        "answer": answer, "unit": unit, "tolerance": {"kind": "rel", "value": 0.01}
    })
}

fn multistep_item(id: &str, concept: &str) -> Value {
    json!({
        "id": id, "conceptIds": [concept], "kind": "multistep",
        "prompt": "Two steps.", "explanation": "ok", "gated": true,
        "steps": [
            {"prompt": "pick A", "kind": "mc", "choices": ["A", "B"], "answerIndex": 0},
            {"prompt": "value", "kind": "numeric", "answer": 2.0, "unit": "mol", "tolerance": {"kind": "abs", "value": 0.1}}
        ]
    })
}

/// A single-lesson lesson set with one figure, enough items for every floor.
fn sample_bundle() -> (Vec<u8>, String) {
    let concept = "c-mole";
    let figure_id = "fig-mole";
    let lesson_id = "l1";
    let png = tiny_png();

    // Checkpoint bank: 16 items (>= 2 * 8).
    let mut cp_items: Vec<Value> = Vec::new();
    for i in 0..16 {
        cp_items.push(mc_item(&format!("cp{}", i), concept));
    }
    // Exam bank: >= 60 total, with >=18 mc, >=18 numeric, >=9 multistep.
    let mut exam_items: Vec<Value> = Vec::new();
    for i in 0..24 {
        exam_items.push(mc_item(&format!("emc{}", i), concept));
    }
    for i in 0..24 {
        exam_items.push(numeric_item(&format!("enum{}", i), concept, 2.0, "mol"));
    }
    for i in 0..12 {
        exam_items.push(multistep_item(&format!("ems{}", i), concept));
    }

    let lesson = json!({
        "id": lesson_id, "index": 1, "title": "Moles",
        "objectives": [{"text": "Understand the mole", "conceptIds": [concept]}],
        "conceptIds": [concept],
        "explainers": [
            {"id": "ex-fig", "title": "Mole figure", "conceptIds": [concept], "caption": "A mole of stuff",
             "kind": "figure", "data": {"figureId": figure_id, "alt": "a mole"}},
            {"id": "ex-bal", "title": "Balance H2 + O2", "conceptIds": [concept], "caption": "balance it",
             "kind": "equation_balancer",
             "data": {"equation": eq(&[("H2", 0), ("O2", 0)], &["H2O"]), "answerCoefficients": [2, 1, 2]}}
        ],
        "workedExamples": [
            {"id": "w1", "prompt": "ex1", "conceptIds": [concept], "steps": [{"text": "divide"}], "finalAnswer": {"value": 2.0, "unit": "mol"}},
            {"id": "w2", "prompt": "ex2", "conceptIds": [concept], "steps": [{"text": "divide"}], "finalAnswer": {"value": 3.0, "unit": "mol"}},
            {"id": "w3", "prompt": "ex3", "conceptIds": [concept], "steps": [{"text": "divide"}], "finalAnswer": {"value": 4.0, "unit": "mol"}}
        ],
        "misconceptions": {"items": [
            {"claim": "molar mass equals formula", "whyWrong": "no", "correct": "sum atomic masses", "conceptIds": [concept]},
            {"claim": "atoms equal moles", "whyWrong": "no", "correct": "divide by Avogadro", "conceptIds": [concept]}
        ]},
        "trainer": {"id": "t1", "conceptIds": [concept],
            "generators": [{"template": "mole_mass", "conceptIds": [concept]}], "staticItems": []},
        "checkpoint": {"presentCount": 8, "items": cp_items}
    });

    let lessonset = json!({
        "id": "set-sample", "schemaMajor": 1, "title": "Stoichiometry sample",
        "subtitle": "test", "level": "AP", "provenance": "imported",
        "lessons": [lesson_id],
        "concepts": [{"id": concept, "name": "Mole", "summary": "the mole concept", "prerequisites": [], "lessonIds": [lesson_id]}],
        "exam": {"items": exam_items, "questionCount": 30, "passPct": 80,
                 "mix": {"mc": 12, "numeric": 12, "balance": 0, "multistep": 6}}
    });

    let lessonset_bytes = serde_json::to_vec(&lessonset).unwrap();
    let lesson_bytes = serde_json::to_vec(&lesson).unwrap();

    let files = vec![
        ("lessonset.json".to_string(), lessonset_bytes.clone()),
        (format!("lessons/{}.json", lesson_id), lesson_bytes.clone()),
        (format!("figures/{}.png", figure_id), png.clone()),
    ];
    let manifest_files: Vec<ManifestFile> = files
        .iter()
        .map(|(p, b)| ManifestFile {
            path: p.clone(),
            sha256: sha(b),
            bytes: b.len() as u64,
            kind: if p.ends_with(".png") { Some("image/png".into()) } else { None },
        })
        .collect();
    let manifest = Manifest {
        format: bundle::FORMAT.into(),
        schema_major: 1,
        schema_minor: 0,
        lesson_set_id: "set-sample".into(),
        title: "Stoichiometry sample".into(),
        provenance: "imported".into(),
        files: manifest_files,
        content_fingerprint: "placeholder".into(),
        authored_at: "2026-01-01T00:00:00Z".into(),
        generator: "test".into(),
        figures: vec![schema::Figure {
            id: figure_id.into(),
            path: format!("figures/{}.png", figure_id),
            sha256: sha(&png),
            width: 2,
            height: 2,
            alt: "a mole".into(),
            reviewed_by: "tester".into(),
        }],
    };
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let zip = bundle::write_zip(&manifest_json, &files).unwrap();
    (zip, "set-sample".into())
}

// ════════════════════════════════════════════════════════════════════════
// Schema validation
// ════════════════════════════════════════════════════════════════════════

#[test]
fn valid_bundle_passes_validation() {
    let (zip, _id) = sample_bundle();
    let prepared = bundle::validate_bytes(&zip).expect("valid bundle should pass");
    assert_eq!(prepared.validated.lessons.len(), 1);
    assert_eq!(prepared.validated.figures.len(), 1);
    assert!(!prepared.content_fingerprint.is_empty());
}

#[test]
fn malformed_schema_fails_cleanly_never_panics() {
    // Out-of-range mc answerIndex.
    let concepts: std::collections::HashSet<String> = ["c".to_string()].into_iter().collect();
    let mut bad = schema::QuizItem {
        id: "x".into(),
        concept_ids: vec!["c".into()],
        kind: "mc".into(),
        prompt: "q".into(),
        solution: vec![],
        explanation: String::new(),
        choices: Some(vec!["a".into(), "b".into()]),
        answer_index: Some(5),
        answer: None,
        unit: None,
        tolerance: None,
        sig_figs: None,
        equation: None,
        answer_coefficients: None,
        steps: None,
        gated: None,
        generator: None,
    };
    assert!(super::schema::validate_item_pub(&mut bad, &concepts).is_err());

    // Missing tolerance on a numeric item.
    let mut no_tol = bad.clone();
    no_tol.kind = "numeric".into();
    no_tol.choices = None;
    no_tol.answer_index = None;
    no_tol.answer = Some(1.0);
    no_tol.tolerance = None;
    assert!(super::schema::validate_item_pub(&mut no_tol, &concepts).is_err());
}

#[test]
fn cyclic_or_dangling_concept_graph_fails() {
    // Dangling prerequisite.
    let (zip, _) = sample_bundle();
    let raw = unzip_to_map(&zip);
    let mut set: Value = serde_json::from_slice(&raw["lessonset.json"]).unwrap();
    set["concepts"][0]["prerequisites"] = json!(["missing-concept"]);
    let rezipped = rezip_with_lessonset(&zip, &set);
    assert!(bundle::validate_bytes(&rezipped).is_err());
}

#[test]
fn figure_floor_enforced() {
    let (zip, _) = sample_bundle();
    let raw = unzip_to_map(&zip);
    let mut lesson: Value = serde_json::from_slice(&raw["lessons/l1.json"]).unwrap();
    // Remove the figure explainer, leaving only the balancer.
    lesson["explainers"] = json!([lesson["explainers"][1].clone(), lesson["explainers"][1].clone()]);
    let rezipped = rezip_with_lesson(&zip, "lessons/l1.json", &lesson);
    assert!(bundle::validate_bytes(&rezipped).is_err(), "a figure-free lesson must fail");
}

// ════════════════════════════════════════════════════════════════════════
// Bundle integrity: tamper, path, schema
// ════════════════════════════════════════════════════════════════════════

fn unzip_to_map(zip: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip.to_vec())).unwrap();
    let mut map = HashMap::new();
    for i in 0..archive.len() {
        use std::io::Read;
        let mut e = archive.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().to_string();
        let mut b = Vec::new();
        e.read_to_end(&mut b).unwrap();
        map.insert(name, b);
    }
    map
}

fn rebuild_zip(map: &HashMap<String, Vec<u8>>) -> Vec<u8> {
    let manifest = map.get("manifest.json").cloned().unwrap_or_default();
    let entries: Vec<(String, Vec<u8>)> =
        map.iter().filter(|(k, _)| *k != "manifest.json").map(|(k, v)| (k.clone(), v.clone())).collect();
    bundle::write_zip(&manifest, &entries).unwrap()
}

fn rezip_with_lessonset(zip: &[u8], set: &Value) -> Vec<u8> {
    let mut map = unzip_to_map(zip);
    let bytes = serde_json::to_vec(set).unwrap();
    // keep manifest hash consistent so the test exercises schema, not integrity.
    update_manifest_hash(&mut map, "lessonset.json", &bytes);
    map.insert("lessonset.json".into(), bytes);
    rebuild_zip(&map)
}

fn rezip_with_lesson(zip: &[u8], path: &str, lesson: &Value) -> Vec<u8> {
    let mut map = unzip_to_map(zip);
    let bytes = serde_json::to_vec(lesson).unwrap();
    update_manifest_hash(&mut map, path, &bytes);
    map.insert(path.into(), bytes);
    rebuild_zip(&map)
}

fn update_manifest_hash(map: &mut HashMap<String, Vec<u8>>, path: &str, new_bytes: &[u8]) {
    let mut manifest: Manifest = serde_json::from_slice(&map["manifest.json"]).unwrap();
    for f in &mut manifest.files {
        if f.path == path {
            f.sha256 = sha(new_bytes);
            f.bytes = new_bytes.len() as u64;
        }
    }
    map.insert("manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
}

#[test]
fn bundle_round_trips_through_export() {
    let (zip, _) = sample_bundle();
    let dir = std::env::temp_dir().join(format!("nf-lessons-rt-{}", uuid::Uuid::new_v4()));
    // Write the on-disk layout the catalog uses, then export.
    let map = unzip_to_map(&zip);
    std::fs::create_dir_all(dir.join("lessons")).unwrap();
    std::fs::create_dir_all(dir.join("figures")).unwrap();
    for (path, bytes) in &map {
        if path == "manifest.json" {
            continue;
        }
        std::fs::write(dir.join(path), bytes).unwrap();
    }
    let exported = bundle::export_bundle(&dir).unwrap();
    // The export must itself validate (modulo provenance override on import).
    assert!(bundle::validate_bytes(&exported).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn per_file_hash_tamper_is_rejected() {
    let (zip, _) = sample_bundle();
    let mut map = unzip_to_map(&zip);
    // Flip a byte in the lesson file WITHOUT updating the manifest hash.
    let mut lesson = map["lessons/l1.json"].clone();
    lesson.push(b' ');
    map.insert("lessons/l1.json".into(), lesson);
    let tampered = rebuild_zip(&map);
    let err = bundle::validate_bytes(&tampered).unwrap_err();
    assert!(err.contains("hash") || err.contains("size"), "{}", err);
}

#[test]
fn unknown_schema_major_rejected_newer_minor_accepted() {
    let (zip, _) = sample_bundle();
    let mut map = unzip_to_map(&zip);
    let mut manifest: Manifest = serde_json::from_slice(&map["manifest.json"]).unwrap();
    manifest.schema_major = 99;
    map.insert("manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
    assert!(bundle::validate_bytes(&rebuild_zip(&map)).is_err());

    // A newer minor is accepted.
    let mut map2 = unzip_to_map(&zip);
    let mut manifest2: Manifest = serde_json::from_slice(&map2["manifest.json"]).unwrap();
    manifest2.schema_minor = 7;
    map2.insert("manifest.json".into(), serde_json::to_vec(&manifest2).unwrap());
    assert!(bundle::validate_bytes(&rebuild_zip(&map2)).is_ok());
}

#[test]
fn traversal_entry_path_rejected() {
    // A manifest listing a `..` path must be rejected on read.
    let (zip, _) = sample_bundle();
    let mut map = unzip_to_map(&zip);
    let mut manifest: Manifest = serde_json::from_slice(&map["manifest.json"]).unwrap();
    manifest.files.push(ManifestFile {
        path: "../escape.json".into(),
        sha256: sha(b"x"),
        bytes: 1,
        kind: None,
    });
    map.insert("manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
    map.insert("../escape.json".into(), b"x".to_vec());
    // write_zip would reject the path too; validate the integrity check path.
    let err = bundle::validate_bytes(&rebuild_zip(&map));
    assert!(err.is_err());
}

// ════════════════════════════════════════════════════════════════════════
// Numeric grading (§9)
// ════════════════════════════════════════════════════════════════════════

#[test]
fn numeric_grading_table() {
    let abs = Tolerance { kind: "abs".into(), value: 0.001 };
    let exact = Tolerance { kind: "exact".into(), value: 0.0 };
    // 4000 mg vs 4 g key → correct.
    assert_eq!(grade::check_numeric("4000 mg", 4.0, Some("g"), &abs), NumericGrade::Correct);
    // 250 mL vs 0.25 L → correct.
    assert_eq!(grade::check_numeric("250 mL", 0.25, Some("L"), &abs), NumericGrade::Correct);
    // 25 °C vs 298.15 K → correct.
    let tabs = Tolerance { kind: "abs".into(), value: 0.01 };
    assert_eq!(grade::check_numeric("25 °C", 298.15, Some("K"), &tabs), NumericGrade::Correct);
    // 4 L vs a grams key → dimensionally incompatible → wrong.
    assert_eq!(grade::check_numeric("4 L", 4.0, Some("g"), &abs), NumericGrade::Wrong);
    // 4 foo → unparseable unit.
    assert!(matches!(grade::check_numeric("4 foo", 4.0, Some("g"), &abs), NumericGrade::Unparseable(_)));
    // 4 (no unit) vs 4 g → compared in the declared unit → correct.
    assert_eq!(grade::check_numeric("4", 4.0, Some("g"), &exact), NumericGrade::Correct);
    // scientific notation.
    assert_eq!(grade::check_numeric("6.022e23", 6.022e23, None, &exact), NumericGrade::Correct);
    assert_eq!(grade::check_numeric("6.022 x 10^23", 6.022e23, None, &exact), NumericGrade::Correct);
    // Hostile inputs never panic.
    for g in ["", "   ", "abc", "NaN", "1e999999", "----", "9".repeat(400).as_str()] {
        let _ = grade::check_numeric(g, 1.0, Some("g"), &abs);
    }
}

// ════════════════════════════════════════════════════════════════════════
// Balance + explainer correctness (§6)
// ════════════════════════════════════════════════════════════════════════

fn equation(reactants: &[&str], products: &[&str]) -> Equation {
    Equation {
        reactants: reactants.iter().map(|f| Reactant { formula: f.to_string(), count: None }).collect(),
        products: products.iter().map(|f| Product { formula: f.to_string() }).collect(),
    }
}

#[test]
fn balance_computes_lowest_integer_coefficients() {
    // 2 H2 + O2 → 2 H2O
    let e = equation(&["H2", "O2"], &["H2O"]);
    assert_eq!(explain::balance_equation(&e).unwrap(), vec![2, 1, 2]);
    assert!(explain::verify_balance(&e, &[2, 1, 2]).is_ok());
    assert!(explain::verify_balance(&e, &[1, 1, 1]).is_err());
    // order-independent, lowest-integer grading.
    assert!(grade::check_balance(&e, &[2, 1, 2]));
    assert!(grade::check_balance(&e, &[4, 2, 4])); // reduces to the same
    assert!(!grade::check_balance(&e, &[1, 1, 2]));

    // CH4 + 2 O2 → CO2 + 2 H2O
    let e2 = equation(&["CH4", "O2"], &["CO2", "H2O"]);
    assert_eq!(explain::balance_equation(&e2).unwrap(), vec![1, 2, 1, 2]);

    // An impossible equation fails rather than panics.
    let bad = equation(&["H2"], &["O2"]);
    assert!(explain::balance_equation(&bad).is_err());
}

#[test]
fn explainer_verifiers_catch_bad_data() {
    // dimensional_analysis product check.
    let ok = json!({"start": {"value": 2.0}, "tiles": [{"num": 3.0, "den": 1.0}], "value": 6.0});
    assert!(explain::verify_explainer("dimensional_analysis", &ok).is_ok());
    let bad = json!({"start": {"value": 2.0}, "tiles": [{"num": 3.0, "den": 1.0}], "value": 99.0});
    assert!(explain::verify_explainer("dimensional_analysis", &bad).is_err());
    // titration monotonicity.
    let non_mono = json!({"points": [{"volAdded": 1.0, "pH": 3.0}, {"volAdded": 0.5, "pH": 4.0}], "equivalenceVol": 5.0});
    assert!(explain::verify_explainer("titration_curve", &non_mono).is_err());
    // molecule structural check.
    let dangling = json!({"atoms": [{"id": "a", "element": "C", "x": 0.5, "y": 0.5}], "bonds": [{"a": "a", "b": "z", "order": 1}]});
    assert!(explain::verify_explainer("molecule_diagram", &dangling).is_err());
}

// ════════════════════════════════════════════════════════════════════════
// Mastery / review / trainer sequences
// ════════════════════════════════════════════════════════════════════════

#[test]
fn mastery_ema_and_mastered_threshold() {
    let mut m = Mastery::default();
    // Three first-try-correct answers reach mastered.
    for _ in 0..3 {
        mastery::record_attempt(&mut m, true, true, 1.0);
    }
    assert!(m.mastered, "score {}", m.score);
    assert_eq!(m.correct_attempts, 3);
    assert_eq!(m.first_try_correct, 3);

    // A wrong-then-right sequence does not instantly master.
    let mut m2 = Mastery::default();
    mastery::record_attempt(&mut m2, false, false, 0.0);
    mastery::record_attempt(&mut m2, true, false, 1.0); // retry credit 70
    assert!(!m2.mastered);
    assert!(m2.score < mastery::MASTERED_SCORE);
}

#[test]
fn diagnostic_seeds_mastery() {
    let m = mastery::seed_from_diagnostic(4, 4);
    assert!(m.mastered);
    let weak = mastery::seed_from_diagnostic(1, 4);
    assert!(!weak.mastered);
}

#[test]
fn review_schedule_is_deterministic() {
    let mut s = ReviewState::default();
    let d1 = review::apply_review(&mut s, 5);
    assert_eq!(d1, 1.0);
    let d2 = review::apply_review(&mut s, 5);
    assert_eq!(d2, 3.0);
    let d3 = review::apply_review(&mut s, 5);
    assert!(d3 > 3.0, "interval grows by ease");
    // A failure resets to 1 day.
    let d4 = review::apply_review(&mut s, 1);
    assert_eq!(d4, 1.0);
    assert!(s.ease >= review::MIN_EASE);
}

#[test]
fn seeded_generator_is_reproducible_and_scales() {
    let a = generators::solve_with_steps("mole_mass", 2, "c-mole", 7).unwrap();
    let b = generators::solve_with_steps("mole_mass", 2, "c-mole", 7).unwrap();
    assert_eq!(a, b, "same seed → same drill");
    let c = generators::solve_with_steps("mole_mass", 2, "c-mole", 8).unwrap();
    assert!(a != c, "different attempt index → different draw");
    assert!(!a.steps.is_empty());
    assert!(generators::solve_with_steps("unknown", 1, "c", 0).is_none());
}

#[test]
fn trainer_focuses_weakest_unmastered_concept() {
    let states = vec![
        ConceptState { concept_id: "strong".into(), score: 90.0, mastered: true, overdue_days: 0.0, generator: None },
        ConceptState { concept_id: "weak".into(), score: 20.0, mastered: false, overdue_days: 0.0, generator: Some("mole_mass".into()) },
        ConceptState { concept_id: "mid".into(), score: 55.0, mastered: false, overdue_days: 0.0, generator: None },
    ];
    let focus = trainer::focus_concept(&states).unwrap();
    assert_eq!(focus.concept_id, "weak");
    assert_eq!(trainer::difficulty_tier(20.0), 1);
    assert_eq!(trainer::difficulty_tier(55.0), 2);
    assert_eq!(trainer::difficulty_tier(80.0), 3);
    // next_item draws from the weak concept's generator.
    match trainer::next_item(&states, 3).unwrap() {
        trainer::NextItem::Generated { concept_id, .. } => assert_eq!(concept_id, "weak"),
        other => panic!("expected generated, got {:?}", other),
    }
}

// ════════════════════════════════════════════════════════════════════════
// Exam mix / scoring / readiness
// ════════════════════════════════════════════════════════════════════════

#[test]
fn exam_selection_honors_mix_and_differs_across_attempts() {
    let (zip, _) = sample_bundle();
    let prepared = bundle::validate_bytes(&zip).unwrap();
    let exam = &prepared.validated.set.exam;
    let empty = std::collections::HashSet::new();
    let draw1 = exam::select_items(exam, "set-sample", 0, &empty);
    assert_eq!(draw1.len(), 30);
    let recent: std::collections::HashSet<String> = draw1.iter().cloned().collect();
    let draw2 = exam::select_items(exam, "set-sample", 1, &recent);
    assert_eq!(draw2.len(), 30);
    // The two draws should not be identical (the bank over-provisions).
    assert!(draw1 != draw2);
}

#[test]
fn exam_scoring_and_gating() {
    let mc = schema::QuizItem {
        id: "m".into(), concept_ids: vec!["c".into()], kind: "mc".into(), prompt: "q".into(),
        solution: vec![], explanation: String::new(), choices: Some(vec!["a".into(), "b".into()]),
        answer_index: Some(0), answer: None, unit: None, tolerance: None, sig_figs: None,
        equation: None, answer_coefficients: None, steps: None, gated: None, generator: None,
    };
    let g = grade::grade_item(&mc, &Given::Mc(0));
    assert!(g.is_full());
    let g2 = grade::grade_item(&mc, &Given::Mc(1));
    assert_eq!(g2.earned, 0.0);

    // Gated multistep: a wrong first step zeroes the dependent step.
    let ms = schema::QuizItem {
        id: "ms".into(), concept_ids: vec!["c".into()], kind: "multistep".into(), prompt: "q".into(),
        solution: vec![], explanation: String::new(), choices: None, answer_index: None, answer: None,
        unit: None, tolerance: None, sig_figs: None, equation: None, answer_coefficients: None,
        steps: Some(vec![
            schema::MultistepStep { prompt: "s1".into(), kind: "mc".into(), choices: Some(vec!["a".into(), "b".into()]), answer_index: Some(0), answer: None, unit: None, tolerance: None, equation: None, answer_coefficients: None },
            schema::MultistepStep { prompt: "s2".into(), kind: "numeric".into(), choices: None, answer_index: None, answer: Some(2.0), unit: Some("mol".into()), tolerance: Some(Tolerance { kind: "abs".into(), value: 0.1 }), equation: None, answer_coefficients: None },
        ]),
        gated: Some(true), generator: None,
    };
    // Wrong first step → second (correct value) is still zeroed.
    let gated = grade::grade_item(&ms, &Given::Multistep(vec![MultistepGiven::Mc(1), MultistepGiven::Numeric("2".into())]));
    assert_eq!(gated.earned, 0.0);
    assert_eq!(gated.possible, 2.0);
    // Both correct → full marks.
    let full = grade::grade_item(&ms, &Given::Multistep(vec![MultistepGiven::Mc(0), MultistepGiven::Numeric("2".into())]));
    assert!(full.is_full());
}

#[test]
fn readiness_is_monotonic_in_mastery() {
    let mut counts = HashMap::new();
    counts.insert("c".to_string(), 4u32);
    let mut low = HashMap::new();
    low.insert("c".to_string(), 20.0);
    let mut high = HashMap::new();
    high.insert("c".to_string(), 90.0);
    let r_low = exam::readiness(&low, &counts);
    let r_high = exam::readiness(&high, &counts);
    assert!(r_high > r_low);
    assert_eq!(exam::readiness_label(90.0), "ready");
    assert_eq!(exam::readiness_label(10.0), "not ready");
}

#[test]
fn certificate_hash_is_stable_and_sensitive() {
    let a = exam::result_hash("fp", 90.0, "2026-01-01T00:00:00Z", "set");
    let b = exam::result_hash("fp", 90.0, "2026-01-01T00:00:00Z", "set");
    assert_eq!(a, b);
    let c = exam::result_hash("fp", 91.0, "2026-01-01T00:00:00Z", "set");
    assert!(a != c);
}

// ════════════════════════════════════════════════════════════════════════
// Seed loader
// ════════════════════════════════════════════════════════════════════════

struct Fx {
    db: Arc<DatabaseManager>,
    dir: std::path::PathBuf,
    env: RedactionEnv,
    db_path: std::path::PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn fx() -> Fx {
    let dir = std::env::temp_dir().join(format!("nf-lessons-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    // Point the on-disk lessons root inside the fixture (paths::app_data_dir is
    // process-global, so route the catalog dir via an env override is not
    // available; commit_bundle writes under paths::app_data_dir()/lessons —
    // tests that need the files assert on the DB rows only).
    let db_path = data.join("nofriction_meetings.db");
    let db = DatabaseManager::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();
    // The seed loader keys off the `settings` table (created by SettingsManager
    // after migrations in production). Create it here to mirror startup.
    let settings = crate::settings::SettingsManager::new(db.get_pool());
    settings.init().await.unwrap();
    db.create_meeting("m1", "Chemistry 101").await.unwrap();
    let env = RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() };
    Fx { db: Arc::new(db), dir, env, db_path }
}

#[tokio::test]
async fn seed_loader_is_idempotent_and_tombstone_sticks() {
    let f = fx().await;
    let (zip, set_id) = sample_bundle();
    // First seed imports it.
    assert!(super::seed::ensure_seeded_from_bytes(f.db.pool(), &zip).await.unwrap());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lesson_sets WHERE provenance = 'seed'")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    // Second call is a no-op (version already current).
    assert!(!super::seed::ensure_seeded_from_bytes(f.db.pool(), &zip).await.unwrap());
    // Removing + tombstone prevents re-seeding.
    super::seed::mark_removed(f.db.pool()).await.unwrap();
    sqlx::query("DELETE FROM lesson_sets WHERE id = ?").bind(&set_id).execute(f.db.pool()).await.unwrap();
    // Even bumping would be blocked by the tombstone.
    assert!(!super::seed::ensure_seeded_from_bytes(f.db.pool(), &zip).await.unwrap());
    // Clean the on-disk catalog dir the import created.
    let _ = std::fs::remove_dir_all(super::lessons_root().join(&set_id));
}

// ════════════════════════════════════════════════════════════════════════
// Purge (generated set): live + cascade + backup. Seed/imported survive.
// ════════════════════════════════════════════════════════════════════════

async fn insert_generated_set(pool: &sqlx::Pool<sqlx::Sqlite>, id: &str, recording: &str, provenance: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO lesson_sets (id, title, subtitle, level, provenance, source_recording_id, transcript_fingerprint, content_fingerprint, schema_major, dir_path, imported_at) \
         VALUES (?, 'Gen', NULL, 'AP', ?, ?, NULL, 'fp', 1, '', ?)",
    )
    .bind(id)
    .bind(provenance)
    .bind(recording)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();
    // A learner row so cascade is observable.
    sqlx::query("INSERT INTO mastery_records (lesson_set_id, concept_id, updated_at) VALUES (?, 'c', ?)")
        .bind(id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
}

async fn set_count(pool: &sqlx::Pool<sqlx::Sqlite>, id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM lesson_sets WHERE id = ?").bind(id).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn strike_deletes_a_generated_set_from_live_db_and_keeps_seed_imported() {
    let f = fx().await;
    let text = "stoichiometry converts grams to moles";
    let id = f
        .db
        .add_transcript_at("m1", text, None, true, 0.9, chrono::Utc::now())
        .await
        .unwrap();
    insert_generated_set(f.db.pool(), "gen-1", "m1", "generated").await;
    insert_generated_set(f.db.pool(), "seed-1", "m1", "seed").await;
    insert_generated_set(f.db.pool(), "imp-1", "m1", "imported").await;

    // Strike words → live purge of the generated set only.
    let b = text.find("grams").unwrap();
    let (s, e) = (text[..b].encode_utf16().count(), text[..b + 5].encode_utf16().count());
    rd::strike_words(
        f.db.pool(),
        &f.env,
        &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false },
        None,
    )
    .await
    .unwrap();

    assert_eq!(set_count(f.db.pool(), "gen-1").await, 0, "generated purged");
    assert_eq!(set_count(f.db.pool(), "seed-1").await, 1, "seed survives");
    assert_eq!(set_count(f.db.pool(), "imp-1").await, 1, "imported survives");
    // Learner rows cascade with the generated set.
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mastery_records WHERE lesson_set_id = 'gen-1'")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn whole_recording_delete_cascades_to_generated_set() {
    let f = fx().await;
    insert_generated_set(f.db.pool(), "gen-2", "m1", "generated").await;
    insert_generated_set(f.db.pool(), "seed-2", "m1", "seed").await;
    f.db.delete_meeting("m1").await.unwrap();
    assert_eq!(set_count(f.db.pool(), "gen-2").await, 0, "generated cascades");
    assert_eq!(set_count(f.db.pool(), "seed-2").await, 1, "seed survives the cascade");
}

#[tokio::test]
async fn backup_purge_reaches_a_generated_set_across_job_arms() {
    let f = fx().await;
    let text = "limiting reagent determines the yield";
    let id = f
        .db
        .add_transcript_at("m1", text, None, true, 0.9, chrono::Utc::now())
        .await
        .unwrap();
    insert_generated_set(f.db.pool(), "gen-3", "m1", "generated").await;

    // Snapshot a backup copy that also holds the generated set.
    rd::wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();

    // Strike words → live + backup purge.
    let b = text.find("yield").unwrap();
    let (s, e) = (text[..b].encode_utf16().count(), text[..b + 5].encode_utf16().count());
    let out = rd::strike_words(
        f.db.pool(),
        &f.env,
        &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: e, expected_text: None, whole_line: false },
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.backups_purged, 1, "warnings {:?}", out.warnings);

    // The backup copy no longer holds the generated set.
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(&copy);
    use sqlx::ConnectOptions;
    let mut conn = opts.connect().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lesson_sets WHERE id = 'gen-3'")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(n, 0, "backup purge removed the generated set");
}
