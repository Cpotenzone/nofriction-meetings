# Moment markers and study tools: shared spec (Mac + iOS)

For students: mark moments while a lecture is recorded, then turn the
lecture into a study guide. User-facing text is in
[USER_GUIDE.md](USER_GUIDE.md#mark-moments-and-study-a-lecture).

## Moment markers

| Kind | Stored | Meaning |
|---|---|---|
| ★ Important | `important` | Something that matters. A one-tap mark is this. |
| ? Question | `question` | Confused, or something to ask the instructor |
| ✎ On the test | `test` | The instructor said it will be on the exam |

A marker has a time (wall clock, the same clock as transcript lines), a
kind and an optional note (at most 280 characters, whitespace closed up).
The kind and note can be changed afterwards, and a marker can be deleted.

- **Mac** (`src-tauri/src/markers.rs`, table `meeting_markers`: id,
  meeting_id, ts, kind, note, created_at; `ON DELETE CASCADE`). Mark with
  the capture bar's **Mark** button, **File → Mark Moment**, or the global
  hotkey **⌃⌥⌘M**, which works while another app is in front. The hotkey is
  registered at launch with `tauri-plugin-global-shortcut` (Carbon
  `RegisterEventHotKey`: no Accessibility permission, allowed in the App
  Sandbox). If another app owns the key, the app still starts and logs it.
  Marks within 800 ms of the previous one in the same meeting are the same
  mark, so the hotkey and the menu accelerator never make two. The
  Recordings view shows markers as pins on the scrubber, inline in the
  transcript (after the line being spoken), and in a list that filters by
  kind and jumps to each moment; **Mark m:ss** adds one at the scrubber.
- **iOS** (`ios/NoFriction/Study/StudyModels.swift`, `MomentMarker`,
  cascade from `Meeting.markers`). **Mark this moment** on the Record screen;
  for six seconds after, the three kinds and **Note** are offered. The
  meeting shows **Marked moments** (filter, change type or note, delete,
  jump to the line) and the markers inline in the transcript. The Apple
  Watch app doesn't mark.

## Study guide

Five parts, each generated and stored on its own:

| Part | JSON (validated, as stored) |
|---|---|
| Summary (lecture notes) | `{"title": s?, "sections": [{"heading": s, "bullets": [s]}]}` |
| Key terms | `{"terms": [{"term": s, "definition": s}]}` |
| Flashcards | `{"cards": [{"front": s, "back": s}]}` |
| Practice quiz | `{"questions": [{"question": s, "choices": [s], "answer": n, "explanation": s, "at_ms": n?}]}` |
| Questions to ask | `{"questions": [{"question": s, "at_ms": n?}]}` |

`at_ms` is the transcript time the quiz answer (or question) comes from, in
ms from the meeting start; the UI links to it ("Jump to this moment").

### AI

- The AI is Apple on-device or the one user-entered OpenAI-compatible
  endpoint, through the same client as every other feature
  (`ai::complete_text` on the Mac, `AIClient` via `MeetingAI.studyGuide` on
  iOS). Consent for a public endpoint, the endpoint policy and (Mac App
  Store build) the `entitlement::require_pro()` check in
  `ai::client::complete` apply unchanged; iOS goes through the same
  Pro → configured → consent steps as the other meeting AI actions. Nothing
  adds or skips a gate. Recording, transcription and markers stay free.
- **Input**: the stored transcript, as notes and email use it. Lines were
  filtered for Whisper/recognizer hallucinations when they were transcribed;
  filtered text is never stored, so it is never fed back. Stricken spans are
  `[stricken from the record]` (the model is told never to guess at them)
  and deleted words are gone. Each line carries its time (`[m:ss] text`).
  The markers are listed as `STUDENT MARKS` with their notes; ✎ and ★
  moments are to be covered first. The class name, when the recording has
  one, is in the header.
- **Small context windows**: if the transcript doesn't fit beside the
  prompt and the answer (Apple's on-device model has 4K tokens in all), it
  is condensed chunk by chunk into time-stamped notes (map step, up to three
  rounds), and every part is written from those notes. Budgets use the same
  3.2 characters/token estimate as the clients, with a 15% margin, so the
  clients never have to trim the middle out.
- **Output is untrusted**: the model is asked for one JSON object. The
  answer is parsed tolerantly (think blocks, code fences, prose around it,
  trailing commas), then every field is checked and cleaned: text trimmed to
  one line, control characters removed, lengths and counts capped,
  duplicates dropped, quiz answers resolved from an index, a letter or the
  choice text, times bounded by the lecture's length. Only the cleaned value
  is stored and shown, as plain text (React text / SwiftUI `Text`, never
  HTML). An unusable answer is asked for once more; after that the part
  shows an error and the other parts are kept. Consent, Pro, configuration,
  key and network errors stop the whole run. Transcript text and model
  output are never logged.

### Storage

- **Mac**: `study_materials` (the table first shipped, unused, for the old
  Dork Mode). `study::ensure_schema` adds `kind`, `json`,
  `transcript_fingerprint` and `created_at` through `ensure_columns`. One
  row per (meeting, kind); remaking a part replaces it.
- **iOS**: `StudyMaterial` (kind, json, transcriptFingerprint, createdAt),
  cascade from `Meeting.studyMaterials`.
- `transcript_fingerprint` is SHA-256 of the transcript exactly as the
  prompt showed it. The guide shows "made from an earlier version of the
  transcript" when it no longer matches.

### Purge

See [REDACTION.md](REDACTION.md) (purge step 5 and Time ranges).

- **Study guide**: any Delete or Strike of transcript text (words, lines,
  time ranges) deletes every part of that meeting's guide, in the live
  store and (Mac) in every app backup. It isn't rewritten: it paraphrases
  the lecture, so matching the removed words can't clean it. The previews
  and the Strike confirmation say so. A Delete's undo window keeps the guide
  (it is deleted when the Delete commits). Screen-only edits keep it.
  Saving a newly made guide re-checks the fingerprint (Mac: inside a
  `BEGIN IMMEDIATE` transaction, also refusing while a Delete is in its
  undo window), so a guide made from text that was removed while it was
  being generated is never written. The Mac also refuses to start while a
  Delete of that meeting is pending or the meeting is recording.
- **Markers**: deleted with their meeting. A Mac time-range Delete or
  Strike removes the markers inside its range, notes included; they are
  found at commit (like loose screen text), so the 5-second undo keeps
  them, the preview counts them, a linked Delete that would remove markers
  shows the preview first, and app backups lose them too. Word and line
  edits leave markers alone (a note is the user's own words, like a
  comment).

### Exports

- **Flashcards as CSV** for Anki and Quizlet: `front,back` rows, no header
  row, CRLF line ends, every field quoted with inner quotes doubled, line
  breaks as spaces, and a leading `'` on a field a spreadsheet would run as
  a formula (`=`, `+`, `@`, `-` not followed by a number).
- **Study guide as Markdown**: summary, key terms, marked moments with times
  and notes, questions to ask (and the moments marked ? Question),
  flashcards, the practice quiz and its answer key. Model text is
  backslash-escaped so it can't become links, images, HTML or headings.
- Mac: the save dialog (`tauri-plugin-dialog`); in the Mac App Store sandbox
  the save panel grants write access to the chosen file
  (`files.user-selected.read-write`, already in `entitlements.mas.plist`).
  The file contents are built in Rust from the stored guide; the web view
  never passes a path to write. iOS: the share sheet, from data in memory
  (no copy is left in the app's folders).

### Study UI

- **Flashcards**: click/tap to flip (Mac: Space or Enter), **Known** (K, →)
  or **Again** (A, ←), **Shuffle** (S) the cards not yet seen, **Start
  over**. "Again" cards come back in another round until all are known.
- **Quiz**: one answer per question, right or wrong with the explanation and
  "Jump to this moment", score at the end, and on the Mac **Retry the
  missed** questions.

## Tests

- Rust (`src-tauri/src/markers.rs`, `src-tauri/src/study/tests.rs`): marker
  CRUD, debounce, kinds, cascade; time-range preview counts, undo keeps
  markers, commit and Strike remove only the ones inside, backups lose them;
  study guide deleted by word Strike, line Delete (kept during undo) and
  time-range Delete, and in backups, kept by screen-only edits; a save after
  an edit or during an undo window is refused; prompts never carry stricken
  or deleted words or marker ids; malformed model output (no JSON, truncated,
  wrong shapes, bad quiz answers and times, deep nesting) never panics;
  chunking keeps every request inside a 4K window; a bad answer is retried
  once; consent/Pro/network errors stop the run; CSV and Markdown escaping.
  All with a mock AI.
- TypeScript (`src/lib/studyLogic.test.ts`): marker filters and placement,
  time-range hiding, flashcard rounds and shuffle, quiz scoring, stored-data
  checks.
- Swift (`ios/NoFrictionTests/StudyTests.swift`): JSON parsing and cleaning,
  quiz answers, clocks, the marker model, cascade, prompt input, chunking and
  retry with a mock AI, CSV/Markdown, deck and quiz state, and the purge on
  Strike and Delete.
