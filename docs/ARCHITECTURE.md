# noFriction architecture

noFriction is two native-feeling clients with **no backend**: a Mac app
(Tauri 2: Rust + React/TypeScript) and an iPhone/iPad app (SwiftUI). Each
records, transcribes and stores meetings on the device. AI features call the
AI provider the user picked, directly from the device, with the user's own
key (or Apple's on-device model). Licensing is StoreKit 2, verified on the
device. There are no accounts, no noFriction servers, no analytics.

```
                 ┌──────────────────────── device ────────────────────────┐
 mic / system    │ capture ─► transcription (on-device) ─► local store    │
 audio, screens, │    │            Whisper (Mac)            SQLite (Mac)  │
 photos          │    │            Apple Speech (iOS)       SwiftData(iOS)│
                 │    ▼                                         │         │
 calendar ──────►│ meeting match + attendees ──────────────────►│         │
 (read-only)     │                                              ▼         │
                 │  UI: live view, meetings, people, search, edit/strike  │
                 │                    │ AI feature (Pro)                  │
                 │                    ▼                                   │
                 │  AI layer (Apple on-device or a user endpoint; consent)│
                 └────────────┬───────────────────────┬───────────────────┘
                              ▼                       ▼
                user-entered endpoint          Apple on-device
                (OpenAI-compatible URL)        (Foundation Models, no network)
```

Specs shared by both apps:
- [AI_PROVIDERS.md](AI_PROVIDERS.md): Apple on-device or one user-entered
  endpoint, URL policy, guardrails, endpoint-bound Keychain storage, consent,
  StoreKit licensing and Pro gating.
- [REDACTION.md](REDACTION.md): Delete and "Strike from the record".

Release and distribution:
- [APP_STORE_RELEASE.md](APP_STORE_RELEASE.md) (App Store plan),
  [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md) (sandboxed `mas` flavor),
  [RELEASE_RUNBOOK.md](RELEASE_RUNBOOK.md) (Developer ID DMG),
  [LAUNCH_CHECKLIST.md](LAUNCH_CHECKLIST.md).

---

## Mac app (`src/`, `src-tauri/`)

### Flavors

One codebase, two builds. The UI asks the backend what it can do
(`get_build_capabilities`: `src-tauri/src/build_info.rs`, `src/lib/build.ts`).

| | Developer ID DMG | Mac App Store (`--features mas`) |
|---|---|---|
| Sandbox | no (hardened runtime) | App Sandbox |
| Screen video (ffmpeg) | yes | no; screenshots only |
| Accessibility text capture | yes (optional) | no |
| Admin console | yes | hidden |
| StoreKit / Pro gating | none | AI gated behind noFriction Pro |

Details: [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md).

### Backend modules (`src-tauri/src/`)

| Area | Modules |
|---|---|
| App wiring, commands | `lib.rs`, `commands/` (`mod.rs`, `ai.rs`, `capture.rs`, `capture_sources.rs`, `intel.rs`, `local_stt.rs`, `people.rs`, `prompt.rs`, `vault.rs`) |
| Storage | `database.rs` (SQLite via sqlx, FTS5 search), `settings.rs`, `paths.rs` (data folder + one-time migration), `storage_manager.rs` |
| Capture | `capture_engine.rs` (screenshots of chosen displays/windows, dedupe), `audio_mixer.rs` (mic + system audio), `core_audio.rs`, `privacy_filter.rs`, `dedupe_gate.rs`; DMG only: `video_recorder.rs`, `frame_extractor.rs`, `chunk_manager.rs`, `accessibility_*.rs` |
| Transcription | `transcription/`: `local_whisper.rs` (default, whisper.cpp model `large-v3-turbo` q5, downloaded once into `<app data>/models`) is the only provider (cloud transcription was removed 2026-10-03); `filter.rs` drops Whisper's invented filler on silence |
| Meetings | `meeting_end.rs` (auto-stop: call-app mic release, window, calendar, silence), `calendar_client.rs`, `people.rs`, `attendee_intel.rs` |
| AI | `ai/` (`providers.rs` Apple + custom endpoint choices, URL policy, log redaction; `client.rs` adapters + guardrails; `config.rs`; `commands.rs`), `ai_client.rs` (prompt-level helpers), `meeting_notes.rs`, `live_intel_agent.rs`, `meeting_intel.rs`, `catch_up_agent.rs`, `vlm_client.rs` + `vlm_scheduler.rs` (screenshot analysis via the configured AI), `vision_ocr.rs` (Apple Vision OCR, local), `prompt_manager.rs` |
| Editing | `redaction.rs` (+ `redaction/tests.rs`): Delete / Strike purge pipeline |
| Secrets | `secrets.rs`: Keychain; migrates and deletes old plaintext keys; removes leftovers of retired integrations |
| Store | `store.rs` + `swift/NoFrictionBridge/` (StoreKit 2 in `mas`, Apple Foundation Models in both), `entitlement.rs` |
| Export | `obsidian_vault.rs` + `bookmarks.rs` (security-scoped folder access) |

Every LLM call goes through `ai::client::complete`, which applies the
guardrails and (in `mas`) `entitlement::require_pro()`.

### Frontend (`src/`)

`App.tsx` hosts `components/agency/AgencyLayout.tsx`. Main views
(`AgencyNavbar.tsx`): **Live** (recording, live transcript), **Rewind**
(meeting history, timeline of transcript + screenshots, notes), **Intel**,
**Chat** (ask questions across meetings, answered from local search results),
plus **Vault**, **Zen**, **Prompts** and **Help** under "More views".
Settings (`features/settings/FullSettings.tsx`): General, Transcription,
Obsidian, AI Engine, Subscription (`mas` only), Data. Consent and paywall:
`AiConsentModal.tsx`, `PaywallModal.tsx`, `withAiConsent()` in `lib/ai.ts`.
Editing UI: `components/redaction/Redaction.tsx`.

### Chat with your meetings

`chat_with_data` (`commands/intel.rs`) searches the local SQLite FTS index
(`search_knowledge_base`), puts the top results into the prompt, and asks the
active text provider. There is no vector database and nothing is uploaded
except that prompt.

### Data on disk

| Item | DMG | Mac App Store |
|---|---|---|
| Data folder | `~/Library/Application Support/com.nofriction.meetings/` | `~/Library/Containers/com.nofriction.meetings/Data/Library/Application Support/com.nofriction.meetings/` |
| Database | `nofriction_meetings.db` in the data folder | same |
| Logs | `logs/` in the data folder (no transcript text) | same |
| Whisper models | `models/` in the data folder | same |
| API keys | login Keychain, service `com.nofriction.meetings.ai` | data-protection Keychain |

---

## iPhone / iPad app (`ios/`)

SwiftUI, iOS/iPadOS 18+, generated with XcodeGen (`ios/project.yml`). File
map and test commands: [ios/README.md](../ios/README.md).

- **Capture**: `AudioCapture` writes an AAC file per meeting and keeps
  recording with the screen locked (background audio mode).
  `RecordingSession` handles start/stop/pause and the calendar match.
  `MeetingEndDetector` decides when the meeting is over.
- **Transcription** (on device only): `SpeechAnalyzer`/`SpeechTranscriber`
  on iOS 26+, on-device `SFSpeechRecognizer` on iOS 18–25. Word timings are
  stored for precise audio removal.
- **Screens**: iOS can't capture other apps, so meetings get photos (camera)
  or images imported from Photos.
- **Store**: SwiftData (`Meeting`, `Segment`, `Snapshot`, `Person`,
  `Attendance`, `Redaction`).
- **AI**: `AI/` mirrors the Mac provider layer; `MeetingAI` has two features:
  notes (summary, decisions, action items) and a follow-up email draft.
- **StoreKit 2**: `Store/Store.swift`, `Views/PaywallView.swift`.

---

## Shared rules

- **No servers.** Network traffic is limited to: the AI endpoint the user
  entered (if any), the one-time Whisper model download (Mac, Hugging Face),
  StoreKit (Apple), and links the user opens (e.g. LinkedIn search).
- **Keys** live only in the Keychain and never reach the UI (only `last4`).
- **Consent** is asked before meeting content first goes to a public custom
  endpoint; it's tied to that endpoint.
- **Free vs Pro**: recording, transcription, calendar/people, photos and
  screens, search and export are free; AI features need noFriction Pro
  (App Store builds only).
- **Universal Purchase**: both apps use bundle id `com.nofriction.meetings`
  and the subscription group "noFriction Pro"
  (`com.nofriction.meetings.pro.monthly`, `.pro.yearly`).
