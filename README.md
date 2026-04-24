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
| 🎤 **Live Transcription** | Real-time speech-to-text for all your meetings |
| ⏪ **Rewind** | Visual playback with synchronized screenshots and audio |
| 🧠 **Deep Intel** | AI-generated summaries, action items, and insights |
| 🔍 **Knowledge Base** | Search across all your past meetings instantly |
| 🔒 **Privacy First** | All processing happens locally on your Mac |

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
