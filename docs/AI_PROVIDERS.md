# AI execution: shared contract (iOS + Mac)

Updated 2026-10-03 for the owner's instruction: **no built-in AI services or API keys**. This supersedes the former named-provider presets and key-autodetection flow.

## Supported choices

| Choice | Endpoint | Credential | Execution |
|---|---|---|---|
| Apple on-device | No network endpoint | None | Foundation Models on compatible iOS/macOS 26+ hardware with Apple Intelligence and its model available |
| Your custom AI endpoint | User enters the full compatible base URL and model | Optional, supplied by the user | OpenAI-compatible chat-completions protocol on the user's local server or selected HTTPS service |

The custom URL, model and key begin empty. There is no default remote URL, named service picker, key-prefix routing, startup network probe or hosted noFriction AI. A fresh installation may use available Apple on-device text AI; otherwise AI asks for configuration. Transcription remains on-device: Mac local Whisper, and the iOS platform's local transcription path. Mac cloud-transcription implementations have been removed from the source and module graph. Local model downloads remain supported.

## Configuration and migration

- Enter an endpoint and model explicitly. Saving configuration or a key performs no network validation. The Mac offers an explicit model-list refresh for the configured endpoint only.
- AI calls require a supported, valid selection. Previously saved named providers are unavailable; they do not fall back to another network service. Meeting records and historical settings remain preserved.
- Old provider identifiers may remain in migration/redaction tests or inert historical storage. They cannot select a compiled service preset. Protocol/model compatibility heuristics do not supply endpoints or keys.
- Changing the custom endpoint invalidates its consent and model selection/cache and deletes its old Keychain credential. If deletion fails, the endpoint change fails before committing the new URL. Saving the same normalized URL retains its existing credential. Credentials are never silently reused at another destination.
- iOS additionally requires the stored custom credential's endpoint binding to match before reading it. Mac Keychain accounts are derived from a hash of the normalized endpoint, so even a stale request snapshot cannot read another destination's key. Old shared custom credentials remain preserved but require explicit re-entry. Mac key saves check the expected destination under the configuration lock; model-list responses are discarded after a destination change. Both platforms store user-supplied credentials in Keychain and never return the full key to UI or logs.

## URL and content policy

Public endpoints require HTTPS. Plain HTTP is allowed only for approved loopback/private-network hosts. The user selects the destination; the app never infers a service from a key. Validation rejects invalid URLs and embedded URL credentials. iOS also rejects query/fragment components.

Before sending meeting content to a public custom endpoint, the app requests consent. Revoking consent blocks subsequent content requests. Local-network endpoints can send content to another machine on that network; “local” does not mean every request stays inside the device.

Depending on the requested feature, content may include transcripts, meeting titles, attendee names/email/company/rosters, notes and selected images/screenshots. The selected endpoint's operator controls its retention and use. noFriction does not proxy these requests or supply a service account. Apple on-device inference runs on the device. No blanket promise about third-party retention follows from this contract.

The generic request path sends to `{user base}/chat/completions` with the user-entered model and optional bearer credential. Existing response limits, timeout, context fitting, parameter compatibility and redaction remain in force. The unused legacy Anthropic protocol adapter in Mac source has no selectable provider or fixed service endpoint.

## Release checks

Run `python3 scripts/check-ai-provider-policy.py` before release. It checks both provider tables, the empty custom endpoint, local-only Mac transcription, retired service URLs, compile-time credential injection and environment-file resource declarations. Behavioral tests cover fresh installation, saved legacy providers, explicit endpoint requirements, consent and credential invalidation.

Both `scripts/release-ios.sh` and `scripts/release-mas.sh` run that guard. The scripts then scan the actual signed app before export/package/upload with `scripts/scan-release-credentials.py --reject-retired-services`. Mac scans also decode the Tauri asset cache and require exact linked JavaScript/HTML/CSS coverage. A failed or incomplete scan blocks packaging/upload. Exported iOS IPAs receive a second scan. Mac scanning needs a Python interpreter with Brotli (`NF_AUDIT_PYTHON` may select one).

The scanner reports redacted credential candidates, artifact hashes and retired-service host findings. A static pass does not prove the absence of encrypted, fragmented or unknown-format credentials. Source loading-path review and actual request tests remain necessary. Old artifacts from before this change do not establish the current contract.

## Subscription and legal links

noFriction Pro uses StoreKit products `com.nofriction.meetings.pro.monthly` and `com.nofriction.meetings.pro.yearly`. Approved U.S. terms are $0.99/month, $5.99/year, each with an eligible one-week introductory trial. Runtime displays Apple's localized products and eligibility. Recording/transcription remain free; gated AI features require verified Pro entitlement in store builds. Debug StoreKit fixtures do not establish live purchase acceptance.

Owner-approved runtime destinations are `https://nofriction.io/privacy`, `https://nofriction.io/contact` and `casey@nofriction.io`; Terms remain Apple's standard EULA. The current public privacy page describes the website and still needs a meeting-app supplement matching this contract before public submission. No website was deployed in this change; mailbox delivery has not been tested.

Source: `src-tauri/src/ai/{providers,config,commands,client}.rs`, `src-tauri/src/transcription/mod.rs`, `src/features/settings/AIProviderSettings.tsx`, `ios/NoFriction/AI/{AIProvider,AISettings,AIClient}.swift`. See the dated developer handoff for test and artifact evidence.
