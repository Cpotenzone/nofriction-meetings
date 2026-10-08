//! What the chat asks the model, and how the conversation and the passages
//! are fitted into its window (Apple's on-device model has 4K tokens for
//! prompt and answer together).

use super::retrieval::{Passage, ScopeInfo};
use super::HISTORY_TURNS;
use crate::ai::Msg;
use crate::study::prompt::clock;

pub fn system_prompt() -> String {
    "You are noFriction's assistant. You answer questions about the user's own recordings (meetings, \
classes and personal recordings) from the SOURCES given, which were found on their computer; each \
source names its recording, date and time. Rules: use only the sources; when they don't contain the \
answer, say what's missing instead of guessing. Cite a source inline as [n] right after the fact it \
supports, using only the numbers given, and cite the single best source rather than many. Answer in \
Markdown: short paragraphs or bullet lists, bold for the key point, headings no larger than ###, no \
HTML, no tables. Be concise. The transcripts come from speech recognition and may contain errors. \
Text shown as [stricken from the record] was removed by the user: never guess at or mention what it \
said."
        .to_string()
}

/// Answer budget, scaled down for small windows.
pub fn max_tokens(context_tokens: usize) -> u32 {
    900u32.min((context_tokens * 15 / 100).max(256) as u32)
}

/// Characters of passages that fit beside the system prompt, the fixed
/// text (scope, question, history) and the answer.
pub fn passage_budget(context_tokens: usize, max_tokens: u32, system: &str, fixed: &str) -> usize {
    crate::study::prompt::body_budget(context_tokens, max_tokens, system, fixed)
}

/// The thread's recent messages as prior turns: the last `HISTORY_TURNS`
/// (fewer in a small window), each clipped, oldest first. Earlier answers'
/// [n] citations refer to their own sources; the system prompt limits the
/// model to the numbers given now.
pub fn history_messages(history: &[(String, String)], context_tokens: usize) -> Vec<Msg> {
    let (keep, each) = if context_tokens < 8_192 { (4usize, 500usize) } else { (HISTORY_TURNS, 1_800) };
    let start = history.len().saturating_sub(keep);
    history[start..]
        .iter()
        .map(|(role, text)| {
            let t = super::retrieval::clip(text, each);
            if role == "assistant" {
                Msg::new(crate::ai::Role::Assistant, t)
            } else {
                Msg::user(t)
            }
        })
        .collect()
}

fn date_of(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|d| d.with_timezone(&chrono::Local).format("%a %-d %b %Y").to_string())
        .unwrap_or_default()
}

fn kind_word(kind: &str) -> &'static str {
    match kind {
        "class" => "Class",
        "personal" => "Personal",
        _ => "Meeting",
    }
}

/// One source as the prompt shows it.
pub fn passage_line(p: &Passage) -> String {
    let mut head = format!("[{}] {} — {}, {}", p.n, p.title.trim(), date_of(&p.started_at), kind_word(&p.kind));
    if let Some(nb) = &p.notebook {
        head.push_str(&format!(", Notebook {}", nb));
    }
    match (p.source, p.timestamp_ms) {
        ("transcript", Some(ms)) => head.push_str(&format!(" — said at {}", clock(ms))),
        ("marker", Some(ms)) => head.push_str(&format!(" — marked at {}", clock(ms))),
        ("notes", _) => head.push_str(" — from the saved notes"),
        _ => {}
    }
    format!("{}\n{}", head, p.excerpt)
}

/// The user message: scope, sources and the question.
pub fn user_message(info: &ScopeInfo, passages: &[Passage], question: &str) -> String {
    let sources = if passages.is_empty() {
        "(nothing in this scope matched the question)".to_string()
    } else {
        passages.iter().map(passage_line).collect::<Vec<_>>().join("\n\n")
    };
    format!(
        "SCOPE: {} ({} recording{})\n\nSOURCES:\n{}\n\nQUESTION: {}",
        info.label,
        info.count,
        if info.count == 1 { "" } else { "s" },
        sources,
        question.trim()
    )
}

/// The answer as shown: think blocks gone, a whole-answer code fence
/// unwrapped, trimmed.
pub fn clean_answer(raw: &str) -> String {
    let t = crate::ai::client::strip_think(raw);
    let t = t.trim();
    if t.starts_with("```") && t.ends_with("```") && t.matches("```").count() == 2 {
        let inner = t.trim_start_matches("```");
        let inner = inner.split_once('\n').map(|(_, rest)| rest).unwrap_or(inner);
        return inner.trim_end_matches("```").trim().to_string();
    }
    t.to_string()
}
