# Screen capture on iPhone and iPad

The Mac app keeps a screenshot when the screen changes. iPhone and iPad now
do the same while you record: noFriction captures what's on your screen as
pictures, next to the transcript. With noFriction Pro, **Transcribe what's
playing** also turns the sound of what's playing (a video, a class, a call)
into transcript lines marked **On screen**. Everything stays on the device.

Free/Pro is set by [PRO.md](PRO.md): screens are free; Transcribe what's
playing is Pro (feature key `transcribe_playing`, `ProFeature.transcribePlaying`).

App Review wording: "capture what's on your screen". Never "record movies"
or "save videos", and no app or service names.

## How it works

```
 Record sheet: Capture screen ──► RecordingSession.start ──► Apple's broadcast sheet
                                                             (noFriction preselected, mic hidden)
                                                                       │ Start Broadcast
                                                                       ▼
   NoFrictionBroadcast.appex (ReplayKit upload extension, ~50 MB limit, no network)
     video  ─► 64-cell luma grid ─► ScreenChangeDetector ─► JPEG ≤ 1280 px  ─┐
     .audioApp ─► AAC parts (only when the app's flag says Pro + switch on)  ├─► App Group container
     .audioMic ─► ignored (the app records the mic itself)                   │   ScreenCapture/<broadcast id>/
                                                                             ┘   manifest.json, f-<ms>.jpg, a-<n>.m4a
                       Darwin notifications (started / finished / control), names only
                                                                       │
   noFriction app: ScreenCaptureCenter ◄───────────────────────────────┘
     status ("Capturing your screen · 12 screens"), link broadcast ↔ recording,
     on Stop: ask the extension to end, import screens → Snapshot(source "screen"),
     app audio → Storage.audio → on-device transcription → Segment(source "screen")
     → files deleted
```

| Piece | File |
|---|---|
| Contract (App Group id, folder layout, manifest, shared keys, signals) | `ios/ScreenCaptureShared/ScreenCaptureContract.swift` |
| Change detector, throttle, near-black detector (pure, tested) | `ios/ScreenCaptureShared/FrameAnalysis.swift` |
| Darwin notifications | `ios/ScreenCaptureShared/DarwinSignal.swift` |
| Extension entry (`RPBroadcastSampleHandler`) | `ios/NoFrictionBroadcast/SampleHandler.swift` |
| Frames, app audio, manifest | `ios/NoFrictionBroadcast/BroadcastRecorder.swift` |
| Status, linking, stop, import, transcription queue | `ios/NoFriction/ScreenCapture/ScreenCaptureCenter.swift` |
| Prefs, Pro gate, import, transcription, purge | `ios/NoFriction/ScreenCapture/ScreenCaptureImport.swift` |
| Record sheet switches, status row, hidden-video notice | `ios/NoFriction/ScreenCapture/ScreenCaptureViews.swift` |
| Tests | `ios/NoFrictionTests/ScreenCaptureTests.swift` |

### Targets and signing

- `NoFrictionBroadcast` (`com.nofriction.meetings.broadcast`), an
  `app-extension` embedded at `noFriction.app/PlugIns/NoFrictionBroadcast.appex`,
  same version and build as the app, iPhone and iPad.
- App Group `group.com.nofriction.meetings` on the app and the extension
  (`ios/NoFriction/NoFriction.entitlements`,
  `ios/NoFrictionBroadcast/NoFrictionBroadcast.entitlements`, generated from
  `project.yml`).
- Automatic signing with `-allowProvisioningUpdates` registers the new App
  ID and the App Group when it has an App Store Connect API key
  (`ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_PATH`) or an Xcode account. If it
  can't, register them by hand:
  1. App Group: <https://developer.apple.com/account/resources/identifiers/add/applicationGroup>,
     identifier `group.com.nofriction.meetings`.
  2. App ID `com.nofriction.meetings` → App Groups → check
     `group.com.nofriction.meetings`:
     <https://developer.apple.com/account/resources/identifiers/list>.
  3. App ID `com.nofriction.meetings.broadcast` (new, explicit) with App
     Groups → the same group:
     <https://developer.apple.com/account/resources/identifiers/add/bundleId>.
- Manual signing: `NF_BROADCAST_PROFILE_UUID` (App Store profile for the
  extension) next to `NF_IOS_PROFILE_UUID` and `NF_WATCH_PROFILE_UUID`.
- `scripts/release-ios.sh` checks the extension's bundle id and the App Group
  in `project.yml`; after archiving it checks the `.appex` is embedded with
  the same version and build, has the broadcast upload extension point, is
  signed, both signatures carry the App Group, and the credential scan
  covered it (`--require-embedded PlugIns/NoFrictionBroadcast.appex=…`).
- `scripts/check-ai-provider-policy.py` fails if the extension or its shared
  code uses any network API (URLSession, URLRequest, Network.framework,
  sockets, streams, web views, multipeer, CloudKit) or if the extension's
  target asks for a network entitlement.

## Keeping screens

- The extension looks at a frame at most every 0.25 s. It reads the luma
  plane (or BGRA) into a grid of 64 cells on the long side, each the average
  of every 4th pixel in its block. No pixel buffer is copied or kept.
- `ScreenChangeDetector`: a cell changed when its brightness moved by ≥ 12.
  Under 0.4 % changed cells it's the same screen (a cursor, a clock). From
  20 % on it's a new screen (a slide, a page, a scene cut) and is kept once a
  second has passed since the last screen. A smaller change (a line of text)
  is kept after 5 s. Never more than one screen per second. Compared with
  the last screen **kept**, so a change that persists is kept as soon as the
  interval allows.
- A kept frame is oriented (ReplayKit's orientation attachment), scaled to
  1280 px on its long side and encoded as JPEG (quality 0.6) through one
  reused Core Image context, inside an autorelease pool. Named
  `f-<milliseconds since 1970>.jpg`, written atomically.
- Storage: roughly 100–200 KB a screen. A static screen costs nothing;
  continuous video is about one screen a second, less between cuts.

## Video an app hides from capture

iOS blanks protected (DRM) video in a broadcast. `HiddenFrame.isNearBlack`:
at least 98 % of cells at brightness ≤ 24 (full-range black is 0,
video-range 16; a subtitle still counts as hidden). Such frames are never
saved and never replace the last screen. The extension adds up hidden time
(`hiddenSeconds` in the manifest); after 3 s the app says once, in the
banner slot: "Some apps hide their video from screen capture; those parts
are skipped." It's shown once per install (`screenCaptureHiddenNoticeShown`).

## App audio: Transcribe what's playing (Pro)

- Gate: `ScreenAudioPolicy.allowsAppAudio(isPro:wantsIt:)` = Pro and the
  switch on (default on for Pro). The app publishes it to the shared
  defaults (`screenCapture.appAudio`) at launch, when Pro changes and when
  the switch changes. The extension reads it when a broadcast starts and
  writes **no app audio at all** unless it is true. The import checks again:
  if Pro is gone or the switch is off, app audio files are deleted, never
  kept.
- A free user who turns the switch on gets the paywall,
  `PaywallView(feature: .transcribePlaying)` (item-based sheet), titled
  "Transcribe what's playing is part of noFriction Pro".
- The extension writes `.audioApp` buffers to AAC (`AVAssetWriter`, the
  source rate up to 48 kHz, mono or stereo). Each pause starts a new part.
  `.audioMic` is ignored.
- After import the parts wait in `Storage.audio` as `screen-<recording>-….m4a`
  (`Meeting.screenAudioJSON`: file names, start times, progress; never
  content). When this iPhone isn't recording, they're transcribed with the
  same on-device file transcriber as Apple Watch recordings
  (`FileTranscribers.best()`, chunked, resumable), through `TranscriptFilter`.
- Lines become `Segment`s with `source = "screen"`, at wall-clock time, so
  they interleave with the microphone's lines. They're labeled **On screen**
  in the transcript and "(On screen)" in exports and AI prompts. They have no
  audio offset or word timings: the app audio is deleted once transcribed.
- Echo filter: when the video plays out loud, the microphone hears it too.
  An On screen line whose words (≥ 70 %, at least 3 words) the microphone
  heard within 5 s is dropped; the microphone line stays.

## Recording model

- Screen capture attaches to the recording in progress. The microphone
  recording carries on as always; recording stays free.
- **Record sheet:** "Capture screen" (remembered like the type and length).
  Record starts the recording, then opens Apple's broadcast sheet with
  noFriction preselected and the microphone button hidden: starting is one
  tap (Start Broadcast).
- **Record screen:** while recording, a row shows "Capturing your screen ·
  12 screens" with Stop, or a "Capture screen" button. Both open Apple's
  sheet (start or stop there).
- **Control Center:** starting the broadcast there while noFriction records
  attaches it to that recording (the app is running in the background and
  gets the extension's Darwin notification). Stopping it there imports its
  screens into the recording, which goes on.
- **No recording running:** the microphone can't start in the background,
  so when noFriction next comes on screen with the broadcast still running
  (and the recording notice already accepted), it starts a recording: the
  remembered type and the notebook last picked on the Record sheet, no time
  limit. A broadcast that ended before that becomes a recording of its own
  (its start and end, the remembered type and notebook, calendar match, no
  microphone audio). An empty one (no screens, no audio) is just removed.
- **Pause** tells the extension to keep nothing until Resume (the app audio
  part is closed).
- **Stop** asks the extension to end the broadcast (ReplayKit shows "Your
  recording in noFriction ended, so screen capture stopped."), waits up to
  3 s for it to close its files, imports, and deletes the broadcast folder.
  Screens arriving more than 10 s after the recording ended are deleted, not
  imported.
- A broadcast's id is linked to its recording (`screenCaptureLinks`, ids
  only), so files that arrive late or after a relaunch land in the right
  recording, and Delete Recording finds them.

## Audio session

The app records with `.playAndRecord`, mode `.spokenAudio`, options
`.defaultToSpeaker` and `.mixWithOthers` (`AudioCapture.categoryOptions`).

- `.mixWithOthers` was already set for every recording: activating
  noFriction's session doesn't interrupt another app's playback, and a
  non-mixable app that starts playing later doesn't interrupt noFriction's
  recording. No `.duckOthers`, so the other app isn't made quieter. No
  change was needed for screen capture.
- `.defaultToSpeaker` keeps the other app's sound on the speaker rather
  than the earpiece while noFriction records.
- One change: with **Bluetooth headphones**, `.allowBluetooth`/HFP would
  switch them to the hands-free profile and the video would sound like a
  phone call. A recording started with Capture screen on (or while a
  broadcast runs) uses `.allowBluetoothA2DP` instead: the headphones stay in
  high-quality playback and the iPhone's own microphone records. A broadcast
  started mid-recording doesn't change the session (reconfiguring a running
  engine would interrupt it).
- **Must be verified on a real device** (below): playback continuing, its
  volume, the route with AirPods, and that the recording keeps going while
  another app plays.

## Privacy and purge

- No network in the extension (policy guard). Screens and app audio go only
  to the App Group container, then into the app's own Documents, then (app
  audio) are deleted after transcription.
- The status bar turns red while the broadcast runs (iOS shows it to the
  user; the app can't hide it).
- Delete and Strike ([REDACTION.md](REDACTION.md)):
  - Screens are `Snapshot`s (`source = "screen"`), so Delete / Strike of a
    screen and Delete Recording remove the file like a photo; a struck
    screen leaves the usual marker.
  - On screen lines are `Segment`s: Delete / Strike splice their text like
    any line. They have no audio in the recording's file, so nothing is
    silenced for them (the microphone audio is never touched by them).
  - Delete Recording also deletes app audio still waiting to be transcribed
    and every broadcast folder linked to the recording in the App Group
    container (`ScreenCaptureImporter.purge`).
  - At launch, app audio no recording waits for and broadcast folders whose
    recording was deleted are removed (`removeLeftovers`); a folder without
    a manifest is removed on the second check.

## Limits

- **Simulator:** broadcasts don't run there, and neither does speech
  recognition. Everything below needs a real device.
- **DRM video** is black in a broadcast and skipped (above).
- **Memory:** extensions get about 50 MB. The extension keeps a few KB per
  frame (the grid) and one JPEG at a time; nothing is buffered behind a slow
  encode (ReplayKit drops frames instead).
- **Locking** the device ends the broadcast (iOS).
- **The red status bar** shows while capturing (iOS).
- **Starting from Control Center** with noFriction closed: the recording
  starts when you next open noFriction (the microphone can't start in the
  background).

## Tests

`ScreenCaptureTests.swift` (unit, Simulator):

- Grid size and averaging (luma with row padding, BGRA).
- Change detector: identical, cursor-sized and drift changes ignored; a new
  screen waits 1 s; a small change waits 5 s; 30 fps of changing video keeps
  exactly one per second; rotation; hidden frames never kept.
- Near-black detector (full and video range, subtitles, dark mode with text)
  and hidden-time accounting.
- Contract: frame names ↔ times, manifest round trip, liveness.
- Pro gate, switch, paywall need, published flag, feature key.
- Audio session options (mix, no duck, speaker, A2DP while capturing).
- Echo filter.
- Import: screens as Snapshots in time order, idempotent; late screens
  dropped; app audio kept only when allowed and only from an allowed
  broadcast; unfinished audio kept while live, deleted once over;
  transcription into On screen lines interleaved with the microphone, echo
  dropped, file deleted; resume; a broadcast with no recording.
- Purge: Delete Recording (waiting audio, shared folder, links), launch
  cleanup (never the microphone file), Strike / Delete of screens, Strike of
  an On screen line leaves the microphone audio byte-identical.
- Migration from a store without the new fields.

## Real-device test plan

Use an iPhone (and an iPad) on the build under test, with a video app and a
web page at hand. Check the red status bar each time capture is on.

1. **Start from the Record sheet.** Record → turn on Capture screen → Record.
   Apple's sheet opens with noFriction selected and no microphone button; tap
   Start Broadcast. Expect: the countdown, the red status bar, "Capturing your
   screen · 1 screen" on Record.
2. **Screens.** Open Safari, scroll a long page, switch pages, rotate to
   landscape and back. Return to noFriction: the count grew, about one per
   change, never faster than one a second. Leave a static screen for a minute:
   the count doesn't move.
3. **Stop from the app.** Tap Stop (the big button). ReplayKit says screen
   capture stopped; the status bar turns normal. Open the recording: Screens
   (or Photos and screens) shows thumbnails with the screen badge at the right
   times, landscape ones upright.
4. **Control Center.** Start a recording without Capture screen. In Control
   Center, long-press Screen Recording, pick noFriction, Start. Switch apps
   for a minute, then stop it from Control Center. The recording is still
   running and has the screens.
5. **Start with noFriction closed.** Close noFriction (don't force-quit a
   recording). Start the noFriction broadcast from Control Center, use other
   apps, then open noFriction: a recording starts with the remembered type and
   notebook and no limit. Repeat, but stop the broadcast before opening
   noFriction: a recording of just the screens appears in Recordings.
6. **Pause.** While capturing, Pause for 30 s and change screens; Resume.
   Nothing from the paused stretch is in the recording.
7. **Audio session.** With a video playing on the speaker, start a recording
   with Capture screen: the video keeps playing at the same volume, on the
   speaker. Repeat with AirPods: the video stays in full quality (no
   phone-call sound), and the recording keeps going while the video app is in
   front.
8. **Protected video.** Play a protected (DRM) video full screen for 10 s. The
   capture has no black screens from it, and noFriction says once: "Some apps
   hide their video from screen capture; those parts are skipped." It doesn't
   say it again on the next capture.
9. **Transcribe what's playing (free).** Without Pro, turn on Transcribe
   what's playing on the Record sheet: the paywall opens titled "Transcribe
   what's playing is part of noFriction Pro"; close it, the switch stays off.
   Capture a video with speech: the recording has screens but no On screen
   lines.
10. **Transcribe what's playing (Pro, sandbox).** With Pro, leave the switch on
    (default). Capture 3 minutes of a video with speech, with headphones (so the
    microphone doesn't hear it) and talk a little yourself. After Stop:
    "Transcribing what was playing…", then On screen lines interleaved by time
    with your microphone lines. Repeat on the speaker: lines the microphone also
    heard appear once, not twice.
11. **Delete and Strike.** Strike one screen and one On screen line: the
    screen's file is gone with a marker left, the line shows the marker, and
    playback of the recording is unchanged. Delete the recording while its app
    audio is still waiting, then download the app container (Xcode → Devices
    and Simulators → noFriction → Download Container): no `screen-*.m4a` for it
    in Documents/Audio, and nothing under the App Group's ScreenCapture folder.
12. **Memory.** Capture 20 minutes of continuous video. The broadcast doesn't
    stop by itself (an extension over its memory limit is ended by iOS).
13. **iPad.** Steps 1–3 on iPad, in landscape and with Split View.
