//! Model answer → lesson parts (docs/LESSONS.md §13). Every answer is
//! untrusted: it is parsed tolerantly (think blocks, code fences, prose
//! around it, trailing commas — `study::parse::extract_json`), every field
//! is cleaned, references are resolved against the concepts the lesson
//! actually has, and each element then goes through the strict lessons
//! validators (`schema::validate_*`, generated profile) before it is kept.
//! Only the cleaned structured values are returned; nothing is logged.
//!
//! Repairs (never stored broken):
//! - a concept graph with a cycle loses the edges that close it; a
//!   prerequisite naming no concept is dropped (the concept stays);
//! - a reference to an unknown concept is dropped; an element left with no
//!   concept is tagged with its lesson's concepts;
//! - a multiple-choice answer given as a letter or as the choice's text is
//!   resolved into a 0-based `answerIndex` (`study::parse::answer_index`).
//!
//! Anything that can't be repaired (no concepts, no lessons, too few usable
//! questions) is an error for that part, which is asked for once more.

use crate::lessons::schema::{self, Explainer, MisconceptionItem, Profile, QuizItem, Step, WorkedExample};
use crate::study::parse::{answer_index, clean_text, extract_json, norm_key};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub type ParseError = String;

// Text caps (characters), the same scale as the lessons schema.
const SHORT: usize = 160;
const MEDIUM: usize = 400;
const LONG: usize = 900;

/// Most concepts a generated set keeps.
pub const MAX_CONCEPTS: usize = 16;
/// Most lessons a generated set is split into.
pub const MAX_LESSONS: usize = 4;
pub const MAX_OBJECTIVES: usize = 5;
pub const MAX_EXPLANATIONS: usize = schema::MAX_EXPLAINERS;
pub const MAX_WORKED_EXAMPLES: usize = 4;
pub const MAX_STEPS: usize = 8;
pub const MAX_QUESTIONS: usize = 15;
/// Fewest usable questions a checkpoint answer must have.
pub const MIN_CHECKPOINT_QUESTIONS: usize = schema::GEN_MIN_CHECKPOINT_BANK;

// ── Helpers ─────────────────────────────────────────────────────────────

fn field<'a>(o: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| o.get(*n)).filter(|v| !v.is_null())
}

fn text_field(o: &Map<String, Value>, names: &[&str], max: usize) -> Option<String> {
    field(o, names).and_then(|v| clean_text(v, max))
}

/// The list in an answer: the object's `keys` array, or a bare array.
fn list<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    match v {
        Value::Array(a) => Some(a),
        Value::Object(o) => field(o, keys).and_then(|x| x.as_array()),
        _ => None,
    }
}

/// Strings in a list field (or a single string), each cleaned.
fn strings(v: Option<&Value>, max: usize, limit: usize) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| match x {
                Value::Object(o) => text_field(o, &["text", "step", "body", "paragraph"], max),
                other => clean_text(other, max),
            })
            .take(limit)
            .collect(),
        Some(other) => clean_text(other, max).into_iter().collect(),
        None => Vec::new(),
    }
}

// ── Concepts (the DAG) ──────────────────────────────────────────────────

/// One concept as the generator holds it before it becomes a
/// `schema::Concept`: `id` is ours ("c1", "c2", … in teaching order).
#[derive(Debug, Clone, PartialEq)]
pub struct ConceptDraft {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub prerequisites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptsDraft {
    pub title: Option<String>,
    pub concepts: Vec<ConceptDraft>,
    /// Edges dropped to repair the graph (cycles, self-loops, unknown ids)
    pub repaired_edges: usize,
}

/// Resolves what the model calls a concept (our id, its own id, or the
/// concept's name) to our id.
#[derive(Debug, Clone, Default)]
pub struct ConceptLookup {
    by_key: HashMap<String, String>,
}

impl ConceptLookup {
    pub fn new(concepts: &[ConceptDraft]) -> Self {
        let mut l = ConceptLookup::default();
        for c in concepts {
            l.by_key.entry(norm_key(&c.id)).or_insert_with(|| c.id.clone());
            l.by_key.entry(norm_key(&c.name)).or_insert_with(|| c.id.clone());
        }
        l
    }

    fn add(&mut self, key: &str, id: &str) {
        let k = norm_key(key);
        if !k.is_empty() {
            self.by_key.entry(k).or_insert_with(|| id.to_string());
        }
    }

    pub fn resolve(&self, key: &str) -> Option<&str> {
        self.by_key.get(&norm_key(key)).map(String::as_str)
    }

    /// The known concept ids a field names, in order, without repeats,
    /// limited to `allowed`.
    pub fn refs(&self, v: Option<&Value>, allowed: &[String]) -> Vec<String> {
        let keys: Vec<String> = match v {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect(),
            Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_string()).collect(),
            _ => Vec::new(),
        };
        let mut out: Vec<String> = Vec::new();
        for k in keys {
            if let Some(id) = self.resolve(&k) {
                if allowed.iter().any(|a| a == id) && !out.iter().any(|o| o == id) {
                    out.push(id.to_string());
                }
            }
        }
        out
    }
}

/// Whether `to` can be reached from `from` along prerequisite edges.
fn reaches(adj: &[Vec<usize>], from: usize, to: usize) -> bool {
    let mut stack = vec![from];
    let mut seen = vec![false; adj.len()];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if std::mem::replace(&mut seen[n], true) {
            continue;
        }
        stack.extend(adj[n].iter().copied());
    }
    false
}

/// Repair the prerequisite graph into a DAG by dropping the edges that close
/// a cycle. A prerequisite taught earlier (the shape the prompt asks for) is
/// always kept: those edges alone can't form a cycle. A prerequisite taught
/// later is kept unless it would close a cycle, so in "A needs B, B needs A"
/// the edge that points forward in teaching order is the one dropped.
/// Unknown ids and self-loops are dropped too. Returns how many edges were
/// dropped.
pub fn break_cycles(concepts: &mut [ConceptDraft]) -> usize {
    let index: HashMap<String, usize> = concepts.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
    let n = concepts.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut forward: Vec<(usize, usize)> = Vec::new();
    let mut dropped = 0usize;
    for (i, c) in concepts.iter().enumerate() {
        for p in &c.prerequisites {
            match index.get(p) {
                Some(&j) if j < i => {
                    if !adj[i].contains(&j) {
                        adj[i].push(j);
                    }
                }
                Some(&j) if j > i => forward.push((i, j)),
                _ => dropped += 1, // unknown, or itself
            }
        }
    }
    for (i, j) in forward {
        if adj[i].contains(&j) {
            continue;
        }
        if reaches(&adj, j, i) {
            dropped += 1; // i needs j, but j already (indirectly) needs i
        } else {
            adj[i].push(j);
        }
    }
    for (i, c) in concepts.iter_mut().enumerate() {
        let mut keep: Vec<String> = Vec::new();
        for p in &c.prerequisites {
            if index.get(p).is_some_and(|j| adj[i].contains(j)) && !keep.contains(p) {
                keep.push(p.clone());
            }
        }
        c.prerequisites = keep;
    }
    dropped
}

/// The concepts part: `{"title", "concepts": [{"id","name","summary","prerequisites"}]}`.
pub fn concepts(raw: &str) -> Result<ConceptsDraft, ParseError> {
    let v = extract_json(raw)?;
    let title = v.as_object().and_then(|o| text_field(o, &["title", "course", "course_title", "topic"], SHORT));
    let items = list(&v, &["concepts", "topics"]).ok_or("No concepts in the answer")?;
    let mut drafts: Vec<ConceptDraft> = Vec::new();
    // The model's own keys → our ids, and the raw prerequisite keys per concept
    let mut lookup = ConceptLookup::default();
    let mut raw_prereqs: Vec<Vec<String>> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let Some(name) = text_field(o, &["name", "concept", "title", "term"], SHORT) else { continue };
        let key = field(o, &["id", "key"]).and_then(|k| match k {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        if !seen_names.insert(norm_key(&name)) {
            // The same concept twice: point its key at the first one
            if let (Some(k), Some(first)) = (key.as_deref(), lookup.resolve(&name).map(str::to_string)) {
                lookup.add(k, &first);
            }
            continue;
        }
        if drafts.len() == MAX_CONCEPTS {
            break;
        }
        let id = format!("c{}", drafts.len() + 1);
        lookup.add(&id, &id);
        if let Some(k) = key.as_deref() {
            lookup.add(k, &id);
        }
        lookup.add(&name, &id);
        let summary = text_field(o, &["summary", "definition", "description"], LONG).unwrap_or_default();
        let prereqs: Vec<String> = match field(o, &["prerequisites", "prereqs", "depends_on", "builds_on", "requires"]) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect(),
            Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
            _ => Vec::new(),
        };
        raw_prereqs.push(prereqs);
        drafts.push(ConceptDraft { id, name, summary, prerequisites: Vec::new() });
    }
    if drafts.is_empty() {
        return Err("The answer had no usable concepts".into());
    }
    // Resolve prerequisites: unknown names and self-loops are dropped edges
    let mut repaired = 0usize;
    for (i, keys) in raw_prereqs.into_iter().enumerate() {
        let mut resolved: Vec<String> = Vec::new();
        for k in keys {
            match lookup.resolve(&k) {
                Some(id) if id != drafts[i].id => {
                    if !resolved.iter().any(|r| r == id) {
                        resolved.push(id.to_string());
                    }
                }
                _ => repaired += 1,
            }
        }
        drafts[i].prerequisites = resolved;
    }
    repaired += break_cycles(&mut drafts);
    Ok(ConceptsDraft { title, concepts: drafts, repaired_edges: repaired })
}

// ── Outline (lessons + objectives) ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct LessonOutline {
    pub title: String,
    pub objectives: Vec<String>,
    /// Our concept ids, each in exactly one lesson
    pub concept_ids: Vec<String>,
}

/// The outline part: `{"lessons": [{"title","objectives","concepts"}]}`.
/// A concept listed twice stays in its first lesson; a concept no lesson
/// lists joins the last lesson, so every concept is taught; a lesson left
/// with no concept is dropped.
pub fn outline(raw: &str, concepts: &[ConceptDraft]) -> Result<Vec<LessonOutline>, ParseError> {
    let v = extract_json(raw)?;
    let items = list(&v, &["lessons", "outline", "plan"]).ok_or("No lessons in the answer")?;
    let lookup = ConceptLookup::new(concepts);
    let all: Vec<String> = concepts.iter().map(|c| c.id.clone()).collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut out: Vec<LessonOutline> = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let Some(title) = text_field(o, &["title", "name", "lesson"], SHORT) else { continue };
        let objectives = strings(field(o, &["objectives", "goals", "learning_objectives"]), MEDIUM, MAX_OBJECTIVES);
        let ids: Vec<String> = lookup
            .refs(field(o, &["concepts", "concept_ids", "conceptIds"]), &all)
            .into_iter()
            .filter(|id| taken.insert(id.clone()))
            .collect();
        if ids.is_empty() {
            continue;
        }
        if out.len() == MAX_LESSONS {
            // More lessons than we make: fold the rest into the last one
            out.last_mut().unwrap().concept_ids.extend(ids);
            continue;
        }
        out.push(LessonOutline { title, objectives, concept_ids: ids });
    }
    if out.is_empty() {
        return Err("The answer had no usable lessons".into());
    }
    let missing: Vec<String> = all.into_iter().filter(|id| !taken.contains(id)).collect();
    out.last_mut().unwrap().concept_ids.extend(missing);
    // Keep each lesson's concepts in teaching order
    for l in &mut out {
        l.concept_ids.sort_by_key(|id| concepts.iter().position(|c| &c.id == id).unwrap_or(usize::MAX));
    }
    Ok(out)
}

// ── Per-lesson parts ────────────────────────────────────────────────────

/// What a lesson's parts may reference.
pub struct LessonScope<'a> {
    /// The lesson's id ("l1"), the prefix of its elements' ids
    pub lesson_id: &'a str,
    /// The lesson's concepts (refs are limited to these; the fallback tag)
    pub concept_ids: &'a [String],
    pub lookup: &'a ConceptLookup,
}

impl LessonScope<'_> {
    fn tags(&self, v: Option<&Value>) -> Vec<String> {
        let refs = self.lookup.refs(v, self.concept_ids);
        if refs.is_empty() {
            self.concept_ids.to_vec()
        } else {
            refs
        }
    }

    fn known(&self) -> HashSet<String> {
        self.concept_ids.iter().cloned().collect()
    }
}

/// Explanations → `text` explainers.
pub fn explanations(raw: &str, scope: &LessonScope) -> Result<Vec<Explainer>, ParseError> {
    let v = extract_json(raw)?;
    let items = list(&v, &["explanations", "sections", "explainers"]).ok_or("No explanations in the answer")?;
    let known = scope.known();
    let mut out = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let Some(title) = text_field(o, &["title", "heading", "name"], SHORT) else { continue };
        let paragraphs = strings(field(o, &["paragraphs", "text", "body", "content"]), LONG, schema::MAX_TEXT_PARAGRAPHS);
        let mut e = Explainer {
            id: format!("{}-x{}", scope.lesson_id, out.len() + 1),
            title,
            concept_ids: scope.tags(field(o, &["concepts", "concept_ids", "conceptIds"])),
            caption: String::new(),
            a11y_state: None,
            kind: "text".into(),
            data: json!({ "paragraphs": paragraphs }),
        };
        if schema::validate_explainer(&mut e, &known, &HashSet::new(), Profile::Generated).is_ok() {
            out.push(e);
        }
        if out.len() == MAX_EXPLANATIONS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The answer had no usable explanations".into());
    }
    Ok(out)
}

/// Worked examples whose steps are text. No `finalAnswer` number is ever
/// taken from the model; the answer stays words (`answerText`). An empty
/// list is a valid answer (a lecture with nothing to work through).
pub fn worked_examples(raw: &str, scope: &LessonScope) -> Result<Vec<WorkedExample>, ParseError> {
    let v = extract_json(raw)?;
    let items = list(&v, &["examples", "worked_examples", "workedExamples", "problems"])
        .ok_or("No examples in the answer")?;
    let known = scope.known();
    let mut out = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let Some(prompt) = text_field(o, &["prompt", "problem", "question", "title"], MEDIUM) else { continue };
        let steps: Vec<Step> = strings(field(o, &["steps", "solution"]), MEDIUM, MAX_STEPS)
            .into_iter()
            .map(|text| Step { text, expression: None, result: None, unit: None })
            .collect();
        let mut w = WorkedExample {
            id: format!("{}-w{}", scope.lesson_id, out.len() + 1),
            prompt,
            concept_ids: scope.tags(field(o, &["concepts", "concept_ids", "conceptIds"])),
            steps,
            final_answer: None,
            answer_text: text_field(o, &["answer", "result", "conclusion", "final_answer"], MEDIUM),
        };
        if schema::validate_worked_example(&mut w, &known, Profile::Generated).is_ok() {
            out.push(w);
        }
        if out.len() == MAX_WORKED_EXAMPLES {
            break;
        }
    }
    if out.is_empty() && !items.is_empty() {
        return Err("The answer had no usable worked examples".into());
    }
    Ok(out)
}

/// Common mistakes (the misconceptions cards).
pub fn mistakes(raw: &str, scope: &LessonScope) -> Result<Vec<MisconceptionItem>, ParseError> {
    let v = extract_json(raw)?;
    let items = list(&v, &["mistakes", "misconceptions", "common_mistakes"]).ok_or("No mistakes in the answer")?;
    let known = scope.known();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let (Some(claim), Some(why_wrong), Some(correct)) = (
            text_field(o, &["claim", "mistake", "wrong", "misconception"], MEDIUM),
            text_field(o, &["why_wrong", "whyWrong", "why", "reason"], MEDIUM),
            text_field(o, &["correct", "right", "correct_idea", "fix"], MEDIUM),
        ) else {
            continue;
        };
        if !seen.insert(norm_key(&claim)) {
            continue;
        }
        let mut m = MisconceptionItem {
            claim,
            why_wrong,
            correct,
            concept_ids: scope.tags(field(o, &["concepts", "concept_ids", "conceptIds"])),
        };
        if schema::validate_misconception(&mut m, &known) {
            out.push(m);
        }
        if out.len() == schema::MAX_MISCONCEPTIONS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The answer had no usable mistakes".into());
    }
    Ok(out)
}

/// Multiple-choice questions (a checkpoint or the exam). Each answer is
/// resolved into a stored 0-based index from an index, a letter or the
/// choice's text; a question whose answer can't be resolved is dropped.
pub fn questions(
    raw: &str,
    id_prefix: &str,
    allowed: &[String],
    lookup: &ConceptLookup,
    min: usize,
) -> Result<Vec<QuizItem>, ParseError> {
    let v = extract_json(raw)?;
    let items = list(&v, &["questions", "quiz", "items", "checkpoint", "exam"]).ok_or("No questions in the answer")?;
    let known: HashSet<String> = allowed.iter().cloned().collect();
    let mut seen = HashSet::new();
    let mut out: Vec<QuizItem> = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let Some(prompt) = text_field(o, &["question", "q", "prompt"], MEDIUM) else { continue };
        let Some(raw_choices) = field(o, &["choices", "options", "answers"]).and_then(|c| c.as_array()) else { continue };
        let choices: Vec<String> = raw_choices.iter().filter_map(|c| clean_text(c, SHORT)).collect();
        if choices.len() != raw_choices.len()
            || choices.len() < schema::MIN_CHOICES
            || choices.len() > schema::MAX_CHOICES
        {
            continue;
        }
        let unique: HashSet<String> = choices.iter().map(|c| norm_key(c)).collect();
        if unique.len() != choices.len() {
            continue;
        }
        let Some(idx) = field(o, &["answer", "correct", "correct_index", "answer_index", "answerIndex"])
            .and_then(|a| answer_index(a, &choices))
        else {
            continue;
        };
        if !seen.insert(norm_key(&prompt)) {
            continue;
        }
        let refs = lookup.refs(field(o, &["concepts", "concept_ids", "conceptIds"]), allowed);
        let mut q = QuizItem {
            id: format!("{}{}", id_prefix, out.len() + 1),
            concept_ids: if refs.is_empty() { allowed.to_vec() } else { refs },
            kind: "mc".into(),
            prompt,
            solution: Vec::new(),
            explanation: text_field(o, &["explanation", "why", "reason"], MEDIUM).unwrap_or_default(),
            choices: Some(choices),
            answer_index: Some(idx as u32),
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
        if schema::validate_item(&mut q, &known, Profile::Generated).is_ok() {
            out.push(q);
        }
        if out.len() == MAX_QUESTIONS {
            break;
        }
    }
    if out.len() < min {
        return Err(if out.is_empty() {
            "The answer had no usable questions".to_string()
        } else {
            format!("Only {} usable question{} (need {})", out.len(), if out.len() == 1 { "" } else { "s" }, min)
        });
    }
    Ok(out)
}
