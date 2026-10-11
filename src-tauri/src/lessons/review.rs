//! Spaced-repetition review, SM-2-lite over the mastery EMA (docs/LESSONS.md
//! §8.1). Deterministic and unit-tested with scripted (quality, review-count)
//! sequences. All local; zero network.

pub const START_EASE: f64 = 2.5;
pub const MIN_EASE: f64 = 1.3;
pub const MAX_EASE: f64 = 2.8;

/// The SM-2-lite review state for one concept.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewState {
    pub ease: f64,
    pub interval_days: f64,
    /// How many reviews have happened (drives the 1 / 3 / interval·ease ladder).
    pub reviews: u32,
}

impl Default for ReviewState {
    fn default() -> Self {
        ReviewState { ease: START_EASE, interval_days: 0.0, reviews: 0 }
    }
}

/// Derive an SM-2 quality 0..5 from the item result and whether it was first
/// try correct. Passing is q >= 3.
pub fn quality_from_result(correct: bool, first_try: bool) -> u8 {
    match (correct, first_try) {
        (true, true) => 5,
        (true, false) => 4,
        (false, _) => 2,
    }
}

/// Apply one review attempt. Returns the number of days until the concept is
/// next due. A passing attempt (q >= 3) advances the interval; a failing one
/// resets it to 1 day and nudges ease down (floored at MIN_EASE).
pub fn apply_review(state: &mut ReviewState, quality: u8) -> f64 {
    let q = quality.min(5) as f64;
    if quality >= 3 {
        // ease ← clamp(ease + (0.1 − (5−q)·(0.08 + (5−q)·0.02)), 1.3, 2.8)
        let delta = 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02);
        state.ease = (state.ease + delta).clamp(MIN_EASE, MAX_EASE);
        state.interval_days = match state.reviews {
            0 => 1.0,
            1 => 3.0,
            _ => state.interval_days * state.ease,
        };
        state.reviews += 1;
    } else {
        state.interval_days = 1.0;
        state.ease = (state.ease - 0.2).max(MIN_EASE);
        state.reviews = 0;
    }
    state.interval_days
}
