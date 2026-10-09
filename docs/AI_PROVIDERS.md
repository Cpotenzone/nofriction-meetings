# AI execution: shared contract (iOS + Mac)

Updated 2026-10-03 for the owner's instruction: **no built-in AI services or API keys**. Updated 2026-10-09 (owner decision): **named provider presets** are back as a convenience layer over the one user-entered endpoint; the architecture below does not change.

## Supported choices

| Choice | Endpoint | Credential | Execution |
|---|---|---|---|
| Apple on-device | No network endpoint | None | Foundation Models on compatible iOS/macOS 26+ hardware with Apple Intelligence and its model available |
| Your custom AI endpoint | User enters the full compatible base URL and model, or picks a preset that fills them in | Optional, supplied by the user | OpenAI-compatible chat-completions protocol on the user's local server or selected HTTPS service |

The custom URL, model and key begin empty. There is no default remote URL, key-prefix routing, startup network probe or hosted noFriction AI. A fresh installation may use available Apple on-device text AI; otherwise AI asks for configuration. Transcription remains on-device: Mac local Whisper, and the iOS platform's local transcription path. Mac cloud-transcription implementations have been removed from the source and module graph. Local model downloads remain supported.

## Provider presets (UI convenience, same invariants)

The AI settings show text-only cards: **Apple on-device**, **ChatGPT (OpenAI)**, **Anthropic (Claude)**, **Grok (xAI)**, **Mistral** and **Custom endpoint**. A preset is static data (name, base URL, default model, model hint, key-page URL, one-line note) kept in exactly two places, `src-tauri/src/ai/providers.rs::ENDPOINT_PRESETS` and `ios/NoFriction/AI/AIProvider.swift::AIPreset.all`, with the documentation source cited beside each entry. The Mac UI fetches the table from the backend (`ai_list_presets`); it carries no URL of its own.

| Preset | Base URL | Default model | Also | Key page |
|---|---|---|---|---|
| ChatGPT (OpenAI) | `https://api.openai.com/v1` | `gpt-6-luna` | `gpt-6.1-sol`, `gpt-6-astra` | platform.openai.com/api-keys |
| Anthropic (Claude) | `https://api.anthropic.com/v1` | `claude-sonnet-5-5` | `claude-haiku-5-5`, `claude-opus-5-5` | platform.claude.com/settings/keys |
| Grok (xAI) | `https://api.x.ai/v1` | `grok-4.7` | `grok-4.3` | console.x.ai |
| Mistral | `https://api.mistral.ai/v1` | `mistral-large-latest` | `mistral-small-latest` | console.mistral.ai/api-keys |

What a preset does and does not do:

- Choosing a card only fills the base URL and default model in the form and clears the key field. The user pastes their own key, saves, and the result is an ordinary custom endpoint: same `custom` provider id, same Keychain binding to the normalized URL, same consent rule (every preset is a public HTTPS host, so consent is always required before meeting content is sent). Nothing is selected, saved or contacted until the user clicks.
- The preset "match" is derived from the saved URL, never stored. It supplies the display name (`ChatGPT (OpenAI)` instead of `Custom (OpenAI-compatible)`) and the per-service heuristics (context window, vision, `max_completion_tokens` for OpenAI reasoning models, thinking headroom for Claude 5 through Anthropic's compatibility layer). Editing the URL to a proxy drops the match; the connection keeps working as plain custom.
- Switching cards and saving changes the endpoint, which deletes the previous key from the Keychain and clears consent and the model selection (`change_endpoint` on the Mac, `AISettings.save` on iOS). A key never travels to another host.
- Anthropic needs no native adapter: its OpenAI SDK compatibility layer (`https://api.anthropic.com/v1/`, Bearer auth, `/chat/completions`) is used. Anthropic documents it as a compatibility layer rather than its primary API; if that ever changes, the unused `Protocol::Anthropic` adapter in `client.rs` can be wired in. xAI documents `/v1/chat/completions` as deprecated in favour of `/v1/responses` but keeps it available.
- "Get a key" links open the provider's key page in the default browser; the app never creates accounts or keys.

**Test connection.** The only request that can happen before consent. It POSTs the single user message `Hi` with `max_tokens: 1` (or `max_completion_tokens: 1` for models that require it) to the *saved* endpoint with the *saved* key, and reports the result in plain words: connected, wrong key (401/403), no credit or rate limit (402/429), no `/chat/completions` at that URL (404), unknown model, or unreachable (network error, with the key redacted from the message). It carries no meeting content, so it does not need consent; it runs only from the explicit "Test connection" button, never at startup, on save or on preset selection. Saving a connection still sends nothing.

**Policy guard.** `scripts/check-ai-provider-policy.py` allows exactly this curated table (same ids, hosts and default models on both platforms) and fails on: any credential-shaped literal, `apiKey = "…"` or Bearer literal in source; a provider or preset marked default/active/selected, or a card selection that does not start empty; any AI-service host outside the two preset tables (retired hosts anywhere); network calls from startup, save or card selection paths; retired transcription modules, compile-time credential injection, bundled `.env` files, and network or speech APIs in the watch app. `scripts/scan-release-credentials.py` still fails the signed artifact on credential candidates and retired hosts; the four preset hosts are inventoried in the receipt as static preset data.

## Configuration and migration

- Enter an endpoint and model explicitly, or pick a preset card that fills them in. Saving configuration or a key performs no network validation. The explicit "Test connection" button sends the one-word probe described above; the Mac also offers an explicit model-list refresh for the configured endpoint only.
- AI calls require a supported, valid selection. Previously saved named providers are unavailable; they do not fall back to another network service. Meeting records and historical settings remain preserved.
- Old provider identifiers may remain in migration/redaction tests or inert historical storage. They cannot select a compiled service preset. Protocol/model compatibility heuristics do not supply endpoints or keys.
- Changing the custom endpoint invalidates its consent and model selection/cache and deletes its old Keychain credential. If deletion fails, the endpoint change fails before committing the new URL. Saving the same normalized URL retains its existing credential. Credentials are never silently reused at another destination.
- iOS additionally requires the stored custom credential's endpoint binding to match before reading it. Mac Keychain accounts are derived from a hash of the normalized endpoint, so even a stale request snapshot cannot read another destination's key. Old shared custom credentials remain preserved but require explicit re-entry. Mac key saves check the expected destination under the configuration lock; model-list responses are discarded after a destination change. Both platforms store user-supplied credentials in Keychain and never return the full key to UI or logs.

## URL and content policy

Public endpoints require HTTPS. Plain HTTP is allowed only for approved loopback/private-network hosts. The user selects the destination; the app never infers a service from a key. Validation rejects invalid URLs and embedded URL credentials. iOS also rejects query/fragment components.

Before sending meeting content to a public custom endpoint, the app requests consent. Revoking consent blocks subsequent content requests. Local-network endpoints can send content to another machine on that network; “local” does not mean every request stays inside the device.

Depending on the requested feature, content may include transcripts, meeting titles, attendee names/email/company/rosters, notes and selected images/screenshots. The selected endpoint's operator controls its retention and use. noFriction does not proxy these requests or supply a service account. Apple on-device inference runs on the device. No blanket promise about third-party retention follows from this contract.

The generic request path sends to `{user base}/chat/completions` with the user-entered (or preset-filled) model and optional bearer credential. Existing response limits, timeout, context fitting, parameter compatibility and redaction remain in force. The unused legacy Anthropic protocol adapter in Mac source has no selectable provider or fixed service endpoint; the Anthropic preset uses the OpenAI-compatible path like every other preset.

## Release checks

Run `python3 scripts/check-ai-provider-policy.py` before release. It checks both provider tables and both preset tables (see "Provider presets"), the empty custom endpoint, local-only Mac transcription, retired service URLs, credential literals, startup/save network paths, compile-time credential injection and environment-file resource declarations. Behavioral tests cover fresh installation, saved legacy providers, explicit endpoint requirements, preset switching (old key and consent forgotten), the connection test against a loopback stub (success, 401, network error) and consent and credential invalidation.

Both `scripts/release-ios.sh` and `scripts/release-mas.sh` run that guard. The scripts then scan the actual signed app before export/package/upload with `scripts/scan-release-credentials.py --reject-retired-services`. Mac scans also decode the Tauri asset cache and require exact linked JavaScript/HTML/CSS coverage. A failed or incomplete scan blocks packaging/upload. Exported iOS IPAs receive a second scan. Mac scanning needs a Python interpreter with Brotli (`NF_AUDIT_PYTHON` may select one).

The scanner reports redacted credential candidates, artifact hashes and retired-service host findings. A static pass does not prove the absence of encrypted, fragmented or unknown-format credentials. Source loading-path review and actual request tests remain necessary. Old artifacts from before this change do not establish the current contract.

## Subscription and legal links

noFriction Pro uses StoreKit products `com.nofriction.meetings.pro.monthly` and `com.nofriction.meetings.pro.yearly`. Approved U.S. terms are $0.99/month, $5.99/year, each with an eligible one-week introductory trial. Runtime displays Apple's localized products and eligibility. Recording/transcription remain free; gated AI features require verified Pro entitlement in store builds. Debug StoreKit fixtures do not establish live purchase acceptance.

Owner-approved runtime destinations are `https://nofriction.io/privacy`, `https://nofriction.io/contact` and `casey@nofriction.io`; Terms remain Apple's standard EULA. The current public privacy page describes the website and still needs a meeting-app supplement matching this contract before public submission. No website was deployed in this change; mailbox delivery has not been tested.

Source: `src-tauri/src/ai/{providers,config,commands,client}.rs`, `src-tauri/src/transcription/mod.rs`, `src/features/settings/AIProviderSettings.tsx`, `ios/NoFriction/AI/{AIProvider,AISettings,AIClient}.swift`. See the dated developer handoff for test and artifact evidence.
