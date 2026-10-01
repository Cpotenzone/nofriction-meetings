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
| **Live Transcription** | Words appear as they're spoken (live partials every ~0.8s, finalized on each pause) — on-device Whisper large-v3-turbo by default, fully offline. Cloud providers (Deepgram, Google Chirp 2, Gladia) optional |
| **Choose What's Captured** | Pick any displays or individual windows from a thumbnail picker (Live → Change). Each source is deduplicated independently, so a screenshot is saved only when that source changes |
| **Snap** | One click (or ⌘⇧S) saves full-resolution snapshots of the chosen sources into the meeting timeline |
| **Rewind** | Visual playback with synchronized screenshots and transcripts |
| **Deep Intel** | AI summaries, action items, emails and insights from **your own AI provider**. Paste an API key for OpenAI (default), Anthropic, Google Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity or Together, or point it at a local model (Ollama, LM Studio, any OpenAI-compatible URL) |
| **Knowledge Base** | Full-text search across all your past meetings, entirely local |
| **Private by Default** | Capture, transcription, storage and search run on your Mac. No account, and no noFriction servers. AI features send text only to the provider you choose (asking first), using your key, which is kept in the Keychain |

### Running fully offline

Out of the box the app records, transcribes (local Whisper, one-time 547 MB
model download), stores, and searches with zero cloud dependencies. For AI
without any cloud provider, install [Ollama](https://ollama.com), pull a model,
and choose **Ollama (local)** in Settings → AI Engine:

```bash
brew install ollama
ollama pull qwen3:8b        # chat, summaries, insights
ollama pull qwen3-vl:8b     # optional: screenshot analysis
```

Or paste an API key from any supported provider in Settings → AI Engine. The app
recognizes the provider from the key and checks that it works. See
[docs/AI_PROVIDERS.md](docs/AI_PROVIDERS.md).

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
| Snap chosen screens/windows | ⌘⇧S |

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
