//! Adaptive trainer selection (docs/LESSONS.md §8.2). Targets the weakest
//! unmastered (or most-overdue) concept and scales difficulty by its mastery.
//! Deterministic given the mastery snapshot + attempt index.

use crate::lessons::generators::{self, GeneratedItem};

/// A per-concept snapshot the trainer ranks over.
#[derive(Debug, Clone)]
pub struct ConceptState {
    pub concept_id: String,
    pub score: f64,
    pub mastered: bool,
    /// Days overdue for review (negative = not yet due). Larger = more overdue.
    pub overdue_days: f64,
    /// The generator template bound to this concept, if any.
    pub generator: Option<String>,
}

/// The difficulty tier for a mastery score (§8.2): <40 easy, 40..70 medium,
/// >=70 hard.
pub fn difficulty_tier(score: f64) -> u8 {
    if score < 40.0 {
        1
    } else if score < 70.0 {
        2
    } else {
        3
    }
}

/// Pick the focus concept: lowest `(mastered asc, score asc, overdue desc)`.
pub fn focus_concept(states: &[ConceptState]) -> Option<&ConceptState> {
    states.iter().min_by(|a, b| {
        a.mastered
            .cmp(&b.mastered)
            .then(a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
            .then(
                b.overdue_days
                    .partial_cmp(&a.overdue_days)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    })
}

/// What the trainer serves next.
#[derive(Debug, Clone)]
pub enum NextItem {
    /// A freshly drawn, generator-produced numeric item for a concept/tier.
    Generated { concept_id: String, tier: u8, item: GeneratedItem },
    /// Fall back to a static item from the trainer bank (by concept + tier).
    Static { concept_id: String, tier: u8 },
}

/// Choose the next trainer item for the given concept snapshot. `attempt_index`
/// drives the seeded generator so consecutive draws differ.
pub fn next_item(states: &[ConceptState], attempt_index: u64) -> Option<NextItem> {
    let focus = focus_concept(states)?;
    let tier = difficulty_tier(focus.score);
    if let Some(template) = focus.generator.as_deref() {
        if let Some(item) = generators::solve_with_steps(template, tier, &focus.concept_id, attempt_index) {
            return Some(NextItem::Generated {
                concept_id: focus.concept_id.clone(),
                tier,
                item,
            });
        }
    }
    Some(NextItem::Static { concept_id: focus.concept_id.clone(), tier })
}
