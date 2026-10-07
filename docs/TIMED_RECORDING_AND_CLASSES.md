# Timed recording and classes (Mac + iOS)

Two features for students and classrooms:

- **Timed recording.** Record asks "How long?" and the recording stops by itself at the end.
- **Classes.** A recording can belong to a class, and its notes are written as lecture notes.

User-facing copy is in [USER_GUIDE.md](USER_GUIDE.md#how-long-timed-recording).

## "How long?"

| | |
|---|---|
| Choices | 15, 30, 60, 90 min, **No limit (∞)** |
| Preselected | The last choice: Mac setting `recording_default_duration`, iOS `UserDefaults` `recordingDefaultLength` ("15" … "90", "none") |
| Never chosen | No limit, so a start that skips the sheet never cuts a recording short |
| Mac keys | 1–5 pick (not while typing in Class), Enter starts, Esc cancels |
| Sheet | Also has the optional **Class** field with recent classes as chips |

### Which starts show the sheet

Only a click or tap on a Record button shows it. Every other start uses the remembered choice:

| Mac start path | Sheet? |
|---|---|
| START CAPTURE (navbar), LIVE "Start recording", Zen START | Yes |
| ⌘N (File → New Recording). The Mac app has no global hotkey; this is the shortcut | No: remembered |
| Tray **Start Recording (60 min)** | No: remembered |
| Tray **Start Recording For ▸ 15/30/60/90/No Limit** | No: that length (and remembers it) |
| Tray Capture Mode ▸ Ambient / Meeting | No: remembered |
| Command palette "Start Recording" | No: remembered |
| "Start New Segment" (75-minute prompt) | No: keeps the class and the time that was left |
| Meeting auto-detect | Doesn't exist (`auto_start_recording` is never read). Anything added later goes through `start_recording` and gets the remembered length |

On iOS the Record button shows the sheet (after the one-time recording notice the first time). The `-NFAutoRecord` test start uses the remembered length.

The backend applies the rule, so no path can bypass it. On the Mac, `start_recording(plan?)` resolves a missing `plan.duration` to the remembered length and **every** start arms a plan. On iOS, `RecordingSession.start(limit:className:)` does the same with `limit == nil`.

## Auto-stop

- **Wall clock.** The deadline is start + length. Pausing doesn't move it: a class ends when it ends.
- **Same stop path as Stop.** Stop must call `end_meeting` (CLAUDE.md).
  - Mac: at the deadline the backend emits `timed-recording-auto-stop` `{meetingId}`. The UI stops through the user's own Stop (video, accessibility unlink, `stop_recording`). If the recording is still running 5 s later (the window is closed or the webview asleep), the backend calls `stop_recording_from_backend`. Both end in `stop_recording_core`, which runs `end_meeting` and the auto-report.
  - iOS: `RecordingSession.stop()`.
- **Never another recording.** On the Mac a plan is keyed to its meeting id and a generation counter (`timed_recording::Registry`). A stop disarms it; a new start replaces it. A timer whose generation is gone exits without acting. The backend fallback also checks that the same meeting is still recording and that the stop wasn't overtaken by +15 min. On iOS `TimeLimitSlot` keys the plan to the meeting's UUID.
- **Extend.** +15 min moves the deadline (from now if it already passed), re-arms the warning and updates `planned_minutes`. The total is capped at 12 h. "No limit" clears the deadline and sets `planned_minutes` to NULL/nil. A stale +15 click from the UI carries the meeting id it shows and is refused for any other meeting.
- **Warning.** 5 minutes before the end, or 2 minutes for a plan of 15 minutes or less.
  - Mac: the event `timed-recording-warning` drives an in-app banner with +15 min / No limit (in every view). If the window isn't in front, the Dock icon bounces and a notification is posted (tauri-plugin-notification). It has no action buttons on macOS, so the actions are in the banner, the capture bar and the tray (**Add 15 Minutes**, **Remove Time Limit**). Clicking the notification brings the window up.
  - iOS: a local notification with **+15 min** and **No limit** actions (category `TIME_LIMIT`, handled by `MeetingEndNotifier`'s delegate). It is scheduled ahead with a time trigger, so it fires while the app is suspended, and rescheduled on +15. It is posted only if notification permission is already granted. The app asks in context, at the first timed recording, never at launch. The Record screen shows the same warning as a banner.
- **While recording.**
  - Mac: the capture bar shows "12:34 left" (or elapsed time with no limit) and +15 min / No limit. The tray shows "Time left: 12 min".
  - iOS: the header clock shows the time left, with a "Stops at 10:45 · +15 min · No limit" row.

## Classes

- **Storage.**
  - Mac: `meetings.class_name TEXT` and `meetings.planned_minutes INTEGER`, nullable, added by `classes::ensure_schema` through `database::ensure_columns`. `CREATE TABLE IF NOT EXISTS` never adds columns. `schema_drift_tests.rs` migrates the old schema.
  - iOS: `Meeting.courseName` and `Meeting.plannedMinutes`, optional, so stores from older builds migrate without a schema version (tested against a copy of the previous model). The attribute isn't called `className` because Core Data resolves that key to NSObject's `className`.
- **Names.** Trimmed, whitespace collapsed, ≤ 80 characters. A name that matches an existing class ignoring case uses the existing spelling ("bio 101" joins "BIO 101").
- **Recent classes.** Derived from the meetings that have one, most recent first. No separate list is stored.
- **Set it** in the Record sheet. Edit or clear it later on the recording: Mac REWIND → the recording's Class field; iOS meeting detail, under the date.
- **Filter.**
  - Mac REWIND → Recordings: chips **All · BIO 101 · …**. `get_meetings(limit, className)` filters in SQL, ignoring case.
  - iOS Meetings: the same chips above the list. Search also matches the class.
- **Lecture notes.** A recording with a class gets a prompt variant, never different provider logic.
  - Mac: `classes::report_prompt` replaces the meeting prompt for auto-reports, Generate and Regenerate. It keeps the report's JSON shape: `key_topics` holds concepts, `decisions` holds definitions with the example in `context`, and `action_items` holds announcements and deadlines the instructor stated. It never writes action items for attendees. The notes are saved with `model_used = "lecture-notes"`, and the Notes view titles its sections Key concepts / Definitions and examples / Announcements and deadlines.
  - iOS: `MeetingAI.lectureNotesSystem` (Summary, Key concepts, Definitions, Examples, Announcements and deadlines). The context adds a `Class:` line.
- **One-time notice.** The first recording with a class shows a non-blocking notice, once ever: "Many schools require the instructor's permission to record a class, and some require classmates' consent. Check your school's policy."
  - It is stored as Mac setting `class_recording_notice_shown` and iOS `classRecordingNoticeShown`.
  - It is a reminder, not a legal attestation, and there is no checkbox gate.

## Purge

The class name is user-entered metadata on the meeting row, like the title:

- **Delete Meeting** removes it on both platforms: the Mac deletes the `meetings` row, and iOS `context.delete(meeting)`. Recent classes are derived from meetings, so a class disappears from the chips once no meeting has it.
- Planned length is stored only as minutes.
- Lecture notes are ordinary AI notes, covered by [REDACTION.md](REDACTION.md) step 5.
- Neither feature stores transcript or screen text anywhere new.
- Known gap (not new): Mac **Delete Meeting** doesn't purge the app's
  database backups (`backups/`). A backup taken before the delete still has
  the meeting row, and with it the class name, just as it still has the
  title and transcript. Only Delete and Strike of words or screens purge
  backups today.

## Code

| | Mac | iOS |
|---|---|---|
| Timer, plan, commands | `src-tauri/src/timed_recording.rs` | `ios/NoFriction/Capture/RecordingPlan.swift`, `RecordingSession.swift` |
| Classes, schema, prompt | `src-tauri/src/classes.rs`, `meeting_notes.rs` | `RecordingPlan.swift` (`ClassNames`), `AI/MeetingAI.swift` |
| Tray | `tray_builder.rs` | — |
| UI | `src/components/RecordPicker.tsx`, `TimedRecording.tsx`, `MeetingClass.tsx`; logic in `src/lib/recordPlan.ts` | `Views/TimedRecordingViews.swift`, `LiveView.swift`, `MeetingsView.swift`, `MeetingDetailView.swift` |
| Tests | `timed_recording::tests`, `classes::tests`, `schema_drift_tests.rs`, `src/lib/recordPlan.test.ts` | `NoFrictionTests/TimedRecordingTests.swift` |

The Apple Watch app is unchanged. A watch recording has no class until you set one on the iPhone.
