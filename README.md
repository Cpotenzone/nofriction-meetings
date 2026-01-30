# noFriction Meetings

noFriction Meetings is a professional AI-powered meeting companion for macOS. It records, transcribes, and analyzes your meetings in real-time, providing searchable transcripts, visual timelines with synchronized screenshots, and deep insights.

## Features

- **Live Transcription:** Real-time speech-to-text for all your meetings.
- **Rewind:** Visual playback with synchronized screenshots and audio using "stateful capture" technology.
- **Deep Intel:** AI-generated summaries, action items, and insights powered by local and cloud LLMs.
- **Knowledge Base:** Search across all your past meetings instantly.
- **Privacy First:** All recording and processing happens locally or via secure, private connections.

## Getting Started

1. **Install:** Download the `.dmg` and drag `noFriction Meetings` to your Applications folder.
2. **Permissions:** On first launch, grant Microphone, Screen Recording, and Accessibility permissions.
3. **Record:** Click "Record" in the sidebar or tray menu to start capturing.
4. **Review:** Use the **Rewind** tab to review past meetings with visual context.

## Troubleshooting

- **No Audio:** Check System Settings > Privacy & Security > Microphone.
- **No Screenshots:** Check System Settings > Privacy & Security > Screen Recording.
- **Support:** Contact `support@nofriction.ai` or check the in-app Help section.

## Development

### Prerequisites

- Rust (latest stable)
- Node.js (v18+)
- Xcode (for macOS build tools)

### Build

```bash
npm install
npm run tauri dev   # Run locally
npm run tauri build # Build release DMG
```

---

© 2026 noFriction AI. All rights reserved.
