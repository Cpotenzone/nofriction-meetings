# noFriction for Apple Watch

Record a meeting, a class or anything else on the watch. Say what it is
(Meeting · Class · Personal), how long ("How long?" stops it by itself) and,
optionally, which notebook it belongs to; mark moments while it records;
choose the low-distraction **Discreet** display for lectures. The audio goes
to the iPhone, which imports it as an ordinary recording: transcribed on the
iPhone, matched to the calendar, with its type, notebook, planned length and
marked moments, AI notes, Review, Delete / Strike, export and People, the
same as a recording made on the phone.

The watch app is part of the iPhone app. It has no App Store record of its
own and makes no network requests. The Speech framework doesn't exist on
watchOS, so all transcription happens on the iPhone, on the device.

## How it fits together

```
Apple Watch                                   iPhone
───────────                                   ──────
Record ─▶ AVAudioRecorder                     PhoneWatchLink (WCSession, activated in App.init)
          AAC mono 16 kHz, 32 kbit/s            │ session(_:didReceive file:)
          one file per stretch between          │   moves each part into Documents/WatchInbox
          pauses ("parts")                      │   (+ its .json metadata) before returning
Stop ─▶ WatchRecordingStore (index.json)        ▼
                 │                            WatchImporter
WatchTransferQueue ─▶ WCSession.transferFile    1. import once every part is here: idempotent
   (one per part; queued by the system while       by recordingId; parts joined into
    the phone is away; background delivery)       Documents/Audio/watch-<id>.m4a, the meeting's
                 │                                 audio file; calendar match
                 │                              2. transcribe (queued, resumable): on-device,
session(_:didFinish:error:)                        in chunks; TranscriptFilter; segments with
   success ─▶ keep the file, wait for the ack      audio offsets + word timings
   error   ─▶ keep it, "will retry"            3. "Watch recording added" notification
session(_:didReceiveUserInfo:) ["ack": parts] ◀── transferUserInfo after each part is staged
   ─▶ delete those parts; all confirmed: "Delivered"
```

| Piece | File |
|---|---|
| Transfer contract: metadata keys, audio format, wall-clock mapping, markers, notebook list, shared notice text | `ios/Shared/WatchTransfer.swift` (in both apps) |
| Vocabulary: Meeting · Class · Personal, Notebook, marker labels by type | `ios/Shared/RecordingVocabulary.swift` (in both apps) |
| "How long?" choices and the wall-clock deadline (`TimeLimitPlan`) | `ios/Shared/TimeLimit.swift` (in both apps) |
| Watch app entry, model, Record flow, notification actions, background WatchConnectivity task | `ios/NoFrictionWatch/App/NoFrictionWatchApp.swift` |
| Recorder (AVAudioRecorder, interruptions, time limit, marks, haptics, warning notification) | `ios/NoFrictionWatch/Core/WatchRecorder.swift` |
| Recorder state machine (testable, no audio): pauses, deadline, +15 / No limit, markers | `ios/NoFrictionWatch/Core/RecorderStateMachine.swift` |
| Start options, Discreet setting, what the screen shows (`RecordingPresentation`), haptic patterns | `ios/NoFrictionWatch/Core/RecordingOptions.swift` |
| Recordings list + transfer queue | `ios/NoFrictionWatch/Core/WatchRecordingStore.swift` |
| Watch end of WatchConnectivity (files, acks, notebook list) | `ios/NoFrictionWatch/Core/WatchConnection.swift` |
| Watch UI (Record flow, recording screen, Discreet, Recordings, first-use notice) | `ios/NoFrictionWatch/Views/WatchRootView.swift` |
| "Start Recording" App Intent / App Shortcut | `ios/NoFrictionWatch/App/StartRecordingIntent.swift` |
| Debug demo states and simulator E2E sender | `ios/NoFrictionWatch/App/DemoMode.swift` |
| iPhone end of WatchConnectivity | `ios/NoFriction/Watch/PhoneWatchLink.swift` |
| Inbox (staged files) | `ios/NoFriction/Watch/WatchInbox.swift` |
| Import + transcription queue | `ios/NoFriction/Watch/WatchImporter.swift` |
| File transcription, chunking, line splitting | `ios/NoFriction/Watch/FileTranscription.swift` |
| Meeting fields (`source`, `sourceRecordingID`, `importState`, …) | `ios/NoFriction/Models/Models.swift` |

## Minimum watchOS: 10.0

watchOS 10 is the lowest version with the APIs the app uses without
fallbacks: the Observation framework (`@Observable`, `@Bindable`) and
`AVAudioApplication.requestRecordPermission()` (watchOS 10+), plus SwiftUI's
vertical-page `TabView` and `.backgroundTask(.watchConnectivity)`. It also
keeps Series 4 and 5, whose last release is watchOS 10, which still pair
with iPhones on iOS 18. Nothing needs watchOS 11.

## Recording

- **Format:** AAC, mono, 16 kHz, 32 kbit/s (about 14 MB per hour), written
  by `AVAudioRecorder` to `Application Support/Recordings/<recordingId>-p<N>.m4a`
  in the watch app's container. 16 kHz is the rate speech recognition works
  at, and the files stay small enough to transfer quickly.
- **Parts:** an MPEG-4 audio file can only be read once the recorder closes
  it, and watchOS may end a suspended app. So every Pause (and every
  interruption) closes the current file, and Resume continues in a new one.
  A paused recording is therefore always complete on disk. The iPhone joins
  the parts in order into one file; pause positions are the part boundaries.
- **Controls:** one large Record button opens the Record flow (below).
  While recording: the status ("Recording · Class · BIO 101"), a large
  **time left** (the elapsed time, pauses excluded, when there is no
  limit), a live input meter, **Mark** one tap away, then Pause/Resume and
  Stop. Haptics on start, mark, the time-limit warning, stop, pause, resume
  and interruption. A second page (Digital Crown or swipe) lists recent
  recordings with **Saved on watch / Sending to iPhone / Delivered** (and
  the type, notebook and number of marks until it is delivered).
- **First use:** the same recording-consent notice as the iPhone
  (`RecordingNotice.text`) before the first recording, Discreet or not.
- **App Intent:** "Start Recording" (Siri: "Start a recording in noFriction";
  Shortcuts; on Apple Watch Ultra, assign the shortcut to the Action button).
  It opens the app first, because watchOS only lets recording start in the
  foreground, and shows the notice if it hasn't been accepted. It starts
  with the remembered type, "How long?" and Discreet choice, and no notebook.

### Record flow: what it is, how long, which notebook

Tapping Record (after the one-time notice) opens three short lists; the
Digital Crown scrolls each one:

1. **What is it?** Meeting · Class · Personal (the shared vocabulary in
   `ios/Shared/RecordingVocabulary.swift`; the help line says "Personal
   covers everything else: conversations, appointments, talks, ideas."). A
   **Start** row at the top starts at once with the last type and length
   and no notebook. The **Discreet** switch is on this screen.
2. **How long?** 15 / 30 / 60 / 90 min / No limit.
3. **Notebook**: None, or one of the iPhone's recent notebooks. Skipped
   (None) while the watch has no list from the iPhone. There is no text
   entry on the watch.

The type, the length and Discreet are remembered on the watch
(`recordingKindDefault`, `recordingDefaultLength`, `discreetRecording` in
the watch app's UserDefaults; Meeting, No limit and off until chosen). The
notebook starts at None each time, as on the iPhone.

**Notebook list.** The iPhone sends the names of its most recent notebooks
(at most 8, cleaned) with `WCSession.updateApplicationContext`
(`["recentNotebooks": [names]]`): only the latest list is delivered, also
while the watch app isn't running, and it is sent again whenever the
iPhone's recent notebooks change, including when the last recording in a
notebook is deleted (the name then leaves the watch too). Nothing else of
the user's data goes to the watch. The watch reads the list from
`receivedApplicationContext`; it keeps no copy of its own.

### "How long?" on the watch

The watch enforces the limit itself with the iPhone's rules (the same
`TimeLimitPlan`, in `ios/Shared/TimeLimit.swift`):

- **Wall clock** from the start: pausing doesn't move the deadline.
- **Warning** 5 minutes before the end (2 minutes for a 15-minute plan): a
  haptic and a screen with **+15 min** and **No limit**. +15 min moves the
  deadline (from now if it already passed, 12 hours at most) and re-arms
  the warning; No limit removes it.
- **At the deadline** the recorder stops through the same path as Stop:
  the parts are closed, the recording is finished and queued for the
  iPhone, and the watch says "Stopped at its 60-minute limit."
- The timer ticks every second while recording or paused, and again when
  the app comes back to the foreground, so a paused app that watchOS
  suspended stops as soon as it runs again after the deadline. A recording
  ended by watchOS while paused is finished at the next launch, as before.
- **Warning notification.** watchOS may not play an app's haptics while it
  is in the background (the watch face showing, wrist down for a while), so
  the warning is also scheduled ahead as a local notification with
  **+15 min** / **No limit** actions (category `WATCH_TIME_LIMIT`), if
  notification permission is granted. The watch asks for it in context, at
  the first timed recording, never at launch. Its text is generic ("5
  minutes left in this recording"; no title or notebook). In the
  foreground the app suppresses the banner and shows its own warning. It
  is rescheduled on +15 min and removed on No limit and at stop.
- The planned minutes (after any +15 min) go to the iPhone, which stores
  them as `plannedMinutes`.

### Marks

While recording, **Mark** marks ★ Important with one tap and a confirming
haptic ("★ Marked Important" shows for a moment). Touch and hold Mark for
**? Question** or the third kind, labeled by type: **On the test** (Class),
**Follow up** (Meeting), **Remember** (Personal). The stored kinds are
`important` / `question` / `test` whatever the label. There are no notes on
the watch. A second tap of the same kind within 0.8 s is the same mark;
nothing is marked while paused; at most 500 marks per recording.

Each mark is stored in the recording's local metadata (`index.json`) as it
is made, so a crash keeps it, with its wall-clock time and its audio
offset (seconds of audio before it, pauses excluded). The contract's
`wallClock(atFileOffset:)` maps the offset back to the same wall-clock
time; the iPhone places the marker by its wall-clock time, the clock the
transcript lines use.

### Discreet

A low-distraction display for lectures and other places where a lit, red
recording screen would distract the room. It changes only how the watch
looks and feels while recording. It is not a way to record covertly: the
system microphone indicator still shows (the app doesn't, and can't, hide
it), the recording-consent notice still comes before the first recording,
and the Record flow says to tell people you're recording.

- **On/off:** the Discreet switch in the Record flow. Remembered; off by
  default. The App Intent uses the remembered setting.
- **Screen:** almost entirely black. No red UI, no meter, no big timer. The
  only sign is the noFriction logo (a monochrome template version of the
  app logo, `DiscreetLogo` in the watch asset catalog) fading slowly in and
  out between 12% and 35% opacity over 3.5 s, never brighter.
  - **Reduce Motion:** a static faded logo (22%), no pulse.
  - **Wrist down / Always On** (`isLuminanceReduced`): only the logo,
    dimmer (7%) and still.
  - **Paused:** a still logo and the word "Paused".
- **Time left:** small, dim grey text, shown for 3 seconds after a tap or a
  wrist raise (elapsed time with no limit).
- **Marking:** a tap anywhere marks ★, with a light tap and a brief,
  faint brighten of the logo (to 50% for under half a second). Touch and
  hold, or turn the Digital Crown, for **? Question** / the third kind.
- **Stopping** takes two deliberate steps: touch and hold (or turn the
  crown) to open the dim controls, then **Stop recording**. The controls
  also have Pause/Resume and, with a limit, +15 min / No limit.
- **Haptics:** light taps only: one for start, mark and stop, a gentle
  double tap for the time-limit warning (+15 / No limit then show as dim
  buttons, a tap away) and for an interruption.
- The pure rules (what is visible, at what opacity, in normal, Discreet,
  Discreet + wrist down, Reduce Motion, paused) are `RecordingPresentation`
  in `RecordingOptions.swift`, unit-tested.

### Recording limits (what watchOS actually allows)

| Situation | Behavior |
|---|---|
| Wrist down / screen off | **Keeps recording.** The watch app declares the `audio` background mode (`UIBackgroundModes`). A recording started in the foreground continues when the app goes to the background, and the app comes back when the wrist is raised. |
| User presses the Digital Crown or opens another app | Keeps recording in the background; watchOS shows an indicator on the watch face. |
| Phone call, Siri, another app takes the microphone | **The recording pauses** (interruption). watchOS doesn't let an app start or resume recording from the background, so the watch shows "Paused by a call or Siri. Tap Resume" and buzzes; nothing after that is recorded until the user taps Resume. The file is closed at the interruption, so everything before it is safe even if the user never comes back. |
| Paused (by the user or an interruption) and the wrist goes down | The app may be suspended, and watchOS may end it during a long pause. Nothing is lost: the paused recording's files are already closed. At the next launch it is saved and sent; Resume isn't possible after that (start a new recording). |
| App ended while actually recording (crash, low memory, force quit) | At the next launch, the parts that can be read are kept and sent. The part being written when the app ended can't be read (the recorder hadn't closed it) and is deleted, and the app says so if nothing could be kept. |
| Battery | Continuous recording uses battery. Not yet measured on hardware; see "Not verified". |

Extended runtime sessions (`WKExtendedRuntimeSession`) were considered and
not used: their types are self care, mindfulness, physical therapy and smart
alarm, Apple says to choose a type by the app's intended use, not by the
features it gives, and none of them is meeting recording. The `audio`
background mode is the documented mechanism for audio sessions that continue
in the background.

Sources:
[Enabling Background Sessions](https://developer.apple.com/documentation/watchkit/enabling-background-sessions)
("The Audio, Location updates, and Workout processing modes let your app run
the respective background sessions. Your app must start the session in the
foreground…");
[Apple engineer, forum 750432](https://developer.apple.com/forums/thread/750432)
("Recording cannot be resumed when the app is in the background on watchOS.
It must be a user-initiated event while the app is in the foreground.
(Recording can then continue once the app moves to the background.)");
[Apple engineer, forum 751866](https://developer.apple.com/forums/thread/751866)
("Audio recording must begin when the app is in the foreground.");
[Using extended runtime sessions](https://developer.apple.com/documentation/watchkit/using-extended-runtime-sessions).

## Transfer and privacy

- **Metadata** (property list, with each file): `recordingId` (UUID),
  `startedAt`, `endedAt`, `duration` (seconds of audio, whole recording),
  `appVersion`, `pauses` (`[[fileSeconds, pausedSeconds]]`), `part` and
  `parts` (this file's index and the number of files; 0 and 1 for a
  recording without pauses) and `v`. The phone ignores unknown keys and
  rejects invalid times or part numbers.
- **Optional keys** (added with recording types; `v` stays 1 because they
  are additive):

  | Key | Value |
  |---|---|
  | `kind` | `meeting` / `class` / `personal` |
  | `notebook` | the notebook name picked on the watch (one of the iPhone's recent notebooks) |
  | `plannedMinutes` | "How long?" in minutes when it ended (after any +15 min); absent = no limit |
  | `markers` | `[["id": uuid, "kind": "important" \| "question" \| "test", "at": Date, "offset": seconds]]` |

  A key is present only when set (a property list has no null). No
  transcript, title or audio content: times, ids, the type, the notebook
  name the user picked, the planned length and marker times.
- **Compatibility, both directions.** An iPhone app from before these keys
  imports a new watch's recording (it ignores unknown keys; the first-version
  keys keep their names and types; a test parses new metadata with a copy of
  the first-version parser). A new iPhone app imports a recording from an
  older watch app as a meeting with no notebook, limit or markers. A bad
  value in an optional key (an unknown type, a planned length outside 1 min
  … 12 h, a marker without a valid id or time) is dropped; the recording is
  never rejected for it. A marker kind from a newer build reads as ★
  Important. The ack flow and delete-only-after-ack are unchanged.
- **Markers with any part.** Every part of a recording carries the marker
  list. The iPhone merges the lists of all parts by marker id (first seen
  wins, time order), and takes the type, notebook and planned length from
  part 0, or the first part that has them.
- **Queued delivery:** `WCSession.transferFile` hands the file to the system,
  which delivers it when the iPhone is reachable, in the background. The
  watch re-queues anything not yet delivered at launch, when the iPhone comes
  back in range, and when the app returns to the foreground.
- **Deleted once the iPhone confirms it:** a successful
  `session(_:didFinish:error:)` only means the system has the file, so the
  watch keeps it. After the iPhone app has stored a part in its inbox it
  sends an acknowledgment (`transferUserInfo(["ack": ["<id>#<part>"]])`,
  queued and delivered by the system like the file); only then does the
  watch delete that part. When every part is confirmed the row shows
  **Delivered** (it keeps no audio; rows go after a week, or beyond 20). On
  an error the file stays and that part is sent again. A part handed off
  but not confirmed within an hour is sent again (the iPhone ignores
  duplicates). The iPhone never acknowledges what it couldn't store (bad
  metadata, full disk), so the watch keeps it. In the simulator this is
  visible: transfers "finish" but the iPhone app never receives them, and
  the watch correctly keeps the files as "Sending to iPhone".
- **Deleting on the watch:** a recording not yet sent can be deleted from
  the watch's list; one partly confirmed by the iPhone can't (the rest is on
  its way).
- **Idempotent on the phone:** the import is keyed by `recordingId` and
  waits until every part has arrived (in any order). A part delivered twice
  replaces the earlier copy. The iPhone keeps a list of imported recording
  ids (ids only, in UserDefaults), so a part arriving after its recording
  was imported is discarded, and a recording whose meeting the user deleted
  is not brought back by a late re-delivery. A recording still missing parts
  30 days after its first part arrived is dropped from the inbox.
- **Watch index safety:** the watch's list (`index.json`) is read
  tolerantly (missing or unknown fields take defaults). If it can't be read
  at all, it is moved aside and the rows are rebuilt from the audio files
  (start time from the file's creation date, declared in the watch privacy
  manifest); audio files are never deleted because they are missing from
  the list.
- **Staging:** WatchConnectivity deletes a received file when its delegate
  method returns, so the phone moves it into `Documents/WatchInbox` (with a
  `.json` of the metadata) inside that method, then imports it. Staging and
  listing share a lock so a half-staged recording is never cleared.
- **Nothing else crosses:** the watch receives only acknowledgments (part
  ids) and the recent notebook names (application context) from the phone,
  never audio, transcripts, titles or settings. The watch app has no network
  code and no Speech APIs (the release policy check enforces this) and no
  keys. Notebook names are never logged.
- **What the watch keeps of a recording:** while a recording is on the
  watch, its row in `index.json` holds the type, the notebook name, the
  planned length and the marker times (so a crash keeps them). Once the
  iPhone confirms every part, the row drops the notebook name and the
  markers along with the audio; a delivered row keeps only times and the
  type until it is pruned.
- **Data protection:** the watch index and phone inbox are written with
  `completeFileProtectionUntilFirstUserAuthentication`.

## iPhone import

1. **Meeting:** once every part is staged, the meeting is saved, then the
   parts are joined in order into one AAC file off the main thread
   (`AudioChunks.join`; a single part is just moved). If the app dies in
   between, the next pass finds the meeting without its audio and finishes.
   `startedAt`/`endedAt` from the watch, `source = "watch"`,
   `sourceRecordingID`, `audioFileName = watch-<id>.m4a` in `Storage.audio`,
   `importState = "pending"`. The new `Meeting` fields are optional, so stores
   from earlier builds migrate automatically (no schema version needed).
   **Type, notebook, plan and marks** (`WatchImporter.applyWatchFields`):
   `recordingKind` from `kind` (nil, a meeting, from an older watch),
   `courseName` (the notebook) from `notebook`, spelled like an existing
   notebook ignoring case, `plannedMinutes`, and one `MomentMarker` per
   watch marker at its wall-clock time (kept inside the recording), with
   the watch's marker id and no note. Idempotent: a field is only filled
   when empty and a marker only added if no marker has its id, so a
   re-delivery, or a pass that finishes an import cut short, never doubles
   a marker; a duplicate delivery of an imported recording changes nothing
   (the user may have edited the notebook or deleted a marker since). With
   a notebook and no calendar event, the title is the notebook and the date
   ("BIO 101 — Oct 7"), like a live recording; otherwise the type and the
   date ("Meeting — Oct 7", "Class — Oct 7", "Personal — Oct 7"). Both
   count as untitled, so a calendar match found later names it (the Mac's
   rule).
2. **Calendar:** the same `CalendarMatching.bestEvent` rules as a live
   recording, over the watch recording's start and end; `MeetingLinker`
   fills the title, invite fields and attendees. Unmatched recordings keep the
   default type-and-date title, so the existing backfill can still name them
   when calendar access is granted later.
3. **Transcription (on-device only):** iOS 26+ uses `SpeechAnalyzer` +
   `SpeechTranscriber` on the file (`analyzeSequence(from:)`), iOS 18–25 uses
   `SFSpeechURLRecognitionRequest` with `requiresOnDeviceRecognition = true`.
   The file is cut into chunks (5 minutes for SpeechAnalyzer, 55 seconds for
   SFSpeechRecognizer), each cut moved to the quietest 100 ms in the last
   10 seconds before the limit so words are rarely split. Attendee names and
   companies are passed as contextual strings.
4. **Lines:** results are split into transcript lines at pauses and sentence
   ends, and every line passes through `TranscriptFilter` (the "Bye-bye.
   Bye-bye." guard), with "near silence" taken from the file's own levels.
   Each `Segment` gets `audioOffset` (seconds into the file), word timings in
   file seconds and a wall-clock `start` (the watch's pauses added back).
5. **Resumable:** after every chunk the lines and `importProgress` are saved
   together. Meetings still `pending` or `transcribing` are picked up at
   launch, when the app comes to the foreground and when a live recording on
   the phone stops (one recognizer at a time: watch imports wait while the
   phone records, and a running import stops after its current chunk when
   the phone starts recording). A run cut short (background time over, app closed) goes
   back to `pending`; a failure in the foreground shows the error with
   **Retry**. Failed imports get one automatic retry per launch.
6. **Background:** the system may launch the app in the background to
   deliver a file. Import always runs; transcription runs while a background
   task lasts. Speech permission is never requested from the background; a
   recording waiting for it says "Waiting to transcribe on this iPhone…" and
   continues when the app is opened.
7. **Notification:** "Watch recording added — <title> · <length> —
   transcribed on this iPhone", only if the user already allowed
   notifications; the import never asks for permission.

**After import** the recording is ordinary: AI notes in its type's style
(meeting notes, lecture notes for a Class, personal notes), the follow-up
email for a meeting, and Review use its transcript and its marks;
**Delete / Strike silence the phone's copy** of the audio
(the imported file is the meeting's audio file, with word timings, so exactly
the selected words plus 150 ms are zeroed; see `docs/REDACTION.md`); Delete
Recording removes the file, its markers (cascade) and anything of that
recording still in the inbox, and the import log keeps a late re-delivery
from bringing it back; export and People work unchanged. At launch the app removes temporary files a killed run
may have left (`.joining-*` / `.silencing-*` in `Audio/`, `nf-chunk-*` in
`tmp/`).

**UI:** an Apple Watch mark on the recording in Recordings and "Recorded on Apple
Watch" in its detail; "Transcribing…" / "Not transcribed" in the list and a
progress card (or Retry) in the transcript section; Settings → **Apple Watch**
shows paired / app installed / arriving from the watch (a paused recording
with parts still on the way) / waiting to transcribe (hidden where
WatchConnectivity isn't supported, e.g. iPad). The iPhone can't see the
watch's own queue; the watch's Recordings page shows what it still has to send.

## Testing

```bash
cd ios && xcodegen generate
# iPhone unit tests (include the watch import tests)
xcodebuild test -project NoFriction.xcodeproj -scheme NoFriction \
  -destination 'platform=iOS Simulator,name=NF iPhone 17 Pro' -only-testing:NoFrictionTests
# Watch unit tests
xcodebuild test -project NoFriction.xcodeproj -scheme NoFrictionWatch \
  -destination 'platform=watchOS Simulator,name=NF Apple Watch Series 11 (46mm)'
```

- `NoFrictionTests/WatchImportTests.swift`: metadata encode/decode and
  validation (incl. part numbers and older JSON), wall-clock mapping,
  idempotent import (same id twice → one meeting), a paused recording's
  parts arriving out of order and joined in order, meeting creation + calendar match with injected events, chunked
  transcription with a scripted fake transcriber (offsets, word timings,
  pauses, the filter), resume from a checkpoint after a cut-short run and a
  killed run, waiting for the foreground, failure + Retry, waiting while the
  phone records, deletion mid-run, Strike silencing a watch-format file, the
  silencer's bit-rate fallback, line splitting and chunk planning. Type,
  notebook, plan and markers: imported (notebook spelled like an existing
  one, markers at their wall-clock times with the watch's ids, markers from
  any part merged), idempotent (re-delivery, a pass finishing a cut-short
  import, applying twice), user edits kept on a late re-delivery, markers
  clamped into the recording and deleted with it, metadata from an older
  watch imported as a plain meeting, the new keys' plist/JSON round trip,
  and the notebook list (names only, capped).
- `NoFrictionWatchTests`: recorder state machine (transitions, pauses,
  interruptions, metadata), transfer queue (send once, every part of a
  paused recording, delete each part on delivery, keep and retry only the
  failed part, nothing sent while unavailable or while recording, resend
  after relaunch), crash recovery keeping readable parts, pruning, plist
  round trip. `RecordingModeTests.swift`: the time limit in the state
  machine (warning 5 or 2 minutes ahead, stop at the deadline while
  recording or paused, +15 min before and after the deadline, No limit,
  idle), markers (wall clock + offset mapped back by the contract, none
  while paused, double taps, the cap, labels by type), metadata with the new
  keys in both directions (round trip, a copy of the first-version parser
  reading new metadata, old metadata without the keys, bad values dropped
  but the recording kept), the notebook context, the store (every part
  carries the markers, delivery drops the notebook name and markers, a
  crash keeps them, an index from before types), Discreet (setting off by
  default and persisted, start options remembered without the notebook,
  what is visible and at what opacity in normal, Discreet, Discreet + wrist
  down, Reduce Motion, paused and mark flash, light-tap haptics).
- **Simulators:** a watch simulator paired with an iPhone simulator
  (`xcrun simctl pair <watch> <phone>`; a dependent watch app doesn't launch
  on an unpaired watch simulator). Debug launch arguments on the watch:
  `-NFWatchDemo idle|recording|paused|list` (sample states, no microphone),
  `-NFWatchDemo start|class|warning|discreet` (the Record flow, a Class
  recording in a notebook with time left and marks, its 5-minute warning,
  the Discreet screen), `-NFWatchDemo length|notebook|controls` (the later
  Record flow steps and the Discreet controls, for layout checks) and
  `-NFWatchSendTestRecording` (a 3-second synthetic tone sent through the real
  store and WatchConnectivity path), `-NFWatchAutoRecord` (the real
  microphone recorder: 2 s, pause, 2 s, stop; grant the simulator microphone
  first with `xcrun simctl privacy <watch> grant microphone com.nofriction.meetings.watchkitapp`).
- **Simulator limit:** in the simulator, `transferFile` from the watch
  completes on the watch (the system reports success) but the iPhone
  simulator never calls `session(_:didReceive:)`, so no acknowledgment comes
  back and the watch keeps the files ("Sending to iPhone"). This is a known simulator limitation
  ([forum 128205](https://developer.apple.com/forums/thread/128205)). The
  phone side (staging, import, transcription) is covered by unit tests; the
  delivery itself needs real devices.

### On a real iPhone + Apple Watch (before release)

1. Install from Xcode or TestFlight; the watch app installs from the iPhone
   (Watch app → Available Apps if automatic install is off).
2. Record 2–3 minutes on the watch with the wrist down part of the time;
   confirm the timer kept counting and the audio has no gap.
3. Call the phone (or invoke Siri) mid-recording: it pauses with the message
   and haptic; Resume continues; the transcript times line up after the pause.
4. Stop with the iPhone in airplane mode: the row stays "Sending to iPhone";
   turn it off: it becomes Delivered and the watch copy is gone.
5. On the iPhone: the meeting appears with the watch mark, calendar title and
   attendees; transcript appears (check iOS 26 SpeechAnalyzer and an iOS 18
   device); the notification arrives if allowed.
6. Close the iPhone app during a long transcription; reopen: it continues.
7. Strike a word and play the audio: silence there, nothing else changed.
8. A 60-minute recording: transfer time, transcription time, battery use.
9. Notebooks: add a notebook to a recording on the iPhone; open the Record
   flow on the watch: it is listed. Delete the iPhone's last recording in
   it: it leaves the watch's list.
10. A 15-minute Class recording in a notebook, wrist down most of the time:
    the warning arrives 2 minutes before (haptic in the app, or the
    notification with +15 min / No limit when the watch face shows); +15 min
    from the notification moves the end; it stops by itself at the end and
    arrives on the iPhone as a Class in that notebook with `plannedMinutes`.
11. Mark ★ with a tap and ? / ✎ with a long press during a paused
    recording's second part: on the iPhone the markers sit on the right
    transcript lines, labeled On the test (Class) / Follow up (Meeting) /
    Remember (Personal).
12. Discreet in a dark room: only the faint logo; Always On shows it dimmer
    and still; a tap marks with a light tap; the crown or a long press opens
    the controls; Stop takes two steps; the microphone indicator still
    shows; Reduce Motion stops the pulse.

## Release

- **Build:** XcodeGen embeds the watch app with a copy phase to
  `$(CONTENTS_FOLDER_PATH)/Watch` (Xcode's "Embed Watch Content"), so the
  iPhone app contains `Watch/NoFrictionWatch.app`. The watch target sets
  `SKIP_INSTALL = YES`, so the archive has one top-level app. Version and
  build come from the same `MARKETING_VERSION` / `CURRENT_PROJECT_VERSION`
  as the iPhone app (App Store requires them to match).
- **`scripts/release-ios.sh`:** checks the watch bundle id and companion id
  in `project.yml`; after archiving, checks the watch app is embedded, signed,
  points at `com.nofriction.meetings` and has the same version/build; runs
  the credential/retired-host scan with
  `--require-embedded Watch/NoFrictionWatch.app=com.nofriction.meetings.watchkitapp`
  on the archive and the IPA (a missing or misidentified watch app makes the
  scan incomplete; every file in it is scanned).
- **Signing:** automatic signing (default) creates the watch App Store
  profile itself, given an Xcode account or `ASC_KEY_*`. The manual path
  needs a second installed profile: set `NF_WATCH_PROFILE_UUID` (App Store
  profile for `com.nofriction.meetings.watchkitapp`) alongside
  `NF_IOS_PROFILE_UUID` and `NF_IOS_SIGNING_IDENTITY`; the script maps each
  profile to its target and adds both to the export options.
- **Screenshots:** `scripts/ios-screenshots.sh watch` builds the watch app
  for the "NF Apple Watch Series 11 (46mm)" simulator, captures five demo
  states (recording, Record, Recordings, a Class recording with time left
  and marks, Discreet), removes the alpha channel and checks 416×496 (the
  Series 10/11 size; Ultra 3 would be 422×514). Output:
  `ios/AppStore/screenshots/watch-46mm/`. The committed set was captured
  from the same demo states on a Series 11 (46mm) simulator.
- **Next upload is a new build.** Build 3 was exported without the watch app
  and never uploaded; the release script will produce build 4 with it.

## Apple Developer portal and App Store Connect (owner)

1. **App ID:** Certificates, IDs & Profiles → Identifiers → **+** → App IDs →
   App → Explicit bundle ID **`com.nofriction.meetings.watchkitapp`**
   (description e.g. "noFriction Watch"). No capabilities to enable: background
   audio is an Info.plist key and WatchConnectivity needs no entitlement.
2. **Profile:** with automatic signing nothing to do (Xcode creates it on the
   first archive). For the manual path: Profiles → **+** → Distribution →
   **App Store Connect** → App ID `com.nofriction.meetings.watchkitapp` → the
   Apple Distribution certificate → download, install, and pass its UUID as
   `NF_WATCH_PROFILE_UUID`.
3. **No separate app record.** The watch app ships inside the iOS app (record
   `6818838861`, iOS 1.0.0). Upload build 4 and attach it to the iOS version.
4. **Apple Watch screenshots** are required once the build has a watch app:
   on the iOS 1.0.0 version page, Apple Watch section, upload
   `ios/AppStore/screenshots/watch-46mm/01-recording.png`, `02-record.png`,
   `03-recordings.png`, `04-class.png`, `05-discreet.png` (416×496).
5. **Review notes:** add one paragraph: "The Apple Watch app records a
   meeting, a class or anything else and sends the audio to the iPhone app,
   which transcribes it on the device. Install from the Watch app on the
   paired iPhone; tap Record, choose what it is and how long, then Stop; the
   recording appears in the iPhone app's Recordings tab. Discreet dims the
   watch screen while recording; the system microphone indicator still
   shows."
6. **Listing / privacy:** mention Apple Watch in the description where
   appropriate. The App Privacy answers don't change: audio moves only between
   the user's own watch and iPhone; nothing is collected by the developer.
   Export compliance: `ITSAppUsesNonExemptEncryption = false` in both apps.

## Not verified yet

- Real devices: background recording with the wrist down, interruptions,
  WatchConnectivity delivery to the iPhone app (the simulator can't deliver
  files), background launch of the iPhone app on delivery, on-device
  transcription of an imported file (the Speech framework doesn't run in the
  Simulator), battery and transfer time for long recordings.
- The acknowledgment round trip (`transferUserInfo` from the iPhone app,
  `didReceiveUserInfo` on the watch) needs real devices for the same reason.
- A crash while actually recording loses the part being written. Rotating
  to a new file every few minutes would cap that, but watchOS doesn't let an
  app start recording from the background, so a rotation with the wrist down
  would end the recording; it isn't done.
- Signed archive/upload with the watch app (needs the portal steps above).
- Student mode on hardware: whether watchOS plays the app's haptics (the
  time-limit warning, marks) while the app is in the background with the
  wrist down (the warning notification is the fallback), the warning
  notification and its +15 min / No limit actions, the Discreet display in
  Always On, the Digital Crown opening the Discreet controls, and the
  application context (recent notebooks) reaching the watch. The pure logic
  is unit-tested and the screens were checked in the simulator.
