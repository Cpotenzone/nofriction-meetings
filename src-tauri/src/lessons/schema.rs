//! The Lessons data model (docs/LESSONS.md §3) and its strict validation
//! (§4.6). All types are platform-neutral serde structs shaped 1:1 to Swift
//! `Codable`. Authored content is untrusted on load: tolerant parse
//! (`parse::extract_json`) then strict validate + clean here. A field that
//! fails cleaning drops its element; a structurally unusable file fails the
//! whole bundle (returns `Err`).

use crate::lessons::explain;
use crate::lessons::parse::{self, clean_text, LONG, MEDIUM, SHORT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub type ValidateError = String;

// Count bounds (docs/LESSONS.md §4.6). Lower bounds are the §10.1b floors;
// upper bounds guard against a hostile oversized bundle.
pub const MIN_EXPLAINERS: usize = 2;
pub const MAX_EXPLAINERS: usize = 5;
pub const MIN_MISCONCEPTIONS: usize = 2;
pub const MAX_MISCONCEPTIONS: usize = 6;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 6;
pub const MAX_CHECKPOINT_BANK: usize = 40;
pub const MIN_EXAM_BANK: usize = 60;
pub const MAX_EXAM_BANK: usize = 300;
pub const MIN_WORKED_EXAMPLES: usize = 3;
pub const MAX_LESSONS: usize = 64;
pub const MAX_CONCEPTS: usize = 128;

// ════════════════════════════════════════════════════════════════════════
// Authored content
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LessonSet {
    pub id: String,
    pub schema_major: u32,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub level: String,
    pub provenance: String,
    #[serde(default)]
    pub source_recording_id: Option<String>,
    #[serde(default)]
    pub transcript_fingerprint: Option<String>,
    pub lessons: Vec<String>,
    pub concepts: Vec<Concept>,
    #[serde(default)]
    pub diagnostic: Option<Diagnostic>,
    pub exam: Exam,
    #[serde(default)]
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Concept {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub lesson_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Objective {
    pub text: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    pub id: String,
    pub index: u32,
    pub title: String,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    pub explainers: Vec<Explainer>,
    pub worked_examples: Vec<WorkedExample>,
    pub misconceptions: Misconceptions,
    pub trainer: Trainer,
    pub checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Explainer {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub a11y_state: Option<String>,
    pub kind: String,
    pub data: Value,
}

pub const EXPLAINER_KINDS: &[&str] = &[
    "figure",
    "equation_balancer",
    "mole_ratio",
    "limiting_reagent",
    "dimensional_analysis",
    "molecule_diagram",
    "titration_curve",
    "ideal_gas_pv",
    "misconceptions",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub text: String,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinalAnswer {
    pub value: f64,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkedExample {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    pub steps: Vec<Step>,
    pub final_answer: FinalAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MisconceptionItem {
    pub claim: String,
    pub why_wrong: String,
    pub correct: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Misconceptions {
    pub items: Vec<MisconceptionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tolerance {
    pub kind: String, // exact | abs | rel
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reactant {
    pub formula: String,
    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub formula: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Equation {
    pub reactants: Vec<Reactant>,
    pub products: Vec<Product>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultistepStep {
    pub prompt: String,
    pub kind: String, // mc | numeric | balance
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    #[serde(default)]
    pub answer_index: Option<u32>,
    #[serde(default)]
    pub answer: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub tolerance: Option<Tolerance>,
    #[serde(default)]
    pub equation: Option<Equation>,
    #[serde(default)]
    pub answer_coefficients: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuizItem {
    pub id: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    pub kind: String, // mc | numeric | balance | multistep
    pub prompt: String,
    #[serde(default)]
    pub solution: Vec<Step>,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    #[serde(default)]
    pub answer_index: Option<u32>,
    #[serde(default)]
    pub answer: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub tolerance: Option<Tolerance>,
    #[serde(default)]
    pub sig_figs: Option<u32>,
    #[serde(default)]
    pub equation: Option<Equation>,
    #[serde(default)]
    pub answer_coefficients: Option<Vec<u32>>,
    #[serde(default)]
    pub steps: Option<Vec<MultistepStep>>,
    #[serde(default)]
    pub gated: Option<bool>,
    #[serde(default)]
    pub generator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub present_count: u32,
    pub items: Vec<QuizItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExamMix {
    pub mc: u32,
    pub numeric: u32,
    pub balance: u32,
    pub multistep: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Exam {
    pub items: Vec<QuizItem>,
    pub question_count: u32,
    pub pass_pct: u32,
    pub mix: ExamMix,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemGenerator {
    pub template: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trainer {
    pub id: String,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    #[serde(default)]
    pub generators: Vec<ItemGenerator>,
    #[serde(default)]
    pub static_items: Vec<QuizItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub items: Vec<QuizItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub alt: String,
    #[serde(default)]
    pub reviewed_by: String,
}

// ════════════════════════════════════════════════════════════════════════
// Tolerant parse entry points
// ════════════════════════════════════════════════════════════════════════

pub fn parse_lesson_set(raw: &str) -> Result<LessonSet, ValidateError> {
    let v = parse::extract_json(raw)?;
    serde_json::from_value(v).map_err(|e| format!("lessonset.json: {}", e))
}

pub fn parse_lesson(raw: &str) -> Result<Lesson, ValidateError> {
    let v = parse::extract_json(raw)?;
    serde_json::from_value(v).map_err(|e| format!("lesson: {}", e))
}

// ════════════════════════════════════════════════════════════════════════
// Validation + cleaning
// ════════════════════════════════════════════════════════════════════════

fn clean_in_place(s: &mut String, max: usize) -> bool {
    match clean_text(s, max) {
        Some(c) => {
            *s = c;
            true
        }
        None => false,
    }
}

fn finite(x: f64) -> bool {
    x.is_finite()
}

/// Validate + clean a single quiz/exam item in place. Returns Err to DROP it.
/// `figures`/`concept_ids` are the resolvable id sets for referential checks.
/// Test-only accessor for the item validator (used by lessons::tests).
#[cfg(test)]
pub fn validate_item_pub(item: &mut QuizItem, concepts: &HashSet<String>) -> Result<(), ValidateError> {
    validate_item(item, concepts)
}

fn validate_item(item: &mut QuizItem, concepts: &HashSet<String>) -> Result<(), ValidateError> {
    if !clean_in_place(&mut item.prompt, MEDIUM) {
        return Err("item has no prompt".into());
    }
    clean_in_place(&mut item.explanation, LONG);
    // Referential integrity: at least one resolvable concept id.
    item.concept_ids.retain(|c| concepts.contains(c));
    if item.concept_ids.is_empty() {
        return Err("item tags no known concept".into());
    }
    for step in &mut item.solution {
        clean_in_place(&mut step.text, MEDIUM);
    }
    match item.kind.as_str() {
        "mc" => {
            let choices = item.choices.as_mut().ok_or("mc item has no choices")?;
            choices.retain_mut(|c| clean_in_place(c, SHORT));
            let uniq: HashSet<&String> = choices.iter().collect();
            if choices.len() < MIN_CHOICES
                || choices.len() > MAX_CHOICES
                || uniq.len() != choices.len()
            {
                return Err("mc choices out of range or not unique".into());
            }
            let idx = item.answer_index.ok_or("mc item has no answerIndex")?;
            if idx as usize >= choices.len() {
                return Err("mc answerIndex out of range".into());
            }
        }
        "numeric" => {
            let a = item.answer.ok_or("numeric item has no answer")?;
            if !finite(a) {
                return Err("numeric answer not finite".into());
            }
            let tol = item.tolerance.as_ref().ok_or("numeric item has no tolerance")?;
            if !matches!(tol.kind.as_str(), "exact" | "abs" | "rel") || !finite(tol.value) || tol.value < 0.0 {
                return Err("numeric tolerance invalid".into());
            }
        }
        "balance" => {
            let eq = item.equation.as_ref().ok_or("balance item has no equation")?;
            let coeffs = item.answer_coefficients.as_ref().ok_or("balance item has no coefficients")?;
            explain::verify_balance(eq, coeffs)?;
        }
        "multistep" => {
            let steps = item.steps.as_mut().ok_or("multistep item has no steps")?;
            if steps.is_empty() {
                return Err("multistep item has no steps".into());
            }
            for s in steps.iter_mut() {
                if !clean_in_place(&mut s.prompt, MEDIUM) {
                    return Err("multistep step has no prompt".into());
                }
                match s.kind.as_str() {
                    "mc" => {
                        let choices = s.choices.as_mut().ok_or("multistep mc has no choices")?;
                        choices.retain_mut(|c| clean_in_place(c, SHORT));
                        if choices.len() < MIN_CHOICES || choices.len() > MAX_CHOICES {
                            return Err("multistep mc choices out of range".into());
                        }
                        let idx = s.answer_index.ok_or("multistep mc has no answerIndex")?;
                        if idx as usize >= choices.len() {
                            return Err("multistep mc answerIndex out of range".into());
                        }
                    }
                    "numeric" => {
                        let a = s.answer.ok_or("multistep numeric has no answer")?;
                        if !finite(a) {
                            return Err("multistep numeric answer not finite".into());
                        }
                        let tol = s.tolerance.as_ref().ok_or("multistep numeric has no tolerance")?;
                        if !matches!(tol.kind.as_str(), "exact" | "abs" | "rel") || !finite(tol.value) {
                            return Err("multistep numeric tolerance invalid".into());
                        }
                    }
                    "balance" => {
                        let eq = s.equation.as_ref().ok_or("multistep balance has no equation")?;
                        let coeffs = s.answer_coefficients.as_ref().ok_or("multistep balance has no coefficients")?;
                        explain::verify_balance(eq, coeffs)?;
                    }
                    other => return Err(format!("unknown multistep kind {}", other)),
                }
            }
        }
        other => return Err(format!("unknown item kind {}", other)),
    }
    Ok(())
}

fn validate_explainer(
    e: &mut Explainer,
    concepts: &HashSet<String>,
    figure_ids: &HashSet<String>,
) -> Result<(), ValidateError> {
    if !clean_in_place(&mut e.title, SHORT) {
        return Err("explainer has no title".into());
    }
    clean_in_place(&mut e.caption, MEDIUM);
    if let Some(a) = e.a11y_state.as_mut() {
        clean_in_place(a, MEDIUM);
    }
    if !EXPLAINER_KINDS.contains(&e.kind.as_str()) {
        return Err(format!("unknown explainer kind {}", e.kind));
    }
    e.concept_ids.retain(|c| concepts.contains(c));
    if e.kind == "figure" {
        let fid = e
            .data
            .get("figureId")
            .and_then(|v| v.as_str())
            .ok_or("figure explainer has no figureId")?;
        if !figure_ids.contains(fid) {
            return Err("figure explainer references an unknown figure".into());
        }
    } else {
        // Closed-form / structural correctness check for the kind's payload.
        explain::verify_explainer(&e.kind, &e.data)?;
    }
    Ok(())
}

fn validate_lesson(
    lesson: &mut Lesson,
    concepts: &HashSet<String>,
    figure_ids: &HashSet<String>,
) -> Result<(), ValidateError> {
    if !clean_in_place(&mut lesson.title, SHORT) {
        return Err("lesson has no title".into());
    }
    for o in &mut lesson.objectives {
        clean_in_place(&mut o.text, MEDIUM);
        o.concept_ids.retain(|c| concepts.contains(c));
    }
    lesson.concept_ids.retain(|c| concepts.contains(c));

    // Explainers: drop a bad one; keep count in bounds; enforce the figure floor.
    let mut kept = Vec::new();
    for mut e in std::mem::take(&mut lesson.explainers) {
        if validate_explainer(&mut e, concepts, figure_ids).is_ok() {
            kept.push(e);
        }
    }
    lesson.explainers = kept;
    let figure_count = lesson.explainers.iter().filter(|e| e.kind == "figure").count();
    if figure_count < 1 {
        return Err(format!("lesson {} has no figure-kind explainer (D4 floor)", lesson.index));
    }
    if lesson.explainers.len() < MIN_EXPLAINERS || lesson.explainers.len() > MAX_EXPLAINERS {
        return Err(format!(
            "lesson {} has {} explainers (want {}..={})",
            lesson.index,
            lesson.explainers.len(),
            MIN_EXPLAINERS,
            MAX_EXPLAINERS
        ));
    }

    // Worked examples: drop a bad one, enforce the floor, recompute-verify.
    let mut we = Vec::new();
    for mut w in std::mem::take(&mut lesson.worked_examples) {
        if validate_worked_example(&mut w, concepts).is_ok() {
            we.push(w);
        }
    }
    lesson.worked_examples = we;
    if lesson.worked_examples.len() < MIN_WORKED_EXAMPLES {
        return Err(format!("lesson {} has too few worked examples", lesson.index));
    }

    // Misconceptions: 2..=6.
    lesson.misconceptions.items.retain_mut(|m| {
        clean_in_place(&mut m.claim, MEDIUM)
            && clean_in_place(&mut m.why_wrong, MEDIUM)
            && clean_in_place(&mut m.correct, MEDIUM)
    });
    for m in &mut lesson.misconceptions.items {
        m.concept_ids.retain(|c| concepts.contains(c));
    }
    let n = lesson.misconceptions.items.len();
    if n < MIN_MISCONCEPTIONS || n > MAX_MISCONCEPTIONS {
        return Err(format!("lesson {} misconceptions out of range ({})", lesson.index, n));
    }

    // Trainer: validate static items; drop generators with an unknown template.
    lesson
        .trainer
        .generators
        .retain(|g| crate::lessons::generators::is_known_template(&g.template));
    let mut statics = Vec::new();
    for mut it in std::mem::take(&mut lesson.trainer.static_items) {
        if validate_item(&mut it, concepts).is_ok() {
            statics.push(it);
        }
    }
    lesson.trainer.static_items = statics;

    // Checkpoint: present_count 8..=10, bank present_count*2 ..= 40.
    let pc = lesson.checkpoint.present_count as usize;
    if !(8..=10).contains(&pc) {
        return Err(format!("lesson {} checkpoint presentCount out of range", lesson.index));
    }
    let mut items = Vec::new();
    for mut it in std::mem::take(&mut lesson.checkpoint.items) {
        if validate_item(&mut it, concepts).is_ok() {
            items.push(it);
        }
    }
    lesson.checkpoint.items = items;
    let bank = lesson.checkpoint.items.len();
    if bank < pc * 2 || bank > MAX_CHECKPOINT_BANK {
        return Err(format!(
            "lesson {} checkpoint bank {} not in {}..={}",
            lesson.index,
            bank,
            pc * 2,
            MAX_CHECKPOINT_BANK
        ));
    }
    Ok(())
}

fn validate_worked_example(w: &mut WorkedExample, concepts: &HashSet<String>) -> Result<(), ValidateError> {
    if !clean_in_place(&mut w.prompt, MEDIUM) {
        return Err("worked example has no prompt".into());
    }
    w.concept_ids.retain(|c| concepts.contains(c));
    for s in &mut w.steps {
        clean_in_place(&mut s.text, MEDIUM);
    }
    if w.steps.is_empty() {
        return Err("worked example has no steps".into());
    }
    if !finite(w.final_answer.value) {
        return Err("worked example final answer not finite".into());
    }
    Ok(())
}

fn validate_exam(exam: &mut Exam, concepts: &HashSet<String>) -> Result<(), ValidateError> {
    let mut items = Vec::new();
    for mut it in std::mem::take(&mut exam.items) {
        if validate_item(&mut it, concepts).is_ok() {
            items.push(it);
        }
    }
    exam.items = items;
    let n = exam.items.len();
    if n < MIN_EXAM_BANK || n > MAX_EXAM_BANK {
        return Err(format!("exam bank {} not in {}..={}", n, MIN_EXAM_BANK, MAX_EXAM_BANK));
    }
    let m = &exam.mix;
    if m.mc + m.numeric + m.balance + m.multistep != exam.question_count {
        return Err("exam mix does not sum to questionCount".into());
    }
    if m.balance != 0 {
        return Err("exam mix.balance must be 0 (NIT-6)".into());
    }
    if exam.pass_pct == 0 || exam.pass_pct > 100 {
        return Err("exam passPct out of range".into());
    }
    // Per-kind supply must cover the drawn mix.
    let count_kind = |k: &str| exam.items.iter().filter(|i| i.kind == k).count() as u32;
    if count_kind("mc") < m.mc || count_kind("numeric") < m.numeric || count_kind("multistep") < m.multistep {
        return Err("exam bank cannot supply the mix".into());
    }
    Ok(())
}

/// Detect a cycle in the concept prerequisite graph (DFS). Returns Err on a
/// cycle or a prerequisite referencing a missing concept.
fn validate_concept_dag(concepts: &[Concept]) -> Result<(), ValidateError> {
    let ids: HashSet<&str> = concepts.iter().map(|c| c.id.as_str()).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for c in concepts {
        for p in &c.prerequisites {
            if !ids.contains(p.as_str()) {
                return Err(format!("concept {} has an unknown prerequisite", c.id));
            }
            adj.entry(c.id.as_str()).or_default().push(p.as_str());
        }
    }
    // 0=unvisited, 1=on-stack, 2=done
    let mut state: HashMap<&str, u8> = HashMap::new();
    fn dfs<'a>(
        n: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        state: &mut HashMap<&'a str, u8>,
    ) -> bool {
        state.insert(n, 1);
        if let Some(nbrs) = adj.get(n) {
            for &m in nbrs {
                match state.get(m).copied().unwrap_or(0) {
                    1 => return true,
                    0 => {
                        if dfs(m, adj, state) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        state.insert(n, 2);
        false
    }
    for c in concepts {
        if state.get(c.id.as_str()).copied().unwrap_or(0) == 0 && dfs(c.id.as_str(), &adj, &mut state) {
            return Err("concept prerequisite graph has a cycle".into());
        }
    }
    Ok(())
}

/// A fully validated, cleaned lesson set ready to persist/play.
#[derive(Debug, Clone)]
pub struct ValidatedSet {
    pub set: LessonSet,
    pub lessons: Vec<Lesson>,
    pub figures: Vec<Figure>,
}

/// Validate the whole set: structural bounds, referential integrity, acyclic
/// DAG, item/explainer/exam validation. A structurally unusable set fails.
/// `figures` are the manifest-declared figures (already byte/hash-checked by
/// `bundle`). `lesson_json` maps lessonId → raw JSON string.
pub fn validate_set(
    mut set: LessonSet,
    figures: Vec<Figure>,
    lesson_json: &HashMap<String, String>,
) -> Result<ValidatedSet, ValidateError> {
    if set.schema_major != crate::lessons::SCHEMA_MAJOR {
        return Err("This lesson needs a newer version of noFriction.".into());
    }
    if !clean_in_place(&mut set.title, SHORT) {
        return Err("lesson set has no title".into());
    }
    if let Some(sub) = set.subtitle.as_mut() {
        clean_in_place(sub, SHORT);
    }
    clean_in_place(&mut set.level, SHORT);
    // Provenance accepted syntactically (never rejected for claiming one).
    if !matches!(set.provenance.as_str(), "seed" | "imported" | "generated") {
        return Err("lesson set has an unknown provenance".into());
    }
    if set.concepts.is_empty() || set.concepts.len() > MAX_CONCEPTS {
        return Err("lesson set concept count out of range".into());
    }
    if set.lessons.is_empty() || set.lessons.len() > MAX_LESSONS {
        return Err("lesson set lesson count out of range".into());
    }

    let concept_ids: HashSet<String> = set.concepts.iter().map(|c| c.id.clone()).collect();
    if concept_ids.len() != set.concepts.len() {
        return Err("duplicate concept id".into());
    }
    for c in &mut set.concepts {
        if !clean_in_place(&mut c.name, SHORT) {
            return Err("concept has no name".into());
        }
        clean_in_place(&mut c.summary, LONG);
    }
    validate_concept_dag(&set.concepts)?;

    let figure_ids: HashSet<String> = figures.iter().map(|f| f.id.clone()).collect();

    // Lessons, in the set's declared order; every referenced lesson must exist.
    let mut lessons = Vec::new();
    let lesson_order = set.lessons.clone();
    for lid in &lesson_order {
        let raw = lesson_json
            .get(lid)
            .ok_or_else(|| format!("lesson set references a missing lesson file {}", lid))?;
        let mut lesson = parse_lesson(raw)?;
        if &lesson.id != lid {
            return Err(format!("lesson file id mismatch for {}", lid));
        }
        validate_lesson(&mut lesson, &concept_ids, &figure_ids)?;
        lessons.push(lesson);
    }

    // Diagnostic (optional).
    if let Some(d) = set.diagnostic.as_mut() {
        let mut items = Vec::new();
        for mut it in std::mem::take(&mut d.items) {
            if validate_item(&mut it, &concept_ids).is_ok() {
                items.push(it);
            }
        }
        d.items = items;
        if d.items.is_empty() {
            set.diagnostic = None;
        }
    }

    validate_exam(&mut set.exam, &concept_ids)?;

    Ok(ValidatedSet { set, lessons, figures })
}
