//! Final exam selection, scoring, readiness, and certificate (docs/LESSONS.md
//! §10). 30-item selection honoring the mix 12/12/0/6 without replacement
//! across the two most recent attempts; generator-backed numeric items draw
//! fresh values; score_attempt grades at 80% pass with gated multistep;
//! readiness meter is deterministic; the certificate resultHash is a local
//! SHA-256 stamp. Authoritative in Rust.

use crate::lessons::grade::{self, Given, ItemGrade};
use crate::lessons::schema::{Exam, QuizItem};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// A deterministic draw RNG seeded by (lessonSetId, attemptIndex).
struct DrawRng {
    state: u64,
}
impl DrawRng {
    fn new(set_id: &str, attempt_index: u64) -> Self {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in set_id.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= attempt_index.wrapping_mul(0x9e3779b97f4a7c15);
        DrawRng { state: h | 1 }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        let n = v.len();
        for i in (1..n).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Select the exam items for an attempt, honoring `mix` and preferring items
/// not drawn in the two most recent attempts (`recent` = item ids). Returns the
/// chosen item ids in presentation order.
pub fn select_items(
    exam: &Exam,
    set_id: &str,
    attempt_index: u64,
    recent: &HashSet<String>,
) -> Vec<String> {
    let mut rng = DrawRng::new(set_id, attempt_index);
    let mut chosen = Vec::new();
    for (kind, want) in [
        ("mc", exam.mix.mc),
        ("numeric", exam.mix.numeric),
        ("multistep", exam.mix.multistep),
    ] {
        let mut pool: Vec<&QuizItem> = exam.items.iter().filter(|i| i.kind == kind).collect();
        rng.shuffle(&mut pool);
        // Prefer unseen items, then fall back to recent ones.
        let (mut fresh, mut seen): (Vec<&QuizItem>, Vec<&QuizItem>) =
            pool.into_iter().partition(|i| !recent.contains(&i.id));
        let mut picked = 0u32;
        for src in [&mut fresh, &mut seen] {
            while picked < want {
                let Some(item) = src.pop() else { break };
                chosen.push(item.id.clone());
                picked += 1;
            }
        }
    }
    chosen
}

/// One answered item in a scored attempt.
pub struct AnsweredItem<'a> {
    pub item: &'a QuizItem,
    pub given: Given,
}

/// The scored result of an exam attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredAttempt {
    pub earned: f64,
    pub possible: f64,
    pub score_pct: f64,
    pub passed: bool,
    pub per_item: Vec<ItemGrade>,
}

/// Grade an attempt: per-item grading, total points, pass at `pass_pct`.
pub fn score_attempt(answered: &[AnsweredItem], pass_pct: u32) -> ScoredAttempt {
    let mut earned = 0.0;
    let mut possible = 0.0;
    let mut per_item = Vec::new();
    for a in answered {
        let g = grade::grade_item(a.item, &a.given);
        earned += g.earned;
        possible += g.possible;
        per_item.push(g);
    }
    let score_pct = if possible > 0.0 { earned / possible * 100.0 } else { 0.0 };
    ScoredAttempt {
        earned,
        possible,
        score_pct,
        passed: score_pct >= pass_pct as f64,
        per_item,
    }
}

/// Exam-readiness (§10.3): `mean(min(score/85, 1))` weighted by each concept's
/// exam-item count. Deterministic, 0..=100. `concept_scores` maps conceptId →
/// current mastery score; `exam_item_counts` maps conceptId → how many exam
/// items tag it (the weight).
pub fn readiness(
    concept_scores: &std::collections::HashMap<String, f64>,
    exam_item_counts: &std::collections::HashMap<String, u32>,
) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (cid, &weight) in exam_item_counts {
        if weight == 0 {
            continue;
        }
        let score = concept_scores.get(cid).copied().unwrap_or(0.0);
        let frac = (score / crate::lessons::mastery::MASTERED_SCORE).min(1.0);
        num += frac * weight as f64;
        den += weight as f64;
    }
    if den <= 0.0 {
        0.0
    } else {
        (num / den) * 100.0
    }
}

/// The plain-language readiness band.
pub fn readiness_label(readiness_pct: f64) -> &'static str {
    if readiness_pct >= 85.0 {
        "ready"
    } else if readiness_pct >= 60.0 {
        "almost"
    } else {
        "not ready"
    }
}

/// The certificate's local verification stamp (§10.4): SHA-256 over
/// (contentFingerprint, scorePct, issuedAt, lessonSetId). Cosmetic + local, not
/// a server attestation.
pub fn result_hash(content_fingerprint: &str, score_pct: f64, issued_at: &str, lesson_set_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(content_fingerprint.as_bytes());
    h.update(b"|");
    h.update(format!("{:.4}", score_pct).as_bytes());
    h.update(b"|");
    h.update(issued_at.as_bytes());
    h.update(b"|");
    h.update(lesson_set_id.as_bytes());
    format!("{:x}", h.finalize())
}
