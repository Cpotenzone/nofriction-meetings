# Apple setup: everything needed to publish noFriction

> **Current as of 2026-10-03.** Website and contact: `https://nofriction.io/privacy`,
> `https://nofriction.io/contact` and `casey@nofriction.io`; Terms are Apple's
> standard EULA. AI: Apple on-device, or one OpenAI-compatible endpoint the
> user enters (base URL, model, optional key of their own). There are no named
> AI services, no default remote URL and no supplied API keys. Transcription is
> on-device only. See [AI_PROVIDERS.md](AI_PROVIDERS.md).
>
> Steps marked **Done 2026-10-03 (see closeout)** were completed during setup;
> the evidence and the remaining work are in the
> [dated closeout](APPLE_SETUP_CLOSEOUT_20261003.md). The website pages are
> **not published yet** and delivery to the mailbox is **untested**. No build
> has been uploaded, and nothing has been submitted or released. Don't treat
> this guide as proof that the website or the app-privacy declarations are
> live.

What you need to do in Apple's portals, in order, to ship the iPhone/iPad app and
the Mac app through the App Store, TestFlight first. This guide covers the
account, portal and upload work; source readiness does not establish a successful
App Store build, purchase, restore or review.

Two sites are involved:
- **Developer portal**: [developer.apple.com/account](https://developer.apple.com/account)
  (membership, certificates, identifiers, profiles).
- **App Store Connect**: [appstoreconnect.apple.com](https://appstoreconnect.apple.com)
  (agreements, the app record, subscriptions, TestFlight, submission).

Ready-to-paste text (descriptions, keywords, review notes, privacy and
age-rating answers) is in [APP_STORE_LISTING.md](APP_STORE_LISTING.md).

---

## Values you'll type (keep this open)

| Field | Value |
|---|---|
| Team ID | `C7GCEESE2V` |
| Bundle ID (iOS **and** Mac, one record = Universal Purchase) | `com.nofriction.meetings` |
| Apple Watch app bundle ID (ships inside the iOS app) | `com.nofriction.meetings.watchkitapp` |
| App Store Connect app (Apple ID, = `ASC_APP_ID`) | `6818838861` |
| SKU (your own label, never shown) | `NOFRICTION-001` |
| App name / subtitle | `noFriction: Meeting Notes` / `Record, transcribe, summarize` |
| Subscription group | `noFriction Pro`, group ID `22437188` |
| Monthly product ID | `com.nofriction.meetings.pro.monthly`: $0.99/month (US), 1-week free trial |
| Yearly product ID | `com.nofriction.meetings.pro.yearly`: $5.99/year (US), 1-week free trial |
| Availability | USA only (app and subscriptions); automatic expansion off; Family Sharing off |
| iOS version | `1.0.0` (build number from `ios/build_number.txt`, bumped by the script) |
| Mac version | `3.6.0` (build number from `src-tauri/build_number.txt`) |
| Privacy policy URL | `https://nofriction.io/privacy` (meeting-app page not published yet; see step 6) |
| Support URL | `https://nofriction.io/contact` |
| Support email | `casey@nofriction.io` (delivery not yet tested) |
| Terms | Apple's standard EULA |
| Category | Productivity (secondary: Business) |

The product IDs and bundle ID are compiled into both apps. **Type them exactly**,
because a typo means the paywall finds no products.

Already in place on this Mac (verified 2026-10-03): the **Apple Distribution**,
**3rd Party Mac Developer Installer** and **Developer ID Application**
certificates for team C7GCEESE2V, and both apps declare
`ITSAppUsesNonExemptEncryption = false`.

---

## Step 0: Approved launch decisions (2026-10-03)

The owner approved these values on 2026-10-03; the latest instruction (AI
and domain) supersedes earlier decisions. **Done 2026-10-03 (see closeout)**:
all of them are configured in App Store Connect and in both apps.

1. **Publisher team:** use the existing team `C7GCEESE2V`. The legal seller
   shown in App Store Connect was checked. A personal name on a certificate
   does **not** establish Individual enrollment; the seller follows the legal
   entity recorded by Apple. See
   [Apple's enrollment guidance](https://developer.apple.com/help/account/membership/program-enrollment/).
2. **US launch prices:** **$0.99/month** and **$5.99/year**, with a **1-week free
   introductory trial** for eligible subscribers on both products. These are
   set in App Store Connect; the local StoreKit file is only a test fixture.
3. **Domain:** `nofriction.io` (the owner's existing site); privacy
   `/privacy`, contact `/contact`, and `casey@nofriction.io`. Terms are Apple's
   standard EULA. The domain choice is settled; publishing the meeting-app
   pages and testing mailbox delivery are still open (step 6).
4. **AI:** Apple on-device, or one OpenAI-compatible endpoint that the user
   enters (base URL, model, optional key of their own). No named AI services,
   no key detection, no default remote URL, no supplied keys, no hosted AI.
   Offline AI works with Apple on-device or a local server after their model
   downloads. Transcription is on-device only (Apple speech on iOS, local
   Whisper on the Mac). See [AI_PROVIDERS.md](AI_PROVIDERS.md),
   [AppleOnDevice.swift](../ios/NoFriction/AI/AppleOnDevice.swift) and
   [AIProvider.swift](../ios/NoFriction/AI/AIProvider.swift).
5. **Availability:** USA only, for the app and the subscriptions; automatic
   expansion to new territories off; Family Sharing off.

---

## Step 1: Account and business (start today; Apple takes days)

App Store Connect → **Business** (top menu).

1. **Membership check.** developer.apple.com → Membership details: the
   membership is active and not expiring soon (it's $99/year).
2. **Paid Apps Agreement.** Business → Agreements → *Paid Apps* → review and
   accept. It has to be **Active** before subscriptions can be sold or tested
   in TestFlight sandbox. **Open:** as of 2026-10-03 it shows **Pending User
   Info**; it needs the bank account (item 4). The Free Apps Agreement is
   Active.
3. **Tax.** Business → Tax forms → complete the **W-9** (US person or company)
   or W-8BEN (non-US). Add any other tax forms it asks for.
   **Done 2026-10-03 (see closeout).** W-9 Active since October 1; don't redo it.
4. **Banking.** Business → Bank accounts → add the account Apple pays into.
   Verification can take a few days. **Open:** no bank account yet (the
   portal shows an *Add Bank Account* banner).
5. **EU Digital Services Act trader status.** Business → Compliance (or App
   Store Connect → your account → "Digital Services Act"). Selling a
   subscription makes you a **trader**: Apple shows the address, phone and
   email you enter here on the EU store page. Without this the app can't be
   distributed in the EU. **Done 2026-10-03 (see closeout).** DSA Active since
   October 1.
6. **App Store Small Business Program** (strongly recommended).
   [developer.apple.com/app-store/small-business-program](https://developer.apple.com/app-store/small-business-program/)
   → Enroll. It cuts Apple's commission from 30% to 15% while you earn under
   $1M/year.

---

## Step 2: Developer portal: App ID, certificates, Mac profile

developer.apple.com → **Certificates, IDs & Profiles**.

### 2.1 Register the App ID
Identifiers → **+** → *App IDs* → *App* → Continue.
- **Description:** `noFriction`
- **Bundle ID:** *Explicit* → `com.nofriction.meetings`
- **Capabilities:** leave **In-App Purchase** on (it's on by default). Turn
  **nothing else** on: no Push, iCloud, Sign in with Apple or App Groups. The
  apps don't use them, and extra capabilities can break signing.
- **Platforms:** make sure macOS is included. App IDs cover iOS and macOS
  together, which is what makes Universal Purchase work.
- Register.

**Done 2026-10-03 (see closeout).** Bundle record `CHQ7ZF73F8`.

### 2.2 Certificates (check, don't recreate)
Certificates list → confirm that these exist and aren't expired:
**Apple Distribution**, **Mac Installer Distribution** (shows locally as "3rd
Party Mac Developer Installer"), and **Developer ID Application**. They're all
in this Mac's keychain already. Only create new ones if one has expired.
**Done 2026-10-03 (see closeout).** All three identities are present.

### 2.3 Mac App Store provisioning profile (Mac only)
Profiles → **+** → Distribution → **Mac App Store Connect** → Continue.
- App ID: `com.nofriction.meetings` → Certificate: your **Apple Distribution**
  → Name: `noFriction Mac App Store` → Generate → **Download**.
- Save it as **`src-tauri/embedded.provisionprofile`** in the repo. It's
  gitignored and never committed. `scripts/release-mas.sh` checks that it
  matches the bundle ID and is a distribution profile.

**Done 2026-10-03 (see closeout).** Installed at that path; expires 2027-09-22.

### 2.4 iOS profile
`scripts/release-ios.sh` uses automatic signing with
`-allowProvisioningUpdates` by default, so Xcode can create the iOS App Store
profile itself once it can sign in (step 3). It can also use an installed
manual distribution profile (`NF_IOS_PROFILE_UUID` and
`NF_IOS_SIGNING_IDENTITY`; see the script's header).

**Done 2026-10-03 (see closeout).** An iOS App Store profile was created and installed for
Xcode; expires 2027-09-22. Build 3 was archived and exported with it.

### 2.5 Apple Watch app (added 2026-10-04) — **Open**
The iOS app now embeds a watch app (`ios/NoFrictionWatch`, details in
[WATCH_APP.md](WATCH_APP.md)). It needs its own identifier and profile:
1. Identifiers → **+** → App IDs → App → Explicit bundle ID
   **`com.nofriction.meetings.watchkitapp`**, description `noFriction Watch`.
   No capabilities (background audio is an Info.plist key; WatchConnectivity
   needs no entitlement).
2. Profile: with automatic signing (API key from step 3) nothing to do; Xcode
   creates it on the first archive. Manual path: Profiles → **+** →
   Distribution → **App Store Connect** → that App ID → your Apple
   Distribution certificate → download and install, then pass its UUID as
   `NF_WATCH_PROFILE_UUID` together with `NF_IOS_PROFILE_UUID` and
   `NF_IOS_SIGNING_IDENTITY`.
3. **No separate app record.** The watch app ships inside iOS 1.0.0 (app
   `6818838861`).

---

## Step 3: An App Store Connect API key (for the upload scripts)

**Done 2026-10-03 (see closeout).** **Reuse the existing authorized App
Store Connect key for this team.** Its API access was verified during setup on
2026-10-03; creating another Admin key is unnecessary. Apple's [API documentation](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api/)
explains that team keys apply across all apps in the account; their assigned
role still governs permitted operations. Confirm signing/profile privileges
before an archive that requests automatic provisioning.

Keep the existing `.p8` file in its secure location outside the repo. Do not
copy it into source, paste it into docs, revoke it or change its access merely
to add this app. The Mac upload tool must be able to locate it in a supported
private-key directory (the script documents `~/.appstoreconnect/private_keys/`).
- The two scripts name the variables differently (these names are unchanged).
  Set both sets from the same key, in your shell profile or right before you
  run them:
  ```bash
  # iOS: scripts/release-ios.sh
  export ASC_KEY_ID=<KEYID>
  export ASC_ISSUER_ID=<ISSUER-ID>
  export ASC_KEY_PATH=~/.appstoreconnect/private_keys/AuthKey_<KEYID>.p8
  # Mac: scripts/release-mas.sh
  export APPLE_API_KEY_ID=<KEYID>
  export APPLE_API_ISSUER=<ISSUER-ID>
  export ASC_APP_ID=6818838861   # the app's Apple ID (step 4.3)
  ```
- **Alternative to the key:** sign in to Xcode → Settings → Accounts with an
  Apple ID on team C7GCEESE2V. That covers iOS signing, but the scripted
  uploads still want the key.

---

## Step 4: Create the app record

### 4.1 New app
App Store Connect → **Apps** → **+** → *New App*.
- **Platforms:** check **iOS** and **macOS**. Same record, so one subscription
  unlocks both apps.
- **Name:** `noFriction: Meeting Notes` (25/30). Alternates are in
  APP_STORE_LISTING.md. Names are unique across the store; if it's taken, use
  an alternate.
- **Primary language:** English (U.S.)
- **Bundle ID:** `com.nofriction.meetings` (it appears in the menu after step 2.1)
- **SKU:** `NOFRICTION-001`
- **User access:** Full access
- Create. This also **reserves the name**.

**Done 2026-10-03 (see closeout).** App record `6818838861` with iOS 1.0.0 and macOS 3.6.0,
both in Prepare for Submission with manual release.

### 4.2 App Information (left sidebar)
- **Subtitle:** from APP_STORE_LISTING.md (`Record, transcribe, summarize`, 29/30). Done 2026-10-03.
- **Category:** Primary *Productivity*, Secondary *Business*. Done 2026-10-03.
- **Content rights:** "does not contain, show, or access third-party content".
  **Open:** the owner must make this attestation.
- **Age rating:** Edit → answer as in APP_STORE_LISTING.md (expected 4+).
  Done 2026-10-03: the source-matched questionnaire is saved.
- **Privacy policy URL:** `https://nofriction.io/privacy`. **Open:** set it
  once the meeting-app page is published (step 6).

### 4.3 Note the Apple ID
App Information → **General Information → Apple ID**: **`6818838861`**.
That's `ASC_APP_ID` for the Mac upload, and it's the number in the App Store
link for the website button.

### 4.4 App Privacy
Sidebar → **App Privacy** → Get Started. Use the answers in
APP_STORE_LISTING.md:
- tracking: **No**
- data collected by you: declared conservatively as *Other User Content* (and
  on the Mac, *Audio Data* and *Photos or Videos*), for **App Functionality**,
  **not linked** to identity, **not used for tracking**

Then **Publish**. This has to match the iOS privacy manifest.

**Open:** the privacy answers are still a draft. Reconcile them with the
current app (Apple on-device or a user-entered endpoint, on-device
transcription) before publishing.

### 4.5 Pricing and Availability
- **Price:** **Free** (the app is free to download; Pro is the subscription).
- **Availability:** all countries, or the ones you choose. EU countries need
  step 1.5 done.

**Done 2026-10-03 (see closeout).** Free (USD 0.00), **USA only**, automatic expansion to new
territories off. Decide separately before adding countries.

### 4.6 Accessibility (optional, recommended)
Sidebar → **Accessibility**, if shown. The iOS app supports VoiceOver, Dynamic
Type, Dark Interface and sufficient contrast, so you can declare those.

---

## Step 5: Subscriptions

Sidebar → **Monetization → Subscriptions**.

### 5.1 Subscription group
**Create** → Reference name `noFriction Pro`. Under the group's
**Localization**, add English (U.S.): display name `noFriction Pro`.

### 5.2 The two products
In the group → **Create** (do this twice):

| | Monthly | Yearly |
|---|---|---|
| Reference name | `Pro Monthly` | `Pro Yearly` |
| **Product ID** | `com.nofriction.meetings.pro.monthly` | `com.nofriction.meetings.pro.yearly` |
| Duration | 1 month | 1 year |
| US price | $0.99 | $5.99 |
| Localization: display name / description | from APP_STORE_LISTING.md (≤30 / ≤45 chars) | same |

For each product:
- **Subscription Prices** → set the price (Apple fills in other countries; you
  can adjust them).
- **Introductory Offers** → **+** → *Free* → **1 week**, on both products in
  the selected release countries. The paywall reads the eligible offer from
  Apple; local test settings do not create the live offer.
- **Review Information** → a **screenshot of the paywall** (take it in the iOS
  app: Meetings → a meeting → Summarize without Pro, or Settings →
  Subscription) and a review note such as "Unlocks AI notes, summaries and
  follow-up emails."
- **Family Sharing** → your choice. It can be turned on later, but never off.
- **Availability** → same countries as the app.

The products will show "Missing Metadata" until all of that is filled in, then
"Ready to Submit". **The first subscriptions are submitted together with the
first app version** (step 9.2), not on their own.

**Done 2026-10-03 (see closeout).** Group `22437188` (`noFriction Pro`), both products with
en-US names and descriptions, $0.99/month and $5.99/year in the US, a 1-week
free trial on each, USA availability, Family Sharing off, and both paywall
review images uploaded. The group page shows Prepare for Submission while the
older subscription API still reports `MISSING_METADATA`; neither means
approval.

### 5.3 Sandbox testers (to test purchases outside TestFlight)
Users and Access → **Sandbox** → Test Accounts → **+**: a new email that has
never been an Apple ID. On a device: Settings → App Store → Sandbox Account.
TestFlight builds use your real Apple ID with free sandbox purchases instead,
so this is optional.

---

## Step 6: Host the privacy and support pages, and the support inbox

Apple requires a **privacy policy URL** for external TestFlight and for the
App Store, plus a **support URL** for the store listing.

**Open.** Both apps already link to `https://nofriction.io/privacy`,
`https://nofriction.io/contact` and `casey@nofriction.io`. The live
nofriction.io privacy page covers the website only, so it needs the
meeting-app content before submission.

1. Integrate the meeting-app privacy and contact/support pages from `site/`
   with the existing nofriction.io website. See
   [site/README.md](../site/README.md).
2. Check that each page loads with the meeting-app content:
   `curl -sI https://nofriction.io/privacy | head -1` must return `200`. Do the
   same for `/contact`, and open both from the in-app links.
3. If the URLs change, update them in **both apps** and rebuild:
   - Mac: `src/lib/build.ts` (`PRIVACY_URL`, `SUPPORT_URL`, `SUPPORT_EMAIL`)
   - iOS: `ios/NoFriction/App/AppLinks.swift`
4. Send a test email from an outside account to **casey@nofriction.io** and
   confirm it arrives (or change the address in both files above and in the
   site pages).

---

## Step 7: Upload builds

Every upload needs a **new build number**; the scripts bump it for you.

### 7.1 iPhone/iPad
```bash
scripts/release-ios.sh --check --upload   # local config/key-file checks only; changes nothing
scripts/release-ios.sh --upload           # archive → sign → upload
```
- The local check does not verify API access, profile provisioning or App Store
  processing; those require the actual signing/upload flow.
- Status 2026-10-03: build 3 was archived, signed, exported to
  `ios/build/release/export-3/noFriction.ipa` and credential-scanned. It is
  **not uploaded**, and it is now **superseded**: it has no watch app. Upload
  **build 4 or later** (`scripts/release-ios.sh --upload`), which embeds
  `Watch/NoFrictionWatch.app` and scans it. Step 2.5 must be done first.
- App Store Connect → the app → **TestFlight** shows the build as
  *Processing* (5–30 min, then you get an email).
- No export-compliance question appears, because the app declares no
  non-exempt encryption.

### 7.2 Mac
```bash
scripts/release-mas.sh --upload           # sandboxed build → sign → .pkg → upload
```
It needs `src-tauri/embedded.provisionprofile` (step 2.3), `ASC_APP_ID` and the
API key variables (step 3). The `.pkg` is also kept in `dist-mas/`, and
Apple's **Transporter** app can upload it by drag and drop instead.

Status 2026-10-03: 3.6.0 build38 is signed, verified and packaged at
`dist-mas/noFriction-Meetings-3.6.0-38.pkg`. It is **not uploaded**.

---

## Step 8: TestFlight

App Store Connect → the app → **TestFlight**.

1. **Internal testing** (no Apple review, available right away):
   - Internal Testing → **+** → group `Team` → add people with App Store
     Connect access (up to 100).
   - Turn on *Automatic distribution*.
   - Testers install the **TestFlight** app on iPhone, iPad or Mac (macOS 12+)
     and accept the invite.
2. **Test Information** (needed for external testing). Fill in from
   APP_STORE_LISTING.md:
   - beta app description
   - feedback email
   - **privacy policy URL**
   - marketing URL (optional)
   - **Beta App Review notes**: no noFriction account or hosted AI is needed.
     AI path: Apple on-device (needs an Apple Intelligence-capable device on
     iOS 26 / macOS 26 or later, Apple Intelligence on, model downloaded), or
     an OpenAI-compatible endpoint the tester enters (base URL, model,
     optional key of their own). No key is supplied. Also the
     recording-consent notice and how to reach the paywall
   - your contact name, email and phone
3. **External testing** (up to 10,000 testers, Beta App Review usually < 24 h):
   - External Testing → **+** → group `Beta` → add the build → submit for
     review.
   - When approved: invite by email or turn on a **Public Link**.
4. **What to verify in TestFlight** (purchases are free here, and renewals run
   on an accelerated clock):
   - [ ] first-run welcome; permissions requested in context
   - [ ] record a real meeting and see the live transcript; leave and confirm auto-stop offers to stop
   - [ ] choose Apple on-device, or enter an endpoint (base URL, model, optional key) → for a public HTTPS endpoint, the consent prompt shows the destination → Summarize → notes
   - [ ] paywall shows the price, the trial and Restore; **buy** → Pro unlocks; **Restore** on a second device
   - [ ] the trial expires and the subscription renews (accelerated)
   - [ ] Delete with Undo; Strike a word and see the marker; silence in the recording
   - [ ] Mac: the same, plus screen capture permission, Snap, and follow-up email
   - [ ] Apple Watch (needs a real paired watch; see the device checklist in WATCH_APP.md):
     install from the Watch app → Record, lower the wrist for a few minutes, Pause/Resume,
     Stop → the recording reaches the iPhone (even with the iPhone app closed), becomes a
     meeting with a watch badge, is calendar-matched and transcribed; the watch deletes it
     only after the iPhone confirms; Strike silences the imported audio

TestFlight builds expire after **90 days**, so upload a fresh one before then.

---

## Step 9: Submit to the App Store

### 9.1 Version page (once per platform: iOS and macOS)
Sidebar → **iOS App 1.0** (and **macOS App 3.6.0**):
- **Screenshots:**
  - iOS: upload `ios/AppStore/screenshots/iphone-6.9/*.png` to *iPhone 6.9"
    Display* and `ios/AppStore/screenshots/ipad-13/*.png` to *iPad 13"
    Display*. They're already at 1320×2868 and 2064×2752.
    **Done 2026-10-03 (see closeout).** Six of each, processed, in order, with the
    endpoint-only AI-settings image.
  - Mac: 1–10 images at 16:10 (2880×1800 recommended). Capture them from demo
    data using the steps in APP_STORE_LISTING.md (separate macOS user, no real
    data). **Open:** capture them from a current Mac App Store build.
  - Apple Watch (**required** now that the build includes a watch app): in the
    iOS version's *Apple Watch* section upload
    `ios/AppStore/screenshots/watch-46mm/*.png` (416×496, three images).
    **Open.**
- **Promotional text, description, keywords, support URL, marketing URL:**
  from APP_STORE_LISTING.md (character counts are already checked there).
  Done 2026-10-03 for the text fields and the support URL.
- **Version:** `1.0.0` (iOS) / `3.6.0` (Mac) · **Copyright:** `2026 <seller name>`
- **Build:** select the processed build from step 7.

### 9.2 Attach the subscriptions
On the same version page → **In-App Purchases and Subscriptions** → **+** →
select **Pro Monthly** and **Pro Yearly**. **Required** for the first
submission: subscriptions are reviewed together with the app.

### 9.3 App Review Information
- **Sign-in required:** No.
- **Contact:** your name, phone and email. Done 2026-10-03 on both platforms.
- **Notes:** the review notes from APP_STORE_LISTING.md (how to test, the AI
  path: Apple on-device with its device requirements or an endpoint the
  reviewer enters, the recording-consent notice, the paywall). Never put an
  API key in the notes. Done 2026-10-03: endpoint-only notes saved; verify the
  Apple on-device path on the submitted build. **Open:** add the Apple Watch
  paragraph: "The Apple Watch app records a meeting and sends the audio to the
  iPhone app, which transcribes it on the device. Install it from the Watch app
  on the paired iPhone; tap Record, then Stop; the meeting appears in the iPhone
  app's Meetings tab."
- **Attachment** (optional): a short screen recording of record → notes → strike.

### 9.4 Release option and submit
- **Version Release:** *Manually release this version* (recommended, so you
  pick the launch moment). Done 2026-10-03 on both versions.
- **Add for Review** → **Submit to App Review**. iOS and Mac can go in
  together, or one at a time.
- Review usually takes 1–3 days. Answer any message in **App Review** (Resolution
  Center) from App Store Connect.

### 9.5 Common rejection causes, and where we stand

| Guideline | Requirement | Status |
|---|---|---|
| 2.1 App completeness | Reviewer can use every feature | Review notes describe Apple on-device (compatible hardware, iOS/macOS 26+, Apple Intelligence on) and the user-entered endpoint option; verify on the submitted build |
| 3.1.2 Subscriptions | Price, period, trial, Restore, Terms + Privacy links on the paywall | Built into both paywalls |
| 5.1.1 Data collection | Accurate permission strings; ask in context | Done |
| 5.1.2(i) Third-party AI | Ask before sending personal data to an AI service | Consent sheet showing the destination before content goes to a public endpoint |
| 2.3 Accurate metadata | Screenshots and description match the app | Listing written against the code |
| Legal / recording | Users told about recording consent | Notice on first record |

---

## Step 10: After approval

1. **Release** (if manual) → the app goes live within hours.
2. **Website button:** in `site/index.html`, change "Coming soon to the App
   Store" to `https://apps.apple.com/app/id<ASC_APP_ID>` (the comment in the
   file shows where), then republish the site.
3. **Monitor** (there are no analytics in the app by design):
   - crashes: App Store Connect → the app → *Crashes*, or Xcode → Organizer → Crashes
   - ratings and reviews: reply from App Store Connect
   - sales and trials: App Store Connect → *Sales and Trends* and *Subscriptions*
4. **Updates:** bump `MARKETING_VERSION` (iOS) or the version in
   `tauri.conf.json` / `Cargo.toml` / `package.json` (Mac), run the release
   script, create a new version in App Store Connect, and submit.

---

## Order and timing at a glance

Where things stand on 2026-10-03 (details in the
[closeout](APPLE_SETUP_CLOSEOUT_20261003.md)):
- Done: steps 0, 2 and 3; step 5 apart from the optional sandbox testers
  (5.3); step 4 except App Privacy, content rights and the privacy policy URL;
  in step 1, tax and DSA.
- Open in step 1: the bank account (Paid Apps Agreement is Pending User Info)
  and the Small Business Program.
- Open: steps 6, 7, 8 and 10. In step 9 the iOS screenshots, metadata,
  reviewer contact, review notes and release option are saved; the Mac
  screenshots, build selection, attaching the subscriptions and submission
  remain. Device and sandbox purchase testing is still to do.

| When | What | Waits on |
|---|---|---|
| Day 1 | Step 0 decisions; Step 1 (agreements, tax, bank, DSA, Small Business) | Apple / bank verification: 1–5 days |
| Day 1 | Steps 2–4 (App ID, Mac profile, API key, app record, privacy, pricing) | — |
| Day 1–2 | Step 5 (subscriptions); Step 6 (host site, support inbox) | DNS: minutes to hours |
| Day 2 | Step 7 uploads → Step 8 internal TestFlight | Processing: 5–30 min |
| Day 2–3 | External TestFlight | Beta App Review: < 24 h |
| When happy | Step 9 submit | App Review: 1–3 days |
| If going organization | D-U-N-S + account migration **before** step 4 | Up to ~2 weeks |
