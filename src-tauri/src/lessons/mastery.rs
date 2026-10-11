//! Per-concept mastery model (docs/LESSONS.md §7). EMA on 0..100 with a
//! first-try-correct bonus and a retry credit. Authoritative in Rust; the
//! stored value is always written here via a command. Deterministic + unit
//! tested with scripted attempt sequences.

/// EMA weight on the newest item score.
pub const ALPHA: f64 = 0.4;
/// Credit for a correct answer that followed a wrong one in the same session.
pub const RETRY_CREDIT: f64 = 70.0;
/// Mastery thresholds.
pub const MASTERED_SCORE: f64 = 85.0;
pub const MASTERED_MIN_CORRECT: u32 = 3;

/// A learner's mutable mastery state for one concept.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mastery {
    pub score: f64,
    pub attempts: u32,
    pub correct_attempts: u32,
    pub first_try_correct: u32,
    pub mastered: bool,
    pub ease: f64,
    pub interval_days: f64,
}

impl Default for Mastery {
    fn default() -> Self {
        Mastery {
            score: 0.0,
            attempts: 0,
            correct_attempts: 0,
            first_try_correct: 0,
            mastered: false,
            ease: crate::lessons::review::START_EASE,
            interval_days: 0.0,
        }
    }
}

/// Record one graded attempt of an item tagging this concept.
/// `correct` = fully correct. `first_try` = correct with no prior wrong attempt
/// this session. `fraction` is the per-step fraction for multistep (0..=1);
/// for mc/numeric/balance it is 1.0 when correct, 0.0 otherwise.
pub fn record_attempt(m: &mut Mastery, correct: bool, first_try: bool, fraction: f64) {
    let item_score = if correct {
        if first_try {
            100.0
        } else {
            RETRY_CREDIT
        }
    } else {
        (fraction.clamp(0.0, 1.0)) * 100.0
    };
    if m.attempts == 0 {
        m.score = item_score;
    } else {
        m.score = ALPHA * item_score + (1.0 - ALPHA) * m.score;
    }
    m.attempts += 1;
    if correct {
        m.correct_attempts += 1;
        if first_try {
            m.first_try_correct += 1;
        }
    }
    m.mastered = m.score >= MASTERED_SCORE && m.correct_attempts >= MASTERED_MIN_CORRECT;
}

/// Seed initial mastery from a diagnostic result (§8.4): a concept answered
/// well on the placement test starts provisionally mastered so a strong learner
/// can test out of early concepts. `correct` / `total` are the item counts for
/// this concept on the diagnostic.
pub fn seed_from_diagnostic(correct: u32, total: u32) -> Mastery {
    let mut m = Mastery::default();
    if total == 0 {
        return m;
    }
    let pct = (correct as f64 / total as f64) * 100.0;
    m.score = pct;
    m.attempts = total;
    m.correct_attempts = correct;
    m.first_try_correct = correct;
    m.mastered = pct >= MASTERED_SCORE && correct >= MASTERED_MIN_CORRECT;
    m
}
