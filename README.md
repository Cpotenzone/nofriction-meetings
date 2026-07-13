# noFriction Meetings

**Version 3.4.0** — Your AI-powered meeting companion for macOS

---

## ✨ What's New in 3.4

- **Obsidian Vault Integration** — Bidirectional sync of meeting notes to your vault
- **Calendar Intelligence** — Auto-enriched meeting context from macOS Calendar
- **Data Chatbot (RAG)** — Ask questions across all your meeting history
- **Meeting Reports** — Custom prompt-driven report generation
- **Prompt Studio** — Create and tune prompts for AI analysis
- **Intel Dashboard** — Live meeting intelligence with sentiment and energy scoring


---

## Features

| Feature | Description |
|---------|-------------|
| **Live Transcription** | Real-time speech-to-text — on-device Whisper by default (fully offline), or cloud providers (Deepgram, Google Chirp 2, Gladia) |
| **Rewind** | Visual playback with synchronized screenshots and transcripts |
| **Deep Intel** | AI summaries, action items, and insights via local Ollama (or a remote endpoint) |
| **Knowledge Base** | Full-text search across all your past meetings, entirely local |
| **Offline by Default** | Capture, transcription, storage, search, and AI all run on your Mac — no account, no API key, no network required |

### Running fully offline

Out of the box the app records, transcribes (local Whisper, one-time 142 MB
model download), stores, and searches with zero cloud dependencies. For AI
chat/summaries/frame analysis, install [Ollama](https://ollama.com) and pull
a model:

```bash
brew install ollama
ollama pull qwen3:8b        # chat, summaries, insights
ollama pull qwen3-vl:8b     # optional: screenshot analysis
```

Cloud providers remain available as opt-in upgrades in Settings.

---

## Getting Started

1. **Install:** Download the `.dmg` and drag `noFriction Meetings` to Applications
2. **Permissions:** Grant Microphone, Screen Recording, and Accessibility
3. **Record:** Click "Record" in the sidebar to start capturing
4. **Review:** Use the **⏪ Rewind** tab to review with visual context

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Navigate timeline | ↑↓ or J/K |
| Open search | / |
| Clear | Esc |
| Sync scrolling | 🔗 button |

---

## Troubleshooting

- **No Audio:** System Settings → Privacy → Microphone
- **No Screenshots:** System Settings → Privacy → Screen Recording
- **Support:** support@nofriction.ai

---

## Development

### Prerequisites
- Rust (latest stable)
- Node.js (v18+)
- Xcode (for macOS build tools)

### Build
```bash
npm install
npm run tauri dev    # Run locally
npm run tauri build  # Build release DMG
```

---

© 2026 noFriction AI. All rights reserved.
