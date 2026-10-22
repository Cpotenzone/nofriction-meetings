//! What the Export sheet calls (src/components/export/ExportSheet.tsx):
//!
//! - `get_export_draft`: what this recording can put in a package
//! - `get_export_image`: a picture for the picker (small) or preview (large)
//! - `export_highlights`, `export_captions`: the AI tools (Pro + consent
//!   through `ai::client::complete`, like every AI call)
//! - `save_export_package`: build it and write a .zip (save panel) or a new
//!   folder (folder panel). Both work in the Mac App Store sandbox: the
//!   panels grant access to what the user chose.

use super::ai as xai;
use super::build::{self, EmailPart, Highlight, LinkPart, MarkPart, NotesPart, PackageInput, ScreenPart, Source, TranscriptLine};
use crate::database::DatabaseManager;
use crate::recording_kind::RecordingKind;
use crate::AppState;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, State};

/// A screen or photo of the recording.
#[derive(Debug, Clone)]
struct Item {
    id: String,
    ms: i64,
    end_ms: Option<i64>,
    source: Source,
    path: PathBuf,
    app: Option<String>,
    window: Option<String>,
}

/// The recording's screens and photos with a file on disk, in time order:
/// legacy frames and screen states with a picture (the same set Rewind
/// shows; stricken ones are gone from both tables).
async fn items(db: &DatabaseManager, meeting_id: &str, start: DateTime<Utc>) -> Result<Vec<Item>, String> {
    let mut out = Vec::new();
    for f in db.get_frames(meeting_id, 10_000).await.map_err(|e| format!("Couldn't read the screens: {}", e))? {
        if let Some(p) = f.file_path.filter(|p| !p.is_empty()) {
            out.push(Item {
                id: f.id.to_string(),
                ms: (f.timestamp - start).num_milliseconds().max(0),
                end_ms: None,
                source: Source::Screen,
                path: PathBuf::from(p),
                app: None,
                window: None,
            });
        }
    }
    let rfc = |s: &str| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc));
    for s in db.get_screen_states(meeting_id, 10_000).await.map_err(|e| format!("Couldn't read the screens: {}", e))? {
        let Some(p) = s.keyframe_path.filter(|p| !p.is_empty()) else { continue };
        let ms = rfc(&s.start_ts).map(|t| (t - start).num_milliseconds().max(0)).unwrap_or(0);
        out.push(Item {
            id: s.state_id,
            ms,
            end_ms: s.end_ts.as_deref().and_then(rfc).map(|t| (t - start).num_milliseconds().max(ms)),
            source: if s.state_type == "photo" { Source::Photo } else { Source::Screen },
            path: PathBuf::from(p),
            app: s.app_name.filter(|a| !a.trim().is_empty()),
            window: s.window_title.filter(|w| !w.trim().is_empty()),
        });
    }
    out.retain(|i| i.path.is_file());
    out.sort_by(|a, b| a.ms.cmp(&b.ms).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

async fn transcript(db: &DatabaseManager, meeting_id: &str, start: DateTime<Utc>) -> Result<Vec<TranscriptLine>, String> {
    let rows = db.get_transcripts(meeting_id).await.map_err(|e| format!("Couldn't read the transcript: {}", e))?;
    Ok(rows
        .into_iter()
        .map(|t| TranscriptLine {
            ms: (t.timestamp - start).num_milliseconds().max(0),
            speaker: t.speaker,
            text: crate::redaction::render_plain(&t.text),
        })
        .filter(|l| !l.text.trim().is_empty())
        .collect())
}

/// What the AI's highlights and captions were made from: if the transcript
/// reads differently now (a Delete or Strike, here or over Sync), they may
/// quote removed words.
pub(crate) fn fingerprint(lines: &[TranscriptLine]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for l in lines {
        h.update(l.ms.to_le_bytes());
        h.update(l.text.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().take(12).map(|b| format!("{:02x}", b)).collect()
}

/// The answer when the AI parts were made from a transcript that has
/// changed since. The sheet clears them and asks to make them again.
pub const TRANSCRIPT_CHANGED: &str = "TRANSCRIPT_CHANGED: The transcript was edited while you were exporting. The highlights, captions and email made before the edit were removed. Make them again.";

/// Refuse while a Delete is in its undo window: the line is hidden in the
/// app but still in `transcripts` (same rule as the review guide).
async fn no_pending_delete(db: &DatabaseManager, meeting_id: &str) -> Result<(), String> {
    let mut conn = db.pool().acquire().await.map_err(|e| format!("Database busy: {}", e))?;
    if crate::study::has_pending_delete(&mut conn, meeting_id).await? {
        return Err(crate::study::PENDING_EDIT.into());
    }
    Ok(())
}

fn when_label(start: DateTime<Utc>) -> String {
    start.with_timezone(&chrono::Local).format("%A %-d %B %Y, %H:%M").to_string()
}

fn duration_label(seconds: Option<i64>) -> Option<String> {
    let s = seconds.filter(|s| *s > 0)?;
    let m = (s + 30) / 60;
    Some(if m < 1 {
        "under a minute".into()
    } else if m < 60 {
        format!("{} min", m)
    } else if m % 60 == 0 {
        format!("{} h", m / 60)
    } else {
        format!("{} h {} min", m / 60, m % 60)
    })
}

async fn people_names(db: &DatabaseManager, meeting_id: &str) -> Vec<String> {
    let people = crate::people::meeting_people(db.pool(), meeting_id).await.map(|p| p.people).unwrap_or_default();
    people
        .iter()
        .filter(|p| !p.is_self)
        .map(|p| {
            p.name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| crate::attendee_intel::extract_name_from_email(&p.email))
        })
        .collect()
}

fn json_list(json: Option<&str>) -> Vec<serde_json::Value> {
    json.and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn notes_part(n: &crate::database::MeetingNotes) -> Option<NotesPart> {
    if n.model_used.as_deref() == Some(crate::sync::store::SYNCED_NOTES_MODEL) {
        return n.summary.clone().filter(|s| !s.trim().is_empty()).map(NotesPart::Markdown);
    }
    let text = |v: &serde_json::Value, k: &str| v.as_str().map(str::to_string).or_else(|| v[k].as_str().map(str::to_string));
    let topics: Vec<String> = json_list(n.key_topics.as_deref()).iter().filter_map(|v| text(v, "topic")).collect();
    let decisions: Vec<String> = json_list(n.decisions.as_deref()).iter().filter_map(|v| text(v, "text")).collect();
    let actions: Vec<String> = json_list(n.action_items.as_deref())
        .iter()
        .filter_map(|v| {
            let task = text(v, "task")?;
            let who = v["assignee"].as_str().map(str::trim).filter(|s| !s.is_empty());
            let due = v["due_date"].as_str().map(str::trim).filter(|s| !s.is_empty());
            Some(match (who, due) {
                (Some(w), Some(d)) => format!("{} ({}, {})", task, w, d),
                (Some(w), None) => format!("{} ({})", task, w),
                (None, Some(d)) => format!("{} (by {})", task, d),
                (None, None) => task,
            })
        })
        .collect();
    let summary = n.summary.clone().filter(|s| !s.trim().is_empty());
    if summary.is_none() && topics.is_empty() && decisions.is_empty() && actions.is_empty() {
        return None;
    }
    Some(NotesPart::Structured { summary, topics, decisions, actions })
}

// ─── The draft ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DraftItem {
    pub id: String,
    pub ms: i64,
    /// "screen" | "photo"
    pub source: &'static str,
    /// "Keynote — Q3 plan", when the Mac knows it
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftNotes {
    pub generated_at: String,
    pub stale: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportDraft {
    pub meeting_id: String,
    pub title: String,
    /// "meeting" | "class" | "personal"
    pub kind: String,
    pub when: String,
    pub duration: Option<String>,
    /// The package's folder name, for the sheet to show
    pub folder: String,
    pub notes: Option<DraftNotes>,
    /// Which review-guide parts exist ("summary", "terms", "flashcards", …)
    pub guide_parts: Vec<String>,
    pub marks: usize,
    pub links: usize,
    pub transcript_lines: usize,
    /// Pass back as `basis` with the AI tools and Save
    pub fingerprint: String,
    pub items: Vec<DraftItem>,
}

async fn meeting(db: &DatabaseManager, meeting_id: &str) -> Result<crate::database::Meeting, String> {
    db.get_meeting(meeting_id)
        .await
        .map_err(|e| format!("Couldn't read the recording: {}", e))?
        .ok_or_else(|| "That recording is gone.".to_string())
}

fn folder_date(start: DateTime<Utc>) -> String {
    start.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_export_draft(state: State<'_, AppState>, meeting_id: String) -> Result<ExportDraft, String> {
    draft(&state.database, &meeting_id).await
}

pub(crate) async fn draft(db: &DatabaseManager, meeting_id: &str) -> Result<ExportDraft, String> {
    let meeting_id = meeting_id.to_string();
    let m = meeting(db, &meeting_id).await?;
    let notes = db
        .get_meeting_notes(&meeting_id)
        .await
        .map_err(|e| format!("Couldn't read the notes: {}", e))?
        .filter(|n| notes_part(n).is_some())
        .map(|n| DraftNotes { generated_at: n.generated_at.to_rfc3339(), stale: n.stale_after_edit, summary: n.summary.clone() });
    let guide = crate::study::load_guide(db.pool(), &meeting_id).await.ok();
    let guide_parts = guide.as_ref().map(|g| g.materials.keys().cloned().collect()).unwrap_or_default();
    let marks = guide.as_ref().map(|g| g.markers.len()).unwrap_or(0);
    let links = crate::meeting_links::list_links(db.pool(), &meeting_id).await.map(|l| l.links.len()).unwrap_or(0);
    let lines = transcript(db, &meeting_id, m.started_at).await?;
    let items = items(db, &meeting_id, m.started_at).await?;
    Ok(ExportDraft {
        meeting_id: meeting_id.clone(),
        folder: build::folder_name(&folder_date(m.started_at), &m.title),
        title: m.title,
        kind: m.recording_kind.clone(),
        when: when_label(m.started_at),
        duration: duration_label(m.duration_seconds),
        notes,
        guide_parts,
        marks,
        links,
        transcript_lines: lines.len(),
        fingerprint: fingerprint(&lines),
        items: items
            .into_iter()
            .map(|i| DraftItem {
                label: match (&i.app, &i.window) {
                    (Some(a), Some(w)) => Some(format!("{} — {}", a, w)),
                    (Some(a), None) => Some(a.clone()),
                    (None, Some(w)) => Some(w.clone()),
                    _ => None,
                },
                id: i.id,
                ms: i.ms,
                source: i.source.as_str(),
            })
            .collect(),
    })
}

/// A picture of this recording as base64 JPEG: `small` for the picker
/// (fits 360×240), else up to 1600 px for the preview.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_export_image(
    state: State<'_, AppState>,
    meeting_id: String,
    item_id: String,
    small: bool,
) -> Result<String, String> {
    let m = meeting(&state.database, &meeting_id).await?;
    let item = items(&state.database, &meeting_id, m.started_at)
        .await?
        .into_iter()
        .find(|i| i.id == item_id)
        .ok_or("That picture is gone.")?;
    tokio::task::spawn_blocking(move || {
        let img = image::open(&item.path).map_err(|e| format!("Couldn't open the picture: {}", e))?;
        let (w, h) = if small { (360, 240) } else { (1600, 1600) };
        let img = if img.width() > w || img.height() > h { img.thumbnail(w, h) } else { img };
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img.to_rgb8())
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| format!("Couldn't read the picture: {}", e))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
    })
    .await
    .map_err(|e| format!("Couldn't read the picture: {}", e))?
}

// ─── AI tools ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HighlightDto {
    pub ms: i64,
    pub label: String,
    pub quote: Option<String>,
}

impl From<Highlight> for HighlightDto {
    fn from(h: Highlight) -> Self {
        HighlightDto { ms: h.ms, label: h.label, quote: h.quote }
    }
}

async fn ai_text(system: &str, prompt: &str, max_tokens: u32) -> Result<String, String> {
    crate::ai_client::AIClient::new().complete_with(Some(system), prompt, max_tokens, 0.2).await
}

/// The recording's key moments, each with words copied from the transcript.
#[tauri::command(rename_all = "camelCase")]
pub async fn export_highlights(
    state: State<'_, AppState>,
    meeting_id: String,
    basis: Option<String>,
) -> Result<Vec<HighlightDto>, String> {
    let m = meeting(&state.database, &meeting_id).await?;
    no_pending_delete(&state.database, &meeting_id).await?;
    let lines = transcript(&state.database, &meeting_id, m.started_at).await?;
    if basis.is_some_and(|b| b != fingerprint(&lines)) {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let parts = xai::parts(&lines);
    if parts.is_empty() {
        return Err("This recording has no transcript, so there's nothing to pick highlights from.".into());
    }
    let raw = ai_text(xai::HIGHLIGHTS_SYSTEM, &xai::highlights_prompt(&m.title, &parts), 900).await?;
    Ok(xai::parse_highlights(&raw, &parts)?.into_iter().map(HighlightDto::from).collect())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CaptionDto {
    pub id: String,
    pub caption: String,
}

/// Captions for the pictures `item_ids`. With `pick`, the model first
/// chooses at most that many of them (all of the recording's pictures when
/// `item_ids` is empty) and captions only those; the answer is the choice.
#[tauri::command(rename_all = "camelCase")]
pub async fn export_captions(
    state: State<'_, AppState>,
    meeting_id: String,
    item_ids: Vec<String>,
    pick: Option<usize>,
    basis: Option<String>,
) -> Result<Vec<CaptionDto>, String> {
    let m = meeting(&state.database, &meeting_id).await?;
    no_pending_delete(&state.database, &meeting_id).await?;
    let all = items(&state.database, &meeting_id, m.started_at).await?;
    let mut chosen: Vec<Item> = if item_ids.is_empty() {
        all
    } else {
        let want: std::collections::HashSet<&str> = item_ids.iter().map(String::as_str).collect();
        all.into_iter().filter(|i| want.contains(i.id.as_str())).collect()
    };
    if chosen.is_empty() {
        return Err("There are no screens or photos to caption.".into());
    }
    let lines = transcript(&state.database, &meeting_id, m.started_at).await?;
    if basis.is_some_and(|b| b != fingerprint(&lines)) {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let k = pick.map(|k| k.clamp(1, xai::MAX_PICKS));
    // Too many to send: sample evenly (pick mode), or caption the first ones
    if chosen.len() > xai::MAX_PICTURES {
        chosen = if k.is_some() {
            xai::sample(chosen.len(), xai::MAX_PICTURES).into_iter().map(|i| chosen[i].clone()).collect()
        } else {
            chosen.into_iter().take(xai::MAX_PICTURES).collect()
        };
    }
    let pictures: Vec<xai::Picture> = chosen
        .iter()
        .map(|i| xai::Picture { ms: i.ms, end_ms: i.end_ms, source: i.source.as_str(), app: i.app.clone(), window: i.window.clone() })
        .collect();
    let per = if chosen.len() > 20 { 220 } else { 420 };
    let prompt = xai::pictures_prompt(&m.title, &pictures, &lines, per);
    let (system, key) = match k {
        Some(k) => (xai::picks_system(k), "picks"),
        None => (xai::CAPTIONS_SYSTEM.to_string(), "captions"),
    };
    let raw = ai_text(&system, &prompt, 1_200).await?;
    let parsed = xai::parse_captions(&raw, chosen.len(), key)?;
    let parsed = match k {
        Some(k) => parsed.into_iter().take(k).collect(),
        None => parsed,
    };
    Ok(parsed.into_iter().map(|(i, caption)| CaptionDto { id: chosen[i].id.clone(), caption }).collect())
}

// ─── Saving ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Include {
    pub notes: bool,
    pub highlights: bool,
    pub screens: bool,
    pub marks: bool,
    pub links: bool,
    pub email: bool,
    pub guide: bool,
    pub transcript: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedItem {
    pub id: String,
    pub caption: Option<String>,
    /// The caption was written by the AI (and not edited since)
    #[serde(default)]
    pub ai: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailDto {
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub include: Include,
    /// In the order the package shows them
    #[serde(default)]
    pub items: Vec<PickedItem>,
    #[serde(default)]
    pub highlights: Vec<HighlightDto>,
    pub email: Option<EmailDto>,
    /// Any of the above was written by AI
    #[serde(default)]
    pub has_ai: bool,
    /// The draft's `fingerprint` the AI parts were made from
    #[serde(default)]
    pub basis: Option<String>,
}

fn cap(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Everything the request asked for, read fresh from the database.
pub(crate) async fn package_input(db: &DatabaseManager, meeting_id: &str, req: &ExportRequest) -> Result<(PackageInput, String), String> {
    let m = meeting(db, meeting_id).await?;
    no_pending_delete(db, meeting_id).await?;
    let lines = transcript(db, meeting_id, m.started_at).await?;
    // AI parts made from a transcript that reads differently now may quote
    // removed words: refuse them (the sheet clears them)
    let inc0 = &req.include;
    let ai_parts = (inc0.highlights && !req.highlights.is_empty())
        || (inc0.email && req.email.as_ref().is_some_and(|e| !e.body.trim().is_empty()))
        || (inc0.screens && req.items.iter().any(|i| i.ai && i.caption.as_deref().is_some_and(|c| !c.trim().is_empty())));
    if ai_parts && req.basis.as_deref() != Some(fingerprint(&lines).as_str()) {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let kind = RecordingKind::from_stored(Some(&m.recording_kind));
    let inc = &req.include;
    let mut p = PackageInput {
        title: m.title.clone(),
        kind,
        when: when_label(m.started_at),
        duration: duration_label(m.duration_seconds),
        notebook: m.class_name.clone(),
        people: people_names(db, meeting_id).await,
        has_ai: req.has_ai,
        ..Default::default()
    };
    if inc.notes {
        let n = db.get_meeting_notes(meeting_id).await.map_err(|e| format!("Couldn't read the notes: {}", e))?;
        p.notes = n.as_ref().and_then(notes_part);
        if p.notes.is_some() {
            p.has_ai = true;
        }
    }
    if inc.highlights {
        p.highlights = req
            .highlights
            .iter()
            .filter(|h| !h.label.trim().is_empty())
            .take(50)
            .map(|h| Highlight { ms: h.ms.max(0), label: cap(h.label.trim(), 300), quote: h.quote.as_deref().map(|q| cap(q.trim(), 600)) })
            .collect();
    }
    if inc.screens && !req.items.is_empty() {
        let all = items(db, meeting_id, m.started_at).await?;
        for pick in req.items.iter().take(500) {
            if let Some(i) = all.iter().find(|i| i.id == pick.id) {
                p.screens.push(ScreenPart {
                    ms: i.ms,
                    source: i.source,
                    caption: pick.caption.as_deref().map(|c| cap(c.trim(), 400)).filter(|c| !c.is_empty()),
                    path: i.path.clone(),
                });
            }
        }
    }
    let guide = if inc.marks || inc.guide { crate::study::load_guide(db.pool(), meeting_id).await.ok() } else { None };
    if inc.marks {
        if let Some(g) = &guide {
            p.marks = g
                .markers
                .iter()
                .map(|mk| MarkPart {
                    ms: mk.offset_ms,
                    symbol: crate::markers::symbol(&mk.kind),
                    label: crate::markers::label(&mk.kind, kind),
                    note: mk.note.clone(),
                })
                .collect();
        }
    }
    if inc.links {
        if let Ok(l) = crate::meeting_links::list_links(db.pool(), meeting_id).await {
            p.links = l.links.into_iter().map(|l| LinkPart { url: l.url, title: l.title, first_ms: l.first_ms }).collect();
        }
    }
    if inc.email {
        p.email = req
            .email
            .as_ref()
            .filter(|e| !e.body.trim().is_empty())
            .map(|e| EmailPart { subject: cap(e.subject.trim(), 300), body: cap(e.body.trim(), 20_000) });
    }
    if inc.guide {
        if let Some(g) = guide.as_ref().filter(|g| !g.materials.is_empty()) {
            p.guide_md = Some(crate::study::commands::guide_markdown_of(g));
            let cards = g.materials.get("flashcards").map(|m| crate::study::export::cards_of(&m.data)).unwrap_or_default();
            if !cards.is_empty() {
                p.flashcards_csv = Some(crate::study::export::flashcards_csv(&cards));
            }
            p.has_ai = true;
        }
    }
    if inc.transcript {
        p.transcript = lines;
    }
    Ok((p, folder_date(m.started_at)))
}

/// Build the package and write it where the user chooses: `destination`
/// "zip" (save panel, one .zip) or "folder" (folder panel; a new folder is
/// made inside). Ok(None) when cancelled; Ok(path) of what was written.
#[tauri::command(rename_all = "camelCase")]
pub async fn save_export_package(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: String,
    request: ExportRequest,
    destination: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (input, date) = package_input(&state.database, &meeting_id, &request).await?;
    let pkg = build::package(&input, &date);
    let (tx, rx) = tokio::sync::oneshot::channel();
    match destination.as_str() {
        "zip" => {
            app.dialog()
                .file()
                .set_title("Export recording")
                .set_file_name(format!("{}.zip", pkg.folder))
                .add_filter("Zip archive", &["zip"])
                .save_file(move |p| {
                    let _ = tx.send(p);
                });
        }
        "folder" => {
            app.dialog().file().set_title("Choose where to put the export").pick_folder(move |p| {
                let _ = tx.send(p);
            });
        }
        _ => return Err("Unknown export destination".into()),
    }
    let Some(chosen) = rx.await.map_err(|_| "The dialog closed unexpectedly".to_string())? else {
        return Ok(None);
    };
    let chosen = chosen.into_path().map_err(|_| "That location can't be written".to_string())?;
    let dest = destination.clone();
    let written = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
        if dest == "zip" {
            // Exactly the file the save panel chose: in the Mac App Store
            // sandbox that is the only path it grants (no temp file beside
            // it, no renamed extension). A failed write is removed.
            let file = std::fs::File::create(&chosen).map_err(|e| format!("Couldn't save the file: {}", e))?;
            let res = build::write_zip(&pkg, std::io::BufWriter::new(file)).and_then(|w| {
                w.into_inner().map_err(|e| format!("Couldn't save the file: {}", e))?.sync_all().map_err(|e| format!("Couldn't save the file: {}", e))
            });
            if let Err(e) = res {
                let _ = std::fs::remove_file(&chosen);
                return Err(e);
            }
            Ok(chosen)
        } else {
            let dir = build::unique_child(&chosen, &pkg.folder, "");
            if let Err(e) = build::write_folder(&pkg, &dir) {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
            Ok(dir)
        }
    })
    .await
    .map_err(|e| format!("Couldn't save the export: {}", e))??;
    Ok(Some(written.display().to_string()))
}

/// Show the exported file or folder in Finder.
#[tauri::command(rename_all = "camelCase")]
pub async fn reveal_export(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("That export isn't there any more.".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(p).map_err(|e| format!("Couldn't show it in Finder: {}", e))
}
