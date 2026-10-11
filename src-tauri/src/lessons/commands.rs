//! Tauri commands for the Lessons subsystem (docs/LESSONS.md §17). Taking a
//! lesson is 100% offline and free; the only network command is
//! `lessons_import_bundle` with a URL source (user-initiated, no default URL).
//! `lessons_make_from_recording` is deferred (§13) and is NOT registered.
//!
//! Correctness/grading is authoritative here in Rust. The frontend renders the
//! typed structures and calls these commands to grade and persist.

use crate::lessons::bundle;
use crate::lessons::exam::{self, AnsweredItem};
use crate::lessons::grade::{Given, MultistepGiven};
use crate::lessons::mastery::Mastery;
use crate::lessons::schema::{Lesson, LessonSet, QuizItem};
use crate::lessons::{review, seed};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::{AppHandle, State};

fn pool_of(state: &AppState) -> Pool<Sqlite> {
    state.database.pool().clone()
}

// ── Catalog ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub level: Option<String>,
    pub provenance: String,
    pub source_recording_id: Option<String>,
    pub content_fingerprint: String,
    pub schema_major: i64,
    pub imported_at: String,
}

async fn catalog_rows(pool: &Pool<Sqlite>) -> Result<Vec<CatalogEntry>, String> {
    let rows = sqlx::query(
        "SELECT id, title, subtitle, level, provenance, source_recording_id, content_fingerprint, \
         schema_major, imported_at FROM lesson_sets ORDER BY imported_at ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Couldn't read the lessons catalog: {}", e))?;
    Ok(rows
        .iter()
        .map(|r| CatalogEntry {
            id: r.get("id"),
            title: r.get("title"),
            subtitle: r.get("subtitle"),
            level: r.get("level"),
            provenance: r.get("provenance"),
            source_recording_id: r.get("source_recording_id"),
            content_fingerprint: r.get("content_fingerprint"),
            schema_major: r.get("schema_major"),
            imported_at: r.get("imported_at"),
        })
        .collect())
}

async fn dir_path_of(pool: &Pool<Sqlite>, set_id: &str) -> Result<PathBuf, String> {
    let row = sqlx::query("SELECT dir_path FROM lesson_sets WHERE id = ?")
        .bind(set_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Database busy: {}", e))?
        .ok_or("That lesson set no longer exists")?;
    Ok(PathBuf::from(row.get::<String, _>("dir_path")))
}

fn load_set(dir: &PathBuf) -> Result<LessonSet, String> {
    let bytes = std::fs::read(dir.join("lessonset.json")).map_err(|e| format!("Couldn't read the lesson: {}", e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("The lesson is corrupt: {}", e))
}

fn load_lesson(dir: &PathBuf, lesson_id: &str) -> Result<Lesson, String> {
    let bytes = std::fs::read(dir.join("lessons").join(format!("{}.json", lesson_id)))
        .map_err(|e| format!("Couldn't read the lesson: {}", e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("The lesson is corrupt: {}", e))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_list_catalog(state: State<'_, AppState>) -> Result<Vec<CatalogEntry>, String> {
    catalog_rows(&pool_of(&state)).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_get_set(state: State<'_, AppState>, set_id: String) -> Result<LessonSet, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    load_set(&dir)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_get_lesson(
    state: State<'_, AppState>,
    set_id: String,
    lesson_id: String,
) -> Result<Lesson, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    load_lesson(&dir, &lesson_id)
}

// ── Import / remove ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "source", content = "value")]
pub enum ImportSource {
    File(String),
    Folder(String),
    Url(String),
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_import_bundle(state: State<'_, AppState>, source: ImportSource) -> Result<String, String> {
    let pool = pool_of(&state);
    match source {
        ImportSource::File(p) => bundle::import_from_file(&pool, &PathBuf::from(p)).await,
        ImportSource::Folder(p) => bundle::import_from_folder(&pool, &PathBuf::from(p)).await,
        ImportSource::Url(u) => bundle::import_from_url(&pool, &u).await,
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_remove_set(state: State<'_, AppState>, set_id: String) -> Result<(), String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let provenance: Option<String> = sqlx::query_scalar("SELECT provenance FROM lesson_sets WHERE id = ?")
        .bind(&set_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for table in ["lesson_progress", "mastery_records", "exam_attempts", "certificates"] {
        sqlx::query(&format!("DELETE FROM {} WHERE lesson_set_id = ?", table))
            .bind(&set_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    sqlx::query("DELETE FROM lesson_sets WHERE id = ?")
        .bind(&set_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    // Removing the seed set leaves a tombstone so it is not re-seeded.
    if provenance.as_deref() == Some("seed") {
        let _ = seed::mark_removed(&pool).await;
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

// ── Progress ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LessonProgress {
    #[serde(default)]
    pub explainers_seen: Vec<String>,
    #[serde(default)]
    pub worked_examples_seen: Vec<String>,
    #[serde(default)]
    pub checkpoint_best_pct: f64,
    #[serde(default)]
    pub checkpoint_passed: bool,
    #[serde(default)]
    pub last_viewed_explainer_id: Option<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_save_progress(
    state: State<'_, AppState>,
    set_id: String,
    lesson_id: String,
    progress: LessonProgress,
) -> Result<(), String> {
    let pool = pool_of(&state);
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO lesson_progress \
         (lesson_set_id, lesson_id, explainers_seen, worked_examples_seen, checkpoint_best_pct, \
          checkpoint_passed, last_viewed_explainer_id, started_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, COALESCE((SELECT started_at FROM lesson_progress WHERE lesson_set_id=? AND lesson_id=?), ?), ?) \
         ON CONFLICT(lesson_set_id, lesson_id) DO UPDATE SET \
          explainers_seen=excluded.explainers_seen, worked_examples_seen=excluded.worked_examples_seen, \
          checkpoint_best_pct=MAX(lesson_progress.checkpoint_best_pct, excluded.checkpoint_best_pct), \
          checkpoint_passed=(lesson_progress.checkpoint_passed OR excluded.checkpoint_passed), \
          last_viewed_explainer_id=excluded.last_viewed_explainer_id, updated_at=excluded.updated_at",
    )
    .bind(&set_id)
    .bind(&lesson_id)
    .bind(serde_json::to_string(&progress.explainers_seen).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&progress.worked_examples_seen).unwrap_or_else(|_| "[]".into()))
    .bind(progress.checkpoint_best_pct)
    .bind(progress.checkpoint_passed as i64)
    .bind(&progress.last_viewed_explainer_id)
    .bind(&set_id)
    .bind(&lesson_id)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| format!("Couldn't save your progress: {}", e))?;
    Ok(())
}

// ── Mastery persistence helpers ─────────────────────────────────────────────

async fn load_mastery(pool: &Pool<Sqlite>, set_id: &str, concept_id: &str) -> Mastery {
    let row = sqlx::query(
        "SELECT score, attempts, correct_attempts, first_try_correct, mastered, ease, interval_days, reviews \
         FROM mastery_records WHERE lesson_set_id = ? AND concept_id = ?",
    )
    .bind(set_id)
    .bind(concept_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match row {
        Some(r) => Mastery {
            score: r.get("score"),
            attempts: r.get::<i64, _>("attempts") as u32,
            correct_attempts: r.get::<i64, _>("correct_attempts") as u32,
            first_try_correct: r.get::<i64, _>("first_try_correct") as u32,
            mastered: r.get::<i64, _>("mastered") != 0,
            ease: r.get("ease"),
            interval_days: r.get("interval_days"),
            reviews: r.get::<i64, _>("reviews") as u32,
        },
        None => Mastery::default(),
    }
}

async fn save_mastery(pool: &Pool<Sqlite>, set_id: &str, concept_id: &str, m: &Mastery) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    // When a review ladder is active (reviews > 0, interval > 0), schedule the
    // next due date so the "Review due" surface (§8.1) can resurface it. Until
    // a concept has been reviewed its due_at stays NULL (not yet in the ladder).
    let due_at: Option<String> = if m.reviews > 0 && m.interval_days > 0.0 {
        let secs = (m.interval_days * 86_400.0) as i64;
        Some((chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339())
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO mastery_records \
         (lesson_set_id, concept_id, score, attempts, correct_attempts, first_try_correct, mastered, \
          ease, interval_days, reviews, due_at, last_reviewed_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(lesson_set_id, concept_id) DO UPDATE SET \
          score=excluded.score, attempts=excluded.attempts, correct_attempts=excluded.correct_attempts, \
          first_try_correct=excluded.first_try_correct, mastered=excluded.mastered, ease=excluded.ease, \
          interval_days=excluded.interval_days, reviews=excluded.reviews, due_at=excluded.due_at, \
          last_reviewed_at=excluded.last_reviewed_at, updated_at=excluded.updated_at",
    )
    .bind(set_id)
    .bind(concept_id)
    .bind(m.score)
    .bind(m.attempts as i64)
    .bind(m.correct_attempts as i64)
    .bind(m.first_try_correct as i64)
    .bind(m.mastered as i64)
    .bind(m.ease)
    .bind(m.interval_days)
    .bind(m.reviews as i64)
    .bind(&due_at)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| format!("Couldn't save mastery: {}", e))?;
    Ok(())
}

// ── Submitted answers (shared shape) ─────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmittedAnswer {
    pub item_id: String,
    #[serde(default)]
    pub choice_index: Option<u32>,
    #[serde(default)]
    pub numeric: Option<String>,
    #[serde(default)]
    pub coefficients: Option<Vec<u32>>,
    #[serde(default)]
    pub multistep: Option<Vec<SubmittedStep>>,
    #[serde(default)]
    pub first_try: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmittedStep {
    #[serde(default)]
    pub choice_index: Option<u32>,
    #[serde(default)]
    pub numeric: Option<String>,
    #[serde(default)]
    pub coefficients: Option<Vec<u32>>,
}

fn to_given(item: &QuizItem, a: &SubmittedAnswer) -> Given {
    match item.kind.as_str() {
        "mc" => Given::Mc(a.choice_index.unwrap_or(u32::MAX)),
        "numeric" => Given::Numeric(a.numeric.clone().unwrap_or_default()),
        "balance" => Given::Balance(a.coefficients.clone().unwrap_or_default()),
        "multistep" => Given::Multistep(
            a.multistep
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|s| {
                    if let Some(c) = s.choice_index {
                        MultistepGiven::Mc(c)
                    } else if let Some(coeffs) = s.coefficients {
                        MultistepGiven::Balance(coeffs)
                    } else {
                        MultistepGiven::Numeric(s.numeric.unwrap_or_default())
                    }
                })
                .collect(),
        ),
        _ => Given::Mc(u32::MAX),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradedItem {
    pub item_id: String,
    pub earned: f64,
    pub possible: f64,
    pub correct: bool,
}

/// Grade `items` (the full presented/selected set, authoritative in Rust)
/// against the client's submitted answers and update per-concept mastery.
///
/// Scoring is over the **server-known** item set, not the posted subset: every
/// item in `items` is graded, and an item the client omitted (or sent blank)
/// grades to zero via its default `Given`. This closes the gap where a
/// one-item submission could score 100% and unlock a certificate.
///
/// `advance_review` gates the SM-2-lite review ladder: pass `true` only once a
/// checkpoint is actually passed (LESSONS.md §8.1), `false` for a failing
/// checkpoint or exam attempt so `due_at` is not written early.
async fn grade_and_update_mastery(
    pool: &Pool<Sqlite>,
    set_id: &str,
    items: &[QuizItem],
    answers: &[SubmittedAnswer],
    advance_review: bool,
) -> Vec<GradedItem> {
    let by_answer: HashMap<&str, &SubmittedAnswer> = answers.iter().map(|a| (a.item_id.as_str(), a)).collect();
    let mut out = Vec::new();
    for item in items {
        let submitted = by_answer.get(item.id.as_str()).copied();
        // An omitted item is an unanswered item: a default `Given` grades to zero.
        let given = match submitted {
            Some(a) => to_given(item, a),
            None => to_given(item, &SubmittedAnswer::default()),
        };
        let first_try = submitted.map(|a| a.first_try).unwrap_or(false);
        let g = crate::lessons::grade::grade_item(item, &given);
        let correct = g.is_full();
        // Update mastery for each tagged concept.
        for cid in &item.concept_ids {
            let mut m = load_mastery(pool, set_id, cid).await;
            crate::lessons::mastery::record_attempt(&mut m, correct, first_try, g.fraction());
            if advance_review {
                let q = review::quality_from_result(correct, first_try);
                let mut rs = review::ReviewState { ease: m.ease, interval_days: m.interval_days, reviews: m.reviews };
                m.interval_days = review::apply_review(&mut rs, q);
                m.ease = rs.ease;
                m.reviews = rs.reviews;
            }
            let _ = save_mastery(pool, set_id, cid, &m).await;
        }
        out.push(GradedItem { item_id: item.id.clone(), earned: g.earned, possible: g.possible, correct });
    }
    out
}

// ── Checkpoint ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointResult {
    pub score_pct: f64,
    pub passed: bool,
    pub graded: Vec<GradedItem>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_submit_checkpoint(
    state: State<'_, AppState>,
    set_id: String,
    lesson_id: String,
    answers: Vec<SubmittedAnswer>,
) -> Result<CheckpointResult, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let lesson = load_lesson(&dir, &lesson_id)?;

    // Score against the items the learner was presented, not the posted subset:
    // grade only the submitted answers that map to the checkpoint bank, but hold
    // the denominator to at least `presentCount` so a short submission cannot
    // game the 80% pass. Unaccounted presented slots count as zero.
    let bank_ids: HashSet<&str> = lesson.checkpoint.items.iter().map(|i| i.id.as_str()).collect();
    let presented: Vec<SubmittedAnswer> =
        answers.into_iter().filter(|a| bank_ids.contains(a.item_id.as_str())).collect();
    let present_count = lesson.checkpoint.present_count.max(1) as usize;

    // Grade purely first (no mastery side effects) so `passed` can gate the
    // review-ladder advance in the single mastery update that follows.
    let by_id: HashMap<&str, &QuizItem> = lesson.checkpoint.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut graded: Vec<GradedItem> = Vec::new();
    for a in &presented {
        let Some(item) = by_id.get(a.item_id.as_str()) else { continue };
        let g = crate::lessons::grade::grade_item(item, &to_given(item, a));
        graded.push(GradedItem { item_id: a.item_id.clone(), earned: g.earned, possible: g.possible, correct: g.is_full() });
    }
    let earned: f64 = graded.iter().map(|g| g.earned).sum();
    // Denominator = graded points plus one point for each unanswered presented slot.
    let graded_possible: f64 = graded.iter().map(|g| g.possible).sum();
    let missing_slots = present_count.saturating_sub(graded.len()) as f64;
    let possible = graded_possible + missing_slots;
    let score_pct = if possible > 0.0 { earned / possible * 100.0 } else { 0.0 };
    let passed = score_pct >= 80.0;

    // Update per-concept mastery once over the presented answers, advancing the
    // SM-2-lite review ladder only when the checkpoint is passed (LESSONS.md
    // §8.1) so `due_at` is not written on a failing attempt. Mastery is updated
    // only for the items actually presented, so unanswered bank items don't
    // record a spurious wrong attempt.
    let presented_items: Vec<QuizItem> = presented
        .iter()
        .filter_map(|a| by_id.get(a.item_id.as_str()).map(|i| (*i).clone()))
        .collect();
    let _ = grade_and_update_mastery(&pool, &set_id, &presented_items, &presented, passed).await;

    // Persist best score + passed flag on the lesson's progress row.
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO lesson_progress (lesson_set_id, lesson_id, checkpoint_best_pct, checkpoint_passed, started_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(lesson_set_id, lesson_id) DO UPDATE SET \
          checkpoint_best_pct=MAX(lesson_progress.checkpoint_best_pct, excluded.checkpoint_best_pct), \
          checkpoint_passed=(lesson_progress.checkpoint_passed OR excluded.checkpoint_passed), \
          updated_at=excluded.updated_at",
    )
    .bind(&set_id)
    .bind(&lesson_id)
    .bind(score_pct)
    .bind(passed as i64)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(CheckpointResult { score_pct, passed, graded })
}

// ── Trainer ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainerItem {
    pub concept_id: String,
    pub tier: u8,
    pub prompt: String,
    pub answer: f64,
    pub unit: String,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_next_trainer_item(
    state: State<'_, AppState>,
    set_id: String,
    attempt_index: u64,
) -> Result<Option<TrainerItem>, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let set = load_set(&dir)?;
    // Build concept state snapshot from mastery + the first generator per concept.
    let mut generators: HashMap<String, String> = HashMap::new();
    for lid in &set.lessons {
        if let Ok(lesson) = load_lesson(&dir, lid) {
            for g in &lesson.trainer.generators {
                for cid in &g.concept_ids {
                    generators.entry(cid.clone()).or_insert_with(|| g.template.clone());
                }
            }
        }
    }
    let mut states = Vec::new();
    for c in &set.concepts {
        let m = load_mastery(&pool, &set_id, &c.id).await;
        states.push(crate::lessons::trainer::ConceptState {
            concept_id: c.id.clone(),
            score: m.score,
            mastered: m.mastered,
            overdue_days: 0.0,
            generator: generators.get(&c.id).cloned(),
        });
    }
    match crate::lessons::trainer::next_item(&states, attempt_index) {
        Some(crate::lessons::trainer::NextItem::Generated { concept_id, tier, item }) => Ok(Some(TrainerItem {
            concept_id,
            tier,
            prompt: item.prompt,
            answer: item.answer,
            unit: item.unit,
        })),
        Some(crate::lessons::trainer::NextItem::Static { concept_id, tier }) => Ok(Some(TrainerItem {
            concept_id,
            tier,
            prompt: String::new(),
            answer: 0.0,
            unit: String::new(),
        })),
        None => Ok(None),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_submit_trainer_item(
    state: State<'_, AppState>,
    set_id: String,
    concept_id: String,
    correct: bool,
    first_try: bool,
) -> Result<Mastery, String> {
    let pool = pool_of(&state);
    let mut m = load_mastery(&pool, &set_id, &concept_id).await;
    crate::lessons::mastery::record_attempt(&mut m, correct, first_try, if correct { 1.0 } else { 0.0 });
    let q = review::quality_from_result(correct, first_try);
    let mut rs = review::ReviewState { ease: m.ease, interval_days: m.interval_days, reviews: m.reviews };
    m.interval_days = review::apply_review(&mut rs, q);
    m.ease = rs.ease;
    m.reviews = rs.reviews;
    save_mastery(&pool, &set_id, &concept_id, &m).await?;
    Ok(m)
}

// ── Review due ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueConcept {
    pub concept_id: String,
    pub score: f64,
    pub due_at: Option<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_review_due(state: State<'_, AppState>, set_id: String) -> Result<Vec<DueConcept>, String> {
    let pool = pool_of(&state);
    let now = chrono::Utc::now().to_rfc3339();
    let rows = sqlx::query(
        "SELECT concept_id, score, due_at FROM mastery_records \
         WHERE lesson_set_id = ? AND due_at IS NOT NULL AND due_at <= ? ORDER BY score ASC",
    )
    .bind(&set_id)
    .bind(&now)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| DueConcept {
            concept_id: r.get("concept_id"),
            score: r.get("score"),
            due_at: r.get("due_at"),
        })
        .collect())
}

// ── Diagnostic ──────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_start_diagnostic(
    state: State<'_, AppState>,
    set_id: String,
) -> Result<Option<crate::lessons::schema::Diagnostic>, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    Ok(load_set(&dir)?.diagnostic)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_submit_diagnostic(
    state: State<'_, AppState>,
    set_id: String,
    answers: Vec<SubmittedAnswer>,
) -> Result<Vec<GradedItem>, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let set = load_set(&dir)?;
    let diagnostic = set.diagnostic.ok_or("This lesson set has no diagnostic")?;
    // Count correct per concept, then seed mastery.
    let by_id: HashMap<&str, &QuizItem> = diagnostic.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut per_concept: HashMap<String, (u32, u32)> = HashMap::new();
    let mut graded = Vec::new();
    for a in &answers {
        let Some(item) = by_id.get(a.item_id.as_str()) else { continue };
        let g = crate::lessons::grade::grade_item(item, &to_given(item, a));
        let correct = g.is_full();
        for cid in &item.concept_ids {
            let e = per_concept.entry(cid.clone()).or_insert((0, 0));
            e.1 += 1;
            if correct {
                e.0 += 1;
            }
        }
        graded.push(GradedItem { item_id: a.item_id.clone(), earned: g.earned, possible: g.possible, correct });
    }
    for (cid, (correct, total)) in per_concept {
        let m = crate::lessons::mastery::seed_from_diagnostic(correct, total);
        save_mastery(&pool, &set_id, &cid, &m).await?;
    }
    Ok(graded)
}

// ── Dashboard ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptMastery {
    pub concept_id: String,
    pub name: String,
    pub score: f64,
    pub mastered: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub set_id: String,
    pub concepts: Vec<ConceptMastery>,
    pub checkpoints_passed: u32,
    pub total_lessons: u32,
    pub exam_unlocked: bool,
    pub readiness_pct: f64,
    pub readiness_label: String,
}

async fn exam_item_counts(set: &LessonSet) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for item in &set.exam.items {
        for cid in &item.concept_ids {
            *counts.entry(cid.clone()).or_insert(0) += 1;
        }
    }
    counts
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_dashboard(state: State<'_, AppState>, set_id: String) -> Result<Dashboard, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let set = load_set(&dir)?;

    let mut concepts = Vec::new();
    let mut scores: HashMap<String, f64> = HashMap::new();
    for c in &set.concepts {
        let m = load_mastery(&pool, &set_id, &c.id).await;
        scores.insert(c.id.clone(), m.score);
        concepts.push(ConceptMastery {
            concept_id: c.id.clone(),
            name: c.name.clone(),
            score: m.score,
            mastered: m.mastered,
        });
    }

    let passed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM lesson_progress WHERE lesson_set_id = ? AND checkpoint_passed = 1",
    )
    .bind(&set_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    let total = set.lessons.len() as u32;
    let exam_unlocked = passed as u32 >= total && total > 0;

    let counts = exam_item_counts(&set).await;
    let readiness_pct = exam::readiness(&scores, &counts);

    Ok(Dashboard {
        set_id,
        concepts,
        checkpoints_passed: passed as u32,
        total_lessons: total,
        exam_unlocked,
        readiness_pct,
        readiness_label: exam::readiness_label(readiness_pct).to_string(),
    })
}

// ── Exam ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedExam {
    pub attempt_id: String,
    pub items: Vec<QuizItem>,
}

async fn recent_item_ids(pool: &Pool<Sqlite>, set_id: &str) -> HashSet<String> {
    let rows = sqlx::query(
        "SELECT answers FROM exam_attempts WHERE lesson_set_id = ? ORDER BY started_at DESC LIMIT 2",
    )
    .bind(set_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut seen = HashSet::new();
    for r in rows {
        if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(&r.get::<String, _>("answers")) {
            for a in v {
                if let Some(id) = a.get("itemId").and_then(|x| x.as_str()) {
                    seen.insert(id.to_string());
                }
            }
        }
    }
    seen
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_start_exam(state: State<'_, AppState>, set_id: String) -> Result<StartedExam, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let set = load_set(&dir)?;

    // Gate: all lessons' checkpoints must be passed.
    let passed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM lesson_progress WHERE lesson_set_id = ? AND checkpoint_passed = 1",
    )
    .bind(&set_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    if (passed as usize) < set.lessons.len() {
        return Err("Pass every lesson checkpoint before the exam unlocks.".into());
    }

    let attempt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM exam_attempts WHERE lesson_set_id = ?")
        .bind(&set_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let recent = recent_item_ids(&pool, &set_id).await;
    let chosen_ids = exam::select_items(&set.exam, &set_id, attempt_count as u64, &recent);
    let by_id: HashMap<&str, &QuizItem> = set.exam.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let items: Vec<QuizItem> = chosen_ids.iter().filter_map(|id| by_id.get(id.as_str()).map(|i| (*i).clone())).collect();

    let attempt_id = uuid::Uuid::new_v4().simple().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // Persist the drawn item ids so the submit scores against the server-known
    // selection (unanswered → zero), not whatever subset the client posts.
    let selected_json = serde_json::to_string(&chosen_ids).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "INSERT INTO exam_attempts (id, lesson_set_id, started_at, answers, selected_item_ids) \
         VALUES (?, ?, ?, '[]', ?)",
    )
    .bind(&attempt_id)
    .bind(&set_id)
    .bind(&now)
    .bind(&selected_json)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(StartedExam { attempt_id, items })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamResult {
    pub score_pct: f64,
    pub passed: bool,
    pub graded: Vec<GradedItem>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_submit_exam(
    state: State<'_, AppState>,
    set_id: String,
    attempt_id: String,
    answers: Vec<SubmittedAnswer>,
) -> Result<ExamResult, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    let set = load_set(&dir)?;
    let by_id: HashMap<&str, &QuizItem> = set.exam.items.iter().map(|i| (i.id.as_str(), i)).collect();

    // Load the server-known selection drawn at `lessons_start_exam`. The exam is
    // scored over exactly these ids (unanswered → zero), never the posted subset,
    // so a short submission cannot score 100% and earn a certificate.
    let selected_json: String =
        sqlx::query_scalar("SELECT selected_item_ids FROM exam_attempts WHERE id = ? AND lesson_set_id = ?")
            .bind(&attempt_id)
            .bind(&set_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("That exam attempt no longer exists")?;
    let selected_ids: Vec<String> = serde_json::from_str(&selected_json).unwrap_or_default();
    if selected_ids.is_empty() {
        return Err("That exam attempt has no recorded question set.".into());
    }
    let selected_items: Vec<QuizItem> =
        selected_ids.iter().filter_map(|id| by_id.get(id.as_str()).map(|i| (*i).clone())).collect();

    // Build the scored attempt over the full selected set: an item the client
    // omitted (or sent blank) grades to zero via its default `Given`.
    let by_answer: HashMap<&str, &SubmittedAnswer> = answers.iter().map(|a| (a.item_id.as_str(), a)).collect();
    let givens: Vec<(QuizItem, Given)> = selected_items
        .iter()
        .map(|item| {
            let given = match by_answer.get(item.id.as_str()) {
                Some(a) => to_given(item, a),
                None => to_given(item, &SubmittedAnswer::default()),
            };
            (item.clone(), given)
        })
        .collect();
    let answered: Vec<AnsweredItem> =
        givens.iter().map(|(item, given)| AnsweredItem { item, given: given.clone() }).collect();
    let scored = exam::score_attempt(&answered, set.exam.pass_pct);

    // Update mastery over the selected set (unanswered → zero). The exam does not
    // advance the SM-2-lite review ladder (that is checkpoint/trainer-driven).
    let graded = grade_and_update_mastery(&pool, &set_id, &selected_items, &answers, false).await;

    // Persist the attempt.
    let answer_json: Vec<serde_json::Value> = graded
        .iter()
        .map(|g| serde_json::json!({ "itemId": g.item_id, "correct": g.correct, "pointsEarned": g.earned, "pointsPossible": g.possible }))
        .collect();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE exam_attempts SET submitted_at = ?, answers = ?, score_pct = ?, passed = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(serde_json::to_string(&answer_json).unwrap_or_else(|_| "[]".into()))
    .bind(scored.score_pct)
    .bind(scored.passed as i64)
    .bind(&attempt_id)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    // Issue a certificate on a pass.
    if scored.passed {
        let fingerprint: String = sqlx::query_scalar("SELECT content_fingerprint FROM lesson_sets WHERE id = ?")
            .bind(&set_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let hash = exam::result_hash(&fingerprint, scored.score_pct, &now, &set_id);
        let cert_id = uuid::Uuid::new_v4().simple().to_string();
        let _ = sqlx::query(
            "INSERT INTO certificates (id, lesson_set_id, exam_attempt_id, course_title, level, score_pct, issued_at, provenance, result_hash) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&cert_id)
        .bind(&set_id)
        .bind(&attempt_id)
        .bind(&set.title)
        .bind(&set.level)
        .bind(scored.score_pct)
        .bind(&now)
        .bind(&set.provenance)
        .bind(&hash)
        .execute(&pool)
        .await;
    }

    Ok(ExamResult { score_pct: scored.score_pct, passed: scored.passed, graded })
}

// ── Certificate / reset ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Certificate {
    pub id: String,
    pub course_title: String,
    pub level: Option<String>,
    pub score_pct: f64,
    pub issued_at: String,
    pub provenance: String,
    pub result_hash: String,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_export_certificate(
    _app: AppHandle,
    state: State<'_, AppState>,
    set_id: String,
) -> Result<Option<Certificate>, String> {
    let pool = pool_of(&state);
    let row = sqlx::query(
        "SELECT id, course_title, level, score_pct, issued_at, provenance, result_hash \
         FROM certificates WHERE lesson_set_id = ? ORDER BY issued_at DESC LIMIT 1",
    )
    .bind(&set_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|r| Certificate {
        id: r.get("id"),
        course_title: r.get("course_title"),
        level: r.get("level"),
        score_pct: r.get("score_pct"),
        issued_at: r.get("issued_at"),
        provenance: r.get("provenance"),
        result_hash: r.get("result_hash"),
    }))
}

/// The on-disk catalog directory for a set, so the frontend can build LOCAL
/// Tauri asset URLs for the set's figure PNGs (never a remote fetch).
#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_set_dir(state: State<'_, AppState>, set_id: String) -> Result<String, String> {
    let pool = pool_of(&state);
    let dir = dir_path_of(&pool, &set_id).await?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_reset_progress(state: State<'_, AppState>, set_id: String) -> Result<(), String> {
    let pool = pool_of(&state);
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for table in ["lesson_progress", "mastery_records", "exam_attempts", "certificates"] {
        sqlx::query(&format!("DELETE FROM {} WHERE lesson_set_id = ?", table))
            .bind(&set_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the saved per-lesson progress so the player can resume where the
/// learner left off (which explainers/worked examples were seen, the last
/// viewed explainer, and the best checkpoint result). Returns `None` if the
/// lesson has never been opened. Read-only; no network.
#[tauri::command(rename_all = "camelCase")]
pub async fn lessons_get_progress(
    state: State<'_, AppState>,
    set_id: String,
    lesson_id: String,
) -> Result<Option<LessonProgress>, String> {
    let pool = pool_of(&state);
    let row = sqlx::query(
        "SELECT explainers_seen, worked_examples_seen, checkpoint_best_pct, \
         checkpoint_passed, last_viewed_explainer_id \
         FROM lesson_progress WHERE lesson_set_id = ? AND lesson_id = ?",
    )
    .bind(&set_id)
    .bind(&lesson_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|r| {
        let parse = |s: Option<String>| -> Vec<String> {
            s.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
        };
        LessonProgress {
            explainers_seen: parse(r.get("explainers_seen")),
            worked_examples_seen: parse(r.get("worked_examples_seen")),
            checkpoint_best_pct: r.get("checkpoint_best_pct"),
            checkpoint_passed: r.get::<i64, _>("checkpoint_passed") != 0,
            last_viewed_explainer_id: r.get("last_viewed_explainer_id"),
        }
    }))
}
