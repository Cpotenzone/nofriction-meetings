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
use sqlx::Row;
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
    let figures = vec![schema::Figure {
        id: figure_id.into(),
        path: format!("figures/{}.png", figure_id),
        sha256: sha(&png),
        width: 2,
        height: 2,
        alt: "a mole".into(),
        reviewed_by: "tester".into(),
    }];
    let manifest = Manifest {
        format: bundle::FORMAT.into(),
        schema_major: 1,
        schema_minor: 0,
        lesson_set_id: "set-sample".into(),
        title: "Stoichiometry sample".into(),
        provenance: "imported".into(),
        files: manifest_files,
        content_fingerprint: bundle::fingerprint_for(&files, &figures).unwrap(),
        authored_at: "2026-01-01T00:00:00Z".into(),
        generator: "test".into(),
        figures,
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
    // Molarity, molar mass and percent round-trip (review-a MEDIUM): a learner
    // who echoes the prompted unit grades Correct, not Unparseable.
    let rel = Tolerance { kind: "rel".into(), value: 0.01 };
    assert_eq!(grade::check_numeric("0.1 mol/L", 0.1, Some("mol/L"), &rel), NumericGrade::Correct);
    assert_eq!(grade::check_numeric("0.1 M", 0.1, Some("mol/L"), &rel), NumericGrade::Correct);
    assert_eq!(grade::check_numeric("44.01 g/mol", 44.01, Some("g/mol"), &rel), NumericGrade::Correct);
    assert_eq!(grade::check_numeric("80 %", 80.0, Some("%"), &rel), NumericGrade::Correct);
    assert_eq!(grade::check_numeric("80", 80.0, Some("%"), &rel), NumericGrade::Correct);
    // Cross-dimension mismatch stays Wrong (percent vs molarity).
    assert_eq!(grade::check_numeric("80 %", 80.0, Some("mol/L"), &rel), NumericGrade::Wrong);
    // Every unit class is now gradeable from its own round-trip helper.
    for u in ["mol/L", "M", "g/mol", "%", "percent", "g", "mol", "L", "°C"] {
        assert!(grade::unit_is_gradeable(u), "unit {} should be gradeable", u);
    }
    assert!(!grade::unit_is_gradeable("foobar"));
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

#[tokio::test]
async fn review_count_persists_so_interval_advances_across_db_round_trips() {
    // Regression for the SM-2-lite bug: the review count was rebuilt as 0 on
    // every grade, pinning the interval to 1 day forever. Driving multiple
    // reviews through the mastery_records table (with the `reviews` column)
    // must walk the 1 -> 3 -> interval*ease ladder.
    let f = fx().await;
    let pool = f.db.pool();
    let now = chrono::Utc::now().to_rfc3339();

    // Each "review" = load the row, apply a passing review, store it back,
    // exactly as the commands do (reviews loaded from and written to the DB).
    let mut intervals = Vec::new();
    for _ in 0..3 {
        let row = sqlx::query(
            "SELECT ease, interval_days, reviews FROM mastery_records \
             WHERE lesson_set_id = 'set' AND concept_id = 'c'",
        )
        .fetch_optional(pool)
        .await
        .unwrap();
        let (ease, interval_days, reviews) = match row {
            Some(r) => (r.get::<f64, _>("ease"), r.get::<f64, _>("interval_days"), r.get::<i64, _>("reviews") as u32),
            None => (review::START_EASE, 0.0, 0u32),
        };
        let mut rs = ReviewState { ease, interval_days, reviews };
        let next = review::apply_review(&mut rs, 5); // passing, first-try
        intervals.push(next);
        sqlx::query(
            "INSERT INTO mastery_records (lesson_set_id, concept_id, ease, interval_days, reviews, updated_at) \
             VALUES ('set', 'c', ?, ?, ?, ?) \
             ON CONFLICT(lesson_set_id, concept_id) DO UPDATE SET \
              ease=excluded.ease, interval_days=excluded.interval_days, reviews=excluded.reviews, updated_at=excluded.updated_at",
        )
        .bind(rs.ease)
        .bind(rs.interval_days)
        .bind(rs.reviews as i64)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
    }
    assert_eq!(intervals[0], 1.0, "first review is 1 day");
    assert_eq!(intervals[1], 3.0, "second review is 3 days (reviews persisted)");
    assert!(intervals[2] > 3.0, "third review grows by ease, got {} (bug: pinned to 1)", intervals[2]);

    // The persisted reviews counter reached 3.
    let reviews: i64 = sqlx::query_scalar(
        "SELECT reviews FROM mastery_records WHERE lesson_set_id = 'set' AND concept_id = 'c'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(reviews, 3);
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
// Shipped seed bundle validation + content rigor (docs/LESSONS.md §14/§15)
// ════════════════════════════════════════════════════════════════════════

/// The shipped seed course, embedded at compile time. Produced by
/// `cargo run --example author_seed_lessons` (FEAT-002). If this file is
/// absent the build fails here, which is the point: a figure-free or missing
/// seed course must never ship (owner-decision D4).
const SEED_BUNDLE: &[u8] = include_bytes!("../../resources/seed/stoichiometry.lesson");

#[test]
fn shipped_seed_bundle_passes_validation_with_all_floors() {
    // 1. It validates end-to-end through the ordinary import path (§4.5).
    let prepared = bundle::validate_bytes(SEED_BUNDLE)
        .expect("the shipped stoichiometry seed bundle must pass validate_bundle with zero errors");
    let set = &prepared.validated.set;
    let lessons = &prepared.validated.lessons;

    // 2. Exactly six lessons, in order.
    assert_eq!(lessons.len(), 6, "the seed course ships six lessons");

    // 3. Required-figure floor (D4): every lesson embeds >=1 figure-kind
    //    explainer whose figureId resolves to a manifest Figure, and that PNG
    //    decoded (validate_bytes already decoded every figure to its declared
    //    width/height, so a non-decoding/empty PNG would have failed above).
    let figure_ids: std::collections::HashSet<&str> =
        prepared.validated.figures.iter().map(|f| f.id.as_str()).collect();
    assert!(!figure_ids.is_empty(), "seed set must carry figures");
    for lesson in lessons {
        let figs: Vec<&schema::Explainer> =
            lesson.explainers.iter().filter(|e| e.kind == "figure").collect();
        assert!(
            !figs.is_empty(),
            "lesson {} ships no figure-kind explainer (D4 floor)",
            lesson.index
        );
        for f in figs {
            let fid = f.data.get("figureId").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                figure_ids.contains(fid),
                "lesson {} figure explainer references unknown figure {}",
                lesson.index,
                fid
            );
        }
    }
    // Every manifest figure has a non-empty reviewedBy signoff (§12.2).
    for f in &prepared.validated.figures {
        assert!(!f.reviewed_by.trim().is_empty(), "figure {} has no reviewedBy signoff", f.id);
    }

    // 4. Set-level FR-EXP inventory completeness (NIT-7): the union of
    //    interactive explainer kinds across the six lessons covers every kind.
    let kinds: std::collections::HashSet<&str> =
        lessons.iter().flat_map(|l| l.explainers.iter().map(|e| e.kind.as_str())).collect();
    for k in [
        "equation_balancer",
        "mole_ratio",
        "limiting_reagent",
        "dimensional_analysis",
        "molecule_diagram",
        "titration_curve",
        "ideal_gas_pv",
    ] {
        assert!(kinds.contains(k), "seed set is missing explainer kind {}", k);
    }

    // 5. Checkpoint item-bank floors (MEDIUM-5): bank >= 2x presentCount, and
    //    every tested concept has >= 2 items.
    for lesson in lessons {
        let pc = lesson.checkpoint.present_count as usize;
        let bank = lesson.checkpoint.items.len();
        assert!(bank >= pc * 2, "lesson {} checkpoint bank {} < 2x presentCount {}", lesson.index, bank, pc);
        let mut per_concept: HashMap<&str, usize> = HashMap::new();
        for it in &lesson.checkpoint.items {
            for cid in &it.concept_ids {
                *per_concept.entry(cid.as_str()).or_insert(0) += 1;
            }
        }
        for (cid, n) in &per_concept {
            assert!(*n >= 2, "lesson {} concept {} has only {} checkpoint items", lesson.index, cid, n);
        }
    }

    // 6. Balance-kind checkpoint floor (Finding 4): every concept taught by an
    //    equation_balancer explainer is covered by >= 1 balance-kind item in
    //    that lesson's checkpoint. Concretely Lesson 2 (balancing) must have one.
    for lesson in lessons {
        let teaches_balance = lesson.explainers.iter().any(|e| e.kind == "equation_balancer");
        if teaches_balance {
            let has_balance_item = lesson.checkpoint.items.iter().any(|i| i.kind == "balance");
            assert!(
                has_balance_item,
                "lesson {} teaches equation_balancer but has no balance-kind checkpoint item",
                lesson.index
            );
        }
    }

    // 7. Exam bank floors (MEDIUM-5): >=60 total, >=18 mc, >=18 numeric,
    //    >=9 multistep, mix sums to 30, mix.balance == 0, >=2 items per concept.
    let exam = &set.exam;
    assert!(exam.items.len() >= 60, "exam bank {} < 60", exam.items.len());
    let count = |k: &str| exam.items.iter().filter(|i| i.kind == k).count();
    assert!(count("mc") >= 18, "exam mc bank {} < 18", count("mc"));
    assert!(count("numeric") >= 18, "exam numeric bank {} < 18", count("numeric"));
    assert!(count("multistep") >= 9, "exam multistep bank {} < 9", count("multistep"));
    let m = &exam.mix;
    assert_eq!(m.mc + m.numeric + m.balance + m.multistep, exam.question_count);
    assert_eq!(m.balance, 0, "mix.balance must be 0 (NIT-6)");
    assert_eq!(exam.question_count, 30);
    let mut exam_concept: HashMap<&str, usize> = HashMap::new();
    for it in &exam.items {
        for cid in &it.concept_ids {
            *exam_concept.entry(cid.as_str()).or_insert(0) += 1;
        }
    }
    for (cid, n) in &exam_concept {
        assert!(*n >= 2, "exam concept {} has only {} items", cid, n);
    }

    // 7b. MC answer keys must vary WITHIN EACH assessment, not merely across
    //     the course union (review findings 1/2). An assessment whose every MC
    //     key sits at one index is gameable by always clicking that choice; an
    //     alternate client (planned iOS, an automated grader) that submits
    //     authored indices without replicating the frontend's presentation-time
    //     shuffle would pass it trivially. So the anti-gameability property must
    //     live in the platform-neutral content: assert per-assessment variety.
    //
    //     Any assessment with >= 2 MC items must use at least two distinct key
    //     positions. This is checked for every lesson checkpoint, the exam, and
    //     the diagnostic individually — a uniform-key checkpoint fails the build.
    let mc_key_positions = |items: &[schema::QuizItem]| -> std::collections::HashSet<u32> {
        items
            .iter()
            .filter(|it| it.kind == "mc")
            .filter_map(|it| it.answer_index)
            .collect()
    };
    let assert_mc_variety = |label: &str, items: &[schema::QuizItem]| {
        let mc_count = items.iter().filter(|it| it.kind == "mc").count();
        let positions = mc_key_positions(items);
        if mc_count >= 2 {
            assert!(
                positions.len() > 1,
                "{label}: all {mc_count} MC answer keys sit at one position ({:?}) — \
                 the assessment is trivially gameable by any client that submits \
                 authored indices without the frontend's choice shuffle",
                positions
            );
        }
    };
    for lesson in lessons {
        assert_mc_variety(
            &format!("lesson {} checkpoint", lesson.index),
            &lesson.checkpoint.items,
        );
    }
    assert_mc_variety("exam", &exam.items);
    if let Some(d) = &set.diagnostic {
        assert_mc_variety("diagnostic", &d.items);
    }

    // 7c. Both the multistep EXAM items and every CHECKPOINT bank must be made
    //     of DISTINCT prompts (review finding 3 / MEDIUM-3): a retake must be a
    //     genuinely different paper, so no bank may pad with verbatim-duplicated
    //     questions. Assert prompt uniqueness per assessment.
    let ms_prompts: std::collections::HashSet<&str> =
        exam.items.iter().filter(|i| i.kind == "multistep").map(|i| i.prompt.as_str()).collect();
    let ms_count = exam.items.iter().filter(|i| i.kind == "multistep").count();
    assert!(
        ms_prompts.len() >= ms_count,
        "multistep exam items are not all distinct: {} items but only {} unique prompts",
        ms_count,
        ms_prompts.len()
    );
    for lesson in lessons {
        let prompts: std::collections::HashSet<&str> =
            lesson.checkpoint.items.iter().map(|i| i.prompt.as_str()).collect();
        assert_eq!(
            prompts.len(),
            lesson.checkpoint.items.len(),
            "lesson {} checkpoint bank padded with duplicate prompts: {} items but {} unique prompts",
            lesson.index,
            lesson.checkpoint.items.len(),
            prompts.len()
        );
    }

    // 8. Content rigor: every worked example and item answer key recomputes
    //    consistently. Balance keys are already re-verified by validate_bundle
    //    (verify_balance runs on every balance item). Spot-check a known key:
    //    molar mass of H2O to 2 dp drives the Lesson-1 "36.04 g -> 2.00 mol".
    let l1 = &lessons[0];
    let n1 = l1
        .checkpoint
        .items
        .iter()
        .find(|i| i.id == "l1-cp-n1")
        .expect("lesson 1 checkpoint has the 36.04 g H2O item");
    let ans = n1.answer.expect("numeric answer present");
    assert!((ans - 2.0).abs() < 0.01, "36.04 g H2O should be 2.00 mol, got {}", ans);

    // 8b. Every authored numeric answer unit — on items AND on multistep steps,
    //     across every lesson checkpoint, trainer, the exam, and the diagnostic
    //     — must round-trip through the authoritative grader's own units table
    //     (review-a MEDIUM). An authored unit the grader would reject as
    //     "couldn't read that unit" is a build break here, not a runtime
    //     surprise for a learner who echoes the prompted unit. (The import-time
    //     validator enforces the same thing; this is a direct, legible guard on
    //     the shipped seed so the property is visible in the test suite.)
    let assert_unit_gradeable = |label: &str, unit: &Option<String>| {
        if let Some(u) = unit {
            if !u.is_empty() {
                assert!(
                    grade::unit_is_gradeable(u),
                    "{label}: authored numeric unit {:?} is not in the grader's UNITS table — \
                     a learner who answers with this unit would be told 'couldn't read that unit'",
                    u
                );
            }
        }
    };
    let check_item_units = |label: &str, items: &[schema::QuizItem]| {
        for it in items {
            if it.kind == "numeric" {
                assert_unit_gradeable(&format!("{label} item {}", it.id), &it.unit);
            }
            if it.kind == "multistep" {
                for (si, s) in it.steps.iter().flatten().enumerate() {
                    if s.kind == "numeric" {
                        assert_unit_gradeable(&format!("{label} item {} step {}", it.id, si), &s.unit);
                    }
                }
            }
        }
    };
    for lesson in lessons {
        check_item_units(&format!("lesson {} checkpoint", lesson.index), &lesson.checkpoint.items);
        check_item_units(&format!("lesson {} trainer", lesson.index), &lesson.trainer.static_items);
    }
    check_item_units("exam", &exam.items);
    if let Some(d) = &set.diagnostic {
        check_item_units("diagnostic", &d.items);
    }

    // 8c. Recompute the numeric answer keys from the SAME shared molar-mass
    //     table / STP constant the generators use (docs/LESSONS.md §4.6 full
    //     recompute), rather than trusting a hand-authored key. This closes the
    //     gap where a future wrong numeric key would pass the finiteness-only
    //     structural check. Each recomputed value must equal the authored key
    //     within the item's own relative tolerance.
    let find_item = |id: &str| -> &schema::QuizItem {
        lessons
            .iter()
            .flat_map(|l| l.checkpoint.items.iter())
            .chain(exam.items.iter())
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("seed is missing numeric item {}", id))
    };
    let assert_key = |id: &str, expected: f64| {
        let it = find_item(id);
        let got = it.answer.unwrap_or_else(|| panic!("item {} has no answer", id));
        let rel = it.tolerance.as_ref().map(|t| t.value).unwrap_or(0.01).max(1e-9);
        assert!(
            (got - expected).abs() <= rel * expected.abs().max(1e-9),
            "numeric key {} = {} does not recompute to {} (rel tol {})",
            id,
            got,
            expected,
            rel
        );
    };
    let mm_h2o = generators::molar_mass("H2O").expect("H2O molar mass");
    let mm_h2 = generators::molar_mass("H2").expect("H2 molar mass");
    let mm_co2 = generators::molar_mass("CO2").expect("CO2 molar mass");
    let vm = generators::MOLAR_VOLUME_STP;
    // mole = mass / molar mass
    assert_key("l1-cp-n1", 36.04 / mm_h2o); // 2.00 mol
    assert_key("l1-cp-n3", 88.02 / mm_co2); // 2.00 mol
    assert_key("l1-cp-n2", mm_co2); // molar mass of CO2 as the key itself
    assert_key("ex-num-0", 36.04 / mm_h2o);
    assert_key("ex-num-2", 18.02 / mm_h2o); // 1.00 mol
    assert_key("ex-num-18", mm_co2 / mm_co2); // 1.00 mol from 44.01 g CO2
    // mass = moles * molar mass
    assert_key("ex-num-3", 2.0 * mm_co2); // 88.02 g
    assert_key("ex-num-11", (2.02 / mm_h2) * mm_h2o); // 2.02 g H2 → 1 mol → 18.02 g H2O
    // mole ratio 2 H2 + O2 → 2 H2O: g H2O from 4.00 g H2 (1:1 mole ratio)
    assert_key("l3-cp-n1", (4.00 / mm_h2) * mm_h2o);
    assert_key("ex-num-6", (4.00 / mm_h2) * mm_h2o);
    // percent yield = actual / theoretical * 100
    assert_key("l4-cp-n1", 8.00 / 10.00 * 100.0); // 80 %
    assert_key("ex-num-9", 8.00 / 10.00 * 100.0);
    assert_key("ex-num-10", 4.50 / 5.00 * 100.0); // 90 %
    // molarity = moles / liters, and dilution M1V1 = M2V2
    assert_key("l5-cp-n1", 0.100 * 0.250); // 0.025 mol
    assert_key("l5-cp-n2", 0.0500 / 0.500); // 0.1 mol/L
    assert_key("l5-cp-n3", 2.00 * 0.0500 / 0.200); // 0.5 mol/L
    assert_key("ex-num-12", 0.100 * 0.250);
    assert_key("ex-num-13", 0.0500 / 0.500);
    assert_key("ex-num-14", 2.00 * 0.0500 / 0.200);
    // gas volume at STP = moles * molar volume
    assert_key("l6-cp-n1", 2.0 * vm); // 44.82 L
    assert_key("l6-cp-n2", 44.82 / vm); // 2.00 mol
    assert_key("ex-num-15", 2.0 * vm);
    assert_key("ex-num-16", 44.82 / vm);
    assert_key("ex-num-19", 0.500 * vm); // 11.205 L
    // ideal gas V = nRT/P
    assert_key("l6-cp-n3", 1.00 * 0.0821 * 273.15 / 1.00);
    assert_key("ex-num-17", 1.00 * 0.0821 * 273.15 / 1.00);
}

/// Build the correct `Given` for an item straight from its stored answer key —
/// the "perfect learner" oracle used by the exam-passability test.
fn perfect_given(item: &schema::QuizItem) -> Given {
    match item.kind.as_str() {
        "mc" => Given::Mc(item.answer_index.expect("mc item has an answerIndex")),
        "numeric" => Given::Numeric(format!("{}", item.answer.expect("numeric item has an answer"))),
        "balance" => Given::Balance(
            item.answer_coefficients
                .clone()
                .expect("balance item has coefficients")
                .iter()
                .map(|&c| c as u32)
                .collect(),
        ),
        "multistep" => Given::Multistep(
            item.steps
                .as_ref()
                .expect("multistep item has steps")
                .iter()
                .map(|s| match s.kind.as_str() {
                    "mc" => MultistepGiven::Mc(s.answer_index.expect("mc step answerIndex")),
                    "numeric" => MultistepGiven::Numeric(format!("{}", s.answer.expect("numeric step answer"))),
                    "balance" => MultistepGiven::Balance(
                        s.answer_coefficients.clone().expect("balance step coeffs").iter().map(|&c| c as u32).collect(),
                    ),
                    other => panic!("unexpected step kind {}", other),
                })
                .collect(),
        ),
        other => panic!("unexpected item kind {}", other),
    }
}

#[test]
fn seed_exam_is_passable_by_a_perfect_learner_and_multistep_counts() {
    // Regression for the HIGH exam-integrity findings: with multistep answers
    // actually submitted, a perfect learner must clear 80% with real margin
    // (previously the 6 multistep points were unreachable from the UI, capping
    // a flawless MC+numeric learner at exactly 80%).
    let prepared = bundle::validate_bytes(SEED_BUNDLE).expect("seed validates");
    let exam = &prepared.validated.set.exam;

    // Score the whole bank as a perfect learner (a drawn attempt is a subset,
    // so a perfect score on the superset implies a perfect score on any draw).
    let answered: Vec<exam::AnsweredItem> =
        exam.items.iter().map(|i| exam::AnsweredItem { item: i, given: perfect_given(i) }).collect();
    let scored = exam::score_attempt(&answered, exam.pass_pct);
    assert!(
        (scored.score_pct - 100.0).abs() < 1e-6,
        "a perfect learner should score 100%, got {}",
        scored.score_pct
    );
    assert!(scored.passed, "a perfect learner must pass");

    // The multistep portion is non-trivial: it contributes real points, so an
    // otherwise-perfect learner who leaves every multistep item blank must lose
    // enough to NOT trivially pass on MC+numeric alone (the old failure mode was
    // the inverse — multistep could never contribute).
    let answered_no_ms: Vec<exam::AnsweredItem> = exam
        .items
        .iter()
        .map(|i| {
            let given = if i.kind == "multistep" {
                Given::Multistep(vec![]) // no step answers submitted
            } else {
                perfect_given(i)
            };
            exam::AnsweredItem { item: i, given }
        })
        .collect();
    let scored_no_ms = exam::score_attempt(&answered_no_ms, exam.pass_pct);
    assert!(
        scored_no_ms.score_pct < 100.0,
        "leaving multistep blank must cost points, got {}",
        scored_no_ms.score_pct
    );
    // And the multistep items each carry more than one point (per-step), so the
    // bank's total possible exceeds its item count — proving multistep is graded
    // per step, not as a single all-or-nothing point.
    assert!(scored.possible > exam.items.len() as f64, "multistep items must be graded per step");
}

/// A blank/unanswered `Given` for an item — what the fixed exam/checkpoint
/// commands substitute for any item in the server-known selection that the
/// client did not submit. Every variant grades to zero.
fn blank_given(item: &schema::QuizItem) -> Given {
    match item.kind.as_str() {
        "mc" => Given::Mc(u32::MAX),
        "numeric" => Given::Numeric(String::new()),
        "balance" => Given::Balance(vec![]),
        "multistep" => Given::Multistep(vec![]),
        other => panic!("unexpected item kind {}", other),
    }
}

#[test]
fn exam_scored_over_selected_set_not_posted_subset() {
    // Regression for the MEDIUM scoring-integrity gap: the exam is scored over
    // the server-known selection drawn at start, with every un-submitted item
    // counting as zero. A one-item-correct submission must therefore score far
    // below the pass line, never 100%. This mirrors `lessons_submit_exam`,
    // which now builds the scored attempt over the full selected set and pads
    // missing answers with a blank `Given`.
    let prepared = bundle::validate_bytes(SEED_BUNDLE).expect("seed validates");
    let exam = &prepared.validated.set.exam;

    // The selection is the full bank here (a draw is a subset); grade one item
    // correctly and leave the rest blank.
    let answered: Vec<exam::AnsweredItem> = exam
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let given = if i == 0 { perfect_given(item) } else { blank_given(item) };
            exam::AnsweredItem { item, given }
        })
        .collect();
    let scored = exam::score_attempt(&answered, exam.pass_pct);
    assert!(
        !scored.passed,
        "a single correct answer over the full selection must not pass, got {}%",
        scored.score_pct
    );
    assert!(
        scored.score_pct < exam.pass_pct as f64,
        "score must be below the pass line, got {}%",
        scored.score_pct
    );
    // Sanity: the denominator is the whole selection, not the one posted item.
    assert!(scored.possible >= exam.items.len() as f64, "possible spans the whole selection");
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

/// The shipped course is in the binary and passes the import checks, so a
/// store build's Lessons tab starts with it. Validation only: committing would
/// write into the real app-data lessons folder under the seed's own id.
#[test]
fn the_seed_course_is_compiled_in_and_valid() {
    assert!(super::seed::SEED_BUNDLE.len() > 10_000, "the seed bundle is embedded");
    let prepared = super::bundle::validate_bytes(super::seed::SEED_BUNDLE).expect("the shipped seed passes import validation");
    assert_eq!(prepared.validated.set.id, super::seed::SEED_SET_ID);
    assert!(prepared.validated.set.title.to_lowercase().contains("stoichiometry"), "{}", prepared.validated.set.title);
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

#[test]
fn oversized_unzipped_bundle_is_rejected_before_it_fills_memory() {
    let (zip, _id) = sample_bundle();
    let mut map = unzip_to_map(&zip);
    // 65 MB of zeros deflates to a tiny zip: a classic bomb.
    map.insert("figures/bomb.png".into(), vec![0u8; 65 * 1024 * 1024]);
    let err = bundle::validate_bytes(&rebuild_zip(&map)).err().expect("bomb must be rejected");
    assert!(err.contains("too large"), "{}", err);
}

#[tokio::test]
async fn url_import_requires_https_for_public_hosts() {
    let f = fx().await;
    let pool = f.db.get_pool();
    let err = bundle::import_from_url(&pool, "http://example.com/course.lesson").await.unwrap_err();
    assert!(err.contains("https"), "{}", err);
    let err = bundle::import_from_url(&pool, "ftp://example.com/course.lesson").await.unwrap_err();
    assert!(err.contains("only http(s)"), "{}", err);
}

// ════════════════════════════════════════════════════════════════════════
// A bundle that claims to be generated (docs/LESSONS.md §4.6/§4.7)
// ════════════════════════════════════════════════════════════════════════

/// Replace files in a bundle, keeping the per-file hashes right and dropping
/// the declared content fingerprint (tolerated when empty), so the test
/// exercises schema and provenance, not integrity.
fn rezip_with(zip: &[u8], edits: &[(&str, Value)]) -> Vec<u8> {
    let mut map = unzip_to_map(zip);
    for (path, v) in edits {
        let bytes = serde_json::to_vec(v).unwrap();
        update_manifest_hash(&mut map, path, &bytes);
        map.insert(path.to_string(), bytes);
    }
    let mut manifest: Manifest = serde_json::from_slice(&map["manifest.json"]).unwrap();
    manifest.content_fingerprint = String::new();
    map.insert("manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
    rebuild_zip(&map)
}

fn claiming_generated(zip: &[u8]) -> Value {
    let map = unzip_to_map(zip);
    let mut set: Value = serde_json::from_slice(&map["lessonset.json"]).unwrap();
    set["provenance"] = json!("generated");
    set["sourceRecordingId"] = json!("m1");
    set["transcriptFingerprint"] = json!("abc");
    set
}

#[test]
fn a_bundle_claiming_generated_is_held_to_the_authored_floors() {
    let (zip, _) = sample_bundle();
    // Authored-valid content claiming `generated` still validates (as authored)
    let ok = rezip_with(&zip, &[("lessonset.json", claiming_generated(&zip))]);
    assert!(bundle::validate_bytes(&ok).is_ok());
    // Figure-free content (enough for the generated profile) claiming
    // `generated` fails the D4 figure floor: the claim buys nothing
    let map = unzip_to_map(&zip);
    let mut lesson: Value = serde_json::from_slice(&map["lessons/l1.json"]).unwrap();
    let text = json!({"id": "ex-text", "title": "Moles", "conceptIds": ["c-mole"], "kind": "text", "data": {"paragraphs": ["A mole counts particles."]}});
    lesson["explainers"] = json!([text, lesson["explainers"][1].clone()]);
    let bad = rezip_with(&zip, &[("lessonset.json", claiming_generated(&zip)), ("lessons/l1.json", lesson)]);
    let err = bundle::validate_bytes(&bad).err().expect("no generated shortcut on import");
    assert!(err.contains("figure"), "{}", err);
}

#[tokio::test]
async fn an_import_claiming_generated_is_stored_and_shown_as_imported() {
    let f = fx().await;
    let root = f.dir.join("catalog");
    let (zip, set_id) = sample_bundle();
    let claimed = rezip_with(&zip, &[("lessonset.json", claiming_generated(&zip))]);
    let prepared = bundle::validate_bytes(&claimed).unwrap();
    bundle::commit_bundle_at(f.db.pool(), &root, &prepared, "imported", None).await.unwrap();

    // The stored file says what the catalog says
    let stored: Value = serde_json::from_slice(&std::fs::read(root.join(&set_id).join("lessonset.json")).unwrap()).unwrap();
    assert_eq!(stored["provenance"], "imported");
    assert!(stored.get("sourceRecordingId").map_or(true, |v| v.is_null()), "{}", stored);
    assert!(stored.get("transcriptFingerprint").map_or(true, |v| v.is_null()), "{}", stored);
    let row = sqlx::query("SELECT provenance, source_recording_id FROM lesson_sets WHERE id = ?")
        .bind(&set_id)
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("provenance"), "imported");
    assert_eq!(row.get::<Option<String>, _>("source_recording_id"), None);
    // The course, trainer and certificate read the catalog's provenance
    let shown = super::commands::load_set_as_cataloged(f.db.pool(), &set_id).await.unwrap();
    assert_eq!((shown.provenance.as_str(), shown.source_recording_id.as_deref()), ("imported", None));

    // A file left from before the rewrite that still claims `generated` is
    // shown as the catalog row says
    std::fs::write(root.join(&set_id).join("lessonset.json"), serde_json::to_vec(&claiming_generated(&zip)).unwrap()).unwrap();
    let shown = super::commands::load_set_as_cataloged(f.db.pool(), &set_id).await.unwrap();
    assert_eq!((shown.provenance.as_str(), shown.source_recording_id.as_deref()), ("imported", None));

    // …and a recording edit never purges it (rows or files)
    let mut conn = f.db.pool().acquire().await.unwrap();
    assert_eq!(super::purge_for_meeting(&mut conn, "m1").await.unwrap(), 0);
    drop(conn);
    assert_eq!(super::purge::remove_unreferenced_dirs_for_meeting(f.db.pool(), &root, "m1").await.unwrap(), 0);
    assert!(root.join(&set_id).join("lessonset.json").is_file());
}
