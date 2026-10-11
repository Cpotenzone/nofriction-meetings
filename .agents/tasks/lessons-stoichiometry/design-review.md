# Lessons design review — `docs/LESSONS.md` (revision 4)

Reviewer: design-review subagent (fresh read, no authoring context).
Reviewed against: `requirements.md` (authoritative), `owner-decisions.md` (D1–D3),
`COORDINATION.md`, `CLAUDE.md`, `DESIGN.md`, `docs/STUDY_TOOLS.md`, and the cited
source files in `src-tauri/` and `src/`.

Verdict: **CHANGES_REQUESTED** (2 MEDIUM, 0 HIGH, plus NITs). The gate rule is
mechanical: any HIGH or MEDIUM ⇒ CHANGES_REQUESTED.

The design is strong and unusually complete: it carries verifiable owner
authority for its two prior HIGH findings, its purge wiring and source-code
citations check out against the actual tree, and nearly every requirements open
question is resolved concretely. The two MEDIUM findings below are genuine
gaps an implementer would hit — not authority disputes.

---

## Findings

### 1. [MEDIUM] AI-authored figures are required by the requirements but not actually mandated anywhere in the seed set

**Where:** §3.2 (`Lesson.explainers: 2..=4 interactive/figure per lesson`), §4.6
(count bound "2..=4 explainers per lesson"), §14, and the §15 seed tests
(NIT-7 union test).

**Problem:** Requirements AC 4.4.2 states "**Two complementary visual
mechanisms, both required**": (i) deterministic interactive explainers AND
(ii) AI-authored static figure PNGs. AC 4.6.2 says the seed set includes "the
embedded, authoring-time figure PNGs." The design mechanically enforces (i) —
the NIT-7 seed test asserts the union of *interactive* explainer kinds covers all
seven FR-EXP kinds — but it never asserts that any `figure`-kind explainer /
embedded PNG actually exists in the seed set. The `2..=4 explainers per lesson`
cap counts `figure` and interactive kinds together, so a seed set with **zero
figures** (all slots filled by interactive kinds) would pass `validate_bundle`
and every §15 test. Compounding this, §12.1 explicitly allows the whole figure
set to fall back to deterministic SVG when genmedia is unavailable, and §18.4
marks the "art-directed genmedia figure set" as *stretch* — so there is a live
path where the shipped seed set contains no embedded PNG figures at all, which
contradicts AC 4.4.2 ("both required") and AC 4.6.2.

**Concrete fix:** Decide and state one of:
(a) make figures genuinely required and add a seed assertion, e.g. "the seed set
MUST contain ≥1 `figure`-kind explainer with an embedded PNG per lesson (or ≥N
across the set); the seed-validation test asserts `figures.len() >= N` and that
each referenced PNG decodes," and reconcile §18.4 so the figure set is not
"stretch"; **or**
(b) if the owner accepts an all-deterministic-diagram seed set (SVG only) as
satisfying "visual explainers," then amend the requirement interpretation
explicitly: cite the authority (an owner decision), state that AC 4.4.2(ii) is
satisfied by deterministic diagrams for the seed build, and drop the "both
required" language from the implementer-facing contract so §14/§15 and the
requirement agree. Either way, remove the ambiguity about whether a figure-free
seed set is shippable.

### 2. [MEDIUM] The "zero runtime network" offline guarantee is asserted but the test design cannot actually prove it for the whole Lessons code path

**Where:** §11.2, §15 ("Offline enforcement (OQ-11)"), AC 4.2.2, FR-OFF-1,
NFR-2, and the COORDINATION.md §5 invariant.

**Problem:** AC 4.2.2 requires that taking a present bundle performs **zero**
network requests and that this is **enforced and tested** so that "a lesson
taken with the network disabled behaves identically." The design's test is:
"taking the seed lesson with the **network client stubbed to fail** asserts
success end-to-end." That proves the Rust `ai::client` path is not *required*,
but it does not prove the **frontend render path makes no network request** —
which is where the actual offline risk lives (a web font `@font-face` src, an
`<img>`/`<link>` to a CDN, an analytics beacon, a map tile, an SVG `xlink:href`
to a remote asset). The design asserts "fonts come from the app's bundled Inter,
never a CDN" and relies on `check-ai-provider-policy.py`, but that script (per
CLAUDE.md / COORDINATION.md §5) checks for AI-service hosts, default remote
URLs, and startup/save network — it does **not** scan the Lessons React/CSS
bundle for arbitrary outbound URLs (CDN fonts, external images). So the stated
enforcement does not cover the stated guarantee.

**Concrete fix:** Specify a concrete enforcement that covers the frontend, e.g.:
(a) a build/test-time guard that greps the compiled Lessons bundle + `Lessons.css`
for any `http(s)://` / `url(` referencing a non-local origin and fails on a
match (name the check and where it runs), and/or
(b) a Content-Security-Policy / Tauri CSP note that blocks remote `connect-src`,
`img-src`, `font-src` for the Lessons surface, and/or
(c) an explicit assertion that the Lessons code path imports no remote asset and
that `Lessons.css` uses only `--font-primary` (bundled Inter). State which one is
authoritative. Without this, "behaves identically with the network disabled"
(AC 4.2.2) is claimed but not verifiable by the tests the doc lists.

### 3. [NIT] `Shell.tsx` AppMode comment/header says "three views"; design plans the enum change but not the stale comment

**Where:** §17 ("add `"lessons"` to `AppMode`"), verified against
`src/components/Shell.tsx:1-2,23`.

**Problem:** `Shell.tsx` opens with a comment "one of three views (Record ·
Recordings · Chat)" and `AppMode = "record" | "recordings" | "chat"` (confirmed
at line 23, exactly as the design claims). The design lists the enum edit and
the DESIGN.md nav-table edit, but does not mention updating the stale in-file
header comment, which will then contradict the four-view reality.

**Concrete fix:** Add the `Shell.tsx` top-of-file comment to the §17 edit list
("one of four views (Record · Recordings · Lessons · Chat)"), alongside the
`AppMode` change.

### 4. [NIT] `balance` is a declared `QuizItem` kind and checkpoint kind, but no floor guarantees any `balance` item exists in a checkpoint

**Where:** §3.2 (`QuizItem.kind: "mc" | "numeric" | "balance" | "multistep"`),
§6 ("balance quiz/exam items"), §10.1b (floors), §14 (Lesson 2 is the balancing
lesson).

**Problem:** The design introduces `balance` as a first-class gradeable kind
specifically so balancing is testable, and §14 makes Lesson 2 ("Chemical
equations and balancing") the balancing lesson. But the §10.1b checkpoint floors
only constrain total count (≥2×) and per-concept coverage (≥2 items/concept);
they do not require that Lesson 2's checkpoint actually contains any `balance`-kind
item. A balancing lesson whose checkpoint tests balancing only via MC would pass
validation, undercutting the stated value of the `balance` kind.

**Concrete fix:** Add a targeted seed assertion that Lesson 2's checkpoint bank
contains ≥1 `balance` item (or more generally: every concept whose explainer
kind is `equation_balancer` is covered by ≥1 `balance`-kind quiz item), and add
it to the §15 item-bank-floor test list.

---

## Verified assumptions (checked against source, not taken on the doc's word)

- **owner-decision D1 (fourth tab) exists and says "gets it own tab."** — Verified
  in `owner-decisions.md`. COORDINATION.md §0 independently corroborates "gets its
  own tab and generate button." The design's §2 justification and the
  DESIGN.md three→four-view expansion are properly authority-backed. (Resolves the
  prior MEDIUM-2.)
- **owner-decision D2 (Source Path A deferred to a stub) exists and says "add a
  generate button that we will make work later."** — Verified in
  `owner-decisions.md`. This is a durable, checked-in authority that supersedes
  requirements §4.1's "built now." The design's deferral and honest coming-soon
  preview (§12.3) are legitimate, not a silent scope drop. (Resolves the prior HIGH-1.)
- **owner-decision D3 ("finish and enhance / make this high end") authorizes the
  §8/§10 enhancement set.** — Verified in `owner-decisions.md`.
- **COORDINATION.md §4 nav sign-off reconciliation.** — Verified: §4 does say "Get
  Casey's yes before merging the nav change," written before §0/D1 were recorded.
  §0 ("gets its own tab and generate button") and D1 are the owner yes; the
  design's reading that §4 is now stale documentation drift (not an open gate) is
  correct.
- **`ai::client::complete` checks Pro first.** — Verified: `complete()` in
  `ai/client.rs` calls `crate::entitlement::require_pro().await?` as its first
  line, then `resolve(kind)` then `run(...)`. The design's "Pro → configured →
  consent" ordering citation is accurate.
- **`redaction.rs::purge_one_backup` has three arms (`Words`/`Screens`/`RangeExtras`),
  only `Words` calls `redact_ai_outputs`, and there is one `tx.commit()` after the
  `for job in jobs` loop.** — Verified. The design's MEDIUM-3 fix (insert a
  job-kind-independent `lessons::purge_for_meeting(&mut *tx, meeting_id)` after the
  match and before `tx.commit()`) lands at the correct, real insertion point.
- **`database.rs::delete_meeting` is non-transactional and deletes transcripts by
  raw SQL first, then `meeting_links::purge_meeting`, then `topics`/`chat`
  purge on a freshly acquired connection, then the `meetings` row.** — Verified.
  The design's resolve→delete-rows→remove-dirs ordering and its claim that the
  non-transactional sequence is acceptable because each step is idempotent and
  keyed by `meeting_id`/`source_recording_id` is sound and slots in beside the
  topics/chat calls as described.
- **`study.rs` save uses `BEGIN IMMEDIATE` → `save_locked`, which rechecks pending
  delete and transcript fingerprint.** — Verified; the design's reuse plan for the
  save-time fingerprint recheck is accurate.
- **`study/parse.rs::answer_index` resolves a 0-based index, a letter, or choice
  text.** — Verified; the design's plan to reuse it for MC resolution in the
  deferred §13 generation step is accurate.
- **`entitlements.mas.plist` grants `files.user-selected.read-write`.** — Verified
  (line 55), supporting the certificate/import save-dialog claim.
- **No existing `zip` dependency; `sha2`, `serde_json`, `uuid`, `sqlx` present.** —
  Consistent with the study subsystem using `sha2` already; the design pins
  `zip = "=2.2.0"` pure-Rust, which keeps the `mas` sandbox clean (no system libzip).
- **Study subsystem is the correct template.** — `docs/STUDY_TOOLS.md` and
  `study.rs`/`study/parse.rs` confirm the tolerant-parse → strict-validate → store
  cleaned → render-as-plain-text pattern the design extends. Faithful.
- **No-server / no-default-URL / consent / Pro invariants (CLAUDE.md).** — The
  design's local-catalog + user-supplied-import + seed resolution of "download a
  class," the single user-initiated URL fetch with no default URL, no runtime
  image client, model output never rendered as HTML (grep test for
  `dangerouslySetInnerHTML`), and both-Mac-flavors green all align with CLAUDE.md
  and COORDINATION.md §5.
- **DESIGN.md conformance.** — Icon sizing (nav 14–17px, hero 40–48px,
  `strokeWidth={1.5}`), hazard-yellow `#facc15` on matte black, Inter, sentence
  case, semantic tokens, reduced-motion, WCAG, empty/loading/error states, and
  "Make lesson" fitting the Make-notes/Make-review-guide vocabulary are all
  consistent with DESIGN.md. The four-view expansion is justified and
  authority-backed.

## Unverified / wrong assumptions

- **Figure-review human gate owner (`docs/LESSONS_AUTHORING.md`, "seed content
  reviewer" role, OQ-9).** — Not verifiable here: `docs/LESSONS_AUTHORING.md` does
  not yet exist (it is to be created by the content step) and no named person is
  assigned to the "seed content reviewer" role. The design is honest that this is
  a human gate it cannot automate, so this is not a finding against the design —
  but the accuracy of seed figures/curves ultimately depends on a reviewer and
  checklist that do not yet exist and could not be verified in this review.
- **`study.rs:507` / test `bad_answer_is_retried_once_then_reported_for_that_part`
  and `study/prompt.rs` `CHARS_PER_TOKEN` / `SAFETY = 0.85`.** — These back the
  **deferred §13** pipeline only (not built this build). I confirmed the study
  save/parse patterns exist and are faithful; I did not line-verify these specific
  §13 citations because §13 ships no code this build. They are contract-level and
  carry no risk for the shipped scope.
- **`redaction.rs:667` (`redact_ai_outputs` already calls `study::purge_for_meeting`).**
  — I verified `purge_one_backup`'s arm structure and the single `tx.commit()`
  directly, which is what MEDIUM-3's fix depends on; I did not separately
  line-confirm the `:667` live-path call site. The structural claim the fix relies
  on is verified; the exact line number is incidental.
- **NFR-4 "<1s first explainer on a mid Mac."** — Correctly acknowledged as not
  machine-verifiable; the design routes it to a CI smoke budget plus a recorded
  manual QA step. Not a finding; just flagged as unprovable by automated test.

---

## Notes on scope creep (not findings — within D3 authority)

The §8 depth (SM-2-lite spaced repetition, adaptive trainers, diagnostic
pre-test, mastery dashboard, readiness meter, certificate hash) goes well beyond
the requirements' minimum, but it is explicitly authorized by owner-decision D3
("finish and enhance / make this high end") and schedule-risk items are flagged
"stretch" in §18.4 rather than silently dropped. This is within authority and
not counted against the design.
