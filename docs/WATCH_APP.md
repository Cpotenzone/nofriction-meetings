# noFriction for Apple Watch

Record a meeting on the watch. The audio goes to the iPhone, which imports
it as an ordinary meeting: transcribed on the iPhone, matched to the
calendar, with AI notes, follow-up email, Delete / Strike, export and People,
the same as a recording made on the phone.

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
   success ─▶ delete the watch copy, "Delivered"   audio offsets + word timings
   error   ─▶ keep it, "will retry"            3. "Watch recording added" notification
```

| Piece | File |
|---|---|
| Transfer contract: metadata keys, audio format, wall-clock mapping, shared notice text | `ios/Shared/WatchTransfer.swift` (in both apps) |
| Watch app entry, model, background WatchConnectivity task | `ios/NoFrictionWatch/App/NoFrictionWatchApp.swift` |
| Recorder (AVAudioRecorder, interruptions, haptics) | `ios/NoFrictionWatch/Core/WatchRecorder.swift` |
| Recorder state machine (testable, no audio) | `ios/NoFrictionWatch/Core/RecorderStateMachine.swift` |
| Recordings list + transfer queue | `ios/NoFrictionWatch/Core/WatchRecordingStore.swift` |
| Watch end of WatchConnectivity | `ios/NoFrictionWatch/Core/WatchConnection.swift` |
| Watch UI (Record, Recordings, first-use notice) | `ios/NoFrictionWatch/Views/WatchRootView.swift` |
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
- **Controls:** one large Record button, the elapsed time (pauses excluded),
  a live input meter, Pause/Resume and Stop. Haptics on start, stop, pause,
  resume and interruption. A second page (Digital Crown or swipe) lists
  recent recordings with **Saved on watch / Sending to iPhone / Delivered**.
- **First use:** the same recording-consent notice as the iPhone
  (`RecordingNotice.text`) before the first recording.
- **App Intent:** "Start Recording" (Siri: "Start a recording in noFriction";
  Shortcuts; on Apple Watch Ultra, assign the shortcut to the Action button).
  It opens the app first, because watchOS only lets recording start in the
  foreground, and shows the notice if it hasn't been accepted.

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
  recording without pauses) and `v`. Times and ids only: no transcript,
  title or other content. The phone ignores unknown keys and rejects invalid
  times or part numbers.
- **Queued delivery:** `WCSession.transferFile` hands the file to the system,
  which delivers it when the iPhone is reachable, in the background. The
  watch re-queues anything not yet delivered at launch, when the iPhone comes
  back in range, and when the app returns to the foreground.
- **Deleted after delivery:** on `session(_:didFinish:error:)` without an
  error the watch deletes that part's file; when every part is delivered the
  row shows **Delivered** (it keeps no audio; rows go after a week, or beyond
  20). On an error the file stays and only that part is sent again. A
  recording not sent yet can be deleted from the watch's list.
- **Idempotent on the phone:** the import is keyed by `recordingId` and
  waits until every part has arrived (in any order). A part delivered twice
  replaces the earlier copy; a recording delivered again after its import
  is discarded.
- **Staging:** WatchConnectivity deletes a received file when its delegate
  method returns, so the phone moves it into `Documents/WatchInbox` (with a
  `.json` of the metadata) inside that method, then imports it. Staging and
  listing share a lock so a half-staged recording is never cleared.
- **Nothing else crosses:** the watch never receives audio, transcripts or
  settings from the phone. The watch app has no network code (the release
  policy check enforces this) and no keys.
- **Data protection:** the watch index and phone inbox are written with
  `completeFileProtectionUntilFirstUserAuthentication`.

## iPhone import

1. **Meeting:** once every part is staged, the parts are joined in order
   into one AAC file (`AudioChunks.join`; a single part is just moved).
   `startedAt`/`endedAt` from the watch, `source = "watch"`,
   `sourceRecordingID`, `audioFileName = watch-<id>.m4a` in `Storage.audio`,
   `importState = "pending"`. The new `Meeting` fields are optional, so stores
   from earlier builds migrate automatically (no schema version needed).
2. **Calendar:** the same `CalendarMatching.bestEvent` rules as a live
   recording, over the watch recording's start and end; `MeetingLinker`
   fills the title, invite fields and attendees. Unmatched recordings keep the
   default "Meeting · …" title, so the existing backfill can still name them
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
   phone records). A run cut short (background time over, app closed) goes
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

**After import** the meeting is ordinary: AI notes and the follow-up email
use its transcript; **Delete / Strike silence the phone's copy** of the audio
(the imported file is the meeting's audio file, with word timings, so exactly
the selected words plus 150 ms are zeroed; see `docs/REDACTION.md`); Delete
Meeting removes the file; export and People work unchanged.

**UI:** an Apple Watch mark on the meeting in Meetings and "Recorded on Apple
Watch" in its detail; "Transcribing…" / "Not transcribed" in the list and a
progress card (or Retry) in the transcript section; Settings → **Apple Watch**
shows paired / app installed / waiting to transcribe (hidden where
WatchConnectivity isn't supported, e.g. iPad).

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
  silencer's bit-rate fallback, line splitting and chunk planning.
- `NoFrictionWatchTests`: recorder state machine (transitions, pauses,
  interruptions, metadata), transfer queue (send once, every part of a
  paused recording, delete each part on delivery, keep and retry only the
  failed part, nothing sent while unavailable or while recording, resend
  after relaunch), crash recovery keeping readable parts, pruning, plist
  round trip.
- **Simulators:** a watch simulator paired with an iPhone simulator
  (`xcrun simctl pair <watch> <phone>`). Debug launch arguments on the watch:
  `-NFWatchDemo idle|recording|paused|list` (sample states, no microphone) and
  `-NFWatchSendTestRecording` (a 3-second synthetic tone sent through the real
  store and WatchConnectivity path), `-NFWatchAutoRecord` (the real
  microphone recorder: 2 s, pause, 2 s, stop; grant the simulator microphone
  first with `xcrun simctl privacy <watch> grant microphone com.nofriction.meetings.watchkitapp`).
- **Simulator limit:** in the simulator, `transferFile` from the watch
  completes on the watch (the system reports success, the watch deletes its
  copy and shows Delivered) but the iPhone simulator never calls
  `session(_:didReceive:)`. This is a known simulator limitation
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
  for the "NF Apple Watch Series 11 (46mm)" simulator, captures three demo
  states, removes the alpha channel and checks 416×496 (the Series 10/11
  size; Ultra 3 would be 422×514). Output: `ios/AppStore/screenshots/watch-46mm/`.
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
   `03-recordings.png` (416×496).
5. **Review notes:** add one paragraph: "The Apple Watch app records a
   meeting and sends the audio to the iPhone app, which transcribes it on the
   device. Install from the Watch app on the paired iPhone; tap Record, then
   Stop; the meeting appears in the iPhone app's Meetings tab."
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
- Delivery confirmation: the watch deletes its copy when WatchConnectivity
  reports the transfer finished, as the system guarantees delivery to the
  iPhone from there. If real-device testing shows files lost between the
  system and the app, a stronger option is an acknowledgment from the phone
  (`transferUserInfo` after staging) before the watch deletes.
- Signed archive/upload with the watch app (needs the portal steps above).
