# noFriction Meetings - Developer Guide

## Development Environment Setup

This guide covers setting up a local development environment for noFriction Meetings, a Tauri desktop application for macOS.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Project Structure](#project-structure)
3. [Initial Setup](#initial-setup)
4. [Development Workflow](#development-workflow)
5. [Architecture Deep Dive](#architecture-deep-dive)
6. [Adding New Features](#adding-new-features)
7. [Testing](#testing)
8. [Debugging](#debugging)
9. [Building for Production](#building-for-production)
10. [Contributing Guidelines](#contributing-guidelines)

---

## Prerequisites

### Required Software

| Tool | Version | Installation |
|------|---------|--------------|
| **macOS** | 12.0+ | Required OS |
| **Xcode** | 14.0+ | Mac App Store |
| **Xcode CLI** | Latest | `xcode-select --install` |
| **Rust** | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` |
| **Node.js** | 18+ | `brew install node` or [nodejs.org](https://nodejs.org) |
| **npm** | 9+ | Included with Node.js |

### Optional Tools

| Tool | Purpose | Installation |
|------|---------|--------------|
| **cargo-watch** | Auto-rebuild on changes | `cargo install cargo-watch` |
| **Ollama** | Local AI models | [ollama.ai](https://ollama.ai) |
| **Supabase CLI** | Database migrations | `brew install supabase/tap/supabase` |

### Verify Installation

```bash
# Check all tools
rustc --version     # rustc 1.75.0 or higher
cargo --version     # cargo 1.75.0 or higher
node --version      # v18.0.0 or higher
npm --version       # 9.0.0 or higher
xcode-select -p     # /Applications/Xcode.app/Contents/Developer
```

---

## Project Structure

```
nofriction-meetings/
├── src/                          # Frontend (React + TypeScript)
│   ├── components/               # React components (43 files)
│   │   ├── AIChat.tsx           # AI chat with RAG
│   │   ├── CopilotPanel.tsx     # Side panel chat
│   │   ├── RewindGallery.tsx    # Visual timeline
│   │   ├── AdminConsole.tsx     # System management
│   │   ├── PromptLibrary.tsx    # Prompt editor
│   │   └── ...
│   ├── hooks/                    # React hooks
│   │   ├── useRecording.ts
│   │   └── useTranscripts.ts
│   ├── App.tsx                   # Main app layout
│   ├── App.css                   # Global styles
│   └── main.tsx                  # Entry point
│
├── src-tauri/                    # Backend (Rust)
│   ├── src/
│   │   ├── lib.rs               # App initialization, command registration
│   │   ├── main.rs              # Entry point
│   │   ├── commands.rs          # Tauri command handlers (206+ commands)
│   │   ├── database.rs          # SQLite operations
│   │   ├── capture_engine.rs    # Recording coordinator
│   │   ├── transcription/       # Transcription providers
│   │   ├── vlm_client.rs        # AI engine (local Ollama by default)
│   │   ├── pinecone_client.rs   # Vector database
│   │   └── ...                  # 45 modules total
│   ├── Cargo.toml               # Rust dependencies
│   ├── tauri.conf.json          # Tauri configuration
│   └── Info.plist               # macOS app metadata
│
├── supabase/                     # Database migrations
│   └── migrations/
│       └── 20260203_create_conversations_table.sql
│
├── docs/                         # Documentation
│   ├── ARCHITECTURE.md
│   ├── USER_GUIDE.md
│   ├── DEVELOPER_GUIDE.md       # This file
│   ├── RELEASE_RUNBOOK.md
│   └── CHANGELOG.md
│
├── package.json                  # Node.js dependencies
├── tsconfig.json                 # TypeScript config
├── vite.config.ts                # Vite bundler config
└── README.md
```

---

## Initial Setup

### 1. Clone the Repository

```bash
git clone https://github.com/nofriction/meetings.git
cd nofriction-meetings
```

### 2. Install Dependencies

```bash
# Install Node.js dependencies
npm install

# Rust dependencies are handled automatically by cargo
```

### 3. Configure Development Environment

Create a `.env.local` file (not committed to git):

```bash
# Optional: Development API keys
DEEPGRAM_API_KEY=your_dev_key

# Optional: Local services
SUPABASE_URL=http://localhost:54321
SUPABASE_KEY=your_local_key
```

### 4. Run Development Server

```bash
# Start development mode (hot reload for frontend + backend)
npm run tauri dev
```

This command:
1. Starts the Vite dev server (frontend) on `http://localhost:5173`
2. Compiles the Rust backend
3. Launches the Tauri app window
4. Enables hot reload for both frontend and backend

### 5. Grant Permissions

On first run, grant these macOS permissions:
- Microphone (for audio capture)
- Screen Recording (for visual capture)
- Accessibility (for text extraction)

---

## Development Workflow

### Frontend Development

```bash
# Run frontend only (no Tauri)
npm run dev
# Opens at http://localhost:5173

# Type checking
npx tsc --noEmit

# Lint (if configured)
npm run lint
```

#### Creating a New Component

```tsx
// src/components/MyComponent.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface MyComponentProps {
  meetingId: string;
}

export function MyComponent({ meetingId }: MyComponentProps) {
  const [data, setData] = useState<string[]>([]);

  useEffect(() => {
    // Call Rust backend
    invoke<string[]>('my_command', { meetingId })
      .then(setData)
      .catch(console.error);
  }, [meetingId]);

  return (
    <div className="my-component">
      {data.map((item, i) => (
        <div key={i}>{item}</div>
      ))}
    </div>
  );
}
```

### Backend Development

```bash
# Check Rust code without building
cd src-tauri
cargo check

# Run with detailed logging
RUST_LOG=debug npm run tauri dev

# Format code
cargo fmt

# Lint
cargo clippy
```

#### Creating a New Tauri Command

```rust
// src-tauri/src/commands.rs

#[tauri::command]
pub async fn my_command(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db = state.database.read();
    
    let results = db
        .get_something(&meeting_id)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(results)
}
```

Register the command in `lib.rs`:

```rust
// src-tauri/src/lib.rs (in the .invoke_handler section)
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::my_command,  // Add new command
])
```

### Database Development

Local SQLite schema is in `src-tauri/src/database.rs`.

For Supabase migrations:

```bash
# Create new migration
supabase migration new my_migration

# Apply locally
supabase db push

# Apply to production
supabase db push --db-url "postgres://..."
```

---

## Architecture Deep Dive

### Module Categories

#### Core System
| Module | Purpose |
|--------|---------|
| `lib.rs` | App state initialization |
| `commands.rs` | Frontend → Backend IPC |
| `database.rs` | SQLite CRUD |
| `settings.rs` | User preferences |

#### Capture Pipeline
| Module | Purpose |
|--------|---------|
| `capture_engine.rs` | Recording orchestration |
| `video_recorder.rs` | Screen recording |
| `frame_extractor.rs` | Screenshot capture |
| `ambient_capture.rs` | Background mode |
| `power_manager.rs` | Battery optimization |

#### Transcription
| Module | Purpose |
|--------|---------|
| `transcription/` | Multi-provider support |
| `deepgram_client.rs` | WebSocket streaming |

#### AI/ML
| Module | Purpose |
|--------|---------|
| `ai_client.rs` | Local Ollama |
| `vlm_client.rs` | AI engine (Ollama-compatible) |
| `vlm_scheduler.rs` | Batch processing |
| `pinecone_client.rs` | Vector search |
| `prompt_manager.rs` | Prompt templates |

#### Intelligence
| Module | Purpose |
|--------|---------|
| `semantic_classifier.rs` | Content classification |
| `timeline_builder.rs` | Event aggregation |
| `episode_builder.rs` | Session chunking |
| `meeting_intel.rs` | Live insights |

### Data Flow

```
User Action
    │
    ▼
Frontend (React)
    │ invoke()
    ▼
Tauri IPC
    │
    ▼
Command Handler (Rust)
    │
    ├─► Database (SQLite)
    │
    ├─► AI engine (local Whisper/Ollama; optional cloud providers)
    │
    └─► State Manager
```

### State Management

```rust
// AppState holds all shared state
pub struct AppState {
    pub database: Arc<RwLock<DatabaseManager>>,
    pub capture_engine: Arc<RwLock<CaptureEngine>>,
    pub transcription_manager: Arc<RwLock<TranscriptionManager>>,
    pub vlm_client: Arc<RwLock<VLMClient>>,
    pub pinecone_client: Arc<RwLock<PineconeClient>>,
    pub supabase_client: Arc<RwLock<SupabaseClient>>,
    // ...
}
```

---

## Adding New Features

### Feature Checklist

When adding a new feature:

- [ ] Create Rust command in `commands.rs`
- [ ] Register in `lib.rs` invoke_handler
- [ ] Create React component in `src/components/`
- [ ] Add styles to `App.css`
- [ ] Update types if needed
- [ ] Add tests
- [ ] Update documentation

### Example: Adding a New AI Feature

**1. Backend Command**

```rust
// commands.rs
#[derive(serde::Serialize)]
pub struct AnalysisResult {
    summary: String,
    confidence: f32,
}

#[tauri::command]
pub async fn analyze_topic(
    meeting_id: String,
    topic: String,
    state: State<'_, AppState>,
) -> Result<AnalysisResult, String> {
    // Implementation
}
```

**2. Register Command**

```rust
// lib.rs
commands::analyze_topic,
```

**3. Frontend Component**

```tsx
// TopicAnalyzer.tsx
const result = await invoke<AnalysisResult>('analyze_topic', {
  meetingId,
  topic,
});
```

**4. Styles**

```css
/* App.css */
.topic-analyzer {
  /* styles */
}
```

---

## Testing

### Unit Tests (Rust)

```bash
cd src-tauri
cargo test
```

```rust
// Example test
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_transcript_search() {
        let db = DatabaseManager::new_in_memory().await.unwrap();
        // Test implementation
    }
}
```

### Integration Tests

```bash
# Run with test database
DATABASE_URL=":memory:" cargo test --features integration
```

### Frontend Tests

```bash
# If test framework configured
npm test
```

---

## Debugging

### Rust Logging

```rust
// Enable in code
log::info!("Processing meeting: {}", meeting_id);
log::debug!("Frame count: {}", frames.len());
log::error!("Failed to capture: {}", e);
```

```bash
# Run with debug logs
RUST_LOG=debug npm run tauri dev

# Specific module
RUST_LOG=nofriction_meetings::capture_engine=trace npm run tauri dev
```

### Frontend Debugging

1. Open DevTools: `⌘ + Option + I`
2. Console shows JavaScript logs
3. Network tab shows API calls

### Common Issues

| Issue | Debug Approach |
|-------|----------------|
| Command not found | Check registration in `lib.rs` |
| Type mismatch | Verify Rust/TS types match |
| Permission denied | Check macOS permissions |
| Database error | Check `~/Library/Application Support/com.nofriction.meetings/db.sqlite` |

---

## Building for Production

### Development Build

```bash
npm run tauri build -- --debug
```

### Release Build

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/macos/noFriction Meetings.app`

### Signed Release

See [RELEASE_RUNBOOK.md](./RELEASE_RUNBOOK.md) for:
- Code signing with Developer ID
- Notarization
- DMG creation

---

## Contributing Guidelines

### Branch Strategy

```
main          ← Production releases
  │
  └── develop ← Integration branch
        │
        └── feature/my-feature ← Feature branches
```

### Commit Messages

```
type(scope): description

feat(chat): add RAG context display
fix(capture): resolve memory leak in frame extraction
docs(readme): update installation instructions
refactor(vlm): simplify authentication flow
```

### Pull Request Process

1. Fork the repository
2. Create feature branch from `develop`
3. Make changes with tests
4. Update documentation
5. Submit PR with description
6. Address review feedback

### Code Style

**Rust:**
- Run `cargo fmt` before committing
- Address all `cargo clippy` warnings
- Document public functions

**TypeScript:**
- Use functional components
- Type all props and state
- Use `invoke` for backend calls

---

## Resources

### Internal
- [ARCHITECTURE.md](./ARCHITECTURE.md) - System overview
- [USER_GUIDE.md](./USER_GUIDE.md) - End-user documentation
- [RELEASE_RUNBOOK.md](./RELEASE_RUNBOOK.md) - Release procedures

### External
- [Tauri Documentation](https://tauri.app/v1/guides/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [React Documentation](https://react.dev/)
- [Deepgram API](https://developers.deepgram.com/)

---

## Quick Reference

### Common Commands

```bash
# Development
npm run tauri dev              # Start dev server
cargo check                    # Check Rust code
cargo fmt                      # Format Rust
cargo clippy                   # Lint Rust

# Testing
cargo test                     # Rust tests
npm test                       # JS tests (if configured)

# Building
npm run tauri build            # Production build
npm run tauri build -- --debug # Debug build

# Database
sqlite3 ~/Library/Application\ Support/com.nofriction.meetings/db.sqlite
```

### File Locations

| Item | Path |
|------|------|
| App data | `~/Library/Application Support/com.nofriction.meetings/` |
| Database | `~/Library/Application Support/com.nofriction.meetings/db.sqlite` |
| Logs | `~/Library/Logs/com.nofriction.meetings/` |
| Preferences | `~/Library/Preferences/com.nofriction.meetings.plist` |

---

Happy coding! 🎉
