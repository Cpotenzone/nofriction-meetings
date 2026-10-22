//! The package, from plain data: file names, the Markdown document, the
//! printable HTML page, the transcript, and writing it all as a folder or a
//! zip. No database, no AI, no clock: everything here is tested.

use crate::recording_kind::RecordingKind;
use crate::study::export::{file_stem, md_escape};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where an image came from: a screen captured on the Mac (or the iPhone's
/// Capture screen), or a photo taken on the iPhone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Screen,
    Photo,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Screen => "screen",
            Source::Photo => "photo",
        }
    }
}

/// The saved notes as the package shows them.
#[derive(Debug, Clone, PartialEq)]
pub enum NotesPart {
    /// Notes made on the Mac: summary, topics, decisions, action items
    Structured {
        summary: Option<String>,
        topics: Vec<String>,
        decisions: Vec<String>,
        actions: Vec<String>,
    },
    /// Notes synced from the iPhone: Markdown text
    Markdown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    pub ms: i64,
    pub label: String,
    /// Words copied from the transcript (checked by `ai::parse_highlights`)
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenPart {
    pub ms: i64,
    pub source: Source,
    pub caption: Option<String>,
    /// The image file on disk (copied into the package as is)
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkPart {
    pub ms: i64,
    pub symbol: &'static str,
    pub label: &'static str,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkPart {
    pub url: String,
    pub title: Option<String>,
    pub first_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmailPart {
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
    pub ms: i64,
    pub speaker: Option<String>,
    /// Already through `redaction::render_plain`
    pub text: String,
}

/// Everything a package can hold. Empty parts are left out of the files.
#[derive(Debug, Clone, Default)]
pub struct PackageInput {
    pub title: String,
    pub kind: RecordingKind,
    /// "Tuesday 21 October 2026, 14:00"
    pub when: String,
    /// "42 min"
    pub duration: Option<String>,
    pub notebook: Option<String>,
    pub people: Vec<String>,
    pub notes: Option<NotesPart>,
    pub highlights: Vec<Highlight>,
    pub screens: Vec<ScreenPart>,
    pub marks: Vec<MarkPart>,
    pub links: Vec<LinkPart>,
    pub email: Option<EmailPart>,
    /// The review guide as Markdown (study::commands::guide_markdown_of)
    pub guide_md: Option<String>,
    pub flashcards_csv: Option<String>,
    pub transcript: Vec<TranscriptLine>,
    /// Any part was written by AI: the footer says to check it
    pub has_ai: bool,
}

/// One file in the package, relative to its folder.
#[derive(Debug, Clone, PartialEq)]
pub enum FileData {
    Text(String),
    /// Copied from disk (screens and photos)
    Copy(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// The folder name (and the zip's root folder): "2026-10-21 Weekly sync"
    pub folder: String,
    /// (path inside the folder, contents), in the order written
    pub files: Vec<(String, FileData)>,
}

pub const TRANSCRIPT_FILE: &str = "transcript.txt";
pub const FLASHCARDS_FILE: &str = "flashcards.csv";
pub const SCREENS_DIR: &str = "screens";

/// "12:03", or "1:02:03" past an hour.
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, sec)
    } else {
        format!("{}:{:02}", m, sec)
    }
}

/// The folder name: "<date> <title>", safe in a file name on any system.
pub fn folder_name(date: &str, title: &str) -> String {
    let stem = file_stem(title);
    if date.is_empty() {
        stem
    } else {
        format!("{} {}", date, stem)
    }
}

/// "screens/03-12-07.jpg": numbered in package order, then the time, so the
/// files sort the way the document shows them.
pub fn image_name(index: usize, s: &ScreenPart) -> String {
    let ext = s
        .path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["jpg", "jpeg", "png", "heic"].contains(&e.as_str()))
        .unwrap_or_else(|| "jpg".into());
    let prefix = if s.source == Source::Photo { "photo" } else { "screen" };
    format!("{}/{:02}-{}-{}.{}", SCREENS_DIR, index + 1, prefix, clock(s.ms).replace(':', "-"), ext)
}

/// The guide's file name inside the package.
pub fn guide_name(kind: RecordingKind) -> String {
    format!("{}.md", kind.guide_title())
}

fn images_heading(screens: &[ScreenPart]) -> &'static str {
    let photos = screens.iter().filter(|s| s.source == Source::Photo).count();
    if photos == 0 {
        "Screens"
    } else if photos == screens.len() {
        "Photos"
    } else {
        "Screens and photos"
    }
}

fn footer(p: &PackageInput) -> &'static str {
    if p.has_ai {
        "Made with noFriction. Parts were written by AI from the transcript and can be wrong; check them against the recording."
    } else {
        "Made with noFriction."
    }
}

// ─── Markdown ────────────────────────────────────────────────────────────

/// The recording as one Markdown document, with the images embedded by
/// relative path.
pub fn markdown(p: &PackageInput) -> String {
    let mut out = format!("# {}\n\n", md_escape(&p.title));
    let mut meta = vec![p.kind.label().to_string()];
    if !p.when.is_empty() {
        meta.push(p.when.clone());
    }
    if let Some(d) = &p.duration {
        meta.push(d.clone());
    }
    out.push_str(&md_escape(&meta.join(" · ")));
    out.push_str("\n\n");
    if let Some(n) = p.notebook.as_deref().filter(|n| !n.trim().is_empty()) {
        out.push_str(&format!("Notebook: {}\n\n", md_escape(n.trim())));
    }
    if !p.people.is_empty() {
        out.push_str(&format!("People: {}\n\n", md_escape(&p.people.join(", "))));
    }

    match &p.notes {
        Some(NotesPart::Structured { summary, topics, decisions, actions }) => {
            if let Some(s) = summary.as_deref().filter(|s| !s.trim().is_empty()) {
                out.push_str("## Summary\n\n");
                // Paragraphs stay paragraphs, lines stay lines
                for para in s.trim().split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
                    let lines: Vec<String> = para.lines().map(|l| md_escape(l.trim())).collect();
                    out.push_str(&lines.join("  \n"));
                    out.push_str("\n\n");
                }
            }
            if !p.highlights.is_empty() {
                highlights_md(&mut out, &p.highlights);
            }
            list_md(&mut out, "Key topics", topics);
            list_md(&mut out, "Decisions", decisions);
            list_md(&mut out, "Action items", actions);
        }
        Some(NotesPart::Markdown(md)) => {
            if !p.highlights.is_empty() {
                highlights_md(&mut out, &p.highlights);
            }
            out.push_str("## Notes\n\n");
            // The iPhone's notes are Markdown already; headings move down
            // a level so they sit under "Notes"
            for line in md.lines() {
                let t = line.trim_end();
                if t.starts_with('#') {
                    out.push_str("##");
                }
                out.push_str(&md_inert(t));
                out.push('\n');
            }
            out.push('\n');
        }
        None => {
            if !p.highlights.is_empty() {
                highlights_md(&mut out, &p.highlights);
            }
        }
    }

    if !p.screens.is_empty() {
        out.push_str(&format!("## {}\n\n", images_heading(&p.screens)));
        for (i, s) in p.screens.iter().enumerate() {
            let caption = s.caption.as_deref().map(str::trim).filter(|c| !c.is_empty());
            let alt = caption.map(md_escape).unwrap_or_else(|| format!("{} at {}", s.source.as_str(), clock(s.ms)));
            out.push_str(&format!("![{}]({})\n\n", alt, image_name(i, s)));
            match caption {
                Some(c) => out.push_str(&format!("*{}* · {}\n\n", clock(s.ms), md_escape(c))),
                None => out.push_str(&format!("*{}*\n\n", clock(s.ms))),
            }
        }
    }

    if !p.marks.is_empty() {
        out.push_str("## Marked moments\n\n");
        for m in &p.marks {
            let mut line = format!("- {} {} {}", clock(m.ms), m.symbol, m.label);
            if let Some(n) = m.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                line.push_str(&format!(": {}", md_escape(n)));
            }
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
    }

    if !p.links.is_empty() {
        out.push_str("## Links\n\n");
        for l in &p.links {
            let label = l.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(md_escape).unwrap_or_else(|| md_escape(&l.url));
            let when = l.first_ms.map(|ms| format!(" ({})", clock(ms))).unwrap_or_default();
            out.push_str(&format!("- [{}](<{}>){}\n", label, l.url.replace('>', "%3E"), when));
        }
        out.push('\n');
    }

    if let Some(e) = &p.email {
        out.push_str("## Follow-up email\n\n");
        out.push_str(&format!("**Subject:** {}\n\n", md_escape(e.subject.trim())));
        for para in e.body.trim().split("\n\n") {
            let lines: Vec<String> = para.lines().map(md_escape).collect();
            out.push_str(&lines.join("  \n"));
            out.push_str("\n\n");
        }
    }

    let mut also = Vec::new();
    if p.guide_md.is_some() {
        also.push(format!("[{}](<{}>)", p.kind.guide_title(), guide_name(p.kind)));
    }
    if p.flashcards_csv.is_some() {
        also.push(format!("[Flashcards]({}) (CSV, for flashcard apps)", FLASHCARDS_FILE));
    }
    if !p.transcript.is_empty() {
        also.push(format!("[Transcript]({})", TRANSCRIPT_FILE));
    }
    if !also.is_empty() {
        out.push_str("## Also in this package\n\n");
        for a in also {
            out.push_str(&format!("- {}\n", a));
        }
        out.push('\n');
    }

    out.push_str(&format!("_{}_\n", footer(p)));
    out
}

/// AI-written Markdown kept as Markdown, but inert: no raw HTML and no
/// images (a viewer would load them from the internet when the file opens).
/// Headings, lists, bold and links stay.
pub fn md_inert(line: &str) -> String {
    line.replace('<', "\\<").replace('>', "\\>").replace("![", "\\![")
}

fn highlights_md(out: &mut String, hs: &[Highlight]) {
    out.push_str("## Highlights\n\n");
    for h in hs {
        let mut line = format!("- **{}** {}", clock(h.ms), md_escape(h.label.trim()));
        if let Some(q) = h.quote.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            line.push_str(&format!(": “{}”", md_escape(q)));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');
}

fn list_md(out: &mut String, heading: &str, items: &[String]) {
    let items: Vec<&str> = items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("## {}\n\n", heading));
    for i in items {
        out.push_str(&format!("- {}\n", md_escape(i)));
    }
    out.push('\n');
}

// ─── HTML ────────────────────────────────────────────────────────────────

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `**bold**` → <strong>, on already-escaped text.
fn inline_bold(escaped: &str) -> String {
    let mut out = String::new();
    let mut rest = escaped;
    while let Some(i) = rest.find("**") {
        let after = &rest[i + 2..];
        match after.find("**") {
            Some(j) if j > 0 => {
                out.push_str(&rest[..i]);
                out.push_str("<strong>");
                out.push_str(&after[..j]);
                out.push_str("</strong>");
                rest = &after[j + 2..];
            }
            _ => break,
        }
    }
    out.push_str(rest);
    out
}

/// A line that is only bold text, as the iPhone writes section titles:
/// "**Next:**" or "**Next**:" → "Next".
fn bold_heading(line: &str) -> Option<&str> {
    let inner = line.strip_suffix(':').unwrap_or(line).strip_prefix("**")?.strip_suffix("**")?;
    let inner = inner.strip_suffix(':').unwrap_or(inner).trim();
    (!inner.is_empty() && !inner.contains("**")).then_some(inner)
}

/// The iPhone's Markdown notes as HTML: headings, bullets, paragraphs and
/// bold; anything else stays text. Every character is escaped first.
pub fn simple_markdown_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_list = false;
    for raw in md.lines() {
        let line = raw.trim();
        let bullet = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("• "));
        if bullet.is_none() && in_list {
            out.push_str("</ul>\n");
            in_list = false;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(b) = bullet {
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline_bold(&html_escape(b))));
        } else if line.starts_with('#') {
            let text = line.trim_start_matches('#').trim();
            out.push_str(&format!("<h3>{}</h3>\n", html_escape(text)));
        } else if let Some(text) = bold_heading(line) {
            out.push_str(&format!("<h3>{}</h3>\n", html_escape(text)));
        } else {
            out.push_str(&format!("<p>{}</p>\n", inline_bold(&html_escape(line))));
        }
    }
    if in_list {
        out.push_str("</ul>\n");
    }
    out
}

const STYLE: &str = r#"
:root { color-scheme: light; }
body { font: 15px/1.55 -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif; color: #1c1c1e; background: #fff; max-width: 760px; margin: 40px auto; padding: 0 24px; }
h1 { font-size: 28px; line-height: 1.2; margin: 0 0 6px; }
h2 { font-size: 19px; margin: 32px 0 10px; padding-bottom: 4px; border-bottom: 1px solid #e5e5ea; }
h3 { font-size: 16px; margin: 18px 0 6px; }
.meta { color: #636366; margin: 0 0 4px; }
.time { font-variant-numeric: tabular-nums; color: #636366; margin-right: 6px; }
ul { padding-left: 22px; }
li { margin: 3px 0; }
blockquote { margin: 4px 0 0; color: #3a3a3c; font-style: italic; }
figure { margin: 18px 0 26px; break-inside: avoid; page-break-inside: avoid; }
figure img { display: block; max-width: 100%; height: auto; border: 1px solid #e5e5ea; border-radius: 6px; }
figcaption { margin-top: 6px; font-size: 14px; color: #3a3a3c; }
.email { white-space: pre-wrap; background: #f5f5f7; border-radius: 8px; padding: 12px 14px; }
.footer { margin-top: 40px; font-size: 13px; color: #8e8e93; }
a { color: #0a5fd8; }
@media print { body { margin: 0 auto; } h2 { break-after: avoid; } }
"#;

/// The recording as a self-contained web page (no scripts, no outside
/// requests; images by relative path), made to read well and to print to
/// PDF from a browser.
pub fn html(p: &PackageInput) -> String {
    let e = html_escape;
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{}</title>\n<style>{}</style>\n</head>\n<body>\n", e(&p.title), STYLE));
    out.push_str(&format!("<h1>{}</h1>\n", e(&p.title)));
    let mut meta = vec![p.kind.label().to_string()];
    if !p.when.is_empty() {
        meta.push(p.when.clone());
    }
    if let Some(d) = &p.duration {
        meta.push(d.clone());
    }
    out.push_str(&format!("<p class=\"meta\">{}</p>\n", e(&meta.join(" · "))));
    if let Some(n) = p.notebook.as_deref().filter(|n| !n.trim().is_empty()) {
        out.push_str(&format!("<p class=\"meta\">Notebook: {}</p>\n", e(n.trim())));
    }
    if !p.people.is_empty() {
        out.push_str(&format!("<p class=\"meta\">People: {}</p>\n", e(&p.people.join(", "))));
    }

    let highlights = |out: &mut String| {
        if p.highlights.is_empty() {
            return;
        }
        out.push_str("<h2>Highlights</h2>\n<ul>\n");
        for h in &p.highlights {
            out.push_str(&format!("<li><span class=\"time\">{}</span>{}", clock(h.ms), e(h.label.trim())));
            if let Some(q) = h.quote.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                out.push_str(&format!("<blockquote>“{}”</blockquote>", e(q)));
            }
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    };
    let list = |out: &mut String, heading: &str, items: &[String]| {
        let items: Vec<&str> = items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("<h2>{}</h2>\n<ul>\n", heading));
        for i in items {
            out.push_str(&format!("<li>{}</li>\n", e(i)));
        }
        out.push_str("</ul>\n");
    };

    match &p.notes {
        Some(NotesPart::Structured { summary, topics, decisions, actions }) => {
            if let Some(s) = summary.as_deref().filter(|s| !s.trim().is_empty()) {
                out.push_str("<h2>Summary</h2>\n");
                for para in s.trim().split("\n\n") {
                    out.push_str(&format!("<p>{}</p>\n", e(para.trim())));
                }
            }
            highlights(&mut out);
            list(&mut out, "Key topics", topics);
            list(&mut out, "Decisions", decisions);
            list(&mut out, "Action items", actions);
        }
        Some(NotesPart::Markdown(md)) => {
            highlights(&mut out);
            out.push_str("<h2>Notes</h2>\n");
            out.push_str(&simple_markdown_html(md));
        }
        None => highlights(&mut out),
    }

    if !p.screens.is_empty() {
        out.push_str(&format!("<h2>{}</h2>\n", images_heading(&p.screens)));
        for (i, s) in p.screens.iter().enumerate() {
            let caption = s.caption.as_deref().map(str::trim).filter(|c| !c.is_empty());
            let alt = caption.map(e).unwrap_or_else(|| format!("{} at {}", s.source.as_str(), clock(s.ms)));
            out.push_str(&format!(
                "<figure><img src=\"{}\" alt=\"{}\"><figcaption><span class=\"time\">{}</span>{}</figcaption></figure>\n",
                e(&image_name(i, s)),
                alt,
                clock(s.ms),
                caption.map(e).unwrap_or_default()
            ));
        }
    }

    if !p.marks.is_empty() {
        out.push_str("<h2>Marked moments</h2>\n<ul>\n");
        for m in &p.marks {
            out.push_str(&format!("<li><span class=\"time\">{}</span>{} {}", clock(m.ms), e(m.symbol), e(m.label)));
            if let Some(n) = m.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                out.push_str(&format!(": {}", e(n)));
            }
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    }

    if !p.links.is_empty() {
        out.push_str("<h2>Links</h2>\n<ul>\n");
        for l in &p.links {
            // Only web links become anchors (meeting_links keeps http/https)
            let label = l.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or(&l.url);
            let when = l.first_ms.map(|ms| format!(" <span class=\"time\">({})</span>", clock(ms))).unwrap_or_default();
            if l.url.starts_with("https://") || l.url.starts_with("http://") {
                out.push_str(&format!("<li><a href=\"{}\">{}</a>{}</li>\n", e(&l.url), e(label), when));
            } else {
                out.push_str(&format!("<li>{}{}</li>\n", e(label), when));
            }
        }
        out.push_str("</ul>\n");
    }

    if let Some(m) = &p.email {
        out.push_str("<h2>Follow-up email</h2>\n");
        out.push_str(&format!("<p><strong>Subject:</strong> {}</p>\n", e(m.subject.trim())));
        out.push_str(&format!("<div class=\"email\">{}</div>\n", e(m.body.trim())));
    }

    let mut also = Vec::new();
    if p.guide_md.is_some() {
        also.push(format!("<a href=\"{}\">{}</a>", e(&guide_name(p.kind)), e(p.kind.guide_title())));
    }
    if p.flashcards_csv.is_some() {
        also.push(format!("<a href=\"{}\">Flashcards</a> (CSV, for flashcard apps)", FLASHCARDS_FILE));
    }
    if !p.transcript.is_empty() {
        also.push(format!("<a href=\"{}\">Transcript</a>", TRANSCRIPT_FILE));
    }
    if !also.is_empty() {
        out.push_str("<h2>Also in this package</h2>\n<ul>\n");
        for a in also {
            out.push_str(&format!("<li>{}</li>\n", a));
        }
        out.push_str("</ul>\n");
    }

    out.push_str(&format!("<p class=\"footer\">{}</p>\n</body>\n</html>\n", e(footer(p))));
    out
}

// ─── Transcript ──────────────────────────────────────────────────────────

/// "[12:03] Speaker: text", one line each.
pub fn transcript_text(p: &PackageInput) -> String {
    let mut out = format!("{}\n{}\n\n", p.title, p.when);
    for l in &p.transcript {
        let text = l.text.replace(['\r', '\n'], " ");
        match l.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(s) => out.push_str(&format!("[{}] {}: {}\n", clock(l.ms), s.trim(), text.trim())),
            None => out.push_str(&format!("[{}] {}\n", clock(l.ms), text.trim())),
        }
    }
    out
}

// ─── The package ─────────────────────────────────────────────────────────

/// The files of a package, in the order they're written.
pub fn package(p: &PackageInput, date: &str) -> Package {
    let folder = folder_name(date, &p.title);
    let stem = file_stem(&p.title);
    let mut files = vec![
        (format!("{}.md", stem), FileData::Text(markdown(p))),
        (format!("{}.html", stem), FileData::Text(html(p))),
    ];
    for (i, s) in p.screens.iter().enumerate() {
        files.push((image_name(i, s), FileData::Copy(s.path.clone())));
    }
    if !p.transcript.is_empty() {
        files.push((TRANSCRIPT_FILE.into(), FileData::Text(transcript_text(p))));
    }
    if let Some(g) = &p.guide_md {
        files.push((guide_name(p.kind), FileData::Text(g.clone())));
    }
    if let Some(c) = &p.flashcards_csv {
        files.push((FLASHCARDS_FILE.into(), FileData::Text(c.clone())));
    }
    Package { folder, files }
}

/// A name under `parent` that doesn't exist yet: "Name", "Name 2", "Name 3"…
pub fn unique_child(parent: &Path, name: &str, ext: &str) -> PathBuf {
    let with = |n: &str| if ext.is_empty() { n.to_string() } else { format!("{}.{}", n, ext) };
    let first = parent.join(with(name));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|i| parent.join(with(&format!("{} {}", name, i))))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn rel_ok(rel: &str) -> bool {
    !rel.is_empty() && !rel.starts_with('/') && !rel.split('/').any(|c| c == ".." || c.is_empty())
}

/// Write the package into a new folder `dir` (which must not exist yet).
pub fn write_folder(pkg: &Package, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make the folder: {}", e))?;
    for (rel, data) in &pkg.files {
        if !rel_ok(rel) {
            return Err(format!("Bad path in the package: {}", rel));
        }
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't make the folder: {}", e))?;
        }
        match data {
            FileData::Text(t) => std::fs::write(&path, t).map_err(|e| format!("Couldn't write {}: {}", rel, e))?,
            FileData::Copy(src) => {
                std::fs::copy(src, &path).map_err(|e| format!("Couldn't copy {}: {}", rel, e))?;
            }
        }
    }
    Ok(())
}

/// Write the package as a zip whose root is the package folder. Images are
/// stored (already compressed); text is deflated.
pub fn write_zip<W: Write + std::io::Seek>(pkg: &Package, out: W) -> Result<W, String> {
    use zip::write::FileOptions;
    use zip::CompressionMethod;
    let mut zw = zip::ZipWriter::new(out);
    let text: FileOptions<'_, ()> = FileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored: FileOptions<'_, ()> = FileOptions::default().compression_method(CompressionMethod::Stored);
    for (rel, data) in &pkg.files {
        if !rel_ok(rel) {
            return Err(format!("Bad path in the package: {}", rel));
        }
        let name = format!("{}/{}", pkg.folder, rel);
        match data {
            FileData::Text(t) => {
                zw.start_file(name, text).map_err(|e| format!("Couldn't write the zip: {}", e))?;
                zw.write_all(t.as_bytes()).map_err(|e| format!("Couldn't write the zip: {}", e))?;
            }
            FileData::Copy(src) => {
                let bytes = std::fs::read(src).map_err(|e| format!("Couldn't read {}: {}", rel, e))?;
                zw.start_file(name, stored).map_err(|e| format!("Couldn't write the zip: {}", e))?;
                zw.write_all(&bytes).map_err(|e| format!("Couldn't write the zip: {}", e))?;
            }
        }
    }
    zw.finish().map_err(|e| format!("Couldn't finish the zip: {}", e))
}
