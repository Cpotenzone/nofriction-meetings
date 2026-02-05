# noFriction Meetings - Complete System Documentation

## What Is This Application?

noFriction Meetings is a **professional macOS meeting companion** that automatically records, transcribes, and analyzes meetings. It captures audio, screen content, and generates AI-powered insights—all while respecting privacy with local-first processing.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           noFriction Meetings                               │
│                          (Tauri Desktop App)                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  FRONTEND (React + TypeScript)              BACKEND (Rust)                  │
│  ┌──────────────────────────────┐           ┌────────────────────────────┐ │
│  │ 43 React Components          │           │ 45 Rust Modules            │ │
│  │ • AIChat, CopilotPanel       │    IPC    │ • capture_engine           │ │
│  │ • RewindGallery              │◄─────────►│ • transcription/           │ │
│  │ • ActivityTimeline           │           │ • database                  │ │
│  │ • AdminConsole               │           │ • vlm_client               │ │
│  │ • PromptLibrary              │           │ • pinecone_client          │ │
│  │ • KnowledgeBaseSettings      │           │ • supabase_client          │ │
│  └──────────────────────────────┘           └────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           EXTERNAL SERVICES                                 │
├─────────────────┬─────────────────┬─────────────────┬───────────────────────┤
│   Deepgram      │   TheBrain      │   Pinecone      │   Supabase           │
│   (Speech→Text) │   (AI/LLM)      │   (Vectors)     │   (PostgreSQL)       │
└─────────────────┴─────────────────┴─────────────────┴───────────────────────┘
```

---

## Core Features

### 1. Recording System

**What it does:** Captures microphone audio, system audio, and screen content simultaneously.

**Components:**
- `capture_engine.rs` - Main capture coordinator
- `video_recorder.rs` - Screen recording (video)
- `ambient_capture.rs` - Always-on background capture
- `frame_extractor.rs` - Periodic screenshot extraction

**Modes:**
| Mode | Description |
|------|-------------|
| Meeting Capture | Full recording when meetings detected |
| Ambient Capture | Low-power background monitoring |
| Manual Recording | User-initiated sessions |

---

### 2. Transcription System

**What it does:** Converts speech to text in real-time using Deepgram.

**Components:**
- `transcription/` module - Multi-provider support
- `deepgram_client.rs` - WebSocket streaming
- Speaker diarization (who said what)

**Providers Supported:**
- Deepgram (primary)
- Gladia
- Google Speech-to-Text

---

### 3. Visual Intelligence (VLM)

**What it does:** Analyzes screenshots using vision-language models to understand screen content.

**Components:**
- `vlm_client.rs` - TheBrain API integration
- `vlm_scheduler.rs` - Batch processing queue
- `vision_ocr.rs` - Native macOS text extraction
- `snapshot_extractor.rs` - Text/UI element extraction

**Pipeline:**
```
Screenshot → OCR Text → VLM Analysis → Structured Data → Database
```

---

### 4. AI Chat & RAG Pipeline

**What it does:** Intelligent chatbot that searches your meeting history for context before responding.

**Components:**
- `ai_client.rs` - Local Ollama models
- `vlm_client.rs` - TheBrain cloud API
- `pinecone_client.rs` - Vector search
- `commands.rs` - RAG commands

**Flow:**
```
Question → Pinecone Search → Context Assembly → TheBrain → Response
           ↓
    Store conversation for future retrieval
```

---

### 5. Knowledge Base

**What it does:** Searchable index of all your meetings, transcripts, and AI analyses.

**Storage:**
| Data | Local (SQLite) | Pinecone | Supabase |
|------|----------------|----------|----------|
| Transcripts | ✅ | ✅ | ✅ |
| Frames/OCR | ✅ | ✅ | ✅ |
| AI Insights | ✅ | ✅ | ✅ |
| Conversations | ✅ | ✅ | ✅ |

**Search Types:**
- Keyword search (SQLite FTS)
- Semantic search (Pinecone vectors)
- Timeline-based browsing

---

### 6. Meeting Detection

**What it does:** Automatically detects when video meetings start (Zoom, Google Meet, Teams).

**Components:**
- `meeting_trigger.rs` - App detection
- `calendar_client.rs` - macOS Calendar integration

**Triggers:**
- Meeting apps becoming active
- Calendar events starting
- Audio input activation

---

### 7. Timeline & Rewind

**What it does:** Visual playback of past sessions with synchronized audio, transcripts, and screenshots.

**Components:**
- `timeline_builder.rs` - Event aggregation
- `episode_builder.rs` - Session chunking
- `state_builder.rs` - Diff-based states

**UI Components:**
- `RewindGallery.tsx` - Visual timeline
- `SyncedTimeline.tsx` - Transcript sync
- `ActivityTimeline.tsx` - Activity feed

---

### 8. Prompt Library

**What it does:** Customizable AI prompts for different analysis types and themes.

**Components:**
- `prompt_manager.rs` - Prompt CRUD
- `PromptLibrary.tsx` - UI editor
- `semantic_classifier.rs` - Context classification

**Features:**
- Prompt templates with variables
- Theme-specific prompts
- A/B testing (comparison lab)

---

### 9. Admin Console

**What it does:** System management, storage, and data editing.

**Components:**
- `admin_commands.rs` - Admin operations
- `audit_log.rs` - Action logging
- `data_editor.rs` - Learned data management
- `storage_manager.rs` - Storage stats

**Capabilities:**
- Storage usage visualization
- Recording deletion with preview
- Learned data editing/versioning
- System health monitoring

---

## Data Flow Diagram

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   CAPTURE    │     │   PROCESS    │     │    STORE     │
├──────────────┤     ├──────────────┤     ├──────────────┤
│ • Microphone │     │ • Deepgram   │     │ • SQLite     │
│ • System     │────►│ • Vision OCR │────►│ • Pinecone   │
│   Audio      │     │ • VLM        │     │ • Supabase   │
│ • Screen     │     │ • Classifier │     │              │
└──────────────┘     └──────────────┘     └──────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────┐
│                       RETRIEVE                           │
├──────────────────────────────────────────────────────────┤
│ • Search (keyword, semantic)                             │
│ • Timeline browsing                                      │
│ • AI chat with RAG                                       │
│ • Export                                                 │
└──────────────────────────────────────────────────────────┘
```

---

## Configuration

### Required API Keys

| Service | Setting Location | Purpose |
|---------|------------------|---------|
| Deepgram | Settings → Transcription | Speech-to-text |
| TheBrain | Settings → AI | Cloud LLM |
| Pinecone | Settings → Knowledge Base | Vector search |
| Supabase | Settings → Knowledge Base | Cloud storage |

### macOS Permissions

| Permission | Why Needed |
|------------|------------|
| Microphone | Audio capture |
| Screen Recording | Visual capture |
| Accessibility | Text extraction |
| Calendar | Meeting detection |

---

## Frontend Components

| Component | Purpose |
|-----------|---------|
| `App.tsx` | Main layout + routing |
| `AIChat.tsx` | AI chat with RAG |
| `CopilotPanel.tsx` | Side panel chat |
| `RewindGallery.tsx` | Visual timeline |
| `ActivityTimeline.tsx` | Activity feed |
| `FullSettings.tsx` | All settings |
| `AdminConsole.tsx` | System management |
| `PromptLibrary.tsx` | Prompt editor |
| `KnowledgeBaseSettings.tsx` | KB config |
| `SetupWizard.tsx` | First-run setup |

---

## Backend Modules

### Core
| Module | Purpose |
|--------|---------|
| `lib.rs` | App initialization |
| `commands.rs` | Tauri command handlers |
| `database.rs` | SQLite operations |
| `settings.rs` | App settings |

### Capture
| Module | Purpose |
|--------|---------|
| `capture_engine.rs` | Recording coordinator |
| `video_recorder.rs` | Screen video |
| `ambient_capture.rs` | Background mode |
| `frame_extractor.rs` | Screenshot extraction |

### AI/ML
| Module | Purpose |
|--------|---------|
| `ai_client.rs` | Ollama client |
| `vlm_client.rs` | TheBrain client |
| `pinecone_client.rs` | Vector DB |
| `prompt_manager.rs` | Prompt templates |

### Intelligence
| Module | Purpose |
|--------|---------|
| `meeting_intel.rs` | Meeting insights |
| `live_intel_agent.rs` | Real-time analysis |
| `semantic_classifier.rs` | Content classification |
| `timeline_builder.rs` | Event aggregation |

---

## Version History

| Version | Features |
|---------|----------|
| v2.1.0 | Admin Console, Calendar, OCR, Classification |
| v2.5.0 | Always-On Recording, Meeting Detection |
| Current | RAG Pipeline, TheBrain Integration |

---

## Getting Started

1. **Install:** Download DMG → Drag to Applications
2. **Permissions:** Grant Microphone, Screen Recording, Accessibility
3. **Configure:** Add Deepgram API key in Settings → Transcription
4. **Record:** Click "Record" or wait for auto-detection
5. **Review:** Use Rewind tab to browse recordings
6. **Chat:** Ask the AI about your meetings

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| No transcription | Check Deepgram API key |
| No screenshots | Grant Screen Recording permission |
| AI not responding | Check TheBrain authentication |
| Search returns nothing | Ensure Pinecone is configured |
