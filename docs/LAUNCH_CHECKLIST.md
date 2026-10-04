# noFriction commercial launch checklist

The one list to go from "builds pass" to "on sale, monitored". It points to
the detailed docs instead of repeating them:

- [APPLE_SETUP.md](APPLE_SETUP.md): step-by-step Apple portal guide.
- [APPLE_SETUP_CLOSEOUT_20261003.md](APPLE_SETUP_CLOSEOUT_20261003.md): what
  was configured and verified on 2026-10-03, and what remains.
- [APP_STORE_RELEASE.md](APP_STORE_RELEASE.md): App Store plan and portal
  steps (§5 is referenced throughout as "ASR §…").
- [APP_STORE_LISTING.md](APP_STORE_LISTING.md): every metadata field,
  questionnaire answer and review note, ready to paste.
- [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md): Mac `.pkg` build and upload.
- [AI_PROVIDERS.md](AI_PROVIDERS.md): the AI contract (Apple on-device or one
  user-entered OpenAI-compatible endpoint; no named services, no supplied keys).
- [site/README.md](../site/README.md): the website pages to integrate with
  nofriction.io.

**Who:** *Owner* = account holder (Casey). Only the owner can sign
agreements, accept terms, set prices and submit. *Eng* = whoever builds and
tests (may be the owner or an agent). ☑ = done; the date says when.

**Status (2026-10-03):** the app record, subscriptions, signing profiles, U.S.
pricing and availability, iOS screenshots, metadata and reviewer contact are
configured. iOS 1.0.0 build 3 is archived and exported, and Mac 3.6.0 build38
is packaged; **neither is uploaded**. Nothing has been submitted or released.
The website pages are not published and the support mailbox is untested.

Order matters: start section 1 on day one; banking and verification can take
days.

---

## 1. Business and legal (start first)

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☑ | Use the existing team `C7GCEESE2V`; the legal seller shown in App Store Connect was checked. Done 2026-10-03. Certificate names do not establish the enrollment type; check Membership details if that matters. | developer.apple.com → Membership | Owner |
| ☐ | Confirm Apple Developer Program membership is active and check its renewal date (team `C7GCEESE2V`). | developer.apple.com → Membership | Owner |
| ☑ | **Free Apps Agreement**: Active. Done 2026-10-03 (see closeout). | App Store Connect → Business | Owner |
| ☑ | **Tax**: US W-9 Active (since 2026-10-01). Done 2026-10-03 (see closeout). | App Store Connect → Business | Owner |
| ☐ | **Paid Applications Agreement**: status **Pending User Info**. Add and verify the payout **bank account** and resolve any remaining requirements. Must show **Active** before subscriptions can be sold, even in TestFlight review. ASR §5.1 step 3. | App Store Connect → Business | Owner |
| ☑ | **EU Digital Services Act (DSA)** status: Active (since 2026-10-01). Done 2026-10-03 (see closeout). The app is currently offered in the USA only. | App Store Connect → Business | Owner |
| ☐ | Enroll in the **App Store Small Business Program** (15% commission). ASR §5.1 step 2. | developer.apple.com/app-store/small-business-program | Owner |
| ☐ | **Trademark / name check** for "noFriction" (USPTO search, App Store search). | uspto.gov, App Store | Owner |
| ☐ | Legal read of [site/privacy.html](../site/privacy.html), the meeting-app privacy supplement for `nofriction.io/privacy` (recording-consent language, EU/UK and California notices if counsel wants them). Terms are Apple's standard EULA. | — | Owner (+ counsel) |

## 2. Domain, website and support inbox

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☑ | Domain chosen: **nofriction.io**, the owner's existing live site. Done 2026-10-03. | — | Owner |
| ☑ | Both apps link to `https://nofriction.io/privacy`, `https://nofriction.io/contact` and `casey@nofriction.io`; Terms link to Apple's standard EULA. Done 2026-10-03. If a path changes, update `AppLinks` (`ios/NoFriction/App/AppLinks.swift`) and `PRIVACY_URL` / `SUPPORT_URL` / `SUPPORT_EMAIL` (`src/lib/build.ts`) **before** the release builds. | code | Eng |
| ☐ | Integrate the meeting-app privacy and contact/support pages from `site/` with the existing nofriction.io website. The current public privacy page covers the website only. Steps: [site/README.md](../site/README.md). | nofriction.io hosting | Owner / Eng |
| ☐ | Verify `https://nofriction.io/privacy` and `https://nofriction.io/contact` return 200 and show the meeting-app content (extensionless routes), and that the in-app links open them. | `curl -sI`, both apps | Eng |
| ☐ | Send a test email from an outside account to **casey@nofriction.io** and confirm it arrives (delivery is untested). | mail client | Owner |
| ☐ | Prepare saved replies: setting up AI (Apple on-device or your own endpoint), endpoint errors (wrong key, unreachable server, model ID), permissions, restore, cancel/refund (send refunds to reportaproblem.apple.com). Source: [site/support.html](../site/support.html) and [USER_GUIDE.md](USER_GUIDE.md). | mail client | Owner |
| ☐ | At launch: replace the "Coming soon to the App Store" button in `site/index.html` with the App Store link. | `site/index.html` | Eng |

## 3. Product decisions

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☑ | **US prices: $0.99/month and $5.99/year** configured and read back. Done 2026-10-03 (see closeout). The `.storekit` fixture is local only; live localized prices come from App Store Connect. | App Store Connect → Subscriptions → Pricing | Owner / Eng |
| ☑ | **1-week free introductory trial** on both products (starts 2026-10-03, no end date). Done 2026-10-03 (see closeout). | same → Introductory Offers | Owner / Eng |
| ☑ | Family Sharing: **off**. Done 2026-10-03. Decide separately before turning it on; it can't be turned off once on. | same | Owner |
| ☑ | Release regions: **USA only** for the app and subscriptions; automatic expansion to new territories off. Done 2026-10-03. Decide separately before adding countries. | App Store Connect → Pricing and Availability | Owner |
| ☑ | App name `noFriction: Meeting Notes`, subtitle `Record, transcribe, summarize`. Done 2026-10-03. | App Store Connect | Owner |
| ☐ | Decide whether the Mac DMG (Developer ID) stays available. It has no StoreKit, so it can't honor subscriptions. | — | Owner |
| ☐ | Verify on real hardware that automatic AI respects the selected endpoint and model, the consent for a public endpoint, and the two Settings → AI Engine → Automatic AI toggles. Offline AI uses Apple on-device or a local server; noFriction offers no hosted AI. | `src/features/settings/AIProviderSettings.tsx` | Eng |

## 4. Identifiers and the app record

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☑ | App ID `com.nofriction.meetings` (explicit), iOS + macOS. Done 2026-10-03 (see closeout). ASR §5.2. | developer.apple.com → Identifiers | Owner |
| ☑ | Provisioning profiles: iOS App Store profile installed for Xcode; Mac App Store Connect profile installed at `src-tauri/embedded.provisionprofile` (gitignored). Both expire 2027-09-22. Done 2026-10-03 (see closeout). | developer.apple.com → Profiles | Owner |
| ☐ | **Apple Watch** App ID `com.nofriction.meetings.watchkitapp` (explicit, no capabilities) and, for manual signing, its App Store Connect profile (`NF_WATCH_PROFILE_UUID`). Added 2026-10-04; see APPLE_SETUP.md §2.5 and WATCH_APP.md. | developer.apple.com → Identifiers / Profiles | Owner |
| ☑ | App record `6818838861`, SKU `NOFRICTION-001`, with iOS 1.0.0 and macOS 3.6.0 on the same record (Universal Purchase). Both versions in Prepare for Submission. Done 2026-10-03. ASR §5.3, §5.7. | App Store Connect → Apps | Owner |
| ☑ | Subscription group **noFriction Pro** (`22437188`) with `com.nofriction.meetings.pro.monthly` and `.pro.yearly`; names, descriptions and both paywall review images uploaded. Done 2026-10-03 (see closeout). The products are submitted with the first app version. ASR §5.4. | App Store Connect → Monetization → Subscriptions | Owner |
| ☑ | Categories (Productivity / Business) and the source-matched age-rating questionnaire saved. Done 2026-10-03. | App Store Connect → App Information | Owner |
| ☐ | **App Privacy** declaration: reconcile the draft with the current app (Apple on-device or user-entered endpoint, on-device transcription) and publish it. | App Store Connect → App Privacy | Owner |
| ☐ | **Content rights** attestation. | App Store Connect → App Information | Owner |
| ☑ | Support URL saved on both platform records. Done 2026-10-03. | App Store Connect | Owner |
| ☐ | Privacy Policy URL (`https://nofriction.io/privacy`) once the page is published; Marketing URL (optional). | App Store Connect | Owner |

## 5. Builds

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | iOS: upload **build 4 or later** with `scripts/release-ios.sh --upload` (embeds and scans the Apple Watch app). Build 3 (exported 2026-10-03) is superseded: it has no watch app. Needs the watch App ID above and the App Store Connect API key. | Terminal | Eng / Owner |
| ☐ | Mac: upload 3.6.0 build38. The signed `.pkg` was packaged and scanned on 2026-10-03 (`dist-mas/`), but **not uploaded**. `scripts/release-mas.sh --upload` needs the profile and the `AC_PASSWORD` keychain item or an API key. | Terminal, Transporter | Eng |
| ☐ | Wait for processing; check the build has no missing-compliance warning (both set `ITSAppUsesNonExemptEncryption=false`). | App Store Connect → TestFlight | Eng |

## 6. TestFlight: internal → external

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Internal group (App Store Connect users, no review). ASR §5.6 step 16. | TestFlight → Internal Testing | Owner |
| ☐ | Device checks on real hardware: long recording with the screen locked (iPhone); transcription; calendar match; photos; auto-stop banner; Delete + undo; Strike; export; paywall → sandbox purchase → restore → trial expiry (accelerated renewals); Apple on-device on an Apple Intelligence device, including offline after the model download; a custom endpoint on the local network; a public HTTPS endpoint to see the consent sheet show the destination. ASR "Only you can do these" item 6. | TestFlight builds | Owner / Eng |
| ☐ | Mac: same list, plus system-audio capture, choosing displays/windows, Snap, Chat, sandboxed launch from `/Applications`. | Mac TestFlight | Owner / Eng |
| ☐ | External group: Test Information (beta description, feedback email, What to Test, review notes from the listing doc). Submit for Beta App Review. ASR §5.6 step 17. | TestFlight → External Testing | Owner |
| ☐ | Collect feedback (TestFlight screenshots + crash feedback appear under TestFlight → Feedback). Fix, re-upload, repeat. Builds expire after 90 days. | App Store Connect | Owner / Eng |

## 7. Store listing and assets

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☑ | Name, subtitle, promotional text, descriptions, keywords and endpoint-only review notes saved on both platform records. Done 2026-10-03 (see closeout). What's New isn't shown for a platform's first version. | App Store Connect → each platform's version page | Owner |
| ☑ | iPhone 6.9" and iPad 13" screenshots (six each, from `ios/AppStore/screenshots/`) uploaded and processed; the AI-settings image shows the endpoint-only screen. Done 2026-10-03. | same | Owner / Eng |
| ☐ | Apple Watch screenshots: upload `ios/AppStore/screenshots/watch-46mm/*.png` (416×496, three) to the iOS version's Apple Watch section; required once the build includes the watch app. | App Store Connect → iOS 1.0 | Owner |
| ☐ | Mac screenshots (16:10, e.g. 2880×1800) from a current Mac App Store build, captured with the demo-data procedure in the listing doc. | same | Owner / Eng |
| ☐ | Optional: app preview video (iOS 15–30 s). | same | Owner |
| ☑ | Paywall review image attached to each subscription. Done 2026-10-03. | Subscriptions → Review Information | Owner |

## 8. App Review submission

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | **Review path for AI**, described in the review notes: Apple on-device, which needs an Apple Intelligence-capable device on iOS 26 / macOS 26 or later with Apple Intelligence on and its model downloaded; or an OpenAI-compatible endpoint the reviewer enters (base URL, model, optional key of their own). noFriction supplies no key or hosted model: never put a credential in the notes, the app or the repo. Verify the Apple on-device path on the submitted build before submitting. | App Store Connect → version page → Notes | Owner / Eng |
| ☑ | App Review Information: reviewer contact set on both platforms; "Sign-in required" = **No**; endpoint-only notes saved. Done 2026-10-03. | App Store Connect → version page | Owner |
| ☐ | Add both subscriptions to the version ("In-App Purchases and Subscriptions" section) so they're reviewed with the first build. | version page | Owner |
| ☑ | Release option: **manual** on both versions. Done 2026-10-03. | version page | Owner |
| ☐ | Submit iOS and macOS (separate submissions on the same record). | App Store Connect | Owner |
| ☐ | Respond to any rejection in Resolution Center within a day; common risks for this app: 2.1 (reviewer can't reach AI: point to the Apple on-device device requirements and the endpoint steps in the notes), 3.1.2 (subscription terms visible in app and description), 5.1.1/5.1.2 (data sent to a user-chosen AI endpoint: point to the consent sheet and privacy policy). | App Store Connect → Resolution Center | Owner |

## 9. Launch day

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Release the approved versions. | App Store Connect | Owner |
| ☐ | Update the site's App Store button; publish. | `site/index.html` | Eng |
| ☐ | Tag the release commit (`git tag ios-1.0.0`, `mac-3.6.0`) and add a [CHANGELOG.md](CHANGELOG.md) entry. Don't commit build archives. | git | Eng |

## 10. After launch: monitoring without analytics

The apps have no analytics or crash SDK by design. Everything comes from
Apple and from email.

| ✓ | Cadence | Task | Where | Who |
|---|---|---|---|---|
| ☐ | Daily (first 2 weeks), then weekly | **Crashes**: read crash reports and hangs; open the logs in Xcode for symbolicated traces. | Xcode → Window → Organizer → Crashes; App Store Connect → TestFlight/App Analytics → Crashes | Eng |
| ☐ | Daily, then weekly | **Ratings and reviews**: reply to reviews (especially 1–3 stars) in App Store Connect. | App Store Connect → Ratings and Reviews | Owner |
| ☐ | Weekly | **Sales, trials, conversions, renewals, refunds** (aggregate, from Apple). | App Store Connect → Sales and Trends; Subscriptions reports | Owner |
| ☐ | Weekly | **App Analytics** (impressions, page views, downloads; only from users who opted in to share with developers). | App Store Connect → App Analytics | Owner |
| ☐ | Daily | **Support inbox**: answer within two business days (promised on the support page). | casey@nofriction.io | Owner |
| ☐ | Each release | AI policy guard and artifact scan: `python3 scripts/check-ai-provider-policy.py` and the signed-artifact credential scan (both run by the release scripts) must pass. Don't bypass a failure. See [AI_PROVIDERS.md](AI_PROVIDERS.md). | Terminal | Eng |
| ☐ | Yearly | Renew the provisioning profiles (expire 2027-09-22) and certificates; re-accept updated Apple agreements when prompted. | developer.apple.com | Owner |
| ☐ | Each release | Keep privacy policy, App Privacy answers and the privacy manifest in sync with any new network destination. | `site/privacy.html`, App Store Connect, `ios/NoFriction/PrivacyInfo.xcprivacy` | Eng → Owner |
