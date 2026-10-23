// Mac App Store screenshots for noFriction 3.7 (the redesigned window:
// Record · Recordings · Chat, Sync, Pro). The real React UI with the
// mocked backend and fictional demo data (mock/), at 1440x900 CSS px with a
// device scale factor of 2: exactly 2880x1800, opaque, no cursor.
//
// Run from the repo root, with the harness server up:
//
//   npx vite --config marketing/film/mac-harness/vite.config.mjs --port 5287 --strictPort
//   node marketing/film/mac-harness/make-frames.mjs        # once: the slide "screens"
//   node marketing/film/mac-harness/capture-store-3.7.mjs  # every shot
//   node marketing/film/mac-harness/capture-store-3.7.mjs 03-search 07-settings-ai
//   node marketing/film/mac-harness/capture-store-3.7.mjs site   # the website's Mac shots
//
// HARNESS_URL picks another server (default http://127.0.0.1:5287/).
// Output: marketing/film/stills/mac-3.7/NN-name.png
//         marketing/out/site-src/mac-NN-name.png (site; gitignored, encoded
//         into site/img/ by marketing/film/scripts/site-images.mjs)
//
// (capture.mjs films the same UI for the launch film and the app previews.)

import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "../../..");
const STORE_OUT = path.join(REPO, "marketing/film/stills/mac-3.7");
const SITE_OUT = path.join(REPO, "marketing/out/site-src");
const BASE = process.env.HARNESS_URL || "http://127.0.0.1:5287/";
const STORE = { w: 1440, h: 900, scale: 2 };

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function load(page, query = "") {
    await page.goto(`${BASE}${query}`, { waitUntil: "networkidle" });
    await page.waitForSelector(".topbar", { timeout: 20000 });
    await page.evaluate(() => document.fonts.ready);
    await sleep(600);
}

async function mode(page, label) {
    await page.click(`.topbar__mode:has-text("${label}")`);
    await sleep(500);
}

async function openRecording(page, title, tab) {
    await mode(page, "Recordings");
    await page.locator(".meeting-item", { hasText: title }).first().click();
    await sleep(700);
    if (tab) {
        await page.locator(".recording-detail > .deck-tabs .deck-tab", { hasText: tab }).first().click();
        await sleep(900);
    }
}

async function openSettings(page, category) {
    await page.click(".topbar__gear");
    await sleep(500);
    await page.locator(".settings__nav-item", { hasText: new RegExp(`^${category}$`) }).first().click();
    await sleep(800);
}

const SHOTS = {
    // 1. A lecture recording: live transcript, captured slides, the capture bar
    async "01-record-lecture"(page) {
        await load(page);
        // 23:24 into a 60-minute class: about 36 minutes left
        await page.evaluate(() => window.__harness.startLive("class", { prefill: 6, backdateSec: 1404, duration: "60" }));
        // The last lines stream in, then the screens settle
        await sleep(9000);
        await page.mouse.move(1440, 900);
    },

    // 2. A meeting's Notes: summary, decisions, action items
    async "02-meeting-notes"(page) {
        await load(page);
        await openRecording(page, "Acme project sync", "Notes");
        await page.waitForSelector(".mn-notes, .mn-section", { timeout: 10000 });
    },

    // 3. Search across every recording
    async "03-search"(page) {
        await load(page);
        await mode(page, "Recordings");
        await page.click(".rec-search__input");
        await page.keyboard.type("membrane", { delay: 40 });
        await page.waitForSelector(".rec-search__line", { timeout: 10000 });
        await sleep(800);
    },

    // 4. A study guide: the practice quiz with a question answered
    async "04-study-quiz"(page) {
        await load(page);
        await openRecording(page, "BIO 101: Cell Biology", "Study");
        await page.locator(".study-tabs .deck-tab", { hasText: "quiz" }).first().click();
        await sleep(700);
        await page.locator(".study-choice").nth(1).click();
        await sleep(700);
    },

    // 5. Chat: answers with citations, across recordings
    async "05-chat"(page) {
        await load(page);
        await mode(page, "Chat");
        const ask = async (q) => {
            const before = await page.locator(".rc-sources__toggle").count();
            await page.locator("textarea").first().click();
            await page.keyboard.type(q, { delay: 12 });
            await page.keyboard.press("Enter");
            await page.waitForFunction((n) => document.querySelectorAll(".rc-sources__toggle").length > n, before, { timeout: 10000 });
            await sleep(500);
        };
        await ask("What did we decide about the launch?");
        await ask("What's going to be on the BIO 101 exam?");
        // The latest answer's citations open: each names the recording and the moment
        await page.locator(".rc-sources__toggle").last().click();
        await sleep(800);
    },

    // 6. The Record sheet: Class, 60 min, notebook BIO 101
    async "06-record-sheet"(page) {
        await load(page);
        await page.click(".lt-empty__action");
        await sleep(500);
        await page.click('.rpick__kind:has-text("Class")');
        await page.click('.rpick__choice:has-text("60")');
        // The recent notebooks stay listed; BIO 101 picked
        await page.click('.rpick__chip:has-text("BIO 101")');
        await sleep(600);
    },

    // 7. Settings → AI: Connect your AI (Sign in with OpenRouter, on this
    // Mac, or a key card with Get a key → Paste key and connect)
    async "07-settings-ai"(page) {
        await load(page);
        await openSettings(page, "AI");
        await page.waitForSelector(".ai-quick .ai-quick-card", { timeout: 10000 });
        await page.locator(".ai-preset-card", { hasText: "Anthropic" }).first().click();
        await sleep(400);
        await page.evaluate(() => {
            const h = [...document.querySelectorAll(".settings__body h3")].find((e) => e.textContent.trim() === "Connect your AI");
            const sec = h?.closest("section") ?? h;
            sec?.scrollIntoView({ block: "start" });
            const scroller = (el) => (el && el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== "visible" ? el : el?.parentElement ? scroller(el.parentElement) : null);
            const box = scroller(sec);
            if (box) box.scrollTop -= 20;
        });
        await sleep(500);
    },

    // 9. Export…: the package filled with AI, screens picked and captioned
    async "09-export"(page) {
        await load(page);
        await openRecording(page, "Acme project sync");
        await page.click(".deck-tabs__export");
        await page.waitForSelector(".xp-sheet .xp-rows", { timeout: 10000 });
        await page.click('button:has-text("Fill with AI")');
        await page.waitForSelector(".xp-highlights li", { timeout: 10000 });
        await page.waitForFunction(() => [...document.querySelectorAll(".xp-caption")].some((i) => i.value), null, { timeout: 10000 });
        await imagesLoaded(page, ".xp-tile__img img", 4);
        // The highlights at the top, the picked screens with captions below
        await page.evaluate(() => {
            const body = document.querySelector(".xp-body");
            const pics = document.querySelector(".xp-pics");
            if (body && pics) body.scrollTop = Math.max(0, pics.offsetTop - body.clientHeight + pics.clientHeight * 0.95);
        });
        await sleep(600);
    },

    // 8. Settings → Sync: the pairing code
    async "08-settings-sync"(page) {
        await load(page);
        await openSettings(page, "Sync");
        await page.click('button:has-text("Pair a device")');
        await page.waitForSelector(".sync-qr svg", { timeout: 10000 });
        await sleep(600);
    },
};

/** Scroll the Rewind transcript so the highlighted line sits lowest in view,
 *  its first visible line starting cleanly (the lines before give context). */
async function alignTranscript(page) {
    await page.evaluate(() => {
        const panel = document.querySelector(".rewind-transcripts");
        const active = panel?.querySelector(".transcript-entry.active");
        if (!panel || !active) return;
        const pr = panel.getBoundingClientRect();
        const top = (el) => el.getBoundingClientRect().top - pr.top + panel.scrollTop;
        const aTop = top(active);
        const aBottom = aTop + active.offsetHeight;
        const fits = [...panel.querySelectorAll(".transcript-entry")]
            .map((e) => top(e) - 8)
            .filter((t) => t <= aTop && t >= aBottom + 8 - panel.clientHeight);
        if (fits.length) panel.scrollTop = Math.min(...fits);
    });
    await sleep(300);
}

/** Wait until every <img> matching the selector has loaded. */
async function imagesLoaded(page, selector, min = 1) {
    await page.waitForFunction(
        ([sel, n]) => {
            const imgs = [...document.querySelectorAll(sel)];
            return imgs.length >= n && imgs.every((i) => i.complete && i.naturalWidth > 0);
        },
        [selector, min],
        { timeout: 15000 },
    );
}

// nofriction.io (site/index.html): same window size and scale. Encoded to
// 1400x875 WebP + PNG by marketing/film/scripts/site-images.mjs.
const SITE_SHOTS = {
    // (a) The hero: a recording open in Recordings, its Rewind at the
    // Important mark: the roadmap slide on screen at 3:34 beside the lines
    // said then, the marks on the timeline, the screens strip below
    async "mac-01-rewind"(page) {
        await load(page);
        await openRecording(page, "Acme project sync");
        await imagesLoaded(page, ".rewind-thumbnails .thumbnail img", 6);
        await page.locator(".study-pins .study-pin.is-important").first().click();
        await sleep(900);
        await imagesLoaded(page, ".rewind-frame-preview img", 1);
        await alignTranscript(page);
        await sleep(500);
    },
    // (d) The Record sheet: Class, 60 min, BIO 101 (the App Store shot)
    "mac-02-record-sheet": (page) => SHOTS["06-record-sheet"](page),
    // (e) The Study guide's practice quiz, a question answered (the App Store shot)
    "mac-03-review": (page) => SHOTS["04-study-quiz"](page),
    // (c) Links: the addresses said in the meeting with when, and one added by hand
    async "mac-04-links"(page) {
        await load(page);
        await openRecording(page, "Acme project sync", "Links");
        await page.waitForSelector(".ml-row", { timeout: 10000 });
    },
    // (b) Recordings filtered to the BIO 101 notebook, the latest lecture's Notes open
    async "mac-05-notebooks"(page) {
        await load(page);
        await mode(page, "Recordings");
        await page.locator(".class-filter .class-chip", { hasText: "BIO 101" }).first().click();
        await sleep(600);
        await page.locator(".meeting-item", { hasText: "BIO 101: Cell Biology" }).first().click();
        await sleep(700);
        await page.locator(".recording-detail > .deck-tabs .deck-tab", { hasText: "Notes" }).first().click();
        await sleep(900);
    },
    // (f) Export…: the package filled with AI (the App Store shot)
    "mac-09-export": (page) => SHOTS["09-export"](page),
    // (g) Lessons: the course Make lesson made from a class (the App Store shot)
    "mac-10-lessons": (page) => SHOTS["10-lessons"](page),
};

let args = process.argv.slice(2);
const site = args[0] === "site";
if (site) args = args.slice(1);
const SET = site ? SITE_SHOTS : SHOTS;
const OUT = site ? SITE_OUT : STORE_OUT;
const names = args.length ? args : Object.keys(SET);
const unknown = names.filter((n) => !(n in SET));
if (unknown.length) {
    console.error(`Unknown shots: ${unknown.join(", ")}\nShots: ${Object.keys(SET).join(", ")}`);
    process.exit(1);
}

fs.mkdirSync(OUT, { recursive: true });
const browser = await chromium.launch();
let failed = false;
for (const name of names) {
    // A fresh page each time: the mock's state (recordings, chat) starts clean
    const context = await browser.newContext({ viewport: { width: STORE.w, height: STORE.h }, deviceScaleFactor: STORE.scale });
    const page = await context.newPage();
    page.on("pageerror", (e) => console.warn(`  [${name}] page error:`, e.message));
    page.on("console", (m) => m.text().includes("[mock] unhandled") && console.warn(`  [${name}]`, m.text()));
    try {
        await SET[name](page);
        // Park the (invisible) mouse where it hovers nothing
        await page.mouse.move(STORE.w - 2, STORE.h - 2);
        await sleep(400);
        const out = path.join(OUT, `${name}.png`);
        await page.screenshot({ path: out, type: "png", omitBackground: false });
        console.log(`  ${path.relative(REPO, out)}`);
    } catch (e) {
        failed = true;
        console.error(`  ${name} failed:`, e.message);
    }
    await context.close();
}
await browser.close();
process.exit(failed ? 1 : 0);
