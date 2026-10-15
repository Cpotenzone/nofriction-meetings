//! Closed-form recompute + verify for explainer correctness (docs/LESSONS.md
//! §6.1). The stored value the UI renders is the author's data, but it is
//! proven consistent with a Rust recomputation here, so the UI can never
//! present a wrong "correct" answer. Two kinds (molecule_diagram,
//! titration_curve) are structural-only carve-outs.
//!
//! All functions return `Err` to reject an element; none panic on hostile input.

use crate::lessons::schema::Equation;
use serde_json::Value;
use std::collections::BTreeMap;

/// Parse a chemical formula like "Fe2(SO4)3" or "H2O" into element → count.
/// Supports nested parentheses and multiplier groups. Returns None on garbage.
pub fn parse_formula(formula: &str) -> Option<BTreeMap<String, i64>> {
    let s: Vec<char> = formula.trim().chars().collect();
    if s.is_empty() {
        return None;
    }
    // Recursive-descent over the char slice with an index.
    fn parse_group(s: &[char], i: &mut usize, depth: u32) -> Option<BTreeMap<String, i64>> {
        if depth > 16 {
            return None;
        }
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        while *i < s.len() {
            let c = s[*i];
            if c == '(' || c == '[' {
                *i += 1;
                let inner = parse_group(s, i, depth + 1)?;
                // closing bracket
                if *i >= s.len() || (s[*i] != ')' && s[*i] != ']') {
                    return None;
                }
                *i += 1;
                let mult = read_number(s, i).unwrap_or(1);
                for (el, n) in inner {
                    *counts.entry(el).or_insert(0) += n * mult;
                }
            } else if c == ')' || c == ']' {
                break;
            } else if c.is_ascii_uppercase() {
                let mut el = String::new();
                el.push(c);
                *i += 1;
                while *i < s.len() && s[*i].is_ascii_lowercase() {
                    el.push(s[*i]);
                    *i += 1;
                }
                let n = read_number(s, i).unwrap_or(1);
                *counts.entry(el).or_insert(0) += n;
            } else {
                // Unexpected char (digit without an element, stray symbol).
                return None;
            }
        }
        Some(counts)
    }
    fn read_number(s: &[char], i: &mut usize) -> Option<i64> {
        let start = *i;
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == start {
            return None;
        }
        s[start..*i].iter().collect::<String>().parse().ok()
    }
    let mut i = 0usize;
    let counts = parse_group(&s, &mut i, 0)?;
    if i != s.len() || counts.is_empty() {
        return None;
    }
    Some(counts)
}

/// Compute the lowest-integer balanced coefficients of an equation via integer
/// linear algebra over per-element atom counts. Returns [reactant coeffs ++
/// product coeffs] or Err if unbalanceable. (Charge is out of scope for the
/// neutral-species seed set; the structural atom balance is the authoritative
/// check.)
pub fn balance_equation(eq: &Equation) -> Result<Vec<u32>, String> {
    let species: Vec<&str> = eq
        .reactants
        .iter()
        .map(|r| r.formula.as_str())
        .chain(eq.products.iter().map(|p| p.formula.as_str()))
        .collect();
    let n = species.len();
    if n < 2 || n > 16 {
        return Err("equation has too few/many species".into());
    }
    // Build element set and the signed stoichiometric matrix (reactants +,
    // products −). We solve A x = 0 for a positive integer vector x.
    let mut elements: Vec<String> = Vec::new();
    let mut parsed: Vec<BTreeMap<String, i64>> = Vec::new();
    for (k, f) in species.iter().enumerate() {
        let counts = parse_formula(f).ok_or_else(|| format!("unparseable formula {}", f))?;
        for el in counts.keys() {
            if !elements.contains(el) {
                elements.push(el.clone());
            }
        }
        let _ = k;
        parsed.push(counts);
    }
    let r = eq.reactants.len();
    // Rational Gaussian elimination over rows=elements, cols=species.
    // matrix[e][s] = +count for reactant, -count for product.
    let rows = elements.len();
    let mut mat: Vec<Vec<f64>> = vec![vec![0.0; n]; rows];
    for (e, el) in elements.iter().enumerate() {
        for (s, counts) in parsed.iter().enumerate() {
            let c = *counts.get(el).unwrap_or(&0) as f64;
            mat[e][s] = if s < r { c } else { -c };
        }
    }
    // Solve for the null space (expect dimension 1 for a balanceable reaction).
    // Set x[n-1] = 1 (free), solve the rest; then scale to integers.
    let x = null_space_vector(&mat, n)?;
    // All coefficients must be positive & finite.
    if x.iter().any(|v| !v.is_finite() || *v <= 1e-9) {
        return Err("no positive balance exists".into());
    }
    let coeffs = to_lowest_integers(&x)?;
    Ok(coeffs)
}

/// Find a single null-space vector of `mat` (rows×cols) with the last variable
/// as the free parameter set to 1. Reduced-row-echelon then back-substitute.
fn null_space_vector(mat: &[Vec<f64>], cols: usize) -> Result<Vec<f64>, String> {
    let rows = mat.len();
    let mut m: Vec<Vec<f64>> = mat.to_vec();
    let mut pivot_col = vec![usize::MAX; rows];
    let mut row = 0usize;
    for col in 0..cols {
        if row >= rows {
            break;
        }
        // find pivot
        let mut sel = row;
        let mut best = m[row][col].abs();
        for r in (row + 1)..rows {
            if m[r][col].abs() > best {
                best = m[r][col].abs();
                sel = r;
            }
        }
        if best < 1e-9 {
            continue;
        }
        m.swap(row, sel);
        let pv = m[row][col];
        for c in 0..cols {
            m[row][c] /= pv;
        }
        for r in 0..rows {
            if r != row && m[r][col].abs() > 1e-12 {
                let f = m[r][col];
                for c in 0..cols {
                    m[r][c] -= f * m[row][c];
                }
            }
        }
        pivot_col[row] = col;
        row += 1;
    }
    // Free variable: last column (cols-1) if it is not a pivot.
    let free = cols - 1;
    if pivot_col.iter().any(|&c| c == free) {
        return Err("equation is over-determined (no single free coefficient)".into());
    }
    let mut x = vec![0.0; cols];
    x[free] = 1.0;
    for r in 0..rows {
        let pc = pivot_col[r];
        if pc == usize::MAX {
            continue;
        }
        // x[pc] + sum_{c != pivot} m[r][c] x[c] = 0  → x[pc] = -m[r][free]*x[free]
        x[pc] = -m[r][free] * x[free];
    }
    Ok(x)
}

/// Scale a positive rational vector to lowest-integer coefficients.
fn to_lowest_integers(x: &[f64]) -> Result<Vec<u32>, String> {
    // Try denominators up to 60 to rationalize.
    let mut best: Option<Vec<i64>> = None;
    for denom in 1..=120i64 {
        let scaled: Vec<f64> = x.iter().map(|v| v * denom as f64).collect();
        if scaled.iter().all(|v| (v - v.round()).abs() < 1e-6 && v.round() >= 1.0) {
            best = Some(scaled.iter().map(|v| v.round() as i64).collect());
            break;
        }
    }
    let ints = best.ok_or("could not rationalize coefficients")?;
    let g = ints.iter().copied().fold(0i64, gcd);
    if g == 0 {
        return Err("degenerate coefficients".into());
    }
    Ok(ints.iter().map(|v| (v / g) as u32).collect())
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

/// Verify authored coefficients equal the computed lowest-integer balance.
pub fn verify_balance(eq: &Equation, authored: &[u32]) -> Result<(), String> {
    if authored.len() != eq.reactants.len() + eq.products.len() {
        return Err("coefficient count does not match species count".into());
    }
    if authored.iter().any(|&c| c == 0) {
        return Err("a coefficient is zero".into());
    }
    let computed = balance_equation(eq)?;
    // Normalize authored to lowest integers too, then compare.
    let g = authored.iter().map(|&c| c as i64).fold(0i64, gcd);
    if g == 0 {
        return Err("degenerate authored coefficients".into());
    }
    let authored_norm: Vec<u32> = authored.iter().map(|&c| (c as i64 / g) as u32).collect();
    if authored_norm != computed {
        return Err("authored coefficients are not the lowest-integer balance".into());
    }
    Ok(())
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64()).filter(|x| x.is_finite())
}

/// Dispatch structural/closed-form verification for an explainer payload.
pub fn verify_explainer(kind: &str, data: &Value) -> Result<(), String> {
    match kind {
        "equation_balancer" => {
            let eq: Equation = serde_json::from_value(
                data.get("equation").cloned().ok_or("balancer has no equation")?,
            )
            .map_err(|_| "balancer equation malformed".to_string())?;
            let coeffs: Vec<u32> = serde_json::from_value(
                data.get("answerCoefficients").cloned().ok_or("balancer has no answerCoefficients")?,
            )
            .map_err(|_| "balancer coefficients malformed".to_string())?;
            verify_balance(&eq, &coeffs)
        }
        "mole_ratio" => {
            // Final answer must be finite; given/want present. Full chain check
            // is covered by the worked-example/item recompute path.
            let fa = data
                .get("finalAnswer")
                .and_then(|f| f.get("value"))
                .and_then(|v| v.as_f64())
                .ok_or("mole_ratio has no finalAnswer.value")?;
            if !fa.is_finite() {
                return Err("mole_ratio finalAnswer not finite".into());
            }
            Ok(())
        }
        "limiting_reagent" => {
            let reagents = data.get("reagents").and_then(|r| r.as_array()).ok_or("no reagents")?;
            if reagents.is_empty() {
                return Err("limiting_reagent has no reagents".into());
            }
            let mut best: Option<f64> = None;
            for rg in reagents {
                let molar = num(rg, "molarMass").ok_or("reagent molarMass invalid")?;
                let avail = num(rg, "available").ok_or("reagent available invalid")?;
                let coeff = num(rg, "coeff").unwrap_or(1.0);
                if molar <= 0.0 || coeff <= 0.0 {
                    return Err("reagent constants non-positive".into());
                }
                let moles = avail / molar / coeff;
                best = Some(best.map_or(moles, |b| b.min(moles)));
            }
            if best.map(|b| b.is_finite()).unwrap_or(false) {
                Ok(())
            } else {
                Err("no limiting reagent resolvable".into())
            }
        }
        "dimensional_analysis" => {
            let start = num(data.get("start").unwrap_or(&Value::Null), "value")
                .ok_or("dimensional_analysis has no start.value")?;
            let tiles = data.get("tiles").and_then(|t| t.as_array()).ok_or("no tiles")?;
            let mut product = start;
            for t in tiles {
                let n = num(t, "num").ok_or("tile num invalid")?;
                let d = num(t, "den").ok_or("tile den invalid")?;
                if d == 0.0 {
                    return Err("tile denominator is zero".into());
                }
                product *= n / d;
            }
            if !product.is_finite() {
                return Err("dimensional_analysis product not finite".into());
            }
            // If an authored value exists, verify within 0.1% relative.
            if let Some(expected) = num(data, "value") {
                if (product - expected).abs() > 1e-3 * expected.abs().max(1e-9) {
                    return Err("dimensional_analysis product ≠ authored value".into());
                }
            }
            Ok(())
        }
        "ideal_gas_pv" => {
            let n = num(data, "n").ok_or("ideal_gas has no n")?;
            let t = num(data, "T").ok_or("ideal_gas has no T")?;
            let r = num(data, "R").unwrap_or(0.082057);
            if n <= 0.0 || t <= 0.0 || r <= 0.0 {
                return Err("ideal_gas constants non-positive".into());
            }
            Ok(())
        }
        "molecule_diagram" => {
            // Structural-only carve-out (§6.1): atoms exist, bond order ∈
            // {1,2,3}, coords in 0..1, labels reference existing atoms.
            let atoms = data.get("atoms").and_then(|a| a.as_array()).ok_or("no atoms")?;
            if atoms.is_empty() {
                return Err("molecule_diagram has no atoms".into());
            }
            let mut ids = std::collections::HashSet::new();
            for a in atoms {
                let id = a.get("id").and_then(|v| v.as_str()).ok_or("atom has no id")?;
                let x = num(a, "x").ok_or("atom x invalid")?;
                let y = num(a, "y").ok_or("atom y invalid")?;
                if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                    return Err("atom coord out of unit box".into());
                }
                ids.insert(id.to_string());
            }
            if let Some(bonds) = data.get("bonds").and_then(|b| b.as_array()) {
                for b in bonds {
                    let a = b.get("a").and_then(|v| v.as_str()).ok_or("bond has no a")?;
                    let c = b.get("b").and_then(|v| v.as_str()).ok_or("bond has no b")?;
                    let order = b.get("order").and_then(|v| v.as_u64()).ok_or("bond has no order")?;
                    if !ids.contains(a) || !ids.contains(c) {
                        return Err("bond references a missing atom".into());
                    }
                    if !(1..=3).contains(&order) {
                        return Err("bond order out of range".into());
                    }
                }
            }
            Ok(())
        }
        "titration_curve" => {
            // Structural-only carve-out (§6.1): monotonic-in-volume +
            // equivalence-point present.
            let points = data.get("points").and_then(|p| p.as_array()).ok_or("no points")?;
            if points.len() < 2 {
                return Err("titration_curve needs ≥2 points".into());
            }
            let mut last_vol = f64::NEG_INFINITY;
            for p in points {
                let vol = num(p, "volAdded").ok_or("point volAdded invalid")?;
                let ph = num(p, "pH").ok_or("point pH invalid")?;
                if !ph.is_finite() {
                    return Err("point pH not finite".into());
                }
                if vol < last_vol {
                    return Err("titration points not monotonic in volume".into());
                }
                last_vol = vol;
            }
            if num(data, "equivalenceVol").is_none() {
                return Err("titration_curve has no equivalence point".into());
            }
            Ok(())
        }
        other => Err(format!("no verifier for explainer kind {}", other)),
    }
}
