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
  the text is a bonus on the Mac.
- **Screens**: one screenshot/photo/frame, or several selected in the
  timeline/gallery.
- Actions appear in a context menu / toolbar: **Delete** · **Strike from the record…**

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
   - Mac DMG flavor: if a **screen video chunk** covers that moment, blank that
     time range in the video (re-encode with black frames via the existing
     ffmpeg path).
   - If blanking fails, the action fails loudly, rolls back, and tells the user.
     Never claim success while the content still exists.
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

Device backups (Time Machine, iCloud backup) are outside the app's control.
The Strike confirmation mentions it in one line.

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

If the commit fails for a transient reason (database busy, ffmpeg missing,
blanking failed, the meeting is recording), the pending delete is kept, marked
failed with a reason, retried at the next launch and on the next commit, and
shown in the meeting view ("1 deletion couldn't be completed — Retry"). It is
only dropped when it can never apply (the target line changed or the screens
are already gone), and the user is told.

## Tests (both platforms)

- Word-range delete and strike update the text exactly and keep word boundaries clean.
- The FTS search no longer finds removed words. The Mac also needs a test for
  the new update trigger.
- Audio: the silenced range is all zeros (decode and check the samples) and the duration is unchanged.
- Screen delete removes the file, derived files and rows. A strike leaves a marker and nothing else.
- AI outputs: occurrences are redacted and the flag is set.
- Markers render in exports, and prompts use the placeholder.
- A Strike can't be undone or edited, and no API returns the removed content.

## Mac implementation notes

Code: `src-tauri/src/redaction.rs` (tests in `src-tauri/src/redaction/tests.rs`),
UI in `src/components/redaction/Redaction.tsx`.

- **Distinctive removals only propagate.** `is_distinctive()` gates the
  rewrite of AI outputs (`meeting_notes`, `study_materials`, assistant chats
  that used the meeting, timeline entries, topic clusters), the app log
  rewrite, and the meeting-wide "other copies" check on app backups. A common
  word only sets `stale_after_edit`. The edited line itself is always purged
  from backups; a backup holding a different version of that line that still
  has the words is deleted.
- **Comments.** `meeting_comments` are never rewritten, for Delete or Strike.
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
- **Not applicable on iOS:** FTS (in-app search scans `transcriptText`, which
  renders markers as placeholders), app DB backups, screen video chunks, and
  OCR/VLM rows. Photos imported from the Photos library stay there; the
  confirmation says so.
