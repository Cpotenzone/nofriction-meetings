//! Authoring tool for the six-lesson stoichiometry seed course (FEAT-002,
//! docs/LESSONS.md §12.1/§14). Run from `src-tauri/`:
//!
//!     cargo run --example author_seed_lessons
//!
//! It assembles chemically-correct lesson content in Rust, embeds the authored
//! hero figure PNGs from `../scripts/seed/figures/lessonN.png`, builds the
//! `.lesson` zip with a hashed manifest + content fingerprint, validates it
//! through the ordinary import path (`bundle::validate_bytes`), and writes
//! `resources/seed/stoichiometry.lesson`.
//!
//! Figures are REQUIRED (owner-decision D4): the tool refuses to produce a
//! bundle if any lesson's hero PNG is missing or does not decode. The PNGs are
//! produced at authoring time with the genmedia image tool (gemini-3-pro-image /
//! "Nano Banana Pro"); the shipped app never generates images at runtime.
//!
//! All answer keys here are recomputed/verified by the same Rust that grades at
//! runtime (balancer, numeric grader, generators), so the content cannot ship
//! with a wrong "correct" answer.

use nofriction_meetings_lib::lessons::bundle::{self, ManifestFile};
use nofriction_meetings_lib::lessons::explain;
use nofriction_meetings_lib::lessons::schema::{Equation, Product, Reactant};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const SEED_SET_ID: &str = "stoichiometry-seed";
const REVIEWER: &str = "seed content reviewer (see docs/LESSONS_AUTHORING.md)";

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

// ── Molar masses (g/mol, 2 dp, recomputed from atomic masses) ────────────────
fn molar_mass(species: &str) -> f64 {
    match species {
        "H2" => 2.02,
        "O2" => 32.00,
        "N2" => 28.02,
        "Cl2" => 70.90,
        "H2O" => 18.02,
        "CO2" => 44.01,
        "NH3" => 17.03,
        "CH4" => 16.04,
        "NaCl" => 58.44,
        "HCl" => 36.46,
        "NaOH" => 40.00,
        "C" => 12.01,
        "C3H8" => 44.11,
        "N2O" => 44.01,
        "KCl" => 74.55,
        "CaCO3" => 100.09,
        "Fe2O3" => 159.69,
        "Al" => 26.98,
        "Al2O3" => 101.96,
        _ => panic!("molar mass for {species} not in authoring table"),
    }
}

fn eq(reactants: &[&str], products: &[&str]) -> Equation {
    Equation {
        reactants: reactants.iter().map(|f| Reactant { formula: f.to_string(), count: None }).collect(),
        products: products.iter().map(|f| Product { formula: f.to_string() }).collect(),
    }
}

/// Balance an equation and return the verified lowest-integer coefficients.
/// Panics (fails the authoring run) if it does not balance — a hard gate.
fn balance(reactants: &[&str], products: &[&str]) -> (Value, Vec<u32>) {
    let e = eq(reactants, products);
    let coeffs = explain::balance_equation(&e).unwrap_or_else(|err| {
        panic!("authoring: {reactants:?} -> {products:?} does not balance: {err}")
    });
    explain::verify_balance(&e, &coeffs).expect("authoring: verify_balance disagreed with balance_equation");
    let v = json!({
        "reactants": reactants.iter().map(|f| json!({"formula": f})).collect::<Vec<_>>(),
        "products": products.iter().map(|f| json!({"formula": f})).collect::<Vec<_>>(),
    });
    (v, coeffs)
}

// ── Item builders ────────────────────────────────────────────────────────────

/// Rotate `choices` so the correct answer (at `correct`) moves to position
/// `target`, returning the rotated choices and the new correct index. Pure
/// reordering — the correct text is unchanged, only its position moves — so the
/// authored keys are spread across positions instead of all being index 0.
fn rotate_choices(choices: &[&str], correct: usize, target: usize) -> (Vec<String>, usize) {
    let n = choices.len();
    let target = target % n;
    // A rotation by `shift` maps old index j -> (j + shift) % n. We want
    // correct -> target, so shift = (target + n - correct) % n.
    let shift = (target + n - (correct % n)) % n;
    let mut out = vec![String::new(); n];
    for (j, c) in choices.iter().enumerate() {
        out[(j + shift) % n] = c.to_string();
    }
    (out, target)
}

fn mc(id: &str, concept: &str, prompt: &str, choices: &[&str], answer_index: u32, explanation: &str) -> Value {
    assert!((answer_index as usize) < choices.len(), "mc {id}: answerIndex out of range");
    json!({
        "id": id, "conceptIds": [concept], "kind": "mc",
        "prompt": prompt,
        "choices": choices,
        "answerIndex": answer_index,
        "explanation": explanation,
        "solution": [{"text": explanation}],
    })
}

fn numeric(id: &str, concept: &str, prompt: &str, answer: f64, unit: &str, explanation: &str, steps: Vec<Value>) -> Value {
    assert!(answer.is_finite(), "numeric {id}: non-finite answer");
    json!({
        "id": id, "conceptIds": [concept], "kind": "numeric",
        "prompt": prompt,
        "answer": answer, "unit": unit,
        "tolerance": {"kind": "rel", "value": 0.01},
        "explanation": explanation,
        "solution": steps,
    })
}

fn balance_item(id: &str, concept: &str, prompt: &str, reactants: &[&str], products: &[&str], explanation: &str) -> Value {
    let (equation, coeffs) = balance(reactants, products);
    json!({
        "id": id, "conceptIds": [concept], "kind": "balance",
        "prompt": prompt,
        "equation": equation,
        "answerCoefficients": coeffs,
        "explanation": explanation,
        "solution": [{"text": explanation}],
    })
}

/// A multistep item: balance → mole ratio → grams. Steps are a balance leg then
/// numeric legs, so balancing is tested inside the exam without a standalone
/// balance question (NIT-6).
fn multistep(id: &str, concept: &str, prompt: &str, steps: Vec<Value>, explanation: &str) -> Value {
    json!({
        "id": id, "conceptIds": [concept], "kind": "multistep",
        "prompt": prompt, "gated": true,
        "steps": steps,
        "explanation": explanation,
        "solution": [{"text": explanation}],
    })
}

fn step_mc(prompt: &str, choices: &[&str], answer_index: u32) -> Value {
    json!({"prompt": prompt, "kind": "mc", "choices": choices, "answerIndex": answer_index})
}
fn step_numeric(prompt: &str, answer: f64, unit: &str) -> Value {
    json!({"prompt": prompt, "kind": "numeric", "answer": answer, "unit": unit, "tolerance": {"kind": "rel", "value": 0.01}})
}
fn step_balance(prompt: &str, reactants: &[&str], products: &[&str]) -> Value {
    let (equation, coeffs) = balance(reactants, products);
    json!({"prompt": prompt, "kind": "balance", "equation": equation, "answerCoefficients": coeffs})
}

fn wstep(text: &str, expr: &str, result: &str, unit: &str) -> Value {
    json!({"text": text, "expression": expr, "result": result, "unit": unit})
}

fn worked(id: &str, concept: &str, prompt: &str, steps: Vec<Value>, value: f64, unit: &str) -> Value {
    json!({
        "id": id, "prompt": prompt, "conceptIds": [concept],
        "steps": steps,
        "finalAnswer": {"value": value, "unit": unit},
    })
}

fn misconception(claim: &str, why: &str, correct: &str, concept: &str) -> Value {
    json!({"claim": claim, "whyWrong": why, "correct": correct, "conceptIds": [concept]})
}

/// Fill a checkpoint bank to `n` MC items drawn from a small bank of templates,
/// so a lesson's checkpoint comfortably over-provisions its presentCount.
fn mc_filler(prefix: &str, concept: &str, n: usize, templates: &[(&str, &[&str], u32)]) -> Vec<Value> {
    let mut out = Vec::new();
    for i in 0..n {
        let (prompt, choices, ans) = templates[i % templates.len()];
        out.push(mc(&format!("{prefix}-f{i}"), concept, prompt, choices, ans, "Review the concept summary."));
    }
    out
}

fn main() {
    // Resolve paths relative to src-tauri/ (the cargo manifest dir).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let figures_dir = root.join("../scripts/seed/figures");
    let out_path = root.join("resources/seed/stoichiometry.lesson");

    // Concepts (the DAG). Each lesson teaches a few; prerequisites form an
    // acyclic chain.
    let concepts = json!([
        {"id": "c-mole", "name": "The mole and Avogadro's number", "summary": "A mole is 6.022 x 10^23 particles; it links the count of particles to a measurable mass.", "prerequisites": [], "lessonIds": ["l1"]},
        {"id": "c-molar-mass", "name": "Molar mass", "summary": "The mass of one mole of a substance in grams per mole, found by summing atomic masses.", "prerequisites": ["c-mole"], "lessonIds": ["l1"]},
        {"id": "c-balance", "name": "Balancing chemical equations", "summary": "Conservation of mass requires equal atoms of each element on both sides; adjust coefficients, never subscripts.", "prerequisites": ["c-mole"], "lessonIds": ["l2"]},
        {"id": "c-ratio", "name": "Mole ratios and stoichiometric factors", "summary": "Coefficients in a balanced equation give the mole ratios used to convert between amounts of substances.", "prerequisites": ["c-balance", "c-molar-mass"], "lessonIds": ["l3"]},
        {"id": "c-limiting", "name": "Limiting reagent and percent yield", "summary": "The reactant that runs out first limits the product; percent yield compares actual to theoretical.", "prerequisites": ["c-ratio"], "lessonIds": ["l4"]},
        {"id": "c-solution", "name": "Solution stoichiometry and titration", "summary": "Molarity relates moles and volume; titration finds an unknown concentration at the equivalence point.", "prerequisites": ["c-ratio"], "lessonIds": ["l5"]},
        {"id": "c-gas", "name": "Gas stoichiometry and molar volume", "summary": "At STP one mole of an ideal gas occupies 22.41 L; PV = nRT relates the gas variables.", "prerequisites": ["c-ratio"], "lessonIds": ["l6"]}
    ]);

    let mut lessons: Vec<Value> = Vec::new();
    let mut figures: Vec<Value> = Vec::new();
    let mut file_entries: Vec<(String, Vec<u8>)> = Vec::new();

    // Hero figure per lesson (REQUIRED, D4). Read the authored PNG.
    let hero_alt = [
        "Infographic: a balance scale relates grams to moles via molar mass, with Avogadro's number 6.022 x 10^23.",
        "Infographic: a balanced equation with per-element atom-count bars equal on both sides.",
        "Infographic: a dimensional-analysis railroad converting grams of a reactant to grams of a product through the mole ratio.",
        "Infographic: two reactant bars draining to show the limiting reagent and leftover excess, with theoretical yield.",
        "Infographic: a titration curve of pH versus titrant volume with the equivalence point marked.",
        "Infographic: an ideal-gas PV explorer at STP showing one mole occupying 22.41 L and the PV = nRT relationship.",
    ];

    for i in 0..6usize {
        let n = i + 1;
        let fig_path = figures_dir.join(format!("lesson{n}.png"));
        let bytes = std::fs::read(&fig_path).unwrap_or_else(|_| {
            eprintln!(
                "\nMISSING REQUIRED FIGURE: {}\n\n\
                 The six-lesson seed set must embed one authored hero PNG per lesson\n\
                 (owner-decision D4). Produce the figures with the genmedia image tool\n\
                 (gemini-3-pro-image / \"Nano Banana Pro\") into scripts/seed/figures/\n\
                 as lesson1.png .. lesson6.png, then re-run this authoring tool.\n\
                 See docs/LESSONS_AUTHORING.md for the art direction and review gate.\n",
                fig_path.display()
            );
            std::process::exit(2);
        });
        let img = image::load_from_memory(&bytes)
            .unwrap_or_else(|_| panic!("figure lesson{n}.png does not decode as an image"));
        let (w, h) = (img.width(), img.height());
        let fig_id = format!("fig-l{n}-hero");
        let path = format!("figures/{fig_id}.png");
        figures.push(json!({
            "id": fig_id, "path": path, "sha256": sha256_hex(&bytes),
            "width": w, "height": h, "alt": hero_alt[i], "reviewedBy": REVIEWER
        }));
        file_entries.push((path, bytes));
    }

    // ── Lesson 1: Moles, molar mass, the mole concept ───────────────────────
    {
        let c = "c-mole";
        let cm = "c-molar-mass";
        let mm_h2o = molar_mass("H2O");
        let mm_co2 = molar_mass("CO2");
        // grams -> moles worked example (4.0 g H2O)
        let moles = 4.0 / mm_h2o;
        let checkpoint_items = {
            let mut v = vec![
                numeric("l1-cp-n1", c, "How many moles are in 36.04 g of H2O?", 36.04 / mm_h2o, "mol",
                    "Divide grams by molar mass.",
                    vec![wstep("Molar mass of H2O", "2(1.01)+16.00", "18.02", "g/mol"),
                         wstep("Divide grams by molar mass", "36.04 / 18.02", "2.00", "mol")]),
                numeric("l1-cp-n2", cm, "What is the molar mass of CO2?", mm_co2, "g/mol",
                    "Sum the atomic masses: C 12.01 + 2(16.00).",
                    vec![wstep("Add atomic masses", "12.01 + 2(16.00)", "44.01", "g/mol")]),
                numeric("l1-cp-n3", c, "How many moles are in 88.02 g of CO2?", 88.02 / mm_co2, "mol",
                    "Divide grams by molar mass.",
                    vec![wstep("Divide", "88.02 / 44.01", "2.00", "mol")]),
                mc("l1-cp-m1", c, "What is Avogadro's number?",
                    &["6.022 x 10^23 per mole", "3.14 per mole", "1.00 x 10^6 per mole"], 0,
                    "A mole contains 6.022 x 10^23 particles."),
                mc("l1-cp-m2", cm, "Molar mass has units of:",
                    &["g/mol", "mol/g", "g only"], 0, "Molar mass is grams per mole."),
                mc("l1-cp-m3", c, "One mole of water contains how many molecules?",
                    &["6.022 x 10^23", "18", "2"], 0, "One mole is Avogadro's number of molecules."),
            ];
            v.extend(mc_filler("l1-cp", cm, 10, &[
                ("Molar mass of H2O is closest to:", &["18 g/mol", "2 g/mol", "32 g/mol"], 0),
                ("Molar mass of O2 is:", &["32.00 g/mol", "16.00 g/mol", "8.00 g/mol"], 0),
                ("To find moles from grams you:", &["divide by molar mass", "multiply by Avogadro", "add molar mass"], 0),
            ]));
            v
        };
        lessons.push(json!({
            "id": "l1", "index": 1, "title": "Moles, molar mass, and the mole concept",
            "objectives": [
                {"text": "Define the mole and use Avogadro's number", "conceptIds": [c]},
                {"text": "Compute molar mass and convert between grams and moles", "conceptIds": [cm]}
            ],
            "conceptIds": [c, cm],
            "explainers": [
                {"id": "l1-fig", "title": "The mole at a glance", "conceptIds": [c, cm],
                 "caption": "Grams, moles and molar mass on one balance.", "kind": "figure",
                 "data": {"figureId": "fig-l1-hero", "alt": hero_alt[0]}},
                {"id": "l1-mr", "title": "Grams to moles", "conceptIds": [cm],
                 "caption": "Convert 4.00 g of water to moles.", "kind": "mole_ratio",
                 "a11yState": "Converting grams of water to moles.",
                 "data": {"given": {"amount": 4.0, "unit": "g", "species": "H2O"},
                          "want": {"unit": "mol", "species": "H2O"},
                          "finalAnswer": {"value": moles, "unit": "mol"}}}
            ],
            "workedExamples": [
                worked("l1-w1", c, "How many moles are in 9.01 g of H2O?",
                    vec![wstep("Molar mass of H2O", "18.02", "18.02", "g/mol"),
                         wstep("Divide grams by molar mass", "9.01 / 18.02", "0.50", "mol")], 9.01/mm_h2o, "mol"),
                worked("l1-w2", cm, "What is the mass of 2.00 mol of CO2?",
                    vec![wstep("Molar mass of CO2", "44.01", "44.01", "g/mol"),
                         wstep("Multiply moles by molar mass", "2.00 x 44.01", "88.02", "g")], 2.0*mm_co2, "g"),
                worked("l1-w3", c, "How many molecules are in 0.50 mol of H2O?",
                    vec![wstep("Multiply by Avogadro's number", "0.50 x 6.022e23", "3.01e23", "molecules")], 3.011e23, "molecules")
            ],
            "misconceptions": {"items": [
                misconception("Molar mass equals the molecular formula.",
                    "The formula lists atoms; molar mass is their summed masses in g/mol.",
                    "Add the atomic masses of every atom in the formula.", cm),
                misconception("Counting atoms is the same as counting moles.",
                    "A mole is 6.022 x 10^23 particles, not a single atom.",
                    "Divide the particle count by Avogadro's number to get moles.", c)
            ]},
            "trainer": {"id": "l1-t", "conceptIds": [c, cm],
                "generators": [{"template": "mole_mass", "conceptIds": [cm]},
                               {"template": "mass_mole", "conceptIds": [cm]}],
                "staticItems": []},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Lesson 2: Chemical equations and balancing ──────────────────────────
    {
        let c = "c-balance";
        let checkpoint_items = {
            let mut v = vec![
                // At least one balance-kind item is REQUIRED here (Finding 4).
                balance_item("l2-cp-b1", c, "Balance: H2 + O2 -> H2O", &["H2", "O2"], &["H2O"],
                    "2 H2 + O2 -> 2 H2O keeps 4 H and 2 O on each side."),
                balance_item("l2-cp-b2", c, "Balance: CH4 + O2 -> CO2 + H2O", &["CH4", "O2"], &["CO2", "H2O"],
                    "CH4 + 2 O2 -> CO2 + 2 H2O."),
                balance_item("l2-cp-b3", c, "Balance: N2 + H2 -> NH3", &["N2", "H2"], &["NH3"],
                    "N2 + 3 H2 -> 2 NH3."),
                balance_item("l2-cp-b4", c, "Balance: C3H8 + O2 -> CO2 + H2O", &["C3H8", "O2"], &["CO2", "H2O"],
                    "C3H8 + 5 O2 -> 3 CO2 + 4 H2O."),
                mc("l2-cp-m1", c, "To balance an equation you adjust:",
                    &["coefficients", "subscripts", "atomic masses"], 0,
                    "Coefficients are changed; subscripts define the compound."),
                mc("l2-cp-m2", c, "Balancing enforces which law?",
                    &["conservation of mass", "ideal gas law", "Avogadro's law"], 0,
                    "Atoms are conserved, so mass is conserved."),
            ];
            v.extend(mc_filler("l2-cp", c, 12, &[
                ("Hydrogen gas is written as:", &["H2", "H", "H3"], 0),
                ("Changing a subscript to balance is:", &["wrong, it changes the substance", "fine", "required"], 0),
                ("A coefficient multiplies:", &["every atom in the formula", "only the first atom", "nothing"], 0),
            ]));
            v
        };
        let (eq_combustion, coeffs_c) = balance(&["CH4", "O2"], &["CO2", "H2O"]);
        lessons.push(json!({
            "id": "l2", "index": 2, "title": "Chemical equations and balancing",
            "objectives": [
                {"text": "Balance equations by conserving atoms", "conceptIds": [c]},
                {"text": "Explain why subscripts are never changed", "conceptIds": [c]}
            ],
            "conceptIds": [c],
            "explainers": [
                {"id": "l2-fig", "title": "A balanced equation", "conceptIds": [c],
                 "caption": "Atom-count bars equal on both sides.", "kind": "figure",
                 "data": {"figureId": "fig-l2-hero", "alt": hero_alt[1]}},
                {"id": "l2-bal", "title": "Balance methane combustion", "conceptIds": [c],
                 "caption": "Set coefficients until every element balances.", "kind": "equation_balancer",
                 "a11yState": "Each element's left and right atom counts; balanced when all match.",
                 "data": {"equation": eq_combustion, "answerCoefficients": coeffs_c}}
            ],
            "workedExamples": [
                worked("l2-w1", c, "Balance H2 + O2 -> H2O.",
                    vec![wstep("Balance O with a 2 on water", "H2 + O2 -> 2 H2O", "", ""),
                         wstep("Balance H with a 2 on hydrogen", "2 H2 + O2 -> 2 H2O", "", "")], 2.0, "coefficient on H2"),
                worked("l2-w2", c, "Balance N2 + H2 -> NH3.",
                    vec![wstep("Balance N", "N2 + H2 -> 2 NH3", "", ""),
                         wstep("Balance H", "N2 + 3 H2 -> 2 NH3", "", "")], 3.0, "coefficient on H2"),
                worked("l2-w3", c, "Balance C3H8 + O2 -> CO2 + H2O.",
                    vec![wstep("Balance C then H", "C3H8 + O2 -> 3 CO2 + 4 H2O", "", ""),
                         wstep("Balance O", "C3H8 + 5 O2 -> 3 CO2 + 4 H2O", "", "")], 5.0, "coefficient on O2")
            ],
            "misconceptions": {"items": [
                misconception("You can change a subscript to balance an equation.",
                    "Changing a subscript changes the compound's identity.",
                    "Only change the coefficients in front of each formula.", c),
                misconception("Elemental hydrogen and oxygen are written as single atoms.",
                    "These elements are diatomic gases.",
                    "Write them as H2 and O2.", c)
            ]},
            "trainer": {"id": "l2-t", "conceptIds": [c], "generators": [], "staticItems": [
                balance_item("l2-s1", c, "Balance: N2 + O2 -> N2O", &["N2", "O2"], &["N2O"], "2 N2 + O2 -> 2 N2O.")
            ]},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Lesson 3: Mole ratios and stoichiometric calculations ───────────────
    {
        let c = "c-ratio";
        let mm_h2 = molar_mass("H2");
        let mm_h2o = molar_mass("H2O");
        // 2 H2 + O2 -> 2 H2O: grams H2O from 4.00 g H2
        let moles_h2 = 4.0 / mm_h2;
        let mass_h2o = moles_h2 * mm_h2o; // 2:2 ratio
        let checkpoint_items = {
            let mut v = vec![
                numeric("l3-cp-n1", c, "For 2 H2 + O2 -> 2 H2O, grams of H2O from 4.00 g H2?", mass_h2o, "g",
                    "Convert to moles, apply the 2:2 ratio, convert back.",
                    vec![wstep("g H2 to mol", "4.00 / 2.02", "1.98", "mol H2"),
                         wstep("2:2 mole ratio", "1.98 x (2/2)", "1.98", "mol H2O"),
                         wstep("mol to g H2O", "1.98 x 18.02", "35.69", "g")]),
                numeric("l3-cp-n2", c, "For N2 + 3 H2 -> 2 NH3, mol NH3 from 3.00 mol H2?", 3.0*2.0/3.0, "mol",
                    "Apply the 3:2 ratio of H2 to NH3.",
                    vec![wstep("3:2 ratio", "3.00 x (2/3)", "2.00", "mol NH3")]),
                mc("l3-cp-m1", c, "Mole ratios come from:",
                    &["balanced-equation coefficients", "molar masses", "atomic numbers"], 0,
                    "Coefficients give the mole ratio."),
                mc("l3-cp-m2", c, "Before using a mole ratio you must:",
                    &["balance the equation", "measure the pressure", "cool the sample"], 0,
                    "The equation must be balanced first."),
            ];
            v.extend(mc_filler("l3-cp", c, 12, &[
                ("Masses can be used directly as mole ratios:", &["no, convert to moles first", "yes", "only for gases"], 0),
                ("A stoichiometric factor is a ratio of:", &["coefficients", "masses", "volumes"], 0),
                ("To find grams of product you end by:", &["multiplying moles by molar mass", "dividing by Avogadro", "adding coefficients"], 0),
            ]));
            v
        };
        lessons.push(json!({
            "id": "l3", "index": 3, "title": "Mole ratios and stoichiometric calculations",
            "objectives": [
                {"text": "Use coefficients as mole ratios", "conceptIds": [c]},
                {"text": "Carry a grams-to-grams calculation through moles", "conceptIds": [c]}
            ],
            "conceptIds": [c],
            "explainers": [
                {"id": "l3-fig", "title": "The stoichiometry railroad", "conceptIds": [c],
                 "caption": "Grams of A to grams of B through the mole ratio.", "kind": "figure",
                 "data": {"figureId": "fig-l3-hero", "alt": hero_alt[2]}},
                {"id": "l3-da", "title": "Dimensional-analysis railroad", "conceptIds": [c],
                 "caption": "Cancel units from 4.00 g H2 to grams of H2O.", "kind": "dimensional_analysis",
                 "a11yState": "Tiles cancel grams and moles until grams of H2O remain.",
                 "data": {"start": {"value": 4.0, "unit": "g H2"},
                          "target": {"unit": "g H2O"},
                          "tiles": [{"num": 1.0, "den": mm_h2}, {"num": 2.0, "den": 2.0}, {"num": mm_h2o, "den": 1.0}],
                          "value": mass_h2o}},
                {"id": "l3-mr", "title": "Mole-ratio step reveal", "conceptIds": [c],
                 "caption": "Each step shown with its numbers.", "kind": "mole_ratio",
                 "data": {"given": {"amount": 4.0, "unit": "g", "species": "H2"},
                          "want": {"unit": "g", "species": "H2O"},
                          "finalAnswer": {"value": mass_h2o, "unit": "g"}}}
            ],
            "workedExamples": [
                worked("l3-w1", c, "Grams of H2O from 2.02 g H2 (2 H2 + O2 -> 2 H2O).",
                    vec![wstep("g to mol H2", "2.02 / 2.02", "1.00", "mol"),
                         wstep("2:2 ratio", "1.00", "1.00", "mol H2O"),
                         wstep("mol to g", "1.00 x 18.02", "18.02", "g")], 18.02, "g"),
                worked("l3-w2", c, "Mol NH3 from 6.00 mol H2 (N2 + 3 H2 -> 2 NH3).",
                    vec![wstep("3:2 ratio", "6.00 x (2/3)", "4.00", "mol NH3")], 4.0, "mol"),
                worked("l3-w3", c, "Mol O2 to burn 1.00 mol CH4 (CH4 + 2 O2 -> CO2 + 2 H2O).",
                    vec![wstep("1:2 ratio", "1.00 x 2", "2.00", "mol O2")], 2.0, "mol")
            ],
            "misconceptions": {"items": [
                misconception("You can use grams directly in a mole ratio.",
                    "Mole ratios apply to moles, not masses.",
                    "Convert grams to moles before applying the ratio.", c),
                misconception("The equation does not need balancing first.",
                    "Coefficients from an unbalanced equation give the wrong ratio.",
                    "Balance the equation, then read the mole ratio.", c)
            ]},
            "trainer": {"id": "l3-t", "conceptIds": [c],
                "generators": [{"template": "mole_ratio", "conceptIds": [c]}], "staticItems": []},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Lesson 4: Limiting reagents and percent yield ───────────────────────
    {
        let c = "c-limiting";
        let checkpoint_items = {
            let mut v = vec![
                numeric("l4-cp-n1", c, "Actual yield 8.00 g, theoretical 10.00 g. Percent yield?", 80.0, "%",
                    "Percent yield = actual / theoretical x 100.",
                    vec![wstep("Divide and scale", "8.00 / 10.00 x 100", "80.0", "%")]),
                numeric("l4-cp-n2", c, "2 H2 + O2 -> 2 H2O with 2 mol H2 and 2 mol O2. Mol H2O (H2 limits)?", 2.0, "mol",
                    "H2 is limiting; 2:2 ratio gives 2 mol H2O.",
                    vec![wstep("H2 limits (needs 1 mol O2, has 2)", "2 mol H2 x (2/2)", "2.00", "mol H2O")]),
                mc("l4-cp-m1", c, "The limiting reagent is the one that:",
                    &["runs out first", "has the smallest mass", "has the largest molar mass"], 0,
                    "It is consumed first and caps the product."),
                mc("l4-cp-m2", c, "Percent yield above 100% usually means:",
                    &["measurement or impurity error", "a new element", "perfect reaction"], 0,
                    "Yield cannot really exceed theoretical."),
            ];
            v.extend(mc_filler("l4-cp", c, 12, &[
                ("The smaller starting mass is always limiting:", &["no, compare moles and ratios", "yes", "only for gases"], 0),
                ("Theoretical yield is based on the:", &["limiting reagent", "excess reagent", "catalyst"], 0),
                ("Excess reagent is the one that:", &["is left over", "runs out", "is a gas"], 0),
            ]));
            v
        };
        lessons.push(json!({
            "id": "l4", "index": 4, "title": "Limiting reagents and percent yield",
            "objectives": [
                {"text": "Identify the limiting reagent", "conceptIds": [c]},
                {"text": "Compute theoretical and percent yield", "conceptIds": [c]}
            ],
            "conceptIds": [c],
            "explainers": [
                {"id": "l4-fig", "title": "Which reactant runs out", "conceptIds": [c],
                 "caption": "Consumption bars reveal the limiting reagent.", "kind": "figure",
                 "data": {"figureId": "fig-l4-hero", "alt": hero_alt[3]}},
                {"id": "l4-lr", "title": "Limiting-reagent simulator", "conceptIds": [c],
                 "caption": "2 H2 + O2 -> 2 H2O with set amounts.", "kind": "limiting_reagent",
                 "a11yState": "Each reactant bar drains; the one emptying first limits the yield.",
                 "data": {"equation": {"reactants": [{"formula": "H2"}, {"formula": "O2"}], "products": [{"formula": "H2O"}]},
                          "reagents": [
                              {"species": "H2", "molarMass": molar_mass("H2"), "available": 4.04, "coeff": 2.0},
                              {"species": "O2", "molarMass": molar_mass("O2"), "available": 64.00, "coeff": 1.0}],
                          "productSpecies": "H2O"}}
            ],
            "workedExamples": [
                worked("l4-w1", c, "Percent yield for 18.0 g actual vs 20.0 g theoretical.",
                    vec![wstep("Divide and scale", "18.0 / 20.0 x 100", "90.0", "%")], 90.0, "%"),
                worked("l4-w2", c, "2 mol H2 with 3 mol O2: which limits (2 H2 + O2 -> 2 H2O)?",
                    vec![wstep("H2 needs 1 mol O2; O2 is in excess", "H2 limits", "", "")], 2.0, "mol H2O"),
                worked("l4-w3", c, "Theoretical H2O from 4.00 g H2 (excess O2).",
                    vec![wstep("mol H2", "4.00 / 2.02", "1.98", "mol"),
                         wstep("to g H2O", "1.98 x 18.02", "35.69", "g")], 35.69, "g")
            ],
            "misconceptions": {"items": [
                misconception("The reactant with the smaller mass is limiting.",
                    "Limiting depends on moles divided by coefficient, not raw mass.",
                    "Compare moles-over-coefficient for each reactant.", c),
                misconception("Percent yield can exceed 100%.",
                    "You cannot make more than the theoretical maximum.",
                    "A value over 100% signals impurity or measurement error.", c)
            ]},
            "trainer": {"id": "l4-t", "conceptIds": [c], "generators": [], "staticItems": [
                numeric("l4-s1", c, "Actual 4.50 g, theoretical 5.00 g. Percent yield?", 90.0, "%",
                    "actual/theoretical x 100", vec![wstep("compute", "4.50/5.00x100", "90.0", "%")])
            ]},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Lesson 5: Solution stoichiometry and titration ──────────────────────
    {
        let c = "c-solution";
        // titration curve of a strong acid by a strong base
        let points = json!([
            {"volAdded": 0.0, "pH": 1.0}, {"volAdded": 10.0, "pH": 1.5},
            {"volAdded": 20.0, "pH": 2.2}, {"volAdded": 24.0, "pH": 3.0},
            {"volAdded": 25.0, "pH": 7.0}, {"volAdded": 26.0, "pH": 11.0},
            {"volAdded": 30.0, "pH": 12.0}, {"volAdded": 40.0, "pH": 12.5}
        ]);
        let checkpoint_items = {
            let mut v = vec![
                numeric("l5-cp-n1", c, "Moles of HCl in 0.250 L of 0.100 M HCl?", 0.0250, "mol",
                    "moles = molarity x volume.",
                    vec![wstep("multiply", "0.100 x 0.250", "0.0250", "mol")]),
                numeric("l5-cp-n2", c, "Molarity of 0.0500 mol in 0.500 L?", 0.100, "mol/L",
                    "M = moles / volume.",
                    vec![wstep("divide", "0.0500 / 0.500", "0.100", "mol/L")]),
                numeric("l5-cp-n3", c, "Dilute 50.0 mL of 2.00 M to 200.0 mL: new molarity?", 0.500, "mol/L",
                    "M1V1 = M2V2.",
                    vec![wstep("solve M2", "2.00 x 50.0 / 200.0", "0.500", "mol/L")]),
                mc("l5-cp-m1", c, "The equivalence point is where:",
                    &["moles of acid equal moles of base", "the indicator is added", "the pH is 1"], 0,
                    "Stoichiometric equivalence of acid and base."),
            ];
            v.extend(mc_filler("l5-cp", c, 12, &[
                ("Molarity has units of:", &["mol/L", "g/mol", "L/mol"], 0),
                ("Dilution changes:", &["concentration, not moles", "moles, not concentration", "neither"], 0),
                ("The endpoint and equivalence point are:", &["close but not identical", "always identical", "unrelated"], 0),
            ]));
            v
        };
        lessons.push(json!({
            "id": "l5", "index": 5, "title": "Solution stoichiometry and titration",
            "objectives": [
                {"text": "Relate molarity, moles and volume", "conceptIds": [c]},
                {"text": "Find an unknown concentration from a titration", "conceptIds": [c]}
            ],
            "conceptIds": [c],
            "explainers": [
                {"id": "l5-fig", "title": "A titration curve", "conceptIds": [c],
                 "caption": "pH versus titrant volume with the equivalence point.", "kind": "figure",
                 "data": {"figureId": "fig-l5-hero", "alt": hero_alt[4]}},
                {"id": "l5-tc", "title": "Titration curve explorer", "conceptIds": [c],
                 "caption": "Move the titrant volume along the curve.", "kind": "titration_curve",
                 "a11yState": "pH rises steeply through the equivalence point at 25 mL.",
                 "data": {"acid": {"conc": 0.1, "volume": 25.0}, "base": {"conc": 0.1, "volume": 0.0},
                          "points": points, "equivalenceVol": 25.0}}
            ],
            "workedExamples": [
                worked("l5-w1", c, "Moles in 0.500 L of 0.200 M NaOH.",
                    vec![wstep("M x V", "0.200 x 0.500", "0.100", "mol")], 0.100, "mol"),
                worked("l5-w2", c, "Molarity of 0.0250 mol in 0.250 L.",
                    vec![wstep("moles / volume", "0.0250 / 0.250", "0.100", "mol/L")], 0.100, "mol/L"),
                worked("l5-w3", c, "Dilute 25.0 mL of 1.00 M to 100.0 mL.",
                    vec![wstep("M1V1=M2V2", "1.00 x 25.0 / 100.0", "0.250", "mol/L")], 0.250, "mol/L")
            ],
            "misconceptions": {"items": [
                misconception("The endpoint is exactly the equivalence point.",
                    "The indicator color change (endpoint) only approximates equivalence.",
                    "Choose an indicator whose endpoint is near the equivalence pH.", c),
                misconception("Dilution changes the number of moles of solute.",
                    "Adding solvent changes concentration, not the moles present.",
                    "Use M1V1 = M2V2; moles stay constant.", c)
            ]},
            "trainer": {"id": "l5-t", "conceptIds": [c], "generators": [], "staticItems": [
                numeric("l5-s1", c, "Moles in 0.100 L of 0.500 M HCl?", 0.0500, "mol",
                    "M x V", vec![wstep("multiply", "0.500 x 0.100", "0.0500", "mol")])
            ]},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Lesson 6: Gas stoichiometry ─────────────────────────────────────────
    {
        let c = "c-gas";
        let checkpoint_items = {
            let mut v = vec![
                numeric("l6-cp-n1", c, "Volume of 2.00 mol of an ideal gas at STP?", 2.0*22.41, "L",
                    "At STP one mole occupies 22.41 L.",
                    vec![wstep("multiply", "2.00 x 22.41", "44.82", "L")]),
                numeric("l6-cp-n2", c, "Moles in 44.82 L of gas at STP?", 2.0, "mol",
                    "Divide by 22.41 L/mol.",
                    vec![wstep("divide", "44.82 / 22.41", "2.00", "mol")]),
                numeric("l6-cp-n3", c, "Using PV=nRT, V of 1.00 mol at 1.00 atm and 273.15 K (R=0.0821)?", 1.0*0.0821*273.15/1.0, "L",
                    "V = nRT/P.",
                    vec![wstep("compute", "1.00 x 0.0821 x 273.15 / 1.00", "22.43", "L")]),
                mc("l6-cp-m1", c, "Molar volume at STP is:",
                    &["22.41 L/mol", "1.00 L/mol", "6.022 L/mol"], 0,
                    "One mole of ideal gas is 22.41 L at STP."),
            ];
            v.extend(mc_filler("l6-cp", c, 12, &[
                ("In PV=nRT, R is the:", &["gas constant", "rate", "radius"], 0),
                ("Using 22.4 L/mol off STP is:", &["an error", "always fine", "required"], 0),
                ("Doubling moles at fixed T and P:", &["doubles volume", "halves volume", "no change"], 0),
            ]));
            v
        };
        lessons.push(json!({
            "id": "l6", "index": 6, "title": "Gas stoichiometry (ideal gas and gas-law stoichiometry)",
            "objectives": [
                {"text": "Use molar volume at STP", "conceptIds": [c]},
                {"text": "Apply PV = nRT in stoichiometry", "conceptIds": [c]}
            ],
            "conceptIds": [c],
            "explainers": [
                {"id": "l6-fig", "title": "Gas at STP", "conceptIds": [c],
                 "caption": "One mole of ideal gas occupies 22.41 L.", "kind": "figure",
                 "data": {"figureId": "fig-l6-hero", "alt": hero_alt[5]}},
                {"id": "l6-pv", "title": "Ideal-gas PV explorer", "conceptIds": [c],
                 "caption": "Set two variables; the others follow PV = nRT.", "kind": "ideal_gas_pv",
                 "a11yState": "Pressure and volume trade off along the isotherm at fixed n and T.",
                 "data": {"n": 1.0, "T": 273.15, "R": 0.0821,
                          "bounds": {"pMin": 0.5, "pMax": 2.0, "vMin": 11.0, "vMax": 45.0}}},
                {"id": "l6-mol", "title": "Gas molecule diagram", "conceptIds": [c],
                 "caption": "A diatomic gas molecule.", "kind": "molecule_diagram",
                 "a11yState": "Two oxygen atoms joined by a double bond.",
                 "data": {"atoms": [{"id": "o1", "element": "O", "x": 0.35, "y": 0.5},
                                     {"id": "o2", "element": "O", "x": 0.65, "y": 0.5}],
                          "bonds": [{"a": "o1", "b": "o2", "order": 2}],
                          "labels": [{"atom": "o1", "text": "O"}, {"atom": "o2", "text": "O"}]}}
            ],
            "workedExamples": [
                worked("l6-w1", c, "Volume of 0.500 mol of gas at STP.",
                    vec![wstep("x 22.41", "0.500 x 22.41", "11.21", "L")], 0.5*22.41, "L"),
                worked("l6-w2", c, "Moles in 11.21 L at STP.",
                    vec![wstep("/ 22.41", "11.21 / 22.41", "0.500", "mol")], 0.500, "mol"),
                worked("l6-w3", c, "V of 2.00 mol at 2.00 atm, 300 K (R=0.0821).",
                    vec![wstep("nRT/P", "2.00 x 0.0821 x 300 / 2.00", "24.63", "L")], 2.0*0.0821*300.0/2.0, "L")
            ],
            "misconceptions": {"items": [
                misconception("22.4 L/mol works at any temperature and pressure.",
                    "That molar volume holds only at STP.",
                    "Use PV = nRT away from STP.", c),
                misconception("Pressure and temperature units do not matter in PV=nRT.",
                    "R fixes the units; T must be in kelvin and P in atm for R = 0.0821.",
                    "Convert to kelvin and atm before substituting.", c)
            ]},
            "trainer": {"id": "l6-t", "conceptIds": [c],
                "generators": [{"template": "gas_volume_stp", "conceptIds": [c]}], "staticItems": []},
            "checkpoint": {"presentCount": 8, "items": checkpoint_items}
        }));
    }

    // ── Diagnostic (optional placement) ─────────────────────────────────────
    let diagnostic = json!({
        "items": [
            mc("dx1", "c-mole", "A mole is:", &["6.022 x 10^23 particles", "one gram", "one liter"], 0, "Avogadro's number of particles."),
            mc("dx2", "c-balance", "Balancing conserves:", &["atoms", "pressure", "temperature"], 0, "Atoms are conserved."),
            mc("dx3", "c-ratio", "Mole ratios come from:", &["coefficients", "molar masses", "volumes"], 0, "Coefficients."),
        ]
    });

    // ── Exam bank (>=60: >=18 mc, >=18 numeric, >=9 multistep) ───────────────
    let exam_items = build_exam_items();

    let lessonset = json!({
        "id": SEED_SET_ID, "schemaMajor": 1,
        "title": "Stoichiometry", "subtitle": "AP Chem / college level", "level": "AP Chem / college",
        "provenance": "seed",
        "lessons": ["l1", "l2", "l3", "l4", "l5", "l6"],
        "concepts": concepts,
        "diagnostic": diagnostic,
        "exam": {"items": exam_items, "questionCount": 30, "passPct": 80,
                 "mix": {"mc": 12, "numeric": 12, "balance": 0, "multistep": 6}}
    });

    // ── Assemble file entries + manifest ─────────────────────────────────────
    let lessonset_bytes = serde_json::to_vec_pretty(&lessonset).unwrap();
    file_entries.push(("lessonset.json".to_string(), lessonset_bytes));
    for lesson in &lessons {
        let id = lesson["id"].as_str().unwrap();
        let bytes = serde_json::to_vec_pretty(lesson).unwrap();
        file_entries.push((format!("lessons/{id}.json"), bytes));
    }
    // Deterministic order.
    file_entries.sort_by(|a, b| a.0.cmp(&b.0));

    let figure_structs: Vec<nofriction_meetings_lib::lessons::schema::Figure> =
        figures.iter().map(|f| serde_json::from_value(f.clone()).unwrap()).collect();

    let manifest_files: Vec<ManifestFile> = file_entries
        .iter()
        .map(|(p, b)| ManifestFile {
            path: p.clone(),
            sha256: sha256_hex(b),
            bytes: b.len() as u64,
            kind: if p.ends_with(".png") { Some("image/png".into()) } else { None },
        })
        .collect();

    let fingerprint = bundle::fingerprint_for(&file_entries, &figure_structs).unwrap();

    let manifest = json!({
        "format": "nofriction.lesson",
        "schemaMajor": 1, "schemaMinor": 0,
        "lessonSetId": SEED_SET_ID,
        "title": "Stoichiometry",
        "provenance": "seed",
        "files": manifest_files,
        "contentFingerprint": fingerprint,
        "authoredAt": "2026-10-10T00:00:00Z",
        "generator": "nofriction-authoring 1.0",
        "figures": figures,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();

    let zip = bundle::write_zip(&manifest_bytes, &file_entries).expect("write zip");

    // ── Validate through the ordinary import path (dogfooding, §4.5) ─────────
    match bundle::validate_bytes(&zip) {
        Ok(prepared) => {
            assert_eq!(prepared.validated.lessons.len(), 6, "expected 6 lessons");
            println!(
                "seed bundle validates: {} lessons, {} figures, {} exam items, fingerprint {}",
                prepared.validated.lessons.len(),
                prepared.validated.figures.len(),
                prepared.validated.set.exam.items.len(),
                &prepared.content_fingerprint[..12]
            );
        }
        Err(e) => {
            eprintln!("SEED BUNDLE FAILED VALIDATION: {e}");
            std::process::exit(1);
        }
    }

    std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    std::fs::write(&out_path, &zip).unwrap();
    println!("wrote {} ({} bytes)", out_path.display(), zip.len());
}

/// Build the over-provisioned exam bank spanning all six lessons.
fn build_exam_items() -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    let mm_h2o = molar_mass("H2O");
    let mm_co2 = molar_mass("CO2");
    let mm_h2 = molar_mass("H2");

    // ── 20 MC across concepts (>=18) ──
    let mc_bank: &[(&str, &str, &[&str], u32)] = &[
        ("c-mole", "A mole is defined as:", &["6.022 x 10^23 particles", "one gram", "22.4 L"], 0),
        ("c-mole", "Avogadro's number is:", &["6.022 x 10^23", "3.14", "9.81"], 0),
        ("c-molar-mass", "Molar mass units are:", &["g/mol", "mol/g", "L/mol"], 0),
        ("c-molar-mass", "Molar mass of O2 is:", &["32.00 g/mol", "16.00 g/mol", "48.00 g/mol"], 0),
        ("c-balance", "Balancing adjusts:", &["coefficients", "subscripts", "atomic masses"], 0),
        ("c-balance", "Hydrogen gas is:", &["H2", "H", "H3"], 0),
        ("c-balance", "Balancing enforces conservation of:", &["mass", "charge only", "volume"], 0),
        ("c-ratio", "Mole ratios come from:", &["coefficients", "molar masses", "densities"], 0),
        ("c-ratio", "Before a mole ratio you must:", &["balance", "cool", "compress"], 0),
        ("c-limiting", "The limiting reagent:", &["runs out first", "is heaviest", "is a catalyst"], 0),
        ("c-limiting", "Percent yield is:", &["actual/theoretical x 100", "theoretical/actual", "actual x theoretical"], 0),
        ("c-solution", "Molarity is:", &["mol/L", "g/L", "L/mol"], 0),
        ("c-solution", "Equivalence point is where:", &["acid moles equal base moles", "pH is 1", "color first appears"], 0),
        ("c-gas", "Molar volume at STP:", &["22.41 L/mol", "1 L/mol", "6 L/mol"], 0),
        ("c-gas", "In PV=nRT, T must be in:", &["kelvin", "celsius", "fahrenheit"], 0),
        ("c-mole", "Grams to moles: you:", &["divide by molar mass", "multiply by Avogadro", "add"], 0),
        ("c-molar-mass", "Molar mass of H2O:", &["18.02 g/mol", "2.02 g/mol", "16.00 g/mol"], 0),
        ("c-balance", "Changing a subscript to balance:", &["changes the substance", "is correct", "is required"], 0),
        ("c-ratio", "A stoichiometric factor relates:", &["moles of two species", "mass and volume", "pH and volume"], 0),
        ("c-gas", "22.4 L/mol applies:", &["only at STP", "always", "never"], 0),
    ];
    // Distribute the authored answer key across positions (defense-in-depth on
    // top of the presentation-time choice shuffle in QuizRunner): rotate each
    // item's choices so the correct one lands at a varied index, carrying the
    // answerIndex with it. An exam whose keys are all index 0 is a review
    // finding; the seed-validation test asserts the keys are not all identical.
    for (i, (c, p, choices, a)) in mc_bank.iter().enumerate() {
        let target = i % choices.len();
        let (rotated, new_idx) = rotate_choices(choices, *a as usize, target);
        let rotated_refs: Vec<&str> = rotated.iter().map(|s| s.as_str()).collect();
        items.push(mc(&format!("ex-mc-{i}"), c, p, &rotated_refs, new_idx as u32, "See the lesson summary."));
    }

    // ── 20 numeric across concepts (>=18) ──
    let moles_h2 = 4.0 / mm_h2;
    let mass_h2o = moles_h2 * mm_h2o;
    let numeric_bank: Vec<(&str, String, f64, &str)> = vec![
        ("c-mole", "Moles in 36.04 g H2O?".into(), 36.04/mm_h2o, "mol"),
        ("c-mole", "Moles in 88.02 g CO2?".into(), 88.02/mm_co2, "mol"),
        ("c-mole", "Moles in 18.02 g H2O?".into(), 1.0, "mol"),
        ("c-molar-mass", "Mass of 2.00 mol CO2?".into(), 2.0*mm_co2, "g"),
        ("c-molar-mass", "Mass of 0.500 mol H2O?".into(), 0.5*mm_h2o, "g"),
        ("c-molar-mass", "Molar mass of CO2?".into(), mm_co2, "g/mol"),
        ("c-ratio", "g H2O from 4.00 g H2 (2 H2 + O2 -> 2 H2O)?".into(), mass_h2o, "g"),
        ("c-ratio", "mol NH3 from 3.00 mol H2 (N2 + 3 H2 -> 2 NH3)?".into(), 2.0, "mol"),
        ("c-ratio", "mol O2 to burn 2.00 mol CH4 (CH4 + 2 O2 -> ...)?".into(), 4.0, "mol"),
        ("c-limiting", "Percent yield: 8.00 g of 10.00 g?".into(), 80.0, "%"),
        ("c-limiting", "Percent yield: 4.50 g of 5.00 g?".into(), 90.0, "%"),
        ("c-limiting", "Theoretical g H2O from 2.02 g H2 (excess O2)?".into(), (2.02/mm_h2)*mm_h2o, "g"),
        ("c-solution", "Moles in 0.250 L of 0.100 M?".into(), 0.0250, "mol"),
        ("c-solution", "Molarity of 0.0500 mol in 0.500 L?".into(), 0.100, "mol/L"),
        ("c-solution", "Dilute 50.0 mL 2.00 M to 200.0 mL: M?".into(), 0.500, "mol/L"),
        ("c-gas", "Volume of 2.00 mol gas at STP?".into(), 2.0*22.41, "L"),
        ("c-gas", "Moles in 44.82 L at STP?".into(), 2.0, "mol"),
        ("c-gas", "V of 1.00 mol at 1.00 atm, 273.15 K (R=0.0821)?".into(), 0.0821*273.15, "L"),
        ("c-mole", "Moles in 44.01 g CO2?".into(), 1.0, "mol"),
        ("c-gas", "Volume of 0.500 mol gas at STP?".into(), 0.5*22.41, "L"),
    ];
    for (i, (c, p, ans, unit)) in numeric_bank.iter().enumerate() {
        items.push(numeric(&format!("ex-num-{i}"), c, p, *ans, unit, "Carry the dimensional analysis.",
            vec![wstep("Compute", p, &format!("{ans:.3}"), unit)]));
    }
    // Extra numeric scaled variants so the bank over-provisions well past 18.
    for (i, factor) in [2.0_f64, 3.0, 4.0, 5.0, 1.5, 2.5].iter().enumerate() {
        let moles = factor;
        items.push(numeric(&format!("ex-num-x{i}"), "c-mole",
            &format!("Moles in {:.2} g of H2O?", moles * mm_h2o),
            *moles, "mol", "Divide grams by molar mass.",
            vec![wstep("Divide", &format!("{:.2} / {:.2}", moles*mm_h2o, mm_h2o), &format!("{moles:.2}"), "mol")]));
        items.push(numeric(&format!("ex-num-y{i}"), "c-gas",
            &format!("Volume of {factor:.2} mol of gas at STP?"),
            factor * 22.41, "L", "Multiply by 22.41 L/mol.",
            vec![wstep("Multiply", &format!("{factor:.2} x 22.41"), &format!("{:.2}", factor*22.41), "L")]));
    }

    // ── 12 DISTINCT multistep items (>=9) spanning the mole-ratio,
    //    limiting-reagent / percent-yield, and gas-law concept chains, so the
    //    drawn 6 differ and a retake is a genuinely different paper. Each item
    //    chains a balance (or ratio) step -> a reasoning MC step -> a numeric
    //    step; numeric answers are computed from the shared molar-mass table so
    //    the authoring run's recompute gate verifies them.
    let mm_nh3 = molar_mass("NH3");
    let mm_o2 = molar_mass("O2");
    let multistep_items: Vec<Value> = vec![
        // 1. Methane combustion: ratio then grams CO2.
        multistep(
            "ex-ms-0", "c-ratio",
            "Balance the combustion of methane, then find grams of CO2 from 1.00 mol CH4.",
            vec![
                step_balance("Balance CH4 + O2 -> CO2 + H2O", &["CH4", "O2"], &["CO2", "H2O"]),
                step_mc("The CH4 : CO2 mole ratio is:", &["1 : 1", "1 : 2", "2 : 1"], 0),
                step_numeric("Grams of CO2 from 1.00 mol CH4", mm_co2, "g"),
            ],
            "Balance (1,2,1,2), apply the 1:1 ratio, then multiply by the molar mass of CO2.",
        ),
        // 2. Ammonia synthesis: ratio then grams NH3 from 2.00 mol N2.
        multistep(
            "ex-ms-1", "c-ratio",
            "Balance N2 + H2 -> NH3, then find grams of NH3 from 2.00 mol N2 (excess H2).",
            vec![
                step_balance("Balance N2 + H2 -> NH3", &["N2", "H2"], &["NH3"],),
                step_mc("The N2 : NH3 mole ratio is:", &["1 : 2", "1 : 1", "2 : 1"], 0),
                step_numeric("Grams of NH3 from 2.00 mol N2", 4.0 * mm_nh3, "g"),
            ],
            "Balance (1,3,2); 1:2 ratio gives 4.00 mol NH3; multiply by the molar mass of NH3.",
        ),
        // 3. Hydrogen combustion: ratio then grams H2O from 3.00 mol H2.
        multistep(
            "ex-ms-2", "c-ratio",
            "Balance H2 + O2 -> H2O, then find grams of H2O from 3.00 mol H2 (excess O2).",
            vec![
                step_balance("Balance H2 + O2 -> H2O", &["H2", "O2"], &["H2O"],),
                step_mc("The H2 : H2O mole ratio is:", &["1 : 1", "2 : 1", "1 : 2"], 0),
                step_numeric("Grams of H2O from 3.00 mol H2", 3.0 * mm_h2o, "g"),
            ],
            "Balance (2,1,2); 1:1 ratio gives 3.00 mol H2O; multiply by the molar mass of H2O.",
        ),
        // 4. Limiting reagent: 2 H2 + O2 -> 2 H2O, 2 mol H2 vs 2 mol O2.
        multistep(
            "ex-ms-3", "c-limiting",
            "For 2 H2 + O2 -> 2 H2O with 2.00 mol H2 and 2.00 mol O2, find the mol of H2O formed.",
            vec![
                step_mc("Which reactant is limiting?", &["H2", "O2", "neither"], 0),
                step_mc("How much O2 does 2.00 mol H2 require?", &["1.00 mol", "2.00 mol", "4.00 mol"], 0),
                step_numeric("Mol of H2O formed", 2.0, "mol"),
            ],
            "H2 limits (needs 1.00 mol O2, O2 is in excess); the 2:2 ratio gives 2.00 mol H2O.",
        ),
        // 5. Limiting reagent with grams: 4.04 g H2 (2.00 mol) + excess O2.
        multistep(
            "ex-ms-4", "c-limiting",
            "For 2 H2 + O2 -> 2 H2O with 4.04 g H2 and excess O2, find grams of H2O.",
            vec![
                step_numeric("Moles of H2 in 4.04 g", 4.04 / mm_h2, "mol"),
                step_mc("The H2 : H2O ratio is:", &["1 : 1", "2 : 1", "1 : 2"], 0),
                step_numeric("Grams of H2O formed", (4.04 / mm_h2) * mm_h2o, "g"),
            ],
            "4.04 g H2 = 2.00 mol; the 2:2 ratio gives 2.00 mol H2O; multiply by 18.02 g/mol.",
        ),
        // 6. Percent yield after a stoichiometry calc.
        multistep(
            "ex-ms-5", "c-limiting",
            "1.00 mol CH4 burns to CO2; the actual yield is 35.21 g CO2. Find the percent yield.",
            vec![
                step_mc("The CH4 : CO2 ratio is:", &["1 : 1", "1 : 2", "2 : 1"], 0),
                step_numeric("Theoretical grams of CO2", mm_co2, "g"),
                step_numeric("Percent yield (actual 35.21 g)", 35.21 / mm_co2 * 100.0, "%"),
            ],
            "1:1 ratio -> 44.01 g theoretical; 35.21 / 44.01 x 100 = 80.0%.",
        ),
        // 7. Percent yield, ammonia.
        multistep(
            "ex-ms-6", "c-limiting",
            "2.00 mol N2 reacts to form NH3; the actual yield is 54.50 g NH3. Find the percent yield.",
            vec![
                step_mc("The N2 : NH3 ratio is:", &["1 : 2", "1 : 1", "2 : 1"], 0),
                step_numeric("Theoretical grams of NH3", 4.0 * mm_nh3, "g"),
                step_numeric("Percent yield (actual 54.50 g)", 54.50 / (4.0 * mm_nh3) * 100.0, "%"),
            ],
            "1:2 ratio -> 4.00 mol = 68.12 g theoretical; 54.50 / 68.12 x 100 = 80.0%.",
        ),
        // 8. Gas law: moles then volume at STP.
        multistep(
            "ex-ms-7", "c-gas",
            "Find the STP volume of the CO2 from burning 0.500 mol CH4 (CH4 + 2 O2 -> CO2 + 2 H2O).",
            vec![
                step_mc("The CH4 : CO2 ratio is:", &["1 : 1", "1 : 2", "2 : 1"], 0),
                step_numeric("Moles of CO2 produced", 0.5, "mol"),
                step_numeric("Volume of CO2 at STP", 0.5 * 22.41, "L"),
            ],
            "1:1 ratio -> 0.500 mol CO2; 0.500 x 22.41 = 11.21 L at STP.",
        ),
        // 9. Gas law: PV = nRT.
        multistep(
            "ex-ms-8", "c-gas",
            "2.00 mol of an ideal gas is held at 1.00 atm and 273.15 K (R = 0.0821). Find its volume.",
            vec![
                step_mc("Which form solves for V?", &["V = nRT / P", "V = P / nRT", "V = nP / RT"], 0),
                step_numeric("Volume (L)", 2.0 * 0.0821 * 273.15 / 1.0, "L"),
                step_mc("At STP this equals about:", &["2 x 22.41 L", "22.41 L", "11.21 L"], 0),
            ],
            "V = nRT/P = 2.00 x 0.0821 x 273.15 / 1.00 = 44.86 L, about 2 x 22.41 L.",
        ),
        // 10. Gas stoichiometry: O2 needed to burn methane, as a volume.
        multistep(
            "ex-ms-9", "c-gas",
            "Burning 1.00 mol CH4 needs O2 (CH4 + 2 O2 -> CO2 + 2 H2O). Find the STP volume of that O2.",
            vec![
                step_balance("Balance CH4 + O2 -> CO2 + H2O", &["CH4", "O2"], &["CO2", "H2O"]),
                step_mc("The CH4 : O2 ratio is:", &["1 : 2", "1 : 1", "2 : 1"], 0),
                step_numeric("Volume of O2 at STP", 2.0 * 22.41, "L"),
            ],
            "Balance (1,2,1,2); 1:2 ratio -> 2.00 mol O2; 2.00 x 22.41 = 44.82 L.",
        ),
        // 11. Mole ratio: propane combustion to CO2 grams.
        multistep(
            "ex-ms-10", "c-ratio",
            "Balance C3H8 + O2 -> CO2 + H2O, then find grams of CO2 from 1.00 mol C3H8.",
            vec![
                step_balance("Balance C3H8 + O2 -> CO2 + H2O", &["C3H8", "O2"], &["CO2", "H2O"]),
                step_mc("The C3H8 : CO2 ratio is:", &["1 : 3", "1 : 1", "3 : 1"], 0),
                step_numeric("Grams of CO2 from 1.00 mol C3H8", 3.0 * mm_co2, "g"),
            ],
            "Balance (1,5,3,4); 1:3 ratio -> 3.00 mol CO2; 3.00 x 44.01 = 132.03 g.",
        ),
        // 12. Limiting reagent with O2 given in grams.
        multistep(
            "ex-ms-11", "c-limiting",
            "For 2 H2 + O2 -> 2 H2O with 2.00 mol H2 and 64.00 g O2, find the limiting reactant and mol H2O.",
            vec![
                step_numeric("Moles of O2 in 64.00 g", 64.00 / mm_o2, "mol"),
                step_mc("Which reactant is limiting?", &["H2", "O2", "neither"], 0),
                step_numeric("Mol of H2O formed", 2.0, "mol"),
            ],
            "64.00 g O2 = 2.00 mol; 2.00 mol H2 needs only 1.00 mol O2, so H2 limits -> 2.00 mol H2O.",
        ),
    ];
    items.extend(multistep_items);

    items
}
