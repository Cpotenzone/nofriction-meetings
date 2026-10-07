# Editing and "Strike from the record": shared spec (Mac + iOS)

Users can remove words and screens from a meeting. There are two actions:

| | **Delete** | **Strike from the record** |
|---|---|---|
| Use for | Fixing mistakes, junk, false starts | Content that must not exist anywhere: confidential, legal, personal |
| Content | Removed everywhere (below) | Removed everywhere (below) |
| Trace left | None; the transcript closes up | A visible marker: **"Stricken from the record"**, with the meeting time it covered and when it was stricken |
| Undo | 5-second "Undo" toast, then permanent | **No undo.** A confirmation names exactly what will be destroyed |
| Optional note | — | A short reason (e.g. "privileged"). The marker shows it. It must not contain the removed content |

The marker records **that** something was removed, **where** (meeting time /
position), **when**, and an optional reason. It never records **what**. There
is no hidden copy, no "reveal", and no admin override.

## What the user can select

- **Words**: any run of words inside one transcript line, or one or more whole
  lines. Word selection uses tappable/clickable word tokens, which is reliable
  on both platforms and maps cleanly to character offsets. A drag-select of
  the text is a bonus on the Mac. On the Mac the word picker opens by
  double-clicking a line (or **Edit words**); Esc or **Done** closes it.
- **Transcript lines** (Mac): the same selection as screens. A click selects
  a line (and shows that moment), Shift-click selects every line from the
  last clicked one, ⌘-click adds or removes one, ⌘A selects all while the
  transcript has focus, plus **Select all**, **From here to the end** and
  **Last N minutes**. Delete/Backspace deletes the selection (with the undo
  toast), Esc clears it. A line holding only strike markers can't be
  selected (nothing in it is left to remove).
- **Screens**: one screenshot/photo/frame, or several selected in the
  timeline/gallery. On the Mac: click views a screen, ⌘-click toggles one,
  Shift-click selects every screen from the last clicked (or the one being
  viewed) to this one, ⌘A selects all while the screen strip has focus,
  plus **Select all**, **From here to the end** and **Last N minutes**. The
  selection bar shows the exact count and span ("5 screens · 10:41–10:53"),
  and that count is computed from the same list of ids that is sent.
  Delete/Backspace in the strip deletes the selection (with the undo toast).
- **A block of time** (Mac): "delete everything from 10:41 to 10:53", from a
  screen selection or typed start/end times. See [Time ranges](#time-ranges).
- Actions appear in a context menu / toolbar: **Delete** · **Strike from the record…**

### Linked selection (Mac)

Screens and transcript lines are selected together by time: deleting
screens must not leave behind what was said while they were shown. It is on
by default; the **Linked** switch in the selection bar turns it off, and the
choice is remembered (per user, in the app's local storage).

- A linked selection is a set of time spans: each run of adjacent picks
  (screens or lines) is one span, gaps inside it included. A screen's span
  runs until just before the next screen (or until its recorded end, when
  the next screen comes more than 5 s later). **Last N minutes**, **From
  here to the end** and **Select all** pick that exact span. ⌘-click on a
  highlighted item takes its time out of the selection.
- Both panes then highlight what those spans remove, by the
  [time range](#time-ranges) rules: the screens captured inside, and the
  lines inside (a line only partly inside, by its word timings, is marked as
  split). The bar says so: "17 screens · 42 lines (3 split) · 10:41–10:53 ·
  2 groups". The scrubber shows the spans.
- **Delete** is a time-range delete of every span at once, with one undo.
  It first previews the spans; if the preview would remove anything other
  than what is highlighted, the preview is shown instead of deleting.
  **Strike** shows that preview as its confirmation and leaves markers for
  each span.
- Not linked, Delete and Strike act only on the pane the user selected in,
  and the bar says "screens only" or "transcript only". Several transcript
  lines are deleted as one action (one undo toast).

## "Removed everywhere": the purge checklist

Every action, Delete or Strike, must remove the content from all of these that
exist on the platform:

1. **Transcript text.** Update the segment (or remove the line if it becomes
   empty). For Strike, splice in a marker token at that position (see Rendering).
2. **Search index.** The SQLite FTS rows must reflect the new text. The Mac
   `transcripts_fts` is an external-content FTS5 table with insert/delete
   triggers but **no update trigger**: add `transcripts_au` (delete the old row +
   insert the new one) in the migration that owns `transcripts`, and make sure
   no code path updates `transcripts.text` without it.
3. **Audio.** If the meeting has a saved recording, overwrite that time range
   with **digital silence** in place (re-encode/rewrite the file; don't just
   hide it).
   - With stored **word timings**, silence exactly those words plus 150 ms of
     padding on each side.
   - Without them, silence the **whole line's time span**. This guarantees
     removal, at the cost of extra silence around it.
   - From now on, store word timings at transcription time on both platforms:
     - iOS: `SFTranscriptionSegment` timestamps on 18–25; `SpeechTranscriber`
       audio time ranges on 26+.
     - Mac: whisper token or segment timestamps.
   - This applies to Delete and Strike alike.
4. **Screens.** Delete the image file(s) and every derived file: thumbnails,
   OCR text, extracted frames, cached previews. Delete the DB rows: frames,
   screen states, text snapshots, activity entries that quote that screen's
   text, and VLM/AI analysis of that frame.
   - Text snapshots include browser addresses captured while recording
     (`source = 'browser_url'`, DMG only; [docs/LINKS.md](LINKS.md)). They are
     loose screen text: a screen purge removes those captured while that
     screen was shown, a time range those inside it. No purge may filter
     `text_snapshots` by `source`.
   - Mac DMG flavor: if a **screen video chunk** covers that moment, blank that
     time range in the video with black frames. The order is:
     1. The database purge commits first, so the screens disappear at once
        and for good, and in the same transaction a durable job is queued
        for each covered time range (`video_blank_jobs`: times only).
     2. A background worker blanks the video afterwards, **outside** the
        app-wide redaction lock (no other edit waits on ffmpeg), with
        progress in the meeting view ("Removing from screen video… 40%").
     3. Until a job finishes, that span is **still in the screen video**.
        The meeting view says so, and a Strike marker shows "screen video
        still being blanked". If ffmpeg is missing or blanking fails, the job
        is kept, shown as a persistent warning with Retry, retried with
        backoff (30 s doubling to 1 h, then at each launch), and resumed
        after a quit or crash. Never claim the video is clean before it is.
   - Blanking re-encodes only the keyframe-bounded pieces around the range
     and stream-copies the rest (cost scales with the removed span, not the
     meeting); a chunk whose codec configuration can't be matched is
     re-encoded whole instead. The result is verified (same frame count and
     length, black inside the range) before it replaces the chunk.
   - Blanked ranges are recorded per chunk (`blanked.json`, times only) and
     never re-encoded again; a range with no record that is already black
     (blanked by an older build) is detected and recorded, not redone.
5. **AI outputs derived from it.** Saved notes, summaries, reports, action
   items, insights, catch-ups, assistant chat history and briefings for that
   meeting:
   - redact every occurrence of the removed text (exact and case-insensitive
     match) with the marker, **only when the removed text is distinctive**:
     two or more words, or a single word of 6+ characters that isn't a common
     word (shared stopword list, `LONG_STOPWORDS` on the Mac,
     `RedactionText.longStopwords` on iOS). Removing "plan" or "the" from one
     line must not rewrite every "plan" in the meeting's notes; for those the
     output is only flagged.
   - set a "made before an edit, regenerate?" flag on that output (always,
     distinctive or not)
   - user-authored comments are never rewritten (they're the user's own words
     and the user can edit them)
   - the AI "regenerate" uses the edited transcript, where stricken spans
     appear as `[stricken from the record]`
   - **review guides (study guides, for a class) are deleted, not rewritten**
     (Mac `study_materials`, iOS `StudyMaterial`): summary, key terms,
     flashcards, quiz and questions paraphrase the recording, so matching the
     removed words can't clean them. Any Delete or Strike of transcript text
     (words, lines, time ranges) deletes every part of the recording's guide,
     in the live store and in app backups; the preview/confirmation says so
     (Mac: "The guide in REVIEW (summary, key terms, flashcards, quiz,
     questions): deleted; make it again after the edit"). Screen-only edits
     keep it (it is made from the transcript only). A guide still being
     generated is not saved if the transcript changed meanwhile (the
     transcript fingerprint is re-checked when saving). See
     [STUDY_TOOLS.md](STUDY_TOOLS.md#purge).
6. **Exports.** Share/export/Markdown/Obsidian output from now on renders the
   marker. Files already exported outside the app can't be recalled; the Strike
   confirmation says so in one line.
7. **Database backups made by the app.** On the Mac there's a `backups/`
   folder in app data. Purge the same content from every backup, or delete any
   backup that contains it, and say so in the confirmation.
8. **Freed disk space.**
   - Mac: enable `PRAGMA secure_delete = ON` for the app's SQLite connections,
     so deleted content is overwritten rather than left in free pages. After a
     Strike, run `PRAGMA wal_checkpoint(TRUNCATE)` so no copy survives in the
     WAL.
   - iOS: SwiftData/Core Data's SQLite store. Use whatever the platform
     allows (e.g. NSPersistentStore pragmas `secure_delete`, or a store
     checkpoint/vacuum after Strike). If something can't be guaranteed, say so
     in the confirmation instead of overclaiming.

9. **Links** ([docs/LINKS.md](LINKS.md)).
   - "Said" and "On screen" links are derived from the transcript and screen
     text whenever the list is shown, never stored, so steps 1 and 4 remove
     them. A strike marker is a boundary: no link is built across one.
   - Hidden links are stored only as a salted hash per meeting
     (`meeting_link_hidden`). After every Delete, Strike or time range, hashes
     whose link no longer appears in the meeting are deleted
     (`meeting_links::prune_hidden`, in the Mac's post-commit purge).
   - Added references (`meeting_references` on the Mac, `MeetingReference` on
     iOS) are the user's own words, like comments: a Delete or Strike of
     transcript words never rewrites them.

**Deleting a whole meeting** removes, besides its transcript, screens and AI
outputs: its browser-address rows (`text_snapshots.meeting_id` cascade), its
added references and its hidden-link hashes (Mac: deleted explicitly in
`DatabaseManager::delete_meeting` and by `ON DELETE CASCADE`; iOS: cascade
from `Meeting`).

Device backups (Time Machine, iCloud backup) are outside the app's control.
The Strike confirmation mentions it in one line.

**Recording metadata that isn't transcript or screen text.** A recording's
type (Meeting · Class · Personal; Mac `meetings.recording_kind`), notebook
(Mac `meetings.class_name`, iOS `Meeting.courseName`) and planned length
(`planned_minutes` / `plannedMinutes`) are user-entered fields on the
recording's row, like its title. Delete and Strike of words or screens don't
touch them. **Delete** of the whole recording removes them with the row on
both platforms. The recent-notebook chips are read from the remaining
recordings, never stored separately, so a deleted recording's notebook
disappears with it. Notes of every type (meeting, lecture, personal) are
ordinary AI notes (step 5). See
[TIMED_RECORDING_AND_NOTEBOOKS.md](TIMED_RECORDING_AND_NOTEBOOKS.md#purge).

## Rendering the marker

- **In the transcript:** a dark bar with the text **"Stricken from the record"**
  and a small caption: `10:42–10:43 · stricken Oct 1, 2026 · "privileged"`. It
  is not editable, not selectable as text, and has no content behind it.
- **Screens:** the thumbnail becomes a hatched placeholder card with the same
  label and the capture time.
- **Exports and AI prompts:** `[stricken from the record]` for text,
  `[screen stricken from the record]` for screens.

## Data model

A `redactions` record per action, stored with the meeting (SQLite table on the
Mac, a SwiftData model on iOS):
`id, meeting_id, kind (words|line|screen), action (delete|strike), media_start,
media_end, created_at, reason?`.

- Strike records are shown as markers.
- Delete records are kept only for the 5-second undo, then removed entirely
  (Delete leaves no trace).
- Strike records are never editable. Deleting the whole meeting removes them
  with it.

## Undo for Delete

Hold the change in memory for 5 seconds with an Undo toast, then commit and
purge. If the app quits in that window, the delete is committed; it is never
silently dropped. Strike has no undo.

If the commit fails for a transient reason (database busy, the meeting is
recording), the pending delete is kept, marked failed with a reason, retried
at the next launch and on the next commit, and shown in the meeting view ("1
deletion couldn't be completed — Retry"). It is only dropped when it can never
apply (the target line changed or the screens are already gone), and the user
is told. Screen video blanking is not part of the commit (it's the background
job above), so a video problem never keeps a delete pending.

## Time ranges

"Delete everything from 10:41 to 10:53" (Mac). Within `[start, end]` it removes,
with every step of the purge checklist:

- **Screens** captured in the span (and everything derived from them), plus
  screen text (OCR/accessibility snapshots), AI screen-activity summaries and
  timeline entries captured in the span that no removed screen owns.
- **Transcript lines.** With stored word timings a line is split exactly: a
  word goes when the middle of its time is inside the range. A line without
  word timings goes whole only if at least half of it falls inside (its length
  is estimated from its word count, 0.4 s a word, 1–30 s, bounded by the next
  line), and the preview says how many lines that rule included or kept.
- **Screen video** for the whole span (DMG), via the background job.
- **Moment markers** (★ / ? / ✎, `meeting_markers`) placed in the span, with
  their notes. The preview counts them ("2 moment markers you placed in that
  span"), and a linked Delete that would remove markers shows that preview
  instead of deleting straight away. Like loose screen text they are found
  and removed when the Delete commits, so its 5-second undo keeps them; app
  backups lose them too. Word and line edits leave markers alone (a marker's
  note is the user's own words, like a comment).

The preview shows exact counts first, and the action is refused if the range
now resolves to different counts. The plan (ids, offsets, line hashes; never
content) is stored with a pending Delete, so the undo window and the commit
remove exactly what was previewed; a line edited in between drops the delete.
Delete keeps the 5-second undo. **Strike time range** leaves one marker for the
span in the transcript (in its first line; the rest closes up) and one in the
screen strip, both with the span's times and the reason.

**Several ranges** (a linked selection) are one action. Overlapping or
touching ranges merge, and the preview gives exact totals for all of them
together. A Delete is one pending row with one undo. A Strike leaves, for
each range, a transcript marker (in the first line that range touches) if it
removed words, and a screen-strip marker if it removed screens or no words.
A line cut by two ranges keeps the words between them, and a line without
word timings counts the overlap of every range toward its 50%. Range times
from the UI are whole milliseconds, as the timeline shows them, and an end
covers its whole millisecond, so the UI and the backend agree on every item
at an edge.

## Tests (both platforms)

- Word-range delete and strike update the text exactly and keep word boundaries clean.
- The FTS search no longer finds removed words. The Mac also needs a test for
  the new update trigger.
- Audio: the silenced range is all zeros (decode and check the samples) and the duration is unchanged.
- Screen delete removes the file, derived files and rows. A strike leaves a marker and nothing else.
- Mac: a database created by an older build migrates to every current column
  and screen/transcript deletes work on it.
- Mac: time ranges (word-timing split, the 50% rule, preview counts = what is
  removed, undo, strike markers); screen video jobs (retry with backoff,
  parking, resume at launch, never holding the redaction lock); partial
  blanking (same length and frame count, black only in the range, everything
  else stream-copied), no second re-encode of a blanked range, per-chunk
  offsets, and chunk rotation without gaps. Several ranges in one action:
  exact totals, one undo, a line cut twice, the 50% rule across ranges,
  markers and video jobs per range, and pending deletes from older builds.
  The screen, line and linked selection logic has its own tests (`npm test`:
  Shift ranges, linking both ways, Last N minutes, From here to the end,
  span merging, and counts that equal what is sent).
- AI outputs: occurrences are redacted and the flag is set.
- Markers render in exports, and prompts use the placeholder.
- A Strike can't be undone or edited, and no API returns the removed content.

## Mac implementation notes

Code: `src-tauri/src/redaction.rs` (tests in `src-tauri/src/redaction/tests.rs`),
UI in `src/components/redaction/Redaction.tsx`.

- **Distinctive removals only propagate.** `is_distinctive()` gates the
  rewrite of AI outputs (`meeting_notes`, assistant chats that used the
  meeting, timeline entries, topic clusters), the app log
  rewrite, and the meeting-wide "other copies" check on app backups. A common
  word only sets `stale_after_edit`. The edited line itself is always purged
  from backups; a backup holding a different version of that line that still
  has the words is deleted.
- **Comments.** `meeting_comments` are never rewritten, for Delete or Strike.
- **Review / study guides.** `redact_ai_outputs` calls `study::purge_for_meeting`
  for every line edit (any removal, distinctive or not), in the action's
  transaction and in each backup's purge. `study::save_materials` re-checks
  the transcript fingerprint and pending deletes inside `BEGIN IMMEDIATE`, so
  a guide made from text that was removed during generation is never saved.
- **Moment markers.** `RangeExtras.marker_ids` (`find_range_extras` /
  `purge_range_extras`); `TimeRangePreview.moment_markers`. A meeting delete
  removes them by `ON DELETE CASCADE`.
- **Pending deletes.** A transient commit failure keeps the `redactions` row
  (`pending_payload` set) and records `failed_at` / `failure`. It is retried by
  `commit_all_pending` at launch, after any later commit in the same meeting,
  and by the Retry button (`retry_failed_redactions`). `list_failed_redactions`
  feeds the meeting-view banner; `redaction_failed` carries `retryable`. A
  changed line (text hash mismatch) or missing screens drop it permanently.
- **Whole-line actions** resolve the line's range inside the redaction lock,
  after overlapping pending deletes are flushed (`WordTarget.whole_line`).
- **Recording meetings.** Screens of the meeting being recorded can't be
  deleted or stricken in any flavor ("Stop the recording to edit its
  screens.").
- **Screen purge** also deletes the data editor's history (`data_versions`
  rows for the removed text snapshots and episodes).
- **Lock scope.** `LOCK` covers database work only. Screen video blanking is
  `redaction/video_jobs.rs` (durable queue, single worker, backoff, resume at
  launch, `video_blank_progress` events, `list_video_blank_jobs` /
  `retry_video_blank_jobs`) and `redaction/video_blank.rs` (ffmpeg). The
  worker skips meetings being recorded and stops ffmpeg on quit.
- **Screen video chunks** rotate every 5 minutes (`video_recorder.rs`): the
  next chunk starts and records its first frame before the previous one
  stops, so chunks overlap slightly instead of leaving a gap. Each chunk's
  first-frame time is saved in `chunk_times.json`, and blanking uses each
  chunk's own start.
- **Time ranges** are `redaction/time_range.rs`: `preview_time_range`,
  `delete_time_range`, `strike_time_range` (offsets in ms from the meeting
  start), and `preview_time_ranges`, `delete_time_ranges` and
  `strike_time_ranges` for several at once (a list of `{start_ms, end_ms}`).
  The timeline gives each transcript line its span as these read it
  (`end_ms`, and `word_mids_ms` when word timings are stored; times only),
  so the linked selection (`src/lib/timelineSelection.ts`) highlights
  exactly what they remove.
- **Links in app backups.** A backup's browser-address rows are screen text,
  so the screen and time-range backup jobs purge them like OCR text. Added
  references and hidden-link hashes aren't transcript or screen content, so
  no word or screen job touches them. Deleting a whole meeting doesn't purge
  app backups (as before this feature). Today the app itself writes only the
  archived pre-3.6 ingest queue there (`paths::archive_removed_ingest_queue`),
  which has none of these tables.
- **Schema drift.** Columns added to a table after it first shipped go through
  `database::ensure_columns` (checks `pragma_table_info`), never only into a
  `CREATE TABLE IF NOT EXISTS`, which doesn't alter a table an older build
  created. Databases from before `text_snapshots.meeting_id` existed failed
  every screen delete ("no such column: meeting_id"); the migration adds and
  backfills it. `database/schema_drift_tests.rs` migrates a real old schema
  and checks every current column exists.

## iOS implementation notes

Code: `ios/NoFriction/Redaction/` (`RedactionText` pure text ops, `RedactionEngine`
purge pipeline, `RedactionCenter` undo window + purge queue, `AudioSilencer`,
`StoreHygiene`), UI in `ios/NoFriction/Views/TranscriptEditing.swift` and
`MeetingDetailView.swift`. Tests: `ios/NoFrictionTests/RedactionTests.swift`,
`ios/NoFrictionUITests/ScreensTests.swift` (`StrikeFlowTests`).

- **Marker token.** A strike splices `⟦stricken:<redaction uuid>⟧` into
  `Segment.text`. It holds only the id of the `Redaction` row (time covered,
  date, reason). A word range can never include a marker, so a strike can't be
  edited or undone through a later edit. A struck run of whole lines leaves the
  same token in each line; the view and exports collapse them into one marker.
- **Word timings.** `Segment.audioOffset` (seconds into the AAC file) and
  `Segment.wordTimingsJSON` (`[{location,length,start,end}]`, UTF-16 offsets,
  file seconds) are recorded from iOS 18 `SFTranscriptionSegment` (task audio
  start comes from counting the frames fed to the recognizer) and from the iOS 26
  `SpeechTranscriber` `.audioTimeRange` runs. Lines recorded before this have no
  offset. Their audio range comes from wall-clock time, widened by 1 s each side.
  If the meeting was paused, that estimate can be off, and the confirmation says
  the recording predates word timings.
- **Delete undo.** The row change is saved immediately, and a `Redaction(action:
  delete)` row stores the pending audio ranges and photo file names (no content).
  The removed words are held in memory only for the AI-notes redaction. The delete
  commits after 5 s, when another edit starts, on resign-active/background/terminate,
  or at the next launch (`recover`) if the process was killed. Once it commits,
  the row is deleted. A recovered delete can no longer redact words from the AI
  notes, so it only flags them as made before an edit.
- **AI notes.** Strike replaces each match with `[stricken from the record]`.
  Delete removes the match and closes up the spacing, so no trace is left.
  Matching ignores case, needs word boundaries and allows any whitespace between
  words. Only distinctive phrases are rewritten (`RedactionText.isDistinctive`,
  same rule as the Mac); a common word leaves the notes as they are. Either
  action sets `Meeting.aiNotesStale`.
- **Study guides.** A Strike that removes text, a Delete when it commits (and
  a recovered Delete) delete the meeting's `StudyMaterial` rows
  (`StudyStore.purge`). During a Delete's undo window the guide is kept, like
  the AI notes. `StudyStore.save` refuses a guide whose transcript
  fingerprint no longer matches. The Strike confirmation lists the guide.
- **Moment markers** (`MomentMarker`) cascade with their meeting. iOS has no
  time-range action, so markers are otherwise left alone, like the Mac's
  word and line edits.
- **SQLite.** SwiftData doesn't expose store options, so the app can't set
  `secure_delete` on SwiftData's own connections. The system SQLite on iOS defaults
  `secure_delete` to FAST (2), which zeroes freed cells on pages it already
  rewrites but not whole freed pages. After every committed Delete or Strike the
  app deletes SwiftData history, then opens a second connection that runs
  `secure_delete=ON`, `wal_checkpoint(TRUNCATE)`, `VACUUM` and
  `wal_checkpoint(TRUNCATE)`. A test checks that the raw bytes of `.store`, `-wal`
  and `-shm` don't contain the stricken text afterwards. The app can't overwrite
  the flash blocks the file system frees; the confirmation says so.
- **Audio.** `AVAudioFile` decodes the AAC to PCM in chunks. The app zeroes the
  ranges, re-encodes to AAC with the same rate, channels and bitrate, checks the
  length to within one packet, and calls `FileManager.replaceItemAt`. The test
  decodes the file and checks for exact zeros inside the range, away from the
  1024-sample AAC overlap at its edges.
- **Apple Watch recordings** (`docs/WATCH_APP.md`). The imported file is the
  meeting's audio file (`Documents/Audio/watch-<id>.m4a`), and its segments
  carry `audioOffset` and word timings from the file transcription, so Delete
  and Strike silence it exactly like a phone recording. The silencer
  re-encodes with a bit rate the AAC encoder accepts at the file's rate
  (16 kHz for watch audio). No other copy outlives the import: the watch
  deletes each file once the iPhone confirms it stored it, the inbox files
  are moved into `Audio/` (or, for a paused recording, joined into one file
  there and then deleted), and transcription chunk files in `tmp/` are
  deleted after each chunk (leftovers from a killed run, and half-joined or
  half-silenced files, are removed at launch). Delete Meeting also removes
  any inbox copy of that recording, and a list of imported recording ids
  (ids only) keeps a late re-delivery from bringing a deleted meeting back.
  A recording not yet delivered isn't a meeting; it can be deleted from the
  watch's list.
- **Links.** "Said" links are derived from `Segment.text` each time the
  meeting is shown (`MeetingLinks.items`), so Delete and Strike remove them
  with the words. `MeetingReference` rows cascade with their `Meeting`.
- **Not applicable on iOS:** FTS (in-app search scans `transcriptText`, which
  renders markers as placeholders), app DB backups, screen video chunks, and
  OCR/VLM rows. Photos imported from the Photos library stay there; the
  confirmation says so.
