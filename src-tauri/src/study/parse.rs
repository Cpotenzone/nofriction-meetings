//! Model output → validated study material. The model's answer is
//! untrusted: it is parsed as JSON, every field is checked and cleaned, and
//! only the cleaned values are stored and shown (as text, never HTML).
//! Nothing here panics on malformed input, and nothing here is logged.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// The five parts of a study guide. Stored as `study_materials.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
    Summary,
    Terms,
    Flashcards,
    Quiz,
    Questions,
}

impl StudyKind {
    pub const ALL: [StudyKind; 5] =
        [StudyKind::Summary, StudyKind::Terms, StudyKind::Flashcards, StudyKind::Quiz, StudyKind::Questions];

    pub fn as_str(self) -> &'static str {
        match self {
            StudyKind::Summary => "summary",
            StudyKind::Terms => "terms",
            StudyKind::Flashcards => "flashcards",
            StudyKind::Quiz => "quiz",
            StudyKind::Questions => "questions",
        }
    }

    pub fn parse(s: &str) -> Option<StudyKind> {
        StudyKind::ALL.iter().copied().find(|k| k.as_str() == s.trim())
    }

    /// For progress and error messages
    pub fn label(self) -> &'static str {
        match self {
            StudyKind::Summary => "summary",
            StudyKind::Terms => "key terms",
            StudyKind::Flashcards => "flashcards",
            StudyKind::Quiz => "practice quiz",
            StudyKind::Questions => "questions to ask",
        }
    }
}

// Limits: enough for a long lecture, small enough that a runaway answer
// can't flood the UI or the database.
pub const MAX_SECTIONS: usize = 12;
pub const MAX_BULLETS: usize = 10;
pub const MAX_TERMS: usize = 40;
pub const MAX_CARDS: usize = 60;
pub const MAX_QUIZ: usize = 20;
pub const MAX_QUESTIONS: usize = 20;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 6;
const SHORT: usize = 160;
const MEDIUM: usize = 400;
const LONG: usize = 600;

/// Why an answer couldn't be used (shown to the user; never contains the answer).
pub type ParseError = String;

// ── JSON extraction ─────────────────────────────────────────────────────

/// Remove `<think>…</think>` blocks (an unclosed one runs to the end).
fn strip_think(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(a) = out.find("<think>") {
        match out[a..].find("</think>") {
            Some(b) => out.replace_range(a..a + b + "</think>".len(), ""),
            None => out.truncate(a),
        }
    }
    out
}

/// The first complete JSON object or array in `s` (string-aware bracket
/// matching), so prose or code fences around it don't matter.
fn first_json_value(s: &str) -> Option<&str> {
    let start = s.find(|c| c == '{' || c == '[')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in s[start..].char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Trailing commas (`[1, 2,]`) are the most common small-model JSON slip.
/// Only applied after a strict parse failed. String-aware.
fn remove_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in chars.iter().enumerate() {
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        if c == ',' {
            let next = chars[i + 1..].iter().find(|x| !x.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Parse the JSON in a model answer, tolerating think blocks, code fences,
/// surrounding prose and trailing commas.
pub fn extract_json(raw: &str) -> Result<Value, ParseError> {
    let text = strip_think(raw);
    let candidate = first_json_value(&text).ok_or("The answer had no JSON in it")?;
    serde_json::from_str::<Value>(candidate)
        .or_else(|_| serde_json::from_str::<Value>(&remove_trailing_commas(candidate)))
        .map_err(|_| "The answer's JSON couldn't be read".to_string())
}

// ── Cleaning ────────────────────────────────────────────────────────────

/// Trimmed, single-line, no control characters, leading list markers
/// removed, at most `max` characters (cut with "…"). Empty → None.
pub fn clean_text(v: &Value, max: usize) -> Option<String> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let s: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // "- point", "* point", "• point", "1. point", "1) point" (a marker
    // followed by a space; "**bold**" and "-5" are text)
    loop {
        let t = s.as_str();
        let t = match t.chars().next() {
            Some(c @ ('-' | '*' | '•' | '–')) if t[c.len_utf8()..].starts_with(' ') => t[c.len_utf8()..].trim_start(),
            _ => {
                let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
                if digits > 0 && digits <= 2 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") ")) {
                    t[digits + 1..].trim_start()
                } else {
                    t
                }
            }
        };
        if t.len() == s.len() {
            break;
        }
        s = t.to_string();
    }
    if s.is_empty() {
        return None;
    }
    if s.chars().count() > max {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        s = format!("{}…", cut.trim_end());
    }
    Some(s)
}

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

/// "12:34", "1:02:03", "[12:34]", "12:34.5" or a number of seconds → ms.
pub fn parse_clock(s: &str) -> Option<i64> {
    let t = s.trim().trim_start_matches('[').trim_end_matches(']').trim();
    if t.is_empty() {
        return None;
    }
    if !t.contains(':') {
        let secs: f64 = t.trim_end_matches('s').trim().parse().ok()?;
        return (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as i64);
    }
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.trim().is_empty()) {
        return None;
    }
    let mut total = 0f64;
    for (i, p) in parts.iter().enumerate() {
        let n: f64 = p.trim().parse().ok()?;
        if !n.is_finite() || n < 0.0 || (i > 0 && n >= 60.0) {
            return None;
        }
        total = total * 60.0 + n;
    }
    Some((total * 1000.0).round() as i64)
}

/// A source time from the answer, within the lecture (a minute of slack).
fn time_field(o: &Map<String, Value>, duration_ms: i64) -> Option<i64> {
    // Stored material (already cleaned) keeps ms
    let ms = if let Some(ms) = o.get("at_ms").and_then(|v| v.as_i64()) {
        ms
    } else {
        match field(o, &["time", "t", "timestamp", "at", "source_time"])? {
            Value::String(s) => parse_clock(s)?,
            Value::Number(n) => (n.as_f64()? * 1000.0).round() as i64,
            _ => return None,
        }
    };
    (ms >= 0 && (duration_ms <= 0 || ms <= duration_ms + 60_000)).then_some(ms)
}

fn norm_key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

// ── Per kind ────────────────────────────────────────────────────────────

fn summary(v: &Value) -> Result<Value, ParseError> {
    let title = v.as_object().and_then(|o| text_field(o, &["title", "topic"], SHORT));
    let sections = list(v, &["sections", "topics", "notes"]).ok_or("No sections in the summary")?;
    let mut out = Vec::new();
    for s in sections.iter().take(MAX_SECTIONS * 2) {
        let Some(o) = s.as_object() else { continue };
        let Some(heading) = text_field(o, &["heading", "title", "topic", "name"], SHORT) else { continue };
        let bullets: Vec<String> = field(o, &["bullets", "points", "notes", "items"])
            .and_then(|b| b.as_array())
            .map(|b| b.iter().filter_map(|x| clean_text(x, MEDIUM)).take(MAX_BULLETS).collect())
            .unwrap_or_default();
        if bullets.is_empty() {
            continue;
        }
        out.push(json!({ "heading": heading, "bullets": bullets }));
        if out.len() == MAX_SECTIONS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The summary had no usable sections".into());
    }
    Ok(json!({ "title": title, "sections": out }))
}

fn terms(v: &Value) -> Result<Value, ParseError> {
    let items = list(v, &["terms", "key_terms", "glossary"]).ok_or("No terms in the answer")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in items {
        let Some(o) = t.as_object() else { continue };
        let (Some(term), Some(def)) =
            (text_field(o, &["term", "word", "name"], SHORT), text_field(o, &["definition", "meaning", "def"], LONG))
        else {
            continue;
        };
        if seen.insert(norm_key(&term)) {
            out.push(json!({ "term": term, "definition": def }));
        }
        if out.len() == MAX_TERMS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The answer had no usable terms".into());
    }
    Ok(json!({ "terms": out }))
}

fn flashcards(v: &Value) -> Result<Value, ParseError> {
    let items = list(v, &["cards", "flashcards"]).ok_or("No cards in the answer")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in items {
        let Some(o) = c.as_object() else { continue };
        let (Some(front), Some(back)) =
            (text_field(o, &["front", "question", "q", "term"], MEDIUM), text_field(o, &["back", "answer", "a", "definition"], LONG))
        else {
            continue;
        };
        if norm_key(&front) == norm_key(&back) {
            continue;
        }
        if seen.insert(norm_key(&front)) {
            out.push(json!({ "front": front, "back": back }));
        }
        if out.len() == MAX_CARDS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The answer had no usable flashcards".into());
    }
    Ok(json!({ "cards": out }))
}

/// The correct choice: a 0-based index, a letter ("B", "b)"), or the
/// choice's own text.
fn answer_index(v: &Value, choices: &[String]) -> Option<usize> {
    match v {
        Value::Number(n) => n.as_u64().map(|i| i as usize).filter(|i| *i < choices.len()),
        Value::String(s) => {
            let t = s.trim();
            let letter = t.trim_end_matches(|c| c == ')' || c == '.' || c == ':').trim();
            if letter.chars().count() == 1 {
                let c = letter.chars().next()?.to_ascii_uppercase();
                if c.is_ascii_uppercase() {
                    let i = (c as u8 - b'A') as usize;
                    return (i < choices.len()).then_some(i);
                }
                if let Some(d) = c.to_digit(10) {
                    return ((d as usize) < choices.len()).then_some(d as usize);
                }
            }
            let key = norm_key(t);
            choices.iter().position(|c| norm_key(c) == key)
        }
        _ => None,
    }
}

fn quiz(v: &Value, duration_ms: i64) -> Result<Value, ParseError> {
    let items = list(v, &["questions", "quiz"]).ok_or("No questions in the quiz")?;
    let mut out = Vec::new();
    for q in items {
        let Some(o) = q.as_object() else { continue };
        let Some(question) = text_field(o, &["question", "q", "prompt"], MEDIUM) else { continue };
        let Some(raw) = field(o, &["choices", "options", "answers"]).and_then(|c| c.as_array()) else { continue };
        let choices: Vec<String> = raw.iter().filter_map(|c| clean_text(c, SHORT)).collect();
        if choices.len() != raw.len() || choices.len() < MIN_CHOICES || choices.len() > MAX_CHOICES {
            continue;
        }
        let unique: std::collections::HashSet<String> = choices.iter().map(|c| norm_key(c)).collect();
        if unique.len() != choices.len() {
            continue;
        }
        let Some(answer) =
            field(o, &["answer", "correct", "correct_index", "answer_index"]).and_then(|a| answer_index(a, &choices))
        else {
            continue;
        };
        let explanation = text_field(o, &["explanation", "why", "reason"], MEDIUM).unwrap_or_default();
        out.push(json!({
            "question": question,
            "choices": choices,
            "answer": answer,
            "explanation": explanation,
            "at_ms": time_field(o, duration_ms),
        }));
        if out.len() == MAX_QUIZ {
            break;
        }
    }
    if out.is_empty() {
        return Err("The quiz had no usable questions".into());
    }
    Ok(json!({ "questions": out }))
}

fn questions(v: &Value, duration_ms: i64) -> Result<Value, ParseError> {
    let items = list(v, &["questions", "ask"]).ok_or("No questions in the answer")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for q in items {
        let (question, at) = match q {
            Value::Object(o) => (text_field(o, &["question", "q", "text"], MEDIUM), time_field(o, duration_ms)),
            Value::String(_) => (clean_text(q, MEDIUM), None),
            _ => (None, None),
        };
        let Some(question) = question else { continue };
        if seen.insert(norm_key(&question)) {
            out.push(json!({ "question": question, "at_ms": at }));
        }
        if out.len() == MAX_QUESTIONS {
            break;
        }
    }
    if out.is_empty() {
        return Err("The answer had no usable questions".into());
    }
    Ok(json!({ "questions": out }))
}

/// Validate a model answer for `kind`; returns the cleaned material (the
/// only thing stored). `duration_ms` bounds source times (0 = unknown).
pub fn validate(kind: StudyKind, raw: &str, duration_ms: i64) -> Result<Value, ParseError> {
    let v = extract_json(raw)?;
    match kind {
        StudyKind::Summary => summary(&v),
        StudyKind::Terms => terms(&v),
        StudyKind::Flashcards => flashcards(&v),
        StudyKind::Quiz => quiz(&v, duration_ms),
        StudyKind::Questions => questions(&v, duration_ms),
    }
}

/// Re-check stored material on the way out (a row written by an older
/// build, or edited outside the app, never reaches the UI unchecked).
pub fn revalidate(kind: StudyKind, stored: &str, duration_ms: i64) -> Option<Value> {
    validate(kind, stored, duration_ms).ok()
}

/// Condensed notes from one chunk: plain lines, each cleaned. Empty → error.
pub fn condensed_lines(raw: &str, max_lines: usize) -> Result<Vec<String>, ParseError> {
    let text = strip_think(raw);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .filter_map(|l| {
            // Keep a leading [m:ss] time; clean the rest
            let (time, rest) = match (l.starts_with('['), l.find(']')) {
                (true, Some(end)) if parse_clock(&l[..=end]).is_some() => (Some(&l[..=end]), l[end + 1..].trim()),
                _ => (None, l),
            };
            let body = clean_text(&Value::String(rest.to_string()), MEDIUM)?;
            Some(match time {
                Some(t) => format!("{} {}", t, body),
                None => body,
            })
        })
        .take(max_lines)
        .collect();
    if lines.is_empty() {
        return Err("The condensed notes were empty".into());
    }
    Ok(lines)
}
