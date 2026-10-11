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
//
// HARNESS_URL picks another server (default http://127.0.0.1:5287/).
// Output: marketing/film/stills/mac-3.7/NN-name.png
//
// (capture.mjs films the pre-redesign UI and is kept for the launch film.)

import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "../../..");
const OUT = path.join(REPO, "marketing/film/stills/mac-3.7");
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

    // 7. Settings → AI: the connector cards
    async "07-settings-ai"(page) {
        await load(page);
        await openSettings(page, "AI");
        // Scroll down to Connect, the cards (Apple on-device, the presets,
        // Custom), with the Models card above it whole
        await page.waitForSelector(".ai-preset-grid .ai-preset-card", { timeout: 10000 });
        await page.evaluate(() => {
            const h = [...document.querySelectorAll(".settings__body h3")].find((e) => e.textContent.trim() === "Models");
            const sec = h?.closest("section") ?? h;
            sec?.scrollIntoView({ block: "start" });
            const scroller = (el) => (el && el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== "visible" ? el : el?.parentElement ? scroller(el.parentElement) : null);
            const box = scroller(sec);
            if (box) box.scrollTop -= 20;
        });
        await sleep(500);
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

const args = process.argv.slice(2);
const names = args.length ? args : Object.keys(SHOTS);
const unknown = names.filter((n) => !(n in SHOTS));
if (unknown.length) {
    console.error(`Unknown shots: ${unknown.join(", ")}\nShots: ${Object.keys(SHOTS).join(", ")}`);
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
        await SHOTS[name](page);
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
