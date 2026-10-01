# noFriction for iPhone & iPad

Native SwiftUI app (iOS/iPadOS 18+). Records meetings, transcribes them on
the device, matches them to your calendar, and links attendees to LinkedIn.
Audio, photos and transcripts stay on the device, and transcription runs on it.
AI features (notes, follow-up emails) send the meeting's text to the AI provider
**you** choose, with your own key, or run on Apple's on-device model. We run no
servers.

## Build

```bash
cd ios
xcodegen generate            # project.yml is the source of truth; .xcodeproj is generated
open NoFriction.xcodeproj
```

Team `C7GCEESE2V`, automatic signing. Bundle id `com.nofriction.meetings` (shared with
the Mac app for Universal Purchase). Version = `MARKETING_VERSION`, build =
`CURRENT_PROJECT_VERSION` in `project.yml`; bump the build for every upload.

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
| Privacy manifest | `PrivacyInfo.xcprivacy` |

**AI** follows `docs/AI_PROVIDERS.md` (shared with the Mac app): paste a key in
Settings and the provider is detected from its prefix and checked by listing models.
Before the first request to a cloud provider the app asks for consent. With no
provider set, Apple's on-device model is used where available (iOS 26+, Apple
Intelligence on). AI notes and follow-up emails need noFriction Pro.

**Transcription** picks the best on-device engine:

- **iOS 26+**: `SpeechAnalyzer` + `SpeechTranscriber` — Apple's long-form model,
  streaming live (volatile) text that settles into final text. Attendee names
  from the invite are passed as contextual strings so they're spelled right.
- **iOS 18–25**: `SFSpeechRecognizer` forced on-device, committing text at
  natural pauses and rotating recognition tasks so long meetings don't hit
  system limits.

Neither runs in the Simulator (Apple disables on-device speech there); test
transcription on a device.

**Screens**: iOS doesn't let one app capture another app's screen, so the
phone/iPad version takes photos (slides, whiteboards, a laptop screen) or
imports screenshots from Photos into the meeting timeline.

## Tests

```bash
xcodebuild test -project NoFriction.xcodeproj -scheme NoFriction \
  -destination 'platform=iOS Simulator,name=NF iPhone 17 Pro' -only-testing:NoFrictionTests
```

- `NoFrictionTests` — LinkedIn normalization, calendar matching, name/company parsing,
  key detection, URL policy, redaction, context fitting, request building, entitlements.
- `NoFrictionTests/RedactionTests` — word splicing, strike markers, AI-notes redaction, export/prompt
  placeholders, audio silencing (decoded zeros, same length), photo purge, store files free of struck text.
- `NoFrictionUITests/ScreensTests` — walks every screen with sample data
  (`-NFSeedDemo`), saving screenshots to `$NF_SCREENSHOT_DIR`.
- `NoFrictionUITests/RecordingFlowTests` — end-to-end recording from an audio
  file (`NF_TEST_AUDIO`, debug builds only) — **device only**. Copy the file
  into the app's Documents with `xcrun devicectl device copy to …` and pass
  `TEST_RUNNER_NF_TEST_AUDIO=<file name>`.
