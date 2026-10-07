# Recording type, timed recording and notebooks (Mac + iOS)

The Record sheet asks three things, with the same words on every platform:

- **What is it?** A recording's type: **Meeting · Class · Personal**. It sets
  how notes are written, what the third mark is called and what the review
  guide is called. Nothing else depends on it.
- **How long?** The recording stops by itself at the end.
- **Notebook** (optional). A grouping for recordings of any type, such as
  "Acme project", "BIO 101" or "Health".

noFriction is for meetings, classes and everyday life alike; the type only
changes labels and prompts. User-facing copy is in
[USER_GUIDE.md](USER_GUIDE.md#what-is-it-how-long-and-notebook).

## Shared vocabulary

| | |
|---|---|
| Picker title | "What is it?" (segmented), above "How long?", then Notebook |
| Picker labels | **Meeting · Class · Personal** |
| Stored values | `meeting` / `class` / `personal` |
| Help text | "Personal covers everything else: conversations, appointments, talks, ideas." |
| Default | Meeting. The last type picked in the sheet is remembered |
| Notebook field label / filter title | **Notebook** / **Notebooks** |
| Notebook placeholder | Meeting "e.g. Acme project", Class "e.g. BIO 101", Personal "e.g. Health" |
| Notes | Meeting: meeting notes. Class: lecture notes. Personal: summary, key points, to-dos and reminders |
| Marks | ★ Important · ? Question · ✎ third mark: Class **On the test**, Meeting **Follow up**, Personal **Remember** (stored as `test` for all three) |
| Review | Tab **REVIEW**. The guide is the **Study guide** for a Class and the **Review guide** otherwise |
| Untitled recording | Notebook + date ("BIO 101 — Oct 7"), else type + date ("Class — Oct 7", "Personal — Oct 7") |
| School-policy notice | Once, on the first **Class**-type recording |

## "What is it?"

| | Mac |
|---|---|
| Storage | `meetings.recording_kind TEXT`, nullable, added by `recording_kind::ensure_schema` through `database::ensure_columns`. NULL (and anything unreadable) means meeting |
| Backfill | Once, when the column is added, in the same transaction: rows with a non-blank `class_name` become `'class'`; the others stay NULL. A notebook set later never makes a recording a class, even when the migrations run again at the next launch (`schema_drift_tests.rs`) |
| Remembered | Setting `recording_default_kind` ("meeting" until the sheet picks one) |
| Keys | **M / C / P** pick the type, but only when focus isn't in the Notebook field (so "Acme" can be typed) |
| Change later | REWIND → the recording's **Type** menu (`set_meeting_recording_kind`). New notes and guides use the new type; saved notes keep the layout of the prompt that wrote them until regenerated |
| Library | Class and Personal recordings show a small type tag; meetings (the default) show none |

Starts that skip the sheet (⌘N, the tray, the command palette, the capture
mode items) send no type, and the backend uses the remembered one. The tray's
plain item shows it: **Start Recording (Meeting, 60 min)**. **Start
Recording For ▸ 15/30/60/90/No Limit** records the remembered type for the
chosen length.

## "How long?"

| | |
|---|---|
| Choices | 15, 30, 60, 90 min, **No limit (∞)** |
| Preselected | The last choice: Mac setting `recording_default_duration`, iOS `UserDefaults` `recordingDefaultLength` ("15" … "90", "none") |
| Never chosen | No limit, so a start that skips the sheet never cuts a recording short |
| Mac keys | 1–5 pick (not while typing in Notebook), Enter starts, Esc cancels |

### Which starts show the sheet

Only a click or tap on a Record button shows it. Every other start uses the remembered type and length:

| Mac start path | Sheet? |
|---|---|
| START CAPTURE (navbar), LIVE "Start recording", Zen START | Yes |
| ⌘N (File → New Recording). The Mac app has no global record hotkey; this is the shortcut | No: remembered |
| Tray **Start Recording (Meeting, 60 min)** | No: remembered |
| Tray **Start Recording For ▸ 15/30/60/90/No Limit** | No: that length (and remembers it), remembered type |
| Tray Capture Mode ▸ Ambient / Meeting | No: remembered |
| Command palette "Start Recording" | No: remembered |
| "Start New Segment" (75-minute prompt) | No: keeps the type, the notebook and the time that was left |
| Meeting auto-detect | Doesn't exist (`auto_start_recording` is never read). Anything added later goes through `start_recording` and gets the remembered type and length |

On iOS the Record button shows the sheet (after the one-time recording notice the first time). The `-NFAutoRecord` test start uses the remembered length.

The backend applies the rule, so no path can bypass it. On the Mac, `start_recording(plan?)` resolves a missing `plan.duration` to the remembered length and a missing `plan.recordingKind` to the remembered type, and **every** start arms a plan. On iOS, `RecordingSession.start(…)` does the same with a nil limit.

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

## Notebooks

- **Storage.** The notebook keeps the column the classes feature added; there is no data migration.
  - Mac: `meetings.class_name TEXT` (the notebook) and `meetings.planned_minutes INTEGER`, nullable, added by `notebooks::ensure_schema` through `database::ensure_columns`. `CREATE TABLE IF NOT EXISTS` never adds columns. `schema_drift_tests.rs` migrates the old schema.
  - iOS: `Meeting.courseName` (the notebook) and `Meeting.plannedMinutes`, optional, so stores from older builds migrate without a schema version. The attribute isn't called `className` because Core Data resolves that key to NSObject's `className`.
- **Names.** Trimmed, whitespace collapsed, ≤ 80 characters. A name that matches an existing notebook ignoring case uses the existing spelling ("bio 101" joins "BIO 101").
- **Recent notebooks.** Derived from the recordings that have one, most recent first. No separate list is stored.
- **Set it** in the Record sheet. Edit or clear it later on the recording: Mac REWIND → the recording's Notebook field (next to Type).
- **Filter.** Mac REWIND → Recordings: **Notebooks** chips **All · BIO 101 · …**. `get_meetings(limit, notebook)` filters in SQL, ignoring case.
- **A notebook doesn't change how notes are written.** Only the type does.

## Notes by type

The type picks a prompt variant, never different provider logic. All three
keep the meeting report's JSON shape (`summary`, `key_topics`, `decisions`,
`action_items`, `participants`), so storage and parsing are the same; the
Notes view relabels by `meeting_notes.model_used`.

| Type | Prompt (Mac) | `model_used` | Notes view |
|---|---|---|---|
| Meeting | The user's `meeting_report` prompt (PROMPTS) or the default | `auto-report` / `default` | Summary · Key topics · Decisions · Action items, and **Follow-up email** |
| Class | `recording_kind::lecture_notes_prompt`: `key_topics` holds concepts, `decisions` definitions with the example in `context`, `action_items` the announcements and deadlines the instructor stated. Never action items for attendees | `lecture-notes` | Lecture summary · Key concepts · Definitions and examples · Announcements and deadlines |
| Personal | `recording_kind::personal_notes_prompt`: `key_topics` holds key points, `action_items` the to-dos and reminders that were said. `decisions` and `participants` stay empty: no minutes, no attendees, no owners | `personal-notes` | Summary · Key points · To-dos and reminders |

The lecture prompt is chosen by **type == Class**, not by a notebook being
set. The notebook, when there is one, names the class or notebook in the
prompt. The follow-up email is offered for meetings only.

## Class notice

The first **Class**-type recording shows a non-blocking notice, once ever:
"Many schools require the instructor's permission to record a class, and some
require classmates' consent. Check your school's policy."

- Mac: `start_recording` checks and sets the setting `class_recording_notice_shown` and emits `class-recording-notice`, so it shows for any start path (the sheet, ⌘N or the tray with Class remembered). iOS: `classRecordingNoticeShown`.
- A notebook alone never shows it.
- It is a reminder, not a legal attestation, and there is no checkbox gate.

## Untitled recordings

With no calendar match, the title is the notebook plus the date when there
is a notebook ("BIO 101 — Oct 7"), else the type plus the date ("Meeting —
Oct 7", "Class — Oct 7", "Personal — Oct 7"). A calendar match replaces it
with the event's title, at the start or later from People → link to calendar
(`recording_kind::is_untitled_title` also recognises the "Meeting
2026-09-24 09:01" titles of earlier builds).

## Purge

The type and the notebook are user-entered metadata on the recording row, like the title:

- **Delete** removes them on both platforms: the Mac deletes the `meetings` row, and iOS `context.delete(meeting)`. Recent notebooks are derived from recordings, so a notebook disappears from the chips once no recording has it.
- Planned length is stored only as minutes.
- Notes of every type are ordinary AI notes, covered by [REDACTION.md](REDACTION.md) step 5.
- None of these features stores transcript or screen text anywhere new.
- Known gap (not new): Mac **Delete** doesn't purge the app's
  database backups (`backups/`). A backup taken before the delete still has
  the row, and with it the notebook and type, just as it still has the
  title and transcript. Only Delete and Strike of words or screens purge
  backups today.

## Code

| | Mac | iOS |
|---|---|---|
| Type: schema, backfill, prompts, labels, titles | `src-tauri/src/recording_kind.rs`; `src/lib/recordingKind.ts` | `ios/NoFriction/…` |
| Timer, plan, commands | `src-tauri/src/timed_recording.rs` | `ios/NoFriction/Capture/RecordingPlan.swift`, `RecordingSession.swift` |
| Notebooks | `src-tauri/src/notebooks.rs` | `RecordingPlan.swift`, `AI/MeetingAI.swift` |
| Notes | `meeting_notes.rs` (calls `recording_kind::report_prompt`), `src/components/MeetingNotesPanel.tsx` | `AI/MeetingAI.swift` |
| Tray | `tray_builder.rs` | — |
| UI | `src/components/RecordPicker.tsx`, `TimedRecording.tsx`, `Notebook.tsx`; logic in `src/lib/recordPlan.ts`, `src/lib/recordingKind.ts`; `src/hooks/useRecordingKind.ts` | `Views/TimedRecordingViews.swift`, `LiveView.swift`, `MeetingsView.swift`, `MeetingDetailView.swift` |
| Tests | `recording_kind::tests`, `timed_recording::tests`, `notebooks::tests`, `schema_drift_tests.rs`, `src/lib/recordPlan.test.ts`, `src/lib/recordingKind.test.ts` | `NoFrictionTests/TimedRecordingTests.swift` |

The Apple Watch app is unchanged. A watch recording has no notebook until you set one on the iPhone.
