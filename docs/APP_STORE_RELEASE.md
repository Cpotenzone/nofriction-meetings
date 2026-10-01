# noFriction: shipping through the App Store (TestFlight first)

Goal: distribute the iPhone/iPad app and the Mac app through Apple, start on
TestFlight (free), and handle licensing entirely with StoreKit. We run **no
servers**: no accounts, no license server, no hosted AI. Users bring their own
AI provider; OpenAI is the default choice.

## Status (2026-09-28)

**Code: done for TestFlight on both platforms.** §2–§4 below are the original
gap list, kept for reference.

- **Decisions** made: bundle ID `com.nofriction.meetings` on both; subscription
  (monthly + yearly, trial set in App Store Connect); iOS first, Mac next.
- **Bring-your-own-key AI** (both apps), per [AI_PROVIDERS.md](AI_PROVIDERS.md):
  - paste a key and the provider is detected and validated
  - OpenAI is the default
  - also supported: Anthropic, Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity, Together, Ollama, LM Studio, custom endpoints
  - Apple on-device model as the key-less fallback
  - keys are stored in the Keychain
  - consent is asked per provider
  - the private Castle host is removed
- **iOS**:
  - privacy manifest
  - Settings tab
  - StoreKit 2 paywall, restore, and Pro gating
  - recording-consent notice
  - local-network access limited to the right hosts
  - 41 unit tests pass, and the Release build passes
- **Mac App Store flavor** (`--features mas`, `tauri.mas.conf.json`,
  `scripts/release-mas.sh`; see [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md)):
  - m1–m15 resolved
  - sandboxed launch verified
  - StoreKit plus the Foundation Models Swift bridge
  - the DMG flavor still builds
- **Fixed on the way:** a fresh-install database race ("no such table" after
  migrations). Migrations now run on one connection.
- **Static pages** for Privacy and Support are in `site/`.

**Only you can do these (portal / accounts):**
1. §5.1: Paid Apps agreement, tax, banking, DSA trader status, Small Business Program.
2. §5.2–5.3: App ID `com.nofriction.meetings` (iOS + macOS) and the app record.
3. §5.4: subscription group "noFriction Pro" with `…pro.monthly` / `…pro.yearly` and the trial.
4. Host `site/` (GitHub Pages or your domain). Update the placeholder
   `https://nofriction.ai/privacy` in `ios/NoFriction/App/AppLinks.swift` and
   `src/lib/build.ts` (`PRIVACY_URL`) if the URL differs. Make sure
   `support@nofriction.ai` receives mail.
5. Mac: create a Mac App Store Connect provisioning profile and save it as
   `src-tauri/embedded.provisionprofile`. Store the upload password with
   `xcrun altool --store-password-in-keychain-item AC_PASSWORD …`.
6. On-device checks: iPhone recording and transcription on real hardware; a
   purchase, restore and trial via TestFlight sandbox; one real call per
   provider with your own keys.

---

## 1. Decisions to make before anything else

These are hard or impossible to change later.

| # | Decision | Recommendation | Why it matters |
|---|---|---|---|
| D1 | **Bundle ID** for both apps | Use **`com.nofriction.meetings`** for iOS *and* Mac | A bundle ID can't be changed once the App Store Connect record exists. Using the *same* ID on both platforms enables **Universal Purchase**: one purchase or subscription unlocks both apps. Today iOS is `com.nofriction.meetings.mobile`, the Mac `Info.plist` says `com.nofriction.meetings`, and `tauri.conf.json` says `ai.nofriction.meetings`. Three different IDs. |
| D2 | **Business model** | Free download + auto-renewing subscription ("noFriction Pro", monthly + yearly) with a free trial, *or* a one-time "Lifetime" unlock | With bring-your-own-key AI there's no per-user cost to us, so a one-time purchase is viable. Both are verified on-device with StoreKit 2. No server needed either way. |
| D3 | **Does the Mac app go through the Mac App Store?** | Yes, but as phase 2, after iOS is on TestFlight | "License through the App Store" means the Mac app must be distributed by Apple. The Mac App Store requires App Sandbox, and today's Mac app breaks several sandbox rules (§4). The existing Developer ID DMG can't check App Store purchases. |
| D4 | **What "OpenAI by default" means** | OpenAI is preselected. The user pastes their own key on first run. With no key, fall back to Apple's on-device model where available. | With no server, any key we ship inside the app can be extracted and billed to us. Apple's on-device Foundation Models (iOS/macOS 26+) give key-less AI out of the box. |
| D5 | **App name** | Check availability in App Store Connect ("noFriction" may be taken) | Names are unique across the store. Reserve it by creating the record (§5 step 4). |

---

## 2. AI providers ("point to any model"): what to build

**Current state:** there is no OpenAI path at all.
- Text AI goes Castle Chat → local Ollama. Castle's URL is my private tailnet host, hardcoded in `ai_client.rs:149` and `CastleChat.swift:16`.
- Vision goes to Ollama only.
- The "Remote / OpenAI" settings UI (`AISettings.tsx`) is never rendered, and nothing reads the values it saves.
- All API keys are stored as plain text in SQLite and `~/.nofriction-meetings/.env`.

**Target:** one provider layer, the same shape on iOS and Mac.

| Provider preset | Protocol | Notes |
|---|---|---|
| **OpenAI** (default) | `POST {base}/v1/chat/completions`, `GET /v1/models` | Base URL `https://api.openai.com/v1`; user's key |
| Anthropic | Messages API (`/v1/messages`) | Needs its own adapter |
| Google Gemini | OpenAI-compatible endpoint (`generativelanguage.googleapis.com/v1beta/openai`) | Reuses the OpenAI adapter |
| OpenRouter, Groq, Together, Mistral, xAI, DeepSeek | OpenAI-compatible | Presets = base URL + model list |
| **Local**: Ollama, LM Studio, vLLM, Castle Chat | OpenAI-compatible (`/v1/chat/completions`) | User enters a URL. On iOS, LAN access needs `NSLocalNetworkUsageDescription`, plus ATS `NSAllowsLocalNetworking` for `http://` |
| **Apple on-device** | Foundation Models framework | No key, nothing leaves the device. iOS/macOS 26+ with Apple Intelligence |
| Custom | OpenAI-compatible | Any base URL + key + model |

Requirements:
- **Keys in the Keychain** on both platforms. Never SQLite, never `.env`. Migrate existing keys and delete the old copies.
- **Model picker** filled from `/v1/models` (Ollama: `/api/tags`), with a free-text override.
- **Separate choices for text vs vision**. Screenshots need a vision-capable model; hide vision on providers that don't support it.
- **Test connection** button showing the exact error (bad key / wrong URL / model missing).
- **Keep the Castle Chat guardrails for every provider:**
  - always send `max_tokens`
  - fit the prompt to the model's context window
  - never use `response_format` JSON-schema mode; ask for JSON in the prompt instead
  - apply timeouts
- **Consent before first use of each cloud provider** (App Review guideline 5.1.2(i)). Name the provider and what is sent (transcript text, attendee names, screenshots), then ask the user to agree.
- **Remove the private Castle hostname from shipping code.** Castle becomes a "Custom" entry you add on your own devices.
- **Desktop only:** remove the legacy TheBrain login and the unused `REMOTE_INTELLIGENCE_*` settings. The ingest pipeline, Supabase and Pinecone integrations have been removed entirely: the app is client-only (search and chat context use the local SQLite FTS index).

Transcription stays on-device by default:
- **iOS:** Apple Speech (already on-device).
- **Mac:** local Whisper. Change the store build's default from Deepgram (which needs a key) to local Whisper. Keep Deepgram, Gemini, Gladia and Google as bring-your-own-key options.

---

## 3. iOS: what's missing before TestFlight

Already fine:
- builds and tests pass
- on-device transcription
- usage strings for mic, speech, calendar, camera and photos
- `ITSAppUsesNonExemptEncryption = false`
- background audio mode, justified by recording
- demo data only in DEBUG builds
- no accounts, so no account-deletion requirement

| # | Item | Needed for | Work |
|---|---|---|---|
| i1 | Change bundle ID to `com.nofriction.meetings` (D1) | Upload | `ios/project.yml` |
| i2 | **Privacy manifest** `PrivacyInfo.xcprivacy`: declare the Required Reason APIs used (UserDefaults, file timestamps, etc.) and data types | Upload is rejected without it | New file |
| i3 | **Settings tab**: AI provider + key + model, subscription status, Restore Purchases, privacy policy, terms, "what leaves this device" | AI, StoreKit, review | New view |
| i4 | **Provider layer** (§2), replacing `CastleChat.swift`'s hardcoded host | Your requirement | Swift |
| i5 | **StoreKit 2**: paywall, `Product.products`, `purchase()`, `Transaction.currentEntitlements`, `Transaction.updates` listener, `AppStore.sync()` restore, a `.storekit` config file for local testing | Licensing | Swift |
| i6 | Decide what's free vs Pro (e.g. recording + transcripts free; AI notes / email / unlimited history Pro) | Paywall | Product call |
| i7 | **Recording-consent notice**. Some US states and countries require every participant's consent to record. Show a one-time notice and a reminder on the Record screen. | Review + legal | Small UI |
| i8 | README says "Nothing leaves the device". Update it, the App Store text, and the in-app copy now that AI can be sent to a provider | Accuracy / 5.1.2 | Copy |
| i9 | Launch screen and accent colour: the theme is still hazard-yellow, not the blue icon | Polish | Optional |
| i10 | Real-device test of recording, lock screen and a long meeting. Transcription doesn't run in the Simulator. | TestFlight quality | Manual |

## 4. Mac: what's missing before Mac App Store / TestFlight

The Mac app is signed for Developer ID (DMG). Mac App Store builds must be
**sandboxed**, signed with **Apple Distribution**, embed a **provisioning
profile**, and ship as a `.pkg` signed with the **3rd Party Mac Developer
Installer** certificate. Tauri supports this (the Tauri docs cover App Store distribution).

Sandbox blockers found in the code:

| # | Blocker | Where | Fix |
|---|---|---|---|
| m1 | Runs Homebrew **ffmpeg / ffprobe** for screen video and frames | `video_recorder.rs:235`, `frame_extractor.rs:75,157,219,288` | Replace with ScreenCaptureKit + AVAssetWriter (native). Or bundle a signed, sandboxed ffmpeg helper (heavier, and has licensing implications). |
| m2 | **Accessibility API** reads other apps' text | `accessibility_extractor.rs`, `accessibility_capture.rs`, `snapshot_extractor.rs` | Not allowed in sandbox. Remove from the store build (off by default already). |
| m3 | `osascript` → System Events for window titles | `privacy_filter.rs:106` | Use the window title that ScreenCaptureKit (`SCWindow.title`) already provides |
| m4 | `sh -c "ioreg …"` to detect active audio | `meeting_trigger.rs:505` | Core Audio `kAudioDevicePropertyDeviceIsRunningSomewhere` |
| m5 | Reads/writes `~/.nofriction-meetings/.env`, plus `dotenvy::dotenv()` from the working directory | `env_config.rs:45-125` | Remove from the store build; keys move to the Keychain |
| m6 | Obsidian vault path stored as a string | `settings.rs:93`, `obsidian_vault.rs` | Security-scoped bookmark + `files.user-selected.read-write` entitlement |
| m7 | `tauri-plugin-shell` granted (`shell:default`) | `capabilities/default.json` | Remove; use `opener` only |
| m8 | Entitlements: no `app-sandbox`, no `network.client`; Developer ID-only exceptions | `entitlements.plist` | New `entitlements.mas.plist`: app-sandbox, network.client, device.audio-input, personal-information.calendars, files.user-selected.read-write; drop `disable-library-validation` if possible |
| m9 | Identifier mismatch (`ai.nofriction.meetings` vs `com.nofriction.meetings`) | `tauri.conf.json`, `Info.plist` | Pick D1. Note that Tauri's data folder follows `identifier`, so migrate the existing data folder. |
| m10 | Add `ITSAppUsesNonExemptEncryption=false` and `LSApplicationCategoryType=public.app-category.productivity` | `Info.plist` | 2 keys |
| m11 | **StoreKit** is Swift-only | — | Small Swift bridge (Swift package linked into the Tauri binary via `swift-rs`) exposing products, purchase, entitlements and restore to Rust/JS |
| m12 | Default transcription = Deepgram (needs a key) | `settings.rs:111` | Default to local Whisper in store builds |
| m13 | Keys stored as plain text in SQLite | `settings.rs`, many | Keychain (`security-framework` crate) |
| m14 | Dead/owner-infra features (TheBrain, admin console; ingest server, Supabase, Pinecone removed) | various | Hide or cut for v1 |
| m15 | `scripts/release-macos.sh` builds a DMG | — | Add a `release-mas.sh`: build → sign with Apple Distribution + profile → `productbuild` `.pkg` → upload |

No hardcoded API keys were found in either app. The only private data in
shipping code is the Castle tailnet hostname (§2).

---

## 5. App Store Connect: step by step

### 5.1 One-time account setup (developer.apple.com + App Store Connect)
1. **Apple Developer Program**: team `C7GCEESE2V` is enrolled (you have Developer ID + Distribution certs). Confirm the membership is current at developer.apple.com → Membership.
2. **App Store Small Business Program**: enroll at developer.apple.com/app-store/small-business-program. Apple's commission drops from 30% to 15% under $1M/yr.
3. **Agreements, Tax, and Banking** (App Store Connect → Business):
   - accept the **Paid Applications Agreement**
   - add a bank account
   - complete the tax forms (W-9 for a US entity)
   Paid subscriptions don't work until this shows "Active". Do it now; approval can take days.
4. **EU Digital Services Act trader status** (App Store Connect → Business): declare whether you're a trader. If you sell anything, you are. Your address, phone and email will then be shown on the EU store page. Without it the app can't be distributed in the EU.

### 5.2 Identifiers (developer.apple.com → Certificates, IDs & Profiles)
5. **Identifiers → + → App IDs → App**:
   - Bundle ID **explicit** `com.nofriction.meetings` (D1)
   - platforms iOS + macOS
   - capabilities: **In-App Purchase** (on by default). Add *nothing* you don't use (no Push, no iCloud).
6. **Certificates**: you already have **Apple Distribution** and **3rd Party Mac Developer Installer**. Xcode automatic signing handles iOS. For the Mac, create a **Mac App Store Connect provisioning profile** for the bundle ID (Profiles → + → Mac App Store Connect).

### 5.3 The app record (App Store Connect → Apps → +)
7. **New App**:
   - platforms: **iOS** (add **macOS** to the same record later for Universal Purchase)
   - name
   - primary language: English (U.S.)
   - bundle ID: `com.nofriction.meetings`
   - SKU: `NOFRICTION-001`
   - user access: Full
8. **App Information**:
   - subtitle
   - category **Productivity** (secondary: Business)
   - content rights: "does not contain third-party content"
   - **Age Rating** questionnaire: likely 4+. Answer the user-generated-content and AI questions honestly.
9. **Privacy Policy URL** (required, even with no server): a static page, e.g. GitHub Pages in a `nofriction-site` repo. It must say:
   - what stays on the device
   - that transcripts go to the AI provider the user picks, under that provider's policy
   - that we collect nothing
   - a contact email
10. **App Privacy** ("nutrition label"):
    - Tracking: No.
    - Data collected *by us*: none.
    - Conservative choice: declare **User Content → Other User Content (transcripts)** as *not linked to identity, used for App Functionality*, sent to the user's chosen AI provider.
    - If you add crash reporting later, update this.

### 5.4 In-App Purchases (App Store Connect → your app → Monetization)
11. **Subscriptions → Subscription Group** "noFriction Pro":
    - Products `com.nofriction.meetings.pro.monthly` and `.pro.yearly`: pricing, **Introductory Offer** (e.g. 7- or 14-day free trial), display name, description, and a review screenshot of the paywall.
    - Or for D2 one-time: **In-App Purchases → Non-Consumable** `com.nofriction.meetings.lifetime`.
    - Optional: enable Family Sharing.
12. The paywall must show:
    - price and billing period
    - trial terms
    - **Restore Purchases**
    - links to **Terms of Use (EULA)** and **Privacy Policy**
    The EULA can be Apple's standard one; link it in the app description (guideline 3.1.2).

### 5.5 Build → upload (iOS)
13. Bump `CFBundleVersion` for every upload (`CURRENT_PROJECT_VERSION` in `project.yml`); `MARKETING_VERSION` = 1.0.0.
14. Xcode → Product → **Archive** (destination "Any iOS Device") → Organizer → **Distribute App → App Store Connect → Upload**. CLI equivalent:
    ```bash
    cd ios && xcodegen generate
    xcodebuild archive -project NoFriction.xcodeproj -scheme NoFriction \
      -destination 'generic/platform=iOS' -archivePath build/NoFriction.xcarchive
    xcodebuild -exportArchive -archivePath build/NoFriction.xcarchive \
      -exportOptionsPlist ExportOptions.plist -exportPath build/export
    # ExportOptions.plist: method=app-store-connect, destination=upload, teamID=C7GCEESE2V
    ```
15. Wait for "Processing" to finish (5–30 min; you get an email). Export compliance is answered automatically by `ITSAppUsesNonExemptEncryption=false`.

### 5.6 TestFlight (free)
16. **Internal testing**: TestFlight → Internal Testing → +, add up to **100** App Store Connect users (Admin/Developer/etc. roles). **No review needed**; available minutes after processing.
17. **External testing**: TestFlight → External Testing → new group → add the build → fill in **Test Information**:
    - beta description
    - feedback email
    - "What to Test"
    - review notes
    Submit for **Beta App Review** (first build of each version only, usually under 24h). Then invite by email or turn on a **public link**, up to **10,000** testers.
18. Review notes for the beta reviewer:
    - how to record
    - that AI needs a provider key: include a **spending-capped OpenAI key** or say that the Apple on-device model works without one
    - that no account is required
19. In TestFlight, **purchases are free** (sandbox) and subscriptions renew on an accelerated clock. Test buy, cancel, restore and trial expiry here.
20. TestFlight builds **expire after 90 days**, so upload new builds regularly.

### 5.7 Mac (phase 2, after §4)
21. In the same app record: **+ Add Platform → macOS** (Universal Purchase with the same bundle ID and products).
22. Build the sandboxed app, sign with *Apple Distribution* + embedded profile, then package:
    ```bash
    productbuild --component "noFriction Meetings.app" /Applications \
      --sign "3rd Party Mac Developer Installer: casey potenzone (C7GCEESE2V)" noFriction.pkg
    xcrun altool --upload-package noFriction.pkg --type macos --apple-id … --password @keychain:AC_PASSWORD
    ```
    Or drag the `.pkg` into Apple's **Transporter** app.
23. Mac TestFlight works the same way as iOS (internal/external groups). Testers install via the TestFlight app on macOS 12+.

### 5.8 Later: public App Store release
24. Screenshots: **iPhone 6.9"** and **iPad 13"** (required); Mac 1280×800+ for macOS.
25. Description, keywords, support URL, marketing URL, promotional text, "What's New".
26. Submit for App Review with the same review notes as §5.6 step 18. Choose manual or automatic release.

---

## 6. Order of work

1. D1–D5 decisions → create the App ID and app record (reserves the name and bundle ID).
2. Sign the Paid Apps agreement, tax, banking, DSA (these take longest; start day 1).
3. iOS: i1, i2, i3, i4, i5, i7, i8 → internal TestFlight.
4. Privacy policy + terms pages → external TestFlight.
5. Mac: m1–m15 → Mac TestFlight on the same record.
6. Screenshots and metadata → App Store review.
