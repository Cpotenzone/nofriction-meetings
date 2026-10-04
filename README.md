# noFriction

Meeting recorder for **iPhone, iPad and Mac**. It records and transcribes on
the device, matches each meeting to your calendar and its attendees, keeps
photos (iOS) or screenshots (Mac) next to the transcript, and writes notes
with **Apple's on-device model or an AI endpoint you set up yourself**. No
accounts, no noFriction servers, no analytics.

| | iPhone / iPad | Mac |
|---|---|---|
| App | SwiftUI, iOS/iPadOS 18+ (`ios/`) | Tauri 2: Rust + React (`src-tauri/`, `src/`), macOS 12.3+ |
| Version | 1.0.0 | 3.6.0 |
| Transcription | Apple on-device speech | Local Whisper on the Mac (one-time model download); no cloud transcription |
| Screens | Camera photos, images from Photos | Screenshots of chosen displays/windows, Snap |
| AI (noFriction Pro) | Notes (summary, decisions, action items), follow-up email | Chat across meetings, live insights, after-meeting report, meeting prep brief, screenshot analysis |
| Also | Calendar match, People + LinkedIn, auto-stop, Delete / Strike from the record, Markdown share | Calendar match, People + LinkedIn, auto-stop, Delete / Strike from the record, Obsidian export, JSON export |

**AI** has two choices. **Apple on-device** (Foundation Models, iOS/macOS 26+
with Apple Intelligence on and its model available) needs no key and keeps
everything on the device. Or **your own endpoint**: you enter the base URL and
model of one OpenAI-compatible server (for example Ollama or LM Studio on your
own machine) and, if it needs one, your own key. There are no built-in service
presets, no default remote URL and no key detection, and noFriction supplies no
model, service or API key. A key you enter is stored in the Keychain, tied to
that endpoint. The app asks before it sends meeting content to a public
(non-local) endpoint. Spec: [docs/AI_PROVIDERS.md](docs/AI_PROVIDERS.md).

**Business model:** free download. Recording, transcription, calendar and
people, photos/screens, search and export are free. AI features need the
**noFriction Pro** subscription (monthly or yearly, StoreKit 2, verified on
the device). Bundle id `com.nofriction.meetings` on both platforms, so one
subscription covers both (Universal Purchase). The Developer ID (DMG) Mac
build has no StoreKit and no gating.

## Documentation

For customers:
- [docs/USER_GUIDE.md](docs/USER_GUIDE.md): how to use the apps
- [site/](site/): website (landing, support, privacy policy, terms); deploy steps in [site/README.md](site/README.md)

For launch:
- [docs/LAUNCH_CHECKLIST.md](docs/LAUNCH_CHECKLIST.md): the end-to-end commercial launch checklist
- [docs/APP_STORE_LISTING.md](docs/APP_STORE_LISTING.md): App Store metadata, privacy label, age rating, review notes, screenshot plan
- [docs/APP_STORE_RELEASE.md](docs/APP_STORE_RELEASE.md): App Store plan and portal steps
- [docs/MAC_APP_STORE_BUILD.md](docs/MAC_APP_STORE_BUILD.md): sandboxed Mac App Store build
- [docs/RELEASE_RUNBOOK.md](docs/RELEASE_RUNBOOK.md): Developer ID DMG build and notarization

For developers:
- [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md): build, run, test
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how the apps fit together
- [docs/AI_PROVIDERS.md](docs/AI_PROVIDERS.md): AI contract (Apple on-device or your own endpoint, keys, consent), shared by both apps
- [docs/REDACTION.md](docs/REDACTION.md): Delete and "Strike from the record" spec
- [ios/README.md](ios/README.md): iOS app file map and tests
- [docs/CHANGELOG.md](docs/CHANGELOG.md): release history
- [DESIGN.md](DESIGN.md): design system (hazard yellow on matte black)

## Quick start (developers)

```bash
# Mac app
npm install
npm run tauri dev
(cd src-tauri && cargo test)

# iPhone/iPad app
cd ios && xcodegen generate && open NoFriction.xcodeproj
```

Requirements: macOS with Xcode, Rust (stable), Node.js 18+, XcodeGen. No API
keys are needed to build. If your own AI endpoint needs a key, enter it in the
running app (never commit it).
Details: [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md).

## Support

casey@nofriction.io

© 2026 noFriction. All rights reserved.
