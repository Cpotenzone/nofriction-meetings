# Lessons subsystem — backend / correctness / invariants review (review-a)

Scope: the Rust `lessons` module on `feature/lessons-stoichiometry` vs `main`, the
`.lesson` bundle integrity/import/export, the authoritative grader, mastery +
spaced-repetition, adaptive trainers, exam/checkpoint scoring + gating,
certificate hashing, seed loading, purge integration, and the chemistry of the
six shipped seed lessons. Verified against docs/LESSONS.md (rev 5) and
owner-decisions D1–D5.

The subsystem is in strong shape. It compiles in both Mac flavors
(`cargo test --lib` and `--features mas --no-run`), the AI-provider policy guard
and the D5 Lessons offline guard both pass, all 28 lessons unit tests pass, and
every piece of seed chemistry I spot-checked by hand is correct. There is one
grading-correctness gap worth fixing before this ships as a "pass a real exam"
product: ten authored numeric items declare answer units the authoritative
grader's own unit table cannot parse, so a learner who types the correct number
*with* the unit the prompt asks for is graded "couldn't read that unit."

**Watch for:** [MEDIUM, confirmed] numeric items with units `mol/L`, `g/mol`,
`%` are graded unparseable when the learner echoes the unit — the keys are
correct, but entering the stated unit is rejected (10 items + multistep legs).

**Verdict**: NEEDS_CHANGES

## High-level view

Integrity is sound and layered. Every bundle file is SHA-256-checked against
`manifest.files`, the file set must match exactly (no extra/missing entries),
`..`/absolute zip paths are rejected, each figure PNG is re-hashed *and* decoded
to its declared dimensions, and the recomputed content fingerprint must equal a
non-empty declared one. Import validates fully in a temp dir and commits in one
DB transaction with staging→rename so a reader never sees a row without files.
Tamper of any payload byte is rejected (test `per_file_hash_tamper_is_rejected`).

The grader is authoritative in Rust and never panics on hostile input. Numeric
parsing tolerates scientific-notation spellings and an optional trailing unit;
the single UNITS table converts compatible prefixed/scaled units (incl. the
one `°C→K` offset) and rejects dimension mismatches. The balance checker
recomputes lowest-integer coefficients by integer linear algebra and compares
order-independently, accepting any correct multiple. MC/multistep grading and
gated-chain zeroing are correct.

Exam and checkpoint scoring are server-authoritative. The exam gate requires
every lesson's checkpoint passed; the drawn 30-item selection is persisted and
the submit scores over that server-known set (unanswered → zero), so a one-item
post cannot reach 100%. The checkpoint denominator is floored at `presentCount`.
Mastery (EMA α=0.4, retry credit, mastered ≥85 & ≥3 correct) and SM-2-lite
review (persisted `reviews` count so the interval ladder advances across
sessions) match the spec and are deterministic.

Purge is wired correctly and is purely `provenance = 'generated'`-scoped, so
seed and imported sets are never touched; it runs both on the Delete/Strike
`redact_ai_outputs` path and the `delete_meeting` cascade. In this build no
`generated` set is produced (D2), so the path is dormant but fully covered by
tests (live-strike, backup, cascade).

Offline + no-server invariants hold. No runtime image/AI network for a present
lesson; figures render through `convertFileSrc` (local asset protocol) with a
caption fallback on decode failure; the only network is the user-initiated URL
import with no default URL; no `dangerouslySetInnerHTML`, no hardcoded key/URL
in the Lessons path; the policy and offline guards pass.

The one gap: the seed authors numeric answer units (`mol/L`, `g/mol`, `%`) that
are not members of the grader's UNITS table, and §9's own rule returns a
present-but-unmapped unit as unparseable. The keys are chemically right and the
bare-number path grades correctly, but a learner who answers "0.1 mol/L" to a
molarity prompt is told "couldn't read that unit."

<details>
<summary>Issues (3)</summary>

1. **Ungradeable authored units** — [MEDIUM] ten numeric items/steps declare
   `mol/L`, `g/mol`, or `%`, none of which are in `grade::UNITS`; a learner
   answering with the stated unit is graded `Unparseable`. Either add these as
   dimension classes (molarity base `mol/L`, a dimensionless `%`, and the
   molar-mass compound `g/mol`) to the UNITS table, or strip a trailing
   known-but-dimensionless unit before comparison, and add a seed-validation
   assertion that every authored numeric `unit` is parseable so authored keys
   can never out-run the grader again.
2. **Seed answer keys only structurally validated, not recomputed** — [NIT]
   §4.6 claims seed validation recomputes *every* numeric/worked-example key,
   but `validate_item`/`validate_worked_example` only check finiteness; the test
   recomputes balance items (via `verify_balance`) and spot-checks one numeric
   item. The authored keys I hand-verified are all correct, but there is no
   mechanical guard against a future wrong numeric key. Consider recomputing
   numeric keys from the shared molar-mass table in the seed test.
3. **Exam gate counts progress rows, not current-set lessons** — [NIT]
   `lessons_start_exam` compares `COUNT(checkpoint_passed=1)` to
   `set.lessons.len()`; a stale `lesson_progress` row for a lesson id no longer
   in the set could inflate the count. Harmless given stable in-bundle ids, but
   joining the count to the set's current lesson ids would be tighter.

</details>

<details>
<summary>Details</summary>

### The authored-unit / grader-table mismatch (the one blocking item)

`grade::UNITS` carries mass, volume, amount, pressure and temperature members.
It does **not** carry molarity (`mol/L`, `M`), molar mass (`g/mol`), or percent
(`%`). §9's grading rule for a present-but-unmapped unit is explicit and correct
as a defense: "returned as unparseable with 'couldn't read that unit', never
compared as a bare magnitude." The problem is that the shipped seed then
*authors* those very units as the declared answer unit of real items.

Counts across the seed (exam + checkpoints + steps): `mol/L` ×4, `g/mol` ×2,
`%` ×4 at the item level, plus `%` ×2 and (via the chained percent/STP legs)
several more at the step level. Grading behavior, confirmed by exercising
`check_numeric` directly:

```
check_numeric("0.1",        0.1,   Some("mol/L"), rel 0.01) => Correct
check_numeric("0.1 mol/L",  0.1,   Some("mol/L"), rel 0.01) => Unparseable("couldn't read that unit")
check_numeric("44.01 g/mol",44.01, Some("g/mol"), rel 0.01) => Unparseable("couldn't read that unit")
check_numeric("80 %",       80.0,  Some("%"),     rel 0.01) => Unparseable("couldn't read that unit")
```

The bare-number path works because `(None, _) => raw_value`. The UI placeholder
("value in {unit}") nudges learners toward entering just the number, which
mitigates — but does not close — this: a chem student writing "0.1 mol/L" or
"80 %" on an exam-grade product is answering correctly and being told the app
can't read the unit. The answer keys themselves are chemically correct (molarity
`moles/L`, dilution `M1V1=M2V2`, percent yield), so this is a grader/content
consistency bug, not a wrong-key bug. Fix at the grader (add the three unit
classes — `%` and `g/mol` as scale-1 members of their own dimension, `mol/L`/`M`
as a molarity class) and add a seed assertion that every authored `unit` round-
trips through `lookup_unit`, so authored content can never again declare a unit
the authoritative grader rejects.

### Chemistry spot-check (keys are correct)

Molar masses match the shared 2-dp table (H2O 18.02, CO2 44.01, NH3 17.03, O2
32.00, H2 2.02, CH4 16.04). Representative keys I verified by hand:

```
36.04 g H2O / 18.02      = 2.00 mol              (ex-num-0)      ok
4.00 g H2  → 1.980 mol → 2:2 → ×18.02 = 35.68 g  (ex-num-6)      ok
2.02 g H2  → 1.00 mol  → ×18.02 = 18.02 g        (ex-num-11)     ok
2.00 mol × 22.41 L/mol  = 44.82 L  (STP)         (ex-num-15)     ok
1·0.0821·273.15         = 22.426 L               (ex-num-17)     ok
```

Every `balance` equation balances and the authored coefficients are the lowest-
integer set (re-verified by `verify_balance` at import):

```
2 H2 + O2 → 2 H2O         [2,1,2]
CH4 + 2 O2 → CO2 + 2 H2O  [1,2,1,2]
N2 + 3 H2 → 2 NH3         [1,3,2]
C3H8 + 5 O2 → 3 CO2 + 4 H2O [1,5,3,4]
```

All twelve multistep exam items chain correctly (balance → ratio → mass/volume/
percent); e.g. ex-ms-10 (propane): 1 mol C3H8 → 3 mol CO2 × 44.01 = 132.03 g, ratio
1:3; ex-ms-6: 2 mol N2 → 4 mol NH3 × 17.03 = 68.12 g theoretical, 54.50/68.12 =
80.0% yield. No wrong answer key found.

### Integrity and tamper rejection

`verify_integrity` enforces the format tag, schema major, every per-file SHA-256,
the exact file-set match (missing or extra files both fail), and safe entry
paths. `validate_bundle` then re-hashes each figure against `manifest.figures[].sha256`
(which also feeds the content fingerprint) and decodes every PNG to its declared
width×height, so a byte-tampered, hash-skewed, or non-decoding figure fails the
whole bundle. The recomputed content fingerprint is authoritative and must equal
a non-empty declared value. Commit is a single transaction with temp-dir staging
and staging→final rename after the DB row, rolled back on any failure. The seed
loads through this exact path (no privileged loader), so a bad seed is a build
break — `shipped_seed_bundle_passes_validation_with_all_floors` enforces it.

### Grader robustness

`parse_value_unit` handles `e`/`×10^`/`x 10^` spellings and rejects non-finite /
unreadable input without panicking (`malformed_schema_fails_cleanly_never_panics`,
`numeric_grading_table`). Unit conversion goes given→base→item-unit with the one
additive `°C→K` offset handled explicitly. Dimension mismatch → Wrong (not
unparseable), which is the correct fail-closed behavior. `check_balance`
normalizes the learner's coefficients by GCD before comparing, so any correct
multiple (`[4,2,4]` for `[2,1,2]`) is accepted while import-time `verify_balance`
still insists authored keys be the reduced set.

### Scoring, gating, mastery, review

Checkpoint: denominator floored at `presentCount`, best-score + sticky-pass
persisted, review ladder advanced only on a pass. Exam: gated on all checkpoints,
30-item draw persisted and scored server-side over the stored selection with
blanks → zero, multistep graded per step (total possible > item count), 80% pass,
certificate issued on pass with a local SHA-256 `result_hash` over
(fingerprint,score,issuedAt,setId). Mastery EMA and SM-2-lite constants match
§7/§8.1 and persist the review count so intervals advance across DB round-trips
(`review_count_persists_...`). Adaptive trainer focuses the weakest unmastered
concept and scales tier by score; generator items carry closed-form answers from
the same molar-mass table, so a drawn drill's key is correct by construction.

### Invariants held

No runtime AI/image network for a present lesson; figures via local asset
protocol with caption fallback; only network is the user-initiated URL import
(no default URL); no model-text-as-HTML (`dangerouslySetInnerHTML` absent from
the Lessons path); no hardcoded key/URL in `lessons`; `check-ai-provider-policy.py`
PASS and `check-lessons-offline.mjs` PASS; both Mac flavors build; purge is
`generated`-only so seed/imported are exempt and is wired on both the Strike/Delete
and delete-cascade paths; each of the six seed lessons embeds ≥1 decoding PNG
figure (D4) with a non-empty `reviewedBy`.

</details>

<details>
<summary>File map</summary>

- `src-tauri/src/lessons.rs` — module root, schema major/minor, DB schema for
  catalog + learner records, purge re-export.
- `src-tauri/src/lessons/bundle.rs` — zip read/write, integrity, fingerprint,
  validate/commit/import/export.
- `src-tauri/src/lessons/schema.rs` — data model + strict validation/cleaning,
  item/explainer/exam floors, concept DAG.
- `src-tauri/src/lessons/grade.rs` — authoritative grader (UNITS table, numeric,
  balance, mc, multistep). *Site of the unit-table gap.*
- `src-tauri/src/lessons/explain.rs` — formula parse, integer-LA balancer,
  closed-form/structural explainer verification.
- `src-tauri/src/lessons/exam.rs` — selection, scoring, readiness, cert hash.
- `src-tauri/src/lessons/mastery.rs`, `review.rs`, `trainer.rs`, `generators.rs`
  — mastery EMA, SM-2-lite, adaptive selection, seeded generators.
- `src-tauri/src/lessons/purge.rs` — generated-only Delete/Strike purge.
- `src-tauri/src/lessons/seed.rs` — first-run seeding, tombstone, idempotence.
- `src-tauri/src/lessons/commands.rs` — Tauri commands (checkpoint/exam/trainer/
  diagnostic/dashboard/cert).
- `src-tauri/src/lessons/tests.rs` — 28 tests incl. shipped-seed validation.
- `src-tauri/resources/seed/stoichiometry.lesson` — the 6-lesson seed bundle
  (6 decoding PNG figures). Chemistry verified correct.
- `src/components/lessons/**`, `src/lib/lessons.ts`, `src/lib/lessonLogic.ts` —
  frontend render + non-authoritative preview mirror.

Full diff: `git diff main` in `.worktrees/lessons-stoichiometry`.

</details>
