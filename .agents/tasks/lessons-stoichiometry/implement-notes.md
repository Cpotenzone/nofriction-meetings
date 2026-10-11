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

## Not done (and why)
- **Pro feature key `lessons`** (COORDINATION §3): deferred with the generation
  pipeline (D2). Adding a half-wired key now would risk the cross-platform
  key-match test; documented in PRO.md instead.
- **Art-directed genmedia figures**: blocked on genmedia cloud reauth
  (`invalid_rapt`) — an owner-only action. Shipped deterministic figures are
  correct and reviewed; the genmedia versions are a documented drop-in.
