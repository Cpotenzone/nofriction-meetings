//! Grading, authoritative in Rust (docs/LESSONS.md §9). One shared numeric
//! checker with a single UNITS table (dimension class + scale factor/offset),
//! an order-independent balance checker, MC via the stored answerIndex, and
//! multistep per-step/gated grading. Nothing here panics on hostile input.

use crate::lessons::explain;
use crate::lessons::schema::{Equation, MultistepStep, QuizItem, Tolerance};

/// A unit: its dimension class, scale factor into the base unit, and an offset
/// (only °C→K uses a non-zero offset). value_in_base = given*factor + offset.
struct Unit {
    name: &'static str,
    dim: &'static str,
    factor: f64,
    offset: f64,
}

/// The single units table (§9). Adding a unit is one row.
const UNITS: &[Unit] = &[
    // mass (base g)
    Unit { name: "g", dim: "mass", factor: 1.0, offset: 0.0 },
    Unit { name: "grams", dim: "mass", factor: 1.0, offset: 0.0 },
    Unit { name: "gram", dim: "mass", factor: 1.0, offset: 0.0 },
    Unit { name: "mg", dim: "mass", factor: 1e-3, offset: 0.0 },
    Unit { name: "µg", dim: "mass", factor: 1e-6, offset: 0.0 },
    Unit { name: "ug", dim: "mass", factor: 1e-6, offset: 0.0 },
    Unit { name: "kg", dim: "mass", factor: 1e3, offset: 0.0 },
    // volume (base L)
    Unit { name: "l", dim: "volume", factor: 1.0, offset: 0.0 },
    Unit { name: "liters", dim: "volume", factor: 1.0, offset: 0.0 },
    Unit { name: "liter", dim: "volume", factor: 1.0, offset: 0.0 },
    Unit { name: "ml", dim: "volume", factor: 1e-3, offset: 0.0 },
    Unit { name: "dl", dim: "volume", factor: 0.1, offset: 0.0 },
    Unit { name: "cm³", dim: "volume", factor: 1e-3, offset: 0.0 },
    Unit { name: "cm3", dim: "volume", factor: 1e-3, offset: 0.0 },
    Unit { name: "cc", dim: "volume", factor: 1e-3, offset: 0.0 },
    // amount (base mol)
    Unit { name: "mol", dim: "amount", factor: 1.0, offset: 0.0 },
    Unit { name: "mole", dim: "amount", factor: 1.0, offset: 0.0 },
    Unit { name: "moles", dim: "amount", factor: 1.0, offset: 0.0 },
    Unit { name: "mmol", dim: "amount", factor: 1e-3, offset: 0.0 },
    // pressure (base atm)
    Unit { name: "atm", dim: "pressure", factor: 1.0, offset: 0.0 },
    Unit { name: "kpa", dim: "pressure", factor: 1.0 / 101.325, offset: 0.0 },
    Unit { name: "mmhg", dim: "pressure", factor: 1.0 / 760.0, offset: 0.0 },
    Unit { name: "torr", dim: "pressure", factor: 1.0 / 760.0, offset: 0.0 },
    Unit { name: "pa", dim: "pressure", factor: 1.0 / 101325.0, offset: 0.0 },
    // temperature (base K) — °C uses an additive offset, not a factor
    Unit { name: "k", dim: "temperature", factor: 1.0, offset: 0.0 },
    Unit { name: "°c", dim: "temperature", factor: 1.0, offset: 273.15 },
    Unit { name: "c", dim: "temperature", factor: 1.0, offset: 273.15 },
    Unit { name: "°k", dim: "temperature", factor: 1.0, offset: 0.0 },
    // molarity (base mol/L) — "M" and "mol/L" are the same concentration unit
    Unit { name: "mol/l", dim: "molarity", factor: 1.0, offset: 0.0 },
    Unit { name: "m", dim: "molarity", factor: 1.0, offset: 0.0 },
    Unit { name: "mm", dim: "molarity", factor: 1e-3, offset: 0.0 },
    // molar mass (base g/mol) — its own dimension so it never cross-matches mass
    Unit { name: "g/mol", dim: "molar_mass", factor: 1.0, offset: 0.0 },
    // percent (dimensionless, base %) — used by percent-yield/composition items
    Unit { name: "%", dim: "percent", factor: 1.0, offset: 0.0 },
    Unit { name: "percent", dim: "percent", factor: 1.0, offset: 0.0 },
];

/// Public, test-visible wrapper over the private unit lookup so the seed
/// validator can assert every authored numeric `unit` round-trips through the
/// grader's own table (an authored unit the grader rejects is a build break).
pub fn unit_is_gradeable(token: &str) -> bool {
    lookup_unit(token).is_some()
}

fn lookup_unit(token: &str) -> Option<&'static Unit> {
    let t = token.trim().to_lowercase();
    UNITS.iter().find(|u| u.name == t)
}

/// The result of grading a free-response numeric answer.
#[derive(Debug, Clone, PartialEq)]
pub enum NumericGrade {
    Correct,
    Wrong,
    /// Could not read the number or its unit (shown to the user, never crashed).
    Unparseable(&'static str),
}

/// Parse a value + optional trailing unit token from free text.
/// Accepts plain decimals, scientific notation (`6.02e23`, `6.02×10^23`,
/// `6.02 x 10^23`), leading sign. Returns (magnitude, Option<unit token>).
fn parse_value_unit(given: &str) -> Option<(f64, Option<String>)> {
    let s = given.trim();
    if s.is_empty() {
        return None;
    }
    // Normalize the various "×10^" / "x 10^" spellings into `e`.
    let lowered = s.replace('×', "x").replace('X', "x").replace('*', "x");
    let norm = lowered
        .replace(" x 10^", "e")
        .replace("x 10^", "e")
        .replace(" x10^", "e")
        .replace("x10^", "e");
    let norm = norm.trim();
    // Split off a leading numeric token.
    let mut num_end = 0;
    let bytes: Vec<char> = norm.chars().collect();
    let mut seen_digit = false;
    let mut i = 0;
    // optional sign
    if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
        i += 1;
    }
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() {
            seen_digit = true;
            i += 1;
        } else if c == '.' {
            i += 1;
        } else if (c == 'e' || c == 'E') && seen_digit {
            i += 1;
            if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
                i += 1;
            }
        } else {
            break;
        }
        num_end = i;
    }
    if !seen_digit {
        return None;
    }
    let number: String = bytes[..num_end].iter().collect();
    let value: f64 = number.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let rest: String = bytes[num_end..].iter().collect::<String>().trim().to_string();
    let unit = if rest.is_empty() { None } else { Some(rest) };
    Some((value, unit))
}

/// Grade a free-response numeric answer against a numeric item (§9).
pub fn check_numeric(given: &str, answer: f64, item_unit: Option<&str>, tol: &Tolerance) -> NumericGrade {
    let Some((raw_value, given_unit)) = parse_value_unit(given) else {
        return NumericGrade::Unparseable("couldn't read that number");
    };
    // Resolve the compared magnitude.
    let compared: f64 = match (given_unit.as_deref(), item_unit) {
        (Some(gu), Some(iu)) => {
            let (Some(gun), Some(iun)) = (lookup_unit(gu), lookup_unit(iu)) else {
                return NumericGrade::Unparseable("couldn't read that unit");
            };
            if gun.dim != iun.dim {
                return NumericGrade::Wrong; // dimensionally incompatible
            }
            // Convert given → base → item unit.
            let base = raw_value * gun.factor + gun.offset;
            (base - iun.offset) / iun.factor
        }
        (Some(gu), None) => {
            // A unit was given but the item declares none: only accept if known.
            if lookup_unit(gu).is_none() {
                return NumericGrade::Unparseable("couldn't read that unit");
            }
            raw_value
        }
        (None, _) => raw_value, // absent unit compares in the declared unit
    };
    let ok = match tol.kind.as_str() {
        // A small RELATIVE epsilon (plus a tiny absolute floor for answers near
        // zero) so a correctly-rounded value entered at normal precision is
        // accepted; a 4-ULP machine epsilon rejected legitimate answers for
        // imported bundles whose `exact` key came from a division.
        "exact" => (compared - answer).abs() <= 1e-9 * answer.abs().max(1e-9),
        "abs" => (compared - answer).abs() <= tol.value,
        "rel" => (compared - answer).abs() <= tol.value * answer.abs(),
        _ => return NumericGrade::Unparseable("unknown tolerance"),
    };
    if ok {
        NumericGrade::Correct
    } else {
        NumericGrade::Wrong
    }
}

/// Grade a balance item: the learner's coefficients must be the lowest-integer
/// balance, order-independent (the species order is fixed by the equation).
pub fn check_balance(eq: &Equation, given: &[u32]) -> bool {
    if given.iter().any(|&c| c == 0) || given.len() != eq.reactants.len() + eq.products.len() {
        return false;
    }
    match explain::balance_equation(eq) {
        Ok(computed) => {
            let g = given.iter().map(|&c| c as i64).fold(0i64, |a, b| gcd(a, b));
            if g == 0 {
                return false;
            }
            let norm: Vec<u32> = given.iter().map(|&c| (c as i64 / g) as u32).collect();
            norm == computed
        }
        Err(_) => false,
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// MC grading via the stored answerIndex.
pub fn check_mc(answer_index: u32, chosen: u32) -> bool {
    answer_index == chosen
}

/// A learner's answer to one item, used by trainer/checkpoint/exam grading.
#[derive(Debug, Clone)]
pub enum Given {
    Mc(u32),
    Numeric(String),
    Balance(Vec<u32>),
    Multistep(Vec<MultistepGiven>),
}

#[derive(Debug, Clone)]
pub enum MultistepGiven {
    Mc(u32),
    Numeric(String),
    Balance(Vec<u32>),
}

/// The graded outcome of one item: points earned / possible (0..=1 for simple
/// kinds; step count for multistep).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemGrade {
    pub earned: f64,
    pub possible: f64,
}

impl ItemGrade {
    pub fn fraction(&self) -> f64 {
        if self.possible <= 0.0 {
            0.0
        } else {
            (self.earned / self.possible).clamp(0.0, 1.0)
        }
    }
    pub fn is_full(&self) -> bool {
        (self.earned - self.possible).abs() < 1e-9
    }
}

/// Grade a single stored item against the learner's answer.
pub fn grade_item(item: &QuizItem, given: &Given) -> ItemGrade {
    match (item.kind.as_str(), given) {
        ("mc", Given::Mc(c)) => {
            let correct = item.answer_index.map(|a| check_mc(a, *c)).unwrap_or(false);
            ItemGrade { earned: correct as u8 as f64, possible: 1.0 }
        }
        ("numeric", Given::Numeric(s)) => {
            let ok = match (item.answer, item.tolerance.as_ref()) {
                (Some(a), Some(tol)) => {
                    matches!(check_numeric(s, a, item.unit.as_deref(), tol), NumericGrade::Correct)
                }
                _ => false,
            };
            ItemGrade { earned: ok as u8 as f64, possible: 1.0 }
        }
        ("balance", Given::Balance(c)) => {
            let ok = item.equation.as_ref().map(|eq| check_balance(eq, c)).unwrap_or(false);
            ItemGrade { earned: ok as u8 as f64, possible: 1.0 }
        }
        ("multistep", Given::Multistep(answers)) => grade_multistep(item, answers),
        _ => ItemGrade { earned: 0.0, possible: 1.0 },
    }
}

fn grade_step(step: &MultistepStep, given: &MultistepGiven) -> bool {
    match (step.kind.as_str(), given) {
        ("mc", MultistepGiven::Mc(c)) => step.answer_index.map(|a| a == *c).unwrap_or(false),
        ("numeric", MultistepGiven::Numeric(s)) => match (step.answer, step.tolerance.as_ref()) {
            (Some(a), Some(tol)) => matches!(check_numeric(s, a, step.unit.as_deref(), tol), NumericGrade::Correct),
            _ => false,
        },
        ("balance", MultistepGiven::Balance(c)) => {
            step.equation.as_ref().map(|eq| check_balance(eq, c)).unwrap_or(false)
        }
        _ => false,
    }
}

fn grade_multistep(item: &QuizItem, answers: &[MultistepGiven]) -> ItemGrade {
    let steps = match item.steps.as_ref() {
        Some(s) if !s.is_empty() => s,
        _ => return ItemGrade { earned: 0.0, possible: 1.0 },
    };
    let gated = item.gated.unwrap_or(false);
    let possible = steps.len() as f64;
    let mut earned = 0.0;
    let mut prev_ok = true;
    for (i, step) in steps.iter().enumerate() {
        let given = answers.get(i);
        let ok = match given {
            Some(g) => grade_step(step, g),
            None => false,
        };
        if gated && !prev_ok {
            // A wrong early step zeroes the dependent steps.
            prev_ok = false;
            continue;
        }
        if ok {
            earned += 1.0;
        }
        prev_ok = ok;
    }
    ItemGrade { earned, possible }
}
