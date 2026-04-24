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
