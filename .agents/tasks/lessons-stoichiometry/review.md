# Lessons subsystem — consolidated gate verdict

**Verdict: CHANGES_REQUESTED**

Two independent reviews were run against `feature/lessons-stoichiometry`:

- **review-a** (backend / correctness / invariants / chemistry): did **not** approve. One MEDIUM grading-correctness gap plus two NITs.
- **review-b** (frontend / UX / accessibility / design / offline-UI / docs): approved its own scope, but explicitly stated the approval is frontend-scoped and does **not** clear review-a's finding. Three NITs.

Because the gate requires BOTH reviewers to approve with no HIGH/MEDIUM findings, and review-a is NEEDS_CHANGES with an open MEDIUM, the overall verdict is **CHANGES_REQUESTED**.

## Must-fix before approval (HIGH / MEDIUM)

1. **[MEDIUM] Ungradeable authored units** (review-a) — `src-tauri/src/lessons/grade.rs`.
   Ten seed numeric items/steps declare answer units (`mol/L` x4, `g/mol` x2, `%` x4) that are not in the grader's `UNITS` table. A learner who enters the correct number *with* the prompted unit (e.g. "0.1 mol/L", "80 %") is graded `Unparseable`. The answer keys are chemically correct and the bare-number path grades Correct, so this is a grader/content consistency bug. Fix: add molarity, molar-mass and percent as unit classes (or strip a trailing known-dimensionless unit), and add a seed-validation assertion that every authored numeric unit round-trips through `lookup_unit`.

## Also carried (NITs — not blocking on their own)

- **[NIT]** Seed numeric/worked-example keys are finiteness-checked, not recomputed, despite the LESSONS.md §4.6 full-recompute claim. Recompute numeric keys from the shared molar-mass table in the shipped-seed test. (review-a)
- **[NIT]** `lessons_start_exam` counts `checkpoint_passed` progress rows instead of joining to the current set's lesson ids; a stale row could inflate the pass count. (review-a)
- **[NIT]** Adaptive trainer updates server mastery from a client-computed verdict; submit the raw answer for Rust to grade so the authority story is uniform. Low-stakes — checkpoints, exam, certificate stay server-graded. (review-b)
- **[NIT]** Shuffled MC radiogroup is Tab-operable (WCAG 2.1.1 passes) but lacks arrow-key roving focus expected by the ARIA radiogroup pattern. (review-b)
- **[NIT]** README "fourth view (⌘3)" phrasing is momentarily confusing — clarify that Lessons is the fourth view at ⌘3 with Chat moving to ⌘4. (review-b)

## Note on severity reconciliation

review-b's narrative refers to review-a's scoring finding as "HIGH." The authoritative review-a structured output (review-a.json and review-a.md) classifies the open blocker as **MEDIUM** (the exam/checkpoint scoring denominator was verified server-authoritative, with blanks → zero). This gate carries each finding at the severity its own source assigned. The gate blocks regardless, since review-a did not approve.

## What both reviewers confirmed is solid

Bundle integrity (per-file SHA-256, exact file-set match, path-traversal rejection, figure re-hash + decode, content fingerprint), transactional import with staging→rename, authoritative Rust grading that never panics, server-side exam/checkpoint scoring over the persisted selection (one-item post cannot reach 100%), mastery EMA + SM-2-lite persistence, generated-only purge wiring, the build-time offline guard (verified to pass on a real build and fail on an injected remote reference) plus the restrictive Tauri CSP, no model/bundle output rendered as HTML, design-token discipline with focus rings and WCAG-passing contrast, three UI states on every screen, reduced-motion handling, an honest coming-soon Generate button with no AI/network call, and honest docs. Both Mac flavors build; the policy and offline guards pass; backend 28 tests and frontend 9 lessonLogic tests pass; `tsc --noEmit` is clean.
