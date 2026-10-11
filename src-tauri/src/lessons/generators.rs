//! Parameterized numeric item generators (docs/LESSONS.md §8.2/§8.3). A closed
//! enum of templates, each with a verified closed-form answer, so a generated
//! item's answer is always correct by construction. Values are drawn with a
//! seeded RNG (seed = (conceptId, attemptIndex)) so a drill is reproducible and
//! unit-testable. `solve_with_steps` emits the dimensional-analysis steps so a
//! miss still teaches with the actual numbers.

use crate::lessons::schema::{Step, Tolerance};

/// The closed set of generator template ids. An unknown template drops the
/// generator at import (schema.rs).
pub const TEMPLATES: &[&str] = &["mole_mass", "mass_mole", "mole_ratio", "gas_volume_stp"];

pub fn is_known_template(template: &str) -> bool {
    TEMPLATES.contains(&template)
}

/// A deterministic xorshift RNG seeded from (conceptId, attemptIndex), so a
/// generated drill is reproducible.
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub fn new(concept_id: &str, attempt_index: u64) -> Self {
        // FNV-1a over the concept id, mixed with the attempt index.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in concept_id.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= attempt_index.wrapping_mul(0x9e3779b97f4a7c15);
        SeededRng { state: h | 1 }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    /// A value in [lo, hi] (integer-ish grams), deterministic.
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % (hi - lo + 1) as u64) as u32
    }
}

/// A molar mass table for the species the generators use (g/mol, 2 dp).
pub fn molar_mass(species: &str) -> Option<f64> {
    let m = match species {
        "H2" => 2.016,
        "O2" => 31.998,
        "H2O" => 18.015,
        "CO2" => 44.009,
        "NaCl" => 58.44,
        "N2" => 28.014,
        "CH4" => 16.043,
        "NH3" => 17.031,
        "C" => 12.011,
        "O" => 15.999,
        _ => return None,
    };
    Some(m)
}

/// A generated numeric item: a prompt, the correct answer + unit + tolerance,
/// and the ordered worked-solution steps.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedItem {
    pub prompt: String,
    pub answer: f64,
    pub unit: String,
    pub tolerance: Tolerance,
    pub steps: Vec<Step>,
}

/// Draw and solve a generator item for a difficulty tier (1=easy..3=hard),
/// seeded by (conceptId, attemptIndex). Returns None for an unknown template or
/// an unknown species draw.
pub fn solve_with_steps(
    template: &str,
    tier: u8,
    concept_id: &str,
    attempt_index: u64,
) -> Option<GeneratedItem> {
    let mut rng = SeededRng::new(concept_id, attempt_index);
    let species_pool: &[&str] = match tier {
        1 => &["H2O"],
        2 => &["H2O", "CO2", "NaCl"],
        _ => &["H2O", "CO2", "NaCl", "NH3", "CH4"],
    };
    let species = species_pool[(rng.next_u64() as usize) % species_pool.len()];
    let mm = molar_mass(species)?;
    let gmax = match tier {
        1 => 20,
        2 => 80,
        _ => 120,
    };
    let grams = rng.range(1, gmax) as f64;
    let tol = Tolerance { kind: "rel".into(), value: 0.01 };
    match template {
        "mole_mass" => {
            // grams → moles
            let moles = grams / mm;
            Some(GeneratedItem {
                prompt: format!("How many moles are in {:.1} g of {}?", grams, species),
                answer: moles,
                unit: "mol".into(),
                tolerance: tol,
                steps: vec![
                    Step {
                        text: format!("Molar mass of {} is {:.3} g/mol.", species, mm),
                        expression: None,
                        result: None,
                        unit: None,
                    },
                    Step {
                        text: "Divide grams by molar mass.".into(),
                        expression: Some(format!("{:.1} g ÷ {:.3} g/mol", grams, mm)),
                        result: Some(format!("{:.4}", moles)),
                        unit: Some("mol".into()),
                    },
                ],
            })
        }
        "mass_mole" => {
            // moles → grams (reuse grams draw as a mole count scaled down)
            let moles = (grams / 10.0).max(0.1);
            let mass = moles * mm;
            Some(GeneratedItem {
                prompt: format!("What is the mass of {:.2} mol of {}?", moles, species),
                answer: mass,
                unit: "g".into(),
                tolerance: tol,
                steps: vec![
                    Step {
                        text: format!("Molar mass of {} is {:.3} g/mol.", species, mm),
                        expression: None,
                        result: None,
                        unit: None,
                    },
                    Step {
                        text: "Multiply moles by molar mass.".into(),
                        expression: Some(format!("{:.2} mol × {:.3} g/mol", moles, mm)),
                        result: Some(format!("{:.3}", mass)),
                        unit: Some("g".into()),
                    },
                ],
            })
        }
        "mole_ratio" => {
            // 2 H2 + O2 → 2 H2O : grams H2O from grams H2
            let mm_h2 = molar_mass("H2")?;
            let mm_h2o = molar_mass("H2O")?;
            let moles_h2 = grams / mm_h2;
            let moles_h2o = moles_h2; // 2:2 ratio
            let mass_h2o = moles_h2o * mm_h2o;
            Some(GeneratedItem {
                prompt: format!("For 2 H2 + O2 → 2 H2O, how many grams of H2O form from {:.1} g of H2?", grams),
                answer: mass_h2o,
                unit: "g".into(),
                tolerance: tol,
                steps: vec![
                    Step {
                        text: "Convert grams H2 to moles.".into(),
                        expression: Some(format!("{:.1} g ÷ {:.3} g/mol", grams, mm_h2)),
                        result: Some(format!("{:.4}", moles_h2)),
                        unit: Some("mol H2".into()),
                    },
                    Step {
                        text: "Apply the 2:2 mole ratio (H2 : H2O).".into(),
                        expression: Some(format!("{:.4} mol × (2/2)", moles_h2)),
                        result: Some(format!("{:.4}", moles_h2o)),
                        unit: Some("mol H2O".into()),
                    },
                    Step {
                        text: "Convert moles H2O to grams.".into(),
                        expression: Some(format!("{:.4} mol × {:.3} g/mol", moles_h2o, mm_h2o)),
                        result: Some(format!("{:.3}", mass_h2o)),
                        unit: Some("g".into()),
                    },
                ],
            })
        }
        "gas_volume_stp" => {
            // moles → volume at STP (22.414 L/mol)
            let moles = (grams / 10.0).max(0.1);
            let vol = moles * 22.414;
            Some(GeneratedItem {
                prompt: format!("What volume does {:.2} mol of an ideal gas occupy at STP?", moles),
                answer: vol,
                unit: "L".into(),
                tolerance: tol,
                steps: vec![Step {
                    text: "At STP one mole occupies 22.414 L.".into(),
                    expression: Some(format!("{:.2} mol × 22.414 L/mol", moles)),
                    result: Some(format!("{:.3}", vol)),
                    unit: Some("L".into()),
                }],
            })
        }
        _ => None,
    }
}
