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

# 3. Run in development mode
npm run tauri dev
```

No `.env` file and no API keys are needed to build or run. Transcription runs
on-device (Whisper; the setup assistant downloads the model on first run). AI
features use Apple's on-device model (macOS 26+) or one endpoint you enter in
**Settings → AI Engine** (an OpenAI-compatible URL, model and optional key,
e.g. a local Ollama or LM Studio server). There are no built-in services or
supplied keys. Keys are stored in the macOS Keychain, never in SQLite, files
or the repo.
See `docs/AI_PROVIDERS.md`.

> **Note:** First build will take 5-10 minutes for Rust compilation. Subsequent builds are fast (incremental).

## macOS Permissions

When running for the first time, the setup assistant walks through:
- **Microphone** — required to transcribe what you say
- **Screen & System Audio Recording** — call audio and screenshots
- **Calendar** — optional; names recordings after calendar events
- **Notifications** — asked at the first recording (meeting-end countdown)
- **Accessibility** — optional, Developer ID build only (text capture)

Re-run it any time from Settings → General → Run Setup Assistant.

## Build flavors

- Default (Developer ID DMG): `npm run tauri build`
- Mac App Store (sandboxed, StoreKit): `cargo build --features mas` /
  see `docs/MAC_APP_STORE_BUILD.md`

Both must compile with zero warnings: `cargo check` and
`cargo check --features mas` in `src-tauri/`. Run the tests with
`cargo test --lib` (both flavors).

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
│       ├── lib.rs            # App initialization, state, command registry
│       ├── commands/         # Tauri command handlers (by domain)
│       ├── ai/               # Bring-your-own-key AI providers (Keychain)
│       ├── database.rs       # SQLite operations + migrations
│       ├── transcription/    # Local Whisper (on-device only)
│       ├── redaction.rs      # Delete / "Strike from the record"
│       └── ...               # Domain modules
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
- New Tauri commands go in a domain file under `src-tauri/src/commands/` (e.g. `commands/vault.rs`) and must be registered in `lib.rs`; every frontend `invoke()` must name a registered command
- Database migrations stay on the single connection inside `run_migrations`

### TypeScript/React
- Use TypeScript strict mode
- Component files use PascalCase (e.g., `MeetingHistory.tsx`)
- Hook files use camelCase with `use` prefix (e.g., `useRecording.ts`)
- CSS files match their component name (e.g., `IntelDashboard.css`)

## Security Rules

1. **Never hardcode API keys** and never add a key as a fallback — keys live in the macOS Keychain (`src-tauri/src/secrets.rs`)
2. **Never log transcript text or keys**
3. **Never commit `.env` files or secrets** — the app reads no `.env`; a legacy `~/.nofriction-meetings/.env` is migrated into the Keychain and deleted on startup
4. **All user input must be validated** before passing to SQL or external APIs

## Building for Release

```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/dmg/noFriction Meetings.dmg
```
