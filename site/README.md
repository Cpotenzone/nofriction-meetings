# noFriction website (`site/`)

Static marketing, support and legal pages for noFriction. Plain HTML and one
stylesheet: no build step, no JavaScript, no analytics, no external fonts or
CDNs.

| File | URL path | Used by |
|---|---|---|
| `index.html` | `/` | App Store **Marketing URL** |
| `support.html` | `/support.html` | App Store **Support URL** (required) |
| `privacy.html` | `/privacy.html` | App Store **Privacy Policy URL** (required), in-app links |
| `terms.html` | `/terms.html` | Subscription terms; links to Apple's standard EULA |
| `style.css`, `mark.svg` | | shared style and logo |

## URLs the apps expect

The apps link to the privacy policy at a **placeholder** URL:

- iOS: `ios/NoFriction/App/AppLinks.swift` → `privacyPolicy = https://nofriction.ai/privacy`
- Mac: `src/lib/build.ts` → `PRIVACY_URL = "https://nofriction.ai/privacy"`
- Support email in both: `support@nofriction.ai`

`https://nofriction.ai/privacy` (no `.html`) must resolve. GitHub Pages
serves `privacy.html` for `/privacy` automatically, so the custom-domain
setup below satisfies it without code changes. If you host somewhere else,
make sure extensionless URLs work, or change both constants (the app code is
owned outside `site/`).

## Deploy to GitHub Pages

### Option A: Pages from this repo, via a workflow (recommended)

Pages can only serve the repo root or `/docs` from a branch, and `docs/` here
is engineering documentation, so publish `site/` with a workflow:

1. GitHub → repo → **Settings → Pages → Build and deployment → Source:
   GitHub Actions**.
2. Add `.github/workflows/pages.yml`:

   ```yaml
   name: Deploy site
   on:
     push:
       branches: [main]
       paths: ["site/**"]
     workflow_dispatch:
   permissions:
     contents: read
     pages: write
     id-token: write
   concurrency: { group: pages, cancel-in-progress: true }
   jobs:
     deploy:
       runs-on: ubuntu-latest
       environment: { name: github-pages, url: "${{ steps.deployment.outputs.page_url }}" }
       steps:
         - uses: actions/checkout@v4
         - uses: actions/configure-pages@v5
         - uses: actions/upload-pages-artifact@v3
           with: { path: site }
         - id: deployment
           uses: actions/deploy-pages@v4
   ```

3. Push to `main`. The site appears at `https://<owner>.github.io/<repo>/`.

Note: a private repo needs a paid GitHub plan for Pages. If the app repo must
stay private on a free plan, use option B.

### Option B: a separate public repo

1. Create a public repo, e.g. `nofriction-site`.
2. Copy the contents of `site/` to its root (this README is optional).
3. **Settings → Pages → Source: Deploy from a branch → `main` / `(root)`**.

### Custom domain (`nofriction.ai`)

Do this once, in this order. (No `CNAME` file is checked in here on purpose:
with the GitHub Actions source, the domain is set in Settings and a `CNAME`
file is ignored. For option B, GitHub writes the file for you when you save
the domain.)

1. **Verify the domain** (prevents takeover): GitHub → your profile or org →
   **Settings → Pages → Add a domain** → `nofriction.ai`. Add the TXT record
   it shows (`_github-pages-challenge-<owner>.nofriction.ai`) at your DNS
   provider, then click Verify.
2. **DNS records** at the registrar:
   - Apex `nofriction.ai`: four `A` records
     `185.199.108.153`, `185.199.109.153`, `185.199.110.153`, `185.199.111.153`
     (and optionally `AAAA` `2606:50c0:8000::153`, `…8001::153`, `…8002::153`, `…8003::153`).
   - `www`: `CNAME` → `<owner>.github.io` (no repo name).
   - Check GitHub's current docs ("Managing a custom domain for your GitHub
     Pages site") for these IPs before you enter them.
3. Repo → **Settings → Pages → Custom domain** → `nofriction.ai` → Save.
   Wait for the DNS check to pass.
4. Tick **Enforce HTTPS** once the certificate is issued (can take up to an
   hour).
5. Check: `curl -sI https://nofriction.ai/privacy` returns `200`, and
   `https://nofriction.ai/support.html` loads.

### Email

`support@nofriction.ai` must receive mail before App Review (it's on the
Support page, in the apps and in the App Store listing). Pages doesn't host
mail: add `MX` records for your mail provider (Google Workspace, Fastmail,
iCloud+ custom domain, or forwarding such as ImprovMX/Cloudflare Email
Routing). Add SPF/DMARC `TXT` records as the provider instructs. Send a test
message from an outside address.

## Editing

- Keep claims in sync with the apps. Feature facts come from the code; see
  `docs/APP_STORE_LISTING.md` for the vetted copy.
- If the privacy policy changes, update its "Effective" date.
- Prices are deliberately not on the site: they vary by country and are set
  in App Store Connect.
- Preview locally: `python3 -m http.server -d site 8000` → http://localhost:8000
