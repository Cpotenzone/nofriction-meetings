//! What the model is asked when it makes a lesson (docs/LESSONS.md §13), and
//! how the prompts fit a small context window.
//!
//! The material is the one study guides use (`study::prompt::user_message`):
//! the stored transcript (Whisper-filtered when it was transcribed, so
//! filtered text is never fed back), stricken spans as `[stricken from the
//! record]`, deleted words gone, each line time-stamped, and the student's
//! marks — or, for a long lecture on a small model, notes condensed from it
//! by `study::condense_to_budget`. Each part's prompt adds a short block of
//! context (the concepts, or the lesson being written), capped at
//! [`DYNAMIC_RESERVE`] characters so the budget worked out up front holds.

use super::parse::{ConceptDraft, LessonOutline};
use super::Part;
use crate::recording_kind::RecordingKind;
use crate::study::prompt::{self as sp, StudyInput};

/// Characters reserved for a part's context block (concepts, the lesson).
pub const DYNAMIC_RESERVE: usize = 2_000;
/// A concept's summary as a prompt shows it.
const SUMMARY_IN_PROMPT: usize = 160;

/// Answer budget for each part, scaled down for small context windows
/// (Apple's on-device model has 4K tokens for prompt and answer together).
pub fn max_tokens(part: Part, context_tokens: usize) -> u32 {
    let want: u32 = match part {
        Part::Concepts => 1_000,
        Part::Outline => 700,
        Part::Explanations => 1_400,
        Part::WorkedExamples => 1_300,
        Part::Mistakes => 900,
        Part::Checkpoint => 1_800,
        Part::Exam => 2_000,
    };
    want.min((context_tokens * 3 / 10).max(256) as u32)
}

/// Characters of material (transcript or condensed notes) that fit beside
/// every part's prompt, its context block and its answer.
pub fn final_budget(context_tokens: usize, input: &StudyInput) -> usize {
    let fixed = format!("{}{}", sp::user_message(input, true, ""), " ".repeat(DYNAMIC_RESERVE));
    Part::ALL
        .iter()
        .map(|p| sp::body_budget(context_tokens, max_tokens(*p, context_tokens), &system_for(*p), &fixed))
        .min()
        .unwrap_or(600)
}

fn base() -> String {
    let rec = RecordingKind::Class;
    format!(
        "You turn a lecture transcript into an interactive lesson for a student: explanations, worked \
examples, common mistakes and quizzes that teach what the lecture taught. The transcript comes from \
speech recognition: it has no speaker labels and may contain recognition errors; don't repeat obvious \
errors. Each line starts with its time in the lecture as [m:ss]. Use only what the lecture says; never \
invent facts, names, numbers, dates or examples. Text shown as [stricken from the record] was removed by \
the student: never guess at or mention what it said. The student marked some moments while listening \
({heading}): moments marked ✎ {third} matter most, then ★ Important; make sure they are covered. Write \
plain text: no Markdown, no LaTeX, no HTML. Reply with only one JSON object: no code fence, no text \
before or after it.",
        heading = sp::marks_heading(rec),
        third = rec.third_mark_label(),
    )
}

const MC_SHAPE: &str = "\"answer\" is the 0-based index of the correct choice. \"explanation\" is one \
line saying why. \"concepts\" lists the ids of the concepts the question tests. Shape: {\"questions\": \
[{\"question\": \"question\", \"choices\": [\"a\", \"b\", \"c\", \"d\"], \"answer\": 0, \"explanation\": \
\"why\", \"concepts\": [\"c1\"]}]}";

/// System prompt for one part.
pub fn system_for(part: Part) -> String {
    let task = match part {
        Part::Concepts => "Name the concepts the lecture teaches, in the order it teaches them: 3 to 12 \
(fewer for a short lecture). Each concept has an id (\"c1\", \"c2\", …), a name of a few words, a summary \
of one or two sentences taken from the lecture, and \"prerequisites\": the ids of earlier concepts it \
builds on (empty when it builds on none). Also give the lecture a short course title. Shape: {\"title\": \
\"course title\", \"concepts\": [{\"id\": \"c1\", \"name\": \"name\", \"summary\": \"summary\", \
\"prerequisites\": []}]}"
            .to_string(),
        Part::Outline => "Plan the lessons: split the CONCEPTS into 1 to 4 lessons in teaching order, about \
one lesson for every 15 to 20 minutes of lecture (one lesson for a short lecture). Each lesson has a short \
title, 2 to 4 learning objectives (what the student will be able to do afterwards, one sentence each) and \
the ids of the concepts it teaches. Every concept belongs to exactly one lesson. Shape: {\"lessons\": \
[{\"title\": \"title\", \"objectives\": [\"objective\"], \"concepts\": [\"c1\", \"c2\"]}]}"
            .to_string(),
        Part::Explanations => "Write the explanations for THIS LESSON: 2 to 4 short sections that teach its \
concepts the way the lecture explained them, each with a title, 1 to 3 short paragraphs in plain language, \
and the ids of the concepts it explains. Shape: {\"explanations\": [{\"title\": \"title\", \"concepts\": \
[\"c1\"], \"paragraphs\": [\"paragraph\"]}]}"
            .to_string(),
        Part::WorkedExamples => "Write worked examples for THIS LESSON: 1 to 3 problems or cases the lecture \
worked through, or that apply its concepts directly, each solved in 2 to 6 short steps in words, ending \
with the answer in one sentence. Use only numbers and facts the lecture gave. If the lecture has nothing \
to work through, reply {\"examples\": []}. Shape: {\"examples\": [{\"prompt\": \"problem\", \"concepts\": \
[\"c1\"], \"steps\": [\"step\"], \"answer\": \"the answer in one sentence\"}]}"
            .to_string(),
        Part::Mistakes => "List 2 to 4 common mistakes a student could make about THIS LESSON's concepts: \
the tempting wrong idea, why it's wrong, and the correct idea, based on what the lecture says. Shape: \
{\"mistakes\": [{\"claim\": \"the wrong idea\", \"why_wrong\": \"why it's wrong\", \"correct\": \"the \
correct idea\", \"concepts\": [\"c1\"]}]}"
            .to_string(),
        Part::Checkpoint => format!(
            "Write a multiple-choice checkpoint quiz for THIS LESSON: 6 to 10 questions on its concepts, \
each with 4 choices and exactly one correct choice. {}",
            MC_SHAPE
        ),
        Part::Exam => format!(
            "Write the final exam for the whole lecture: 8 to 15 multiple-choice questions across all the \
CONCEPTS, a little harder than a checkpoint (combine ideas where the lecture did), each with 4 choices and \
exactly one correct choice. {}",
            MC_SHAPE
        ),
    };
    format!("{}\n\n{}", base(), task)
}

/// Cut a context block to `max` characters (char-safe).
fn cap(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn short(s: &str, max: usize) -> String {
    cap(s.to_string(), max)
}

/// `CONCEPTS:` with ids and names (and what each builds on).
pub fn concepts_block(concepts: &[ConceptDraft]) -> String {
    let mut s = String::from("CONCEPTS:\n");
    for c in concepts {
        s.push_str(&format!("{}: {}", c.id, short(&c.name, 80)));
        if !c.prerequisites.is_empty() {
            s.push_str(&format!(" (builds on {})", c.prerequisites.join(", ")));
        }
        s.push('\n');
    }
    cap(s, DYNAMIC_RESERVE)
}

/// `THIS LESSON (i of n): title`, its objectives and its concepts.
pub fn lesson_block(index: usize, count: usize, lesson: &LessonOutline, concepts: &[ConceptDraft]) -> String {
    let mut s = format!("THIS LESSON ({} of {}): {}\n", index, count, short(&lesson.title, 120));
    if !lesson.objectives.is_empty() {
        s.push_str("Objectives:\n");
        for o in &lesson.objectives {
            s.push_str(&format!("- {}\n", short(o, 160)));
        }
    }
    s.push_str("Its concepts:\n");
    for id in &lesson.concept_ids {
        if let Some(c) = concepts.iter().find(|c| &c.id == id) {
            if c.summary.is_empty() {
                s.push_str(&format!("{}: {}\n", c.id, short(&c.name, 80)));
            } else {
                s.push_str(&format!("{}: {}: {}\n", c.id, short(&c.name, 80), short(&c.summary, SUMMARY_IN_PROMPT)));
            }
        }
    }
    cap(s, DYNAMIC_RESERVE)
}

/// The user message: the part's context block, then the header, marks and
/// material (`study::prompt::user_message`).
pub fn user_message(block: &str, base_user: &str) -> String {
    if block.is_empty() {
        base_user.to_string()
    } else {
        format!("{}\n{}", block, base_user)
    }
}

/// Asked once after an answer couldn't be used.
pub fn retry_note(label: &str, why: &str) -> String {
    format!(
        "Your previous answer for the {} couldn't be used ({}). Answer again with only the JSON object in \
the shape described, and nothing else.",
        label, why
    )
}
