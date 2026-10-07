# nofriction.io (`site/`)

The public website: plain HTML + CSS, two small scripts, optimized images.
No framework, no build step, no trackers, no cookies, no external fonts or
scripts. Deployed as-is to S3 + CloudFront; see
[`infra/site/README.md`](../infra/site/README.md).

## Pages (clean URLs: `path/index.html`)

| URL | File | What |
|---|---|---|
| `/` | `index.html` | Sales page: hero + film slot, privacy strip, how it works, three audiences, Rewind and features, Apple Watch, pricing, FAQ, notify CTA |
| `/download/` | `download/index.html` | "Get notified" + TestFlight today; App Store badges once live |
| `/support/` | `support/index.html` | Support and FAQ |
| `/privacy/` | `privacy/index.html` | App privacy policy (the App Store Connect privacy URL) |
| `/privacy/website/` | `privacy/website/index.html` | Website and consulting privacy policy |
| `/terms/` | `terms/index.html` | App subscription terms (under Apple's standard EULA) + website Terms of Service |
| `/about/` | `about/index.html` | CCA Innovations OU d/b/a No Friction: the app and the consulting practice |
| `/casey/` | `casey/index.html` | Casey Potenzone: story, CV, patents; links `files/POTENZONE_Casey_CV-EN.docx` |
| `/contact/` | `contact/index.html` | Email, phone, address. No form. |
| `/404.html` | `404.html` | Not-found page (CloudFront custom error response) |
| `/privacy.html` | `privacy.html` | Redirect stub to `/privacy/` (the old flat URL) |

Plus `sitemap.xml`, `robots.txt`, `style.css`, `js/store.js`, `js/site.js`,
`img/` (WebP + PNG fallbacks, icons, `og.png`, Apple badges), `files/`, and
`video/` (empty in git; the deploy copies `marketing/out/hero.{mp4,webm}` in).

## Launch-day flip

`js/store.js` → `live: true`. Every "Get notified" block becomes the official
App Store / Mac App Store badges (already in the HTML, hidden), the header
button reads "Download", and store links come from the same object. The HTML
defaults to the not-live state, so the site is right with JavaScript off.

## Editing

- Every page carries the same header and footer; edit them in each file (there
  is no template). `grep -l 'site-header' site -r` lists them.
- Images: keep each under 300 KB. Add a WebP and a PNG fallback, and set
  `width`/`height` on the `<img>` so nothing shifts while loading.
- Product facts come from `docs/APP_STORE_LISTING.md` and `docs/USER_GUIDE.md`.
  No third-party product names in the copy (App Store guideline 2.3.7).
- Check locally: `cd site && python3 -m http.server 8787`, then
  `npx -y htmlhint "site/**/*.html"`.
