# Lessons (interactive education): technical design (Mac + iOS-ready)

Status: design revision 5 (resolves the revision-4 design-review
CHANGES_REQUESTED: both remaining MEDIUMs — required seed figures and a
frontend-provable zero-runtime-network guard — and both remaining NITs — the
stale Shell.tsx header comment and a balance-kind checkpoint floor for Lesson 2;
cites the durable owner decisions of record, D1–D5)
Scope: noFriction Mac app (Tauri 2 / Rust + React), with an explicit iOS parity plan
Implements: `.agents/tasks/lessons-stoichiometry/requirements.md`
Source of record for owner decisions:
`.agents/tasks/lessons-stoichiometry/owner-decisions.md` (D1–D5)
Related: `docs/STUDY_TOOLS.md` (the pattern this extends), `docs/AI_PROVIDERS.md`,
`docs/REDACTION.md`, `docs/ARCHITECTURE.md`, `DESIGN.md`, `CLAUDE.md`,
`docs/PRO.md`
Revision note: see §18 for the point-by-point response to the design review.
**Revision 5** closes the four findings the revision-4 review left open when it
hit its iteration ceiling, in one pass, each against a durable owner decision:
- **Finding 1 [MEDIUM] — AI-authored figures were required by the requirements
  but never mandated in the seed set** → resolved per **owner-decision D4**: the
  authored PNG figures are now **genuinely required**, with a seed assertion
  that each of the six seed lessons embeds ≥1 `figure`-kind explainer backed by a
  **decoding** embedded PNG; the `2..=4 explainers` cap is reworded so figure and
  interactive kinds are both counted but a **figure floor** is enforced; and the
  "stretch" / "all-deterministic-SVG seed is shippable" hedges are removed (§3.2,
  §4.6, §6, §12.1, §14, §15, §18.4). Deterministic SVG explainers are additive,
  not a replacement.
- **Finding 2 [MEDIUM] — "zero runtime network" was asserted but not provable on
  the frontend render path** → resolved per **owner-decision D5**: an
  authoritative build/test **grep guard** scans the compiled Lessons JS bundle +
  `Lessons.css` for any non-local `http(s)://` or `url(...)` remote reference and
  **fails on a match**; a **Tauri CSP** note restricts `connect-src`/`img-src`/
  `font-src` for the Lessons surface; and an assertion confirms `Lessons.css`
  uses only the bundled Inter font and imports no remote asset (§11.2, §15).
- **Finding 3 [NIT] — stale Shell.tsx header comment** → added to the §17 edit
  list (updated to "one of four views (Record / Recordings / Lessons / Chat)")
  alongside the `AppMode` enum change.
- **Finding 4 [NIT] — no floor guaranteeing a `balance`-kind item in the
  balancing checkpoint** → added a seed assertion that **Lesson 2's checkpoint
  contains ≥1 `balance`-kind item** (and, generally, every concept whose
  explainer kind is `equation_balancer` is covered by ≥1 `balance`-kind item) to
  the §10.1b / §15 item-bank-floor tests.

The **revision-4** pass resolved the revision-3 review's one MEDIUM (the §9
numeric grader now converts compatible prefixed/scaled units instead of leaving
them undefined) and both NITs (owner-authority precision in §18.2, and the §2
COORDINATION.md §4 sign-off reconciliation). The earlier revision resolved the
two HIGH-adjacent authority questions the reviewer refused to take on the doc's
word against a **durable, verifiable file**,
`.agents/tasks/lessons-stoichiometry/owner-decisions.md`:
- **owner-decision D1** confirms Lessons is a first-class fourth top-bar view
  (resolves review MEDIUM-2 — the doc no longer asserts an unverifiable inline
  "confirmed with owner"; it cites D1).
- **owner-decision D2** confirms Source Path A (generate-from-recording) is
  **deferred and ships as an honest "coming soon" Generate stub this build**,
  with the full AI pipeline captured as a vNext contract carrying its accuracy
  precondition (resolves review HIGH-1 — the requirements' "built now" is
  superseded by D2; "built now" for this build means the first-class Generate
  entry point and its coming-soon preview, not the AI pipeline).
- **owner-decision D3** ("finish and enhance" / "make this high end") authorizes
  the enhancement set in §8 and §10; schedule-risk items are marked "stretch,"
  never silently dropped (§18.4).
- **owner-decision D4** ("not slop … AI you generate good visuals using the best
  models possible") makes AI-authored figures a **required** part of the shipped
  seed set, not a stretch item: each of the six seed lessons embeds ≥1 decoding
  figure PNG, enforced by seed validation (§3.2/§4.6/§6/§12.1/§14/§15; resolves
  review Finding 1 via its fix (a)).
- **owner-decision D5** ("yes work fully offline") requires the zero-runtime-
  network guarantee to be **enforced and tested on the React/CSS render path**,
  not only by stubbing the Rust AI client: an authoritative grep guard over the
  compiled Lessons bundle + `Lessons.css`, a Tauri CSP note, and a bundled-font
  assertion (§11.2/§15; resolves review Finding 2).

---

## 1. Overview

Lessons is noFriction's first interactive-education subsystem. A **lesson set**
(a course) is a self-contained, portable, 100%-offline unit of study material:
the richer successor to a Class recording's study guide. Where a study guide is
five text parts, a lesson set carries a concept graph, deterministic
data-driven interactive explainers (purpose-built React components, never model
HTML), AI-authored static figure PNGs embedded in the bundle, per-lesson
checkpoint quizzes, per-concept mastery tracking, spaced-repetition review,
adaptive trainers, a cumulative final exam, and a locally-issued certificate.

The design extends the study subsystem's proven architecture rather than
inventing a new one. The whole value chain is: **untrusted structured content →
tolerant parse → strict validation + cleaning → store only cleaned structured
data → render as plain text / typed React components, never HTML**. That is
exactly `src-tauri/src/study/parse.rs::validate` applied to a bigger schema.
Purge of recording-derived lessons joins the existing
`redaction.rs::redact_ai_outputs` pipeline next to `study::purge_for_meeting`.

**What ships in this build (per owner-decision D2):** the only lessons that
exist at runtime are the **six human-authored, verified seed stoichiometry
lessons** (Source Path C) and any **imported** `.lesson` bundles (Source Path
B). **AI generation from a recording (Source Path A) is deferred by owner-
decision D2** and ships as a first-class, visible **Generate** affordance that
opens an intentional, honest "AI lesson generation — coming soon" preview (§12).
It makes **no AI call** in this build and is **not a dead or broken control**.
The full recording→lesson pipeline is captured at contract level, with its
correctness preconditions, in the **deferred §13**, not implemented this cycle.

This resolves the design review's two HIGH findings against a verifiable
authority rather than an inference: HIGH-1 (Source Path A deferral) is now
backed by **owner-decision D2** (recorded verbatim: *"add a generate button that
we will make work later"*) and the requirements' "built now" is explicitly
superseded by D2 — for this build "built now" means the first-class Generate
entry point and its coming-soon preview, not the AI pipeline. HIGH-2 (no
correctness contract for AI-generated explainer data) is defused because there
is **no AI-generated content to validate in this build**, and §13 states the
explicit, hard precondition that any future generated explainer data must pass
the §6 Rust recompute-and-replace validation plus the accuracy gate before a
generated lesson is ever marked playable (the model's own numbers are never
stored, only the Rust-recomputed values).

The locked decisions this document makes (resolving every requirements open
question) are summarized in §17. The headline choices: the portable unit is a
**zip-based `.lesson` bundle** with a hashed `manifest.json`; the final exam is
**30 questions, 80% pass, gated on all six checkpoints**; Lessons is a
**first-class fourth top-bar view**; **Mac ships fully, iOS data+correctness
layer is shared Rust and the SwiftUI render layer is a documented follow-up**;
and the shipped app makes **zero image/AI calls for a present lesson** (figures
are embedded PNGs authored offline).

### Technology stack (locked once approved)

- **Rust** backend in `src-tauri/src/lessons/` using **sqlx** against the
  existing app SQLite database (the same pool `study.rs` uses). No new DB
  engine.
- **Bundle container**: zip, read/written in Rust with the **`zip` crate**
  pinned **`zip = "=2.2.0"`** (NIT-10): a pure-Rust reader/writer, no system
  `libzip`, dual MIT/Apache-2.0 licensed, so the `mas` sandbox build stays
  clean. We use only the `deflate` + `stored` features (no bzip2/zstd native
  deps). Figures are stored as loose PNG files on disk under the app data folder
  after import; the DB holds paths, not blobs.
- **Hashing/integrity**: **`sha2`** (already a dependency, used by
  `study::prompt::fingerprint`).
- **Frontend**: **React + TypeScript**, explainers rendered with inline
  **SVG/Canvas** driven by validated structured data. No new charting or
  physics library — the explainers are small and must be unit-testable and
  deterministic; a third-party chart lib would add runtime weight, a possible
  CDN/font dependency (forbidden by FR-OFF-2), and opacity we don't want in a
  correctness-critical path.
- **Correctness/grading is authoritative in Rust** (`lessons::grade`,
  `lessons::explain`, `lessons::mastery`, `lessons::exam`), shared with a future
  Swift layer. The web layer calls these through Tauri commands for grading; a
  small TS mirror exists **only for live, non-authoritative UI hints** and never
  decides pass/fail or writes a stored score (MEDIUM-6).
- **AI**: `ai::client::complete` (Mac) / `AIClient` (iOS) only, and **only in
  the deferred §13 pipeline** — not wired in this build. No image API client in
  the shipped app, in any flavor.

---

## 2. Where Lessons lives (navigation) — DECISION

**Lessons is a first-class fourth top-level view**, per **owner-decision D1**
(`.agents/tasks/lessons-stoichiometry/owner-decisions.md`, verbatim: *"gets it
own tab."*). The top bar becomes **Record · Recordings · Lessons · Chat**
(⌘1–⌘4), resolving OQ-1 as the requirements recommended. This is the durable,
verifiable confirmation the design review (MEDIUM-2) correctly demanded before
DESIGN.md's "three views" rule may be expanded; the doc no longer asserts an
inline, unverifiable "confirmed with owner." The nav change is implemented in
one PR across `menu_builder.rs`, `TopBar.tsx`, `App.tsx` listeners,
`tray_builder.rs`, `docs/USER_GUIDE.md`, `DESIGN.md`, and the film-harness mock
(per COORDINATION.md §4).

**Sign-off reconciliation (review finding 3).** COORDINATION.md §4 phrases the
nav change as still pending ("Get Casey's yes before merging the nav change"),
written before the owner's decision was recorded. **That gate is now
satisfied, not outstanding:** owner-decision **D1** ("gets it own tab") and
COORDINATION.md **§0** ("gets its own tab and generate button") *are* the owner
yes §4 asks for. D1/§0 supersede §4's pending phrasing; the nav PR is not
blocked on any further confirmation. (The stale §4 wording should be updated in
the coordination note; it is documentation drift, not an open approval gate.)

Justification against DESIGN.md's "three views, nothing under a MORE menu"
rule: the rule's intent is that every surface must *earn* its place and nothing
hides in an overflow menu. A lesson set is a distinct noun with a lifecycle
(browse → take → master → review → exam → certificate) that shares nothing with
a recording's timeline or with chat. Nesting it under Recordings would bury the
entire browse/take/exam experience behind a recording selection and violate
"one control per act". Lessons earns a first-class slot exactly as Chat did; it
is not a MORE menu. This is a deliberate, documented expansion from three to
four primary views, and DESIGN.md's navigation table and the vocabulary table
are updated in the same change (see §16).

Vocabulary (added to DESIGN.md's table): the noun is **lesson** (one unit) and
**lesson set** / **course** (the ordered collection). The verb to create one
from a recording (deferred) is **Make lesson** (sentence case), aligning with
DESIGN.md's existing "Make notes / Make review guide" pattern (COORDINATION.md
§4); the first-class affordance that opens its coming-soon preview is labeled
**Generate** per owner-decision D2's verbatim "generate button." Both words name
the same deferred capability — **Make lesson** is the catalog/vocabulary verb,
**Generate** is the button on the surface — and the design review's vocabulary
rule is satisfied because "Make lesson" is the table entry. The list surface is
**Lessons**. The menu bar, tray and window all say **Lessons**. "Download a
class" surfaces in the UI as **Import a lesson…** (because that is honestly what
it does; see §5). No vocabulary collision with the forbidden word "Markers".

Taking and importing happen in Lessons. The deferred **Make lesson / Generate**
affordance appears in **two** places, per owner-decision D2 and COORDINATION.md
§0: (1) a primary button in the **Lessons** view, and (2) a **Make lesson**
action on a **Class-type recording in Recordings** (consistent with that
recording's existing "Make notes / Make review guide" actions). In this build
both openings lead to the same honest coming-soon preview (§12.3) and make no AI
call; when §13 ships, the Recordings action generates from that recording and
the Lessons button opens a recording picker.

All Lessons UI follows DESIGN.md: hazard-yellow `#facc15` tokens on matte
black, Inter, sentence case, SVG icons from `src/components/icons.tsx`, semantic
tokens only, `prefers-reduced-motion` honored by every explainer animation and
every lesson-player transition, WCAG (keyboard operable, visible 2px focus ring,
4.5:1 body contrast, 44px targets, ARIA landmarks, and a screen-reader
text-equivalent for every interactive explainer — §6). Every Lessons surface has
an **empty state**, a **loading state**, and an **error state** per DESIGN.md
(§15). The library is non-empty on first run (seed), so the "no lessons" empty
state is only reachable after the user removes the seed set.

### New icons to add to `src/components/icons.tsx` (append only)

24×24 viewBox, `currentColor`, round caps, `strokeWidth={1.5}`, matching the
existing stroke set. **Render sizes per DESIGN.md iconography (NIT-11):** nav /
control glyphs render at **14–17px**; empty-state / hero glyphs render at
**40–48px**.

- `GraduationCapIcon` — the Lessons top-bar **nav** icon and tray/menu glyph
  (nav size 14–17px).
- `AtomIcon` — lesson-set / science-course badge and the library **hero /
  empty-state** glyph (hero size 40–48px).
- `FlaskIcon` — chemistry lesson marker; limiting-reagent/titration explainers
  (control size).
- `ScaleIcon` (balance) — equation balancer and mole-ratio explainers (control).
- `RouteIcon` — dimensional-analysis "railroad" builder (control).
- `GaugeIcon` — ideal-gas PV explorer; mastery / exam-readiness meter (control).
- `TrophyIcon` — exam pass / mastery-complete and the mastery-dashboard **hero**
  when a course is complete (hero size 40–48px in that one placement, control
  size inline).
- `CertificateIcon` — the certificate surface and its share/print action
  (control; hero size on the certificate screen).
- `TargetIcon` — adaptive trainer and the "Review due" surface (control).
- `ImportIcon` (tray-with-down-arrow) — "Import a lesson…" (control).
- `SparkleIcon` — the deferred **Generate** affordance's "coming soon" preview
  (control; hero size on the preview panel).

No emoji, no one-off inline SVGs (DESIGN.md iconography rule).

---

## 3. Data model

All types are **platform-neutral** (plain structs with serde on Rust; the same
shapes map 1:1 to Swift `Codable` for iOS, FR-DATA-1). Rust is the source of
truth for the schema; `docs/LESSONS.md` (this doc) is the contract both
platforms satisfy. IDs are UUID strings generated at author/import time and are
stable inside a bundle (so mastery/progress rows survive re-import of the same
version).

### 3.1 Entities and relationships

```
LessonSet (course)  1─┬─* Lesson        (ordered)
                      ├─* Concept        (the concept graph; edges = prerequisites)
                      ├─1 Diagnostic?    (optional placement pre-test, §8.4)
                      └─1 Exam           (cumulative item bank + config)

Lesson      1─┬─* Explainer       (ordered; interactive or figure)
             ├─* WorkedExample    (ordered; step-by-step reveal)
             ├─1 Misconceptions   (the "common mistakes" explainer, §4.6/§6)
             ├─1 Trainer          (adaptive, difficulty-scaling drill spec)
             └─1 Checkpoint       (8–10 item quiz)

Concept     *──* Lesson         (a concept is taught across one or more lessons)
QuizItem / ExamItem  *── Concept (each item tags the concept(s) it tests)

MasteryRecord   per (lessonSetId, conceptId)      — learner-local, +SR fields (§8.1)
LessonProgress  per (lessonSetId, lessonId)       — learner-local
ExamAttempt     per (lessonSetId, attemptId)      — learner-local
Certificate     per passed ExamAttempt            — learner-local, exportable
```

Bundle-authored content (LessonSet, Lesson, Concept, Explainer, WorkedExample,
Misconceptions, Trainer, Checkpoint, Exam, Diagnostic, Figure) is **immutable**
at runtime — it is the imported artifact. Learner-local records (MasteryRecord,
LessonProgress, ExamAttempt, Certificate) are mutable and live in the app DB
keyed by the catalog entry.

### 3.2 Authored content (lives in the bundle)

**LessonSet**
```
id: Uuid
schemaMajor: u32                   // bundle major schema; import rejects an unknown major (NIT-8)
                                   //   the minor lives only in the manifest (§4.2); the set carries the major
title: String                      // "Stoichiometry"
subtitle: String?                  // "AP Chem / college level"
level: String                      // free-text level label
provenance: "seed" | "imported" | "generated"   // "generated" never produced in this build (§12/§13)
sourceRecordingId: Uuid?           // reserved for provenance=generated (deferred §13)
transcriptFingerprint: String?     // reserved for generated (deferred §13; purge link)
lessons: [Uuid]                    // ordered lesson ids
concepts: [Concept]
diagnostic: Diagnostic?            // optional placement pre-test (§8.4)
exam: Exam
fingerprint: String                // SHA-256 over the canonicalized content (§4.4)
```

**Concept**
```
id: Uuid
name: String                       // "Mole ratio"
summary: String                    // one-paragraph plain text
prerequisites: [Uuid]              // concept ids; forms a DAG (validated acyclic)
lessonIds: [Uuid]                  // where it is taught
```

**Lesson**
```
id: Uuid
index: u32                         // 1..=6
title: String
objectives: [{ text: String, conceptIds: [Uuid] }]   // learning objectives mapped to concepts (§4.6)
conceptIds: [Uuid]
explainers: [Explainer]            // ordered; 2..=5 per lesson, counting figure AND interactive
                                   //   kinds together, with a HARD FLOOR of >=1 figure-kind
                                   //   explainer backed by a decoding embedded PNG (D4; §4.6/§14)
workedExamples: [WorkedExample]    // ordered; 3+ per lesson, step reveal
misconceptions: Misconceptions     // the "common mistakes" explainer (§4.6/§6 kind 8)
trainer: Trainer
checkpoint: Checkpoint
```

**Explainer** (tagged union — see §6 for each payload)
```
id: Uuid
title: String
conceptIds: [Uuid]
caption: String                    // plain-text description
a11yState: String?                 // template for the screen-reader "current state" text (§6)
kind: ExplainerKind                // "figure" | "equation_balancer" | "mole_ratio"
                                   // | "limiting_reagent" | "dimensional_analysis"
                                   // | "molecule_diagram" | "titration_curve"
                                   // | "ideal_gas_pv" | "misconceptions"
data: <kind-specific validated structured object>   // §6
```

A `figure` explainer's `data` is `{ figureId: Uuid, alt: String }` and refers
to an embedded PNG (§4.2). All other kinds carry structured parameters; they
render deterministically with zero assets.

**WorkedExample** (step reveal; also the full-solution template reused by
trainers/exam, §8.3)
```
id: Uuid
prompt: String                     // "How many grams of H2O form from 4.0 g H2?"
conceptIds: [Uuid]
steps: [{ text: String, expression: String?, result: String?, unit: String? }]  // revealed one at a time
finalAnswer: { value: f64, unit: String? }   // the verified key (§4.6 rigor; recomputed-checkable, §6)
```

**Misconceptions** (the "common mistakes" explainer; `kind: "misconceptions"`)
```
items: [{ claim: String,          // the tempting-but-wrong statement, plain text
          why_wrong: String,      // plain text
          correct: String,        // plain text
          conceptIds: [Uuid] }]   // 2..=6 per lesson
```

**Checkpoint / Exam item bank** — items are a tagged union. The **stored**
schema always carries a resolved `answerIndex` for MC (MEDIUM-4): authored and
imported bundles must present a validated 0-based `answerIndex`; letter/text
resolution happens only in the deferred generation step (§13), never at
take-time.
```
QuizItem / ExamItem:
  id: Uuid
  conceptIds: [Uuid]              // ≥1 (drives mastery + remediation)
  kind: "mc" | "numeric" | "balance" | "multistep"
  prompt: String
  solution: [{ text, expression?, result?, unit? }]   // full worked solution, shown after answering (§8.3)
  explanation: String            // plain-text summary, shown after answering
  // mc:
  choices: [String]?             // 2..=6, unique
  answerIndex: u32?              // 0-based, REQUIRED for mc; out-of-range fails validation (MEDIUM-4)
  // numeric (see §9 tolerance):
  answer: f64?
  unit: String?
  tolerance: { kind: "exact" | "abs" | "rel", value: f64 }   // REQUIRED for numeric (MEDIUM-5)
  sigFigs: u32?                  // guidance/hint only, not graded
  // balance (MEDIUM-5): tests balancing as a gradeable item
  equation: { reactants:[{formula,count?}], products:[{formula}] }?
  answerCoefficients: [u32]?     // lowest-integer coefficients; graded exact
  // multistep:
  steps: [{ prompt, kind:"mc"|"numeric"|"balance", ...per-kind fields as above }]?
  gated: bool?                   // multistep: later steps locked until earlier correct
```

**Checkpoint** = `{ presentCount: u32 (8..=10), items: [QuizItem] }` where
`items.len() >= 2 * presentCount` (the §10.1b floor; validated at import).
**Exam** = `{ items: [ExamItem], questionCount: u32, passPct: u32,
mix: {mc, numeric, balance, multistep} }` where
`mix.mc + mix.numeric + mix.balance + mix.multistep == questionCount`,
`mix.balance == 0` (NIT-6), `items.len() >= 60`, and the per-kind floors
(≥18 MC, ≥18 numeric, ≥9 multistep) and per-concept coverage quota (§10.1b) all
hold — each asserted by import validation and the seed test.

**Trainer** (parameterized drill spec; difficulty scales with mastery, §8.2)
```
id: Uuid
conceptIds: [Uuid]
generators: [ItemGenerator]        // §8.2: numeric templates with ranges + a difficulty tier map
staticItems: [QuizItem]            // fallback pool when a concept has no generator
```

**Diagnostic** (optional placement pre-test, §8.4)
```
items: [QuizItem]                  // short; each tags concept(s); grading seeds initial mastery
```

**Figure** (manifest-level, bytes on disk; authored per the figure manifest §12.1)
```
id: Uuid
path: "figures/<id>.png"           // inside the bundle
sha256: String
width: u32, height: u32
alt: String                        // required, plain text (a11y + render fallback)
reviewedBy: String                 // authoring-time accuracy-review signoff tag (OQ-9, §12.2)
```

### 3.3 Learner-local records (live in the app DB, not the bundle)

**MasteryRecord** per `(lessonSetId, conceptId)` — EMA plus spaced-repetition
state (§7, §8.1):
```
score: f64 (0..=100)               // EMA, §7
attempts: u32
correctAttempts: u32
firstTryCorrect: u32
mastered: bool                     // score >= 85 AND correctAttempts >= 3
// spaced-repetition (SM-2-lite, §8.1):
ease: f64                          // 1.3..=2.8
intervalDays: f64
dueAt: String                      // when this concept resurfaces for review
lastReviewedAt: String?
updatedAt: String
```

**LessonProgress** per `(lessonSetId, lessonId)`: `{ explainersSeen: [Uuid],
workedExamplesSeen: [Uuid], checkpointBestPct: f64, checkpointPassed: bool,
lastViewedExplainerId: Uuid?, startedAt, updatedAt }`.

**ExamAttempt**: `{ id, lessonSetId, startedAt, submittedAt?, answers:
[{itemId, given, correct, pointsEarned, pointsPossible}], scorePct, passed }`.

**Certificate**: `{ id, lessonSetId, examAttemptId, courseTitle, level,
scorePct, issuedAt, provenance, resultHash }` (§10.4).

---

## 4. The portable `.lesson` bundle format

### 4.1 Container — DECISION (OQ-3)

A `.lesson` is a **zip archive** (chosen over single-JSON-with-base64 and over a
SQLite file). Rationale: zip is portable and inspectable, keeps PNGs as
efficient binary (base64 would bloat ~33% and defeat NFR-5's size cap), avoids
shipping a second SQLite file whose schema would drift from the app's, and lets
import stream-validate per file. Layout:

```
<name>.lesson   (zip, deflate/stored only)
├── manifest.json          // schema version, provenance, per-file hashes, index
├── lessonset.json         // LessonSet (concepts, diagnostic, exam config, lesson order)
├── lessons/<lessonId>.json// one Lesson per file (explainers, examples, misconceptions, trainer, checkpoint)
└── figures/<figureId>.png // embedded PNG assets
```

### 4.2 Manifest schema

```jsonc
{
  "format": "nofriction.lesson",
  "schemaMajor": 1,            // import rejects an unknown major with a plain error
  "schemaMinor": 0,            // a newer minor is accepted (forward-compatible read)
  "lessonSetId": "…uuid…",
  "title": "Stoichiometry",
  "provenance": "seed",        // seed | imported | generated
  "files": [                   // every non-manifest file, with its hash
    { "path": "lessonset.json",       "sha256": "…", "bytes": 12345 },
    { "path": "lessons/<id>.json",    "sha256": "…", "bytes": 6789 },
    { "path": "figures/<id>.png",     "sha256": "…", "bytes": 45678, "kind": "image/png" }
  ],
  "contentFingerprint": "…",   // SHA-256 over the canonical content (§4.4)
  "authoredAt": "2026-10-10T…Z",
  "generator": "nofriction-authoring 1.0"  // informational
}
```

`schemaMajor` and `schemaMinor` live here; `LessonSet.schemaMajor` carries only
the **major** and must equal the manifest's major (validated on import). This
removes the NIT-8 naming inconsistency: one `schemaMajor` everywhere, with the
minor confined to the manifest.

### 4.3 Versioning

`schemaMajor` is the compatibility gate. The app declares the majors it reads
(v1 initially). **Unknown major → import rejected** with
"This lesson needs a newer version of noFriction." (AC 4.2.3). A newer
`schemaMinor` is accepted and unknown fields are ignored (tolerant read), so
additive changes don't break older apps. A bundle is never silently upgraded on
disk; re-export writes the app's current version.

### 4.4 Integrity / fingerprint (SHA)

Two independent checks, both in `sha2`:

1. **Per-file hashes.** Each entry in `manifest.files` carries the SHA-256 of
   its bytes. On import every file is hashed and compared; any mismatch, or any
   file present in the zip but absent from `files` (or vice versa), fails import
   (AC 4.2.3, NFR-6). This catches corruption and tampering of the payload.
2. **Content fingerprint.** `contentFingerprint` = SHA-256 over the
   **canonicalized** content (lessonset + lessons serialized with sorted keys,
   figure bytes folded in by their hashes). This is the stable identity of a
   lesson *version* and is what MasteryRecord/progress rows key against, so
   re-importing the identical set reattaches existing progress, while a changed
   set is treated as a new version. It is also embedded (cosmetically) into the
   certificate's `resultHash` (§10.4, OQ-13).

The manifest itself is not self-hashed (a file cannot contain its own hash);
its integrity is implied because every file it references is hash-checked and
the set of files must match exactly. A zip entry path is validated to stay
within the bundle (no `..`, no absolute paths) so import cannot escape the app
data folder (NFR-6).

### 4.5 Dogfooding: one schema for seed and imported content

The seed stoichiometry set is **authored into the exact same `.lesson` format**
and loaded through the **same import+validate path** as a third-party bundle.
There is no privileged "internal" loader. Concretely: the build step produces
`src-tauri/resources/seed/stoichiometry.lesson`; on first run the app copies it
into the catalog by calling the ordinary `import_bundle` function. If the seed
set fails validation, that is a build-breaking bug (a seed-validation unit test
asserts the shipped bundle passes `validate_bundle` with zero errors, §14). This
guarantees the authored content can never rely on anything the public format
can't express.

### 4.6 Tolerant parse + strict validation of untrusted content (mirrors study)

Every JSON file in a bundle — **including seed** (AC 4.2.4) — is treated as
untrusted on load, exactly as `study::parse` treats model output:

- **Tolerant parse**: `serde_json` with a trailing-comma fallback
  (`lessons::parse::extract_json`, lifted from `study::parse`), unknown fields
  ignored.
- **Strict validation + cleaning** (`lessons::schema::validate_*`): every text
  field cleaned via the shared `clean_text` (trim, strip control chars,
  single-line where applicable, length caps); counts bounded (2..=5 explainers
  per lesson, counting figure and interactive kinds together, **with a hard
  floor of ≥1 `figure`-kind explainer per lesson** whose referenced embedded PNG
  actually decodes (owner-decision D4; §12.1/§14 — a figure-free seed set fails
  validation); a checkpoint bank of `presentCount*2 ..= 40` items; an exam bank of
  `60 ..= 300` items; choices 2..=6, misconceptions 2..=6, etc. — upper bounds
  guard against a hostile oversized bundle, lower bounds are the §10.1b floors);
  referential integrity enforced (every `conceptId`/`figureId`/`lessonId`
  referenced must exist; the concept prerequisite graph must be **acyclic** —
  a cycle fails the whole bundle); numeric fields finite; **every mc item's
  `answerIndex` in range**; **every numeric item carries a required
  `tolerance`**; balance items' `answerCoefficients` are
  lowest-integer and actually balance (recomputed, §6); **item-bank floors**
  (§10.1b: checkpoint ≥2×, exam ≥60 with per-kind + per-concept quotas, `mix`
  summing to `questionCount`, `mix.balance == 0`) hold; **and the required-figure
  floor (owner-decision D4)**: every lesson carries **≥1 `figure`-kind
  explainer**, each such explainer's `data.figureId` resolves to a manifest
  `Figure`, and that figure's embedded PNG **decodes** (the `image` crate reads
  it to a valid raster of the manifest's `width`×`height`) with a byte-hash that
  matches `manifest.files` — a lesson with no figure, a dangling `figureId`, or a
  non-decoding/empty PNG **fails the whole bundle**. A field that fails
  cleaning drops that element; a structurally unusable file fails the whole
  import.
- **Provenance precedence (NIT-10):** schema validation accepts any of the three
  provenance values (`seed` | `imported` | `generated`) **syntactically** — it
  never rejects a bundle for *claiming* a provenance. The authoritative value is
  set at the **catalog-write** step, not at parse: `import_bundle`
  unconditionally overwrites a user-imported bundle's provenance to `imported`
  before persisting (§4.7); seed loading is the only path that persists `seed`;
  `generated` is written only by the deferred generation pipeline (§13). A
  hostile bundle that claims `generated` therefore parses fine and is still
  stored as `imported`, so the "strict validation" and "import forces imported"
  rules never fight.
- **Content rigor for seed (owner-decision D3, "high end"):** the seed-validation test
  additionally recomputes and asserts every explainer/worked-example/item answer
  key is numerically self-consistent (balanced equations balance, molar masses
  correct, dimensional-analysis products match the stated `finalAnswer`). This
  is the mechanical half of the figure/answer-accuracy gate; the scientific
  review half is the human gate in §12.2 (OQ-9).
- **Render as plain text / typed components, never HTML.** React renders every
  authored string as a text node and every explainer from its validated typed
  payload. There is no `dangerouslySetInnerHTML` anywhere in the Lessons code
  path (enforced by a grep test, §14).

A malformed or oversized bundle fails import atomically with an error state and
**never partially corrupts the catalog** (AC 4.2.4): import validates fully in
a temp dir first, then commits in one DB transaction; a failure rolls back and
removes the temp dir.

### 4.7 Provenance

`provenance ∈ {seed, imported, generated}` is recorded in the manifest and the
catalog row and drives purge (§11) and the certificate issuer line (§10.4). It
is set by the producer: the seed build sets `seed`; `import_bundle` from a user
source sets `imported` and **forces the catalog row to `imported` regardless of
the manifest value**, so a hostile bundle can't claim purge-exempt provenance it
isn't entitled to; `generated` is only ever written by the local generation
pipeline (deferred §13) and is **never** accepted from an imported file. In this
build no code path writes `generated`.

---

## 5. Local library / catalog and seed content

### 5.1 No owner server — DECISION (the "download a class" tension)

"Download a class" **cannot** mean an owner-hosted store without violating
CLAUDE.md's no-servers invariant. Resolved exactly as the requirements
recommend: **local catalog + user-supplied import + shipped seed content**. The
Lessons view opens on a **library** of the lesson sets present locally; it is
non-empty on first run because the six stoichiometry lessons ship as seed.
Options considered and rejected: (a) an owner catalog server — rejected, breaks
the invariant; (b) a hardcoded default remote catalog URL — rejected, breaks
the no-default-URL rule (same rule as AI endpoints). Chosen: user supplies the
source.

### 5.2 Catalog storage

A `lesson_sets` catalog table (the app DB) plus bundle files on disk under the
app data folder (FR-DATA-1, OQ-4: files on disk, not DB blobs):

```
<app data>/lessons/<lessonSetId>/
  ├── lessonset.json
  ├── lessons/*.json
  └── figures/*.png
```

`lesson_sets` row: `id, title, subtitle, level, provenance, source_recording_id?,
transcript_fingerprint?, content_fingerprint, schema_major, dir_path,
imported_at`. Learner records (§3.3) reference `id`.

### 5.3 Import from file / folder / URL — DECISION

"Import a lesson…" offers three sources (AC 4.3.2):

- **File**: a `.lesson` the user picks via `tauri-plugin-dialog`
  (`files.user-selected.read-write` already granted in the `mas` sandbox,
  verified at `entitlements.mas.plist:55`, same as study export).
- **Folder**: an unzipped bundle directory (useful for authoring/testing).
- **URL**: the user pastes a URL. This single fetch is the **only** network
  activity, clearly user-initiated (a button, not silent), with **no default
  URL** shipped (AC 4.3.3). After the download the bundle runs fully offline.

All three land in the same validate → commit path (§4.6). URL fetch uses the
existing HTTP client already present for AI calls, restricted to this one
user-initiated download; the policy guard confirms no *default* URL and no
startup/save network (§14, NFR-2).

### 5.4 Seed loading on first run

On startup, `lessons::seed::ensure_seeded(pool)` checks a `lessons_seed_version`
value in the app settings table. If the shipped seed version is newer than what
was loaded (or nothing was loaded), it imports
`resources/seed/stoichiometry.lesson` via the ordinary `import_bundle` path with
provenance `seed`, idempotently (keyed by content fingerprint). If the user
*removed* the seed set deliberately, a tombstone flag prevents re-seeding it on
the next launch (so "remove" sticks). Free users can take the full seed course
offline with no AI configured and no provider active (AC 4.3.4, FR-PRO-3,
FR-OFF-3).

### 5.5 Removing a set

Removing an `imported` or `seed` set deletes only its local catalog copy, its
on-disk dir, and its learner records (progress/mastery/exam/cert). No recording
purge semantics apply (AC 4.3.5, §11). A per-set **Start over** clears learner
records but keeps the bundle (OQ-14).

---

## 6. Interactive explainer / trainer inventory

Every explainer is a **deterministic React component driven by validated
structured data** (never model HTML, AC 4.4.3). Each has: a typed data payload
(validated at import), a **correctness rule computed in shared Rust** (so it is
unit-testable and reusable on iOS), **live correct/incorrect feedback**, a
`prefers-reduced-motion` path, full keyboard operation, and an accessible
**text-equivalent** — a screen-reader description of the *current* interactive
state generated from the component's live values and the `a11yState` template
(WCAG per DESIGN.md; hero interaction per owner-decision D3). The web layer renders; the authored
`data` and the correctness functions are shared Rust (`lessons::explain`),
mirrored in TS **only as a non-authoritative live hint** (MEDIUM-6).

### 6.1 Authored-vs-recomputed correctness contract (the AI-generation precondition, §13)

Because this build ships only human-authored seed content and imported bundles,
every **closed-form** explainer's `data` carries author-provided values that are
**mechanically re-derived and verified in Rust at import** against the
structural inputs — the seed-validation test fails the build on any mismatch
(§4.6, §14). The stored value the UI renders is the author's `data`, but it is
proven consistent with a Rust recomputation, so the UI can never present a wrong
"correct" answer.

**Scope of the "recomputed" guarantee (NIT-8, NIT-9).** The full numeric
re-derivation applies to the closed-form kinds: equation balancer (1), mole-ratio
calculator (2), limiting-reagent simulator (3), dimensional-analysis builder (4),
and ideal-gas PV explorer (7), plus the numeric/balance/multistep items (whose
answer keys are structured and recomputed at authoring/import). Two kinds are **explicitly carved out** of the full-recompute
claim and their scientific correctness is routed to the **§12.2 human review
gate** (same as figures), because their fidelity is not a closed-form
computation:

- **Molecule / reaction diagram (5):** the mechanical check is **structural
  only** (atoms exist, bond order ∈ {1,2,3}, coords in the unit box, labels
  reference existing atoms). Chemical fidelity — valence consistency,
  label-to-atom correspondence, atom conservation across a reaction arrow — is
  **not machine-validated** and is owned by the §12.2 human gate. Where feasible
  the authoring step adds a **flagged** valence check (sum of bond orders per
  atom ≤ the element's typical valence) that warns, not fails.
- **Titration curve (6):** the mechanical check is **monotonicity-in-volume +
  equivalence-point presence**; the curve's physics (buffer region, exact
  equivalence pH from `{conc, volume, ka?}`) is **not** re-derived, so a
  pedagogically-wrong authored curve is caught by the §12.2 human gate, not the
  machine. (A future enhancement may recompute the curve and verify authored
  `points[]` within tolerance; it is not required this build.)
- **Worked examples (§6.2):** a worked example is **prose-authored** — its
  `steps[]` are free-text teaching narration with no machine-parseable
  structural inputs, so Rust validation checks structure (non-empty prompt,
  ≥1 step, a finite `finalAnswer.value`) rather than re-deriving the final
  answer. The worked-example numbers are instead verified at **authoring time**
  (the `author_seed_lessons` tool computes them from the same shared
  molar-mass table and balancer the runtime grader uses) and by the §12.2 human
  review gate. An imported bundle's worked examples are trusted as authored
  prose, consistent with their structure-only validation.

Every other kind below is fully recomputed.

The table below is the single uniform rule — **kind → structural input Rust
trusts → value Rust recomputes and verifies → drop/fail condition.** The
*deferred* generation pipeline (§13) inherits this table as a hard precondition:
when AI generation is built, the shared Rust function must **recompute and
replace** the model's numeric outputs from the structural inputs (never store
the model's own numbers), and any explainer whose inputs don't yield a valid
computation is dropped before the lesson is marked playable (HIGH-2 precondition,
restated in §13).

| # | Explainer / item | Structural input Rust trusts | Value Rust recomputes & verifies | Drop / fail condition | Hero interaction (owner-decision D3) |
|---|---|---|---|---|---|
| 1 | **Equation balancer** | `reactants[].formula`, `products[].formula`, element set | Lowest-integer balanced coefficients via integer linear solve over per-element atom counts (atom balance only — the seed set is neutral species, so charge is not part of the system); the authored `answerCoefficients` must equal them | Unbalanceable system, or authored coefficients ≠ computed | **Live per-element mass-balance meter** (one bar per element, left vs right atom count) + a particle-count visualization that updates as the learner sets coefficients; balanced only when every bar matches and charge nets to zero |
| 2 | **Mole-ratio / stoichiometry calculator (step reveal)** | `given {amount,unit,species}`, `want {unit,species}`, balanced `equation` | Each step value from the previous step (molar mass, mole ratio); final answer | Any recomputed step ≠ authored step beyond tolerance, or final ≠ `finalAnswer` | Steps reveal one at a time (Enter/→), each with the live dimensional cancellation shown |
| 3 | **Limiting-reagent simulator** | `equation`, `reagents[] {species,molarMass,available}`, `productSpecies` | moles per reagent, limiting reagent = min(amount/coeff), theoretical yield | Non-finite inputs, no limiting reagent resolvable | **Before/after consumption bars**: each reactant's bar drains as the reaction proceeds, showing leftover of the excess reagent and product formed |
| 4 | **Dimensional-analysis "railroad" builder** | `start {value,unit}`, `target {unit}`, `tiles[] {num,den}` | Symbolic unit-multiset cancellation + numeric product | Units don't reduce to `target.unit`, or product ≠ authored value beyond tolerance | Drag tiles onto the track; **live cancellation strike-through** of matched units and a running product |
| 5 | **Labeled molecule / reaction diagram** | `atoms[] {id,element,x,y}`, `bonds[] {a,b,order}`, `labels[]` | **Structural validity only** (atoms exist, bond order ∈ {1,2,3}, coords in 0..1, labels reference existing atoms); chemical fidelity → §12.2 human gate (NIT-8); optional flagged valence check | Dangling bond, bad order, coord out of box, label to a missing atom | Rendered SVG from structured data; hover/focus reveals labels (keyboard-navigable atom list) |
| 6 | **Titration curve explorer** | `acid/base {conc,volume,ka?}`, `points[] {volAdded,pH}`, `equivalenceVol` | **Monotonic-in-volume + equivalence-point present** only; curve physics → §12.2 human gate (NIT-9), not re-derived | Non-monotonic points, non-finite pH, missing equivalence point | **Draggable titrant slider** moves the live pH point along the authored curve; equivalence point marked; reduced-motion = stepped |
| 7 | **Ideal-gas PV explorer** | `n, T, bounds, R` | Enforces PV = nRT; dependent variable from the two the learner sets | Non-finite / non-positive constants | **Two coupled variables**: learner drags two of P/V/n/T, the others update live on the curve |
| 8 | **Misconceptions ("common mistakes")** | `items[] {claim,why_wrong,correct}` | None (text) | Empty items, or >6 | Flip/reveal cards: tempting wrong claim → why it's wrong → correct approach (keyboard-operable, reduced-motion = instant) |

**`balance` quiz/exam items (MEDIUM-5):** balancing is tested as a first-class
gradeable item kind (`kind: "balance"`), not shoehorned into `numeric`. The item
carries the `equation` and `answerCoefficients`; grading recomputes the balanced
coefficients in Rust (`lessons::grade::check_balance`) and compares exactly
(order-independent, lowest-integer). This is why §9's numeric tolerance never
has to approximate an integer coefficient.

**Figures** (the second visual mechanism, AC 4.4.2.ii) are a **required** part of
the shipped seed set (owner-decision D4), not an optional or stretch extra:
every seed lesson embeds **≥1** `figure`-kind explainer. They are static PNGs
authored at authoring time with the genmedia MCP image tool (gemini-3-pro-image
/ "Nano Banana Pro"), art-directed to a consistent visual language (hazard
yellow on matte black, clean labeled scientific style, legible at 2×; §12.1),
reviewed for accuracy (§12.2, OQ-9), embedded in the bundle, and rendered as
`<img>` from bundle bytes. The shipped app **never calls an image API at
runtime** (FR-FIG-2). A `figure` explainer always carries required `alt` text so
it is accessible and so that **at runtime** a transiently failed decode degrades
gracefully to its caption rather than breaking the lesson. That runtime
graceful-degradation is independent of the **authoring-time** guarantee: seed
validation (§4.6/§14) fails the build if any seed figure PNG does not decode, so
the shipped seed figures are proven-present and proven-valid before release — the
caption fallback is a defensive runtime path, never an excuse for a figure-free
or broken-figure seed bundle. Deterministic SVG explainers (the kinds 1–7 in the
table above) are **additive** to these authored figures, never a replacement for
them (owner-decision D4).

---

## 7. Mastery model (per concept)

`MasteryRecord.score` is an **exponential moving average** on 0..100 (the
requirements' recommended rule, made concrete; authoritative in
`lessons::mastery`, Rust):

- On each graded attempt of an item tagging concept `c`, compute an
  `itemScore ∈ {0..100}` (MC/numeric/balance: correct=100, wrong=0; multistep:
  per-step fraction × 100).
- `score ← α·itemScore + (1−α)·score`, with **α = 0.4**. First attempt seeds
  `score = itemScore`.
- **First-try-correct bonus**: an item answered correctly on the first attempt
  (no retry within the session) counts `itemScore = 100` and increments
  `firstTryCorrect`; a correct answer after a wrong one counts `itemScore = 70`
  so mastery reflects genuine recall, not retry grinding.
- `attempts` counts graded attempts; `mastered = score ≥ 85 AND
  correctAttempts ≥ 3`.

The constants (α = 0.4, retry credit 0.7, mastered threshold 85, min 3 correct)
are one Rust constants block (`lessons::mastery`) and unit-tested with scripted
attempt sequences. The TS mirror (`src/lib/lessonLogic.ts`) is live-preview only;
the stored value is always written by Rust via a command.

---

## 8. Learning engine (depth) — spaced repetition, adaptive trainers, worked solutions, diagnostic

All of §8 is **local and offline**. It is the depth that makes Lessons worth
using (owner-decision D3, "finish and enhance / make this high end").

### 8.1 Spaced-repetition review (SM-2-lite over the mastery EMA)

Once a lesson's checkpoint is passed, its concepts enter a lightweight review
schedule layered on the existing per-concept `MasteryRecord` (no new store):

- Each concept carries `ease` (1.3..=2.8, start 2.5), `intervalDays` (start 0),
  and `dueAt`. On a review attempt, grade quality `q ∈ {0..5}` is derived from
  the item result and whether it was first-try correct.
- **SM-2-lite update** (`lessons::review`): on a passing attempt,
  `ease ← clamp(ease + (0.1 − (5−q)·(0.08 + (5−q)·0.02)), 1.3, 2.8)`;
  `intervalDays ← if firstReview {1} else if second {3} else intervalDays·ease`;
  `dueAt ← now + intervalDays`. A failing attempt resets `intervalDays` to 1 and
  nudges `ease` down (floored at 1.3). The algorithm is deterministic and
  unit-tested with scripted (concept, quality, day) sequences.
- A **"Review due"** surface in the Lessons view lists concepts whose `dueAt`
  has passed, newest-weakest first, and launches a targeted trainer (§8.2) over
  exactly those concepts. All computed from local records; zero network.

### 8.2 Adaptive trainers with difficulty scaling

A trainer targets the learner's **weakest unmastered concepts first**
(AC 4.5.3) and **scales item difficulty by current mastery** (owner-decision D3).
Algorithm (`lessons::trainer::next_item`):

1. Rank the lesson set's concepts by `(mastered asc, score asc, dueAt asc)`;
   take the lowest unmastered (or most-overdue) concept as the focus.
2. Choose a **difficulty tier** from the focus concept's mastery:
   `score < 40 → tier 1 (easy)`, `40..70 → tier 2 (medium)`,
   `≥ 70 → tier 3 (hard / multi-step)`.
3. Draw an item for that concept, preferring a **parameterized generator** so
   drills aren't identical repeats. A generator is a numeric template with a
   per-tier range/shape map, e.g.
   `{ template: "mole_mass", tiers: { 1:{grams:[1,20],species:["H2O"]},
   2:{grams:[1,80],species:["H2O","CO2","NaCl"]},
   3:{chain:["mole_ratio","grams"], grams:[1,120]} }, computeAnswer:<builtin id> }`.
   The builtin compute functions are a small fixed set in Rust
   (`lessons::generators`) — mole↔mass, mole-ratio, limiting-reagent, gas-law —
   each with a verified closed-form answer, so a generated item's answer is
   always correct by construction. Values are drawn with a **seeded RNG**
   (seed = `(conceptId, attemptIndex)`), so a drill is reproducible and
   unit-testable, including the tier scaling.
4. If a concept has no generator, fall back to `Trainer.staticItems`.
5. Grade with the same tolerance rules as the exam (§9), reveal the full worked
   solution (§8.3), update mastery (§7) and the review schedule (§8.1), and
   continue until the focus concept is mastered or the learner stops.

The generator template ids are a closed enum validated at import (an unknown
template id drops that generator).

### 8.3 Full worked-solution reveal on every item (owner-decision D3)

After any trainer, checkpoint, or exam item is answered, the UI reveals the
**step-by-step solution with the specific numbers for that item** — not just
right/wrong. For authored items the solution is the item's `solution[]` steps
(§3.2). For generator-produced items, the same Rust compute function that
produced the answer also emits the ordered dimensional-analysis steps
(`lessons::generators::solve_with_steps`), so a miss on a freshly-drawn drill
still teaches with the actual numbers. All step text is plain text / typed, never
HTML.

### 8.4 Diagnostic pre-test (optional, offline)

A lesson set may carry an optional `Diagnostic` (§3.2): a short placement check.
Taking it is optional and offered on first open of a set. Grading seeds initial
`MasteryRecord.score` per tested concept (`lessons::mastery::seed_from_diagnostic`)
so a strong learner can **test out** of early concepts — a concept seeded
`≥ 85` with the diagnostic's correct-count counts as provisionally mastered and
its lesson is marked optional (the checkpoint still gates the exam, §10.1, so
testing out never bypasses the exam-unlock rule). Fully local; no network.

---

## 9. Numeric answer checking (shared Rust, tolerant) — DECISION (OQ-12)

Numeric free-response and the numeric legs of multistep items are graded by one
shared function, **authoritative in Rust** (`lessons::grade::check_numeric`),
mirrored in TS only for live hints (MEDIUM-6):

```
check_numeric(given: &str, item: &NumericItem) -> Grade
```

1. **Parse** `given`: strip whitespace; accept plain decimals, scientific
   notation (`6.02e23`, `6.02×10^23`, `6.02 x 10^23`), leading `+`/`-`, and an
   optional trailing unit token. Reject anything else as unparseable (shown as
   "couldn't read that number", never a crash/panic).
2. **Normalize units (DECISION — prefixed/scaled units convert, finding 1).**
   The canonical-unit map is not just synonym folding; each known unit carries a
   **dimension class** and a **scale factor + offset** into that class's base
   unit, so a chemically-correct answer in a compatible prefixed unit is
   converted before comparison instead of being silently marked wrong. The map:

   | Dimension | Base | Members (→ base conversion) |
   |---|---|---|
   | mass | g | `g`/`grams` ×1, `mg` ×1e−3, `µg`/`ug` ×1e−6, `kg` ×1e3 |
   | volume | L | `L`/`liters` ×1, `mL`/`ml` ×1e−3, `dL` ×0.1, `cm³`/`cm3`/`cc` ×1e−3 |
   | amount | mol | `mol`/`moles`/`mole` ×1, `mmol` ×1e−3 |
   | pressure | atm | `atm` ×1, `kPa` ×(1/101.325), `mmHg`/`torr` ×(1/760), `Pa` ×(1/101325) |
   | temperature | K | `K` ×1, `°C`/`C` → `K` via `+273.15` (offset, not a factor) |

   Grading logic when the item declares a `unit`:
   - **Compatible, known unit** (same dimension class as the item unit): convert
     the given value to the item unit using the factor/offset, then compare the
     converted magnitude within tolerance (step 3). So `4000 mg` grades equal to
     a `4 g` key, `250 mL` to `0.25 L`, `25 °C` to `298.15 K`.
   - **Dimensionally incompatible** (e.g. grams given, liters expected): **wrong**.
   - **Present but not in the map** (an unrecognized token like `foo`, or a
     compatible-looking unit the map doesn't carry): **not silently graded** —
     returned as unparseable with "couldn't read that unit", never compared as a
     bare magnitude. This closes the undefined branch finding 1 identified.
   - **Absent unit on the answer**: accepted (the prompt states the expected
     unit); the magnitude is compared **in the item's declared unit** (no
     conversion, since there is no given unit to convert from).

   The map is a single Rust table (`lessons::grade::UNITS`), unit-tested; adding a
   unit is a one-line table entry. `°C→K` is the only offset conversion and is
   handled explicitly (never a pure multiply).
3. **Compare within the item's REQUIRED tolerance** (MEDIUM-5 — there is **no
   silent ±1% default**): `tolerance.kind` is
   - **`exact`** → the parsed value must equal `answer` exactly after
     normalization (this is the **default an author uses for integer/exact
     answers**; validation requires every numeric item to declare a tolerance,
     and exact-valued items declare `exact`);
   - **`abs`** → `|given − answer| ≤ value`;
   - **`rel`** → `|given − answer| ≤ value·|answer|`, used only where the item
     explicitly declares it (e.g. a value derived through several multiplications
     where sig-fig rounding is expected).
   `sigFigs` is **guidance only**, surfaced as a hint, never graded (a correct
   value with extra digits is accepted).

Balancing is never graded here — it is a `balance` item graded by
`check_balance` (§6). The parser and comparator are unit-tested against a table
of (input, item, expected grade) including hostile inputs (empty, letters,
overflow, `NaN`, enormous exponents) — none may panic.

---

## 10. Final exam, mastery dashboard, remediation, and certificate

### 10.1 Final exam — DECISION

- **30 questions** total (`Exam.questionCount = 30`), drawn from the exam item
  bank spanning all six lessons, with the exact mix **`Exam.mix = { mc: 12,
  numeric: 12, balance: 0, multistep: 6 }`** (AC 4.5.4). Multi-step items chain
  concepts across lessons (e.g. limiting reagent → percent yield → gas volume at
  STP), calibrated to AP Chem / first-year college difficulty (owner-decision
  D3).
- **Standalone `balance` items are NOT part of the exam (NIT-6):**
  `Exam.mix.balance = 0`, and validation asserts
  **`mix.mc + mix.numeric + mix.balance + mix.multistep == questionCount`**
  (12+12+0+6 = 30). Balancing is tested **inside multi-step chains** (a
  "balance → mole ratio → grams" leg) and as checkpoint `balance` items, never
  as a standalone exam question. This removes the mix-accounting ambiguity.
- **Pass = 80% of exam points** (AC 4.5.5). MC/numeric/balance legs are 1 point;
  a multi-step is worth its step count, scored per step (or as a gated chain
  when `gated=true`: a wrong early step zeroes the dependent steps).
- **Gate (OQ-6): the exam unlocks only when all six checkpoints are passed**
  (each ≥ 80%; see §10.1a). A dev-only settings override exists for testing; it
  is not shown in release UI.
- On each attempt, question **selection re-randomizes** from the over-provisioned
  bank (§10.1b) and numeric items with generators **draw fresh values**
  (AC 4.5.7), so a retake is genuinely a different paper.
- Everything the exam needs is **in the bundle**, so it runs fully offline
  (AC 4.5.8, FR-OFF-1). Grading is authoritative in Rust (`lessons::exam`).

### 10.1a Checkpoint gating, failure, and remediation (MEDIUM-4)

The lesson-player state machine for a checkpoint result is defined concretely so
an implementer can build it without inventing policy:

- **A checkpoint below 80% does NOT block forward lesson navigation.** Lessons
  are explorable in any order (the library and the player both allow jumping to
  any lesson). A failed checkpoint leaves `LessonProgress.checkpointPassed =
  false` and marks that lesson **"needs review"** in the library and the lesson
  header.
- **It DOES keep the final exam locked.** The exam-unlock predicate is
  `all six lessons have checkpointPassed == true` (each earned at ≥80% on some
  attempt; the best attempt counts via `checkpointBestPct`). A single
  sub-80% checkpoint ⇒ learner is **not exam-eligible** until it is retaken and
  passed. This is the only gate; there is no separate "lesson complete" gate.
- **Remediation on a sub-80% checkpoint:** the player surfaces a **"Review these
  concepts"** list that deep-links, for each **missed** item, to the first
  explainer tagged with that item's `conceptIds` (the item → concept → explainer
  map is exact and unit-tested, §10.3). It also offers a **targeted trainer**
  (§8.2) over exactly those weak concepts and a **"Retake checkpoint"** action.
- **Retake behavior:** "Retake checkpoint" **re-draws** the 8–10 presented items
  from the checkpoint's over-provisioned authored bank (§10.1b), **preferring
  items not shown on the immediately previous attempt** where the bank allows,
  so a retake is not the same paper. Checkpoints are **selection-only** (a fixed
  authored bank, no generators) — retake variety comes entirely from the ≥2×
  bank floor. The best `checkpointBestPct` across attempts is retained; passing
  once (≥80%) permanently sets `checkpointPassed = true` and clears the "needs
  review" flag, and its concepts enter the spaced-repetition schedule (§8.1).

### 10.1b Authored item-bank floors so re-randomization is real (MEDIUM-5)

Re-randomization is meaningless if a bank holds exactly the presented count, so
the seed-validation test (§15) enforces concrete **over-provisioning floors**;
the shipped bundle cannot silently fall to the minimum:

- **Checkpoint bank:** each lesson's checkpoint authors **≥ 2× its presented
  count** — i.e. **≥ 16 items for an 8-item checkpoint, ≥ 20 for a 10-item
  one** — with every tested concept covered by at least two items so a retake
  can swap items without dropping concept coverage.
- **Balance-kind coverage in the checkpoint bank (review Finding 4):** every
  concept whose explainer `kind` is `equation_balancer` must be covered by
  **≥ 1 `balance`-kind quiz item** in the checkpoint bank of the lesson that
  teaches it. Concretely, **Lesson 2 (balancing) authors ≥ 1 `balance` item** in
  its checkpoint bank, so balancing is actually tested as a gradeable `balance`
  item and never only via MC. The seed-validation test (§15) asserts this; a
  balancing checkpoint with zero `balance` items fails the build.
- **Exam bank:** the exam authors **≥ 60 items total** (**≥ 18 MC, ≥ 18 numeric,
  ≥ 9 multistep** — each ≥ 1.5× the drawn mix of 12/12/6), spanning **all six
  lessons**, with a **per-concept coverage quota**: every exam-relevant concept
  is tagged by **≥ 2** bank items so the draw can honor concept balance while
  re-randomizing. Selection is **without replacement across the two most recent
  attempts** where the bank size allows, so consecutive retakes differ.
- **Generator-backed vs selection-only:** **numeric** exam/checkpoint items that
  reference a trainer generator (§8.2) draw **fresh values** every attempt (the
  strongest form of variation). **MC** and **multistep** items are
  **selection-only** (drawn from the authored bank; variation comes from the
  floor). `balance` legs inside multistep use generator-drawn coefficients where
  a generator is attached, else selection-only. The seed-validation test asserts
  both floors (checkpoint ≥2×, exam ≥60 with the per-kind and per-concept quotas)
  so a thin bank fails the build.

### 10.2 Scoring and mastery coupling

`lessons::exam::score_attempt` grades every item (MC via the stored
`answerIndex`, MEDIUM-4; numeric via §9; balance via §6; multistep per step),
writes an `ExamAttempt`, updates per-concept mastery (§7) and the review schedule
(§8.1) from the item results, reveals the full worked solution per item (§8.3),
and returns the breakdown.

### 10.3 Mastery dashboard, exam-readiness, remediation

A proper **mastery dashboard** (owner-decision D3) shows per-concept mastery bars,
per-lesson completion, a **streak** (consecutive days with a review/trainer
session, computed from local timestamps), an **exam-readiness meter**, and a
**what-to-study-next** list. The readiness meter predicts pass-likelihood from
current mastery: `readiness = mean(min(score/85, 1))` weighted by each concept's
exam-item count, surfaced as a 0–100% band with the plain-language label
"not ready / almost / ready" (deterministic, unit-tested; `lessons::readiness`).
It never claims a guarantee — it is a local estimate.

**Remediation routing**: on a miss, "Review" on a missed exam/checkpoint item
deep-links to the first explainer tagged with that item's `conceptIds`, then
offers a targeted trainer on that concept (§8.2) and a retake (AC 4.5.1,
AC 4.5.7). Because every item carries `conceptIds` and every concept maps to the
lessons/explainers that teach it, the mapping item → concept → explainer is
exact and unit-tested.

### 10.4 Certificate — DECISION (OQ-13), upgraded (owner-decision D3)

On pass, the app issues a **polished, printable/exportable certificate**,
produced **locally and offline** (AC 4.5.6):

- Content: course title, level, score %, date, the **issuer/provenance** line
  (`Seed content` / `Imported lesson` / `Generated from your recording` — the
  last reserved for the deferred path), and a locally-computed **verification
  hash** (integrity stamp, not server-verified).
- **Cosmetic + local, no server verification** (OQ-13): `resultHash` = SHA-256
  over `(contentFingerprint, scorePct, issuedAt, lessonSetId)`, clearly labeled
  as a local record, not a third-party attestation.
- **Rendering/export**: a printable React view styled in the hazard-yellow
  design language, exported via the OS **print-to-PDF** and via the save dialog,
  matching the study export pattern (`files.user-selected.read-write` in the
  `mas` sandbox; the web view never passes a path to write — the file is built
  from stored data). On iOS (when built) it uses the share sheet from in-memory
  data.

---

## 11. Pro-gating, offline, and purge

### 11.1 Pro gating (FR-PRO)

- **Taking** a present lesson (seed or imported), **importing** a bundle, the
  diagnostic, trainers, review, mastery dashboard, exam, and certificate are all
  **free and offline** (FR-PRO-2). The six seed lessons — including exam and
  certificate — are fully usable by free users (FR-PRO-3).
- **Generating** a lesson from a recording *will* require **Pro** through
  `ai::client::complete` (which already performs `entitlement::require_pro().await`
  first — verified at `ai/client.rs:644` — then endpoint policy, then consent,
  the identical chain study guides use). In **this build the Generate affordance
  is a non-AI stub** (§12.3) and performs no entitlement check and no AI call; it
  is a visible preview, not a gated feature that looks broken (owner-decision
  D2). The Pro → configured → consent gate is specified in the deferred §13 so it
  is added unchanged when the pipeline is built (FR-PRO-1). When built, Making a
  lesson is a Pro feature per `docs/PRO.md` and COORDINATION.md §3 (taking,
  mastery, exam, and certificate stay free).

### 11.2 Offline (FR-OFF)

- Taking a lesson end-to-end does **zero network** once the bundle is present:
  no AI, no image fetch, no font/CDN fetch, no telemetry (AC 4.2.2). The bundle
  carries everything, including figure PNGs; fonts come from the app's bundled
  Inter, never a CDN (FR-OFF-2). Spaced repetition, trainers, diagnostic,
  dashboard, exam, and certificate are all local.
- Only acquisition (a user-initiated URL import, §5.3) — and, in the deferred
  §13, generation — touch the network. Proven by a test (OQ-11, §14).

**Frontend-path enforcement of zero runtime network (owner-decision D5,
revision-5 review Finding 2).** The Rust network-stub take-test (OQ-11, §15)
proves the *backend* makes no AI call, but it cannot prove the React/CSS render
path issues no outbound request (a CDN `@font-face`, a remote `<img>`/`<link>`,
an analytics beacon, a remote SVG `xlink:href`). Three mechanisms close that
gap, with one named **authoritative**:

- **Authoritative: a build/test grep guard.** A test (`scripts/
  check-lessons-offline.*`, run in CI alongside the policy guard, §15) scans the
  **compiled** Lessons JS bundle (the Vite build output for the
  `src/components/lessons/**` + `src/lib/lessonLogic*` surface) and the compiled
  `Lessons.css` for any **non-local** `http(s)://` reference or any `url(...)`
  pointing at a remote host, and **fails the build on a match**. Local/relative
  references (`url(./…)`, `url(data:…)`, Tauri asset-protocol/`tauri://` URLs,
  and bundled font paths) are allow-listed; anything resolving to a remote host
  fails. This is the single source of truth for "the Lessons frontend fetches
  nothing remote," because it inspects the actual shipped bytes rather than
  source intent. It complements `check-ai-provider-policy.py` (which covers AI
  hosts / default URLs / startup-save network, not arbitrary outbound URLs in
  the Lessons bundle) — the two guards together cover both backend and frontend.
- **Defense-in-depth: a Tauri CSP note.** The Lessons surface is served under a
  Content-Security-Policy that restricts `connect-src`, `img-src`, and
  `font-src` to `'self'` plus the Tauri asset protocol (and `data:` for inline
  figures where used), with **no remote origin** allowed. A stray remote request
  that somehow slipped past the grep guard is then **blocked at runtime** by the
  webview. (The CSP is a backstop; the grep guard is the authoritative gate
  because it fails the build rather than merely blocking at runtime.)
- **Assertion: `Lessons.css` uses only the bundled Inter font and imports no
  remote asset.** The grep guard's CSS pass specifically asserts there is **no
  `@import url(http…)`**, no remote `@font-face src: url(http…)`, and that every
  `font-family` resolves to the app's **bundled Inter** (loaded from the app's
  local font asset, never a Google Fonts / CDN URL), matching FR-OFF-2.

### 11.3 Both Mac flavors

- **default (Developer ID DMG)** and **`--features mas` (sandboxed)** both build
  and test green. The Lessons path uses no ffmpeg/Accessibility/osascript/shell
  and reads only the bundled seed resource and user-selected files, so it is
  sandbox-clean. Figure PNGs are read from the app data folder and from
  user-selected files (sandbox-granted), never from arbitrary paths. The `zip`
  crate is pure-Rust (§1), so `mas` needs no extra entitlement or system lib.

### 11.4 Purge (FR-PURGE) — the seed/imported vs generated distinction

**Provenance decides purge behavior.** In this build no `generated` set is ever
produced, so purge of generated lessons is **dormant but fully wired** so the
deferred pipeline (§13) is safe the moment it lands and the backup tests pass
now.

- A **`generated`** lesson set is recording-derived: it would paraphrase a user
  recording exactly like a study guide, so it is **deleted, not rewritten**, on
  any Delete or Strike of that recording's transcript text (words, lines, time
  ranges) (FR-PURGE-1). Hook: `redaction::redact_ai_outputs` already calls
  `study::purge_for_meeting` (verified `redaction.rs:667`); we add
  `lessons::purge_for_meeting(conn, meeting_id)` right beside it for the **live**
  path. It runs `DELETE FROM lesson_sets WHERE source_recording_id = ? AND
  provenance = 'generated'` plus its learner records and on-disk dir. Screen-only
  edits keep it.
- **Backup purge across ALL job kinds (MEDIUM-3).** The live `redact_ai_outputs`
  call does not reach every backup job arm: in `redaction.rs::purge_one_backup`
  the match over `BackupJob` has three arms — `Words` (which *does* call
  `redact_ai_outputs`, `redaction.rs:1283`), `Screens` (`:1308`) and
  `RangeExtras` (`:1316`) — and the latter two do **not** call it. A time-range
  Delete or a whole-recording Delete therefore would not purge a generated set
  from backup databases under a Words-only hook. **Fix:** add a single
  job-kind-independent step inside `purge_one_backup`, **after the `for job in
  jobs` match and before `tx.commit()`**, that calls
  `lessons::purge_for_meeting(&mut *tx, meeting_id)` once per backup transaction.
  Because it runs regardless of arm, every backup-reaching Delete/Strike
  (`Words`, `Screens`, `RangeExtras`, and the whole-recording job list) purges a
  generated set from that backup. This is the exact function and the exact
  insertion point the backup test asserts against (§14).
- Mirroring `study::save_materials`, a `generated` set still being generated is
  **not saved** if the transcript fingerprint changed meanwhile: the save
  re-checks the fingerprint and pending deletes inside `BEGIN IMMEDIATE`,
  refusing during a Delete's undo window (FR-PURGE-2). The same `BEGIN IMMEDIATE`
  + fingerprint-recheck code from `study.rs::save_locked` (verified `study.rs:225`)
  is reused. (Dormant until §13.)
- Deleting a whole recording cascades to its `generated` lessons (FR-PURGE-3)
  via `DatabaseManager::delete_meeting` (verified `database.rs:1337`). **Exact
  ordering and atomicity for the live path (MEDIUM-3).** `delete_meeting` is
  non-transactional: it deletes transcripts by raw SQL first
  (`database.rs:1338`), then runs `meeting_links::purge_meeting`, then
  `topics::purge_for_meeting` / `chat::purge_for_meeting` on a freshly acquired
  connection (`:1350–1351`), then deletes the `meetings` row. We slot the
  lessons purge beside the topics/chat calls, and `lessons::purge_for_meeting`
  itself performs its steps in this order so a partial failure can never leave a
  dangling on-disk dir or orphaned learner rows:
  1. **Resolve first:** `SELECT id, dir_path FROM lesson_sets WHERE
     source_recording_id = ? AND provenance = 'generated'` — read the dir paths
     and ids **before** deleting any row (the row's `dir_path` is the only way to
     find the on-disk figures).
  2. **Delete DB rows next:** delete the learner records (`lesson_progress`,
     `mastery_records`, `exam_attempts`, `certificates`) for those set ids —
     so **in-progress attempts and mastery for a generated lesson cascade with
     it** — then delete the `lesson_sets` rows. These are plain `DELETE`s keyed
     by set id / `source_recording_id`.
  3. **Remove on-disk dirs last:** `remove_dir_all(<app data>/lessons/<id>/)` for
     each resolved dir, **tolerating an already-absent dir** (an `ENOENT` is not
     an error). A dir-removal failure *after* the DB rows are gone is
     **logged and ignored** (never surfaced to the user, never blocks the
     recording delete); the orphaned bytes are harmless and the next catalog
     reconcile sweeps them.
  Because every step is **idempotent and keyed by
  `meeting_id`/`source_recording_id`**, the non-transactional sequencing of
  `delete_meeting` is acceptable: re-running the purge after any partial failure
  converges to the same clean state. A **`seed` or `imported` set is never
  touched** by this path — the `WHERE provenance = 'generated'` clause is the
  guard (FR-PURGE-4), so a recording delete can only ever remove recording-
  derived lessons. The identical resolve→delete-rows→remove-dirs ordering is used
  by the standalone Strike/Delete hook (`redact_ai_outputs`) and the backup hook
  (`purge_one_backup`); only the connection/transaction they run on differs.
- **`seed` and `imported` sets are NOT recording-derived and are exempt from
  recording purge** (FR-PURGE-4). The `WHERE provenance = 'generated'` clause
  guarantees a Strike/Delete of a recording can never touch a seed or imported
  set. Surfaced in the UI (a generated set would show "Generated from
  <recording>"; seed/imported show their provenance and are untouched by
  recording edits) and documented in `docs/REDACTION.md`'s purge checklist
  (extend step 5 — §16).
- Learner progress/mastery/exam/cert for a `generated` set are deleted with it;
  for seed/imported they live until the user removes the catalog entry
  (FR-PURGE-5).
- **Sync**: lessons are **not synced in v1** (OQ-10, FR-PURGE-6). If added later,
  a struck/deleted-recording tombstone triggers the receiver's own purge of its
  `generated` lesson and removed transcript text is never sent — the SYNC.md
  rule, not a shortcut.
- **Search index** (OQ-8, FR-DATA-3): `generated`-lesson text is **not** added to
  the recordings FTS index in v1 (limits purge surface, avoids the
  no-update-trigger FTS pitfall noted in REDACTION.md).

---

## 12. Figures (authoring-time only) and the Make lesson / Generate stub

### 12.1 Figure authoring invariant + manifest (FR-FIG, owner-decision D3)

Figure PNGs are produced with the genmedia MCP image tool
(gemini-3-pro-image / "Nano Banana Pro") **at authoring time only**, during the
offline build of the seed bundle (FR-FIG-1). The flow is an **authoring script**
(`scripts/author-seed-lessons.*`, not shipped in the app binary) that, for each
figure: reads a **figure manifest** entry (so figures are reproducible), calls
the image tool with the art-direction prompt, routes the output through the
accuracy-review gate (§12.2), optimizes it (downscale/compress to meet NFR-5,
target < ~40 MB total for the six-lesson seed set), and writes it into
`resources/seed/stoichiometry.lesson`.

The **authoring-time figure manifest** (`scripts/seed/figures.manifest.json`,
checked in, not shipped in the app) records per figure:
`{ id, prompt, model: "gemini-3-pro-image", artDirection, targetSize,
reviewedBy, sha256 }`. Art direction is one shared style string (hazard yellow
`#facc15` on matte black, clean labeled scientific illustration, legible at 2×,
sentence-case labels) so the set has a consistent visual language matching
DESIGN.md. Re-running the script with the same manifest reproduces the same
figures.

The **shipped app contains no image-generation client and makes no image API
call at runtime, in any flavor** (FR-FIG-2); `check-ai-provider-policy.py` must
still pass (no new AI host, no credential literal, no default URL, no provider
active at first run). Figures render as `<img>` from bundle bytes, never
model-HTML (FR-FIG-3).

**Figures are required, not stretch (owner-decision D4).** The shipped seed set
**must** embed authored PNG figures (≥1 figure-kind explainer per lesson, each
backed by a decoding PNG), enforced by seed validation (§4.6/§14). There is **no
"all-deterministic-SVG seed is shippable" escape hatch**: if the genmedia tool
is unavailable in the authoring environment, authoring is **blocked until the
figures are produced** (by re-running the authoring script when the tool is
available, or by an authored-and-reviewed PNG from an equivalent approved
source), because a seed bundle without decoding figure PNGs fails
`validate_bundle` and therefore fails the build. The deterministic SVG explainers
(§6 kinds 1–7) are **additive** — they render alongside the figures from
structured data — and do **not** substitute for the authored figures. The
shipped app is identical in all cases — it never generates images at runtime
(FR-FIG-2); the figure-accuracy review gate (§12.2) always applies.

### 12.2 Figure accuracy review (OQ-9) — explicit human gate + mechanical half

OQ-9 is resolved as a **two-part gate with no surviving ambiguity**:

- **Mechanical half (automated, enforced now):** the seed-validation test
  (§14, §4.6) fails the build if any seed `Figure.reviewedBy` is empty, if any
  explainer/worked-example/item answer key is missing or **does not match the
  Rust recomputation** (balanced equations balance, molar masses correct to 2
  decimals, dimensional-analysis products equal the stated `finalAnswer`). So
  scientific *self-consistency* of the answer keys and the presence of a review
  signoff are mechanically guaranteed.
- **Human half (not automatable, stated plainly):** *scientific correctness of
  the figures' depiction* (a diagram showing the right molecule, the right curve
  shape) is a **human review** owned by the **content step's named reviewer
  role** ("seed content reviewer"), recorded against a checked-in checklist at
  `docs/LESSONS_AUTHORING.md` (created by the content step) and stamped into
  each figure's `reviewedBy`. The design does **not** and **cannot** automate
  judgment of a drawing's scientific fidelity; it makes the signoff mandatory
  and mechanically present, and names who owns the judgment. This removes the
  OQ-9 ambiguity the review flagged.

### 12.3 The Generate affordance — deferred stub (owner-decision D2)

The affordance appears in **two** places (owner-decision D2 + COORDINATION.md
§0): a **first-class, visible Generate button** at the top of the Lessons
library/catalog (icon `SparkleIcon`, label **Generate**), **and** a **Make
lesson** action on a **Class-type recording in Recordings** (sentence case,
beside that recording's existing "Make notes / Make review guide"; non-Class
recordings do not show it, OQ-2). Neither is **a dead button or an error**.
Clicking either opens the same intentional **"coming soon" preview panel** that:

- reads as a deliberate preview of a planned capability, styled in the normal
  design language (not a disabled/greyed control, not an error toast);
- describes what it will do in plain language: **"Turn a Class recording into an
  interactive lesson — explainers, worked examples, a checkpoint, and a trainer,
  generated from your recording's transcript and screenshots. Coming soon."**;
- states honestly that it is a preview and will require Pro and an AI provider
  when it ships, and points the user to the working path today (**Import a
  lesson…** and the seed course);
- makes **no AI call, no entitlement check, no network request** in this build.

UI states for the affordance: **default** (the visible Generate button in Lessons
and the Make lesson action on a Class recording), **preview/coming-soon** (the
panel above), and — reserved for §13 when built — **picker / generating / done /
per-part-error**. Only default and preview exist in this build. The preview is a
pure client view; there is **no `lessons_make_from_recording` command wired in
this build** (it is listed in §17 as deferred). When §13 ships, the Recordings
action generates from that recording directly and the Lessons button opens a
Class-recording picker first.

---

## 13. Deferred: AI generation pipeline (vNext) — contract, not implemented this build

This section captures the intended Source-Path-A contract at a high level so the
Generate stub (§12.3) is an honest preview and the future work has a spec to grow
from. **None of §13 is implemented in this build** — the deferral is owner-
decision D2 (`owner-decisions.md`), which the design review (HIGH-1) correctly
required be a verifiable authority rather than an inference. It also restates the
§6 correctness contract as a **hard, enforced precondition** on the deferred
work: no AI-generated explainer may be marked playable until Rust recomputes and
replaces its numbers (the model's own numbers are never stored).

**Intended shape (mirrors `study.rs` generation, verified patterns):**

- **Entry point (deferred):** `lessons::generate::from_recording` behind a Pro
  gate, invoked for a **Class-type** recording only (OQ-2), through
  `ai::client::complete` with the unchanged **Pro → configured → consent** chain
  (verified `ai/client.rs:644`). Consent/Pro/config/key/network errors stop the
  whole run; a single bad part is retried once then reported for that part while
  the others proceed — exactly the `study::generate` loop that returns
  `Vec<(part, Result<..>)>` (verified `study.rs:507`, test
  `bad_answer_is_retried_once_then_reported_for_that_part`).
- **Input contract (deferred):** the same filtered transcript study uses —
  Whisper hallucinations already dropped and never re-fed, stricken spans as
  `[stricken from the record]`, deleted words gone, each line time-stamped, plus
  screenshot references for grounding. Transcript text and model output are never
  logged.
- **Per-part JSON contract (deferred):** a part table like `STUDY_TOOLS.md`'s,
  each part validated by a named `lessons::schema::validate_*`:
  `concepts` (the DAG), `lesson.objectives`, `lesson.explainers[]`,
  `lesson.workedExamples[]`, `lesson.misconceptions`, `checkpoint.items[]`,
  `exam.items[]`. Each is tolerantly parsed then strictly validated (the §4.6
  path), unknown fields ignored.
- **Small-context handling (deferred):** reuse study's map/condense step —
  `chunk_lines` + `body_budget` using `CHARS_PER_TOKEN` and the `SAFETY = 0.85`
  margin (the "15% margin"; verified `study/prompt.rs`), condensing to
  time-stamped notes in up to the same round cap study uses, then writing each
  part from the notes.
- **Graph repair (deferred):** a model-produced cyclic concept graph or a
  dangling `conceptId/figureId/lessonId` reference is **repaired by dropping the
  offending edge/element** where that leaves a valid DAG, else the part fails and
  is retried once then reported — never stored cyclic.
- **HIGH-2 correctness precondition (hard gate on this deferred work):** for
  every explainer kind with a closed form (§6 table), the shared Rust function
  **recomputes the authoritative values from the structural inputs and stores the
  recomputed result**; the model's own numeric outputs are **never stored**. An
  explainer whose structural inputs don't yield a valid computation
  (unbalanceable equation, units that don't reduce, non-monotonic titration
  points) is **dropped** from the generated lesson. A generated lesson may not be
  marked playable until all its explainer data passes the §6 Rust validation and
  this recompute-and-replace step. MC answers are resolved from
  index/letter/choice-text **into** the stored `answerIndex` by reusing
  `study/parse.rs::answer_index` (verified) at this step (MEDIUM-4); stored
  bundles always contain a resolved index.
- **WorkedExample (deferred, NIT-9):** worked examples are part of the generated
  part table above; their steps/`finalAnswer` are recomputed and verified like
  any other closed-form content before storage.
- **Provenance + purge (deferred):** a generated set is written with provenance
  `generated`, `sourceRecordingId`, and `transcriptFingerprint`, and is subject
  to the full purge wiring already landed in §11.4.

Figures for a generated lesson: **deterministic diagrams only at runtime**, no
runtime image generation ever (OQ-7); a generated lesson renders its structured
diagrams through the §6 explainer components and reuses any applicable seeded
figure library.

---

## 14. The six seeded stoichiometry lessons (authored content) — rigor

Six lessons ship as seed content at **AP Chem / college** level (AC 4.6). Each
lesson has: explicit **learning objectives mapped to concepts**, **2–5
explainers (counting figure and interactive kinds together) that MUST include
≥1 required AI-authored `figure`-kind explainer backed by a decoding embedded
PNG** (owner-decision D4) alongside its interactive explainers, **3+ worked
examples with step reveal**, a **"common mistakes / misconceptions" explainer**
(owner-decision D3), an **adaptive trainer**, and an **8–10 item checkpoint**.
The six, feeding one cumulative 30-question exam and a certificate:

1. **Moles, molar mass, and the mole concept** — concepts: mole, Avogadro's
   number, molar mass. Hero explainer: mole-ratio calculator (mass↔mole).
   Misconceptions: molar mass vs molecular formula; counting atoms vs moles.
2. **Chemical equations and balancing** — concepts: conservation of mass,
   balancing, states. Hero: equation balancer with per-element mass-balance
   meter. Misconceptions: changing subscripts to balance; forgetting diatomics.
3. **Mole ratios and stoichiometric calculations** — concepts: mole ratio,
   stoichiometric factor. Hero: mole-ratio step-reveal + dimensional-analysis
   railroad. Misconceptions: using masses as ratios without converting to moles;
   not balancing first.
4. **Limiting reagents and percent yield** — concepts: limiting reagent, excess,
   theoretical/percent yield. Hero: limiting-reagent before/after consumption
   bars. Misconceptions: assuming the smaller mass is limiting; percent yield > 100%.
5. **Solution stoichiometry and titration** — concepts: molarity, dilution,
   titration, equivalence point. Hero: titration curve with draggable titrant
   slider. Misconceptions: equivalence vs endpoint; forgetting mole ratio in
   titration.
6. **Gas stoichiometry (ideal gas, gas-law stoichiometry)** — concepts: ideal
   gas law, molar volume at STP, gas stoichiometry. Hero: ideal-gas PV explorer
   with two coupled variables. Misconceptions: using 22.4 L/mol off-STP;
   mixing P/T units.

**Content correctness is a shipped guarantee, not a hope:** every balanced
equation balances, every molar mass is correct to 2 decimals, and every worked
arithmetic and item answer key is recomputed and asserted by the seed-validation
test (§4.6, §12.2). These answer keys are the exam keys.

---

## 15. Testability

Mirrors the study subsystem's coverage (`study/tests.rs`), with a mock completer
so no test hits a real endpoint. The deferred §13 generation is covered by
contract-level tests only where a stub exists; full generation tests arrive with
that work.

**Rust (`src-tauri/src/lessons/tests.rs`)**
- **Schema validation**: valid bundles validate; malformed/oversized/cyclic-
  prerequisite/dangling-reference/out-of-range-answerIndex/missing-tolerance
  bundles fail cleanly and never panic (fuzz-style table like `study` quiz
  tests).
- **Bundle round-trip**: export → import yields the same content fingerprint; a
  flipped byte in a figure or JSON file fails per-file hash check; an unknown
  `schemaMajor` is rejected; a newer `schemaMinor` is accepted; a `..`/absolute
  zip entry path is rejected.
- **Seed dogfood + content rigor**: the shipped `stoichiometry.lesson` passes
  `validate_bundle` with zero errors, loads via the ordinary `import_bundle` path
  (§4.5), and every answer key recomputes consistently (balance/molar-mass/
  dimensional-analysis), and every seed figure has a non-empty `reviewedBy`
  (§12.2).
- **Required-figure floor (owner-decision D4, Finding 1)**: a seed assertion that
  **each of the six seed lessons embeds ≥1 `figure`-kind explainer**, that each
  such explainer's `data.figureId` resolves to a manifest `Figure`, and that the
  referenced embedded PNG **decodes** (read via the `image` crate to a valid
  raster of the manifest's declared `width`×`height`) with a byte hash matching
  `manifest.files`. A seed set with a figure-free lesson, a dangling `figureId`,
  or an empty/non-decoding PNG **fails the build**. This is the enforcement that
  makes figures genuinely required rather than nominally present; the earlier
  NIT-7 union test (below) stays, but is no longer the only visual-coverage
  guard — it never counted figures.
- **Set-level FR-EXP inventory completeness (NIT-7)**: a seed assertion that the
  **union of explainer kinds across the six lessons includes every FR-EXP kind**
  — `equation_balancer`, `mole_ratio`, `limiting_reagent`, `dimensional_analysis`,
  `molecule_diagram`, `titration_curve`, `ideal_gas_pv` — so a seed set missing
  one kind fails the build even though each lesson's 2..=5 explainer cap and the
  per-lesson figure floor pass. (This union test covers only interactive kinds;
  the required-figure floor above is the separate, authoritative figure guard.)
- **Item-bank floors (MEDIUM-5)**: a seed assertion that every checkpoint bank is
  ≥2× its `presentCount` with ≥2 items per tested concept, and the exam bank is
  ≥60 with ≥18 MC / ≥18 numeric / ≥9 multistep, `mix.balance == 0`, and
  `mix` summing to 30 — so the shipped banks can't silently fall to the drawn
  count and make re-randomization a no-op.
- **Balance-kind checkpoint floor (Finding 4)**: a seed assertion that
  **Lesson 2's (balancing) checkpoint bank contains ≥1 `balance`-kind item**,
  generalized to the invariant that **every concept whose explainer `kind` is
  `equation_balancer` is covered by ≥1 `balance`-kind quiz item** in the
  checkpoint bank of the lesson that teaches it — so a balancing checkpoint can
  never test balancing via MC alone and leave the `balance` kind unexercised.
  A balancing lesson whose checkpoint has zero `balance` items fails the build.
- **Numeric grading** (§9): the (input, item, grade) table including hostile
  inputs; `exact`/`abs`/`rel` tolerance; sci-notation. **Unit normalization
  (finding 1) is pinned by explicit cases**: a compatible prefixed unit converts
  and grades correct (`4000 mg` vs key `4 g` → correct; `250 mL` vs key `0.25 L`
  → correct; `25 °C` vs key `298.15 K` → correct, exercising the offset path); a
  dimensionally incompatible unit is wrong (`4 L` vs key `4 g` → wrong); a
  present-but-unknown unit is **unparseable, not graded** (`4 foo` vs key `4 g` →
  "couldn't read that unit", never silently compared); and an absent unit
  compares the magnitude in the item's declared unit (`4` vs key `4 g` →
  correct). These cases are first-class rows so the decision stays tested, not
  just documented.
- **Balance grading** (§6): `check_balance` accepts only lowest-integer balanced
  coefficients, order-independent.
- **Explainer correctness** (§6): balancer accepts only balanced equations;
  mole-ratio/limiting-reagent/gas-law/dimensional-analysis recomputations match
  the authored keys.
- **Mastery / review / trainer** (§7–§8): scripted attempt sequences produce
  expected scores and `mastered` transitions; SM-2-lite interval/ease updates;
  trainer picks the weakest/most-overdue concept and the correct difficulty tier;
  seeded generators are reproducible and their answers + worked steps are correct.
- **Diagnostic** (§8.4): grading seeds initial mastery; a tested-out concept is
  provisionally mastered but the exam still gates on checkpoints.
- **Exam / readiness** (§10): 30-item selection honors the mix
  (`mix.balance == 0`, sum == questionCount); scoring and pass threshold; gated
  multistep; readiness meter monotonic in mastery; remediation mapping
  item→concept→explainer; **two consecutive exam draws from the ≥60 bank differ**
  (re-randomization is real); the **exam stays locked until all six checkpoints
  are ≥80%** and unlocks when the sixth passes.
- **Checkpoint gating / retake** (§10.1a): a sub-80% checkpoint sets
  `checkpointPassed=false`, marks the lesson "needs review", does **not** block
  forward lesson navigation, keeps the exam locked, and a retake re-draws from the
  ≥2× bank preferring items not on the previous attempt; a ≥80% attempt sets
  `checkpointPassed=true` permanently (best-attempt wins).
- **Purge (MEDIUM-3)**: a word Strike / line Delete / **time-range Delete** /
  **whole-recording Delete** removes a `generated` set (live DB **and backups**)
  and its learner records; a `seed` and an `imported` set survive every one of
  those; screen-only edits keep a generated set. The backup test exercises the
  `Screens` and `RangeExtras` arms specifically, asserting the job-kind-
  independent `lessons::purge_for_meeting` call in `purge_one_backup` fires.
  (These tests construct a `generated` row directly since no generation pipeline
  exists yet.)
- **Offline enforcement — backend path** (OQ-11): taking the seed lesson with the
  network client stubbed to fail asserts success end-to-end (explainers, quizzes,
  trainers, review, exam, certificate), proving the backend requires no network.
  This proves AI is not required but, by itself, does **not** cover the frontend
  render path — see the frontend guard below (owner-decision D5, Finding 2).

**TypeScript (`src/lib/lessonLogic.test.ts`, Vitest)**: the non-authoritative
preview mirrors only — numeric parse/compare hint, mastery preview, progress/
resume state. (Authoritative grading is Rust; TS never decides pass/fail.)

**Performance (NFR-4, NIT-11)**: a lightweight dev timing assertion (behind a
`#[cfg(test)]`/bench flag, not shipped) measures **bundle load + first-explainer
mount** for the seed set and asserts it under a generous CI budget (e.g. 1.5 s on
CI hardware, with the NFR-4 target of <1 s on a mid Mac). This is a smoke guard
against gross regressions, not a precise benchmark; the authoritative <1s-on-a-
mid-Mac check is a **recorded manual QA step** (method: cold-open Lessons, open
the seed set's first lesson, stopwatch to first interactive explainer), noted in
`docs/USER_GUIDE.md`'s QA appendix so it isn't silently dropped.

**Policy / build (NFR-1, NFR-2)**: `cargo test --lib` and `cargo test --lib
--features mas`; `npx tsc --noEmit` (not `npm run build`, which bumps the build
number — COORDINATION.md §2); `python3 scripts/check-ai-provider-policy.py`
(no new AI host, no default URL, no runtime image client, no startup/save
network, no bundled key); a grep test asserting no `dangerouslySetInnerHTML` in
`src/components/lessons/` or `src/lib/lessonLogic*`.

**Frontend offline guard — AUTHORITATIVE for Finding 2 (owner-decision D5)**:
`scripts/check-lessons-offline.*`, run in CI, scans the **compiled** Lessons JS
bundle + compiled `Lessons.css` and **fails on any non-local `http(s)://` or
remote `url(...)` reference** (local/relative/`data:`/`tauri://`/bundled-font
references allow-listed). It additionally asserts `Lessons.css` has **no**
`@import url(http…)` or remote `@font-face`, and that every `font-family`
resolves to the **bundled Inter** (FR-OFF-2). This is the authoritative proof
that the Lessons frontend fetches nothing remote; the §11.2 Tauri **CSP**
(`connect-src`/`img-src`/`font-src` restricted to `'self'` + the asset protocol,
no remote origin) is the runtime backstop. Together with the backend
network-stub take-test above, the full "zero runtime network when taking a
present lesson" guarantee (AC 4.2.2, FR-OFF-1, NFR-2) is covered on **both** the
backend and the frontend render path.

**Swift**: deferred with the iOS render layer (§16); the shared validators/
grading/explainer-correctness functions port when iOS is built.

---

## 16. iOS parity plan (honest scope) — correctness in shared Rust (MEDIUM-6)

**Mac ships fully. iOS is a documented follow-up, not shipped in this
iteration** (FR-IOS-1). What keeps iOS cheap later:

- The **data AND correctness layer is shared Rust** (FR-IOS-2, MEDIUM-6): the
  bundle format, manifest, schema validation, concept graph, mastery/review math,
  numeric + balance grading, trainer generators and their worked-step solver,
  explainer correctness (balancer validity, unit cancellation, PV, limiting
  reagent), exam scoring, and readiness are all pure Rust functions with no
  Tauri/UI dependency. The TS layer is **preview-only** and never authoritative,
  so there is no correctness logic that fails to port. A Swift port calls the
  same Rust (as markers and study guides already do across `src-tauri` and
  `ios/`). The seed `.lesson` loads on iOS unchanged.
- **iOS target** when built (FR-IOS-3): taking a lesson (explainers, quizzes,
  trainers, review, exam, certificate) in SwiftUI; the deferred `Generate` path
  follows the existing `MeetingAI`/`AIClient` Pro → configured → consent chain.
- **Explainer portability** (FR-IOS-4): because correctness is shared Rust, iOS
  re-implements **only the gesture/drawing layer**, never the correctness
  functions. **Molecule/reaction diagram, titration curve, ideal-gas PV,
  mole-ratio step reveal, worked-example step reveal, and the misconceptions
  cards** port directly (static/parametric draws over shared data). The
  **equation balancer, dimensional-analysis railroad builder, and
  limiting-reagent simulator** need a SwiftUI re-implementation of their
  drag/interaction UI only — the data and the Rust correctness rules are shared.
  This is the honest "ports directly vs needs re-implementation" split required
  by FR-IOS-4.

---

## 17. File / module map and docs to update

### Rust — `src-tauri/src/lessons/` (new module, registered in `lib.rs` like `study`)

```
lessons.rs                     // module root: ensure_schema, catalog CRUD, Progress,
                               //   purge_for_meeting, seed hook wiring
lessons/schema.rs              // typed entities (§3), tolerant parse + strict validate_* (§4.6),
                               //   shared clean_text (reused from study pattern)
lessons/bundle.rs              // zip import/export, manifest, per-file + content SHA (§4),
                               //   validate_bundle, import_bundle (file/folder/URL), export_bundle
lessons/explain.rs             // explainer correctness: balancer, unit cancellation, PV, etc. (§6)
lessons/mastery.rs             // EMA mastery math + constants (§7), seed_from_diagnostic (§8.4)
lessons/review.rs              // SM-2-lite spaced repetition (§8.1)
lessons/trainer.rs             // adaptive selection + difficulty tiers (§8.2)
lessons/generators.rs          // closed set of numeric item generators + solve_with_steps (§8.2–8.3)
lessons/grade.rs               // check_numeric (§9), check_balance (§6), MC/multistep grading
lessons/exam.rs                // exam selection, scoring, readiness, certificate result hash (§10)
lessons/purge.rs               // purge_for_meeting + save-time fingerprint recheck (§11.4)
lessons/seed.rs                // ensure_seeded on first run (§5.4)
lessons/commands.rs            // Tauri commands, registered in lib.rs invoke_handler
lessons/tests.rs               // §15
// lessons/generate.rs         // DEFERRED (§13): recording → per-part lesson JSON; not in this build
```

New commands (registered next to the `study::commands::*` block in `lib.rs`
`invoke_handler`): `lessons_list_catalog`, `lessons_import_bundle`
(file/folder/URL), `lessons_remove_set`, `lessons_get_set`, `lessons_get_lesson`,
`lessons_save_progress`, `lessons_submit_checkpoint`, `lessons_next_trainer_item`,
`lessons_submit_trainer_item`, `lessons_review_due`, `lessons_start_diagnostic`,
`lessons_submit_diagnostic`, `lessons_dashboard`, `lessons_start_exam`,
`lessons_submit_exam`, `lessons_export_certificate`, `lessons_reset_progress`.
**Deferred (not wired this build):** `lessons_make_from_recording`. Seed loading
is called at startup (not a command).

`Cargo.toml`: add `zip = "=2.2.0"` (pure-Rust, MIT/Apache-2.0; §1). `sha2`,
`serde_json`, `uuid`, `sqlx`, `async-trait` are already present (verified).

### Frontend — `src/components/lessons/` + `src/lib/lessonLogic.ts` + a view

```
src/components/views/LessonsView.tsx     // library + Generate affordance + routing
src/components/lessons/LessonLibrary.tsx // catalog list, import + Generate(stub), empty/loading/error
src/components/lessons/GeneratePreview.tsx // the "coming soon" preview panel (§12.3)
src/components/lessons/LessonPlayer.tsx  // explainer sequence, worked examples, misconceptions, checkpoint
src/components/lessons/ExamRunner.tsx    // 30-item exam, numeric input, multistep, solution reveal
src/components/lessons/Certificate.tsx   // printable/exportable certificate view
src/components/lessons/Dashboard.tsx     // mastery dashboard, streak, exam-readiness, what-next
src/components/lessons/ReviewDue.tsx     // spaced-repetition due surface (§8.1)
src/components/lessons/Trainer.tsx       // adaptive drill UI with full-solution reveal
src/components/lessons/explainers/       // one component per §6 kind (SVG/Canvas, a11y, reduced-motion)
src/components/lessons/Lessons.css       // tokens only, mirrors Study.css conventions
src/lib/lessonLogic.ts                   // PREVIEW-ONLY mirror (never authoritative, MEDIUM-6)
src/lib/lessonLogic.test.ts              // Vitest
```

`src/components/Shell.tsx`: add `"lessons"` to `AppMode` (currently
`"record" | "recordings" | "chat"`, verified `Shell.tsx:23`) and render
`<LessonsView/>`; **and update the stale top-of-file comment**
(`Shell.tsx:1-2`, currently "one of three views (Record · Recordings · Chat)",
verified) to **"one of four views (Record / Recordings / Lessons / Chat)"** in
the same edit, so the in-file comment doesn't contradict the four-view reality
(review Finding 3). `src/components/TopBar.tsx`: add the Lessons mode at **⌘3** and
move **Chat to ⌘4** (COORDINATION.md §4) with `GraduationCapIcon`.
`src/components/icons.tsx`: **append** the eleven icons in §2 (append-only).
The **Recordings** view gains a **Make lesson** action on a Class-type recording
(beside its "Make notes / Make review guide"); in this build it opens the §12.3
coming-soon preview and wires to no command. The nav/menu/tray changes
(`menu_builder.rs`, `App.tsx` listeners, `tray_builder.rs`, the film-harness
mock) ship in the same PR as the TopBar change (COORDINATION.md §4).

### Docs to update

- `DESIGN.md`: navigation table (four views) and vocabulary table (lesson /
  lesson set / Generate / Import a lesson).
- `docs/REDACTION.md`: extend purge checklist step 5 — `generated` lessons are
  deleted (not rewritten) on transcript edits; `seed`/`imported` are exempt; note
  the `provenance = 'generated'` guard, the save-time fingerprint recheck, and
  the job-kind-independent backup purge hook in `purge_one_backup` (MEDIUM-3).
- `docs/ARCHITECTURE.md`: add the Lessons subsystem.
- `docs/USER_GUIDE.md`: a Lessons section (take, import, review, exam,
  certificate; Generate is a preview).
- `docs/LESSONS_AUTHORING.md` (new, owned by the content step): the figure
  accuracy-review checklist (OQ-9, §12.2), the figure manifest format (§12.1),
  and the seed-authoring procedure.
- `docs/PRO.md`: add a **Lessons** row — Making a lesson (the deferred Generate
  pipeline) is Pro; taking, mastery, exam, and certificate are free
  (COORDINATION.md §3). Add a `lessons` feature key to `src/lib/pro.ts` and
  `ios/NoFriction/Store/ProFeature.swift` (a unit test checks the keys match), so
  the paywall reads "Making lessons is part of noFriction Pro."
- `docs/SYNC.md`: state that lessons are **not synced in v1** (OQ-10, §11.4).
- `site/privacy/index.html`: add a sentence covering the single user-initiated
  "Import from URL" fetch, keeping "Data Not Collected" true (COORDINATION.md §5).
- `src-tauri/Cargo.toml` / `Cargo.lock`: add `zip = "=2.2.0"`; expect a lock
  conflict with `fix/mac-post-launch` (which removes `tokio-tungstenite` /
  `jsonwebtoken`), resolved on the pre-PR rebase onto `origin/main`
  (COORDINATION.md §1). (`image = "0.25"` and `sha2` are already present —
  verified — so the seed figure-decode check in §4.6/§15 needs no new crate.)
- `scripts/check-lessons-offline.*` (new, run in CI): the **authoritative**
  frontend offline guard (owner-decision D5, §11.2/§15) — scans the compiled
  Lessons JS bundle + `Lessons.css` and fails on any non-local `http(s)://` or
  remote `url(...)` reference; also asserts `Lessons.css` uses only the bundled
  Inter font with no remote `@import`/`@font-face`.
- Tauri config CSP for the Lessons surface (`connect-src`/`img-src`/`font-src`
  restricted to `'self'` + the asset protocol, no remote origin) — the runtime
  backstop for the offline guarantee (owner-decision D5, §11.2).

---

## 18. Resolved open questions and review/directive responses

### 18.1 Resolved open questions (every one, no ambiguity left)

- **OQ-1 (nav)** → **Fourth top-bar view** (Record · Recordings · Lessons · Chat), §2.
- **OQ-2 (generation source)** → **Class recordings only** (deferred §13).
- **OQ-3 (bundle format)** → **Zip `.lesson`** with hashed `manifest.json`, JSON
  data files, `figures/*.png`, §4.
- **OQ-4 (figure storage)** → **Files on disk** under the app data folder, §5.2.
- **OQ-5 (seed size cap)** → **Target < ~40 MB**; optimized in the authoring
  script, §12.1 / NFR-5.
- **OQ-6 (exam gate)** → **All six checkpoints (≥80%) unlock the exam**; dev-only
  override, §10.1.
- **OQ-7 (figures for generated lessons)** → **Deterministic diagrams only at
  runtime**, §12 / §13.
- **OQ-8 (search index)** → **Generated-lesson text not indexed in FTS v1**, §11.4.
- **OQ-9 (figure accuracy review)** → **Two-part gate**: mechanical self-
  consistency (enforced) + named human reviewer signoff in
  `docs/LESSONS_AUTHORING.md`, §12.2.
- **OQ-10 (sync)** → **Not synced in v1**, §11.4.
- **OQ-11 (offline enforcement)** → **Backend network-stubbed take test** +
  **authoritative frontend grep guard over the compiled Lessons bundle +
  `Lessons.css`** (fails on any remote `http(s)://`/`url(...)`) + a Tauri CSP
  backstop + policy guard (owner-decision D5, §11.2/§15).
- **OQ-12 (numeric tolerance)** → **Per-item tolerance REQUIRED; `exact` default
  for exact/integer answers, `rel` only where declared; compatible prefixed/
  scaled units convert before comparison, incompatible units are wrong, unknown
  units are unparseable (never silently graded)**, §9.
- **OQ-13 (certificate verifiability)** → **Cosmetic + local `resultHash`**, §10.4.
- **OQ-14 (progress reset)** → **Per-set "Start over"**, §5.5.

### 18.2 Response to the design review (CHANGES_REQUESTED)

**Revision-5 responses to the latest review (`design-review.json`, revision 4 of
the doc: 2 MEDIUM + 2 NIT, left open when the prior loop hit its iteration
ceiling).** All four are resolved in this pass; no settled decision is
re-litigated:

- **Finding 1 [MEDIUM] — AI-authored figures required by the requirements
  (AC 4.4.2 "both required", AC 4.6.2) but never mandated in the seed set** →
  **Resolved per owner-decision D4 via the review's fix (a).** Figures are now
  **genuinely required**, not stretch: §3.2 reworks the explainers cap to count
  figure and interactive kinds together with a **hard floor of ≥1 `figure`-kind
  explainer per lesson**; §4.6 adds that floor plus a **PNG-decode + hash-match**
  assertion to import validation; §14/§15 add a seed test that each of the six
  lessons embeds ≥1 figure-kind explainer whose referenced embedded PNG decodes;
  §6 and §12.1 state figures are required and that deterministic SVG explainers
  are **additive, not a replacement**; and §12.1/§18.4 **remove the "stretch"
  hedge and the "all-deterministic-SVG seed is shippable" escape hatch** — a
  figure-free seed bundle now fails the build. The runtime caption fallback is
  explicitly reframed as a defensive runtime path, not an authoring escape.
- **Finding 2 [MEDIUM] — "zero runtime network" asserted but not provable on the
  frontend render path** → **Resolved per owner-decision D5.** §11.2 and §15
  specify an **authoritative** build/test **grep guard**
  (`scripts/check-lessons-offline.*`) that scans the **compiled** Lessons JS
  bundle + `Lessons.css` for any non-local `http(s)://` or remote `url(...)`
  reference and **fails on a match**; a **Tauri CSP** backstop restricts
  `connect-src`/`img-src`/`font-src` for the Lessons surface to `'self'` + the
  asset protocol with no remote origin; and a CSS assertion confirms only the
  **bundled Inter** font is used with **no** remote `@import`/`@font-face`. The
  grep guard is named authoritative (it fails the build on the actual shipped
  bytes); the backend network-stub take-test and the frontend grep guard
  together cover both halves of AC 4.2.2 / FR-OFF-1 / NFR-2.
- **Finding 3 [NIT] — stale `Shell.tsx` "three views" header comment not in the
  edit list** → **Resolved in §17**: the edit list now includes updating the
  `Shell.tsx:1-2` top-of-file comment to "one of four views (Record /
  Recordings / Lessons / Chat)" in the same edit as the `AppMode` enum change.
- **Finding 4 [NIT] — no floor guaranteeing a `balance`-kind item in the
  balancing checkpoint** → **Resolved in §10.1b and §15**: a seed assertion
  requires **Lesson 2's checkpoint bank to contain ≥1 `balance`-kind item**
  (generalized: every concept whose explainer kind is `equation_balancer` is
  covered by ≥1 `balance`-kind quiz item), added to the item-bank-floor tests so
  a balancing checkpoint that tests balancing only via MC fails the build.

Every already-locked decision is unchanged: the zip `.lesson` bundle with a
hashed manifest + embedded PNGs; the 30-question / 80%-pass exam gated on six
≥80% checkpoints drawn from a ≥60-item over-provisioned bank; the six-lesson
stoichiometry arc; Lessons as the first-class fourth top-bar view (D1); Source
Path A deferred to an honest coming-soon Generate stub (D2) with the vNext
pipeline contract; the D3 enhancement set (spaced repetition, adaptive trainers,
worked-solution reveal, mastery dashboard, printable offline certificate); and
Mac shipping fully with shared-Rust correctness and SwiftUI render as a
documented iOS follow-up. All invariants hold: no server, no runtime AI/image
calls, no model-output-as-HTML, both Mac flavors build, policy guard passes,
fully offline + free to take.

**Revision-4 responses to the prior review (`design-review` revision 3 of
the doc: 1 MEDIUM + 2 NIT).** All three were resolved:

- **Finding 1 [MEDIUM] — §9 numeric grader left prefixed/scaled units
  unspecified; a correct answer in a compatible unit could be silently marked
  wrong** → **Resolved in §9 step 2.** The canonical-unit map is upgraded from
  synonym-folding to a **dimension-class + scale-factor/offset table**: a
  compatible prefixed unit (`4000 mg` vs `4 g`, `250 mL` vs `0.25 L`, `25 °C` vs
  `298.15 K`) is **converted to the item unit and then compared within
  tolerance**; a dimensionally incompatible unit (grams vs liters) is **wrong**;
  and a present-but-unknown unit is **rejected as unparseable** ("couldn't read
  that unit"), never silently graded as a bare magnitude. The undefined branch
  the reviewer identified is closed. The decision and its edge cases (including
  the hostile `4000 mg` vs `4 g` and `4 foo` vs `4 g` cases) are added to the §15
  numeric-grading test table so the policy is pinned and tested, satisfying the
  review's required fix.
- **Finding 2 [NIT] — the "make work later" deferral clause is single-sourced
  while the doc treated the two owner files as uniformly corroborating** →
  **Resolved in §18.2's HIGH-1 response**: it now states that the **deferral**
  rests on **owner-decision D2 specifically** (single-sourced in the durable
  `owner-decisions.md`), and that COORDINATION.md §0 corroborates the **button
  and tab** but **not** the "later" clause — so a future reader does not
  over-read the corroboration. No change to the decision.
- **Finding 3 [NIT] — COORDINATION.md §4 still phrases the nav change as pending
  owner sign-off; reconcile with D1** → **Resolved in §2**: a reconciliation
  paragraph states that D1 and COORDINATION.md §0 *are* the owner yes §4 asks
  for, so the §4 gate is satisfied rather than outstanding (the stale §4 wording
  is documentation drift to update in the coordination note, not an open gate).

The prior-round mapping below (HIGH 1; MEDIUM 2–5; NIT 6–11) is retained for
history; every HIGH and MEDIUM there remains resolved.

- **HIGH-1 (Source Path A is "built now"; design deferred it on an ambiguous
  directive)** → **Resolved by a durable, recorded owner decision.** The
  deferral is now backed by **owner-decision D2**
  (`.agents/tasks/lessons-stoichiometry/owner-decisions.md`, verbatim: *"add a
  generate button that we will make work later"*), which the reviewer can
  verify independently of this doc. Per the review's Option B, the requirements'
  "built now" is explicitly superseded: for this build, "built now" = the
  first-class **Generate** entry point and its honest coming-soon preview
  (§12.3); the AI pipeline is **contract-now, implemented-vNext** (§13) with its
  stated recompute-and-replace accuracy precondition. The design and
  requirements no longer conflict because D2 is the controlling authority and
  §1/§12.3/§13 state the supersession plainly.
  **Authority precision (review finding 2):** the **deferral** ("make work
  later") rests on **owner-decision D2 specifically** — that clause is
  single-sourced in `owner-decisions.md`, which is a durable, checked-in
  authority file created for exactly this verification. `COORDINATION.md` §0
  independently corroborates the **Generate button and the fourth tab** ("gets
  its own tab and generate button") but does **not** contain the "later" clause,
  so the two files jointly source the *button/tab*, and D2 alone sources the
  *deferral*. A future reader should not over-read §0 as also confirming the
  deferral; D2 is its sole, sufficient authority.
- **MEDIUM-2 (fourth view marked "confirmed" with no verifiable owner source)**
  → **Fixed.** §2 now cites **owner-decision D1** (verbatim *"gets it own tab."*)
  as the source of record and stops asserting an inline "confirmed with owner."
  The DESIGN.md "three views" expansion rides on D1, the verifiable confirmation
  the requirement (OQ-1) demanded.
- **MEDIUM-3 (live `delete_meeting` is non-transactional, deletes transcripts by
  raw SQL first; cascade/dir ordering unspecified)** → **Fixed in §11.4.** The
  live path now specifies the exact order: (1) resolve the affected
  `lesson_sets.dir_path` values **before** deleting any row, (2) delete DB rows
  (catalog + learner records) via `lessons::purge_for_meeting`, (3) remove the
  on-disk dirs **last**, tolerating an already-absent dir, (4) a dir-removal
  failure after the DB delete is logged-and-ignored (never surfaced, never
  blocks the recording delete). It is stated explicitly that
  `delete_meeting`'s existing non-transactional sequencing is acceptable because
  each purge is **idempotent and keyed by `meeting_id`/`source_recording_id`**,
  and that seed/imported sets (`provenance != 'generated'`) are never touched.
  Verified against `database.rs:1338/1350–1351` and the backup arms
  `redaction.rs:1283/1308/1316`.
- **MEDIUM-4 (sub-80% checkpoint remediation / progression gating
  underspecified)** → **Fixed in §10.1a.** A new rule defines the lesson-player
  state machine for `<80%`: forward navigation is **not** blocked (lessons are
  explorable in any order), but the checkpoint leaves `checkpointPassed=false`,
  which keeps the final exam locked (the exam needs all six checkpoints ≥80%).
  A failed checkpoint surfaces a "Review these concepts" deep-link list to the
  explainers tagged with each missed item's `conceptIds`, plus a "Retake
  checkpoint" action that **re-draws** the checkpoint from the over-provisioned
  bank (§10.1b). Checkpoints are selection-only (fixed authored bank, no
  generators), so re-draw variety comes from the bank floor.
- **MEDIUM-5 (exam/checkpoint "re-randomizes" is a no-op without a bank floor)**
  → **Fixed in §10.1b.** Concrete authored-bank floors are specified and added
  to seed validation: each **checkpoint authors ≥2× its presented count** (≥16–20
  items for an 8–10 presented), and the **exam authors ≥60 items** (≥18 MC, ≥18
  numeric, ≥9 multistep — ≥1.5× the drawn mix) spanning all six lessons with
  per-concept coverage quotas, drawn **without replacement across the two most
  recent attempts** where the bank allows. Generator-backed numeric items draw
  fresh values on top. The balancer `mix` accounting is fixed by NIT-6 below.
- **NIT-6 (`balance` in the exam mix but excluded from the 30-count)** → **Fixed
  in §10.1**: `Exam.mix.balance = 0` is explicit and validation asserts
  `mc + numeric + balance + multistep == questionCount`; standalone balance
  items are not part of the 30 — balancing is tested **inside** multistep chains
  and as checkpoint items.
- **NIT-7 (no set-level FR-EXP completeness check)** → **Fixed in §14/§15**: the
  seed-validation test asserts the **union of explainer kinds across the six
  lessons includes every FR-EXP kind** (`equation_balancer`, `mole_ratio`,
  `limiting_reagent`, `dimensional_analysis`, `molecule_diagram`,
  `titration_curve`, `ideal_gas_pv`).
- **NIT-8 (molecule diagram validates structure only, undercutting §6.1)** →
  **Fixed in §6.1/§6 row 5**: §6.1's "recomputed" claim is explicitly scoped to
  the closed-form kinds; chemical fidelity of the molecule/reaction diagram
  (valence, label-to-atom correspondence, reaction conservation) is routed to the
  §12.2 human review gate, with an optional flagged valence check (sum of bond
  orders per atom ≤ typical valence) in authoring.
- **NIT-9 (titration curve trusts authored points)** → **Fixed in §6.1/§6 row
  6**: the titration curve is explicitly **carved out** of the §6.1 full-
  recompute claim; its physics is not re-derived, and its scientific correctness
  is routed to the §12.2 human gate (same as figures). The mechanical check
  remains monotonicity + equivalence-point presence.
- **NIT-10 (provenance precedence between schema-accepts-`generated` and
  import-forces-`imported`)** → **Fixed in §4.6/§4.7**: schema validation accepts
  any of the three provenance values **syntactically**; `import_bundle`
  **unconditionally overwrites** a user-imported bundle's provenance to
  `imported` before persist, so the stored value is authoritative and a
  bundle-claimed `generated` is harmless. Seed loading is the only path that
  persists `seed`.
- **NIT-11 (NFR-4 <1s budget unverified)** → **Fixed in §15**: a lightweight dev
  timing assertion guards bundle-load + first-explainer-mount under budget, and
  the doc records that NFR-4 is otherwise confirmed manually at QA with the
  method stated.

Already-landed resolutions carried from revision 2 (still verified): one
`schemaMajor` everywhere with minor confined to the manifest (§3.2/§4.2); stored
`answerIndex` always validated, letter/text resolution only in the deferred
generation step via `study/parse.rs::answer_index` (§3.2/§13); per-item required
numeric tolerance with `exact` default and `balance` as a first-class graded
kind (§9/§6); all correctness/grading authoritative in shared Rust with a
preview-only TS mirror (§16); the two-part figure-accuracy gate (§12.2); the
job-kind-independent backup purge hook in `purge_one_backup` (§11.4);
`zip = "=2.2.0"` pure-Rust (§1); and per-icon nav vs hero render sizes (§2).

### 18.3 Headline decisions

- **Bundle format**: zip `.lesson` with a hashed `manifest.json` (per-file
  SHA-256 + a content fingerprint), JSON data files, embedded `figures/*.png`;
  seed content dogfoods the identical format and import path.
- **Exam**: exactly 30 questions (`mix = 12 MC / 12 numeric / 0 standalone
  balance / 6 multistep`, summing to 30; balancing is tested inside multistep and
  in checkpoints) drawn from an over-provisioned bank (≥60 items, ≥18/≥18/≥9 per
  kind, ≥2 items per concept) so retakes genuinely differ; 80% to pass, unlocked
  only after all six checkpoints pass (each ≥80%). A sub-80% checkpoint marks the
  lesson "needs review", routes remediation to the missed concepts' explainers,
  offers a retake that re-draws from the ≥2× checkpoint bank, and keeps the exam
  locked — it does not block forward lesson navigation. Mastery is a 0–100 EMA
  per concept (α=0.4), mastered at ≥85 with ≥3 correct; spaced-repetition review
  (SM-2-lite) and adaptive difficulty trainers layer on top.
- **Lesson arc**: six seeded stoichiometry lessons (moles/molar mass →
  balancing → mole ratios → limiting reagent/percent yield → solution
  stoichiometry/titration → gas stoichiometry), each with objectives, 2–5
  explainers including ≥1 required authored figure PNG (owner-decision D4), 3+
  worked examples, a misconceptions explainer, a trainer, and a checkpoint,
  feeding one cumulative exam and a certificate.
- **UI placement**: Lessons is a first-class fourth top-bar view (Lessons ⌘3,
  Chat ⌘4), confirmed by owner-decision D1; the Make lesson / Generate affordance
  (in the Lessons view and on Class recordings) is a visible, honest "coming
  soon" preview (no AI call this build) per owner-decision D2; eleven new icons
  appended to `icons.tsx`.
- **iOS scope**: Mac ships fully; the Rust data/validation/grading/explainer-
  correctness layer is shared and portable; the SwiftUI render layer and iOS
  generation are a documented follow-up, with six explainers porting directly and
  three drag-interaction explainers needing SwiftUI re-implementation of the
  gesture layer only.

### 18.4 Stretch items (flagged, not dropped)

Marked stretch because they add scope beyond the core take/exam/certificate loop;
each stays fully local/offline if built: the **diagnostic pre-test** (§8.4) and
the **exam-readiness predictor** nuance in the dashboard (§10.3). None introduce
runtime network or a backend; none weaken an invariant.

**The art-directed genmedia figure set is NOT stretch (owner-decision D4,
revision-5 review Finding 1).** It was previously listed here as stretch with a
"deterministic-SVG fallback ships instead" escape hatch; that is removed. The
authored PNG figures are a **required** part of the shipped seed set — ≥1
decoding figure per lesson, enforced by seed validation (§4.6/§12.1/§14/§15) —
so a figure-free seed bundle fails the build rather than silently shipping. The
deterministic SVG explainers are additive to the figures, not a replacement.

