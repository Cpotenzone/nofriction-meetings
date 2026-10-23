// Encodes the nofriction.io screenshots (site/img/) from their sources, at
// the sizes site/index.html shows them: a WebP and a palette PNG fallback
// each, every file under 300 KB.
//
//   node marketing/film/mac-harness/capture-store-3.7.mjs site   # Mac sources (harness running)
//   node marketing/film/scripts/site-images.mjs                  # encode all
//
// Sources:
//   Mac     marketing/out/site-src/mac-*.png        (2880x1800, the real React UI in the harness)
//   iPhone  ios/AppStore/screenshots/iphone-6.9/    (1320x2868, the 6.9" App Store set)
//   Watch   ios/AppStore/screenshots/watch-46mm/    (416x496, the 46 mm App Store set)

import { existsSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(FILM, "..", "..");
const IMG = join(REPO, "site", "img");
const MAC = join(REPO, "marketing", "out", "site-src");
const IPHONE = join(REPO, "ios", "AppStore", "screenshots", "iphone-6.9");
const WATCH = join(REPO, "ios", "AppStore", "screenshots", "watch-46mm");
const MAX = 300 * 1000;

// name in site/img → source, display width (height follows the aspect)
const JOBS = [
    ["mac-01-rewind", join(MAC, "mac-01-rewind.png"), 1400],
    ["mac-02-record-sheet", join(MAC, "mac-02-record-sheet.png"), 1400],
    ["mac-03-review", join(MAC, "mac-03-review.png"), 1400],
    ["mac-04-links", join(MAC, "mac-04-links.png"), 1400],
    ["mac-05-notebooks", join(MAC, "mac-05-notebooks.png"), 1400],
    ["mac-09-export", join(MAC, "mac-09-export.png"), 1400],
    ["mac-10-lessons", join(MAC, "mac-10-lessons.png"), 1400],
    ["iphone-01-record", join(IPHONE, "01-record.png"), 600],
    ["iphone-02-notes", join(IPHONE, "02-meeting-notes.png"), 600],
    ["iphone-03-stricken", join(IPHONE, "03-stricken.png"), 600],
    ["watch-02-record", join(WATCH, "02-record.png"), 416],
    ["watch-03-recordings", join(WATCH, "03-recordings.png"), 416],
    ["watch-04-class", join(WATCH, "04-class.png"), 416],
    ["watch-05-discreet", join(WATCH, "05-discreet.png"), 416],
    // /guides/classes/: the lecture being recorded (the Mac App Store shot)
    ["guides/mac-lecture-recording", join(FILM, "stills", "mac-3.7", "01-record-lecture.png"), 1440],
];

const only = process.argv.slice(2);
let failed = false;
for (const [name, src, width] of JOBS) {
    if (only.length && !only.includes(name)) continue;
    if (!existsSync(src)) {
        console.error(`  missing source for ${name}: ${relative(REPO, src)}`);
        failed = true;
        continue;
    }
    const base = sharp(src).removeAlpha().resize({ width, kernel: "lanczos3" });
    const webp = join(IMG, `${name}.webp`);
    const png = join(IMG, `${name}.png`);
    for (const q of [84, 78, 70, 62]) {
        await base.clone().webp({ quality: q, effort: 6, smartSubsample: true }).toFile(webp);
        if (statSync(webp).size <= MAX) break;
    }
    // libimagequant palette PNG; step the quality down until it fits
    for (const q of [90, 80, 70, 60, 50]) {
        await base.clone().png({ palette: true, quality: q, effort: 10, compressionLevel: 9, dither: 0.5 }).toFile(png);
        if (statSync(png).size <= MAX) break;
    }
    const { width: w, height: h } = await sharp(png).metadata();
    const kb = (f) => Math.round(statSync(f).size / 1000);
    const over = [webp, png].filter((f) => statSync(f).size > MAX);
    if (over.length) failed = true;
    console.log(`  ${name}  ${w}x${h}  webp ${kb(webp)} KB  png ${kb(png)} KB${over.length ? "  !! over 300 KB" : ""}`);
}
process.exit(failed ? 1 : 0);
