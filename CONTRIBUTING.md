# Contributing to noFriction Meetings

Thank you for your interest in contributing to noFriction Meetings!

## Prerequisites

- **Rust** (latest stable) — [Install via rustup](https://rustup.rs/)
- **Node.js** v18+ — [Download](https://nodejs.org/)
- **Xcode Command Line Tools** — `xcode-select --install`
- **macOS 12.3+** (required for build)

## Getting Started

```bash
# 1. Clone the repository
git clone https://github.com/nofriction/nofriction-meetings.git
cd nofriction-meetings

# 2. Install Node dependencies
npm install

# 3. Copy environment config
cp .env.example .env
# Edit .env with your API keys (at minimum, DEEPGRAM_API_KEY)

# 4. Run in development mode
npm run tauri dev
```

> **Note:** First build will take 5-10 minutes for Rust compilation. Subsequent builds are fast (incremental).

## macOS Permissions

When running for the first time, macOS will prompt for:
- **Microphone** — Required for audio recording
- **Screen Recording** — Required for screen capture
- **Accessibility** — Required for text extraction
- **Calendar** — Optional, enables meeting detection

## Project Structure

```
nofriction-meetings/
├── src/                      # Frontend (React + TypeScript)
│   ├── components/           # UI components
│   │   └── agency/           # Main layout system
│   │       └── views/        # Full-page views
│   ├── features/             # Feature-specific modules
│   ├── hooks/                # React hooks
│   └── lib/                  # Utilities (tauri.ts)
├── src-tauri/                # Backend (Rust + Tauri)
│   └── src/
│       ├── lib.rs            # App initialization & state
│       ├── commands.rs       # Tauri command handlers
│       ├── database.rs       # SQLite operations
│       ├── transcription/    # Multi-provider transcription
│       └── ...               # 50+ domain modules
├── docs/                     # Documentation
├── DESIGN.md                 # Design system & tokens
└── CLAUDE.md                 # AI assistant guidelines
```

## Development Workflow

### Frontend Changes
```bash
# Vite hot-reloads automatically during `npm run tauri dev`
# Edit files in src/ and see changes instantly
```

### Backend Changes
```bash
# Rust recompiles on save during `npm run tauri dev`
# Edit files in src-tauri/src/ — Tauri watches for changes
```

### Design System
All new CSS should use design system tokens prefixed with `--ds-`. See `DESIGN.md` for the full token reference. Existing code uses legacy tokens (`--hazard-yellow`, `--accent-purple`, etc.) which will be migrated incrementally.

## Code Style

### Rust
- Use `cargo fmt` before committing
- Use `cargo clippy` to check for warnings
- All new Tauri commands go in domain-specific files (e.g., `commands_vault.rs`), not in the main `commands.rs`

### TypeScript/React
- Use TypeScript strict mode
- Component files use PascalCase (e.g., `MeetingHistory.tsx`)
- Hook files use camelCase with `use` prefix (e.g., `useRecording.ts`)
- CSS files match their component name (e.g., `IntelDashboard.css`)

## Security Rules

1. **Never commit `.env` files** — They contain API keys
2. **Never hardcode API keys** — Use environment variables or the settings system
3. **All user input must be validated** before passing to SQL or external APIs

## Building for Release

```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/dmg/noFriction Meetings.dmg
```
