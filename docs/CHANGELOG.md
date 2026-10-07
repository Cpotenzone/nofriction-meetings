# Changelog

All notable changes to noFriction Meetings are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Apple Watch app (iOS 1.0.0, next build).** Record a meeting on the watch
  with one tap: elapsed time, live level, Pause/Resume, Stop. It keeps
  recording with the wrist down; a call or Siri pauses it until you tap
  Resume (watchOS doesn't let apps restart recording in the background). The
  audio goes to the iPhone over Apple's watch connection and is deleted from
  the watch once the iPhone has it. The iPhone imports it as a normal
  meeting: transcribed on the iPhone, on the device, matched to your
  calendar, with AI notes, follow-up email, Delete / Strike, export and
  People. Long recordings are transcribed in chunks and continue where they
  stopped if the app is closed. See `docs/WATCH_APP.md`.

### Changed
- **Meetings, classes and everyday life (Mac).** Record asks **What is it?**
  (Meeting · Class · Personal, remembered; Meeting by default) above **How
  long?**. "Class" is now an optional **Notebook** for any type, with
  **Notebooks** filter chips. Notes follow the type: meeting notes, lecture
  notes, or (Personal) a summary, key points and to-dos. The third mark is
  ✎ Follow up / On the test / Remember, the STUDY tab is **REVIEW** (a Review
  guide, or Study guide for a class), and the school-policy notice shows on
  the first Class recording. Untitled recordings are named "BIO 101 — Oct 7"
  or "Personal — Oct 7". Recordings that had a class become Class recordings
  (`meetings.recording_kind`, backfilled once). UI text that meant any
  recording now says "recording". See `docs/TIMED_RECORDING_AND_NOTEBOOKS.md`.

---

## [3.6.0] - 2026-10-01

Prepared for TestFlight and App Store review on iPhone, iPad and Mac. AI runs
on Apple's on-device model or on an endpoint you set up yourself, transcription
stays on the device, and nothing goes through a noFriction server.

### Added
- **Choose where AI runs.** Use Apple's on-device model (iOS/macOS 26 or later
  with Apple Intelligence; no key, nothing leaves the device), or enter your
  own OpenAI-compatible endpoint: its base URL, the model and, only if it needs
  one, your key. Local servers such as Ollama or LM Studio work. noFriction
  supplies no AI service, model or API key, and nothing remote is set up by
  default. A key you enter stays in the Keychain, tied to that endpoint. The
  app asks before sending meeting content to a public endpoint, and shows
  where it will go.
- **Delete and "Strike from the record."** Remove words, whole lines or
  screens from a meeting:
  - **Delete** leaves no trace and has 5 seconds of undo.
  - **Strike** destroys the content permanently and leaves a marker with the
    time, the date and an optional reason.
  - Removed content is purged from the transcript, search, recorded audio
    (iPhone), screenshots and screen video, AI notes, exports, app backups and
    database free space.
- **Recordings stop when the meeting ends.** The app notices when Zoom, Teams,
  Meet or FaceTime lets go of the mic, the call window closes, the calendar
  event is over, or nobody has spoken for a few minutes. A 30-second banner
  lets you keep recording.
- **iPhone and iPad app.**
  - Settings tab.
  - Subscription (noFriction Pro, monthly or yearly) through the App Store.
  - Recording-consent notice.
  - Privacy manifest.
- **Mac App Store build** (sandboxed): `scripts/release-mas.sh`, with StoreKit
  and Apple on-device AI through a small Swift bridge.
- **Privacy policy and support pages** in `site/`.
- **New app icon.**

### Changed
- **Client-only.** Supabase, Pinecone and the ingest server were removed.
  Search and "chat with your meetings" now run on the local full-text index.
- **Transcription is on-device only:** local Whisper on the Mac, Apple speech
  recognition on iPhone and iPad. The cloud transcription options (Deepgram,
  Gladia, Google, Gemini) were removed.
- AI services chosen by name in earlier versions are no longer available; AI
  asks you to choose Apple on-device or enter an endpoint. Meetings are kept.
- The app's identifier is now `com.nofriction.meetings` on every platform. The
  Mac data folder moves on first launch, and nothing is deleted.

### Fixed
- **No more "bye bye bye" loops.** Whisper's invented filler on silence or
  noise is filtered out.
- **Meetings are marked as ended when you stop recording.** Before, they never
  were, so automatic reports never ran. Meetings left open by earlier versions
  are closed on launch.
- Fresh installs could randomly hit "no such table" right after setting up the
  database.
- Screen-recording permission prompts no longer repeat.
- Release builds refuse to ship a broken code signature, the cause of the
  repeated mic and screen permission prompts.
- Transcripts are no longer written to the log file.

---

## [2.7.0] - 2026-02-13

### Added

#### LiveIntelAgent v2 — Smart Real-Time Meeting Intelligence
- **8 event types** (was 6): ActionItem, Decision, RiskSignal, Commitment, QuestionSuggestion, TopicShift, **KeyInsight**, **Deadline**
- **Smart pattern matching**: Strong vs weak patterns — weak signals require 2+ co-occurring matches to fire
- **30-second cooldown** per event type to prevent insight spam
- **4-word minimum** segment guard — skips filler ("yeah", "uh huh") fragments
- **Hash-based deduplication** across rule-based and AI extraction paths
- **Deadline extraction**: 28 temporal patterns ("by Friday", "end of quarter", "ship by", etc.)
- **Sentiment tracking**: Exponential weighted average from 40+ signal words (-1.0 to 1.0)
- **Meeting energy score**: WPM rate + speaker diversity → 0-100 composite score

#### AI Integration Improvements
- AI prompt now includes live conversation state (current topic, sentiment, energy, speakers)
- AI response parsing for `key_insights` → proper `KeyInsight` event type
- AI response parsing for `deadlines` → `Deadline` events with owner and reference
- `MeetingStats` struct tracks aggregate counts of segments, words, speakers, and event types

### Changed
- Default AI model upgraded from `qwen2.5-coder:7b` to `qwen3:8b` (Sage — deep reasoning model)
- Serendipity API key updated
- `MeetingIntelPanel.tsx` icons: 💡 for key_insight, 📅 for deadline, ❓ for question_suggestion
- Meeting export (stop_recording) now includes Key Insights and Deadlines markdown sections

### Fixed
- `KeyInsight` events were incorrectly mapped to `QuestionSuggestion` — now use dedicated type
- Dead code warning on `AiRisk.type` field suppressed with `#[allow(dead_code)]`

---

## [2.6.0] - 2026-02-11

### Added

#### Prompt Studio — Master Prompt Management UI
- **🧠 PROMPTS tab** in Agency view with split-pane editor
- **Category filtering**: All, Intelligence, Meeting, VLM
- **Persona filtering**: Prospecting, Fundraising, Product Dev, Admin, Personal
- **Inline editor**: Edit name, description, system prompt, temperature, active toggle
- **Test Panel**: Collapsible input/response box — test prompts against AI inline
- **Duplicate & Delete**: Clone prompts for variants, delete custom prompts

#### Persona-Aware AI Pipeline
- **20 built-in prompts** seeded across 4 features × 5 personas
- **Prompt resolution**: All AI surfaces now resolve persona-specific prompts from PromptManager
- **Prompt fallbacks**: Graceful degradation when persona-specific prompt not found

#### LiveIntelAgent Hybrid AI Upgrade
- **Rule-based + AI-powered** dual-phase extraction for live meeting insights
- **`ai_analyze` method**: Sends transcript segments to AI with persona-specific system prompt
- **Structured parsing**: AI response parsed into ActionItems, Decisions, Risks, and KeyInsights
- **Graceful degradation**: Falls back to rule-based only if AI fails

#### New/Updated Tauri Commands
- `test_prompt` — Test any prompt with sample input from the UI
- `get_live_insights` — Now includes AI-powered phase for deeper insights
- `generate_catch_up`, `chat_with_data`, `generate_meeting_report` — All wired to PromptManager

### Changed
- HelpView updated with Prompt Studio documentation (navigation, how-to, tech spec)
- Meeting report prompt management moved from Settings to Prompt Studio
- CHANGELOG updated with full v2.6.0 entry

### Fixed
- CSS `background-clip` and `line-clamp` lint warnings in PromptStudio.css

---

## [2.5.0] - 2026-02-03

### Added

#### RAG (Retrieval Augmented Generation) Pipeline
- **Intelligent Data Access**: AI chat now searches your meeting history before responding
- **Context Cards**: Visual display of sources used in AI responses with confidence scores
- **Conversation Storage**: All Q&A pairs stored to Pinecone (vectors) and Supabase (structured)
- **RAG Toggle**: Enable/disable history search via UI toggle
- **History Quick Action**: One-click search across past meetings

#### New Tauri Commands
- `thebrain_rag_chat` - RAG-enhanced chat without storage
- `thebrain_rag_chat_with_memory` - RAG chat with automatic conversation storage
- `store_conversation` - Manual conversation storage
- `get_conversation_history` - Retrieve past conversations

#### Server-Side Updates (nofriction-intel)
- TheBrain OAuth token authentication in `vlm.py` and `llm.py`
- Token caching with automatic refresh on 401
- Environment variable configuration for credentials

### Changed
- `AIChat.tsx` - Integrated RAG toggle and context display
- `CopilotPanel.tsx` - Added RAG features and model selector

### Fixed
- RwLock guard issues across await points in async commands
- Old "Nano Banana" branding in release documentation

---

## [2.5.0-alpha] - 2026-01-25

### Added

#### Always-On Recording (v2.5 Major Feature)
- **Ambient Capture Mode**: Low-power background recording
- **Meeting Detection**: Auto-detect Zoom, Google Meet, Teams
- **Power Manager**: Battery-aware capture throttling
- **Privacy Filter**: Exclude sensitive apps from capture

#### New Modules
- `ambient_capture.rs` - Background capture service
- `meeting_trigger.rs` - Meeting app detection
- `power_manager.rs` - Battery optimization
- `privacy_filter.rs` - App exclusion rules
- `tray_builder.rs` - System tray enhancements
- `continue_prompt.rs` - Session continuation
- `interaction_loop.rs` - User interaction handling

#### New Commands
- `start_ambient_capture` / `pause_capture`
- `start_meeting_capture`
- `get_capture_mode`
- `get_always_on_settings` / `set_always_on_enabled`
- `get_running_meeting_apps`
- `check_audio_usage`
- `dismiss_meeting_detection`

### Changed
- Tray menu now shows capture mode status
- Settings UI includes Always-On configuration

---

## [2.1.0] - 2026-01-20

### Added

#### Admin Console (Management Suite)
- **Storage Management**: Visualize and manage recording storage
- **Recording Deletion**: Batch delete with preview
- **Audit Log**: Track all administrative actions
- **System Health**: Monitor app performance
- **Feature Flags**: Toggle experimental features

#### Native Text Extraction
- **Vision OCR**: macOS-native screen text extraction
- **Accessibility Extractor**: UI element parsing
- **Semantic Classifier**: Content categorization

#### Calendar Integration
- **macOS Calendar**: Read calendar events
- **Meeting Context**: Auto-associate recordings with events
- **Upcoming Meetings**: Show scheduled meetings

#### Prompt Management (Phase 2)
- **Theme-Specific Prompts**: Prompts organized by activity theme
- **Version History**: Track prompt changes
- **A/B Testing**: Compare prompt effectiveness

#### New Modules
- `admin_commands.rs` - Admin operations
- `audit_log.rs` - Action logging
- `data_editor.rs` - Learned data CRUD
- `storage_manager.rs` - Storage statistics
- `calendar_client.rs` - macOS Calendar API
- `semantic_classifier.rs` - Content classification
- `vision_ocr.rs` - Native OCR
- `accessibility_extractor.rs` - UI text extraction

#### New Components
- `AdminConsole.tsx` - System management UI
- `AuditLog.tsx` - Action history viewer
- `LearnedDataEditor.tsx` - Data editing
- `ToolsConsole.tsx` - Developer tools
- `VideoDiagnostics.tsx` - Capture diagnostics

### Changed
- Sidebar reorganized with admin section
- Settings split into multiple tabs

---

## [2.0.0] - 2026-01-10

### Added

#### Video Recording
- **Native Screen Recording**: Full video capture (not just frames)
- **Moment Pinning**: Bookmark important points
- **Frame Extraction**: Pull frames from video
- **Storage Management**: Video retention policies

#### VLM Scheduler
- **Batch Processing**: Queue frames for VLM analysis
- **Auto-Processing**: Configure automatic analysis intervals
- **Rate Limiting**: Prevent API overload

#### Activity Themes
- **Theme Tracking**: Track time spent per activity type
- **Theme-Specific Settings**: Different capture settings per theme
- **Today's Usage**: See theme time breakdown

#### Intelligence Pipeline
- **Ingest Queue**: Managed processing queue
- **Ingest Client**: nofriction-intel integration
- **Topic Clusters**: Group related content

### Changed
- Frame capture replaced with video recording
- VLM processing now batched and scheduled
- UI updated with video controls

### Deprecated
- Legacy frame dump functionality

---

## [1.5.0] - 2026-01-02

### Added

#### Realtime Transcription
- **Deepgram WebSocket**: Streaming speech-to-text
- **Speaker Diarization**: Who said what
- **Multi-Provider Support**: Deepgram, Gladia, Google STT

#### Combined Audio
- **System + Mic**: Capture both simultaneously
- **Audio Buffer**: Smooth audio handling

#### Prompt Library
- **Custom Prompts**: Create and edit AI prompts
- **Prompt Templates**: Variables and formatting
- **Import/Export**: Share prompts

#### Model Configuration
- **Multiple Models**: Select AI model per task
- **Local + Cloud**: Ollama and TheBrain support
- **Model Availability**: Check which models are ready

### Fixed
- Audio sync issues in long recordings
- Memory leak in frame extraction
- Transcript search performance

---

## [1.0.0] - 2025-12-28

### Added

#### Initial Release
- **Recording Engine**: Mic + screen capture
- **Transcription**: Deepgram integration
- **Rewind**: Visual timeline playback
- **AI Chat**: Ollama/TheBrain integration
- **Knowledge Base**: Pinecone vector search
- **Settings**: Comprehensive configuration
- **Setup Wizard**: First-run experience

#### Core Modules
- `capture_engine.rs`
- `database.rs`
- `transcription/`
- `ai_client.rs`
- `vlm_client.rs`
- `pinecone_client.rs`
- `settings.rs`

#### UI Components
- `App.tsx`
- `AIChat.tsx`
- `RewindGallery.tsx`
- `FullSettings.tsx`
- `SetupWizard.tsx`
- Plus 35+ supporting components

### Security
- Local-first data storage
- Encrypted at-rest
- Optional cloud sync

---

## Version History Summary

| Version | Date | Theme |
|---------|------|-------|
| 2.7.0 | 2026-02-13 | Smart Live Intel v2 + Qwen3 |
| 2.6.0 | 2026-02-11 | Prompt Studio + AI Intelligence |
| 2.5.0 | 2026-02-03 | RAG Pipeline + Always-On |
| 2.1.0 | 2026-01-20 | Admin Console + Calendar |
| 2.0.0 | 2026-01-10 | Video Recording + VLM |
| 1.5.0 | 2026-01-02 | Realtime Transcription |
| 1.0.0 | 2025-12-28 | Initial Release |

---

## Migration Notes

### Upgrading to 2.5.0

1. **Supabase Migration Required**
   Run the conversations table migration:
   ```sql
   -- See supabase/migrations/20260203_create_conversations_table.sql
   ```

2. **Environment Variables**
   For nofriction-intel, add:
   ```bash
   VLM_USERNAME=your_thebrain_username
   VLM_PASSWORD=your_thebrain_password
   ```

3. **RAG Toggle**
   RAG is enabled by default. Disable via the 📚 toggle if not using.

### Upgrading to 2.1.0

1. **Permissions**
   Grant Calendar access for meeting detection.

2. **Admin Console**
   Access via sidebar → Admin (requires local authentication).

---

## Roadmap

### Planned Features
- [ ] Conversation threading (group related Q&As)
- [ ] Query suggestions (auto-complete from history)
- [ ] Daily briefing generation  
- [ ] Mobile app companion
- [ ] Team sharing (multi-user knowledge base)
- [ ] Webhook integrations
- [ ] Custom model fine-tuning

### Known Issues
- `objc` crate warnings (cosmetic, does not affect functionality)
- Some unused imports in Rust code (scheduled for cleanup)

---

## Contributors

- noFriction AI Team
- Casey Potenzone

---

[2.7.0]: https://github.com/nofriction/meetings/compare/v2.6.0...v2.7.0
[2.6.0]: https://github.com/nofriction/meetings/compare/v2.5.0...v2.6.0
[2.5.0]: https://github.com/nofriction/meetings/compare/v2.1.0...v2.5.0
[2.1.0]: https://github.com/nofriction/meetings/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/nofriction/meetings/compare/v1.5.0...v2.0.0
[1.5.0]: https://github.com/nofriction/meetings/compare/v1.0.0...v1.5.0
[1.0.0]: https://github.com/nofriction/meetings/releases/tag/v1.0.0
