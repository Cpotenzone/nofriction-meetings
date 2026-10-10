## gstack (REQUIRED — global install)

**Before doing ANY work, verify gstack is installed:**

```bash
test -d ~/.claude/skills/gstack/bin && echo "GSTACK_OK" || echo "GSTACK_MISSING"
```

If GSTACK_MISSING: STOP. Do not proceed. Tell the user:

> gstack is required for all AI-assisted work in this repo.
> Install it:
> ```bash
> git clone --depth 1 https://github.com/Cpotenzone/gstack.git ~/.claude/skills/gstack
> cd ~/.claude/skills/gstack && ./setup --team
> ```
> Then restart your AI coding tool.

Do not skip skills, ignore gstack errors, or work around missing gstack.

Using gstack skills: After install, skills like /qa, /ship, /review, /investigate,
and /browse are available. Use /browse for all web browsing.
Use ~/.claude/skills/gstack/... for gstack file paths (the global path).

## Secrets, deploys, and what NOT to commit

The Targon persistent-volume deploy is **decommissioned**. Production ships
via GitHub Actions → SSH → GCE (`.github/workflows/production_deploy.yml`).
Any reference to `/pie/criticalasset/*` paths, runtime file-writes that
"need persistent storage," or Targon container assumptions is stale — delete
it, don't work around it.

### API key handling (non-negotiable)

- **Never hardcode an API key as a fallback.** Not `process.env.X || 'AIza...'`,
  not `apiKey = 'fc-...'`. If the env var is missing, fail loud. A prior leak
  required a history rewrite — don't reintroduce the pattern.
- **Local dev**: keys live in `criticalasset/secrets.json` (gitignored).
  Callsites read `path.join(process.cwd(), 'secrets.json')` with
  `process.env.X` as the fallback. Match the pattern in
  `criticalasset/app/api/admin/scrapers/firecrawl/route.js::getFirecrawlKey()`.
- **Production**: keys come from GitHub Actions secrets, forwarded to the
  GCE box by the deploy workflow into `/opt/criticalasset/criticalasset/.env.local`.
  Add a new key by: (1) `gh secret set KEY_NAME`, (2) extend the `env:` block
  and `envs:` whitelist and the heredoc in `production_deploy.yml`.
- **Never** restore a UI-based write endpoint for secrets. The POST to
  `/api/settings` was removed because stateless deploys lose the file on
  every redeploy. `/api/settings` is GET-only (status display).

### What must stay out of the repo

The root `.gitignore` already covers these; don't re-add them:

- `node_modules/`, `.next/`, `dist/`, `build/`, `out/`
- `.env`, `.env.*` (except `.env.example` if you create one), `secrets.json`, `*.pem`, `*.key`
- `*.tar.gz`, `*.tgz` — historically the repo collected 20+ `app_*_update.tar.gz`
  "snapshot" archives. Don't commit new ones; use git tags or releases instead.
- `.DS_Store`

If something belongs tracked but is caught by the above (rare), use a targeted
`!pattern` override rather than weakening the base rule.

### If you find a leaked secret

1. Revoke / rotate the key **first** (dashboard for the provider).
2. Remove the hardcoded value and any callers that assume it.
3. For history scrub, use `git filter-repo --replace-text` with a
   `old==>REDACTED_NAME` list. Back up with a local `backup-pre-scrub`
   branch and tag before running. Never run filter-repo without confirming
   force-push is acceptable to the user.

## Cross-subsystem schema ownership

Some tables are shared across subsystems. The convention: **schema ownership lives
in `criticalasset/app/api/admin/migrate/route.js`** (the centralized migration
endpoint), and consuming subsystems read/write but NEVER ALTER.

| Table | Schema owned by | Consumed by |
|-------|-----------------|-------------|
| `findings` | `app/api/admin/migrate/route.js` | engine-v2 (osint findings), water-control, water-containment (audit→twin feedback loop, `provenance JSONB`), inspections, AI insights |
| `obligations` | `app/api/admin/migrate/route.js` | findings → obligations chain, water-containment v1.1+ |
| `insurance_signals` | `app/api/admin/migrate/route.js` | reports, scoring engine |
| `contractors`, `properties`, `inspection_events`, `normalized_permits`, `jurisdictions`, `systems`, `assets` | `app/api/admin/migrate/route.js` | many |

**Subsystem-owned tables** (each subsystem ALTERs its own via its own bootstrap):
- `water_containment_*` — owned by `app/lib/waterContainment/schemaBootstrap.js`
- `osint_extractions`, `reviewer_feedback`, `research_runs` — owned by `app/lib/engineV2/schema.js`
- `render_buildings`, `render_pins`, `render_topology_edges`, etc. — owned by `app/lib/digitalTwin/schema.js`
- `water_control_*` — owned by `migrations/water_control_schema.sql` (run via admin route)

**Adding a column to a shared table** (e.g., extending `findings` for a new feature):
1. Add the `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` to the `migrations` array in
   `app/api/admin/migrate/route.js`.
2. Document which subsystem reads/writes it (comment above the ALTER).
3. NEVER add the ALTER to a subsystem's own bootstrap (e.g.,
   `ensureWaterContainmentSchema()`) — it would run on every WC request and
   cross-subsystem mutations in the request path are fragile.
4. Run the migration via `POST /api/admin/migrate` (admin-only).

**Why this matters:** If two subsystems both try to ALTER the same shared table
with conflicting shapes, the loser silently corrupts the consumer that ran second.
Centralizing ownership prevents this. The `findings.provenance JSONB` column for
the WC audit→twin loop landed via this pattern in PR #13.

## noFriction (the app): project rules and hard-won lessons

The two sections above ("Secrets, deploys…" and "Cross-subsystem schema
ownership") describe the CriticalAsset web platform, not this repo. For
noFriction (Tauri Mac app in `src-tauri/` + `src/`, SwiftUI iOS app in `ios/`),
these rules apply.

**Product invariants (don't regress):**
- **Client-only.** No noFriction servers. Never add Supabase, Pinecone, an
  ingest server or any owner-hosted endpoint. AI requests go straight from the
  client to the endpoint the user entered. Spec: `docs/AI_PROVIDERS.md`.
- **AI is Apple on-device or one user-entered endpoint; presets are only a
  shortcut for filling that endpoint in.** The choices are Apple Foundation
  Models (iOS/macOS 26+ with Apple Intelligence) or one OpenAI-compatible
  endpoint with a base URL, model and optional user-supplied key. Provider
  preset cards (ChatGPT/OpenAI, Anthropic/Claude, Muse/Meta, Grok/xAI, Mistral; owner
  decision 2026-10-09) are static data in exactly two tables
  (`src-tauri/src/ai/providers.rs::ENDPOINT_PRESETS`,
  `ios/NoFriction/AI/AIProvider.swift::AIPreset.all`, docs cited beside each
  entry) that pre-fill the URL and model; the saved connection stays the
  `custom` provider with the same Keychain binding and consent rule, and the
  preset match is derived from the URL, never stored. Never add a default
  remote URL, a provider active at first run, a preset selected without a
  click, key-prefix detection, a startup provider probe, bundled keys or
  hosted AI; provider hosts may appear nowhere but the two tables. The only
  pre-consent request is the explicit "Test connection" button (the word
  "Hi", one token, saved endpoint and key). Custom URL, model and key start
  empty; switching the endpoint deletes the old key and clears consent. A
  saved legacy named provider id fails closed (no fallback to another network
  service) and meetings are kept. `python3 scripts/check-ai-provider-policy.py`
  must pass; both release scripts run it and then scan the signed artifact for
  credentials and retired service hosts (the four preset hosts are
  inventoried, not failed). Spec: `docs/AI_PROVIDERS.md`.
- **Transcription is on-device only:** local Whisper on the Mac, Apple speech
  on iOS. The cloud transcription modules (Deepgram, Gladia, Google, Gemini)
  were removed from the source; don't bring them back.
- **Never ship or hardcode an API key**, and never hardcode a private host (the
  old Castle/GX10 tailnet URL was removed). User-entered keys live only in the
  Keychain (`secrets.rs` on the Mac, `KeychainStore` on iOS), bound to the
  normalized endpoint: changing the endpoint deletes the old key and clears
  its consent and model, and a key is never reused at another destination.
  Never return a key to the UI (only `last4`), and never log keys or
  transcript text.
- **Consent before sending meeting content to a public endpoint.** Whether an
  endpoint is local depends on its URL (loopback, private ranges, `.local`,
  Tailscale), never on a name. Public endpoints need HTTPS; plain HTTP only
  for private hosts. The consent dialog shows the actual destination.
- **Contact and legal links:** `https://nofriction.io/privacy`,
  `https://nofriction.io/contact`, `casey@nofriction.io`; Terms are Apple's
  standard EULA (`src/lib/build.ts`, `ios/NoFriction/App/AppLinks.swift`).
- **Subscriptions:** noFriction Pro, `com.nofriction.meetings.pro.monthly`
  ($0.99/month) and `.pro.yearly` ($5.99/year), 1-week free trial each, USA
  only, Family Sharing off. Recording and microphone transcription stay free; Pro list in docs/PRO.md (owner decision 2026-10-10).
- **Delete and Strike must purge everywhere.** The checklist is in
  `docs/REDACTION.md`. Any new place that stores transcript or screen text must
  be added to that purge.
- **Apple Watch app** (`ios/NoFrictionWatch`, bundle
  `com.nofriction.meetings.watchkitapp`, embedded in the iOS app):
  - The watch only records. The iPhone transcribes on-device from the
    transferred file.
  - The watch deletes a part only after the iPhone app acknowledges it, not on
    `didFinish` success.
  - The policy guard rejects network or speech APIs in the watch target.
  - Simulators can't deliver WatchConnectivity files, so delivery must be
    verified on real devices. See `docs/WATCH_APP.md`.
- Two Mac flavors must always build: the default (Developer ID DMG) and
  `--features mas` (sandboxed Mac App Store build). In `mas`: no ffmpeg,
  Accessibility, osascript, shell plugin or `.env`; Pro gating goes through
  `entitlement::require_pro()` in `ai::client::complete`.

**Lessons that cost real time:**
- **Migrations run on ONE connection** (`run_migrations` acquires a single
  connection). Running them back-to-back on `&pool` opened extra connections
  mid-migration, and one could keep a stale schema ("no such table" on fresh
  installs, about 30% under load). To reproduce flaky DB bugs, run the lib test
  binary with `--test-threads=32` 10+ times.
- **Signing:** a custom "Always Trust" on the Developer ID cert makes codesign
  emit a non-Apple-anchored requirement. The app then fails its own signature
  check, macOS can't attach permission grants to it, and the mic and screen
  prompts repeat forever. `release-macos.sh` now refuses such a build. Check
  with `codesign -dr - <app>` (it must contain `anchor apple generic`).
- **Screen capture** without Screen Recording permission shows the macOS prompt
  on every capture call. Always check permission (`CGPreflightScreenCaptureAccess`)
  before capturing.
- **Whisper invents text on silence** ("Bye-bye. Bye-bye."). Keep
  `transcription/filter.rs` in the path of every provider, and never feed
  filtered text back as prompt context.
- **Stop must call `end_meeting`.** It once didn't, so every meeting had zero
  duration and reports never ran.
- **Before reinstalling the Mac app,** check that no recording is running
  (latest transcript timestamp). Quitting the app mid-recording loses the stop.
- **The Mac app's DB uses WAL.** When the app has no open connections,
  `sqlite3 -readonly` fails to open it. For the pre-reinstall recording check,
  use a normal connection that only SELECTs:
  `sqlite3 -cmd ".timeout 10000" "$DB" "select max(timestamp) from transcripts"`
  (never select transcript text).
- The bundle ID is `com.nofriction.meetings` everywhere (it can't change; the user-facing name is just "noFriction", product name set in `tauri.conf.json`). The Mac data folder
  migrated from `ai.nofriction.meetings`. `paths.rs` never deletes old data.
- **Shell gotchas:** `wc -l` pads with spaces, so compare with `-gt`/`-eq` or
  `tr -d ' '`. macOS `sed` doesn't support `0,/re/`. Don't run `npm run build`
  for checks, because its prebuild bumps `build_number.txt`; use
  `npx tsc && npx vite build`.

**Verify before claiming done:**
- `cargo test --lib` and `cargo test --lib --features mas` (in `src-tauri/`)
- `npx tsc --noEmit`
- the iOS command: `xcodebuild test -project ios/NoFriction.xcodeproj -scheme NoFriction -only-testing:NoFrictionTests` on a simulator

**Release docs:** `docs/APPLE_SETUP.md` (portal steps) and
`docs/APPLE_SETUP_CLOSEOUT_20261003.md` (what is configured and what remains),
`docs/LAUNCH_CHECKLIST.md`, `docs/APP_STORE_RELEASE.md` (status + owner tasks),
`docs/MAC_APP_STORE_BUILD.md`, `scripts/release-macos.sh` (DMG),
`scripts/release-mas.sh` (Mac App Store), `site/` (privacy/support pages).

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health
