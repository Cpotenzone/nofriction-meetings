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
// Per-kind exam-bank floors (docs/LESSONS.md §4.6/§10.1b): the bank must
// over-provision each kind well past the drawn mix so a retake is a different
// paper and an imported bundle cannot under-provision.
pub const MIN_EXAM_MC: u32 = 18;
pub const MIN_EXAM_NUMERIC: u32 = 18;
pub const MIN_EXAM_MULTISTEP: u32 = 9;
pub const MIN_WORKED_EXAMPLES: usize = 3;
pub const MAX_LESSONS: usize = 64;
pub const MAX_CONCEPTS: usize = 128;
/// A `text` explainer carries 1..=6 plain-text paragraphs.
pub const MAX_TEXT_PARAGRAPHS: usize = 6;

// ════════════════════════════════════════════════════════════════════════
// Validation profiles: authored courses vs lessons made from a recording
// ════════════════════════════════════════════════════════════════════════

/// Which floors a set is held to. The profile follows the set's provenance
/// (`Profile::for_provenance`); only the counts and allowed kinds differ.
/// Every other check (text cleaning, referential integrity, the acyclic
/// concept graph, MC answer indexes in range, upper bounds) applies to both.
///
/// - `Authored` (seed and imported bundles): the authored-course floors of
///   docs/LESSONS.md §4.6/§10.1b, including the owner-decision D4 figure
///   floor (≥1 decoding `figure` explainer per lesson), ≥3 worked examples
///   with a numeric `finalAnswer`, 2..=6 misconceptions, a checkpoint of
///   8..=10 drawn from a bank ≥ 2× that, and an exam bank ≥ 60 with per-kind
///   and per-concept quotas.
/// - `Generated` (provenance `generated`, made from a Class recording by
///   `lessons::generate`, §13): the model's numbers are never stored (§6
///   HIGH-2), so a generated lesson carries only content that needs no
///   closed-form recompute: `text` explainers (no figures — there is no
///   runtime image generation — and no interactive closed-form kinds),
///   worked examples whose steps are text (no `finalAnswer` number, no
///   step `expression`/`result`/`unit`), misconceptions, and
///   multiple-choice checkpoint/exam items whose answer is a resolved index.
///   The floors fit one recording: ≥1 explainer, 0..=6 worked examples,
///   0..=6 misconceptions, a checkpoint of ≥3 from a bank of ≥4, and an MC
///   exam of ≥4 items (its own questions plus the checkpoints', so one
///   lesson's checkpoint is always enough for an exam).
///
/// A hostile bundle that claims `generated` is still stored as `imported`
/// (§4.7); claiming it only buys the narrower generated content rules
/// (text and MC only), never a closed-form shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Authored,
    Generated,
}

impl Profile {
    pub fn for_provenance(provenance: &str) -> Profile {
        if provenance == "generated" {
            Profile::Generated
        } else {
            Profile::Authored
        }
    }
}

/// The only explainer kind a generated lesson may carry.
pub const GEN_EXPLAINER_KINDS: &[&str] = &["text"];
pub const GEN_MIN_EXPLAINERS: usize = 1;
pub const GEN_MAX_WORKED_EXAMPLES: usize = 6;
pub const GEN_MIN_PRESENT: usize = 3;
pub const GEN_MIN_CHECKPOINT_BANK: usize = 4;
pub const GEN_MIN_EXAM_BANK: usize = GEN_MIN_CHECKPOINT_BANK;
pub const GEN_MAX_LESSONS: usize = 6;

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
    // Plain-text explanation: `data = {"paragraphs": [String]}` (schema
    // minor 1). The kind a generated lesson teaches with.
    "text",
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
    /// The verified numeric key. Required for authored courses; never
    /// present in a generated lesson (a model's number is never stored, §6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_answer: Option<FinalAnswer>,
    /// The answer in words (schema minor 1): how a generated worked example
    /// ends ("So the cell enters S phase next.").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_text: Option<String>,
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
    validate_item(item, concepts, Profile::Authored)
}

/// Text-only steps: the `expression`/`result`/`unit` of a step are where a
/// number would be presented as a computed result, so a generated lesson
/// keeps only the narration (§6 HIGH-2).
fn text_only_steps(steps: &mut Vec<Step>) {
    steps.retain_mut(|s| clean_in_place(&mut s.text, MEDIUM));
    for s in steps.iter_mut() {
        s.expression = None;
        s.result = None;
        s.unit = None;
    }
}

pub(crate) fn validate_item(item: &mut QuizItem, concepts: &HashSet<String>, profile: Profile) -> Result<(), ValidateError> {
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
    if profile == Profile::Generated {
        // Multiple choice only: its key is a resolved index, never a number
        // the model computed. Everything a non-MC kind would carry is cleared
        // so only the cleaned MC structure is stored.
        if item.kind != "mc" {
            return Err("a generated lesson only has multiple-choice items".into());
        }
        text_only_steps(&mut item.solution);
        item.answer = None;
        item.unit = None;
        item.tolerance = None;
        item.sig_figs = None;
        item.equation = None;
        item.answer_coefficients = None;
        item.steps = None;
        item.gated = None;
        item.generator = None;
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
            // A declared answer unit must round-trip through the authoritative
            // grader's own units table, so an authored key can never again
            // declare a unit the grader would reject as "couldn't read that unit".
            if let Some(u) = item.unit.as_deref() {
                if !u.is_empty() && !crate::lessons::grade::unit_is_gradeable(u) {
                    return Err(format!("numeric item declares an ungradeable unit {}", u));
                }
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
                        // Same gradeable-unit round-trip guard as the item-level
                        // numeric path (a step unit the grader rejects is a build break).
                        if let Some(u) = s.unit.as_deref() {
                            if !u.is_empty() && !crate::lessons::grade::unit_is_gradeable(u) {
                                return Err(format!("multistep numeric step declares an ungradeable unit {}", u));
                            }
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

pub(crate) fn validate_explainer(
    e: &mut Explainer,
    concepts: &HashSet<String>,
    figure_ids: &HashSet<String>,
    profile: Profile,
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
    // A generated lesson never carries a figure (no runtime image
    // generation) or a closed-form interactive kind (its numbers would be
    // the model's, §6 HIGH-2): only plain text.
    if profile == Profile::Generated && !GEN_EXPLAINER_KINDS.contains(&e.kind.as_str()) {
        return Err(format!("a generated lesson can't carry a {} explainer", e.kind));
    }
    e.concept_ids.retain(|c| concepts.contains(c));
    if e.kind == "text" {
        // Plain-text paragraphs, each cleaned; nothing else in `data` is kept.
        let paragraphs: Vec<String> = e
            .data
            .get("paragraphs")
            .and_then(|p| p.as_array())
            .map(|ps| ps.iter().filter_map(|p| parse::clean_value(p, LONG)).take(MAX_TEXT_PARAGRAPHS).collect())
            .unwrap_or_default();
        if paragraphs.is_empty() {
            return Err("text explainer has no paragraphs".into());
        }
        e.data = serde_json::json!({ "paragraphs": paragraphs });
    } else if e.kind == "figure" {
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
    profile: Profile,
) -> Result<(), ValidateError> {
    let generated = profile == Profile::Generated;
    if !clean_in_place(&mut lesson.title, SHORT) {
        return Err("lesson has no title".into());
    }
    lesson.objectives.retain_mut(|o| clean_in_place(&mut o.text, MEDIUM) || !generated);
    for o in &mut lesson.objectives {
        o.concept_ids.retain(|c| concepts.contains(c));
    }
    lesson.concept_ids.retain(|c| concepts.contains(c));

    // Explainers: drop a bad one; keep count in bounds; enforce the figure
    // floor (authored courses only — a generated lesson has no figures).
    let mut kept = Vec::new();
    for mut e in std::mem::take(&mut lesson.explainers) {
        if validate_explainer(&mut e, concepts, figure_ids, profile).is_ok() {
            kept.push(e);
        }
    }
    lesson.explainers = kept;
    let min_explainers = if generated { GEN_MIN_EXPLAINERS } else { MIN_EXPLAINERS };
    if !generated {
        let figure_count = lesson.explainers.iter().filter(|e| e.kind == "figure").count();
        if figure_count < 1 {
            return Err(format!("lesson {} has no figure-kind explainer (D4 floor)", lesson.index));
        }
    }
    if lesson.explainers.len() < min_explainers || lesson.explainers.len() > MAX_EXPLAINERS {
        return Err(format!(
            "lesson {} has {} explainers (want {}..={})",
            lesson.index,
            lesson.explainers.len(),
            min_explainers,
            MAX_EXPLAINERS
        ));
    }

    // Worked examples: drop a bad one, enforce the floor, recompute-verify.
    let mut we = Vec::new();
    for mut w in std::mem::take(&mut lesson.worked_examples) {
        if validate_worked_example(&mut w, concepts, profile).is_ok() {
            we.push(w);
        }
    }
    lesson.worked_examples = we;
    if generated {
        if lesson.worked_examples.len() > GEN_MAX_WORKED_EXAMPLES {
            return Err(format!("lesson {} has too many worked examples", lesson.index));
        }
    } else if lesson.worked_examples.len() < MIN_WORKED_EXAMPLES {
        return Err(format!("lesson {} has too few worked examples", lesson.index));
    }

    // Misconceptions: 2..=6 (0..=6 for a generated lesson).
    lesson.misconceptions.items.retain_mut(|m| validate_misconception(m, concepts));
    let n = lesson.misconceptions.items.len();
    let min_misconceptions = if generated { 0 } else { MIN_MISCONCEPTIONS };
    if n < min_misconceptions || n > MAX_MISCONCEPTIONS {
        return Err(format!("lesson {} misconceptions out of range ({})", lesson.index, n));
    }

    // Trainer: validate static items; drop generators with an unknown template.
    // A generated lesson has no generator-backed drills.
    if generated {
        lesson.trainer.generators.clear();
    }
    lesson
        .trainer
        .generators
        .retain(|g| crate::lessons::generators::is_known_template(&g.template));
    let mut statics = Vec::new();
    for mut it in std::mem::take(&mut lesson.trainer.static_items) {
        if validate_item(&mut it, concepts, profile).is_ok() {
            statics.push(it);
        }
    }
    lesson.trainer.static_items = statics;

    // Checkpoint: authored present_count 8..=10 from a bank present_count*2
    // ..= 40; generated present_count 3..=10 from a bank of at least
    // max(present_count, 4) (one recording rarely supports 16+ questions).
    let pc = lesson.checkpoint.present_count as usize;
    let pc_range = if generated { GEN_MIN_PRESENT..=10 } else { 8..=10 };
    if !pc_range.contains(&pc) {
        return Err(format!("lesson {} checkpoint presentCount out of range", lesson.index));
    }
    let mut items = Vec::new();
    for mut it in std::mem::take(&mut lesson.checkpoint.items) {
        if validate_item(&mut it, concepts, profile).is_ok() {
            items.push(it);
        }
    }
    lesson.checkpoint.items = items;
    let bank = lesson.checkpoint.items.len();
    let min_bank = if generated { pc.max(GEN_MIN_CHECKPOINT_BANK) } else { pc * 2 };
    if bank < min_bank || bank > MAX_CHECKPOINT_BANK {
        return Err(format!(
            "lesson {} checkpoint bank {} not in {}..={}",
            lesson.index, bank, min_bank, MAX_CHECKPOINT_BANK
        ));
    }
    Ok(())
}

/// Clean one "common mistake" in place; false → drop it.
pub(crate) fn validate_misconception(m: &mut MisconceptionItem, concepts: &HashSet<String>) -> bool {
    let ok = clean_in_place(&mut m.claim, MEDIUM)
        && clean_in_place(&mut m.why_wrong, MEDIUM)
        && clean_in_place(&mut m.correct, MEDIUM);
    m.concept_ids.retain(|c| concepts.contains(c));
    ok
}

pub(crate) fn validate_worked_example(
    w: &mut WorkedExample,
    concepts: &HashSet<String>,
    profile: Profile,
) -> Result<(), ValidateError> {
    if !clean_in_place(&mut w.prompt, MEDIUM) {
        return Err("worked example has no prompt".into());
    }
    w.concept_ids.retain(|c| concepts.contains(c));
    if let Some(t) = w.answer_text.as_mut() {
        if !clean_in_place(t, MEDIUM) {
            w.answer_text = None;
        }
    }
    if profile == Profile::Generated {
        // Steps are text; a model's number is never stored as the key (§6).
        text_only_steps(&mut w.steps);
        w.final_answer = None;
        if w.steps.is_empty() {
            return Err("worked example has no steps".into());
        }
        return Ok(());
    }
    for s in &mut w.steps {
        clean_in_place(&mut s.text, MEDIUM);
    }
    if w.steps.is_empty() {
        return Err("worked example has no steps".into());
    }
    let value = w.final_answer.as_ref().map(|f| f.value).ok_or("worked example has no final answer")?;
    if !finite(value) {
        return Err("worked example final answer not finite".into());
    }
    Ok(())
}

fn validate_exam(exam: &mut Exam, concepts: &HashSet<String>, profile: Profile) -> Result<(), ValidateError> {
    let mut items = Vec::new();
    for mut it in std::mem::take(&mut exam.items) {
        if validate_item(&mut it, concepts, profile).is_ok() {
            items.push(it);
        }
    }
    exam.items = items;
    let n = exam.items.len();
    if profile == Profile::Generated {
        // A generated exam is multiple choice, drawn from a bank of >= 4
        // (its own questions plus the lessons' checkpoint questions).
        if !(GEN_MIN_EXAM_BANK..=MAX_EXAM_BANK).contains(&n) {
            return Err(format!("exam bank {} not in {}..={}", n, GEN_MIN_EXAM_BANK, MAX_EXAM_BANK));
        }
        let m = &exam.mix;
        if m.numeric != 0 || m.balance != 0 || m.multistep != 0 || m.mc != exam.question_count {
            return Err("a generated exam is multiple choice only (mix.mc == questionCount)".into());
        }
        if (exam.question_count as usize) < GEN_MIN_EXAM_BANK || exam.question_count as usize > n {
            return Err("exam questionCount out of range for its bank".into());
        }
        if exam.pass_pct == 0 || exam.pass_pct > 100 {
            return Err("exam passPct out of range".into());
        }
        return Ok(());
    }
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
    // Per-kind supply must cover the drawn mix AND clear the bank floors, so a
    // retake draws a genuinely different paper and an imported bundle cannot
    // under-provision (§4.6/§10.1b).
    let count_kind = |k: &str| exam.items.iter().filter(|i| i.kind == k).count() as u32;
    let (mc_n, num_n, ms_n) = (count_kind("mc"), count_kind("numeric"), count_kind("multistep"));
    if mc_n < m.mc || num_n < m.numeric || ms_n < m.multistep {
        return Err("exam bank cannot supply the mix".into());
    }
    if mc_n < MIN_EXAM_MC || num_n < MIN_EXAM_NUMERIC || ms_n < MIN_EXAM_MULTISTEP {
        return Err(format!(
            "exam bank under-provisioned: mc {} (>= {}), numeric {} (>= {}), multistep {} (>= {})",
            mc_n, MIN_EXAM_MC, num_n, MIN_EXAM_NUMERIC, ms_n, MIN_EXAM_MULTISTEP
        ));
    }
    // Per-concept exam coverage: every concept the exam items tag must carry at
    // least two items so a drawn exam can reach each concept.
    let mut per_concept: HashMap<&str, u32> = HashMap::new();
    for it in &exam.items {
        for cid in &it.concept_ids {
            *per_concept.entry(cid.as_str()).or_insert(0) += 1;
        }
    }
    if let Some((cid, _)) = per_concept.iter().find(|(_, &n)| n < 2) {
        return Err(format!("exam concept {} has fewer than 2 items", cid));
    }
    Ok(())
}

/// Detect a cycle in the concept prerequisite graph (DFS). Returns Err on a
/// cycle or a prerequisite referencing a missing concept.
pub(crate) fn validate_concept_dag(concepts: &[Concept]) -> Result<(), ValidateError> {
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
    let profile = Profile::for_provenance(&set.provenance);
    if set.concepts.is_empty() || set.concepts.len() > MAX_CONCEPTS {
        return Err("lesson set concept count out of range".into());
    }
    let max_lessons = if profile == Profile::Generated { GEN_MAX_LESSONS } else { MAX_LESSONS };
    if set.lessons.is_empty() || set.lessons.len() > max_lessons {
        return Err("lesson set lesson count out of range".into());
    }
    // There is no runtime image generation, so a generated set never has
    // figures (docs/LESSONS.md §13, OQ-7).
    if profile == Profile::Generated && !figures.is_empty() {
        return Err("a generated lesson set can't carry figures".into());
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
        validate_lesson(&mut lesson, &concept_ids, &figure_ids, profile)?;
        lessons.push(lesson);
    }

    // Diagnostic (optional; a generated set has none).
    if profile == Profile::Generated {
        set.diagnostic = None;
    }
    if let Some(d) = set.diagnostic.as_mut() {
        let mut items = Vec::new();
        for mut it in std::mem::take(&mut d.items) {
            if validate_item(&mut it, &concept_ids, profile).is_ok() {
                items.push(it);
            }
        }
        d.items = items;
        if d.items.is_empty() {
            set.diagnostic = None;
        }
    }

    validate_exam(&mut set.exam, &concept_ids, profile)?;

    Ok(ValidatedSet { set, lessons, figures })
}
