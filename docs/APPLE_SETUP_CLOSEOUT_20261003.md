# noFriction Apple setup closeout — 2026-10-03

**Setup and developer validation are in progress; no public release has been submitted.** Apple records, signing profiles, U.S. free-download/subscription configuration, reviewer contacts and iOS screenshot materials are configured. The endpoint-only iOS 1.0.0 build 3 has been archived, signed, exported and scanned. Mac 3.6.0 build38 is signed and packaged with matching source/payload evidence; genuine screenshots are being captured from a separate locally signed MAS copy. Banking, final privacy publication, content-rights attestation and signed-device/subscription acceptance remain gates.

The owner's latest instruction supersedes earlier provider/domain decisions: **Apple on-device or an explicitly user-configured OpenAI-compatible endpoint; no named AI service presets, no built-in cloud transcription, no supplied API keys. Intended website: nofriction.io.**

## Verified configuration

| Item | Result |
|---|---|
| Publisher team | Existing team `C7GCEESE2V`; displayed legal seller is `casey potenzone`. Certificate names do not establish membership type. |
| Bundle ID / Apple bundle record | `com.nofriction.meetings` / `CHQ7ZF73F8` |
| App Store Connect app / SKU | `6818838861` / `NOFRICTION-001` |
| Platforms | iOS `1.0.0`; macOS `3.6.0`, on one app record |
| Version state / release option | Both `PREPARE_FOR_SUBMISSION` / `MANUAL` |
| App name / subtitle | `noFriction: Meeting Notes` / `Record, transcribe, summarize` |
| Categories | Productivity / Business |
| Ordinary metadata | Per-platform descriptions, promotional text, keywords and review notes saved and read back; no app login required |
| App download price | USA base territory, USD **0.00**, one manual price; live readback verified |
| App distribution territories | USA configured explicitly; automatic expansion to new territories disabled. Pricing and distribution availability were configured separately. |
| Reviewer contact | Existing same-team reviewer contact copied and verified on both platform records; no demo login required |
| API access | Existing authorized App Store Connect team key reused; no new key created |

## Subscriptions configured

Group **22437188**, reference/display name **noFriction Pro**. Both products have en-US names/descriptions and the same service level, **1**.

| Product ID | Apple ID | U.S. price / period | Introductory offer | Current state |
|---|---|---|---|---|
| `com.nofriction.meetings.pro.monthly` | `6818839657` | **$0.99 / one month** | **1-week free trial** | `MISSING_METADATA` |
| `com.nofriction.meetings.pro.yearly` | `6818839688` | **$5.99 / one year** | **1-week free trial** | `MISSING_METADATA` |

Fresh API readback confirmed prices, product IDs, durations, localizations and offers. Trials start **2026-10-03**, have one period and no scheduled end date; customer eligibility is controlled by Apple. Subscription territory scope is **USA only**; automatic expansion to new territories is disabled. Family Sharing remains **off**. Apple initially rejected prices/trials until subscription availability existed; configuring USA subscription availability resolved that prerequisite. No agreement was accepted and no product was submitted or released.

A genuine iOS paywall review image was recaptured after removing named providers, from the current app with the local StoreKit fixture. It shows both approved prices and one-week trials, Restore Purchases and legal links. Both subscription review images are uploaded and Apple processing is `COMPLETE` in `nofriction-endpoint-paywall-verified.json`. The group UI shows Prepare for Submission while the older subscription API still reports `MISSING_METADATA`; neither state establishes approval or release readiness. The ordinary store screenshots are separate assets.

The local `ios/NoFriction.storekit` fixture also uses $0.99/$5.99 and `P1W`, but it is separate from the live configuration. Runtime StoreKit reads Apple's localized products and eligible offers.

## Signing profiles and identities

| Profile | Apple profile ID | UUID | Expiration |
|---|---|---|---|
| iOS App Store | `4N7G6ANC55` | `b10c6db7-7a9c-4bd5-949e-3deb98b9e2b6` | 2027-09-22 00:32:54 UTC |
| Mac App Store | `4YXR8G5V39` | `99554d39-3dd1-49c3-872c-1f5e4e216114` | 2027-09-22 00:32:54 UTC |

The iOS profile is ACTIVE and installed in Xcode's provisioning-profile directory, with bytes matching the downloaded profile. The Mac profile is installed at `src-tauri/embedded.provisionprofile`, with application identifier `C7GCEESE2V.com.nofriction.meetings`, matching team, no device list and an unexpired date. Its certificate matches a local valid signing identity. The Apple Distribution and Mac Installer identities are present; Developer ID Application is also present for the separate DMG distribution path. The generated profiles use Apple Distribution certificate record `S2ZF6K23T9`.

The Mac CMS payload signature was verified using OpenSSL with certificate-chain verification disabled. `security cms` hit a sandbox certificate-import restriction during the read-only audit; that error does not establish a bad profile. iOS build 3 archive/export now passed with the installed App Store profile; Apple build processing and real-device acceptance remain separate verification steps.

## Screenshots completed

Six source iPhone screenshots and six source iPad screenshots are installed on **iOS 1.0.0 / en-US**, ordered:

1. `01-record.png`
2. `02-meeting-notes.png`
3. `03-stricken.png`
4. `04-people.png`
5. `05-settings-ai.png`
6. `06-meetings.png`

| Set | Apple set ID | Verified pixels | Result |
|---|---|---|---|
| iPhone 6.9-inch source | `bdb1f3a8-6872-46c7-b323-9b1f519978db` | 1320×2868 | Six `COMPLETE`, correct order and source MD5s |
| iPad 13-inch source | `ffa6692d-19a2-4669-b801-c9dcce08069f` | 2064×2752 | Six `COMPLETE`, correct order and source MD5s |

The iPhone set was uploaded through App Store Connect and then reordered with the public API. The iPad set used Apple's documented reservation → upload → commit → verify API flow. No screenshot was fabricated or edited. All six iPad images were visually inspected: invented contacts use `.example` domains, the provider key field is empty, and content matches the debug sample-data source. See [DemoData.swift](../ios/NoFriction/App/DemoData.swift#L5), [ScreensTests.swift](../ios/NoFrictionUITests/ScreensTests.swift#L31) and [source screenshots](../ios/AppStore/screenshots/).

The fifth AI-settings image in each iOS set was regenerated from the endpoint-only app and replaced, preserving order. Both new assets are `COMPLETE` in `nofriction-endpoint-screenshots-verified.json`. The endpoint-only paywall review image is `COMPLETE` for both products in `nofriction-endpoint-paywall-verified.json`. Earlier provider-branded versions are superseded.

Mac screenshots remain pending a current MAS build. The existing local Mac `.app` is the direct-download flavor, so its screens were not presented as MAS evidence. The paywall review image uses the actual iOS app and local StoreKit products without purchasing or fabricating an entitlement. Screenshot capture/upload does not establish release-build acceptance.

## Developer validation and behavior

- The Mac runtime exposes only Apple and custom AI choices. Four cloud transcription modules were physically removed; local Whisper is the only compiled transcription provider. There is no fixed remote AI URL, automatic key-prefix service selection or startup provider probe.
- Saved named provider selections fail closed without deleting meetings. Mac credentials use Keychain accounts derived from normalized endpoint hashes; old shared custom keys are preserved but require explicit re-entry. Changing a destination clears the previous bound key, consent and model state. Stale key-save dialogs, consent dialogs and returned model lists cannot approve or populate another endpoint. The consent dialog displays the actual destination.
- [Shared AI contract](AI_PROVIDERS.md) explains the current implementation, URL policy, consent, request scope and limitations. Redaction/migration identifiers and inert compatibility helpers are not selectable service presets.
- [Release scripts](../scripts/release-mas.sh) use external checkout storage. The iOS helper supports the installed manual profile. Source policy checks run before builds; signed artifacts are scanned for credential candidates and retired service hosts before export/package/upload. Mac scanning requires exact linked and decoded Tauri JavaScript/HTML/CSS coverage. A static pass is not a proof of absence of encrypted, fragmented or unknown-format secrets.
- Mac validation: 25 AI regression tests passed, plus one local-only transcription test; TypeScript, shell syntax, source policy, scanner self-tests and diff whitespace checks passed. iOS validation: 97 unit tests and six UI scenario executions passed, including fresh settings/paywall captures on phone and iPad and normal/XXXL onboarding.
- iOS signed release: `ios/build/release/noFriction-1.0.0-3.xcarchive` and `ios/build/release/export-3/noFriction.ipa`. IPA SHA-256 `4352b726490cd8913c2d88c344e6ef6f1e71dd874c1ffbe82396d75cf47c965c`. Archive and IPA scans passed with zero credential candidates and zero retired hosts; strict signature verification passed. See `ios/build/release/release-receipt-3.json` and `source-sha256-3.json`.
- Mac 3.6.0 build38 signed distribution package: `dist-mas/noFriction-Meetings-3.6.0-38.pkg`, SHA-256 `1255f5e593ea416fe275c2a2fd702394f4b680b3f62adcbc726aa7b5780a6f4c`. Strict app signature/sandbox/profile verification and both embedded-asset scans passed. Expanded package payload matches the audited app. The 174-file source inventory SHA-256 is `fb8bf11635f3cddfe45716d047bee5b1772abba158962753d47a50b09112f1cf`. Full evidence is in companion `NOFRICTION_DEVELOPER_HANDOFF_20261003.md` and `NOFRICTION_MAC_RELEASE_38.json`.
- Local launch of the production-signed app was rejected by macOS because its production profile is ineligible for direct execution (`amfid -413`). A separate supported local-test copy at `dist-mas/local-test-38/noFriction Meetings.app` has matching MAS code: all 43 file-backed Mach-O sections, Info.plist and icon are identical. Only signatures, embedded profile and local signing entitlements differ. It passes its credential/retired-host scan. The original distribution app/package remain unchanged. Parent owns launch/screenshot verification; no private meeting contents were copied or fabricated.

## Website and App Store declarations

The owner identified **nofriction.io**, whose live CCA Innovations OU/Casey site publishes `casey@nofriction.io`. Both apps now use `https://nofriction.io/privacy`, `https://nofriction.io/contact` and that email; Terms remain Apple's standard EULA. Mailbox delivery is untested. Root saved the support URL and endpoint-only listing/review notes to both platform records; see `nofriction-endpoint-metadata-verified.json`.

The existing public `.io` privacy policy covers the consulting website and still needs the meeting-app supplement. Existing repository `site/index.html`, `privacy.html`, `support.html`, `terms.html` and `README.md` now match the endpoint-only behavior, local transcription and contact. Branding/layout were preserved. **No website was deployed, no DNS changed, and no live app-policy publication is claimed.** The website owner should integrate this ready source with the existing site and verify the extensionless routes.

The previous Gemini-specific age decision is superseded by its removal. Root saved the source-matched age questionnaire with no Gemini age override. The privacy audit remains a draft disclosure recommendation requiring final reconciliation/publication; see `NOFRICTION_PRIVACY_AGE_REVIEW.md`. Content-rights attestation requires the owner. The historical nine-category audit is not evidence of a finalized or published App Privacy declaration.

## Remaining owner actions

- **Banking / Paid Apps:** the Business portal shows Paid Apps Agreement **Pending User Info**, no bank account and an **Add Bank Account** banner. The account holder must enter/verify the payout bank account and resolve the remaining paid-agreement requirements. Free Apps Agreement is Active; US W-9 and DSA are already Active (October 1). Do not repeat those completed steps unnecessarily.
- Integrate the meeting-app privacy/support supplement with the existing nofriction.io website and verify receipt of an external email to casey@nofriction.io. Domain choice is resolved; publication and mailbox delivery are not.
- USA app and subscription availability are configured, with automatic territory expansion off. Decide separately before expanding countries or enabling Family Sharing. The approved prices/trial/team do not need another decision.

## Remaining developer and tester work

- Publish the reviewed meeting-app policy/support supplement at the confirmed destination; verify rendered content, extensionless `/privacy` and `/contact`, and in-app links. Complete final privacy publication and owner content-rights attestation. The source-matched age questionnaire has been updated.
- Capture accurate Mac screenshots from a current MAS build, using an empty state or the approved test fixture. The final iOS paywall is attached to both subscriptions and both image assets are `COMPLETE`; finish the remaining declarations/build acceptance before submission.
- Finish genuine Mac screenshots; upload the verified iOS/Mac candidates only within root authorization, verify Apple processing, and attach the exact builds. iOS build 3 is signed/exported; its archive does not establish Apple upload/processing or physical-device acceptance.
- On real hardware, test microphone/system-audio capture, locked/background recording, transcription, permissions, screenshots, calendar, auto-stop, Delete/Strike, export, notes and follow-up drafts. Test Apple on-device/local AI offline after downloads with an already verified subscription.
- Test sandbox purchase, trial expiry/renewal, cancellation, entitlement refresh and cross-platform restore with the same Apple Account. Verify displayed prices, offer eligibility and Terms/Privacy links. Source inspection and screenshot upload do not establish these outcomes.
- Attach initial subscriptions to the first app submission only after the missing materials and acceptance checks are complete. Public release remains manual; nothing was submitted or released during setup.

## Evidence

Nonsecret API receipts are retained in the Dropbox CriticalTwin workspace at `.local/releases/apple-setup-20261003/`:

- `nofriction-subscriptions-verified.json` and per-product final readbacks: exact live prices/offers/localizations and the original pre-upload state; later endpoint-only image receipts supersede it.
- `nofriction-free-price-verified.json`: free-download schedule; `nofriction-us-app-availability-readback.json` records the later explicit USA availability and disabled automatic expansion.
- `nofriction-metadata-verified.json`: platform version IDs, ordinary metadata hashes, unset URLs and manual release state; `nofriction-review-contact-verified.json` records the later verified reviewer contacts with sensitive values omitted.
- `nofriction-screenshots-verified.json` and per-set final readbacks: all twelve complete assets, source hashes, dimensions and order.
- `nofriction-builds-closeout.json`: zero noFriction builds at the recorded API readback.
- `nofriction-developer-prep-verified.json` and `nofriction-profile-regression-results.json`: earlier preparation evidence. The historical Deepgram opt-out fix/test is superseded by complete removal of cloud transcription.
- `nofriction-endpoint-screenshots-verified.json`, `nofriction-endpoint-paywall-verified.json`: current endpoint-only settings images and both complete subscription review images. Old paywall capture receipts are historical. `ios-endpoint-validation/evidence.json` in the noFriction local release folder records the six new UI scenario executions.
- `nofriction-ios-store.json`, `nofriction-mac-profile.json`: profile identities and installation evidence. Provisioning files remain in their existing private local locations.

Apple API contract: [official OpenAPI specification](https://developer.apple.com/sample-code/app-store-connect/app-store-connect-openapi-specification.zip), version 4.5; [asset-upload workflow](https://developer.apple.com/documentation/appstoreconnectapi/uploading-assets-to-app-store-connect). Temporary upload URLs and authentication headers are excluded from saved screenshot receipts. Source trace: [Apple setup](APPLE_SETUP.md), [App Store listing](APP_STORE_LISTING.md), [iOS StoreKit](../ios/NoFriction/Store/Store.swift), [Mac notes view](../src/components/MeetingNotesPanel.tsx).
