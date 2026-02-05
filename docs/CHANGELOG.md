# Changelog

All notable changes to noFriction Meetings are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[2.5.0]: https://github.com/nofriction/meetings/compare/v2.1.0...v2.5.0
[2.1.0]: https://github.com/nofriction/meetings/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/nofriction/meetings/compare/v1.5.0...v2.0.0
[1.5.0]: https://github.com/nofriction/meetings/compare/v1.0.0...v1.5.0
[1.0.0]: https://github.com/nofriction/meetings/releases/tag/v1.0.0
