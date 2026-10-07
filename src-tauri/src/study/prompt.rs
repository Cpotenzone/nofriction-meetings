//! What the model is asked, and how a long lecture is fitted into a small
//! context window (Apple's on-device model has 4K tokens for prompt and
//! answer together).
//!
//! The input is the transcript as the notes feature builds it: stored lines
//! (already Whisper-filtered when they were transcribed; filtered text is
//! never stored, so it can't be fed back), with stricken spans rendered as
//! `[stricken from the record]` and deleted words gone. Each line carries
//! its time in the lecture so quiz answers can point back to it.

use super::parse::StudyKind;
use sha2::{Digest, Sha256};

/// One transcript line: ms from the meeting start and its plain text.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyLine {
    pub ms: i64,
    pub text: String,
}

/// One moment marker, as the prompt shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyMark {
    pub ms: i64,
    pub kind: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StudyInput {
    pub title: String,
    /// The recording's class ("BIO 101"), when it has one (classes.rs)
    pub class_name: Option<String>,
    pub duration_ms: i64,
    pub lines: Vec<StudyLine>,
    pub marks: Vec<StudyMark>,
}

/// 754_000 → "12:34"; 3_725_000 → "1:02:05".
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

impl StudyInput {
    /// `[m:ss] text` per line, consecutive strike placeholders shown once.
    pub fn transcript_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut last_placeholder = false;
        for l in &self.lines {
            let t = l.text.trim();
            if t.is_empty() {
                continue;
            }
            let placeholder = t == crate::redaction::STRICKEN_PLACEHOLDER;
            if placeholder && last_placeholder {
                continue;
            }
            last_placeholder = placeholder;
            out.push(format!("[{}] {}", clock(l.ms), t));
        }
        out
    }

    pub fn has_transcript(&self) -> bool {
        self.lines.iter().any(|l| {
            let t = l.text.trim();
            !t.is_empty() && t != crate::redaction::STRICKEN_PLACEHOLDER
        })
    }

    /// Marks between `from` and `to` (ms, inclusive), one per line.
    pub fn marks_block(&self, from: i64, to: i64) -> String {
        let lines: Vec<String> = self
            .marks
            .iter()
            .filter(|m| m.ms >= from && m.ms <= to)
            .map(|m| {
                let mut s = format!(
                    "[{}] {} {}",
                    clock(m.ms),
                    crate::markers::symbol(&m.kind),
                    crate::markers::label(&m.kind)
                );
                if let Some(n) = m.note.as_deref().filter(|n| !n.trim().is_empty()) {
                    s.push_str(&format!(": {}", n.trim()));
                }
                s
            })
            .collect();
        if lines.is_empty() {
            "(none)".into()
        } else {
            lines.join("\n")
        }
    }

    /// SHA-256 of the transcript exactly as the prompt shows it. Stored with
    /// each material; a different value later means the transcript changed.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        for l in self.transcript_lines() {
            h.update(l.as_bytes());
            h.update(b"\n");
        }
        h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Pack whole lines into chunks of at most `budget` characters. A single
/// line longer than the budget is cut at character boundaries.
pub fn chunk_lines(lines: &[String], budget: usize) -> Vec<String> {
    let budget = budget.max(200);
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for l in lines {
        let len = l.chars().count();
        if len > budget {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                cur_len = 0;
            }
            let chars: Vec<char> = l.chars().collect();
            for piece in chars.chunks(budget) {
                out.push(piece.iter().collect());
            }
            continue;
        }
        if cur_len + len + 1 > budget && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            cur_len = 0;
        }
        if !cur.is_empty() {
            cur.push('\n');
            cur_len += 1;
        }
        cur.push_str(l);
        cur_len += len;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Time span (ms) a chunk of `[m:ss]` lines covers, for its marks.
pub fn chunk_span(chunk: &str) -> Option<(i64, i64)> {
    let times: Vec<i64> = chunk
        .lines()
        .filter_map(|l| {
            let end = l.find(']')?;
            l.starts_with('[').then(|| super::parse::parse_clock(&l[..=end])).flatten()
        })
        .collect();
    Some((*times.iter().min()?, *times.iter().max()?))
}

// ── Budgets ─────────────────────────────────────────────────────────────

/// Same estimate as `ai::client::fit_messages`.
pub const CHARS_PER_TOKEN: f64 = crate::ai::client::CHARS_PER_TOKEN;
const RESERVE_TOKENS: usize = 512;
/// Our character estimate of tokens is rough; keep a margin so the
/// client's own fitting never has to trim a chunk.
const SAFETY: f64 = 0.85;

/// Answer budget for each part, scaled down for small context windows.
pub fn max_tokens(kind: StudyKind, context_tokens: usize) -> u32 {
    let want: u32 = match kind {
        StudyKind::Summary => 1_600,
        StudyKind::Terms => 1_200,
        StudyKind::Flashcards => 1_600,
        StudyKind::Quiz => 2_000,
        StudyKind::Questions => 800,
    };
    want.min((context_tokens * 3 / 10).max(256) as u32)
}

pub fn condense_max_tokens(context_tokens: usize) -> u32 {
    700u32.min((context_tokens / 5).max(200) as u32)
}

/// Characters of material that fit beside `system` + `fixed` text and an
/// answer of `max_tokens`.
pub fn body_budget(context_tokens: usize, max_tokens: u32, system: &str, fixed: &str) -> usize {
    let fixed_tokens = ((system.chars().count() + fixed.chars().count()) as f64 / CHARS_PER_TOKEN).ceil() as usize;
    let avail = context_tokens.saturating_sub(max_tokens as usize + RESERVE_TOKENS + fixed_tokens);
    ((avail as f64 * CHARS_PER_TOKEN * SAFETY) as usize).max(600)
}

// ── Prompts ─────────────────────────────────────────────────────────────

const BASE: &str = "You turn a lecture transcript into study material for a student. \
The transcript comes from speech recognition: it has no speaker labels and may contain \
recognition errors; don't repeat obvious errors. Each line starts with its time in the \
lecture as [m:ss]. Use only what the lecture says; never invent facts, names, numbers, \
dates or examples. Text shown as [stricken from the record] was removed by the student: \
never guess at or mention what it said. The student marked some moments while listening \
(STUDENT MARKS): moments marked ✎ On the test matter most, then ★ Important; make sure \
they are covered. Reply with only one JSON object: no Markdown, no code fence, no text \
before or after it.";

/// System prompt for one part of the guide.
pub fn system_for(kind: StudyKind) -> String {
    let task = match kind {
        StudyKind::Summary => "Write lecture notes: the main topics in the order they were taught, \
with the definitions, steps, examples and formulas given. 3 to 8 sections, 2 to 6 short \
bullets each. Shape: {\"title\": \"short lecture title\", \"sections\": [{\"heading\": \"topic\", \
\"bullets\": [\"point\", \"point\"]}]}",
        StudyKind::Terms => "List the key terms the lecture introduced or relied on, each with a \
definition of one or two sentences taken from the lecture. 5 to 20 terms. Shape: {\"terms\": \
[{\"term\": \"term\", \"definition\": \"definition\"}]}",
        StudyKind::Flashcards => "Write flashcards for studying: one fact, definition or step per \
card, the front a question or term, the back a short answer. 8 to 25 cards; cover the \
marked moments first. Shape: {\"cards\": [{\"front\": \"question\", \"back\": \"answer\"}]}",
        StudyKind::Quiz => "Write a multiple-choice practice quiz: 5 to 10 questions, each with 4 \
choices and exactly one correct choice. \"answer\" is the 0-based index of the correct \
choice. \"explanation\" is one line saying why. \"time\" is the [m:ss] time of the \
transcript line the answer comes from, without brackets. Shape: {\"questions\": \
[{\"question\": \"question\", \"choices\": [\"a\", \"b\", \"c\", \"d\"], \"answer\": 0, \
\"explanation\": \"why\", \"time\": \"12:34\"}]}",
        StudyKind::Questions => "Write questions the student could ask the instructor: points the \
lecture left unclear or skipped, and the moments marked ? Question (use the student's note \
when there is one). 3 to 8 questions. \"time\" is the [m:ss] time the question is about, \
without brackets. Shape: {\"questions\": [{\"question\": \"question\", \"time\": \"12:34\"}]}",
    };
    format!("{}\n\n{}", BASE, task)
}

/// Condensing one part of a long lecture (map step). Plain lines, not JSON.
pub const CONDENSE_SYSTEM: &str = "You condense part of a lecture transcript into study notes. \
The transcript comes from speech recognition and may contain errors. Each line starts with \
its time as [m:ss]. Write at most 15 short lines. Start every line with the [m:ss] time it \
comes from. Keep definitions, key terms, steps, examples, formulas, anything the lecturer \
stresses (\"this will be on the exam\"), points that sound unclear, and everything said near \
the STUDENT MARKS. Use only what the transcript says. Text shown as [stricken from the record] \
was removed by the student: never guess at or mention what it said. Plain text lines only.";

/// The fixed head of every user message.
pub fn header(input: &StudyInput) -> String {
    let title = input.title.trim();
    let mut s = format!("Lecture: {}\n", if title.is_empty() { "Untitled" } else { title });
    if let Some(c) = input.class_name.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        s.push_str(&format!("Class: {}\n", c));
    }
    if input.duration_ms > 0 {
        s.push_str(&format!("Length: {}\n", clock(input.duration_ms)));
    }
    s
}

/// The user message for one part: header, all marks, and the material
/// (the transcript itself, or notes condensed from it).
pub fn user_message(input: &StudyInput, condensed: bool, body: &str) -> String {
    format!(
        "{}\nSTUDENT MARKS:\n{}\n\n{}:\n{}",
        header(input),
        input.marks_block(i64::MIN, i64::MAX),
        if condensed { "LECTURE NOTES (condensed from the transcript, with times)" } else { "TRANSCRIPT" },
        body
    )
}

/// The user message for condensing one chunk (its own marks only).
pub fn condense_message(input: &StudyInput, chunk: &str, part: usize, parts: usize) -> String {
    let marks = match chunk_span(chunk) {
        Some((a, b)) => input.marks_block(a - 5_000, b + 30_000),
        None => "(none)".into(),
    };
    format!(
        "{}Part {} of {}\n\nSTUDENT MARKS:\n{}\n\nTRANSCRIPT:\n{}",
        header(input),
        part,
        parts,
        marks,
        chunk
    )
}

/// Asked once after an answer couldn't be used.
pub fn retry_note(kind: StudyKind, why: &str) -> String {
    format!(
        "Your previous answer for the {} couldn't be used ({}). Answer again with only the JSON \
object in the shape described, and nothing else.",
        kind.label(),
        why
    )
}
