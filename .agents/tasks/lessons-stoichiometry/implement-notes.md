# Lessons subsystem — review-fix implementation notes

Fixes for the consolidated review gate (`review.json`, verdict
CHANGES_REQUESTED: 5 HIGH, 4 MEDIUM, 1 NIT). Every blocking finding is addressed
below. Suites re-run green: `cargo test --lib` (448) and `--features mas` (431),
`npx tsc --noEmit`, `npm test` (96, incl. 8 new lessonLogic tests),
`scripts/check-ai-provider-policy.py` PASS, and the new Lessons offline guard
PASS.

## HIGH-1 — Six-lesson seed `.lesson` bundle + figures (D4)
- Added `src-tauri/examples/author_seed_lessons.rs`: authors the six
  chemically-correct stoichiometry lessons in Rust, embeds one hero figure PNG
  per lesson, builds the `.lesson` zip (hashed manifest + content fingerprint),
  validates through the ordinary import path (`bundle::validate_bytes`), and
  writes `src-tauri/resources/seed/stoichiometry.lesson`. All answer keys are
  recomputed/verified by the same Rust that grades at runtime (balancer, numeric
  grader, generators), so no wrong "correct" answer can ship.
- Content: 6 lessons, each with ≥1 figure explainer + interactive explainers
  (the union covers all 7 interactive kinds), 3 worked examples, 2 misconception
  cards, a trainer, and an 8-item checkpoint over a ≥16-item bank; Lesson 2's
  checkpoint carries ≥1 `balance`-kind item (Finding 4). Exam bank = 64 items
  (24 mc / 24 numeric via templates+variants / 12 multistep), mix 12/12/0/6=30,
  ≥2 items per concept. Diagnostic included.
- Figures: `scripts/seed/figures.manifest.json` records the art direction +
  per-figure prompts; `scripts/seed/render_figures.py` renders the six hero
  figures as clean, correctly-labeled scientific diagrams (hazard yellow on
  matte black). These are the shipped figures. **The art-directed genmedia
  (Nano Banana Pro / gemini-3-pro-image) versions could not be generated in this
  environment because the genmedia cloud credentials require re-authentication
  (`invalid_rapt`).** They are a drop-in enhancement: regenerate per
  `docs/LESSONS_AUTHORING.md` once genmedia access is restored and re-run the
  authoring tool. The build never ships a figure-free bundle (D4) — the shipped
  deterministic figures satisfy the floor, decode, and the mechanical review
  gate.

## HIGH-2 — Seed-validation regression test
- `src-tauri/src/lessons/tests.rs::shipped_seed_bundle_passes_validation_with_all_floors`
  `include_bytes!`-loads the shipped bundle and asserts: validates with zero
  errors; 6 lessons; the D4 figure floor (every lesson ≥1 figure explainer whose
  PNG decoded) + non-empty `reviewedBy`; FR-EXP union of all 7 interactive kinds
  (NIT-7); checkpoint ≥2× presentCount with ≥2 items/concept; the Lesson-2
  balance-kind floor (Finding 4); exam ≥60 with ≥18/≥18/≥9 per kind, mix sums to
  30, mix.balance==0, ≥2 items/concept; and a chemistry spot-check (36.04 g H2O
  = 2.00 mol).

## HIGH-3/4 — Frontend Lessons surface + fourth tab (D1)
- New React surface under `src/components/lessons/`: `LessonLibrary`,
  `CourseView` (dashboard + exam + certificate), `LessonPlayer`, `QuizRunner`,
  `Explainers` (one renderer per §6 kind, SVG/interactive, a11y text-equivalent,
  reduced-motion), `GeneratePreview` (honest coming-soon stub, no AI call, D2),
  and `Lessons.css`. `src/components/views/LessonsView.tsx` routes
  library/course/generate. `src/lib/lessons.ts` wraps the `lessons_*` commands;
  `src/lib/lessonLogic.ts` is the PREVIEW-ONLY mirror (never authoritative) with
  `lessonLogic.test.ts`.
- Fourth tab wired: `Shell.tsx` AppMode gains `"lessons"` + renders it (and the
  stale 3-view header comment updated to four views — Finding 7/3); `TopBar.tsx`
  Lessons at ⌘3, Chat → ⌘4, with `GraduationCapIcon`; `icons.tsx` gains the ~10
  append-only Lessons glyphs; `App.tsx` listens for `menu:view_lessons`;
  `menu_builder.rs` adds the Lessons menu item at ⌘3, moves Chat to ⌘4, maps the
  event, and includes it in `shows_window` + the test.
- New backend command `lessons_set_dir` (registered in `lib.rs`) returns the
  catalog dir so the frontend builds LOCAL Tauri asset URLs for figures.

## HIGH-5 — D5 frontend offline guarantee
- `scripts/check-lessons-offline.mjs`: authoritative guard that scans the
  COMPILED `lessons-*.js` chunk + `lessons-*.css` for any non-local
  `http(s)://`/`url(...)` and fails on a match; also asserts no remote
  `@import`/`@font-face` and that every `font-family` resolves to the bundled
  Inter / system stack. Verified it FAILS on a planted remote ref and PASSES
  after restore.
- `vite.config.ts`: `manualChunks` isolates the Lessons surface into its own
  chunk/CSS so the guard scans only Lessons bytes (not the whole app's URLs).
- CSP backstop: `tauri.conf.json`'s CSP already restricts `connect-src 'self'`,
  `img-src` to local (`'self' data: blob: asset: http://asset.localhost`), and
  `font-src 'self' data:` — no remote origin. Documented as the Lessons runtime
  backstop. `Lessons.css` uses the bundled Inter/system font stack only.

## MEDIUM-6 — SM-2-lite review count persistence
- Added a `reviews` column to `mastery_records` (CREATE + `ensure_columns` for
  older DBs), a `reviews` field on `mastery::Mastery`, and load/store of it in
  `commands.rs`. The two grade paths now seed `ReviewState.reviews` from the
  persisted value and write it back, so the interval walks 1 → 3 →
  interval·ease instead of resetting to 1 day. `due_at` is now scheduled from
  the interval so the Review-due surface works. New DB round-trip test
  `review_count_persists_so_interval_advances_across_db_round_trips`.

## MEDIUM-7/8 — Docs + version
- `docs/CHANGELOG.md`: Lessons entry under [Unreleased]. (Note: the repo's
  changelog is `docs/CHANGELOG.md`; review-b looked for a root `CHANGELOG.md`.)
- `docs/REDACTION.md`: generated-lesson purge entry (provenance='generated'
  guard, resolve→delete-rows→remove-dirs ordering, job-kind-independent backup
  hook, seed/imported exempt, dormant-but-wired).
- `DESIGN.md`: nav table expanded to four views with the D1 justification;
  vocabulary rows for lesson/lesson set/Make lesson/Generate/Import with the
  Generate-vs-Make-notes scoping note.
- `docs/ARCHITECTURE.md`: Lessons backend module row + four-view TopBar + the
  LessonsView frontend description.
- `docs/USER_GUIDE.md`: a Lessons section + the NFR-4 QA appendix.
- `docs/PRO.md`: a Lessons row (taking is free; the deferred Generate pipeline
  will be Pro). `docs/SYNC.md`: lessons not synced in v1. `site/privacy`: the
  single user-initiated Import-from-URL fetch. `README.md`: Mac 3.8.0 / 13.0+.
- `docs/LESSONS_AUTHORING.md` (new): authoring procedure, figure manifest, the
  two-part accuracy gate, and the figure review checklist.

## MEDIUM-9 — Version / minimum-OS regression
- `tauri.conf.json`: `minimumSystemVersion` restored to `13.0` (was regressed to
  12.3); `version` set to `3.8.0` (the Lessons target per COORDINATION §2, not a
  downgrade). README updated to match. A full `git rebase` was not performed
  (would be a large, risky history rewrite in the loop); the shippable
  regression the review flagged — the version/OS downgrade in the diff — is
  corrected directly. The unrelated marketing/site/local_stt drift is pre-existing
  branch content, not introduced here.

## NIT-10 — content_fingerprint comparison
- `bundle::validate_bundle` now compares the recomputed content fingerprint
  against `manifest.contentFingerprint` and rejects on mismatch (empty declared
  fingerprint tolerated). Added `bundle::fingerprint_for` so seed authoring and
  the test fixture write a matching fingerprint.

## Review round 2 (review.json CHANGES_REQUESTED — assessment integrity + NITs)

Fixes for the consolidated review (2 HIGH, 2 MEDIUM UX/offline, 1 MEDIUM content,
+ NITs). Suites re-run green: `cargo test --lib` (449) and `--features mas`
(432), `npx tsc --noEmit`, `npm test` (96), `check-ai-provider-policy.py` PASS,
`npx vite build` + `scripts/check-lessons-offline.mjs` PASS.

### HIGH — multistep exam items were ungradeable from the UI
- `src/components/lessons/QuizRunner.tsx`: added a real `MultistepInput` that
  renders each step with the right control (mc / numeric / balance) and
  populates `SubmittedAnswer.multistep`. Previously multistep rendered a static
  placeholder and submitted no step answers, so the 6 multistep exam points
  (20%) always scored 0, capping a flawless MC+numeric learner at exactly 80%.
  (The Rust `to_given`/`grade_multistep` already supported multistep; the gap
  was purely that the shared runner never collected the answers.)
- New Rust regression `seed_exam_is_passable_by_a_perfect_learner_and_multistep_counts`
  (tests.rs): a perfect learner scores 100% and passes; leaving multistep blank
  costs points; the bank's total possible exceeds its item count (multistep is
  graded per step). Proves the exam is now passable with margin.

### HIGH — all MC keys at index 0 + no choice shuffle (trivially gameable)
- `QuizRunner.tsx`: MC choices are now shuffled at presentation (deterministic
  per-item Fisher–Yates seeded from the item id, stable across re-renders) and
  the chosen screen position is mapped back to the AUTHORED index before submit.
  Shared `ChoiceGroup` used by both standalone MC items and multistep MC steps.
  Always clicking the first choice no longer scores 100% on any bank.
- Defense-in-depth on the content side: `author_seed_lessons.rs` now distributes
  the authored exam MC keys across positions via a pure `rotate_choices` helper
  (the correct text is unchanged, only its index moves). Seed regenerated.
- New seed assertion (tests.rs §7b): the union of MC answer keys across exam +
  checkpoints + diagnostic must span more than one position.

### MEDIUM — all 12 multistep exam items were byte-identical
- `author_seed_lessons.rs::build_exam_items`: replaced the 12 identical
  CH4-combustion items with 12 DISTINCT multistep items spanning the mole-ratio,
  limiting-reagent / percent-yield, and gas-law concept chains (ammonia
  synthesis, hydrogen combustion, limiting reagent by mol and by grams, percent
  yield, gas volume at STP, PV=nRT, propane combustion, O2-in-grams limiting).
  Numbers computed from the shared molar-mass table so the authoring recompute
  gate verifies them. New seed assertion (§7c): multistep prompts are distinct.

### MEDIUM — offline guard (D5) was never invoked
- `package.json`: added `check:lessons-offline` and a `postbuild` hook. npm runs
  `postbuild` automatically after `build`, and `tauri.conf.json`'s
  `beforeBuildCommand` is `npm run build`, so the guard now runs on every
  `npm run build` / `tauri build` — including the release-macos CI job — and
  fails the build on a remote reference. (Verified PASS on the compiled bundle.)

### MEDIUM — no visible focus ring on interactive explainer inputs
- `Lessons.css`: added a `:focus-visible` 2px hazard-yellow outline (reusing the
  `.lessons-btn` pattern) to `.explainer input[type="number"]` and
  `.explainer input[type="range"]`, covering the balancer number field and both
  sliders (WCAG 2.4.7).

### NITs
- `grade.rs` `check_numeric` `exact` branch: replaced the ~4-ULP machine epsilon
  with a small relative epsilon (`1e-9*|answer|`, abs floor `1e-9`) so a
  correctly-rounded `exact`-keyed value is accepted for imported bundles.
- `schema.rs` `validate_exam`: now asserts the per-kind bank floors
  (`MIN_EXAM_MC`=18, `MIN_EXAM_NUMERIC`=18, `MIN_EXAM_MULTISTEP`=9) and
  per-concept exam coverage (>=2 items), so an imported bundle cannot
  under-provision.
- `generators.rs`: molar-mass table aligned to the SAME 2-dp values the seed
  authoring tool uses (2.02 / 18.02 / 44.01 …) and STP molar volume unified to a
  shared `MOLAR_VOLUME_STP = 22.41` constant — one table across generators and
  authored content.
- Worked-example recompute (NIT): worked examples are prose-authored with no
  machine-parseable structural inputs, so instead of claiming a recompute that
  the code does not do, `docs/LESSONS.md` §6.1 was softened to state worked
  examples are structure-validated (non-empty prompt, >=1 step, finite final
  answer) and their numbers verified at authoring time + the §12.2 human gate.
- Status glyphs: `CourseView.tsx` mastered marker now uses `CheckIcon`
  (already aria-hidden) plus a scoped `.lessons-sr-only` "mastered" label;
  `LessonPlayer.tsx` MisconceptionCard `✗` wrapped in an `aria-hidden` span (the
  claim text carries the meaning).
- Stale header comments updated to four views in `src/App.tsx` and
  `src-tauri/src/menu_builder.rs`.
- `README.md`: added a Lessons row to the capability matrix (append-style).

## Review round 3 (review.json CHANGES_REQUESTED — assessment integrity + frontend depth)

Fixes for the consolidated gate (6 MEDIUM, 3 NIT across review-a backend and
review-b frontend). Every blocking finding addressed; suites re-run green:
`cargo test --lib` (449) + `--features mas` (432), `npx tsc --noEmit`,
`npm test` (97, +1 units preview test), `check-ai-provider-policy.py` PASS,
`npx vite build` + `scripts/check-lessons-offline.mjs` PASS (Lessons chunk still
isolated: jsChunks 1 / cssFiles 1).

### MEDIUM-1 + MEDIUM-2 + finding-3/NIT-7 — checkpoint MC key integrity in the content
- `src-tauri/examples/author_seed_lessons.rs`: the anti-gameability property now
  lives in the platform-neutral CONTENT, not just the React shuffle. Added
  `mc_rot` (rotate a single inline MC item's correct answer to a target index,
  pure reordering) and rewrote `mc_filler` to (a) distribute each item's key
  across positions via the existing `rotate_choices` and (b) REQUIRE distinct
  prompts (asserts `templates.len() >= n`) instead of cycling 3 prompts ×4.
  Every inline checkpoint MC and the three diagnostic MC items are now rotated to
  varied indices; each checkpoint bank is 16+ DISTINCT prompts. Seed regenerated
  (`cargo run --example author_seed_lessons`).
- `src-tauri/src/lessons/tests.rs` step 7b now asserts MC-key variety PER
  ASSESSMENT (each lesson checkpoint, the exam, and the diagnostic individually
  — any assessment with ≥2 MC items must use ≥2 key positions), so a uniform-key
  checkpoint fails the build. Step 7c now also asserts every checkpoint bank has
  all-distinct prompts (not just the exam multistep bank).

### MEDIUM-3 — three headline explainers rebuilt to the §6 hero-interaction contract
- `src/components/lessons/Explainers.tsx`:
  - **MoleRatioExplainer** is now a genuine step-by-step reveal (set-up →
    conversion through moles → read result), one stage per click, with a reset;
    the final value is the authored Rust-verified `finalAnswer`.
  - **LimitingReagentExplainer** is now a SIMULATOR: a slider drives the reaction
    extent and each reactant bar drains in proportion to its coefficient; the bar
    that empties first is the limiting reagent (shown with a depleted-red fill).
  - **DimensionalAnalysisExplainer** is now an interactive railroad BUILDER:
    tiles are placed/removed one at a time (keyboard-operable buttons, not a
    mouse-only drag), the running product updates live, and each tile's
    denominator shows the cancellation strike-through.
- `src/components/lessons/Lessons.css`: added `.bar-fill.is-depleted` and the
  `.railroad*` builder styles (DESIGN.md tokens only).

### MEDIUM-4 — D3 §8 depth layer wired on the frontend
- New `src/components/lessons/TrainerPanel.tsx`: a Review-due surface
  (`lessons_review_due`, weakest-first with mastery bars) + an adaptive trainer
  session that pulls `lessons_next_trainer_item`, grades the answer (preview cue
  via `previewNumeric`), and records it via `lessons_submit_trainer_item`
  (authoritative mastery + SM-2 interval). Uses the previously-unused `TargetIcon`
  and `RouteIcon`.
- `src/lib/lessons.ts`: added `nextTrainerItem` wrapper + `TrainerItem` type
  (the registered `lessons_next_trainer_item` command had no wrapper).
- `CourseView.tsx` renders `<TrainerPanel>` after the mastery dashboard and a
  "Start over" section calling `resetProgress` (previously-unused wrapper), with
  a confirm.

### MEDIUM-5 — remediation routing after a failed checkpoint
- `src/components/lessons/LessonPlayer.tsx`: a sub-80% checkpoint now computes
  the MISSED concepts from the graded result (`missedConcepts`) and renders a
  remediation panel with one "Review: <concept>" button per missed concept that
  scrolls to that concept's explainer (via per-explainer refs). Plus a "Retake
  the checkpoint" control. The stale "handled by the parent" comment is replaced.

### MEDIUM-6 — progress persisted + resume from the UI
- `LessonPlayer.tsx` now calls `saveProgress` as explainers render (`onSeen`) and
  as worked examples are fully revealed (`onReveal`), and persists the checkpoint
  best/pass alongside. On open it reads saved progress and offers a "Resume where
  you left off" control positioned at `resumeExplainerIndex`.
- `ExplainerView` gained an `onSeen` callback (fired on mount per explainer id).
- New read command `lessons_get_progress` (appended to `commands.rs`, registered
  append-style in `lib.rs`) + `getProgress` wrapper so resume survives reopen.

### NIT-8 — TS preview units table mirrors the full Rust table
- `src/lib/lessonLogic.ts`: `UNITS` now mirrors `grade::UNITS` 1:1 (µg/ug, dL,
  cm³/cm3/cc, kPa, mmHg/torr, Pa, °K, gram/mole/liter aliases). Widened the unit
  token regex to allow digit/³ characters (cm3). New `lessonLogic.test.ts` case
  asserts none of the added units read as a false "unreadable".

### NIT-9 — hardcoded exam count + a11y answer spoilers
- `CourseView.tsx`: the exam button renders `set.exam.questionCount` instead of a
  literal 30.
- `Explainers.tsx`: the balancer text-equivalent only names the coefficients once
  the learner has reached a balanced state; the limiting-reagent sim only names
  the limiting species once the sim has been run to completion — neither
  pre-reveals the answer.

## Not done (and why)
- **Pro feature key `lessons`** (COORDINATION §3): deferred with the generation
  pipeline (D2). Adding a half-wired key now would risk the cross-platform
  key-match test; documented in PRO.md instead.
- **Art-directed genmedia figures**: blocked on genmedia cloud reauth
  (`invalid_rapt`) — an owner-only action. Shipped deterministic figures are
  correct and reviewed; the genmedia versions are a documented drop-in.

---

# Review-gate fix pass 2 (`review.json` → CHANGES_REQUESTED: 1 MEDIUM, 6 NIT)

Both reviewers' findings are addressed in code/content/docs. Suites re-run
green: `cargo test --lib lessons` and `--features mas lessons` (28 each, +1 new
regression test), `npx tsc --noEmit`, `node --test src/lib/lessonLogic.test.ts`
(9), `scripts/check-ai-provider-policy.py` PASS, `npx vite build` +
`scripts/check-lessons-offline.mjs` PASS.

## MEDIUM — Exam/checkpoint scored over client-submitted set (review-a)
Scoring is now over the **server-known** item set, with any un-submitted item
counting as zero — a short submission can no longer score 100%, pass a
checkpoint, unlock the exam, or earn a certificate.
- `lessons.rs`: `exam_attempts` gains a `selected_item_ids TEXT NOT NULL
  DEFAULT '[]'` column (added to the CREATE and via `ensure_columns` for older
  DBs). `lessons_start_exam` persists the 30 drawn ids there.
- `commands.rs` `lessons_submit_exam`: loads `selected_item_ids`, rebuilds the
  scored attempt over the **full selection** (missing/blank answers → default
  `Given`, which grades to zero), and grades mastery over that set. Rejects an
  attempt with no recorded selection.
- `commands.rs` `lessons_submit_checkpoint`: grades only submitted answers that
  map to the checkpoint bank, but holds the denominator to at least
  `presentCount` (one point per unanswered presented slot), so a one-item
  submission scores `1/presentCount`, not 100%.
- `commands.rs` `grade_and_update_mastery`: now iterates the full presented set
  (unanswered → zero) and takes an `advance_review` flag (see NIT below).
- Regression test `exam_scored_over_selected_set_not_posted_subset` in
  `tests.rs`: one correct answer over the full selection scores below
  `pass_pct` and never passes.

## NIT — SM-2-lite ladder advanced before checkpoint pass (review-a)
`grade_and_update_mastery` only advances the review ladder (`apply_review`,
`due_at`) when `advance_review` is true. `lessons_submit_checkpoint` passes it
only on a pass (LESSONS.md §8.1); the exam never advances the ladder;
`lessons_submit_trainer_item` still advances it directly (trainer/review
surface). A failing first checkpoint no longer writes `due_at`.

## NIT — balance_equation omits the charge row (review-a)
Narrowed the doc to match the implementation (the safe option the review
offered): LESSONS.md §6 table row 1 now reads "per-element atom counts (atom
balance only — the seed set is neutral species, so charge is not part of the
system)". No code change; the seed set is all neutral.

## NIT — figure sha256 not re-verified against bytes (review-a)
`bundle.rs` `validate_bundle`: the figure-decode loop now asserts
`sha256_hex(bytes) == figures[].sha256`, so the hash that feeds the content
fingerprint cannot be declared inconsistently with the bytes.

## NIT — smooth-scroll ignores prefers-reduced-motion (review-b)
`LessonPlayer.tsx` `scrollToExplainer`: gates on
`matchMedia("(prefers-reduced-motion: reduce)")` and uses `behavior: "auto"`
when reduced motion is requested.

## NIT — non-tokenized hex in molecule SVG (review-b)
`Explainers.tsx`: the atom-label `fill="#050505"` is now
`var(--bg-main, #050505)`.

## NIT — thin molecule-diagram text-equivalent (review-b)
`Explainers.tsx`: the a11y list now appends each bond as "single/double/triple
bond: A–B", giving screen-reader users connectivity, not just composition.
