# noFriction for iPhone & iPad

Native SwiftUI app (iOS/iPadOS 18+), with an Apple Watch app (watchOS 10+)
inside it. Records meetings, transcribes them on the device, matches them to
your calendar, and links attendees to LinkedIn. Audio, photos and transcripts
stay on the device, and transcription runs on it. AI features (notes,
follow-up emails) run on Apple's on-device model, or send the meeting's text
to the one AI endpoint **you** enter. We run no servers.

**Apple Watch:** record on the watch; the audio goes to the iPhone, which
imports it as a normal meeting (transcribed on the iPhone, calendar-matched,
AI notes, Delete / Strike, export). See [docs/WATCH_APP.md](../docs/WATCH_APP.md).

## Build

```bash
cd ios
xcodegen generate            # project.yml is the source of truth; .xcodeproj is generated
open NoFriction.xcodeproj
```

Team `C7GCEESE2V`, automatic signing. Bundle id `com.nofriction.meetings` (shared with
the Mac app for Universal Purchase); the watch app is `com.nofriction.meetings.watchkitapp`,
embedded at `noFriction.app/Watch/NoFrictionWatch.app`. Version = `MARKETING_VERSION` in
`project.yml` (both apps); the build number comes from `build_number.txt` (last one used),
bumped by the release script.

| Target | What |
|---|---|
| `NoFriction` | iPhone/iPad app (`NoFriction/` + `Shared/`) |
| `NoFrictionWatch` | Apple Watch app (`NoFrictionWatch/` + `Shared/`), scheme `NoFrictionWatch` |
| `NoFrictionTests`, `NoFrictionUITests` | iPhone unit / UI tests |
| `NoFrictionWatchTests` | watch unit tests (run on a watch simulator) |

## Release (TestFlight / App Store)

```bash
scripts/release-ios.sh --check     # validate config, change nothing
scripts/release-ios.sh             # archive + export an App Store .ipa
ASC_KEY_ID=… ASC_ISSUER_ID=… ASC_KEY_PATH=~/keys/AuthKey_….p8 \
  scripts/release-ios.sh --upload  # archive + upload to App Store Connect
```

Export settings live in `ExportOptions.plist`. The API key is read from the
environment only; never commit it. The archive and IPA must contain the watch
app; the script checks it (same version/build, signed) and the credential scan
covers it. Manual signing needs a second profile for the watch app
(`NF_WATCH_PROFILE_UUID`; see the script header and docs/WATCH_APP.md).

## App Store screenshots

```bash
scripts/ios-screenshots.sh         # iPhone 6.9" + iPad 13" + Apple Watch 46mm → AppStore/screenshots/<device>/NN-name.png
scripts/ios-screenshots.sh watch   # just the watch (416x496)
```

Boots the existing simulators, sets the 9:41 status bar, runs
`NoFrictionUITests/AppStoreScreenshots` on invented sample data
(`-NFSeedDemo -NFDemoLive`) and checks the pixel sizes. The watch shots come
from the watch app's debug demo states (`-NFWatchDemo recording|idle|list`),
flattened to opaque PNGs.

Debug runs use `NoFriction.storekit` (local StoreKit testing: both Pro products with
a 1-week free trial). Release/TestFlight builds use the real App Store; there is no
Pro bypass.

## How it works

| Piece | File |
|---|---|
| Mic capture + AAC file per meeting (keeps recording with the screen locked) | `Capture/AudioCapture.swift` |
| Session: start/stop/pause, calendar match, saving as it goes | `Capture/RecordingSession.swift` |
| Transcription engines (swappable) | `Transcription/` |
| Calendar (EventKit, read-only) + matching rules shared with the Mac app | `Calendar/CalendarService.swift` |
| LinkedIn search + link normalization | `Calendar/LinkedIn.swift` |
| Store (SwiftData): Meeting, Segment, Snapshot, Person, Attendance | `Models/Models.swift` |
| AI providers (presets, key detection, URL policy, redaction) | `AI/AIProvider.swift` |
| AI requests + guardrails (OpenAI-compatible, Anthropic) | `AI/AIClient.swift` |
| Apple on-device model (iOS 26+) | `AI/AppleOnDevice.swift` |
| Keys (Keychain, this device only) / non-secret AI config | `AI/KeychainStore.swift`, `AI/AISettings.swift` |
| Meeting prompts | `AI/MeetingAI.swift` |
| Edit / Delete / Strike from the record (spec: `docs/REDACTION.md`) | `Redaction/`, `Views/TranscriptEditing.swift` |
| StoreKit 2 (noFriction Pro) + paywall | `Store/Store.swift`, `Views/PaywallView.swift` |
| Settings tab, consent + recording notices | `Views/SettingsView.swift`, `Views/Sheets.swift` |
| First-run welcome (two steps: welcome, permissions in context; the recording notice comes on the first Record, AI and Pro when first needed); Settings → Show welcome again | `Views/OnboardingView.swift` |
| Privacy manifest | `PrivacyInfo.xcprivacy` (watch app: `../NoFrictionWatch/PrivacyInfo.xcprivacy`) |
| Apple Watch recordings: WatchConnectivity, inbox, import + on-device file transcription | `Watch/` (spec: `docs/WATCH_APP.md`) |
| Watch ↔ iPhone contract (metadata, audio format, shared notice text) | `../Shared/WatchTransfer.swift` |
| Watch app: recorder, transfer queue, UI | `../NoFrictionWatch/` |

**AI** follows `docs/AI_PROVIDERS.md` (shared with the Mac app): Apple's
on-device model where available (iOS 26+, Apple Intelligence on), or one
OpenAI-compatible endpoint the user enters in Settings (base URL, model ID and,
only if it needs one, a key kept in the Keychain). There are no built-in
services, presets or keys, and saving makes no network request. Before meeting
content first goes to a public endpoint the app asks for consent. AI notes and
follow-up emails need noFriction Pro.

**Transcription** picks the best on-device engine:

- **iOS 26+**: `SpeechAnalyzer` + `SpeechTranscriber` — Apple's long-form model,
  streaming live (volatile) text that settles into final text. Attendee names
  from the invite are passed as contextual strings so they're spelled right.
- **iOS 18–25**: `SFSpeechRecognizer` forced on-device, committing text at
  natural pauses and rotating recognition tasks so long meetings don't hit
  system limits.

Neither runs in the Simulator (Apple disables on-device speech there); test
transcription on a device. Apple Watch recordings are transcribed from the
file with the same two engines (`Watch/FileTranscription.swift`), in chunks,
resumable, through the same `TranscriptFilter`.

**Screens**: iOS doesn't let one app capture another app's screen, so the
phone/iPad version takes photos (slides, whiteboards, a laptop screen) or
imports screenshots from Photos into the meeting timeline.

## Tests

```bash
xcodebuild test -project NoFriction.xcodeproj -scheme NoFriction \
  -destination 'platform=iOS Simulator,name=NF iPhone 17 Pro' -only-testing:NoFrictionTests
xcodebuild test -project NoFriction.xcodeproj -scheme NoFrictionWatch \
  -destination 'platform=watchOS Simulator,name=NF Apple Watch Series 11 (46mm)'
```

- `NoFrictionTests` — LinkedIn normalization, calendar matching, name/company parsing,
  key detection, URL policy, redaction, context fitting, request building, entitlements.
- `NoFrictionTests/WatchImportTests` — watch metadata, idempotent import, calendar match,
  chunked transcription with a scripted transcriber, resume from checkpoint, Strike on watch audio.
- `NoFrictionWatchTests` — watch recorder state machine, transfer queue (delete on delivery,
  keep and retry on error), crash recovery.
- `NoFrictionTests/RedactionTests` — word splicing, strike markers, AI-notes redaction, export/prompt
  placeholders, audio silencing (decoded zeros, same length), photo purge, store files free of struck text.
- `NoFrictionUITests/ScreensTests` — walks every screen with sample data
  (`-NFSeedDemo`), saving screenshots to `$NF_SCREENSHOT_DIR`; `AppStoreScreenshots`
  writes the store set to `$NF_APPSTORE_DIR`.
- `NoFrictionUITests/OnboardingTests` — the first-run flow end to end (`-NFResetOnboarding`),
  reopening it from Settings; `DynamicTypeTests` checks critical controls at XXXL text.
- `NoFrictionUITests/RecordingFlowTests` — end-to-end recording from an audio
  file (`NF_TEST_AUDIO`, debug builds only) — **device only**. Copy the file
  into the app's Documents with `xcrun devicectl device copy to …` and pass
  `TEST_RUNNER_NF_TEST_AUDIO=<file name>`.
