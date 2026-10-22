//! The Export sheet's AI tools, as prompts and answer parsing:
//!
//! - **Highlights**: the key moments, each with words copied from the
//!   transcript. The model picks numbered parts; a quote it didn't copy
//!   exactly is dropped, so a highlight never puts words in anyone's mouth.
//! - **Captions**: one sentence per picked screen or photo, from what was
//!   being said around it (text only, so Apple's on-device model can do it).
//! - **Pick screens**: the same, choosing the few pictures worth keeping.
//!
//! Prompts carry rendered text only (`redaction::render_plain`), so stricken
//! words reach the model as the marker; a quote with the marker is dropped.

use super::build::{Highlight, TranscriptLine};
use crate::study::parse::{clean_text, extract_json};
use serde_json::Value;

/// Characters of transcript a highlights prompt carries. The client fits the
/// prompt to the model's window again (Apple on-device: 4K tokens).
pub const HIGHLIGHT_BUDGET: usize = 16_000;
pub const MAX_PARTS: usize = 120;
/// Parts a little longer than a sentence or two read well as quotes.
const PART_TARGET: usize = 320;
pub const MAX_HIGHLIGHTS: usize = 8;
/// Pictures one captions prompt carries (pick mode samples down to this).
pub const MAX_PICTURES: usize = 60;
pub const MAX_PICKS: usize = 12;

/// Both removal markers ("[stricken from the record]", "[screen stricken
/// from the record]") contain this: text with it is never quoted or used as
/// a caption.
pub const STRICKEN: &str = "stricken from the record";

/// Quote marks the model may wrap a quote in (the package adds its own).
fn unquote(q: &str) -> &str {
    q.trim_matches(|c| matches!(c, '"' | '“' | '”' | '‘' | '’' | '\'' | '«' | '»')).trim()
}

/// A line that is only a removal marker (a struck line or screen).
fn only_marker(text: &str) -> bool {
    let t = text.trim();
    t == format!("[{}]", STRICKEN) || t == format!("[screen {}]", STRICKEN)
}

pub const HIGHLIGHTS_SYSTEM: &str = "You choose the key moments of a recording for someone who wasn't there. \
You get the transcript as numbered parts, each with its time. \
Answer with JSON only, in this shape: \
{\"highlights\":[{\"part\":3,\"label\":\"What happens, at most 12 words\",\"quote\":\"Up to 25 words copied exactly from that part\"}]}. \
Pick 3 to 8 parts, in time order: decisions, key facts, commitments, open questions and turning points. \
Use only the transcript. Never invent words, names or numbers. \
Never quote text shown as [stricken from the record].";

pub const CAPTIONS_SYSTEM: &str = "You write captions for pictures from a recording: screens that were shown and photos that were taken. \
For each numbered picture you get its time, the app and window if known, and what was being said around then. \
Answer with JSON only, in this shape: {\"captions\":[{\"picture\":1,\"caption\":\"One sentence, at most 20 words\"}]}. \
Say what the picture is about from what was said. Don't describe details you weren't told. \
Use only the given text. Never invent names or numbers.";

pub fn picks_system(k: usize) -> String {
    format!(
        "You choose the pictures worth keeping from a recording: screens that were shown and photos that were taken. \
For each numbered picture you get its time, the app and window if known, and what was being said around then. \
Choose at most {k} pictures that best show what was discussed. Skip near-duplicates and pictures nothing was said about. \
Caption each one you choose. \
Answer with JSON only, in this shape: {{\"picks\":[{{\"picture\":1,\"caption\":\"One sentence, at most 20 words\"}}]}}, in time order. \
Use only the given text. Never invent names or numbers."
    )
}

// ─── Highlights ──────────────────────────────────────────────────────────

/// A stretch of transcript the model can point at.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub ms: i64,
    /// The whole stretch (quotes are checked against this)
    pub text: String,
}

/// Group transcript lines into at most `MAX_PARTS` parts of about
/// `PART_TARGET` characters, in time order.
pub fn parts(lines: &[TranscriptLine]) -> Vec<Part> {
    let lines: Vec<&TranscriptLine> = lines.iter().filter(|l| !l.text.trim().is_empty() && !only_marker(&l.text)).collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let total: usize = lines.iter().map(|l| l.text.len() + 1).sum();
    // Long recordings: bigger parts, so there are never more than MAX_PARTS
    let target = PART_TARGET.max(total / MAX_PARTS + 1);
    let mut out: Vec<Part> = Vec::new();
    let mut cur: Option<Part> = None;
    for l in lines {
        let said = match l.speaker.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => format!("{}: {}", s, l.text.trim()),
            None => l.text.trim().to_string(),
        };
        match cur.as_mut() {
            Some(p) if p.text.len() < target => {
                p.text.push(' ');
                p.text.push_str(&said);
            }
            _ => {
                if let Some(p) = cur.take() {
                    out.push(p);
                }
                cur = Some(Part { ms: l.ms, text: said });
            }
        }
    }
    out.extend(cur);
    out
}

/// The highlights prompt: numbered parts with their times, each cut so the
/// whole fits `HIGHLIGHT_BUDGET`.
pub fn highlights_prompt(title: &str, parts: &[Part]) -> String {
    let per = (HIGHLIGHT_BUDGET / parts.len().max(1)).max(80);
    let mut out = format!("Recording: {}\n\nTRANSCRIPT PARTS:\n", title);
    for (i, p) in parts.iter().enumerate() {
        let text: String = p.text.chars().take(per).collect();
        out.push_str(&format!("[{}] ({}) {}\n", i + 1, super::build::clock(p.ms), text));
    }
    out.push_str("\nJSON:");
    out
}

/// Letters and digits only, lowercased, single-spaced: how quotes are
/// compared with the transcript (punctuation and curly quotes don't count).
pub fn normalize(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap_or(c) } else { ' ' })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The model's highlights: valid part numbers only, once each, in time
/// order, at most `MAX_HIGHLIGHTS`. A quote is kept only if those words are
/// really in that part.
pub fn parse_highlights(raw: &str, parts: &[Part]) -> Result<Vec<Highlight>, String> {
    let v = extract_json(raw)?;
    let items = v["highlights"].as_array().or_else(|| v.as_array()).ok_or("The answer had no highlights")?;
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(usize, Highlight)> = Vec::new();
    for it in items {
        let Some(n) = it["part"].as_u64().or_else(|| it["part"].as_str().and_then(|s| s.trim().parse().ok())) else {
            continue;
        };
        let idx = n as usize;
        if idx == 0 || idx > parts.len() || !seen.insert(idx) {
            continue;
        }
        let part = &parts[idx - 1];
        let Some(label) = clean_text(&it["label"], 120) else { continue };
        let quote = clean_text(&it["quote"], 220).map(|q| unquote(&q).to_string()).filter(|q| {
            let nq = normalize(q);
            !q.contains(STRICKEN) && nq.split(' ').count() >= 2 && normalize(&part.text).contains(&nq)
        });
        out.push((idx, Highlight { ms: part.ms, label, quote }));
    }
    if out.is_empty() {
        return Err("The answer had no usable highlights".into());
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(out.into_iter().take(MAX_HIGHLIGHTS).map(|(_, h)| h).collect())
}

// ─── Captions and picks ──────────────────────────────────────────────────

/// What the model is told about one picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub ms: i64,
    /// When it stopped being shown, if known
    pub end_ms: Option<i64>,
    /// "photo" or "screen"
    pub source: &'static str,
    pub app: Option<String>,
    pub window: Option<String>,
}

/// What was said around a picture: from 30 s before it to when it stopped
/// being shown (at least 60 s after), cut to `max` characters.
pub fn said_around(p: &Picture, lines: &[TranscriptLine], max: usize) -> String {
    let from = p.ms - 30_000;
    let to = p.end_ms.unwrap_or(p.ms).max(p.ms + 60_000);
    let text = lines
        .iter()
        .filter(|l| l.ms >= from && l.ms <= to)
        .map(|l| l.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text: String = text.chars().take(max).collect();
    if text.is_empty() {
        "(nothing was said)".into()
    } else {
        text
    }
}

/// Evenly spaced indexes into `n` items, at most `k`, first and last kept.
pub fn sample(n: usize, k: usize) -> Vec<usize> {
    if n <= k {
        return (0..n).collect();
    }
    if k <= 1 {
        return vec![0];
    }
    let mut out: Vec<usize> = (0..k).map(|i| i * (n - 1) / (k - 1)).collect();
    out.dedup();
    out
}

/// The captions (or picks) prompt for `pictures`, numbered from 1.
pub fn pictures_prompt(title: &str, pictures: &[Picture], lines: &[TranscriptLine], per_picture: usize) -> String {
    let mut out = format!("Recording: {}\n\nPICTURES:\n", title);
    for (i, p) in pictures.iter().enumerate() {
        out.push_str(&format!("[{}] {} at {}", i + 1, p.source, super::build::clock(p.ms)));
        let app = p.app.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let win = p.window.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match (app, win) {
            (Some(a), Some(w)) => out.push_str(&format!(", {} — {}", a, w)),
            (Some(a), None) => out.push_str(&format!(", {}", a)),
            (None, Some(w)) => out.push_str(&format!(", {}", w)),
            _ => {}
        }
        out.push_str(&format!("\nSaid around then: {}\n\n", said_around(p, lines, per_picture)));
    }
    out.push_str("JSON:");
    out
}

/// The model's captions: (picture index from 0, caption), valid numbers
/// only, once each, in picture order. `key` is "captions" or "picks".
pub fn parse_captions(raw: &str, n: usize, key: &str) -> Result<Vec<(usize, String)>, String> {
    let v = extract_json(raw)?;
    let items: &Vec<Value> = v[key]
        .as_array()
        .or_else(|| v["captions"].as_array())
        .or_else(|| v.as_array())
        .ok_or("The answer had no captions")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for it in items {
        let Some(num) = it["picture"].as_u64().or_else(|| it["picture"].as_str().and_then(|s| s.trim().parse().ok())) else {
            continue;
        };
        let idx = num as usize;
        if idx == 0 || idx > n || !seen.insert(idx) {
            continue;
        }
        let Some(caption) = clean_text(&it["caption"], 160) else { continue };
        if caption.contains(STRICKEN) {
            continue;
        }
        out.push((idx - 1, caption));
    }
    if out.is_empty() {
        return Err("The answer had no usable captions".into());
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(out)
}
