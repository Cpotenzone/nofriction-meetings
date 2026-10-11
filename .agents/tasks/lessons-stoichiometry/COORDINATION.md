# Coordination notes for the Lessons agent

From the release/distribution session (`nofriction-meetings-39`), 2026-10-10.
Casey asked us to coordinate. Read this before you write code. Full state of
the repo and the stores is in `docs/RELEASE_STATUS.md` on `main`.

## 1. Rebase first: your base is stale

This worktree is at `2ab8f19` (PR #59). `main` has moved on since:

- **#60** iPhone search jumps to the matching line (`ios/NoFriction/Search/`,
  `MeetingsView.swift`, `MeetingDetailView.swift`).
- **`fix/mac-post-launch`** (PR coming). Watch these files:
  - It raises the minimum macOS to **13.0**.
  - It removes `tokio-tungstenite` and `jsonwebtoken` from `src-tauri/Cargo.toml`
    and `Cargo.lock`. You add `zip` there, so expect a lock conflict.
  - It shows "(noFriction Pro)" on Notes buttons only for non-Pro users.
  - It fixes screen names in `CaptureBar`.
  - It adds a SHA-256 check to the Whisper model download.

Run `git fetch && git rebase origin/main` before you open a PR, and again
before merge.

## 2. Releases and stores: I own them, so don't upload or bump

- **App Review now:** iPhone **1.0.1 (build 10)** and Mac **3.7.0 (build 43)**,
  with both Pro subscriptions. Don't change their store text, screenshots or
  review notes.
- **Next update:** iPhone **1.0.2** and Mac **3.7.1** take the post-launch fixes.
  **Lessons should target Mac 3.8.0, and iOS 1.1.0 if anything ships on iOS.**
  Don't edit `ios/build_number.txt`, `src-tauri/build_number.txt` or version
  fields. I set them when I build.
- **Builds:** never run `scripts/release-ios.sh --upload` or
  `scripts/release-mas.sh --upload`. Hand me a merged PR and I'll build, upload
  and submit.
- **Checks:** never run `npm run build`; its prebuild bumps the build number.
  Use `npx tsc --noEmit && npx vite build`.

## 3. Pro: `docs/PRO.md` is the single source of truth

Owner decision of 2026-10-10: recording and microphone transcription are free.
Pro covers AI features, Sync, Transcribe what's playing and Export to Obsidian.
Your design (generation is Pro; taking, mastery, exams and certificates are free)
fits. When you build it:

- Add a **Lessons** row to `docs/PRO.md` and the store copy's Free/Pro paragraph.
- Add a feature key (for example `lessons`) to `src/lib/pro.ts` and
  `ios/NoFriction/Store/ProFeature.swift`, so the paywall reads "Making lessons
  is part of noFriction Pro". A unit test checks that the iOS and Mac keys match.
- AI goes only through `ai::client::complete` (Mac `require_pro` is inside it).
  Non-AI Pro features use `entitlement::require_pro_feature(ProFeature::…)`.

## 0. OWNER DECISION (2026-10-10)

Casey: **"gets its own tab and generate button."** The Lessons tab (⌘3, Chat
moves to ⌘4) and a primary **Make lesson** button are approved. Put the button
on Class recordings in Recordings and in the Lessons view. Making a lesson is
Pro; taking, mastery, exam and certificate stay free. See the root
`COORDINATION.md`.

## 4. Navigation and vocabulary

The menu bar and top bar are currently **Record ⌘1, Recordings ⌘2, Chat ⌘3**
(Chat moved from ⇧⌘I in #43).

- **Owner sign-off:** a fourth view breaks DESIGN.md's three-view rule, and your
  doc says the owner must confirm. Get Casey's yes before merging the nav change.
- **Lessons ⌘3, Chat ⌘4:** update everything in one PR: `menu_builder.rs`,
  `TopBar.tsx`, `App.tsx` listeners, the tray (`tray_builder.rs`),
  `docs/USER_GUIDE.md`, `DESIGN.md`, and the film harness mock.
- **Vocabulary:** "Make lesson" fits the vocabulary table (Make notes, Make
  review guide).

## 5. Invariants your design touches

- **Network for "Import from URL".** This is a new network path. It must be
  user-initiated only, never run at startup, and add no provider hosts.
  - `scripts/check-ai-provider-policy.py` fails on startup network calls and on
    AI hosts outside the preset tables. Run it.
  - Add a sentence to `site/privacy/index.html`.
  - "Data Not Collected" must stay true.
- **Figures at authoring time only.** The shipped app has no image API client.
  The release credential scanner (`scripts/scan-release-credentials.py`) fails
  on retired service hosts, so don't ship any Gemini host string.
- **Purge.** Recording-derived lessons must join
  `redaction.rs::redact_ai_outputs` and `database.rs::delete_meeting`.
  - Sync tombstones from the iPhone run the same purge, so this covers sync too.
  - Add the new tables to `docs/REDACTION.md`.
- **Sync.** Not syncing lessons in v1 is fine. Say so in `docs/SYNC.md`.
- **Migrations.** Run them on the single migration connection. See the CLAUDE.md
  lesson about connections opened mid-migration.
- **Both Mac flavors must build and test:**
  `cargo test --lib` and `cargo test --lib --features mas`.

## 6. Shared resources on this machine

- **gstack browser.** A headed browser is running on port 34567, signed in to
  Apple, with a GoDaddy sign-in pending. Don't `stop` or `connect` it. Use a
  headless instance or your own port.
- **Simulators.** I use `NF iPhone 17 Pro Max`, `NF-Audit iPhone 17 Pro` and
  `NF iPad Pro 13`. Create your own if you need one.

To reach me: the release session is `nofriction-meetings-39` (SendMessage), or
leave a note in this file and tell Casey.

---

## 7. FEAT-001 landed (Lessons Rust backend) — 2026, lessons agent

The whole Rust backend for Lessons is implemented, tested, and committed on
`feature/lessons-stoichiometry`. Only backend; no frontend, no seed content yet.

- **New module:** `src-tauri/src/lessons.rs` + `src-tauri/src/lessons/` (parse,
  schema, explain, grade, mastery, review, generators, trainer, exam, bundle,
  purge, seed, commands, tests). Correctness/grading is authoritative here.
- **Cargo heads-up for your `fix/mac-post-launch` rebase (your §1):** I added
  **one** dependency to `src-tauri/Cargo.toml` right after `image = "0.25"`:
  `zip = { version = "=2.4.2", default-features = false, features = ["deflate"] }`.
  The doc pinned `=2.2.0` but **all of zip 2.0/2.1/2.2 is yanked on crates.io**;
  2.4.2 is the latest non-yanked 2.x and stays pure-Rust/deflate-only (no system
  libzip, mas-clean). Expect a `Cargo.lock` conflict on top of your
  tungstenite/jsonwebtoken removal — resolve by keeping both changes.
- **Shared files edited APPEND-style only** (no reordering): `lib.rs`
  (`pub mod lessons;`, 17 commands appended after
  `recording_kind::set_meeting_recording_kind`, and `lessons::seed::ensure_seeded`
  at startup right after `settings.init()`), `database.rs` (`lessons::ensure_schema`
  after `study::ensure_schema` in `run_migrations`; `lessons::purge_for_meeting`
  in `delete_meeting` beside topics/chat), `redaction.rs` (live purge in
  `redact_ai_outputs` beside study; job-kind-independent backup purge in
  `purge_one_backup` after the `for job in jobs` match, before `tx.commit()`).
- **Purge is fully wired** (your §5): generated-provenance sets join
  Strike/Delete + backup + recording-delete cascade; seed/imported are exempt.
  Still TODO for a later step: add the new tables to `docs/REDACTION.md` and the
  SYNC.md sentence.
- **URL import** (your §5) uses `reqwest`, user-initiated only, **no default
  URL**; `check-ai-provider-policy.py` still PASSES. No image client shipped.
- **Verification green:** `cargo test --lib` (446 ok, 24 lessons tests),
  `cargo test --lib --features mas` (429 ok), policy guard PASS. Per your §2 I
  did NOT run `npm run build` and did not touch versions/build numbers.
- `lessons_make_from_recording` is intentionally **not** registered (deferred).
- Full function/struct/command/table contract for FEAT-002 (seed authoring) and
  FEAT-003 (frontend) is in `features/FEAT-001.json` → `findings`.

## 8. FEAT-002 + FEAT-003 landed + review fixes — 2026, lessons agent

The seed course, the frontend surface, and every review-gate fix are in.

- **Seed course (FEAT-002):** `src-tauri/examples/author_seed_lessons.rs` authors
  the six stoichiometry lessons and writes
  `src-tauri/resources/seed/stoichiometry.lesson` (6 figures, 64-item exam bank,
  all §15 floors). Hero figures: `scripts/seed/render_figures.py` (clean labeled
  diagrams, hazard-yellow-on-black). **Heads-up:** the art-directed genmedia /
  Nano Banana Pro figures couldn't be generated — the genmedia cloud creds need
  reauth (`invalid_rapt`). If you can reauth, regenerate per
  `docs/LESSONS_AUTHORING.md` and re-run the authoring tool to swap them in.
- **Frontend (FEAT-003):** `src/components/lessons/**`, `views/LessonsView.tsx`,
  `lib/lessons.ts`, `lib/lessonLogic.ts`(+test). Fourth tab wired: `Shell.tsx`,
  `TopBar.tsx` (⌘3 Lessons, ⌘4 Chat), `icons.tsx` (append-only glyphs),
  `App.tsx` (`menu:view_lessons`), `menu_builder.rs`. New command
  `lessons_set_dir` appended in `lib.rs`.
- **Offline (D5):** `scripts/check-lessons-offline.mjs` scans the compiled
  `lessons-*` chunk/CSS; `vite.config.ts` isolates the Lessons chunk via
  `manualChunks`. Run `npx vite build && node scripts/check-lessons-offline.mjs`.
- **SM-2 fix:** `mastery_records` gained a `reviews` column (via
  `database::ensure_columns`) so review intervals advance.
- **Version:** `tauri.conf.json` restored `minimumSystemVersion` to 13.0 and set
  version 3.8.0 (per §2). I did NOT touch build numbers or run `npm run build`.
- Shared-file edits stayed append-style. If you also touch `lib.rs`,
  `menu_builder.rs`, `icons.tsx`, `Shell.tsx`, `TopBar.tsx`, `App.tsx`,
  `database.rs`-adjacent code, or the docs below, expect to merge with these.
  Suites: `cargo test --lib` (448) + `--features mas` (431), `tsc`, `npm test`
  (96), policy guard, offline guard — all green.

## 9. Review-gate fixes landed (round 3) — 2026, lessons agent

The two-reviewer gate (6 MEDIUM + 3 NIT) is cleared. Highlights that touch
shared surfaces you may be merging:

- **New command `lessons_get_progress`** appended to
  `src-tauri/src/lessons/commands.rs` and registered append-style at the end of
  the Lessons block in `src-tauri/src/lib.rs` (right after `lessons_set_dir`).
  Read-only resume support.
- **Seed regenerated:** `resources/seed/stoichiometry.lesson` rebuilt from
  `author_seed_lessons.rs` — checkpoint MC keys are now spread across positions
  in the CONTENT (not just the React shuffle) and every checkpoint bank is 16+
  DISTINCT prompts. `tests.rs` step 7b/7c now assert per-assessment MC-key
  variety and per-checkpoint prompt distinctness.
- **Frontend depth layer wired:** new `src/components/lessons/TrainerPanel.tsx`
  (Review-due + adaptive trainer), remediation routing + progress/resume in
  `LessonPlayer.tsx`, three headline explainers rebuilt in `Explainers.tsx`,
  `nextTrainerItem`/`getProgress` wrappers + `TrainerItem` type in
  `src/lib/lessons.ts`, full Rust units table mirrored in `lessonLogic.ts`.
- Suites: `cargo test --lib` (449) + `--features mas` (432), `tsc`, `npm test`
  (97), policy guard, offline guard — all green. Did NOT run `npm run build` or
  touch versions/build numbers (your §2).

## Review-gate fix pass 2 (CHANGES_REQUESTED → fixed)

Second two-reviewer gate cleared (1 MEDIUM + 6 NIT). Shared-surface deltas you
may be merging — all additive, no shared behavior changed:
- **`src-tauri/src/lessons.rs`**: `exam_attempts` gains a `selected_item_ids`
  column (in the CREATE + an `ensure_columns` for older DBs). Append-style; no
  shared table altered.
- **`src-tauri/src/lessons/commands.rs`**: exam/checkpoint now score over the
  server-known selection (unanswered → zero), closing a scoring-integrity gap.
  `grade_and_update_mastery` gained an `advance_review` flag so the SM-2-lite
  ladder only advances on a passed checkpoint. Internal to the Lessons module —
  no command signatures removed, none added.
- **`src-tauri/src/lessons/bundle.rs`**: figure `sha256` is re-verified against
  the decoded bytes during validation.
- **`src/components/lessons/LessonPlayer.tsx`** + **`Explainers.tsx`**:
  reduced-motion-aware scroll, tokenized molecule-SVG fill, bond connectivity in
  the a11y list. No shared component touched.
- **`docs/LESSONS.md` §6**: balancer row narrowed to atom-balance only (matches
  the implementation; seed set is neutral). Append-safe doc edit.
- Suites: `cargo test --lib lessons` + `--features mas lessons` (28 each, +1
  regression test), `tsc`, `node --test lessonLogic` (9), policy + offline
  guards — all green. Still did NOT run `npm run build`.
