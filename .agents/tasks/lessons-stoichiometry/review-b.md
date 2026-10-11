# Lessons frontend / UX / accessibility / design / offline-UI / docs review (review-b)

The branch `feature/lessons-stoichiometry` adds a fourth top-bar view, Lessons, and the whole React surface behind it: a library with Import and an honest coming-soon Generate preview, a course dashboard with per-concept mastery and an exam-readiness meter, a lesson player with eight explainers (figure plus seven interactive kinds), a shared quiz runner for checkpoints and the final exam, an adaptive trainer with spaced-repetition review, and a printable offline certificate. Correctness and grading stay authoritative in Rust; React renders validated structured data through typed components and never renders model/bundle output as HTML. The offline guarantee is enforced on the frontend path by a build-time grep guard over an isolated Lessons chunk plus a restrictive Tauri CSP — I verified both that the guard passes on a real build and that it fails on an injected remote reference. Watch for: the adaptive trainer updates server mastery from a client-computed verdict rather than a server round-trip (confirmed, design-sanctioned low-stakes path), and the MC radiogroup is Tab-operable but doesn't implement arrow-key roving focus (confirmed, WCAG-passing but an incomplete ARIA pattern). Both are NIT.

**Verdict**: APPROVED

Note on scope: this review covers frontend / UX / accessibility / design / offline-UI / docs. The backend review (review-a) raised a HIGH on exam/checkpoint scoring (score computed over the client-submitted answer set); that is a Rust `commands.rs` concern outside this review's scope and is not re-adjudicated here. My APPROVED verdict is scoped to the frontend surface; the overall feature's shippability still depends on review-a's HIGH being resolved.

## High-level view

The fourth-tab wiring lands on every surface the spec and D1 name, with nothing left half-wired. `AppMode` in Shell.tsx becomes `"record" | "recordings" | "lessons" | "chat"`, Shell routes `lessons` to `LessonsView`, and both the Shell and App file-header comments are updated to four views. TopBar lists Record·Recordings·Lessons·Chat with ⌘1–4 titles and a GraduationCap glyph on the Lessons tab; `menu_builder.rs` inserts Lessons at ⌘3, pushes Chat to ⌘4, and updates `handle_menu_event`, `shows_window`, and the menu-id test list; App.tsx adds the matching `menu:view_lessons` listener. The ten appended icons are 24×24 `currentColor` stroke glyphs on an `aria-hidden` base; Generate reuses the existing SparkleIcon. No emoji anywhere in the Lessons tree.

The explainers are genuinely interactive and keyboard-operable rather than canned animations. The equation balancer parses formulas and drives a live per-element mass-balance meter off bounded number inputs; mole-ratio and dimensional-analysis are click-to-advance step/tile builders (keyboard buttons, not mouse-only drags); the limiting-reagent simulator and the ideal-gas and titration explorers use native range sliders; the molecule diagram is a static labelled SVG. Numeric results come from the authored, Rust-verified keys, so the UI never fabricates chemistry, and spoilers are withheld until the learner reaches the answer. Each explainer carries the author's `a11yState` text-equivalent, and the molecule diagram's a11y list now includes bond order, not just atoms.

No model or bundle output is rendered as HTML — there is no `dangerouslySetInnerHTML`/`innerHTML` in the entire Lessons render path, authored strings render as text nodes, and each explainer renders from a typed `data` payload through a `switch` on `kind` with a plain-text fallback for an unknown kind.

The offline guarantee is provable on the frontend path, not asserted. `vite.config.ts` isolates the Lessons surface into its own chunk, `scripts/check-lessons-offline.mjs` scans that chunk and the compiled CSS for any remote `http(s)`/`url(...)`/`@import`/`@font-face` and any non-bundled font and fails on a match, it is wired as `postbuild`, and I confirmed end-to-end that a real `vite build` plus the guard passes while a crafted remote reference makes the guard exit non-zero. The Tauri CSP backstops with `connect-src 'self'`, `img-src 'self' data: blob: asset: http://asset.localhost`, `font-src 'self' data:`.

Design-token discipline holds throughout — hazard yellow on matte black, Inter, sentence case — with every interactive control carrying a visible 2px hazard-yellow focus ring, WCAG-passing contrast, and the three required states (loading / error / empty) present on every screen. Docs are updated honestly: README, ARCHITECTURE, CHANGELOG (3.8.0), and REDACTION describe the real shipped scope, state Mac-ships-fully with the SwiftUI render layer as a documented iOS follow-up, and the REDACTION lesson-purge entry correctly scopes purge to generated-provenance sets while exempting seed/imported.

The only frontend concerns are two NITs: the adaptive trainer grades client-side and sends the resulting boolean to Rust (so a crafted client could inflate practice mastery — but not a checkpoint, exam, or certificate, all of which stay server-graded), and the shuffled MC radiogroup is reachable and operable by Tab but doesn't implement the arrow-key roving-tabindex that the ARIA radiogroup pattern expects.

<details>
<summary>Issues (3)</summary>

1. **Trainer mastery updates from a client verdict (NIT)** — `TrainerSession.grade` computes correctness with `previewNumeric` client-side and calls `submitTrainerItem(setId, conceptId, correct, …)`, so Rust records a client-decided boolean for the practice/mastery schedule instead of grading a submitted answer. Low-stakes (checkpoints, exam, and the certificate all stay server-graded), and the component comment acknowledges the preview-cue design — but to keep "grading authoritative in Rust" literal, submit the raw answer and let Rust grade the trainer item too.
2. **MC radiogroup lacks arrow-key roving focus (NIT)** — `ChoiceGroup` renders `role="radio"` buttons in a `role="radiogroup"`; each is individually Tab-focusable and activatable (so keyboard operation and WCAG 2.1.1 pass), but the ARIA radiogroup pattern expects Up/Down/Left/Right to move selection within a single tab stop. Add roving `tabIndex` + arrow-key handling, or use native `<input type="radio">`.
3. **README "fourth view (⌘3)" phrasing (NIT)** — the README Lessons row says "a fourth view (⌘3)"; Lessons is the fourth view but sits at ⌘3 (Chat moved to ⌘4). Accurate but momentarily confusing; consider "a fourth view at ⌘3 (Chat moves to ⌘4)".

</details>

<details>
<summary>Details</summary>

### Interactive explainers are genuine, accurate, and keyboard-operable

The seven interactive kinds all drive real state from the validated `data`, not a scripted animation. The equation balancer parses each formula into element counts and recomputes a live left-vs-right meter per element as the learner edits bounded (1–12) number inputs; the bar turns `is-match` green only when every element balances, and the coefficients are only named once balanced. Mole-ratio and dimensional-analysis are step/tile builders advanced by keyboard-reachable buttons (start / next / place / remove), with the running product and unit cancellation updating as tiles land and the result withheld until the final stage. The limiting-reagent simulator derives max reaction extent from moles/coefficient, drains each reactant bar off a range slider, and only names the limiting species at completion. The ideal-gas and titration explorers bind native sliders to PV=nRT and a plotted curve. The molecule diagram is a static labelled SVG. Every numeric answer traces to the authored, Rust-verified `finalAnswer`/`value`/`answer`, so the presentation layer never invents chemistry.

Each explainer appends the author's `a11yState` as a `role="note"` text-equivalent and the interactive controls carry `aria-label`s. The molecule diagram's a11y list enumerates both atoms and bond order (single/double/triple), closing the earlier "composition without connectivity" gap. The figure explainer resolves a bundled PNG through `convertFileSrc` (the local Tauri asset protocol) and degrades to its alt text on a decode/path failure, so a failed figure never breaks a lesson and never triggers a network fetch.

### No model/bundle output rendered as HTML

Grepping the full Lessons render path (`src/components/lessons/**`, `LessonsView.tsx`, `lessons.ts`, `lessonLogic.ts`) for `dangerouslySetInnerHTML`/`innerHTML`/`outerHTML` returns nothing. `Explainer.data` is typed `Record<string, unknown>`, read field-by-field inside each typed component, and dispatched through a `switch` on `kind` whose default renders the plain-text caption. Authored strings are rendered as text nodes everywhere.

### Offline guarantee is provable on the frontend path (D5)

`vite.config.ts` adds a `manualChunks` rule isolating `components/lessons/**`, `views/LessonsView`, `lib/lessonLogic`, and `lib/lessons.ts` into one `lessons` chunk, so the guard scans exactly the Lessons bytes. `scripts/check-lessons-offline.mjs` scans the compiled `lessons-*.js` chunk(s) and the compiled CSS for any non-local `http(s)` reference, remote `url(...)`, remote `@import`/`@font-face`, and any `font-family` that doesn't resolve to the bundled Inter/system stack, failing on a match. It allow-lists namespace/`w3.org`/`data:`/`#`/relative/`tauri:` targets, and asserts the chunk actually contains the Lessons surface via marker strings so it can't pass on an empty scan. It is wired as `postbuild` and `check:lessons-offline` and runs dependency-free.

I verified this end-to-end rather than trusting the wiring. `npx vite build` emits a dedicated `dist/assets/lessons-C2_0eumD.js` (53.37 kB) and `lessons-*.css`, and `node scripts/check-lessons-offline.mjs` prints `{"status":"PASS", ... "scanned":{"jsChunks":1,"cssFiles":1}}` (exit 0). I then appended a remote `@import url(https://fonts.googleapis.com/...)` and a `url(https://evil.example.com/track.png)` to a copy of the compiled Lessons CSS placed in `dist/assets/`; the guard reported all three remote references and exited 1. The guard genuinely catches a regression, not just a clean build.

The CSP backstop in `tauri.conf.json` is present and restrictive — `connect-src 'self'`, `img-src 'self' data: blob: asset: http://asset.localhost`, `font-src 'self' data:` — so even a bundle regression couldn't reach a remote host at runtime. Lessons.css declares only the `"Inter", -apple-system, BlinkMacSystemFont, sans-serif` stack and imports no remote asset.

### Design tokens, focus, contrast, and the three states

Lessons.css uses `var(--token, fallback)` throughout, and every token (`--hazard-yellow` #facc15, `--bg-main` #050505, `--bg-card` #0d0d0d, `--text-primary`, `--text-secondary`, `--accent-success`, `--accent-danger`, `--tactical-gray`, `--border-color`, `--hud-border`) is defined in App.css. There is no raw hex in the CSS or the SVG render path — the molecule atom label uses `var(--bg-main, #050505)` and the bond/atom strokes use `--text-secondary`/`--hazard-yellow`, so the earlier non-tokenized `#050505` is gone. Contrast is comfortable: hazard yellow on matte black is ~13:1 and the primary/secondary text are ~18:1 / ~8.6:1, all clearing WCAG AA.

Every interactive control has a visible 2px hazard-yellow `:focus-visible` ring, including the explainer number field and the range sliders, which otherwise fall through to an unreliable UA default on matte black. The three required states are present and distinct: the library has loading / error / empty (`.lessons-state`, non-empty on first run via the seed), the course and lesson/exam/certificate screens each render loading and error branches, and the certificate screen has a "no certificate yet" empty state. A scoped `.lessons-sr-only` visually-hidden class voices the "mastered" state that an `aria-hidden` check icon conveys visually. The misconception `✗` is an `aria-hidden` dingbat with the real meaning carried in adjacent text — not an emoji UI icon.

### Reduced motion

The CSS `@media (prefers-reduced-motion: reduce)` zeroes transitions/animations under `.explainer *` and `.bar-fill`. The one JS-initiated motion path — `LessonPlayer.scrollToExplainer`, used by the resume button and remediation routing — now reads `matchMedia("(prefers-reduced-motion: reduce)")` and falls back to `behavior: "auto"` when reduced motion is requested, so the earlier gap where a JS smooth scroll bypassed the media query is closed.

### Generate is an honest coming-soon preview (D2)

The Generate affordance is a first-class primary button in the library (not disabled, not an error), and `GeneratePreview` opens a centered coming-soon panel that describes the planned recording→lesson capability, states it will require Pro + an AI provider when it ships, and points the user at the seed course or Import. It makes no `invoke`, no entitlement check, and no network request — confirmed by reading the component. This matches D2 exactly.

### Quiz runner closes two presentation-side integrity holes

`QuizRunner` is shared by checkpoints and the exam. It shuffles MC choices per item with an id-seeded Fisher–Yates and reports the *authored* index back on select, so "always click the first option" no longer scores well, while grading against the stored `answerIndex` is unaffected. Multistep items render a real per-step input (mc/numeric/balance) and populate `SubmittedAnswer.multistep`, so they are gradeable from the UI instead of silently scoring zero. Pass/fail and scoring are a Rust round-trip (`lessons_submit_checkpoint`/`lessons_submit_exam`), and the worked solution/explanation are revealed only after submission. (The server-side scoring-denominator gap is review-a's HIGH, in Rust, not a presentation concern.)

### Trainer grades client-side (NIT)

The adaptive trainer is the one surface where the client decides correctness. `TrainerSession.grade` runs `previewNumeric` locally and then calls `submitTrainerItem(setId, item.conceptId, correct, true)`, so Rust records a client-supplied boolean that advances mastery and the SM-2-lite review interval. The component comment frames `previewNumeric` as a preview cue and treats mastery/schedule as authoritative-in-Rust, but here Rust is only persisting the client's verdict for the trainer, so a crafted client could inflate practice mastery and clear "review due" without actually answering. The blast radius is limited to the local practice signal — checkpoints, the exam, and the certificate all submit answers for Rust to grade — so this is a NIT, but submitting the raw trainer answer for server grading would make the authority story uniform.

### Client preview logic is preview-only and mirrors Rust

`lessonLogic.ts` is explicitly preview-only: it never writes a stored score or decides pass/fail, mirrors the Rust units table 1:1 (so a compatible unit never shows a false "unreadable"), and provides only numeric hints, a mastery-bar preview, resume index, and an exam-unlock preview. Its node `--test` suite passes 9/9 (verified: `TZ=UTC node --test src/lib/lessonLogic.test.ts`), and `npx tsc --noEmit` is clean across the new surface.

### Docs are updated honestly

README adds a Lessons row describing the six-lesson stoichiometry course, offline+free taking, Rust-graded checkpoints, the 30-question exam, the offline certificate, and the honest Generate stub. ARCHITECTURE documents the Rust `lessons/` modules and the React surface, states correctness is authoritative Rust with no AI/image call when taking a lesson, and notes generated-provenance sets join the purge while seed/imported are exempt. docs/CHANGELOG adds a 3.8.0 entry with the fourth-view ⌘1–4 framing, the offline certificate, the no-AI-call Generate button, and the SwiftUI iOS render layer called out as deferred. REDACTION's lesson-purge entry deletes generated-provenance sets (rows + learner records + on-disk bundle) via `lessons::purge_for_meeting`, exempts seed/imported, and honestly states it is dormant-but-wired since no path writes `generated` in this build. LESSONS.md §16 states "Mac ships fully; iOS is a documented follow-up," naming the SwiftUI render layer as the deferred piece.

</details>

<details>
<summary>File map</summary>

- `src/components/Shell.tsx` — `AppMode` gains `lessons`, routes it; header comment updated to four views.
- `src/components/TopBar.tsx` — four modes, ⌘1–4 titles, GraduationCap glyph on Lessons.
- `src/App.tsx` — `menu:view_lessons` listener; header comment updated to four views.
- `src/components/icons.tsx` — ten appended Lessons glyphs (24×24, currentColor, aria-hidden base).
- `src/components/views/LessonsView.tsx` — route between library / course / generate; loading + error states.
- `src/components/lessons/LessonLibrary.tsx` — catalog grid, Import, Generate; empty/loading/error.
- `src/components/lessons/CourseView.tsx` — lesson list, mastery dashboard + readiness, exam flow, certificate, start-over.
- `src/components/lessons/LessonPlayer.tsx` — explainers, worked examples, misconceptions, checkpoint + remediation routing (reduced-motion-aware scroll).
- `src/components/lessons/Explainers.tsx` — figure + seven interactive explainers (tokenized SVG, atom+bond a11y).
- `src/components/lessons/QuizRunner.tsx` — shared runner; MC shuffle + authored-index mapping, multistep inputs.
- `src/components/lessons/TrainerPanel.tsx` — spaced-repetition review + adaptive trainer (client-graded trainer NIT here).
- `src/components/lessons/GeneratePreview.tsx` — honest coming-soon preview, no AI call.
- `src/components/lessons/Lessons.css` — tokenized styles, focus rings, three states, reduced-motion media query.
- `src/lib/lessons.ts` — typed Tauri command wrappers + content shapes.
- `src/lib/lessonLogic.ts` + `.test.ts` — preview-only client logic (9 passing node tests).
- `vite.config.ts` — `manualChunks` isolating the Lessons chunk for the offline guard.
- `scripts/check-lessons-offline.mjs` — frontend offline grep guard (postbuild; verified pass + verified fail-on-remote).
- `src-tauri/src/menu_builder.rs` — Lessons menu item at ⌘3, Chat at ⌘4, test list + shows_window updated.
- `src-tauri/tauri.conf.json` — version 3.8.0; restrictive CSP backstop.
- `README.md`, `docs/ARCHITECTURE.md`, `docs/CHANGELOG.md`, `docs/REDACTION.md`, `docs/LESSONS.md` — docs updated honestly.

Full diff: `git diff main` on `feature/lessons-stoichiometry`.

</details>
