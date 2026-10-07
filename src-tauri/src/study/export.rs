//! Exports: flashcards as CSV (popular flashcard apps import it) and the
//! guide as Markdown ("Study guide" for a class, "Review guide" otherwise).
//! Built from the validated, stored material only.

use super::prompt::clock;
use crate::recording_kind::RecordingKind;
use serde_json::Value;

/// One RFC 4180 field. Always quoted (inner quotes doubled), so commas,
/// quotes and semicolons survive any importer. Line breaks become spaces
/// (Quizlet's importer takes one card per line). A field that a
/// spreadsheet would run as a formula gets a leading apostrophe.
pub fn csv_field(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\r' || c == '\n' || c == '\t' { ' ' } else { c })
        .collect::<String>();
    let flat = flat.trim();
    let risky = match flat.chars().next() {
        Some('=') | Some('+') | Some('@') => true,
        Some('-') => !flat[1..].starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == ' '),
        _ => false,
    };
    let body = if risky { format!("'{}", flat) } else { flat.to_string() };
    format!("\"{}\"", body.replace('"', "\"\""))
}

/// `front,back` rows, CRLF line ends, no header row (Anki and Quizlet would
/// import a header as a card).
pub fn flashcards_csv(cards: &[(String, String)]) -> String {
    let mut out = String::new();
    for (front, back) in cards {
        out.push_str(&csv_field(front));
        out.push(',');
        out.push_str(&csv_field(back));
        out.push_str("\r\n");
    }
    out
}

/// Cards from stored flashcards JSON.
pub fn cards_of(flashcards: &Value) -> Vec<(String, String)> {
    flashcards["cards"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| Some((c["front"].as_str()?.to_string(), c["back"].as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Model text inside Markdown: backslash-escape everything that could
/// become markup (links, images, emphasis, code, HTML, headings, tables).
pub fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '!' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '\r' | '\n' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// A marker as the guide lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideMark {
    pub ms: i64,
    pub kind: String,
    pub note: Option<String>,
}

/// Everything the Markdown guide is made of.
#[derive(Debug, Clone, Default)]
pub struct GuideParts<'a> {
    pub title: &'a str,
    /// Names the guide and labels the marks
    pub kind: RecordingKind,
    pub when: &'a str,
    pub summary: Option<&'a Value>,
    pub terms: Option<&'a Value>,
    pub flashcards: Option<&'a Value>,
    pub quiz: Option<&'a Value>,
    pub questions: Option<&'a Value>,
    pub marks: &'a [GuideMark],
}

fn at(v: &Value) -> Option<String> {
    v["at_ms"].as_i64().map(clock)
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// The guide as Markdown: summary, key terms, marked moments, questions to
/// ask, then the practice quiz with its answer key.
pub fn guide_markdown(g: &GuideParts) -> String {
    let mut out = format!("# {}: {}\n\n", g.kind.guide_title(), md_escape(g.title));
    if !g.when.is_empty() {
        out.push_str(&format!("{}\n\n", md_escape(g.when)));
    }

    if let Some(sum) = g.summary {
        out.push_str("## Summary\n\n");
        if let Some(t) = sum["title"].as_str() {
            out.push_str(&format!("*{}*\n\n", md_escape(t)));
        }
        for sec in sum["sections"].as_array().into_iter().flatten() {
            out.push_str(&format!("### {}\n\n", md_escape(s(sec, "heading"))));
            for b in sec["bullets"].as_array().into_iter().flatten() {
                out.push_str(&format!("- {}\n", md_escape(b.as_str().unwrap_or(""))));
            }
            out.push('\n');
        }
    }

    if let Some(terms) = g.terms {
        out.push_str("## Key terms\n\n");
        for t in terms["terms"].as_array().into_iter().flatten() {
            out.push_str(&format!("- **{}**: {}\n", md_escape(s(t, "term")), md_escape(s(t, "definition"))));
        }
        out.push('\n');
    }

    if !g.marks.is_empty() {
        out.push_str("## Marked moments\n\n");
        for m in g.marks {
            let mut line = format!(
                "- {} {} {}",
                clock(m.ms),
                crate::markers::symbol(&m.kind),
                crate::markers::label(&m.kind, g.kind)
            );
            if let Some(n) = m.note.as_deref().filter(|n| !n.trim().is_empty()) {
                line.push_str(&format!(": {}", md_escape(n.trim())));
            }
            out.push_str(&md_escape_leading(&line));
            out.push('\n');
        }
        out.push('\n');
    }

    let asked: Vec<&Value> = g.questions.and_then(|q| q["questions"].as_array()).map(|a| a.iter().collect()).unwrap_or_default();
    let confused: Vec<&GuideMark> = g.marks.iter().filter(|m| m.kind == "question").collect();
    if !asked.is_empty() || !confused.is_empty() {
        out.push_str("## Questions to ask\n\n");
        for q in asked {
            match at(q) {
                Some(t) => out.push_str(&format!("- {} ({})\n", md_escape(s(q, "question")), t)),
                None => out.push_str(&format!("- {}\n", md_escape(s(q, "question")))),
            }
        }
        let marked_as = if g.kind == RecordingKind::Class { "as confusing" } else { "with a question" };
        for m in confused {
            let note = m.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
            out.push_str(&format!(
                "- You marked {} {}{}\n",
                clock(m.ms),
                marked_as,
                note.map(|n| format!(": {}", md_escape(n))).unwrap_or_default()
            ));
        }
        out.push('\n');
    }

    if let Some(cards) = g.flashcards {
        let cards = cards_of(cards);
        if !cards.is_empty() {
            out.push_str("## Flashcards\n\n");
            for (f, b) in cards {
                out.push_str(&format!("- **{}**: {}\n", md_escape(&f), md_escape(&b)));
            }
            out.push('\n');
        }
    }

    if let Some(quiz) = g.quiz {
        let qs: Vec<&Value> = quiz["questions"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
        if !qs.is_empty() {
            out.push_str("## Practice quiz\n\n");
            for (i, q) in qs.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", i + 1, md_escape(s(q, "question"))));
                for (j, c) in q["choices"].as_array().into_iter().flatten().enumerate() {
                    let letter = (b'A' + j as u8) as char;
                    out.push_str(&format!("   - {}) {}\n", letter, md_escape(c.as_str().unwrap_or(""))));
                }
            }
            out.push_str("\n### Answer key\n\n");
            for (i, q) in qs.iter().enumerate() {
                let n = q["answer"].as_u64().unwrap_or(0) as usize;
                let letter = (b'A' + n.min(25) as u8) as char;
                let mut line = format!("{}. {}", i + 1, letter);
                let ex = s(q, "explanation");
                if !ex.is_empty() {
                    line.push_str(&format!(": {}", md_escape(ex)));
                }
                if let Some(t) = at(q) {
                    line.push_str(&format!(" ({})", t));
                }
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    out.push_str(match g.kind {
        RecordingKind::Class => {
            "_Made with noFriction Meetings from the lecture transcript. AI can make mistakes; check against the lecture._\n"
        }
        RecordingKind::Meeting => {
            "_Made with noFriction Meetings from the meeting transcript. AI can make mistakes; check against the recording._\n"
        }
        RecordingKind::Personal => {
            "_Made with noFriction Meetings from the transcript. AI can make mistakes; check against the recording._\n"
        }
    });
    out
}

/// "Bio 101 study guide.md" for a class, "Weekly sync review guide.md" otherwise.
pub fn guide_file_name(title: &str, kind: RecordingKind) -> String {
    format!("{} {}.md", file_stem(title), kind.guide_title().to_lowercase())
}

/// Marker lines are built from our own text; nothing to escape but keep
/// the helper explicit for the reader.
fn md_escape_leading(line: &str) -> String {
    line.to_string()
}

/// "Biology 101: Cells" → "Biology 101 Cells" (safe in a file name).
pub fn file_stem(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { ' ' })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(60).collect();
    if s.is_empty() {
        "Recording".into()
    } else {
        s
    }
}
