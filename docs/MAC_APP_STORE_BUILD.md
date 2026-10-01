# Mac App Store build (`mas` flavor)

The Mac app ships in two flavors from one codebase:

| | Developer ID (DMG) | Mac App Store (`mas`) |
|---|---|---|
| Build | `scripts/release-macos.sh` | `scripts/release-mas.sh` |
| Cargo feature | none (default) | `--features mas` |
| Tauri config | `tauri.conf.json` | `+ tauri.mas.conf.json` (merged with `--config`) |
| Entitlements | `entitlements.plist` (hardened runtime, no sandbox) | `entitlements.mas.plist` (App Sandbox) |
| Signing | Developer ID Application, notarized DMG | Apple Distribution + embedded provisioning profile, `.pkg` signed with 3rd Party Mac Developer Installer |
| Data folder | `~/Library/Application Support/com.nofriction.meetings` | `~/Library/Containers/com.nofriction.meetings/Data/Library/Application Support/com.nofriction.meetings` |
| AI features | always on (owner's build) | need **noFriction Pro** (StoreKit 2) |
| Screen video (ffmpeg) | yes | no; screenshots still feed the timeline |
| Accessibility text capture | yes | no |
| Ingest / Supabase / Pinecone / Admin console | yes (Settings → AI Engine → Advanced, Admin Console) | hidden |

The UI asks the backend which flavor it is (`get_build_capabilities`, see
`src-tauri/src/build_info.rs` and `src/lib/build.ts`) and hides what the
build doesn't have.

Bundle id for both: **`com.nofriction.meetings`** (same as iOS, for
Universal Purchase).

---

## Building

```bash
# Release .pkg (needs src-tauri/embedded.provisionprofile)
scripts/release-mas.sh

# ... and upload to App Store Connect / TestFlight
ASC_APP_ID=1234567890 APPLE_ID=you@example.com scripts/release-mas.sh --upload

# Local sandbox test: no profile, signed with your Apple Development identity,
# not packaged. Launch from the build folder, never /Applications.
scripts/release-mas.sh --local-test
open "src-tauri/target/release/bundle/macos/noFriction Meetings.app"
```

Or by hand:

```bash
npx tauri build --bundles app --features mas --config src-tauri/tauri.mas.conf.json
```

What the script does:
1. Checks the identities and the provisioning profile (right app id, a
   distribution profile, not expired).
2. Bumps `src-tauri/build_number.txt` and sets it as `CFBundleVersion`
   (every upload needs a new, higher build number; `CFBundleShortVersionString`
   is `version` in `tauri.conf.json`). `--local-test` doesn't bump it.
   Override with `MAS_BUILD_NUMBER=…`.
3. Builds with `--features mas` and the MAS config overlay (`app` bundle
   only; the frontend is built with `npx tsc && npx vite build`, so the DMG
   build-number bump in `npm run build` doesn't run).
4. Copies the profile to `Contents/embedded.provisionprofile` and signs with
   `entitlements.mas.plist`.
5. Verifies: `codesign --verify --deep --strict`; `codesign -d --entitlements -`
   has `app-sandbox` and `network.client` and none of the Developer ID
   exceptions; `Info.plist` has the bundle id, build number,
   `ITSAppUsesNonExemptEncryption=false` and the category; the binary has no
   `ffmpeg`/`ffprobe`/`osascript`/`ioreg`/shell-plugin strings and doesn't
   link the AX API; FoundationModels is weak-linked.
6. `productbuild --component … /Applications --sign "3rd Party Mac Developer Installer: …"`,
   then `pkgutil --check-signature`.
7. `--upload`: `xcrun altool --upload-package` with `ASC_APP_ID` and either
   `APPLE_ID` + an app-specific password in the keychain item `AC_PASSWORD`
   (`xcrun altool --store-password-in-keychain-item AC_PASSWORD -u "$APPLE_ID" -p <password>`)
   or an App Store Connect API key (`APPLE_API_KEY_ID`, `APPLE_API_ISSUER`).
   Nothing is hardcoded; it fails if they're missing. You can also drag the
   `.pkg` into Apple's Transporter app.

`--universal` builds arm64 + x86_64 (`--target universal-apple-darwin`;
needs `rustup target add x86_64-apple-darwin`). The default is this Mac's
architecture, which the App Store accepts because the minimum macOS is 12.3.

---

## One-time setup in the Apple portal (owner)

1. **App ID** `com.nofriction.meetings` (explicit), platforms iOS + macOS,
   capability **In-App Purchase** only (docs/APP_STORE_RELEASE.md §5.2).
2. **Profile**: developer.apple.com → Profiles → + → **Mac App Store Connect**
   → App ID `com.nofriction.meetings` → certificate *Apple Distribution:
   casey potenzone (C7GCEESE2V)*. Download it and save it as
   **`src-tauri/embedded.provisionprofile`** (gitignored; `*.provisionprofile`).
   Renew it yearly.
3. Certificates already in the keychain: *Apple Distribution* and *3rd Party
   Mac Developer Installer* (both team C7GCEESE2V).
4. App Store Connect: add **macOS** to the app record, create the
   subscription group **noFriction Pro** with
   `com.nofriction.meetings.pro.monthly` and `com.nofriction.meetings.pro.yearly`
   (+ an introductory free trial), and sign the Paid Apps agreement.

---

## Sandbox blockers (docs/APP_STORE_RELEASE.md §4) and how they're handled

| # | Resolution |
|---|---|
| m1 ffmpeg video / frame extraction | **Gated** out of `mas` (`video_recorder`, `frame_extractor`, `chunk_manager` and their commands are `#[cfg(not(feature = "mas"))]`). Screenshots from `capture_engine` (xcap) still feed frames and the timeline. The UI skips video start/stop and the video-storage cleanup. |
| m2 Accessibility capture | **Gated**: the AX API calls in `accessibility_extractor.rs` compile only without `mas`; stubs report "not trusted" and never prompt. Permission row and the chat's screen-text button are hidden. |
| m3 `osascript` window title | **Fixed (both)**: `privacy_filter.rs` reads the front window title with `CGWindowListCopyWindowInfo` (needs Screen Recording, which capture already needs). No Apple Events. |
| m4 `sh -c ioreg` | **Fixed (both)**: `core_audio.rs` checks `kAudioDevicePropertyDeviceIsRunningSomewhere` on devices with input streams. |
| m5 `.env` / dotenv | **Gated**: `EnvConfig::load()` returns defaults and `secrets::migrate_home_env()` doesn't exist in `mas`. |
| m6 Obsidian vault path | **Fixed (both)**: `set_vault_path` stores an app-scoped security bookmark (`bookmarks.rs`, setting `obsidian_vault_bookmark`); startup resolves it and calls `startAccessingSecurityScopedResource`. The DMG build falls back to the plain path. Entitlements: `files.user-selected.read-write` + `files.bookmarks.app-scope`. Existing DMG installs get the bookmark the next time the vault folder is picked. |
| m7 shell plugin | **Removed (both)**: `tauri-plugin-shell` dropped from Cargo, `lib.rs`, `capabilities/default.json`. System Settings links open through the opener plugin (NSWorkspace) instead of `/usr/bin/open`. |
| m8 entitlements | `entitlements.mas.plist`: app-sandbox, application-identifier, team-identifier, network.client, device.audio-input, personal-information.calendars, files.user-selected.read-write, files.bookmarks.app-scope, files.downloads.read-write. No JIT / unsigned memory / library-validation exceptions (WKWebView JIT runs out of process; verified by a sandboxed launch). |
| m9 identifier | `tauri.conf.json` identifier is `com.nofriction.meetings`. The DMG build moves `~/Library/Application Support/ai.nofriction.meetings` to `…/com.nofriction.meetings` once at startup (`paths.rs`; only if the new folder is missing or empty; never deletes; logs the result). |
| m10 Info.plist | `ITSAppUsesNonExemptEncryption=false`, `LSApplicationCategoryType=public.app-category.productivity`. Hardcoded `CFBundleVersion` removed so each upload can set its own build number. |
| m11 StoreKit | Swift bridge (below). |
| m12 transcription default | **Fixed (both)**: new installs default to local Whisper, and a saved cloud provider with no key falls back to local Whisper at startup. The model downloads from Hugging Face into `<app data>/models` (needs `network.client`). |
| m13 keys in SQLite | Done earlier (Keychain). The `mas` build uses the data-protection keychain (no `keychain-access-groups` needed; the default group is the app's application-identifier from the profile). A profile-less local test build falls back to the login keychain. |
| m14 owner infra | Hidden in `mas`: Admin Console tab, Supabase/Pinecone settings. (The ingest settings screen isn't mounted in either flavor.) |
| m15 release script | `scripts/release-mas.sh` (above). |

## StoreKit and the Apple on-device model (Swift bridge)

`src-tauri/swift/NoFrictionBridge` is a Swift package with a plain C ABI
(`@_cdecl`). `build.rs` compiles it with `swiftc` for the target
architecture and links it statically:

- `AppleModel.swift`: Foundation Models (`SystemLanguageModel`,
  `LanguageModelSession`). Built into **both** flavors; the framework is
  weak-linked so the app launches on macOS 12–15. Wired into the AI layer as
  the `apple` preset and the key-less fallback (docs/AI_PROVIDERS.md).
- `Store.swift`: StoreKit 2, compiled only for `mas` (`-DNF_STOREKIT`):
  products (display name, price, period, intro offer if eligible),
  purchase with on-device verification, current entitlement
  (`Transaction.currentEntitlements`: isPro, productId, expiration,
  willRenew), restore (`AppStore.sync()`), and a `Transaction.updates`
  listener started at launch that emits the `subscription-changed` event.

We call the Swift code through a small C ABI instead of `swift-rs`'s
`SRString`/`SRObject` types: `swift-rs` builds the package for the host
architecture only (breaks `--universal`) and needs its Swift runtime package
fetched from GitHub at build time. The plain C ABI has neither problem.

**Pro gating** (`mas` only): `entitlement::require_pro()` in
`ai::client::complete`, which every LLM call goes through, returns
`PRO_REQUIRED: …`. `withAiConsent()` in the frontend shows the paywall
(`PaywallModal.tsx`: price, period, trial text, Restore Purchases, Terms =
Apple standard EULA, Privacy = https://nofriction.ai/privacy placeholder)
and retries after a purchase. Settings → Subscription shows the status.
Background jobs (live intel, screenshot analysis) just log the error.
The DMG build has no gating (compiled out).

**Testing purchases**: a `.storekit` configuration only applies when an app
is launched from an Xcode scheme, which doesn't fit a Tauri binary, so none
is included. Test with the App Store sandbox through **TestFlight**
(purchases are free and renew on an accelerated clock), or run a build
signed with a development profile and sign in with a Sandbox Apple ID
(System Settings → App Store → Sandbox Account).

## Things to know

- **Data doesn't carry over** from the DMG build to the App Store build:
  the sandboxed app starts with an empty container. (Moving the SQLite DB,
  `models/` and `frames/` into the container by hand works while the app is
  quit.) An in-app export/import is not built yet.
- **Owner's DMG build after this change**: the bundle id changed, so macOS
  treats it as a new app. Quit the old app before the first launch (the data
  folder is moved at startup), then re-grant Microphone, Screen Recording,
  Accessibility and Calendar in System Settings, and allow Keychain access
  once when asked.
- On a Mac with both builds, each has its own data (the container vs the
  Application Support folder) and its own keys.
