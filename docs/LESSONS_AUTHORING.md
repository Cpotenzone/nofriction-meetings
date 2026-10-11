# Authoring the seed lessons and their figures

This is the authoring-time procedure and review gate for the Stoichiometry seed
course (`src-tauri/resources/seed/stoichiometry.lesson`). None of this ships in
the app binary — the shipped app makes **no image API call at runtime, in any
flavor** (docs/LESSONS.md §12, FR-FIG-2).

## 1. The authoring tool

`src-tauri/examples/author_seed_lessons.rs` assembles the six lessons, embeds
the hero figure PNGs, builds the `.lesson` zip (hashed manifest + content
fingerprint), validates it through the **ordinary import path**
(`bundle::validate_bytes`, dogfooding), and writes the resource. Run it from
`src-tauri/`:

```
cargo run --example author_seed_lessons
```

All answer keys are recomputed and verified by the same Rust that grades at
runtime (the equation balancer, the numeric grader, the generators), so the
content cannot ship with a wrong "correct" answer. The seed-validation test
(`src-tauri/src/lessons/tests.rs::shipped_seed_bundle_passes_validation_with_all_floors`)
re-asserts every §15 floor on the committed artifact, so a content edit that
breaks a key, a floor, or a figure fails the build.

## 2. Figures are required (owner-decision D4)

Each of the six lessons embeds **one AI-authored hero figure PNG**. A seed
bundle with a figure-free lesson, a dangling `figureId`, or a non-decoding PNG
**fails validation** — there is no "all-SVG seed is shippable" escape hatch. The
deterministic SVG explainers are additive, never a replacement.

Figures live at `scripts/seed/figures/lesson1.png … lesson6.png`. The authoring
tool refuses to produce a bundle if any is missing or does not decode.

### Figure manifest (reproducible authoring)

Each figure is recorded in `scripts/seed/figures.manifest.json` so a re-run
reproduces it:

```jsonc
{
  "artDirection": "Hazard yellow (#facc15) line art and accents on a matte-black background; flat minimalist scientific-textbook style; crisp sentence-case sans-serif labels; legible at 2x; no photorealism, no watermark; high contrast.",
  "model": "gemini-3-pro-image",            // Nano Banana Pro, via the genmedia MCP image tool
  "figures": [
    { "id": "fig-l1-hero", "file": "lesson1.png", "prompt": "The mole concept and molar mass: a balance scale relating grams to moles, Avogadro's number 6.022 x 10^23, and grams / molar mass = moles; H2O labeled 18.02 g/mol." },
    { "id": "fig-l2-hero", "file": "lesson2.png", "prompt": "A balanced equation with per-element atom-count bars equal on both sides (2 H2 + O2 -> 2 H2O)." },
    { "id": "fig-l3-hero", "file": "lesson3.png", "prompt": "A dimensional-analysis railroad converting grams of a reactant to grams of a product through the mole ratio." },
    { "id": "fig-l4-hero", "file": "lesson4.png", "prompt": "Two reactant consumption bars draining to reveal the limiting reagent and leftover excess, with the theoretical yield." },
    { "id": "fig-l5-hero", "file": "lesson5.png", "prompt": "A titration curve of pH versus titrant volume with the equivalence point marked." },
    { "id": "fig-l6-hero", "file": "lesson6.png", "prompt": "An ideal-gas PV explorer at STP: one mole occupies 22.41 L and PV = nRT." }
  ]
}
```

### Producing the PNGs

Two sources, in order of preference:

1. **Art-directed (preferred, owner-decision D4):** generate each PNG with the
   genmedia MCP image tool (`gemini-3-pro-image` / "Nano Banana Pro") using the
   shared art direction and the per-figure prompt above, at a legible-at-2x size
   (target the whole six-figure set under ~40 MB, NFR-5), save it to
   `scripts/seed/figures/`, then re-run the authoring tool.

2. **Deterministic fallback (shipped now):** `scripts/seed/render_figures.py`
   renders the six hero figures as clean, correctly-labeled scientific diagrams
   in the same hazard-yellow-on-matte-black style (balance scale, atom-count
   bars, the stoichiometry railroad, draining limiting-reagent bars, a titration
   curve, and the ideal-gas container). These are the **currently shipped** seed
   figures — correct, legible, and reviewed, not placeholder slop — produced
   because the genmedia tool's cloud credentials need re-authentication
   (`invalid_rapt`, see the Google reauth link) at the time of authoring. They
   satisfy the figure floor and the mechanical gate; the art-directed Nano Banana
   Pro versions are a drop-in enhancement: regenerate per step 1 once genmedia
   access is restored and re-run the authoring tool to replace them.

> **The build never ships a figure-free seed bundle (D4).** Whichever source is
> used, the authoring tool refuses to produce a bundle unless every lesson's
> hero PNG is present and decodes, and the seed-validation test re-asserts it.

## 3. The two-part accuracy gate (OQ-9)

- **Mechanical half (automated):** the authoring tool + seed-validation test
  recompute and assert every answer key is self-consistent (balanced equations
  balance, molar masses correct to 2 dp, dimensional-analysis products match the
  stated final answers) and that every figure has a non-empty `reviewedBy`.
- **Human half (not automatable):** the **scientific correctness of each
  figure's depiction** — the right diagram, the right curve shape, correct
  labels — is a human review owned by the **seed content reviewer**, recorded
  against the checklist below and stamped into each figure's `reviewedBy`.

### Figure review checklist (per figure, by the seed content reviewer)

- [ ] The depicted chemistry is correct (right species, right relationship).
- [ ] Labels are legible, sentence case, and match the lesson's values.
- [ ] The art direction is followed (hazard yellow on matte black, no slop,
      legible at 2x).
- [ ] The PNG decodes to its declared width×height and is under the size budget.
- [ ] `reviewedBy` is stamped with the reviewer's name/role.
