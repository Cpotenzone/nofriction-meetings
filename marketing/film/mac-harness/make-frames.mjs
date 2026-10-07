// Renders the fictional slides in slides/slides.html to the "screen
// captures" the mocked Rewind timeline shows:
//   frames/<id>.jpg        2400x1350 (the big preview)
//   frames/<id>-thumb.jpg  480x270   (the thumbnail strip and live filmstrip)
//
// Run from this folder:  npm install && node make-frames.mjs
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, "frames");
await mkdir(out, { recursive: true });

const browser = await chromium.launch();
const url = pathToFileURL(path.join(here, "slides", "slides.html")).href;

for (const [scale, suffix] of [[1.5, ""], [0.3, "-thumb"]]) {
    const page = await browser.newPage({ viewport: { width: 1700, height: 1000 }, deviceScaleFactor: scale });
    await page.goto(url);
    await page.evaluate(() => document.fonts.ready);
    const ids = await page.$$eval("section.slide", (els) => els.map((e) => e.id));
    for (const id of ids) {
        const file = path.join(out, `${id}${suffix}.jpg`);
        await page.locator(`#${id}`).screenshot({ path: file, type: "jpeg", quality: suffix ? 88 : 92 });
        console.log(path.relative(here, file));
    }
    await page.close();
}
await browser.close();
