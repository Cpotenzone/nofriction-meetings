# noFriction Meetings - User Guide

## Welcome to noFriction Meetings

noFriction Meetings is your AI-powered meeting companion for macOS. It automatically captures, transcribes, and analyzes your meetings, giving you instant access to searchable transcripts, visual timelines, and intelligent insights.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Recording Meetings](#recording-meetings)
3. [Reviewing with Rewind](#reviewing-with-rewind)
4. [AI Chat & Intelligence](#ai-chat--intelligence)
5. [Knowledge Base Search](#knowledge-base-search)
6. [Settings & Configuration](#settings--configuration)
7. [Troubleshooting](#troubleshooting)
8. [Keyboard Shortcuts](#keyboard-shortcuts)
9. [Privacy & Security](#privacy--security)

---

## Getting Started

### Installation

1. **Download** the latest `.dmg` from [releases](https://github.com/nofriction/meetings/releases)
2. **Open** the DMG and drag `noFriction Meetings` to your Applications folder
3. **Launch** from Applications or Spotlight (`⌘ + Space`, type "noFriction")

### First Launch Setup

When you first launch noFriction Meetings, a setup wizard guides you through:

#### Step 1: Grant Permissions

macOS requires explicit permission for sensitive features:

| Permission | What It Enables | How to Grant |
|------------|-----------------|--------------|
| **Microphone** | Audio capture | Click "Allow" when prompted |
| **Screen Recording** | Visual capture | System Settings → Privacy → Screen Recording |
| **Accessibility** | Text extraction | System Settings → Privacy → Accessibility |
| **Calendar** | Meeting detection | System Settings → Privacy → Calendar |

> **Tip:** If a permission prompt doesn't appear, open System Settings → Privacy & Security and manually add noFriction Meetings.

#### Step 2: Configure Transcription

Choose your transcription provider:

| Provider | Quality | Speed | Cost |
|----------|---------|-------|------|
| **Deepgram** (Recommended) | Excellent | Real-time | Pay-per-use |
| **Gladia** | Very Good | Real-time | Pay-per-use |
| **Google STT** | Good | Real-time | Pay-per-use |

**To set up Deepgram:**
1. Go to [console.deepgram.com](https://console.deepgram.com)
2. Create an API key
3. Paste in Settings → Transcription → Deepgram API Key

#### Step 3: Configure AI (Optional)

For AI features like summaries, action items, and chat:

| Provider | Setup |
|----------|-------|
| **TheBrain Cloud** | Settings → AI → Login with credentials |
| **Local Ollama** | Install [Ollama](https://ollama.ai), models auto-detected |

---

## Recording Meetings

### Starting a Recording

**Method 1: Sidebar Button**
- Click the red **Record** button in the sidebar

**Method 2: Tray Menu**
- Click the tray icon → "Start Recording"

**Method 3: Keyboard Shortcut**
- Press `⌘ + Shift + R` (global)

**Method 4: Automatic Detection** (if enabled)
- noFriction detects when you open Zoom, Google Meet, or Teams
- A banner appears: "Meeting detected. Start recording?"

### During Recording

While recording, you'll see:
- **Red indicator** in the tray icon
- **Live transcript** scrolling in real-time
- **Timer** showing recording duration

**Available Actions:**
| Action | How |
|--------|-----|
| Pin a moment | Click "📌 Pin" to bookmark important points |
| View live transcript | Open Live Transcript panel |
| Pause (Always-On only) | Tray → Pause Capture |

### Stopping a Recording

- Click **Stop Recording** in sidebar
- Or tray icon → "Stop Recording"
- Or `⌘ + Shift + R` again

The recording is automatically:
1. Saved to your local database
2. Transcribed (if Deepgram configured)
3. Analyzed by VLM (if enabled)
4. Indexed for search

---

## Reviewing with Rewind

The **Rewind** feature lets you visually browse past meetings with synchronized screenshots and transcripts.

### Opening Rewind

1. Click **Rewind** in the sidebar
2. Select a meeting from the list

### Rewind Interface

```
┌─────────────────────────────────────────────────────────────┐
│  Visual Timeline (screenshots over time)                    │
│  ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐          │
│  │ 📷 │ │ 📷 │ │ 📷 │ │ 📷 │ │ 📷 │ │ 📷 │ │ 📷 │          │
│  └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘          │
├─────────────────────────────────────────────────────────────┤
│  ▶️ [══════════════●══════════] 12:34 / 45:00              │
├─────────────────────────────────────────────────────────────┤
│  Transcript (synced to current frame)                       │
│  [John]: "Let's discuss the Q1 numbers..."                  │
│  [Sarah]: "The revenue is up 15%."                          │
└─────────────────────────────────────────────────────────────┘
```

### Rewind Controls

| Control | Action |
|---------|--------|
| Click thumbnail | Jump to that point |
| Drag slider | Scrub through timeline |
| ← / → arrows | Move to previous/next frame |
| Click transcript line | Jump to when it was spoken |

### Exporting

- **Export Transcript**: Downloads as .txt or .srt
- **Export Video Clip**: Saves selected portion as video

---

## AI Chat & Intelligence

### Using AI Chat

Access the AI assistant:
1. Click **Chat** in the sidebar, OR
2. Click the brain icon (🧠) to open CopilotPanel

### RAG (Retrieval Augmented Generation)

When **"Use History"** is enabled (default):
- The AI searches your meeting history for relevant context
- Shows **context cards** with sources used
- Provides grounded answers based on your actual data

**Example:**
```
You: "What did we decide about the marketing budget?"

AI: Based on your January 15th meeting with Sarah, you decided 
    to allocate $500K to marketing for Q1, with a focus on 
    digital campaigns.
    
    📚 Sources:
    • Meeting 2026-01-15 (87% match)
    • Conversation 2026-01-10 (72% match)
```

### Quick Actions

| Button | What It Does |
|--------|--------------|
| **Summary** | Generates meeting summary |
| **Tasks** | Extracts action items |
| **History** | Searches past conversations |

### Model Selection

Choose from available models:
- **Qwen3 8B** - Fast, general purpose
- **Qwen3 14B** - Higher quality
- **Qwen3 VL** - Vision + language
- **Coder 7B** - Code-focused

---

## Knowledge Base Search

### Semantic Search

Search across all your meetings using natural language:

1. Open **Search** (🔍) or press `⌘ + K`
2. Type your query: "discussions about pricing strategy"
3. View ranked results with relevance scores

### Search Types

| Type | Example Query | Searches |
|------|---------------|----------|
| Keyword | "Q1 budget" | Exact phrase matches |
| Semantic | "money discussions" | Meaning-based |
| Entity | "@John" | Specific speakers |
| Date | "last week" | Time-based |

### Filtering Results

Filter by:
- Date range
- Meeting participants
- Topic/theme
- Content type (transcript, screenshot, insight)

---

## Settings & Configuration

Access Settings via sidebar or `⌘ + ,`

### Transcription Settings

| Setting | Description |
|---------|-------------|
| **Provider** | Deepgram, Gladia, or Google STT |
| **API Key** | Your provider's API key |
| **Language** | Primary language for transcription |
| **Speaker Diarization** | Identify who said what |

### Capture Settings

| Setting | Description |
|---------|-------------|
| **Capture Microphone** | Record mic audio |
| **Capture System Audio** | Record computer audio |
| **Capture Screen** | Take periodic screenshots |
| **Frame Interval** | Seconds between screenshots |
| **Always-On Mode** | Background ambient capture |

### AI Settings

| Setting | Description |
|---------|-------------|
| **TheBrain Login** | Cloud AI authentication |
| **Ollama Models** | Local model selection |
| **Auto-Analyze** | VLM processing on/off |
| **VLM Interval** | Processing frequency |

### Knowledge Base Settings

| Setting | Description |
|---------|-------------|
| **Pinecone** | Vector database for semantic search |
| **Supabase** | Cloud PostgreSQL for structured data |
| **Local Only** | Disable cloud sync |

### Privacy Settings

| Setting | Description |
|---------|-------------|
| **Privacy Filter** | Blur sensitive content |
| **Excluded Apps** | Apps not to capture |
| **Data Retention** | How long to keep recordings |

---

## Troubleshooting

### No Audio Captured

**Symptom:** Recording runs but no transcript appears

**Solutions:**
1. Check System Settings → Privacy & Security → Microphone
2. Verify audio device in Settings → Capture Settings
3. Test microphone with QuickTime Player

### No Screenshots

**Symptom:** Rewind shows no visual frames

**Solutions:**
1. Grant Screen Recording permission
2. Restart app after granting permission
3. Check "Capture Screen" is enabled in settings

### Transcription Not Working

**Symptom:** Audio captured but no text

**Solutions:**
1. Verify API key in Settings → Transcription
2. Check internet connection
3. View logs: Help → Diagnostic Logs

### AI Not Responding

**Symptom:** Chat shows no response or errors

**Solutions:**
1. Check TheBrain connection status (green dot)
2. Re-authenticate in Settings → AI
3. Try a different model

### Search Returns Nothing

**Symptom:** Semantic search finds no results

**Solutions:**
1. Verify Pinecone configured in Settings → Knowledge Base
2. Wait for initial indexing to complete
3. Check that meetings have been processed

### App Won't Launch

**Symptom:** App crashes or shows error on startup

**Solutions:**
1. Try launching from Terminal:
   ```bash
   /Applications/noFriction\ Meetings.app/Contents/MacOS/noFriction\ Meetings
   ```
2. Reset app: `rm -rf ~/Library/Application\ Support/com.nofriction.meetings`
3. Check macOS version (requires 12.0+)

---

## Keyboard Shortcuts

### Global (work anywhere)

| Shortcut | Action |
|----------|--------|
| `⌘ + Shift + R` | Start/stop recording |
| `⌘ + Shift + P` | Pin current moment |

### In-App

| Shortcut | Action |
|----------|--------|
| `⌘ + K` | Open command palette / search |
| `⌘ + ,` | Open settings |
| `⌘ + 1-9` | Switch sidebar tabs |
| `⌘ + N` | New prompt |
| `⌘ + Enter` | Send chat message |
| `Escape` | Close panel / cancel |

### Rewind

| Shortcut | Action |
|----------|--------|
| `Space` | Play / pause |
| `←` / `→` | Previous / next frame |
| `⌘ + ←` / `⌘ + →` | Jump 10 seconds |

---

## Privacy & Security

### Data Storage

| Data | Location | Encrypted |
|------|----------|-----------|
| Recordings | `~/Library/Application Support/com.nofriction.meetings/` | Yes (at rest) |
| Transcripts | Local SQLite | Yes |
| Settings | Local preferences | No |

### Cloud Sync (Optional)

If you enable cloud features:
- **Supabase**: PostgreSQL database (your instance)
- **Pinecone**: Vector embeddings only (no raw audio)
- **TheBrain**: API calls only, no data stored

### Data Deletion

To delete all data:
1. Settings → Storage → Clear All Data
2. Or manually: `rm -rf ~/Library/Application\ Support/com.nofriction.meetings`

### What We Don't Do

- ❌ We don't sell your data
- ❌ We don't train AI on your meetings
- ❌ We don't share with third parties
- ❌ Raw audio never leaves your device (except for transcription API)

---

## Getting Help

- **In-App Help**: Click Help in sidebar
- **Logs**: Help → Diagnostic Logs
- **Support**: support@nofriction.ai
- **Documentation**: docs.nofriction.ai

---

© 2026 noFriction AI. All rights reserved.
