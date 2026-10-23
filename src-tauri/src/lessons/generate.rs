//! Make lesson (docs/LESSONS.md §13): a Class recording's transcript → a
//! playable lesson set, made with the user's own AI.
//!
//! - **AI**: every request goes through `study::Completer` →
//!   `ai::complete_text` → `ai::client::complete` (Apple on-device or the one
//!   user-entered endpoint), so the unchanged Pro → configured → consent
//!   chain applies; nothing here adds or skips a gate. Consent, Pro,
//!   configuration, key and network errors stop the whole run.
//! - **Input**: exactly what the study guide sends (`study::load_input`):
//!   the stored transcript (Whisper hallucinations were filtered when it was
//!   transcribed and are never fed back), stricken spans as `[stricken from
//!   the record]`, deleted words gone, each line time-stamped, plus the
//!   student's marks. A long lecture on a small model (Apple's 4K window) is
//!   condensed chunk by chunk first (`study::condense_to_budget`).
//! - **Parts** (each asked for, tolerantly parsed, strictly validated and
//!   retried once on its own): the concept graph, the lesson plan, then per
//!   lesson its explanations, checkpoint, worked examples and common
//!   mistakes, and finally the exam. A part that is unusable twice is
//!   reported; the others go on. A lesson without explanations or a
//!   checkpoint is left out (reported); with no lesson left, or without the
//!   concepts or the plan, the run fails.
//! - **Content rules** (§6 HIGH-2): the model's numbers are never stored. A
//!   generated lesson teaches with `text` explainers, worked examples whose
//!   steps are words, common mistakes, and multiple-choice checkpoints and
//!   exam whose keys are resolved indexes. No figures (no runtime image
//!   generation), no closed-form interactive explainers. The assembled set
//!   is validated again as a whole (`schema::validate_set`, generated
//!   profile) and only that cleaned structure is stored.
//! - **Storage**: an ordinary catalog entry (provenance `generated`,
//!   `source_recording_id`, `transcript_fingerprint`) plus its files under
//!   `<app data>/lessons/<id>/`. Saving re-checks the fingerprint and pending
//!   Deletes inside `BEGIN IMMEDIATE`, and replaces the recording's previous
//!   generated set. Any Delete or Strike of the recording's transcript text
//!   deletes it (`lessons::purge_for_meeting`, live and backups).
//!   Transcript text and model output are never logged.

pub mod parse;
pub mod prompt;
#[cfg(test)]
mod tests;

use crate::ai::{Msg, Opts};
use crate::lessons::schema::{self, Concept, Exam, ExamMix, Lesson, LessonSet, Misconceptions, QuizItem, Trainer};
use crate::recording_kind::RecordingKind;
use crate::study::prompt::StudyInput;
use crate::study::{self, Completer, GenError, Progress};
use parse::{ConceptDraft, ConceptLookup, LessonOutline, LessonScope};
use serde::Serialize;
use sqlx::sqlite::SqliteConnection;
use sqlx::{Pool, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const NOT_A_CLASS: &str =
    "Make lesson works on Class recordings. Change this recording's type to Class first.";
pub const NO_TRANSCRIPT: &str = "This recording has no transcript to make a lesson from.";
pub const TRANSCRIPT_CHANGED: &str =
    "The transcript changed while the lesson was being made (it was edited), so it wasn't saved. Make it again.";

/// Questions a generated exam presents (from its bank).
pub const EXAM_QUESTIONS: usize = 20;
/// Questions a generated checkpoint presents (from its bank).
pub const CHECKPOINT_QUESTIONS: usize = 10;
pub const PASS_PCT: u32 = 80;

/// The parts of a lesson set, each asked for and validated on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Concepts,
    Outline,
    Explanations,
    Checkpoint,
    WorkedExamples,
    Mistakes,
    Exam,
}

impl Part {
    pub const ALL: [Part; 7] = [
        Part::Concepts,
        Part::Outline,
        Part::Explanations,
        Part::Checkpoint,
        Part::WorkedExamples,
        Part::Mistakes,
        Part::Exam,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Part::Concepts => "concepts",
            Part::Outline => "outline",
            Part::Explanations => "explanations",
            Part::Checkpoint => "checkpoint",
            Part::WorkedExamples => "worked_examples",
            Part::Mistakes => "mistakes",
            Part::Exam => "exam",
        }
    }

    /// For progress and error messages
    pub fn label(self) -> &'static str {
        match self {
            Part::Concepts => "concepts",
            Part::Outline => "lesson plan",
            Part::Explanations => "explanations",
            Part::Checkpoint => "checkpoint",
            Part::WorkedExamples => "worked examples",
            Part::Mistakes => "common mistakes",
            Part::Exam => "final exam",
        }
    }
}

/// A part that was unusable twice (the run went on without it).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartFailure {
    /// `Part::as_str`
    pub part: String,
    /// 1-based lesson number, for a per-lesson part
    pub lesson: Option<usize>,
    pub error: String,
}

/// What a run made: the validated set (not yet saved) and the reported parts.
#[derive(Debug)]
pub struct Made {
    pub validated: schema::ValidatedSet,
    pub failed: Vec<PartFailure>,
}

/// Ask for one part; parse + validate; on a bad answer ask once more.
/// `Ok(Err(why))` = unusable twice (reported for that part only).
#[allow(clippy::too_many_arguments)]
async fn ask<T>(
    c: &dyn Completer,
    part: Part,
    label: &str,
    block: &str,
    base_user: &str,
    ctx: usize,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<Result<T, String>, GenError> {
    let system = prompt::system_for(part);
    let user = prompt::user_message(block, base_user);
    let max_tokens = prompt::max_tokens(part, ctx);
    let mut why = String::new();
    for attempt in 0..2 {
        let text = if attempt == 0 { user.clone() } else { format!("{}\n\n{}", user, prompt::retry_note(label, &why)) };
        let opts = Opts { max_tokens, temperature: Some(if attempt == 0 { 0.2 } else { 0.3 }) };
        match c.complete(vec![Msg::system(system.clone()), Msg::user(text)], opts).await {
            Ok(raw) => match parse(&raw) {
                Ok(v) => return Ok(Ok(v)),
                Err(e) => why = e,
            },
            Err(e) if study::answer_error(&e) => why = study::short(&e),
            Err(e) => return Err(GenError::Ai(e)),
        }
    }
    log::warn!("Lesson: the {} answer was unusable twice", part.as_str());
    Ok(Err(format!(
        "Couldn't make the {}: {} (asked twice). Try again, or choose a larger model in Settings → AI Engine.",
        label, why
    )))
}

/// One lesson's parts as made (before assembly).
struct LessonDraft {
    outline: LessonOutline,
    explainers: Vec<schema::Explainer>,
    checkpoint: Vec<QuizItem>,
    worked_examples: Vec<schema::WorkedExample>,
    mistakes: Vec<schema::MisconceptionItem>,
}

/// Make a lesson set from a Class recording's input. `set_id` names the new
/// set; `meeting_id` is its source recording. AI access errors stop the run;
/// a part whose answer is unusable twice is reported and the rest go on.
pub async fn generate(
    c: &dyn Completer,
    input: &StudyInput,
    meeting_id: &str,
    set_id: &str,
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Made, GenError> {
    if input.kind != RecordingKind::Class {
        return Err(GenError::Failed(NOT_A_CLASS.into()));
    }
    if !input.has_transcript() {
        return Err(GenError::Failed(NO_TRANSCRIPT.into()));
    }
    let ctx = c.context_tokens();
    // Concepts + plan + one lesson's four parts + exam, until the plan says
    // how many lessons there are.
    let mut total = 2 + 4 + 1;
    let mut done = 0usize;
    let budget = prompt::final_budget(ctx, input);
    let (body, condensed) = study::condense_to_budget(c, input, budget, &mut total, &mut done, progress).await?;
    let base_user = study::prompt::user_message(input, condensed, &body);
    let mut failed: Vec<PartFailure> = Vec::new();
    let fail = |part: Part, lesson: Option<usize>, error: String| PartFailure { part: part.as_str().into(), lesson, error };

    // 1. The concept graph (repaired into a DAG by the parser).
    progress(Progress { done, total, label: "Finding the concepts…".into() });
    let concepts = match ask(c, Part::Concepts, Part::Concepts.label(), "", &base_user, ctx, parse::concepts).await? {
        Ok(d) => d,
        Err(e) => return Err(GenError::Failed(e)),
    };
    done += 1;
    if concepts.repaired_edges > 0 {
        log::info!("Lesson: repaired {} prerequisite edge(s) in the concept graph", concepts.repaired_edges);
    }

    // 2. The lesson plan.
    progress(Progress { done, total, label: "Planning the lessons…".into() });
    let block = prompt::concepts_block(&concepts.concepts);
    let outline = match ask(c, Part::Outline, Part::Outline.label(), &block, &base_user, ctx, |raw| {
        parse::outline(raw, &concepts.concepts)
    })
    .await?
    {
        Ok(o) => o,
        Err(e) => return Err(GenError::Failed(e)),
    };
    done += 1;
    let n = outline.len();
    total = done + 4 * n + 1;
    let lookup = ConceptLookup::new(&concepts.concepts);

    // 3. Each lesson: explanations and a checkpoint are required; worked
    //    examples and common mistakes are reported if they fail.
    let mut drafts: Vec<LessonDraft> = Vec::new();
    for (i, lo) in outline.into_iter().enumerate() {
        let num = i + 1;
        let lesson_id = format!("l{}", num);
        let block = prompt::lesson_block(num, n, &lo, &concepts.concepts);
        let scope = LessonScope { lesson_id: &lesson_id, concept_ids: &lo.concept_ids, lookup: &lookup };
        let step = |what: &str| format!("Lesson {} of {}: writing the {}…", num, n, what);
        let label = |p: Part| format!("{} for lesson {}", p.label(), num);

        progress(Progress { done, total, label: step(Part::Explanations.label()) });
        let explainers =
            ask(c, Part::Explanations, &label(Part::Explanations), &block, &base_user, ctx, |raw| parse::explanations(raw, &scope))
                .await?;
        done += 1;
        let explainers = match explainers {
            Ok(x) => x,
            Err(e) => {
                failed.push(fail(Part::Explanations, Some(num), e));
                done += 3;
                continue;
            }
        };

        progress(Progress { done, total, label: step(Part::Checkpoint.label()) });
        let prefix = format!("{}-q", lesson_id);
        let checkpoint = ask(c, Part::Checkpoint, &label(Part::Checkpoint), &block, &base_user, ctx, |raw| {
            parse::questions(raw, &prefix, &lo.concept_ids, &lookup, parse::MIN_CHECKPOINT_QUESTIONS)
        })
        .await?;
        done += 1;
        let checkpoint = match checkpoint {
            Ok(q) => q,
            Err(e) => {
                failed.push(fail(Part::Checkpoint, Some(num), e));
                done += 2;
                continue;
            }
        };

        progress(Progress { done, total, label: step(Part::WorkedExamples.label()) });
        let worked_examples = match ask(c, Part::WorkedExamples, &label(Part::WorkedExamples), &block, &base_user, ctx, |raw| {
            parse::worked_examples(raw, &scope)
        })
        .await?
        {
            Ok(w) => w,
            Err(e) => {
                failed.push(fail(Part::WorkedExamples, Some(num), e));
                Vec::new()
            }
        };
        done += 1;

        progress(Progress { done, total, label: step(Part::Mistakes.label()) });
        let mistakes =
            match ask(c, Part::Mistakes, &label(Part::Mistakes), &block, &base_user, ctx, |raw| parse::mistakes(raw, &scope))
                .await?
            {
                Ok(m) => m,
                Err(e) => {
                    failed.push(fail(Part::Mistakes, Some(num), e));
                    Vec::new()
                }
            };
        done += 1;
        drafts.push(LessonDraft { outline: lo, explainers, checkpoint, worked_examples, mistakes });
    }
    if drafts.is_empty() {
        let why = failed.first().map(|f| f.error.clone()).unwrap_or_default();
        return Err(GenError::Failed(format!("Couldn't make any lesson from this recording. {}", why).trim().to_string()));
    }

    // Concepts whose lesson was left out go too (with the edges to them), so
    // every concept in the set is taught somewhere.
    let taught: HashSet<String> = drafts.iter().flat_map(|d| d.outline.concept_ids.iter().cloned()).collect();
    let kept_concepts: Vec<ConceptDraft> = concepts
        .concepts
        .iter()
        .filter(|c| taught.contains(&c.id))
        .map(|c| ConceptDraft {
            prerequisites: c.prerequisites.iter().filter(|p| taught.contains(*p)).cloned().collect(),
            ..c.clone()
        })
        .collect();
    let all_ids: Vec<String> = kept_concepts.iter().map(|c| c.id.clone()).collect();

    // 4. The final exam: its own questions plus the lessons' checkpoint
    //    questions, so a failed exam part still leaves a cumulative exam.
    progress(Progress { done, total, label: "Writing the final exam…".into() });
    let block = prompt::concepts_block(&kept_concepts);
    let exam_items = match ask(c, Part::Exam, Part::Exam.label(), &block, &base_user, ctx, |raw| {
        parse::questions(raw, "exam-q", &all_ids, &lookup, 1)
    })
    .await?
    {
        Ok(q) => q,
        Err(e) => {
            failed.push(fail(Part::Exam, None, e));
            Vec::new()
        }
    };
    done += 1;
    progress(Progress { done, total, label: "Checking the lesson…".into() });

    let validated = assemble(input, meeting_id, set_id, concepts.title.as_deref(), &kept_concepts, drafts, exam_items)
        .map_err(|e| GenError::Failed(format!("The lesson didn't pass its checks ({}). Try again.", e)))?;
    progress(Progress { done: total, total, label: "Done".into() });
    Ok(Made { validated, failed })
}

/// Build the lesson set from its parts and validate it as a whole
/// (generated profile). Only the returned, cleaned structure is stored.
fn assemble(
    input: &StudyInput,
    meeting_id: &str,
    set_id: &str,
    title: Option<&str>,
    concepts: &[ConceptDraft],
    drafts: Vec<LessonDraft>,
    exam_items: Vec<QuizItem>,
) -> Result<schema::ValidatedSet, String> {
    let mut lesson_of: HashMap<String, String> = HashMap::new();
    let mut lessons: Vec<Lesson> = Vec::new();
    let mut exam_bank: Vec<QuizItem> = exam_items;
    let mut seen: HashSet<String> = exam_bank.iter().map(|q| crate::study::parse::norm_key(&q.prompt)).collect();
    for (i, d) in drafts.into_iter().enumerate() {
        let id = format!("l{}", i + 1);
        for cid in &d.outline.concept_ids {
            lesson_of.insert(cid.clone(), id.clone());
        }
        // The exam also draws on every checkpoint question (as its own copy).
        for q in &d.checkpoint {
            if seen.insert(crate::study::parse::norm_key(&q.prompt)) {
                exam_bank.push(QuizItem { id: format!("x-{}", q.id), ..q.clone() });
            }
        }
        let present = d.checkpoint.len().min(CHECKPOINT_QUESTIONS) as u32;
        lessons.push(Lesson {
            id: id.clone(),
            index: (i + 1) as u32,
            title: d.outline.title.clone(),
            objectives: d
                .outline
                .objectives
                .iter()
                .map(|t| schema::Objective { text: t.clone(), concept_ids: d.outline.concept_ids.clone() })
                .collect(),
            concept_ids: d.outline.concept_ids.clone(),
            explainers: d.explainers,
            worked_examples: d.worked_examples,
            misconceptions: Misconceptions { items: d.mistakes },
            trainer: Trainer { id: format!("{}-t", id), concept_ids: d.outline.concept_ids.clone(), generators: Vec::new(), static_items: Vec::new() },
            checkpoint: schema::Checkpoint { present_count: present, items: d.checkpoint },
        });
    }
    exam_bank.truncate(schema::MAX_EXAM_BANK);
    let question_count = exam_bank.len().min(EXAM_QUESTIONS) as u32;
    let known: HashSet<&str> = concepts.iter().map(|c| c.id.as_str()).collect();
    let set = LessonSet {
        id: set_id.to_string(),
        schema_major: crate::lessons::SCHEMA_MAJOR,
        title: title.map(str::to_string).filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            let t = input.title.trim();
            if t.is_empty() { "Lesson".to_string() } else { t.to_string() }
        }),
        subtitle: Some(input.title.trim().to_string()).filter(|t| !t.is_empty()),
        level: input.notebook.clone().map(|n| n.trim().to_string()).unwrap_or_default(),
        provenance: "generated".into(),
        source_recording_id: Some(meeting_id.to_string()),
        transcript_fingerprint: Some(input.fingerprint()),
        lessons: lessons.iter().map(|l| l.id.clone()).collect(),
        concepts: concepts
            .iter()
            .map(|c| Concept {
                id: c.id.clone(),
                name: c.name.clone(),
                summary: c.summary.clone(),
                prerequisites: c.prerequisites.iter().filter(|p| known.contains(p.as_str())).cloned().collect(),
                lesson_ids: lesson_of.get(&c.id).cloned().into_iter().collect(),
            })
            .collect(),
        diagnostic: None,
        exam: Exam {
            items: exam_bank,
            question_count,
            pass_pct: PASS_PCT,
            mix: ExamMix { mc: question_count, numeric: 0, balance: 0, multistep: 0 },
        },
        fingerprint: String::new(),
    };
    let mut lesson_json: HashMap<String, String> = HashMap::new();
    for l in &lessons {
        lesson_json.insert(l.id.clone(), serde_json::to_string(l).map_err(|e| e.to_string())?);
    }
    // The one caller of the generated profile (schema::Profile).
    schema::validate_set(set, Vec::new(), &lesson_json, schema::Profile::Generated)
}

// ═══════════════════════════════════════════════════════════════════════════
// Saving
// ═══════════════════════════════════════════════════════════════════════════

/// The set's files as stored: `lessonset.json` and `lessons/<id>.json`.
fn set_files(v: &schema::ValidatedSet) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut entries = vec![("lessonset.json".to_string(), serde_json::to_vec_pretty(&v.set).map_err(|e| e.to_string())?)];
    for l in &v.lessons {
        entries.push((format!("lessons/{}.json", l.id), serde_json::to_vec_pretty(l).map_err(|e| e.to_string())?));
    }
    Ok(entries)
}

/// Save a made set under `root` (`lessons::lessons_root()` in the app),
/// replacing the recording's previous generated set. Inside one `BEGIN
/// IMMEDIATE` transaction it first checks that the transcript still reads
/// exactly as it did when generation started (`fingerprint`) and that no
/// Delete is in its undo window: a lesson made from text that has since been
/// deleted or stricken must never be written after the purge ran. Returns
/// the new set's id.
pub async fn save_generated(
    pool: &Pool<Sqlite>,
    root: &Path,
    meeting_id: &str,
    fingerprint: &str,
    made: &schema::ValidatedSet,
) -> Result<String, String> {
    let set = &made.set;
    if set.provenance != "generated" || set.source_recording_id.as_deref() != Some(meeting_id) {
        return Err("That lesson wasn't made from this recording.".into());
    }
    let entries = set_files(made)?;
    let content_fingerprint = crate::lessons::bundle::fingerprint_for(&entries, &[])?;

    // Stage the files first, outside the transaction.
    let staging = root.join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
    let final_dir = root.join(&set.id);
    let staged = (|| -> Result<(), String> {
        std::fs::create_dir_all(staging.join("lessons")).map_err(|e| format!("Couldn't save the lesson: {}", e))?;
        for (path, bytes) in &entries {
            std::fs::write(staging.join(path), bytes).map_err(|e| format!("Couldn't save the lesson: {}", e))?;
        }
        Ok(())
    })();
    if let Err(e) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let outcome = commit_generated(pool, meeting_id, fingerprint, set, &content_fingerprint, &staging, &final_dir).await;
    // Whatever happened, the staging dir never outlives the call.
    let _ = std::fs::remove_dir_all(&staging);
    match outcome {
        Ok(old_dirs) => {
            // The previous generated set's files go once its rows are gone (a
            // crash before this leaves folders no row points to; the startup
            // sweep, `purge::sweep_orphans`, removes them).
            crate::lessons::purge::remove_dirs(&old_dirs);
            Ok(set.id.clone())
        }
        Err(e) => {
            // Files placed for a row that didn't commit are removed; a row
            // that did commit (only possible if COMMIT reported an error
            // after succeeding) keeps its files.
            let committed: Option<i64> = sqlx::query_scalar("SELECT 1 FROM lesson_sets WHERE id = ?")
                .bind(&set.id)
                .fetch_optional(pool)
                .await
                .unwrap_or(Some(1));
            if committed.is_none() {
                let _ = std::fs::remove_dir_all(&final_dir);
            }
            Err(e)
        }
    }
}

/// The transaction of `save_generated`: one connection, `BEGIN IMMEDIATE`,
/// the re-checks and writes (`save_locked`), then COMMIT or ROLLBACK. Returns
/// the replaced set's dirs to remove after commit.
async fn commit_generated(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    fingerprint: &str,
    set: &LessonSet,
    content_fingerprint: &str,
    staging: &Path,
    final_dir: &Path,
) -> Result<Vec<String>, String> {
    let mut conn = pool.acquire().await.map_err(|e| format!("Database busy: {}", e))?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await.map_err(|e| format!("Database busy: {}", e))?;
    let result = save_locked(&mut conn, meeting_id, fingerprint, set, content_fingerprint, staging, final_dir).await;
    let end = if result.is_ok() { "COMMIT" } else { "ROLLBACK" };
    if let Err(e) = sqlx::query(end).execute(&mut *conn).await {
        // Never hand a connection with an open transaction back to the pool
        let _ = conn.close().await;
        return Err(match result {
            Err(r) => r,
            Ok(_) => format!("Couldn't save the lesson: {}", e),
        });
    }
    result
}

/// Inside the save transaction: re-check, replace, insert, place the files.
/// Returns the previous generated set's dirs to remove after commit.
async fn save_locked(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    fingerprint: &str,
    set: &LessonSet,
    content_fingerprint: &str,
    staging: &Path,
    final_dir: &Path,
) -> Result<Vec<String>, String> {
    if study::has_pending_delete(conn, meeting_id).await? {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    let now_input = study::load_input_conn(conn, meeting_id).await?;
    if now_input.fingerprint() != fingerprint || set.transcript_fingerprint.as_deref() != Some(fingerprint) {
        return Err(TRANSCRIPT_CHANGED.into());
    }
    // Making again replaces the recording's previous generated set (and its
    // progress, mastery, exam attempts and certificate).
    let (_, old_dirs) = crate::lessons::purge::purge_rows_for_meeting(conn, meeting_id)
        .await
        .map_err(|e| format!("Couldn't save the lesson: {}", e))?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO lesson_sets \
         (id, title, subtitle, level, provenance, source_recording_id, transcript_fingerprint, \
          content_fingerprint, schema_major, dir_path, imported_at) \
         VALUES (?, ?, ?, ?, 'generated', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&set.id)
    .bind(&set.title)
    .bind(&set.subtitle)
    .bind(&set.level)
    .bind(meeting_id)
    .bind(fingerprint)
    .bind(content_fingerprint)
    .bind(crate::lessons::SCHEMA_MAJOR as i64)
    .bind(final_dir.to_string_lossy().to_string())
    .bind(&now)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("Couldn't save the lesson: {}", e))?;
    // Files in place before the row commits, so a reader never sees a row
    // without its files.
    let _ = std::fs::remove_dir_all(final_dir);
    std::fs::rename(staging, final_dir).map_err(|e| format!("Couldn't place the lesson files: {}", e))?;
    Ok(old_dirs)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri command
// ═══════════════════════════════════════════════════════════════════════════

pub mod commands {
    use super::*;
    use crate::AppState;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use tauri::{AppHandle, Emitter, State};

    /// `{ meeting_id, done, total, label }`, like `study_progress`.
    pub const PROGRESS_EVENT: &str = "lessons_progress";

    /// Recordings with a lesson being made (one at a time per recording).
    static RUNNING: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

    struct RunGuard(String);
    impl Drop for RunGuard {
        fn drop(&mut self) {
            RUNNING.lock().remove(&self.0);
        }
    }

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MakeLessonResult {
        pub set_id: String,
        pub title: String,
        pub lessons: usize,
        /// Parts that were unusable twice; the lesson was made without them
        pub failed: Vec<PartFailure>,
    }

    fn recording_meeting(state: &AppState) -> Option<String> {
        if !state.capture_engine.read().is_recording() {
            return None;
        }
        state.state_builder.read().current_meeting_id()
    }

    /// Make (or make again) the lesson for a Class recording. Progress
    /// arrives as `lessons_progress` events. Making again replaces the
    /// recording's previous generated set.
    #[tauri::command(rename_all = "camelCase")]
    pub async fn lessons_make_from_recording(
        app: AppHandle,
        state: State<'_, AppState>,
        meeting_id: String,
    ) -> Result<MakeLessonResult, String> {
        if recording_meeting(&state).as_deref() == Some(meeting_id.as_str()) {
            return Err("Stop the recording first: the lesson is made from the whole recording.".into());
        }
        if !RUNNING.lock().insert(meeting_id.clone()) {
            return Err("A lesson for this recording is already being made.".into());
        }
        let _guard = RunGuard(meeting_id.clone());
        let pool = state.database.pool().clone();
        {
            let mut conn = pool.acquire().await.map_err(|e| format!("Database busy: {}", e))?;
            if study::has_pending_delete(&mut conn, &meeting_id).await? {
                return Err(study::PENDING_EDIT.into());
            }
        }
        let input = study::load_input(&pool, &meeting_id).await?;
        if input.kind != RecordingKind::Class {
            return Err(NOT_A_CLASS.into());
        }
        let fingerprint = input.fingerprint();
        log::info!("Lesson: making a lesson from a {}-line transcript", input.lines.len());
        let mid = meeting_id.clone();
        let app_ev = app.clone();
        let report = move |p: Progress| {
            let _ = app_ev.emit(
                PROGRESS_EVENT,
                serde_json::json!({ "meeting_id": mid, "done": p.done, "total": p.total, "label": p.label }),
            );
        };
        let set_id = uuid::Uuid::new_v4().simple().to_string();
        let made = generate(&study::LiveCompleter, &input, &meeting_id, &set_id, &report).await.map_err(String::from)?;
        let id = save_generated(&pool, &crate::lessons::lessons_root(), &meeting_id, &fingerprint, &made.validated).await?;
        log::info!(
            "Lesson: saved {} lesson(s); {} part(s) reported",
            made.validated.lessons.len(),
            made.failed.len()
        );
        Ok(MakeLessonResult {
            set_id: id,
            title: made.validated.set.title.clone(),
            lessons: made.validated.lessons.len(),
            failed: made.failed,
        })
    }
}
