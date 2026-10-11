# Lessons (interactive education) — Requirements

Status: draft for design review
Scope: noFriction Mac app (Tauri 2 / Rust + React), with explicit iOS parity plan
Related docs: `docs/STUDY_TOOLS.md` (spiritual predecessor), `docs/AI_PROVIDERS.md`,
`docs/REDACTION.md`, `docs/ARCHITECTURE.md`, `DESIGN.md`, `CLAUDE.md`

---

## 1. Summary

Lessons is noFriction's first push into interactive education. A Lesson is a
self-contained, offline, portable unit of study material — the "spiritual
successor" to a Class recording's Study guide, but far richer: it carries
deterministic interactive explainers (purpose-built React components driven by
structured data), AI-authored static figures (reviewed PNGs embedded in the
bundle), per-concept mastery tracking, adaptive trainers, checkpoint quizzes,
and a cumulative final exam that issues a shareable certificate on pass.

Lessons come from three sources, all supported by one architecture:

1. **Generate from a captured Class** — AI converts a Class recording's
   transcript + screenshots into an interactive lesson. This is the primary
   generation path built now. (Pro, uses network for AI.)
2. **Portable `.lesson` bundle** — a lesson is a shareable, importable file that
   runs 100% offline with zero runtime network. Generation/acquisition may use
   the network; a present bundle never does.
3. **Library you browse** — a *local catalog* of bundles the user acquires from
   a source they choose (file / folder / URL they paste). noFriction hosts
   nothing. The library ships non-empty: six authored stoichiometry lessons
   (AP Chem / college level) are seeded on first run.

The ship target is **Mac, fully**. iOS parity is planned and the Rust/data
layer is designed shared-friendly, but iOS interactive-visual richness ships as
a documented follow-up (see §10). We do not quietly make this Mac-only; the
iOS gap is stated explicitly.

Hard product line (from `CLAUDE.md`): no noFriction servers, backend, relay,
iCloud, Supabase, or Pinecone; all AI goes through `ai::client::complete`
(Mac) / `AIClient` (iOS) with consent + endpoint policy + Pro gates; figure
generation happens at **authoring time only** via the genmedia MCP image tool —
the shipped app never calls an image API at runtime; model output is **never**
rendered as HTML; recording-derived lessons join the Delete/Strike purge
pipeline.

---

## 2. Key terms

- **Lesson** — one unit (e.g. "Lesson 3: Mole ratios and stoichiometric
  calculations"). Contains explainers, interactive sessions, a checkpoint quiz,
  trainer drills, concept definitions, and figures.
- **Lesson set / course** — an ordered collection of lessons sharing a final
  exam and certificate (e.g. the six stoichiometry lessons).
- **`.lesson` bundle** — the portable, offline file (or folder) that packages a
  lesson set: all structured interactive content, concept graph, quiz/exam item
  banks, and embedded figure PNGs. No external references.
- **Explainer** — a deterministic, data-driven interactive React component
  (e.g. equation balancer). Accurate and testable; not model HTML.
- **Figure** — a static PNG authored with the best image model at authoring
  time, embedded in the bundle, reviewed, never fetched at runtime.
- **Trainer** — an adaptive drill that targets a learner's weak concepts.
- **Mastery** — a per-concept score the app tracks from quiz/trainer/exam
  performance.
- **Seed content** — the six authored stoichiometry lessons shipped in-app so
  the library is non-empty on first run. Not recording-derived (purge-exempt).
- **Library / catalog** — the local list of lesson sets the user has, with an
  import affordance.

---

## 3. Where Lessons lives (navigation)

DESIGN.md's navigation model is "three views (Record · Recordings · Chat),
nothing under a MORE menu; if it can't earn a place in the three views or
Settings, it is cut." Lessons is a new primary surface and does not fit any of
the three existing views cleanly.

**FR-NAV-1.** Lessons is a **first-class top-level view**, making the top bar
**Record · Recordings · Lessons · Chat** (four views). Rationale: a lesson is a
distinct noun with a distinct lifecycle (browse → take → master → exam →
certificate) that has nothing to do with a recording's timeline or chat; nesting
it under Recordings would bury the entire browse/take experience and violate
"one control per act." This is a deliberate, justified expansion of the
three-view rule, not a MORE menu. The design review must confirm with the owner
(see Open Question OQ-1); the recommended resolution is the fourth view.

**FR-NAV-2.** Menu bar, tray, and window use the same word: **Lessons**.
Generating a lesson *from a recording* is initiated from that recording in
**Recordings** (consistent with "Make a study guide"), then the result appears
in **Lessons**. Taking/importing happens in **Lessons**.

**FR-NAV-3.** All Lessons UI follows DESIGN.md exactly: hazard yellow `#facc15`
on matte black, Inter, sentence case, SVG icons added to
`src/components/icons.tsx` (no emoji, no one-off inline SVGs), semantic tokens
(no raw hex), `prefers-reduced-motion` respected (every explainer animation has
a reduced-motion path), and WCAG (keyboard operable, visible focus ring, 4.5:1
body contrast, 44px touch targets, ARIA landmarks). Every Lessons surface has an
empty state and every async op an error state per DESIGN.md.

---

## 4. User stories and acceptance criteria

### 4.1 Source path A — generate a lesson from a captured Class

**Story.** As a student who recorded a lecture, I want AI to turn that Class
recording (transcript + screenshots) into an interactive lesson so I can learn
from it the way I would from a downloaded class.

Acceptance criteria:

1. From an open **Class**-type recording in Recordings, a **Make lesson** action
   (sentence case) generates a lesson from that recording's stored transcript
   and screenshots. (Non-Class recordings do not offer it; see OQ-2.)
2. Generation routes **every** AI call through `ai::client::complete` (Mac) and
   applies, unchanged, the full gate chain: Pro entitlement
   (`entitlement::require_pro()` in `mas`), configured endpoint, and consent for
   a public endpoint — the same Pro → configured → consent order used by study
   guides. No gate is added or skipped.
3. The generation input mirrors the study-guide contract: filtered transcript
   (Whisper hallucinations already dropped and never re-fed), stricken spans as
   `[stricken from the record]`, deleted words gone, each line time-stamped, and
   screenshot references available for figure/diagram grounding. Transcript text
   and model output are never logged.
4. Model output is **untrusted**: parsed tolerantly (think blocks, code fences,
   trailing commas), then every field strictly validated and cleaned
   (trim to one line where applicable, strip control chars, length/count caps,
   drop duplicates), and only cleaned values are stored and rendered **as plain
   text / structured data — never HTML**. An unusable part is retried once; after
   that the part shows an error and other parts are kept. Consent/Pro/config/key/
   network errors stop the whole run (same as study guides).
5. Small-context handling matches study guides: if the transcript doesn't fit
   beside prompt + answer (Apple on-device = 4K tokens), it is condensed chunk
   by chunk into time-stamped notes (map step, up to three rounds) and each part
   is written from those notes, using the same 3.2 chars/token estimate with a
   15% margin.
6. A generated lesson is written to local storage, labeled as **generated from a
   recording**, and linked to its source recording id and transcript
   fingerprint (SHA-256 of the transcript exactly as the prompt saw it).
7. Figures for a generated lesson: because the shipped app cannot call an image
   API at runtime, a *generated* lesson renders its structured diagrams through
   deterministic explainer components (SVG/canvas from structured data) and
   reuses any applicable seeded figure library; it does **not** fetch or
   synthesize new PNGs at runtime. (See OQ-7 for whether generated lessons may
   request figures through an authoring-time pipeline; recommended: no runtime
   figures, deterministic diagrams only.)

### 4.2 Source path B — portable, offline `.lesson` bundle

**Story.** As a user, I want to download or receive a lesson as a file and have
it work completely offline, so a lesson I got once keeps working on a plane.

Acceptance criteria:

1. A `.lesson` bundle is a single portable artifact (recommended: a zip-based
   container with a manifest; see OQ-3) containing everything the lesson needs:
   lesson metadata and ordering, the concept graph, all explainer structured
   data, quiz + exam item banks, trainer drill definitions, concept definitions,
   and **all figure PNGs embedded** in the bundle.
2. Taking a present bundle performs **zero network requests** — no AI call, no
   image fetch, no font/CDN fetch, no telemetry. This is enforced and tested
   (OQ-11): a lesson taken with the network disabled behaves identically.
3. A bundle carries a **schema version** and an **integrity check** (recommended:
   per-file hashes in the manifest + a manifest hash). On import, the app
   validates the manifest, rejects an unknown major schema version with a plain-
   language error, and rejects a bundle whose files don't match their hashes.
4. All text inside a bundle is treated as **untrusted data** on load (even seed
   content is validated), rendered as plain text / structured components, never
   as HTML. A malformed or oversized bundle fails import with an error state and
   never partially corrupts the local catalog.
5. A bundle records its **provenance**: `seed` (shipped), `imported` (third-party
   file/URL), or `generated` (from a user recording). Provenance drives purge
   behavior (§8) and the certificate's issuer line.

### 4.3 Source path C — the library / catalog + "download a class"

**Story.** As a user, I want to open Lessons and browse available classes, then
"download a class" and take it — without noFriction running any server.

Acceptance criteria:

1. The Lessons view opens on a **library** listing the lesson sets present
   locally, each with title, level (e.g. "AP Chem / college"), lesson count,
   progress, and mastery/exam status. Empty-state rules apply, but the library
   is **never empty on first run** because the six stoichiometry lessons ship as
   seed content.
2. "Download a class" is implemented as **import from a user-chosen source**: a
   local file, a local folder, or a URL the user pastes. noFriction hosts no
   catalog server and ships no default remote URL (consistent with the AI
   endpoint rule). The user supplies the source; the app fetches and validates
   the bundle, then adds it to the local catalog.
3. If a user pastes a URL, that single fetch is the only network activity, it is
   clearly a user-initiated download (not silent), it respects the no-default-
   URL rule, and the downloaded bundle thereafter runs fully offline.
4. The six seeded stoichiometry lessons are present and fully usable by **free**
   users offline on first run, with no AI configured and no provider active.
5. A library entry can be removed; removing an `imported` or `seed` set deletes
   only its local catalog copy and progress (no recording purge semantics —
   see §8).

> Resolution of the stated tension: "download a class" genuinely cannot mean an
> owner-hosted store without violating the no-servers invariant. We resolve it as
> **local catalog + user-supplied import + shipped seed content**, so the
> browse/download experience exists and is non-empty on day one without any
> noFriction backend. We do not silently drop the browse/download experience.

### 4.4 Taking a lesson — explainers and interactive sessions

**Story.** As a learner, I want visual explainers and genuinely interactive
sessions, not slop and not raw model HTML, so I actually understand the material.

Acceptance criteria:

1. A lesson presents, in order: a visual/interactive explainer sequence, an
   interactive session, a checkpoint quiz, and (as mastery dictates) trainer
   drills. Progress is saved locally and resumable.
2. **Two complementary visual mechanisms, both required:**
   - **(i) Deterministic interactive explainers** — purpose-built React
     components driven by the bundle's structured data, accurate and unit-
     testable. The required inventory for the stoichiometry set (FR-EXP):
     1. **Equation balancer** — interactive balancing of chemical equations
        with live validation of atom/charge conservation.
     2. **Mole-ratio / stoichiometry calculator** with **step reveal** —
        shows each dimensional step one at a time.
     3. **Limiting-reagent simulator** — adjust reactant amounts, see which
        runs out and the yield.
     4. **Dimensional-analysis "railroad" builder** — drag/compose unit-
        conversion "tracks" with unit cancellation feedback.
     5. **Labeled molecule / reaction diagrams** rendered from structured data
        (atoms, bonds, labels) — not free-form model HTML.
     6. **Titration curve** explorer.
     7. **Ideal-gas PV explorer** (PV = nRT relationships).
   - **(ii) AI-authored static figures** — the best image model (genmedia MCP
     image tool: gemini-3-pro-image / "Nano Banana Pro") produces reviewed PNGs
     **at authoring time**, embedded in the bundle. The shipped app renders them
     as images and **never calls an image API at runtime**.
3. No explainer renders model-produced HTML. Interactive components consume only
   validated structured data; any text labels are plain text.
4. Each explainer is numerically **accurate** (e.g. the balancer only accepts a
   truly balanced equation; the calculator's answers match the item bank's
   verified values within stated tolerance) and is covered by unit tests.
5. Every explainer has a `prefers-reduced-motion` path, full keyboard operation,
   and screen-reader labels (WCAG per DESIGN.md).

### 4.5 Quizzes, mastery, trainers, exam, and certificate (the "more than a quiz" bar)

**Story.** As a learner, I want checkpoint quizzes, per-concept mastery, adaptive
trainers for my weak spots, and a cumulative final exam, so that passing it means
I actually know stoichiometry.

Acceptance criteria (concrete thresholds — these are the decisions for this doc;
design may refine wording, not loosen rigor):

1. **Checkpoint quiz per lesson** — each of the six lessons ends with a
   checkpoint quiz of **8–10 questions**. A lesson's checkpoint is "passed" at
   **≥ 80%**; below that, the app routes the learner to the explainers/concepts
   for the missed items.
2. **Per-concept mastery tracking** — the bundle defines a **concept graph**
   (each lesson maps to concepts; each quiz/exam/trainer item tags the concept(s)
   it tests). Mastery per concept is tracked on a 0–100 scale, updated from
   quiz, trainer, and exam performance (recommended rule: exponential moving
   average weighting recent attempts, correct-on-first-try worth full credit;
   exact decay in design). A concept is **mastered at ≥ 85** with at least
   **3 correct attempts**.
3. **Adaptive trainers** — a trainer selects items that target the learner's
   **weakest unmastered concepts** first and drills until each reaches mastery or
   the learner stops. Trainers draw from a parameterized/numeric item generator
   where possible (so drills aren't identical repeats) with answers checked by
   the same tolerance rules as the exam.
4. **Cumulative final exam** — **30 questions** spanning all six lessons, mixing:
   - **conceptual multiple-choice**,
   - **numeric free-response with tolerance** (answer accepted within a stated
     absolute-or-relative tolerance, with significant-figures guidance), and
   - **multi-step problems** (e.g. balance → mole ratio → limiting reagent →
     grams), scored per step or as a gated chain.
   Recommended mix: ~12 conceptual MC, ~12 numeric free-response, ~6 multi-step.
5. **Pass threshold = 80%** of exam points. The exam is intended to be taken
   after the full interactive session; it is available once all six checkpoints
   are passed (recommended gate; see OQ-6).
6. **On pass**, the app issues a **shareable/printable certificate** naming the
   course, level, score, date, and issuer/provenance (seed vs imported vs
   generated). The certificate is produced locally and offline (recommended:
   render to PDF/printable view; export via the save dialog, matching the study
   export pattern — `files.user-selected.read-write` in the `mas` sandbox).
7. **On miss**, **remediation routes back** to the specific explainer and
   concept each missed item tested, then offers a targeted trainer and a retake.
   The exam can be retaken; question selection re-randomizes/draws fresh numeric
   values where available.
8. All quiz/exam/trainer item banks live **in the bundle** (so the exam works
   fully offline) and all item/answer/explanation text is validated and rendered
   as plain text, never HTML.

### 4.6 The six seeded stoichiometry lessons (authored content)

**Story.** As a free user on first run, I want a real, complete stoichiometry
course already present so Lessons is immediately useful.

Acceptance criteria:

1. Six lessons ship as seed content at **AP Chem / college** level. Recommended
   ordering (design/content may refine titles):
   1. Moles, molar mass, and the mole concept
   2. Chemical equations and balancing
   3. Mole ratios and stoichiometric calculations
   4. Limiting reagents and percent yield
   5. Solution stoichiometry and titration
   6. Gas stoichiometry (ideal gas, gas-law stoichiometry)
2. The seed set includes, across the six lessons: the full FR-EXP explainer
   inventory (§4.4), the per-lesson checkpoint quizzes, the concept graph,
   trainer item banks, the 30-question final exam, and the embedded,
   authoring-time figure PNGs.
3. The seed set is bundled as a `.lesson` with provenance `seed`, loaded into the
   local catalog on first run, and fully usable **offline by free users** with no
   AI configured.
4. Every figure in the seed set is reviewed for scientific accuracy before
   shipping (authoring-time review gate — see OQ-9).

---

## 5. Pro gating

- **FR-PRO-1.** *Generating* a lesson from a recording requires **Pro** and goes
  through `ai::client::complete` with consent + endpoint policy +
  `entitlement::require_pro()` (in `mas`), identical to study-guide generation.
- **FR-PRO-2.** *Taking* an already-present lesson (seed or imported) is **free**
  and offline. *Importing* a bundle (file/folder/URL) is **free**. Mastery,
  trainers, exam, and certificate for a present lesson are all free.
- **FR-PRO-3.** The six stoichiometry seed lessons are **fully usable by free
  users**, including the exam and certificate.

---

## 6. Offline

- **FR-OFF-1.** Taking a lesson end-to-end (explainers, interactive sessions,
  trainers, quizzes, exam, certificate) works **fully offline** with **zero
  network** once the bundle is present.
- **FR-OFF-2.** Only generation and acquisition (AI calls, a user-initiated URL
  download) may use the network. A present bundle contains everything it needs,
  including fonts/assets it depends on (bundle must not rely on a runtime CDN).
- **FR-OFF-3.** First run with no AI configured and no provider active still
  shows a non-empty library and lets a free user take the seed course fully.

---

## 7. The genmedia / figure-authoring invariant

- **FR-FIG-1.** Figure PNGs are produced with the genmedia MCP image tool
  (gemini-3-pro-image / "Nano Banana Pro") **at authoring time only**, as part of
  building the seed bundle. The output is reviewed, then embedded in the bundle.
- **FR-FIG-2.** The shipped app contains **no image-generation API client** and
  makes **no image API call at runtime**, in any flavor. `check-ai-provider-
  policy.py` must still pass (no new AI-service host, no credential literal, no
  default remote URL, no provider active at first run).
- **FR-FIG-3.** Figures are static and never model-HTML; they render as `<img>`
  from bundle-embedded PNG bytes.

---

## 8. Purge (Delete / Strike)

Per `CLAUDE.md` and `docs/REDACTION.md`, anything derived from a user recording
must join the purge pipeline; shipped/imported third-party content is exempt.

- **FR-PURGE-1.** A lesson with provenance `generated` (built from a user
  recording) is **recording-derived**. Any Delete or Strike of that recording's
  transcript text (words, lines, time ranges) **deletes the generated lesson in
  full**, in the live store and in app backups — it is **not rewritten**
  (it paraphrases the recording, exactly like a study guide, which is deleted,
  not rewritten). The preview/confirmation says so. Screen-only edits keep it.
- **FR-PURGE-2.** A `generated` lesson still being generated is **not saved** if
  the transcript changed meanwhile (re-check the transcript fingerprint inside
  `BEGIN IMMEDIATE`, refusing during a Delete's undo window), mirroring
  `study::save_materials`.
- **FR-PURGE-3.** Deleting a whole recording deletes its `generated` lessons with
  it (cascade), like its study guide.
- **FR-PURGE-4.** `seed` and `imported` lessons are **not** recording-derived and
  are **exempt** from recording purge. This distinction is documented in
  `docs/REDACTION.md`'s purge checklist (extend step 5) and surfaced in the UI
  (a generated lesson shows it's tied to a recording; an imported/seed one does
  not).
- **FR-PURGE-5.** Learner progress/mastery/exam results for a `generated` lesson
  are deleted with the lesson. For `seed`/`imported` lessons they live until the
  user removes the catalog entry.
- **FR-PURGE-6.** Sync (if in scope later): a tombstone for a struck/deleted
  recording triggers the receiver's own purge of its `generated` lesson, never a
  shortcut, and the removed transcript text is never sent (same rule as study
  guides). Recommended: lessons are **not** synced in v1 (see OQ-10).

---

## 9. Storage and data layer (shared-friendly)

- **FR-DATA-1.** The lesson schema, bundle format, validation/parse logic, concept
  graph, mastery math, and item-bank model are defined in a **platform-neutral**
  way so the Rust layer (Mac) and a future Swift layer (iOS) share the same
  contract, mirroring how `docs/STUDY_TOOLS.md` specifies parallel Mac/iOS
  shapes. Mac storage uses SQLite via sqlx (new `study`-adjacent module, e.g.
  `src-tauri/src/lessons/`); figure PNGs are stored as bundle files on disk
  under the app data folder, not as DB blobs (recommended; see OQ-4).
- **FR-DATA-2.** New commands follow the existing `study::commands` registration
  pattern in `lib.rs` and the existing frontend view/hook conventions
  (`components/lessons/`, a `LessonsView`).
- **FR-DATA-3.** Any place that stores recording-derived text (a `generated`
  lesson) is added to the `docs/REDACTION.md` purge checklist and the FTS/search
  index rules if lesson text becomes searchable (recommended: generated lessons
  are **not** added to the recordings FTS index in v1, to limit purge surface;
  see OQ-8).

---

## 10. iOS parity (honest scope)

- **FR-IOS-1.** **Mac ships fully.** iOS is explicitly a **planned follow-up**,
  not shipped in this iteration. We do not quietly make this Mac-only; this
  section is the statement.
- **FR-IOS-2.** The design **must** keep the data layer, bundle format, validation,
  concept graph, mastery math, and item banks shared-friendly so iOS can adopt
  them with a SwiftUI rendering layer, mirroring the Mac/iOS split already used
  for markers and study guides.
- **FR-IOS-3.** iOS parity is **stubbed/planned**: taking a lesson (explainers,
  quizzes, trainers, exam, certificate) is the iOS target; `generate from a
  recording` on iOS follows the existing `MeetingAI`/`AIClient` Pro → configured
  → consent path when built. The seed bundle must be loadable on iOS unchanged.
- **FR-IOS-4.** Interactive-visual richness is acknowledged as harder on iOS;
  Mac-first for the explainer components is acceptable and stated. The design
  must list which explainers port directly vs need a SwiftUI re-implementation.

---

## 11. Non-functional requirements

- **NFR-1 (build).** Both Mac flavors build and test green: default (Developer ID
  DMG) and `--features mas` (sandboxed). In `mas`: no ffmpeg/Accessibility/
  osascript/shell/`.env`; Pro via `entitlement::require_pro()`. Verify with
  `cargo test --lib`, `cargo test --lib --features mas`, `npx tsc --noEmit`.
- **NFR-2 (policy).** `python3 scripts/check-ai-provider-policy.py` passes: no
  credential literal, no default remote URL, no provider active at first run, no
  new AI-service host, no network at startup/save, no bundled key.
- **NFR-3 (design).** Full DESIGN.md conformance (color tokens, Inter, sentence
  case, icons from `icons.tsx`, reduced-motion, WCAG, empty + error states).
- **NFR-4 (performance).** A lesson loads and an explainer becomes interactive
  quickly from the bundle on disk (recommended budget: first explainer
  interactive < 1 s on a mid Mac); figures are embedded so there's no fetch wait.
- **NFR-5 (size).** The seed bundle's embedded PNGs are optimized so the shipped
  app size increase stays reasonable (design to set a concrete cap, e.g. target
  < ~40 MB for the six-lesson seed set; see OQ-5).
- **NFR-6 (security/trust).** All bundle content (including seed) is validated on
  load; a hostile/corrupt bundle cannot execute code, render HTML, or escape the
  app data folder on import.

---

## 12. Open questions (resolve before/at design) — each with a recommendation

- **OQ-1 (nav).** Does Lessons become a **fourth top-level view** (Record ·
  Recordings · Lessons · Chat), breaking the literal "three views" rule?
  *Recommend:* **yes, fourth view** — it's a first-class noun; justify in design
  and confirm with owner.
- **OQ-2 (generation source type).** Can non-Class recordings generate a lesson,
  or only Class type? *Recommend:* **Class only** (lessons are the Class study-
  guide successor), consistent with study-guide framing by type.
- **OQ-3 (bundle format).** Zip container with JSON manifest + PNGs, vs a single
  JSON with base64 PNGs, vs SQLite file. *Recommend:* **zip container** (`.lesson`)
  with a `manifest.json` (schema version, per-file hashes, provenance), JSON data
  files, and a `figures/` dir of PNGs — portable, inspectable, efficient.
- **OQ-4 (figure storage).** Figures on disk as files vs DB blobs. *Recommend:*
  **files on disk** under the app data folder, referenced by the catalog row.
- **OQ-5 (seed size cap).** What's the acceptable app-size increase for the seed
  PNGs? *Recommend:* set a concrete cap in design (target < ~40 MB), optimize/
  downscale figures accordingly.
- **OQ-6 (exam gate).** Must all six checkpoints be passed before the final exam
  unlocks? *Recommend:* **yes** — the exam is cumulative and meant to follow the
  full interactive session; allow a settings/debug override only in dev.
- **OQ-7 (figures for generated lessons).** May a *generated* lesson obtain
  figures (which would need an authoring-time image pipeline), or only use
  deterministic diagrams? *Recommend:* **deterministic diagrams only at runtime**;
  no runtime image generation, period. Any figure authoring stays an offline
  authoring tool, not shipped-app behavior.
- **OQ-8 (search index).** Should generated-lesson text be searchable in the
  recordings FTS index? *Recommend:* **no in v1** (limits purge surface and
  avoids an FTS update-trigger pitfall noted in REDACTION.md).
- **OQ-9 (figure accuracy review).** Who/what gates figure scientific accuracy
  before shipping seed content? *Recommend:* an explicit **authoring-time review
  checklist** (subject-matter review of each figure + explainer answer key),
  recorded in the content-authoring doc; the design/content step owns it.
- **OQ-10 (sync).** Are lessons synced device-to-device? *Recommend:* **not in
  v1**; if added later, follow the SYNC.md tombstone rules and never send removed
  transcript text.
- **OQ-11 (offline enforcement/testing).** How is "zero runtime network" proven?
  *Recommend:* a test that takes the seed lesson with network calls stubbed to
  fail and asserts success, plus the policy guard confirming no runtime image/AI
  host and no CDN font dependency in the Lessons code path.
- **OQ-12 (numeric tolerance spec).** Exact tolerance and significant-figure
  rules for numeric free-response. *Recommend:* per-item tolerance in the item
  bank (absolute or relative, e.g. ±1% default) plus a sig-fig hint; design to
  finalize the comparison algorithm (parse number, normalize units, compare
  within tolerance), unit-tested.
- **OQ-13 (certificate verifiability).** Should the certificate be verifiable
  (signed/hash) or purely cosmetic? *Recommend:* **cosmetic + local** for v1
  (course, level, score, date, provenance), no server-backed verification, since
  there is no backend; optionally embed a self-contained hash of the result for
  the user's own records.
- **OQ-14 (progress reset).** Can a user reset a lesson set's progress/mastery?
  *Recommend:* **yes**, a per-set "Start over" that clears progress but keeps the
  bundle.

---

## 13. Out of scope

- Any noFriction-hosted server, catalog store, account system, relay, iCloud,
  Supabase, or Pinecone.
- Runtime image generation or any runtime image/AI API call for a present lesson.
- Rendering any model output as HTML.
- Subjects other than stoichiometry for the seed content (the architecture is
  general, but only the six stoichiometry lessons are authored now).
- Full iOS implementation (planned follow-up; data layer kept shared-friendly).
- Multi-user / classroom / grade-reporting / LMS integration.
- Lesson sync between devices (deferred; see OQ-10).

---

## 14. Assumptions

- The genmedia MCP image tool is available in the authoring environment (not the
  shipped app) to produce the seed figures. If it is not, seed figures fall back
  to deterministic SVG diagrams and the figure-review gate still applies.
- "Download a class" is acceptably satisfied by user-supplied import + seed
  content, given the no-servers invariant (confirmed by the task's clarified
  decision #1).
- The study-guide validation/parse/purge patterns (`src-tauri/src/study/`) are
  the authoritative template to extend for Lessons.
- StoreKit Pro gating already exists and is reused unchanged via
  `ai::client::complete`.
