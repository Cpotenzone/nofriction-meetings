# noFriction commercial launch checklist

The one list to go from "builds pass" to "on sale, monitored". It points to
the detailed docs instead of repeating them:

- [APP_STORE_RELEASE.md](APP_STORE_RELEASE.md): App Store plan and portal
  steps (§5 is referenced throughout as "ASR §…").
- [APP_STORE_LISTING.md](APP_STORE_LISTING.md): every metadata field,
  questionnaire answer and review note, ready to paste.
- [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md): Mac `.pkg` build and upload.
- [site/README.md](../site/README.md): hosting the website.

**Who:** *Owner* = account holder (Casey). Only the owner can sign
agreements, accept terms, set prices and submit. *Eng* = whoever builds and
tests (may be the owner or an agent). Tick the box when done.

Order matters: start section 1 on day one; tax, banking and DSA
verification can take days.

---

## 1. Business and legal (start first)

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Decide the **legal seller**: individual or company. The seller name shows on the App Store and can't easily change later. If a company: get a D-U-N-S number and enroll (or migrate) the developer account as an organization. | developer.apple.com → Membership | Owner |
| ☐ | Confirm Apple Developer Program membership is active (team `C7GCEESE2V`). | developer.apple.com → Membership | Owner |
| ☐ | **Paid Applications Agreement**: accept; add bank account; complete tax forms (W-9 for US). Must show **Active** before subscriptions work, even in TestFlight review. ASR §5.1 step 3. | App Store Connect → Business | Owner |
| ☐ | **EU DSA trader status**: declare as trader, enter address, phone and email to be shown on EU product pages; complete verification. Without it the app isn't distributed in the EU. ASR §5.1 step 4. | App Store Connect → Business | Owner |
| ☐ | Use a business address/phone you're willing to publish for DSA (consider a registered-agent address and a business number, not a home address). | — | Owner |
| ☐ | Enroll in the **App Store Small Business Program** (15% commission). ASR §5.1 step 2. | developer.apple.com/app-store/small-business-program | Owner |
| ☐ | **Trademark / name check** for "noFriction" (USPTO search, App Store search). | uspto.gov, App Store | Owner |
| ☐ | Legal read of [site/privacy.html](../site/privacy.html) and [site/terms.html](../site/terms.html) (recording-consent language, EU/UK and California notices if counsel wants them). | — | Owner (+ counsel) |

## 2. Domain, website and support inbox

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Own `nofriction.ai` (or choose another domain; then update the URL constants, below). | Registrar | Owner |
| ☐ | Publish `site/` with GitHub Pages and the custom domain; enforce HTTPS. Steps: [site/README.md](../site/README.md). | GitHub → Settings → Pages; DNS | Owner / Eng |
| ☐ | Verify `https://nofriction.ai/privacy` returns 200 (apps link to it), and `/support.html`, `/terms.html`. | `curl -sI` | Eng |
| ☐ | If the domain or path differs: change `AppLinks.privacyPolicy` (`ios/NoFriction/App/AppLinks.swift`) and `PRIVACY_URL` (`src/lib/build.ts`) **before** the release builds. | code | Eng |
| ☐ | Set up **support@nofriction.ai** mail (MX, SPF, DMARC); send a test from outside. | DNS + mail provider | Owner |
| ☐ | Prepare saved replies: connecting a key, wrong key/no credit, permissions, restore, cancel/refund (send refunds to reportaproblem.apple.com). Source: [site/support.html](../site/support.html). | mail client | Owner |
| ☐ | At launch: replace the "Coming soon to the App Store" button in `site/index.html` with the App Store link. | `site/index.html` | Eng |

## 3. Product decisions

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | **Pricing**: monthly and yearly price tiers per storefront. The `.storekit` test file uses 79.99/yr only as a placeholder. Neither the site nor the listing states prices; they come from App Store Connect. | App Store Connect → Subscriptions → Pricing | Owner |
| ☐ | **Trial**: introductory offer length (the local test config uses 1 week), on both products. | same → Introductory Offers | Owner |
| ☐ | Family Sharing on/off (can't be turned off once on). | same | Owner |
| ☐ | Release regions (all, or exclude some). | App Store Connect → Pricing and Availability | Owner |
| ☐ | App name final pick (see listing doc; check availability by creating the record). | App Store Connect | Owner |
| ☐ | Decide whether the Mac DMG (Developer ID) stays available. It has no StoreKit, so it can't honor subscriptions. | — | Owner |
| ☐ | Mac auto-report: after a recording longer than 6 minutes the Mac app writes a report automatically with the connected provider (`auto_generate_report`, default on, no UI toggle). Decide whether to keep it, add a toggle, or turn it off by default; the privacy policy describes current behavior. | `src-tauri/src/settings.rs` | Owner → Eng |

## 4. Identifiers and the app record

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | App ID `com.nofriction.meetings` (explicit), iOS + macOS, In-App Purchase only. ASR §5.2. | developer.apple.com → Identifiers | Owner |
| ☐ | Mac App Store Connect provisioning profile → `src-tauri/embedded.provisionprofile` (gitignored). [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md) "One-time setup". | developer.apple.com → Profiles | Owner |
| ☐ | Create the app record (iOS), SKU `NOFRICTION-001`; then **Add Platform → macOS** on the same record for Universal Purchase. ASR §5.3, §5.7. | App Store Connect → Apps | Owner |
| ☐ | Subscription group **noFriction Pro** with `com.nofriction.meetings.pro.monthly` and `.pro.yearly`; display names, descriptions and a paywall screenshot for review. Copy: listing doc §"Subscriptions". ASR §5.4. | App Store Connect → Monetization → Subscriptions | Owner |
| ☐ | Fill App Information, Age Rating, App Privacy, export compliance and content rights from the listing doc. | App Store Connect | Owner |
| ☐ | Set Privacy Policy URL, Support URL, Marketing URL (both platforms). | App Store Connect | Owner |

## 5. Builds

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | iOS: bump `CURRENT_PROJECT_VERSION` (and `MARKETING_VERSION` if needed) in `ios/project.yml`; archive and upload. ASR §5.5. | Xcode Organizer / `xcodebuild` | Eng |
| ☐ | Mac: `scripts/release-mas.sh --upload` (needs the profile and `AC_PASSWORD` keychain item or an API key). | Terminal | Eng |
| ☐ | Wait for processing; check the build has no missing-compliance warning (both set `ITSAppUsesNonExemptEncryption=false`). | App Store Connect → TestFlight | Eng |

## 6. TestFlight: internal → external

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Internal group (App Store Connect users, no review). ASR §5.6 step 16. | TestFlight → Internal Testing | Owner |
| ☐ | Device checks on real hardware: long recording with the screen locked (iPhone); transcription; calendar match; photos; auto-stop banner; Delete + undo; Strike; export; consent sheet; paywall → sandbox purchase → restore → trial expiry (accelerated renewals); Apple on-device on an Apple Intelligence device; one real call per provider. ASR "Only you can do these" item 6. | TestFlight builds | Owner / Eng |
| ☐ | Mac: same list, plus system-audio capture, choosing displays/windows, Snap, Chat, sandboxed launch from `/Applications`. | Mac TestFlight | Owner / Eng |
| ☐ | External group: Test Information (beta description, feedback email, What to Test, review notes from the listing doc). Submit for Beta App Review. ASR §5.6 step 17. | TestFlight → External Testing | Owner |
| ☐ | Collect feedback (TestFlight screenshots + crash feedback appear under TestFlight → Feedback). Fix, re-upload, repeat. Builds expire after 90 days. | App Store Connect | Owner / Eng |

## 7. Store listing and assets

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Paste name, subtitle, promotional text, description, keywords, What's New (not shown for a platform's first version) for iOS and macOS. Source: [APP_STORE_LISTING.md](APP_STORE_LISTING.md). | App Store Connect → each platform's version page | Owner |
| ☐ | iPhone 6.9" and iPad 13" screenshots (generated under `ios/AppStore/screenshots/`). Check that no real names, emails or keys appear. | same | Owner / Eng |
| ☐ | Mac screenshots (16:10, e.g. 2880×1800) captured with the demo-data procedure in the listing doc. | same | Owner / Eng |
| ☐ | Optional: app preview video (iOS 15–30 s). | same | Owner |
| ☐ | Paywall screenshot attached to each subscription for review. | Subscriptions → Review Information | Owner |

## 8. App Review submission

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Add a **spending-capped** OpenAI key for the reviewer (set a monthly budget limit in the OpenAI dashboard; create a project key just for review). Paste it into Review Notes only, never into the app or repo. Revoke after approval. | platform.openai.com → Limits / API keys | Owner |
| ☐ | App Review Information: contact name, phone, email; "Sign-in required" = **No**; notes from the listing doc. | App Store Connect → version page | Owner |
| ☐ | Add both subscriptions to the version ("In-App Purchases and Subscriptions" section) so they're reviewed with the first build. | version page | Owner |
| ☐ | Release option: **manual** release is recommended for the first version, so you control launch day. | version page | Owner |
| ☐ | Submit iOS and macOS (separate submissions on the same record). | App Store Connect | Owner |
| ☐ | Respond to any rejection in Resolution Center within a day; common risks for this app: 2.1 (reviewer can't reach AI: point to the key), 3.1.2 (subscription terms visible in app and description), 5.1.1/5.1.2 (data sent to third-party AI: point to the consent sheet and privacy policy). | App Store Connect → Resolution Center | Owner |

## 9. Launch day

| ✓ | Task | Where | Who |
|---|---|---|---|
| ☐ | Release the approved versions. | App Store Connect | Owner |
| ☐ | Update the site's App Store button; publish. | `site/index.html` | Eng |
| ☐ | Revoke the review API key (or keep it capped if review continues). | OpenAI dashboard | Owner |
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
| ☐ | Daily | **Support inbox**: answer within two business days (promised on the support page). | support@nofriction.ai | Owner |
| ☐ | Monthly | Provider API changes: re-check the endpoints in [AI_PROVIDERS.md](AI_PROVIDERS.md) (base URLs, model lists, parameter quirks). | provider docs | Eng |
| ☐ | Yearly | Renew the Mac provisioning profile and certificates; re-accept updated Apple agreements when prompted. | developer.apple.com | Owner |
| ☐ | Each release | Keep privacy policy, App Privacy answers and the privacy manifest in sync with any new network destination. | `site/privacy.html`, App Store Connect, `ios/NoFriction/PrivacyInfo.xcprivacy` | Eng → Owner |
