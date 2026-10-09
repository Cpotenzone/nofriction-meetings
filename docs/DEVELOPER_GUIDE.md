# noFriction developer guide

How to build, run and test both apps. Architecture:
[ARCHITECTURE.md](ARCHITECTURE.md). Shipping: [LAUNCH_CHECKLIST.md](LAUNCH_CHECKLIST.md).

## Repository layout

```
src/                 Mac frontend (React 19 + TypeScript, Vite)
src-tauri/           Mac backend (Rust, Tauri 2)
  src/               modules (see ARCHITECTURE.md)
  swift/NoFrictionBridge/   StoreKit 2 + Apple Foundation Models (C ABI)
  tauri.conf.json    base config;  tauri.mas.conf.json  Mac App Store overlay
  entitlements.plist (DMG)  /  entitlements.mas.plist (App Store)
ios/                 iPhone/iPad app (SwiftUI, XcodeGen: ios/project.yml)
scripts/             release-macos.sh (DMG), release-mas.sh (App Store .pkg)
site/                static website: landing, support, privacy, terms
docs/                specs, guides, release docs
```

## Prerequisites

| Tool | Notes |
|---|---|
| macOS 12.3+ | the Mac app's minimum; macOS 26 to exercise Apple on-device AI |
| Xcode (current) + command line tools | Swift bridge, iOS app, signing |
| Rust (stable) | `rustup`; add `x86_64-apple-darwin` for `--universal` builds |
| Node.js 18+ | frontend |
| XcodeGen | `brew install xcodegen` (iOS project generation) |
| Ollama (optional) | local AI for testing without a cloud key |

## Mac app

```bash
npm install
npm run tauri dev               # dev build (Developer ID flavor, no Pro gating)
cd src-tauri && cargo test      # Rust unit tests
npx tsc --noEmit                # type check
```

To try the sandboxed App Store flavor locally, use `scripts/release-mas.sh --local-test`.
Purchases can't be tested from a dev run (StoreKit needs the signed,
sandboxed app). Use TestFlight; see [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md).

`npm run build` runs `increment-version.js` first (bumps the build number).

### API keys in development

There are no `.env` files and no keys in the repo. Paste keys in the running
app (Settings → AI). They're stored in the login Keychain under the service
`com.nofriction.meetings.ai`. Never hardcode a key, even as a fallback.

### Data and logs

`~/Library/Application Support/com.nofriction.meetings/`:
`nofriction_meetings.db` (SQLite), `models/` (Whisper), `frames/`, `logs/`,
`backups/`. The App Store build keeps the same layout inside its sandbox
container (`~/Library/Containers/com.nofriction.meetings/…`). To start from
a clean slate, quit the app and move that folder aside.

Migrations live in `database.rs` and run on one connection at startup.

### Adding a Tauri command

1. Write `#[tauri::command] pub async fn …` in the right `commands/*.rs`
   module.
2. Register it in `lib.rs` (`invoke_handler`).
3. Call it from TypeScript with `invoke("name", { … })` (camelCase args when
   the command uses `rename_all = "camelCase"`).
4. If it calls an LLM, go through `ai::client::complete` so guardrails and
   Pro gating apply, and wrap the UI call in `withAiConsent()`.
5. If the feature can't run in the sandbox, gate it with
   `#[cfg(not(feature = "mas"))]` and expose a capability flag in
   `build_info.rs`.

### Release

- Developer ID DMG: [RELEASE_RUNBOOK.md](RELEASE_RUNBOOK.md)
  (`scripts/release-macos.sh`).
- Mac App Store `.pkg` and upload: [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md)
  (`scripts/release-mas.sh`).

## iPhone / iPad app

```bash
cd ios
xcodegen generate
open NoFriction.xcodeproj
xcodebuild test -project NoFriction.xcodeproj -scheme NoFriction \
  -destination 'platform=iOS Simulator,name=<an installed iPhone simulator>' -only-testing:NoFrictionTests
```

- Debug runs use `NoFriction.storekit` for local purchases. Release builds
  use the real App Store; there's no Pro bypass.
- Launch argument `-NFSeedDemo` (debug builds) fills the app with sample
  meetings and people; `-NFResetDemo` clears them first. Use it for
  screenshots so no real data appears.
- Speech recognition doesn't run in the Simulator; test recording and
  transcription on a device.
- More: [ios/README.md](../ios/README.md).

## Conventions

- Specs first: change [AI_PROVIDERS.md](AI_PROVIDERS.md) or
  [REDACTION.md](REDACTION.md) in the same change as the code, and keep the
  two apps' behavior identical.
- User-facing claims (site, App Store text, in-app copy) must match the code.
  The vetted listing copy is in [APP_STORE_LISTING.md](APP_STORE_LISTING.md).
- No analytics or tracking SDKs. Adding any network destination changes the
  privacy policy (`site/privacy.html`) and the App Privacy answers.
- Record user-visible changes in [CHANGELOG.md](CHANGELOG.md).
