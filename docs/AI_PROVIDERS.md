# AI providers: shared spec (iOS + Mac)

Users bring their own API key. We ship **no keys**, run **no servers**, and
never proxy traffic. Both apps implement this spec identically, so a user who
knows one app knows the other.

## Presets

| id | Name | Protocol | Base URL | Key prefix (auto-detect) | Get-a-key URL |
|---|---|---|---|---|---|
| `openai` | OpenAI (default) | openai | `https://api.openai.com/v1` | `sk-proj-`, `sk-svcacct-`, `sk-` (fallback) | https://platform.openai.com/api-keys |
| `anthropic` | Anthropic Claude | anthropic | `https://api.anthropic.com/v1` | `sk-ant-` | https://console.anthropic.com/settings/keys |
| `gemini` | Google Gemini | openai | `https://generativelanguage.googleapis.com/v1beta/openai` | `AIza` | https://aistudio.google.com/apikey |
| `xai` | xAI Grok | openai | `https://api.x.ai/v1` | `xai-` | https://console.x.ai |
| `groq` | Groq | openai | `https://api.groq.com/openai/v1` | `gsk_` | https://console.groq.com/keys |
| `openrouter` | OpenRouter | openai | `https://openrouter.ai/api/v1` | `sk-or-` | https://openrouter.ai/keys |
| `mistral` | Mistral | openai | `https://api.mistral.ai/v1` | (none) | https://console.mistral.ai/api-keys |
| `deepseek` | DeepSeek | openai | `https://api.deepseek.com/v1` | (none; `sk-` is ambiguous with OpenAI) | https://platform.deepseek.com/api_keys |
| `perplexity` | Perplexity | openai | `https://api.perplexity.ai/router/v1` | `pplx-` | https://www.perplexity.ai/settings/api |
| `together` | Together AI | openai | `https://api.together.xyz/v1` | (none) | https://api.together.ai/settings/api-keys |
| `ollama` | Ollama (local) | openai | `http://localhost:11434/v1` | no key | https://ollama.com |
| `lmstudio` | LM Studio (local) | openai | `http://localhost:1234/v1` | no key | https://lmstudio.ai |
| `custom` | Custom (OpenAI-compatible) | openai | user-entered | optional | — |
| `apple` | Apple on-device | foundation-models | — | no key | — (iOS/macOS 26+, Apple Intelligence on) |

Verify each base URL against the provider's current docs when implementing.
If one has moved, update this table in the same change.

Checked 2026-09-28: Perplexity moved its OpenAI-compatible API to the
"Router" (`/router/v1/chat/completions`, `/router/v1/models`; model ids like
`perplexity/sonar`), so the base URL above changed. Gemini's compatibility
endpoint lists models without the `models/` prefix (e.g. `gemini-3.8-flash`).
Anthropic's `GET /v1/models` returns `max_input_tokens` and
`capabilities.image_input.supported`, which the Mac app uses for context
fitting and the vision picker. Newer Claude models (Opus 5, Sonnet 5, Fable)
reject `temperature`; the Mac adapter only sends it to generations that accept
it and retries once without it on a 400 that names it.

Mac implementation: `src-tauri/src/ai/` (presets, detection, URL policy,
redaction in `providers.rs`; adapters and guardrails in `client.rs`).
The Mac implements the `apple` preset through the Swift bridge
(`src-tauri/swift/NoFrictionBridge/AppleModel.swift`, Rust side
`src-tauri/src/store.rs`): `LanguageModelSession` with the system messages
as instructions, 4,096-token context, text only (no vision). It is the
**key-less fallback**: when no text provider is selected and
`SystemLanguageModel.default.availability == .available` (macOS 26+, Apple
Intelligence on), text features use it automatically. It is local (no
consent dialog). FoundationModels is weak-linked, so the app still launches
on macOS 12–15, where the preset reports `requires_macos_26`.

Endpoint quirks (checked 2026-09-28):
- **OpenRouter**: `GET /models` is public (200 without a key), so it can't validate a
  key. Validate with `GET https://openrouter.ai/api/v1/key`, then list `/models`.
- **Perplexity**: chat is `POST https://api.perplexity.ai/chat/completions` (no `/v1`).
  The authenticated model list is `GET https://api.perplexity.ai/v1/models` and lists
  Agent API models, so validate there and offer a static list (`sonar`, `sonar-pro`,
  `sonar-reasoning-pro`).
- **Gemini**: a bad key returns **400** "Please pass a valid API key" (not 401); treat
  it as a wrong key. Model ids come back as `models/…`; strip the prefix.
- **Together**: `/models` returns a bare JSON array, not `{data:[…]}`.
- **Anthropic**: `/v1/models` returns `max_input_tokens` per model; use it as the
  context window. Current Claude models reject `temperature` and think by default,
  so don't send `temperature`, and leave headroom in `max_tokens`.
- **Ollama**: older builds lack `/v1/models`; fall back to `/api/tags`.

## Paste-a-key flow (the main UX)

1. One field: **"Paste your API key"**. Trim whitespace, quotes, and a leading `Bearer `.
2. Detect the provider from the prefix (table order; longest prefix wins). If
   no prefix matches, show a provider picker. For `sk-`, assume OpenAI; if
   OpenAI's `/models` returns 401, try DeepSeek once before reporting failure.
3. **Validate** by listing models (`GET {base}/models`; Anthropic
   `GET /v1/models` with `x-api-key` + `anthropic-version: 2023-06-01`). Show
   one of: ✓ connected · wrong key (401/403) · no credit / rate-limited (402/429)
   · can't reach (network) · other (status + a short message).
4. Pick a default model from the returned list: the first match from the preset's
   preference list, else the first chat-capable model. The user can change it
   from a picker (with a free-text override for custom endpoints).
5. Save the key in the **Keychain**, then show the consent sheet (below) if this
   provider hasn't been approved yet.
6. Users can save keys for several providers and pick one **active text
   provider** and one **active vision provider** (vision falls back to text if
   that model accepts images, else vision features are hidden).

## Protocols

- **openai**: `POST {base}/chat/completions`, `Authorization: Bearer <key>`,
  body `{model, messages, max_tokens, temperature, stream:false}`. Some newer
  OpenAI models reject `max_tokens` and want `max_completion_tokens`, or accept
  only the default temperature. On a 400 that names the parameter, retry once
  with `max_completion_tokens` and without `temperature`, then remember that
  for the model.
- **anthropic**: `POST {base}/messages`, headers `x-api-key`,
  `anthropic-version: 2023-06-01`. `system` goes in its own top-level field,
  and `max_tokens` is required.
- **foundation-models**: Apple `LanguageModelSession`, on-device only.
- Images (vision): OpenAI-style `image_url` with a `data:` URL; Anthropic
  `image` block with base64 source.

## Guardrails (every provider)

- **Always** send `max_tokens` (Castle lesson: servers keep generating after we disconnect).
- Fit prompts to the model's context window. Default to 32K tokens when unknown, at
  about 3.2 chars/token. Trim the longest message in the middle, with a marker.
- Never send `response_format` / JSON-schema mode. Ask for JSON in the prompt and parse leniently.
- Timeouts: connect 15s, total `60s + max_tokens/8`s.
- Cap response bodies at 4 MB.
- Cloud presets are **HTTPS only**. `http://` is allowed only for `localhost`,
  `127.0.0.1`, `*.local`, RFC1918 and Tailscale `100.64.0.0/10` / `*.ts.net` hosts
  (custom/local presets).
- **Never log keys.** Redact anything matching a key prefix, and `Bearer …`, in
  logs and error strings. Never return a key to the UI; only
  `{configured: true, last4}`.

## Storage

- Keys live only in the Keychain: service `com.nofriction.meetings.ai`,
  account = provider id. iOS uses `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`,
  not synchronizable. The Mac uses a generic password in the login keychain.
- Non-secret config (active provider, model per provider, custom base URL,
  consent flags) lives in normal settings.
- Migrate any key already stored in SQLite / `.env` into the Keychain on first
  launch, then delete the plaintext copy.

## Consent (App Review 5.1.2(i))

Show this before the first request to any **cloud** provider (not local, not
Apple on-device). Store it per provider:

> **Send meeting content to {Provider}?**
> To write notes, summaries and emails, noFriction sends the transcript, the
> meeting title, attendee names and (for screen features) screenshots to
> {Provider} using your API key. {Provider}'s privacy policy and terms apply.
> Nothing is sent to noFriction; we have no servers.
> [Allow] [Not now]

Settings shows a "What leaves this device" line for the active provider, and a
way to revoke consent.

## Licensing (StoreKit 2, on-device, no server)

- Subscription group **noFriction Pro**. Products:
  `com.nofriction.meetings.pro.monthly`, `com.nofriction.meetings.pro.yearly`.
  The trial is an introductory offer configured in App Store Connect.
- **Free:** recording, transcription, calendar and people, photos/screens, export.
- **Pro:** AI notes, summaries, action items, follow-up emails, AI chat, and
  AI briefings.
- Entitlement = any verified, unrevoked transaction for a Pro product in
  `Transaction.currentEntitlements`. Listen to `Transaction.updates`. Restore
  via `AppStore.sync()`.
- The paywall shows price, period, trial terms, Restore Purchases, and links to
  Terms (Apple standard EULA:
  https://www.apple.com/legal/internet-services/itunes/dev/stdeula/) and the
  Privacy Policy.
- Debug builds may use a local `.storekit` configuration. There is **no**
  bypass flag in Release builds.

Mac implementation (Mac App Store build only, `--features mas`):
- StoreKit 2 lives in `src-tauri/swift/NoFrictionBridge/Store.swift` (compiled
  with `-DNF_STOREKIT`); Rust side `src-tauri/src/store.rs`, gate in
  `src-tauri/src/entitlement.rs`.
- `entitlement::require_pro()` runs in `ai::client::complete`, which every
  LLM call goes through (user-invoked and background). Without Pro it
  returns `PRO_REQUIRED: …`; `withAiConsent()` in `src/lib/ai.ts` turns that
  into the paywall (`src/components/PaywallModal.tsx`) and retries after a
  purchase. Settings → Subscription shows status, Restore Purchases and
  "Manage subscription" (opens https://apps.apple.com/account/subscriptions;
  macOS has no in-app manage sheet).
- The Developer ID (DMG) build has no gating: the check is compiled out.
- A `.storekit` configuration only works when the app is launched from an
  Xcode scheme, which a Tauri app isn't; test purchases with the sandbox via
  TestFlight instead (see docs/MAC_APP_STORE_BUILD.md).
