# Owner decisions — Lessons subsystem

These are authoritative product decisions made by the repo owner during the
build of the Lessons feature. They are recorded here so design/review steps can
verify them against a durable source rather than taking the design doc's word.

Owner: Casey (repo owner). Date: 2026-10-10.

## D1 — Lessons is its own top-bar tab (CONFIRMED)
Owner instruction, verbatim: "gets it own tab." and later "finish and enhance."
Decision: Lessons is a first-class fourth top-bar view —
Record · Recordings · Lessons · Chat (⌘1–4). This overrides the DESIGN.md
"three views, nothing under a MORE menu" guidance specifically for Lessons,
because the owner designated interactive education as "our first major push."
The design doc must keep the written justification against the three-view rule.

## D2 — Generate-from-recording (Source Path A) is DEFERRED and ships as a stub (CONFIRMED)
Owner instruction, verbatim: "add a generate button that we will make work later."
Decision: For THIS build, the recording→lesson AI generation pipeline is NOT
implemented. Instead the Lessons view ships a visible, first-class **Generate**
button whose action is an intentional, honest "AI lesson generation — coming
soon" preview state (describing what it will do: turn a Class recording into an
interactive lesson). It makes no AI call and is not a dead/broken control.
The full generation contract is captured as a deferred (vNext) section with an
explicit precondition: any future AI-generated explainer data must pass the
Rust-side recompute-and-replace validation plus an accuracy gate before a
generated lesson can be marked playable.
Consequence: this build ships ONLY human-authored/verified seed lessons (the 6
stoichiometry lessons) and user-imported bundles. No lesson is AI-generated in
this build.

## D3 — "finish and enhance": high-end POC (CONFIRMED)
Owner instruction, verbatim: "finish and enhance." / "make this high end."
Decision: ship the enhancement set — spaced-repetition review (SM-2-lite over
the mastery EMA), adaptive-difficulty trainers, full worked-solution reveal on
every miss, optional diagnostic pre-test, per-explainer hero interactions,
mastery dashboard with exam-readiness, polished printable offline certificate
with a local verification hash, and a per-lesson common-misconceptions
explainer. Items genuinely at schedule risk are marked "stretch," never
silently dropped. All existing invariants remain intact (no server, no runtime
AI/image calls, no model-output-as-HTML, both Mac flavors build, policy guard
passes, fully offline + free to take, Mac-first with iOS parity planned).

## Note to the design reviewer
HIGH-1 and MED-2 from the prior review are resolved by D1 and D2 above: the
fourth-view decision and the defer-Source-Path-A-to-a-stub decision are both
explicit owner decisions, recorded here. Please treat "built now" for Source
Path A as superseded by D2 (owner chose a stub now, pipeline later). The
remaining spec gaps (delete cascade ordering/atomicity, sub-80% checkpoint
remediation gating, exam over-provisioning floor) are real and should be closed
in the design.

## D4 — Figures are REQUIRED, not stretch (CONFIRMED)
Owner instruction, verbatim: "not slop ... AI you generate good visuals using
the best models possible. this is a high value POC." and "finish and enhance."
Decision: AI-authored figures (genmedia / gemini-3-pro-image, "Nano Banana
Pro") are a REQUIRED part of the shipped seed set, not a stretch item. The six
stoichiometry seed lessons must each embed at least one figure-kind explainer
backed by a decoding embedded PNG, and seed validation must assert this. Remove
the "stretch" hedge on the figure set and the "all-deterministic-SVG seed is
shippable" escape hatch. Deterministic SVG explainers are additive to, not a
replacement for, the authored PNG figures. (Resolves design-review Finding 1 via
fix (a).)

## D5 — Offline guarantee must be enforced on the FRONTEND path too (CONFIRMED)
Owner instruction, verbatim: "yes work fully offline."
Decision: "Zero runtime network when taking a present lesson" must be enforced
and tested on the React/CSS render path, not only by stubbing the Rust AI
client. Authoritative mechanism: a build/test guard that scans the compiled
Lessons bundle + Lessons.css for any non-local http(s):// or url(...) reference
to a remote host and FAILS on a match, plus a Tauri CSP note restricting
connect-src/img-src/font-src for the Lessons surface and an assertion that
Lessons CSS uses only the bundled Inter font and imports no remote asset.
(Resolves design-review Finding 2.)
