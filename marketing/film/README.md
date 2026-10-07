# noFriction launch film and App Store app previews

Source for the noFriction launch film (website hero) and the two App Store app
previews (iPhone 6.9" and Mac). Everything renders from this folder; the rendered
videos and the raw footage go to `marketing/out/`, which is gitignored. Only
source and small stills are committed.

| Deliverable | File (in `marketing/out/`) | Spec |
|---|---|---|
| Hero film | `nofriction-launch-film-1080p.mp4` | 1920x1080, 30 fps, 62.4 s, H.264 High yuv420p, +faststart, AAC stereo (< 25 MB) |
| Hero film (web) | `nofriction-launch-film-1080p.webm` | 1920x1080, 30 fps, VP9 + Opus (< 12 MB) |
| Poster | `nofriction-launch-film-poster.png` | 1920x1080 PNG (the Rewind shot) |
| iPhone app preview | `nofriction-app-preview-iphone-6.9in-886x1920.mp4` | 886x1920 portrait, 30 fps, 28.8 s, H.264 High@4.0 ~11 Mbps, AAC 256 kbps 48 kHz stereo |
| Mac app preview | `nofriction-app-preview-mac-1920x1080.mp4` | 1920x1080, 30 fps, 28.8 s, H.264 High@4.0 ~11 Mbps, AAC 256 kbps 48 kHz stereo |
| Website stills | `marketing/film/stills/film-*.png` (committed) | 1920x1080 palette PNG, < 500 KB each |
| App Store screenshots | `marketing/film/stills/iphone-6.9/`, `marketing/film/stills/mac/` (committed) | iPhone 1320x2868 + 1260x2736; Mac 2880x1800 |

The film tells its story in on-screen type, so it works muted and autoplaying
in a website hero. The music is optional.

## What is real footage and what is motion graphics

- **Real app footage** (never redrawn): every phone, watch and Mac screen in
  the film and every frame of the app previews.
  - iPhone: the iOS app in the Simulator with its debug demo data
    (`-NFSeedDemo -NFDemoLive -NFFilm`, `ios/NoFriction/App/DemoData.swift`,
    `ios/NoFriction/App/FilmDemo.swift`), driven by
    `ios/NoFrictionUITests/FilmFootageTests.swift`, recorded with
    `xcrun simctl io … recordVideo`.
  - Apple Watch: the watch app in the watchOS Simulator in its debug demo
    states (`-NFWatchDemo …`, `ios/NoFrictionWatch/App/DemoMode.swift`).
  - Mac: the Mac app's real React UI (`src/`) running in Chromium with a
    mocked Tauri backend and invented demo data (`mac-harness/`), captured
    with Playwright.
- **Motion graphics** (Remotion, `src/`): the type, the moment cards, the
  record button, mark chips, the timeline motif, backgrounds and the end
  card. The app previews add only captions above the real footage (Apple
  guideline 2.3.4 allows text overlays; no fake UI, no prices, no device
  artwork).
- **Music:** synthesized from scratch by `scripts/make-score.mjs` (oscillators,
  noise, envelopes, a small reverb): no samples, no downloaded or copyrighted
  audio, no voices. 100 BPM, so a bar is exactly 72 frames and the scenes cut
  on bars.

All content is invented: no real people, emails, calendars or brands. The
demo recordings are "Brightwater pilot kickoff", "Lecture 7: Cellular
respiration" (BIO 101) and "Physio check-in" on iPhone, and "Acme project
sync", "BIO 101: Cell Biology" and "Dr. visit notes" on the Mac.

## Re-render

Requirements: macOS with Xcode 26 (iOS 26.2 and watchOS 26.2 simulator
runtimes), XcodeGen, Node 20+, ffmpeg with libx264 (Homebrew). The VP9 WebM is
encoded with Remotion's bundled ffmpeg, which has libvpx.

```bash
# 0. dependencies (this folder only; the app's root package.json is untouched)
npm --prefix marketing/film install
npm --prefix marketing/film/mac-harness install
npm install                                   # repo root, for the Mac UI harness (vite, react)

# 1. footage (writes marketing/out/footage/{ios,watch,mac}/)
marketing/film/capture/ios/capture.sh         # iPhone + Apple Watch clips and the 6.9" stills
node marketing/film/mac-harness/capture.mjs   # Mac clips and the 2880x1800 stills

# 2. music (writes marketing/out/audio/score-*.wav)
node marketing/film/scripts/make-score.mjs

# 3. film, previews, poster, stills (writes marketing/out/*, marketing/film/stills/film-*.png)
node marketing/film/scripts/render-all.mjs            # or: film | previews
```

`render-all.mjs` links the footage into `public/`, writes `src/footage.json`
(clip sizes and durations from ffprobe), renders a high-quality master per
composition with Remotion, encodes the deliverables with ffmpeg, and checks
the app previews against Apple's spec (it prints `OK … meets Apple's app
preview spec` or the problem).

To edit, run `npm --prefix marketing/film run studio` (Remotion Studio) after
step 1 and 2. Which clip each shot uses, and from which second, is in
`src/clips.ts`; scene timing is in `src/timing.ts`.

## Layout

```
marketing/film/
  src/
    Root.tsx            compositions: Film, PreviewIPhone, PreviewMac
    Film.tsx            the 8 scenes on the bar grid + the score
    scenes/S1…S8        moments · record · transcribe · mark · rewind · notes · offline · end card
    previews/           App Store previews (real footage + captions)
    components/         kinetic type, footage frames, chips, timeline, icons
    clips.ts            shot → footage file and in-point
    theme.ts            the app's tokens (hazard yellow on matte black), Inter + JetBrains Mono
  scripts/
    make-score.mjs      the synthesized score
    link-media.mjs      public/ symlinks + src/footage.json
    render-all.mjs      masters → deliverables → ffprobe checks
    stills.mjs          single frames (layout checks)
  capture/ios/          iPhone + watch simulator capture
  mac-harness/          Mac UI in a browser with a mocked Tauri backend + Playwright capture
  public/               mark.svg, app-icon.png (512 px)
  stills/               committed stills (website + App Store screenshots)
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
Mono are SIL Open Font License fonts, loaded from Google Fonts at render time.
