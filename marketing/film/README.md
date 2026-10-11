# noFriction launch film and App Store app previews

Source for the noFriction launch film (website hero) and the App Store app
previews. Everything renders from this folder; the rendered videos and the raw
footage go to `marketing/out/`, which is gitignored. Only source and small
stills are committed.

The film and the Mac preview show the redesigned apps (Mac 3.7: one window,
Record · Recordings · Chat, sentence case, the Settings gear, Sync, Pro;
iPhone and Apple Watch 1.0.1).

| Deliverable | File (in `marketing/out/`) | Spec |
|---|---|---|
| Hero film | `nofriction-launch-film-1080p.mp4`, copied to `hero.mp4` | 1920x1080, 30 fps, 33.6 s, H.264 High yuv420p, +faststart, AAC stereo (< 25 MB) |
| Hero film (web) | `nofriction-launch-film-1080p.webm`, copied to `hero.webm` | 1920x1080, 30 fps, 33.6 s, VP9 + Opus (< 12 MB) |
| Poster | `nofriction-launch-film-poster.png`, copied to `poster.png` | 1920x1080 PNG (the Rewind shot) |
| Mac app preview | `previews/nofriction-app-preview-mac-1920x1080.mp4` | 1920x1080, 30 fps, 28.8 s, H.264 High@4.0 ~11 Mbps, AAC 256 kbps 48 kHz stereo |
| iPhone app preview | `previews/nofriction-app-preview-iphone-6.9in-886x1920.mp4` (`IOS_PREVIEW=1`) | 886x1920 portrait, 30 fps, 28.8 s, H.264 High@4.0 ~11 Mbps, AAC 256 kbps 48 kHz stereo |
| Film stills | `marketing/film/stills/film-*.png` (committed) | 1920x1080 palette PNG, < 300 KB each |
| App Store screenshots | `marketing/film/stills/mac-3.7/` (`mac-harness/capture-store-3.7.mjs`), `marketing/film/stills/iphone-6.9/` (committed) | Mac 2880x1800; iPhone 1320x2868 + 1260x2736 |
| Website screenshots | `site/img/` (`scripts/site-images.mjs`) | WebP + PNG at the page's sizes, < 300 KB each |

`infra/site/deploy.sh` copies `marketing/out/hero.mp4` and `hero.webm` into
`site/video/` when it runs on a machine that has them (CI has none, and the
deploy never deletes the uploaded film).

The film tells its story in on-screen type, so it works muted and autoplaying
in a website hero. The music is optional.

## Storyboard (33.6 s, 14 bars at 100 BPM)

| Bars | Scene | On screen |
|---|---|---|
| 0–1 | Record | "Tap Record." → "Meetings, classes and life." The Mac Record sheet: Class, 60 min, BIO 101, Start recording; the iPhone Record screen and the Apple Watch Record button beside it |
| 2–3 | Mark | "Mark what matters." → "Then Stop." A class recording with the capture bar: Mark → On the test → a note, then Stop opens it in Recordings |
| 4–5 | Rewind | "Rewind to any moment." → "The screen, next to what was said." Recordings → Acme project sync: the playhead dragged to the roadmap, the Important pin, the sign-ups chart |
| 6–7 | Notes · Review guide | "Make notes and a review guide." A meeting's Notes, then a class's Study guide practice quiz answered |
| 8–9 | Chat | "Ask your recordings." → "Every answer cites the moment." A question across recordings, the answer, its sources |
| 10 | Sync | "Sync iPhone and Mac. Directly on your Wi-Fi. No server." Settings → Sync → Pair a device (QR code), the iPhone Recordings list |
| 11 | Privacy | "Works offline. Nothing leaves your device unless you want it to." Data Not Collected · No account · No noFriction servers |
| 12–13 | End card | noFriction · Record your Life · nofriction.io |

The Mac preview (28.8 s) is the same takes with captions: Record, On-device,
Marks, Rewind, Review guide, Chat, Sync.

## What is real footage and what is motion graphics

- **Real app footage** (never redrawn): every phone, watch and Mac screen in
  the film and every frame of the app previews.
  - Mac: the Mac app's real React UI (`src/`) running in Chromium with a
    mocked Tauri backend and invented demo data (`mac-harness/`), filmed with
    Playwright (`mac-harness/capture.mjs`).
  - iPhone and Apple Watch in the film: the 1.0.1 App Store screenshots
    (`ios/AppStore/screenshots/`), shown as stills.
  - iPhone app preview: the iOS app in the Simulator with its debug demo data
    (`-NFSeedDemo -NFDemoLive -NFFilm`), driven by
    `ios/NoFrictionUITests/FilmFootageTests.swift`, recorded with
    `xcrun simctl io … recordVideo` (`capture/ios/capture.sh`).
  - What is simulated inside that real UI: the words of the live
    transcripts (the harness has no Rust backend and the Simulator no speech
    recognition, so invented lines are fed in at speaking pace), the AI notes,
    study guides and Chat answers (stored demo text, no AI call), the Mac's
    screen captures (invented slides) and the paired iPhone. The Mac UI is the
    Mac App Store flavor with Pro active, so no paywall or prices appear.
- **Motion graphics** (Remotion, `src/`): the type, the chips, backgrounds,
  the window frames and the end card. The app previews add only captions
  above the real footage (Apple guideline 2.3.4 allows text overlays; no fake
  UI, no prices, no device artwork).
- **Music:** synthesized from scratch by `scripts/make-score.mjs` (oscillators,
  noise, envelopes, a small reverb): no samples, no downloaded or copyrighted
  audio, no voices. 100 BPM, so a bar is exactly 72 frames and the scenes cut
  on bars.

All content is invented: no real people, emails, calendars or brands. The
demo recordings are "Acme project sync", "BIO 101: Cell Biology" and
"Dr. visit notes" on the Mac, and "Brightwater pilot kickoff", "Lecture 7:
Cellular respiration" (BIO 101) and "Physio check-in" on iPhone.

## Re-render

Requirements: macOS, Node 20+, ffmpeg with libx264 (Homebrew), Playwright's
cached Chromium (`npx playwright install chromium chromium-headless-shell` in
`mac-harness/`, once), and the Inter font installed (SIL OFL; without it the
type falls back to the system font). Nothing is fetched at render time:
fonts come from the Mac, Remotion renders with Playwright's headless shell
(`REMOTION_BROWSER` picks another), and the VP9 WebM is encoded with
Remotion's bundled ffmpeg, which has libvpx. The iPhone preview also needs
Xcode 26 (iOS 26.2 and watchOS 26.2 simulator runtimes) and XcodeGen.

```bash
# 0. dependencies (this folder only; the app's root package.json is untouched)
npm --prefix marketing/film ci
npm --prefix marketing/film/mac-harness ci
npm ci                                        # repo root, for the Mac UI harness (vite, react)

# 1. Mac footage (writes marketing/out/footage/mac/)
node marketing/film/mac-harness/make-frames.mjs   # once: the invented slides the Mac "captures"
node marketing/film/mac-harness/capture.mjs       # every clip at 3840x2160; or: mac-03-rewind …
#    (iPhone preview only) marketing/film/capture/ios/capture.sh iphone   (details: capture/README.md)

# 2. film, Mac preview, poster, stills (writes marketing/out/*, marketing/film/stills/film-*.png)
node marketing/film/scripts/render-all.mjs            # or: film | previews
IOS_PREVIEW=1 node marketing/film/scripts/render-all.mjs previews   # adds the iPhone preview

# Website screenshots (site/img/), with the harness server running
npx vite --config marketing/film/mac-harness/vite.config.mjs --port 5287 --strictPort
node marketing/film/mac-harness/capture-store-3.7.mjs site
node marketing/film/scripts/site-images.mjs
```

`render-all.mjs` synthesizes the score, copies the App Store stills into the
footage folder and links it into `public/`, writes `src/footage.json` (clip
sizes and durations from ffprobe), renders a high-quality master per
composition with Remotion, encodes the deliverables with ffmpeg, and checks
the app previews against Apple's spec (it prints `OK … meets Apple's app
preview spec` or the problem).

The Mac harness films in 10x slow motion (page clocks, timers, CSS and Web
Animations slowed, frame times mapped back onto an exact 30 fps grid), so the
clips stay smooth even on a busy machine; each clip takes a few minutes
(`FILM_RES=1080 FILM_SLOW=3` for a quick test take). To browse the harness
yourself: `npx vite --config marketing/film/mac-harness/vite.config.mjs`
from the repo root, then open `http://127.0.0.1:5193/?cursor=1`.

To edit, run `npm --prefix marketing/film run studio` (Remotion Studio) after
step 1. Which clip each shot uses, and from which second, is in
`src/clips.ts` (with cut notes); scene timing is in `src/timing.ts` and the
score's arrangement in `scripts/make-score.mjs`, which must stay in step.
`npm --prefix marketing/film run typecheck` checks the project.

## Layout

```
marketing/film/
  src/
    Root.tsx            compositions: Film, PreviewIPhone, PreviewMac
    Film.tsx            the 8 scenes on the bar grid + the score
    scenes/Scenes.tsx   record · mark · rewind · notes · chat · sync · privacy
    scenes/S8End.tsx    the end card
    previews/           App Store previews (real footage + captions)
    components/         kinetic type, footage frames, chips, timeline, icons
    clips.ts            shot → footage file and in-point (cut notes)
    theme.ts            the app's tokens (hazard yellow on matte black) and font stacks
  scripts/
    make-score.mjs      the synthesized score
    link-media.mjs      App Store stills, public/ symlinks + src/footage.json
    render-all.mjs      masters → deliverables → ffprobe checks
    stills.mjs          single frames (layout checks)
    site-images.mjs     site/img/ screenshots (WebP + PNG, < 300 KB)
  capture/ios/          iPhone + watch simulator capture
  mac-harness/          Mac UI in a browser with a mocked Tauri backend + Playwright capture
  public/               mark.svg, app-icon.png (512 px)
  stills/               committed stills (film + App Store screenshots)
```

## Apple's app preview rules (checked 2026-10-06)

From [App preview specifications](https://developer.apple.com/help/app-store-connect/reference/app-preview-specifications)
and [App previews](https://developer.apple.com/app-store/app-previews/):

- 15–30 seconds, up to 30 fps, up to 500 MB, `.mov`/`.m4v`/`.mp4`.
- H.264: High Profile up to Level 4.0, target 10–12 Mbps, progressive.
- iPhone 6.9" (and 6.5"): 886x1920 portrait or 1920x886 landscape. Mac: 1920x1080.
- Audio: stereo AAC 256 kbps, 44.1 or 48 kHz, all tracks enabled.
- Only footage captured from the app; text and graphic overlays are fine;
  no people or hands, no prices, legible text on screen long enough to read.
  Previews autoplay muted.

## Licenses

Remotion is free for individuals and companies with up to three employees;
larger companies need a Remotion company license
([remotion.dev/license](https://remotion.dev/license)). Inter and JetBrains
Mono are SIL Open Font License fonts, used from the rendering Mac's installed
fonts.
