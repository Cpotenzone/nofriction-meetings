// Films the Mac app's real UI (running in the harness with mocked data) for
// the launch film, and takes the Mac App Store screenshots.
//
//   cd marketing/film/mac-harness
//   npm install            # playwright 1.58.2 (uses the cached Chromium 1208)
//   node make-frames.mjs   # once: renders the fictional slides to frames/
//   node capture.mjs                       # App Store stills, then every clip
//   node capture.mjs stills                # App Store stills only
//   node capture.mjs clips                 # every clip (video + 4K still)
//   node capture.mjs mac-01-rewind mac-05-mark   # some clips
//
// The script starts the harness's Vite server itself (repo root
// node_modules) on a free port and stops it at the end; set HARNESS_URL to
// use a server that is already running.
//
// Options (environment):
//   FILM_TAKES=1         takes per clip (more are filmed when one drops frames);
//                        the one with the fewest dropped-frame stutters is kept
//   FILM_RES=1080        film at 1920x1080 instead of 3840x2160
//   FILM_JPEG_QUALITY=96 screencast JPEG quality
//   FILM_SLOW=10         slow motion factor while filming (1 = real time)
//   FILM_DEBUG=1         log action times and frame gaps
//
// Output
//   ../stills/mac/*.png                  App Store screenshots, 2880x1800
//   ../../out/footage/mac/<clip>.mp4     H.264 yuv420p, 30 fps CFR, CRF 15, 3840x2160
//   ../../out/footage/mac/<clip>.png     4K still of the clip's key state
//
// How the video is made: the page is laid out at 1440x810 CSS px (a Mac
// app window) on a display with a device scale factor of 8/3, so Chromium
// renders 3840x2160 for real (--force-device-scale-factor; Playwright's
// emulated scale factor doesn't reach the screencast). The DevTools
// screencast sends every composited frame (JPEG q96) with its timestamp;
// ffmpeg turns that variable-rate stream into constant 30 fps using those
// times. Filming runs in slow motion (FILM_SLOW, default 10x: page clocks,
// timers, CSS/Web Animations and smooth scrolling all slowed, see time.ts)
// and the frame times are divided back, so a busy Mac still yields smooth
// motion. No cursor is drawn: the mouse still moves, so the UI's own hover
// and pressed states show.

import { chromium } from "playwright";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "../../..");
const CLIPS_OUT = path.join(REPO, "marketing/out/footage/mac");
const STILLS_OUT = path.join(REPO, "marketing/film/stills/mac");
const RAW = path.join(os.tmpdir(), "nofriction-film-raw");

// Film: a 1440x810 window rendered at 3840x2160
// (FILM_RES=1080 renders at 1920x1080 instead)
const HD = process.env.FILM_RES === "1080";
const FILM = { w: 1440, h: 810, scale: HD ? 4 / 3 : 8 / 3 };
const OUT = HD ? { w: 1920, h: 1080 } : { w: 3840, h: 2160 };
// App Store (Mac): 16:10, 2880x1800
const STORE = { w: 1440, h: 900, scale: 2 };
// Slow motion while filming (time.ts): page time runs this many times
// slower while the camera rolls, the script acts this much slower, and the
// frame times are mapped back. 10x keeps 4K smooth on a busy Mac (a loaded
// Mac screencasts 4K at ~8 fps; 10x makes that ~80 fps of page time).
const SLOW = Math.max(1, Number(process.env.FILM_SLOW || 10));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── Harness server ──────────────────────────────────────────────────────

function freePort(start) {
    return new Promise((resolve) => {
        const s = net.createServer();
        s.once("error", () => resolve(freePort(start + 1)));
        s.listen(start, "127.0.0.1", () => s.close(() => resolve(start)));
    });
}

async function startServer() {
    if (process.env.HARNESS_URL) return { url: process.env.HARNESS_URL, stop: () => {} };
    const port = await freePort(5193);
    const vite = path.join(REPO, "node_modules/vite/bin/vite.js");
    const child = spawn(process.execPath, [vite, "--config", path.join(HERE, "vite.config.mjs")], {
        cwd: REPO,
        env: { ...process.env, HARNESS_PORT: String(port) },
        stdio: ["ignore", "pipe", "pipe"],
    });
    let log = "";
    child.stdout.on("data", (d) => (log += d));
    child.stderr.on("data", (d) => (log += d));
    const url = `http://127.0.0.1:${port}/`;
    for (let i = 0; i < 150; i++) {
        try {
            if ((await fetch(url)).ok) return { url, stop: () => child.kill("SIGTERM") };
        } catch {
            /* not up yet */
        }
        await sleep(200);
    }
    child.kill("SIGTERM");
    throw new Error(`Vite didn't start:\n${log}`);
}

// ── Browsers ────────────────────────────────────────────────────────────

/** A real 3840x2160 page (headless Chromium, forced device scale factor). */
async function filmPage(browser) {
    const context = await browser.newContext({ viewport: null });
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    // Size the window so the page is exactly 1440x810
    const { windowId } = await cdp.send("Browser.getWindowForTarget");
    for (let i = 0; i < 4; i++) {
        const [iw, ih] = await page.evaluate(() => [innerWidth, innerHeight]);
        if (iw === FILM.w && ih === FILM.h) break;
        const { bounds } = await cdp.send("Browser.getWindowBounds", { windowId });
        await cdp.send("Browser.setWindowBounds", {
            windowId,
            bounds: { width: bounds.width + FILM.w - iw, height: bounds.height + FILM.h - ih },
        });
        await sleep(100);
    }
    const [iw, ih, dpr] = await page.evaluate(() => [innerWidth, innerHeight, devicePixelRatio]);
    if (iw !== FILM.w || ih !== FILM.h || Math.abs(dpr - FILM.scale) > 0.01) {
        throw new Error(`Film page is ${iw}x${ih}@${dpr}, wanted ${FILM.w}x${FILM.h}@${FILM.scale}`);
    }
    return { page, context, cdp };
}

// ── Screencast recorder ─────────────────────────────────────────────────

class Recorder {
    constructor(cdp, name, out, slow = 1) {
        this.cdp = cdp;
        this.name = name;
        this.out = out;
        this.slow = slow;
        this.dir = path.join(RAW, name);
        this.frames = [];
        this.writes = [];
        // Ack at once and keep the JPEGs in memory until the cut: Chromium
        // waits for the ack before it sends the next frame
        this.onFrame = (f) => {
            this.cdp.send("Page.screencastFrameAck", { sessionId: f.sessionId }).catch(() => {});
            const file = path.join(this.dir, `f${String(this.frames.length).padStart(6, "0")}.jpg`);
            this.frames.push({ ts: f.metadata.timestamp, file, data: f.data });
            this.lastArrival = performance.now();
        };
    }

    async start() {
        fs.rmSync(this.dir, { recursive: true, force: true });
        fs.mkdirSync(this.dir, { recursive: true });
        this.cdp.on("Page.screencastFrame", this.onFrame);
        await this.cdp.send("Page.startScreencast", {
            format: "jpeg",
            quality: Number(process.env.FILM_JPEG_QUALITY || 96),
            maxWidth: OUT.w,
            maxHeight: OUT.h,
            everyNthFrame: 1,
        });
        for (let i = 0; i < 200 && this.frames.length === 0; i++) await sleep(10);
        if (this.frames.length === 0) throw new Error("No screencast frames");
    }

    async stop() {
        const stoppedAt = performance.now();
        await this.cdp.send("Page.stopScreencast");
        this.cdp.off("Page.screencastFrame", this.onFrame);
        const frames = this.frames;
        for (const f of frames) {
            await fs.promises.writeFile(f.file, Buffer.from(f.data, "base64"));
            f.data = null;
        }
        // Page time: real frame times divided by the slow-motion factor
        const t0 = frames[0].ts;
        for (const f of frames) f.ts = (f.ts - t0) / this.slow;
        const endTs = frames[frames.length - 1].ts + (stoppedAt - this.lastArrival) / 1000 / this.slow;
        // Constant 30 fps: output frame n shows the screen as it was at n/30 s
        // (the newest source frame by then). Written as a numbered sequence of
        // hard links, so the timing is exact (ffmpeg's concat demuxer rounds
        // image times to 1/25 s).
        const seq = path.join(this.dir, "seq");
        fs.mkdirSync(seq);
        const total = Math.max(1, Math.round(endTs * 30));
        for (let n = 0, i = 0; n < total; n++) {
            const t = n / 30;
            while (i + 1 < frames.length && frames[i + 1].ts <= t + 1e-6) i++;
            fs.linkSync(frames[i].file, path.join(seq, `s${String(n).padStart(6, "0")}.jpg`));
        }
        const out = this.out;
        const r = spawnSync(
            "ffmpeg",
            [
                "-y", "-loglevel", "error",
                "-framerate", "30", "-i", path.join(seq, "s%06d.jpg"),
                // Screencast JPEGs are full-range BT.601: convert to limited-range BT.709
                "-vf",
                `scale=${OUT.w}:${OUT.h}:flags=lanczos+accurate_rnd+full_chroma_int:in_range=full:out_range=limited:in_color_matrix=bt601:out_color_matrix=bt709,format=yuv420p`,
                "-c:v", "libx264", "-preset", "medium", "-crf", "15", "-profile:v", "high", "-level:v", "5.1",
                "-pix_fmt", "yuv420p", "-color_range", "tv", "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
                "-r", "30", "-movflags", "+faststart", out,
            ],
            { stdio: "inherit" },
        );
        if (r.status !== 0) throw new Error(`ffmpeg failed for ${this.name}`);
        const span = endTs - frames[0].ts;
        if (process.env.FILM_DEBUG) {
            const gaps = frames
                .slice(1)
                .map((f, i) => [frames[i].ts - frames[0].ts, f.ts - frames[i].ts])
                .filter(([, g]) => g > 0.05)
                .map(([at, g]) => `${at.toFixed(2)}s+${Math.round(g * 1000)}ms`);
            console.log(`    source gaps > 50 ms: ${gaps.join(" ")}`);
        }
        console.log(`  ${path.relative(REPO, out)}  ${span.toFixed(2)} s, ${frames.length} source frames (${(frames.length / span).toFixed(0)} fps)`);
        fs.rmSync(this.dir, { recursive: true, force: true });
        // Stutters: pauses too long for motion but too short for a hold
        // (a busy Mac drops frames mid-animation)
        const stutters = frames.slice(1).filter((f, i) => {
            const g = f.ts - frames[i].ts;
            return g > 0.045 && g < 0.4;
        }).length;
        console.log(`    ${stutters} stutters`);
        return { out, span, count: frames.length, stutters };
    }
}

// ── A shot: page helpers that act at human speed ────────────────────────

class StopShot extends Error {}

class Shot {
    /** mode: "video" (record), "still" (4K PNG at the key state), "store" (App Store PNG) */
    constructor(page, name, mode, cdp, url, out = null) {
        this.page = page;
        this.name = name;
        this.mode = mode;
        // Page-time factor: 1 while setting up, SLOW while filming (slowmo())
        this.k = 1;
        this.cdp = cdp;
        this.url = url;
        this.out = out;
        this.stats = null;
        this.mouse = { x: 720, y: 400 };
        this.rec = null;
    }

    async load() {
        await this.page.goto(this.mode === "store" ? this.url : `${this.url}?film=1`, { waitUntil: "networkidle" });
        await this.page.waitForSelector(".agency-layout", { timeout: 20000 });
        await this.page.evaluate(() => document.fonts.ready);
        await this.page.mouse.move(this.mouse.x, this.mouse.y);
        await this.wait(600);
    }

    /** Slow page time down for filming (video takes only; stills need no
     *  motion). Live clips call it before the recording starts, so the
     *  simulated speech is scheduled in slow motion too. */
    async slowmo() {
        if (this.mode !== "video" || this.k === SLOW) return;
        await this.page.evaluate((k) => window.__harness.setSlow(k), SLOW);
        // CSS transitions/animations and Web Animations at the same speed
        await this.cdp.send("Animation.enable");
        await this.cdp.send("Animation.setPlaybackRate", { playbackRate: 1 / SLOW });
        this.k = SLOW;
    }

    /** Wait in page time (slow motion makes it longer in real time). */
    wait(ms) {
        return sleep(ms * this.k);
    }

    /** Instant navigation before the camera rolls (a selector or a locator) */
    async go(target) {
        if (typeof target === "string") await this.page.click(target);
        else await target.click();
        await this.wait(350);
    }

    async record() {
        if (this.mode !== "video") return;
        await this.slowmo();
        this.rec = new Recorder(this.cdp, this.name, this.out, this.k);
        await this.rec.start();
        this.t0 = performance.now();
        this.log("recording");
        if (process.env.FILM_DEBUG) {
            await this.page.evaluate(() => {
                window.__rafGaps = [];
                const t0 = performance.now();
                let last = t0;
                const f = (t) => {
                    if (t - last > 50) window.__rafGaps.push(`${((last - t0) / 1000).toFixed(2)}s+${Math.round(t - last)}ms`);
                    last = t;
                    requestAnimationFrame(f);
                };
                requestAnimationFrame(f);
            });
        }
    }

    async cut() {
        if (process.env.FILM_DEBUG && this.mode === "video") {
            console.log(`    page rAF gaps > 50 ms: ${(await this.page.evaluate(() => window.__rafGaps ?? [])).join(" ")}`);
        }
        if (this.mode === "video" && this.rec) this.stats = await this.rec.stop();
    }

    /** The key state: the still is taken here (and the still run ends). */
    async key(file) {
        if (this.mode === "video") return;
        await this.wait(250);
        const out = file ?? path.join(CLIPS_OUT, `${this.name}.png`);
        await this.page.screenshot({ path: out, type: "png" });
        console.log(`  ${path.relative(REPO, out)}`);
        throw new StopShot();
    }

    async hold(ms) {
        await this.wait(ms);
        this.log(`held ${ms}`);
    }

    async point(target) {
        if (typeof target === "string") target = this.page.locator(target).first();
        if (target.x !== undefined) return target;
        await target.waitFor({ state: "visible", timeout: 10000 });
        const b = await target.boundingBox();
        if (!b) throw new Error(`No box for ${target}`);
        return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
    }

    /** Glide the (invisible) mouse to a target, easing in and out. Timed
     *  by the clock, so a slow frame doesn't stretch the move. */
    async moveTo(target, ms = 420) {
        const p = await this.point(target);
        const from = { ...this.mouse };
        const t0 = performance.now();
        const dur = ms * this.k;
        for (;;) {
            const t = Math.min(1, (performance.now() - t0) / dur);
            const e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
            await this.page.mouse.move(from.x + (p.x - from.x) * e, from.y + (p.y - from.y) * e);
            if (t >= 1) break;
            await sleep(8);
        }
        this.mouse = p;
        this.log(`moved to ${typeof target === "string" ? target : ""}`);
    }

    log(what) {
        if (!process.env.FILM_DEBUG) return;
        this.t0 ??= performance.now();
        console.log(`    ${((performance.now() - this.t0) / 1000 / this.k).toFixed(2)}s ${what}`);
    }

    async click(target, { move = 420, pause = 80 } = {}) {
        await this.moveTo(target, move);
        await this.wait(pause);
        await this.page.mouse.down();
        await this.wait(90);
        await this.page.mouse.up();
    }

    async type(text, delay = 95) {
        for (const ch of text) {
            await this.page.keyboard.type(ch);
            await this.wait(delay + Math.round(Math.sin(text.length + ch.charCodeAt(0)) * 25));
        }
    }

    /** Scroll an element smoothly (the page animates it, like a trackpad glide). */
    async scrollTo(selector, top, ms) {
        await this.page.evaluate(
            ([sel, to, dur]) =>
                new Promise((done) => {
                    const el = document.querySelector(sel);
                    el.style.scrollBehavior = "auto";
                    const from = el.scrollTop;
                    const target = to === "end" ? el.scrollHeight - el.clientHeight : to;
                    const t0 = performance.now();
                    const step = (now) => {
                        const t = Math.min(1, (now - t0) / dur);
                        const e = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
                        el.scrollTop = from + (target - from) * e;
                        if (t < 1) requestAnimationFrame(step);
                        else done();
                    };
                    requestAnimationFrame(step);
                }),
            [selector, top, ms],
        );
    }

    /** Wait until every <img> matching the selector has loaded. */
    async imagesLoaded(selector, min = 1) {
        await this.page.waitForFunction(
            ([sel, n]) => {
                const imgs = [...document.querySelectorAll(sel)];
                return imgs.length >= n && imgs.every((i) => i.complete && i.naturalWidth > 0);
            },
            [selector, min],
            { timeout: 15000 },
        );
    }

    harness(fn, ...args) {
        return this.page.evaluate(([f, a]) => window.__harness[f](...a), [fn, args]);
    }
}

// Common setups (instant, before recording)
async function openRecording(s, title, tab = null) {
    await s.go('button.mode-btn:has-text("REWIND")');
    await s.go(`.meeting-item:has-text("${title}")`);
    await s.wait(600);
    if (tab) await s.go(`button[role=tab]:has-text("${tab}")`);
    await s.wait(500);
}

async function rewindReady(s, collapseMarkers = true) {
    await s.imagesLoaded(".rewind-thumbnails .thumbnail img", 7);
    await s.imagesLoaded(".rewind-frame-preview img", 1);
    // Show every screen once so the big images are loaded and decoded
    // before the camera rolls
    const n = await s.page.locator(".rewind-thumbnails .thumbnail").count();
    for (let i = n - 1; i >= 0; i--) {
        await s.page.locator(".rewind-thumbnails .thumbnail").nth(i).click();
        await s.imagesLoaded(".rewind-frame-preview img", 1);
        await s.wait(120);
    }
    await s.wait(500);
    if (collapseMarkers) {
        // More room for the transcript: the marks stay as pins and inline chips
        await s.go(".study-markers__toggle");
    }
}

/** Scroll the transcript so its first visible line starts cleanly at the
 *  top while the highlighted line stays fully in view (for stills). */
async function alignTranscript(s) {
    await s.page.evaluate(() => {
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
        // The highlighted line lowest in view: the lines before it give context
        if (fits.length) panel.scrollTop = Math.min(...fits);
    });
    await s.wait(300);
}

const thumb = (s, i) => s.page.locator(".rewind-thumbnails .thumbnail").nth(i);
const pin = (s, kind) => s.page.locator(`.study-pins .study-pin.is-${kind}`).first();

// ── Film clips ──────────────────────────────────────────────────────────

const CLIPS = {
    // The signature shot: screenshots in sync with the transcript, marks on the timeline
    async "mac-01-rewind"(s) {
        await s.load();
        await openRecording(s, "Acme project sync");
        await rewindReady(s);
        await s.moveTo(".rewind-frame-preview", 1);
        await s.record();
        await s.hold(700);
        await s.click(thumb(s, 1), { move: 380 }); // roadmap, 1:46
        await s.hold(1000);
        await s.click(pin(s, "important"), { move: 380 }); // ★ 3:32: the decision, annotated slide
        await s.hold(1400);
        if (s.mode !== "video") await alignTranscript(s);
        await s.key();
        await s.click(thumb(s, 3), { move: 380 }); // sign-ups chart, 4:36
        await s.hold(1000);
        await s.click(pin(s, "question"), { move: 380 }); // ? 8:42: beta feedback
        await s.hold(1200);
        await s.click(pin(s, "test"), { move: 380 }); // ✎ Follow up 15:51: launch checklist
        await s.hold(1900);
        await s.cut();
    },

    // Recording: lines streaming in, captures and live insights arriving
    async "mac-02-live"(s) {
        await s.load();
        await s.slowmo();
        await s.harness("startLive", "meeting", { prefill: 3, backdateSec: 42, duration: "60" });
        await s.wait(1300);
        await s.moveTo({ x: 1200, y: 760 }, 1);
        await s.record();
        await s.hold(9300);
        await s.key();
        await s.cut();
    },

    // The idle home screen: "Ready when you are" and the privacy promise
    async "mac-03-home"(s) {
        await s.load();
        await s.moveTo({ x: 720, y: 790 }, 1);
        await s.record();
        await s.key();
        await s.hold(5000);
        await s.cut();
    },

    // Record → What is it? Class · 60 min · BIO 101 → Start
    async "mac-04-record-picker"(s) {
        await s.load();
        await s.moveTo({ x: 720, y: 560 }, 1);
        await s.record();
        await s.hold(500);
        await s.click(".lt-empty__action", { move: 340 });
        await s.hold(700);
        await s.click('.rpick__kind:has-text("Class")', { move: 320 });
        await s.hold(350);
        await s.click('.rpick__choice:has-text("60")', { move: 320 });
        await s.hold(350);
        await s.click("#rpick-notebook", { move: 300 });
        await s.type("BIO", 100);
        await s.hold(150);
        await s.click('.rpick__chip:has-text("BIO 101")', { move: 260 });
        await s.hold(600);
        await s.key();
        await s.click(".rpick__go", { move: 380 });
        await s.hold(1900);
        await s.cut();
    },

    // While recording a class: Mark → On the test → a note
    async "mac-05-mark"(s) {
        await s.load();
        // 4 lines in; "Write that one down. It will be on the exam." is being said
        await s.slowmo();
        await s.harness("startLive", "class", { prefill: 4, backdateSec: 150, duration: "60" });
        await s.wait(1300);
        await s.moveTo({ x: 1100, y: 600 }, 1);
        await s.record();
        await s.hold(900);
        await s.click(".study-mark__btn", { move: 450 });
        await s.hold(1000);
        await s.click(".study-mark__card .study-kind.is-test", { move: 380 });
        await s.hold(600);
        await s.click(".study-mark__card .study-input", { move: 300 });
        await s.type("Pump: 3 Na out, 2 K in", 60);
        await s.hold(900);
        await s.key();
        await s.page.keyboard.press("Enter");
        await s.hold(1000);
        await s.cut();
    },

    // The lecture's Study guide: flip a flashcard, answer a quiz question
    async "mac-06-review"(s) {
        await s.load();
        await openRecording(s, "BIO 101: Cell Biology", "REVIEW");
        await s.page.waitForSelector(".study-tabs");
        await s.moveTo({ x: 900, y: 700 }, 1);
        await s.record();
        await s.hold(1000);
        await s.click('.study-tabs button:has-text("Flashcards")', { move: 450 });
        await s.hold(700);
        await s.click(".study-card", { move: 420 });
        await s.hold(1300);
        await s.click('.study-tabs button:has-text("Practice quiz")', { move: 450 });
        await s.hold(900);
        await s.click(s.page.locator(".study-choice").nth(1), { move: 420 });
        await s.hold(1700);
        await s.key();
        await s.cut();
    },

    // Meeting notes written on this Mac: summary, decisions, action items
    async "mac-07-notes"(s) {
        await s.load();
        await openRecording(s, "Acme project sync", "NOTES");
        await s.page.waitForSelector(".mn-notes");
        const scroller = await s.page.evaluate(() => {
            let el = document.querySelector(".mn-notes");
            while (el && !(el.scrollHeight > el.clientHeight + 4 && /(auto|scroll)/.test(getComputedStyle(el).overflowY))) el = el.parentElement;
            if (!el) return null;
            el.setAttribute("data-film-scroll", "1");
            return el.className;
        });
        await s.moveTo({ x: 900, y: 500 }, 1);
        await s.record();
        await s.key();
        await s.hold(1100);
        if (scroller) await s.scrollTo("[data-film-scroll]", "end", 3200);
        await s.hold(1200);
        await s.cut();
    },

    // The library, filtered by notebook
    async "mac-08-library"(s) {
        await s.load();
        await s.go('button.mode-btn:has-text("REWIND")');
        await s.page.waitForSelector(".meeting-item");
        await s.wait(800); // the view's fade-in
        await s.moveTo({ x: 700, y: 600 }, 1);
        await s.record();
        await s.hold(1100);
        await s.click('.class-filter .class-chip:has-text("BIO 101")', { move: 600 });
        await s.hold(2400);
        await s.key();
        await s.cut();
    },

    // Links: a site said in the meeting → jump to that moment in Rewind
    async "mac-09-links"(s) {
        await s.load();
        await openRecording(s, "Acme project sync", "LINKS");
        await s.page.waitForSelector(".ml-row");
        await s.moveTo({ x: 900, y: 650 }, 1);
        await s.record();
        await s.key();
        await s.hold(1300);
        await s.click('.ml-link-btn:has-text("first said at 5:48")', { move: 520 });
        await s.hold(2400);
        await s.cut();
    },
};

// ── App Store screenshots (1440x900 @2x) ────────────────────────────────

const STORE_SHOTS = {
    async "01-rewind"(s) {
        await s.load();
        await openRecording(s, "Acme project sync");
        await rewindReady(s);
        await s.page.locator(".study-pins .study-pin.is-important").click();
        await sleep(1200);
        await s.page.mouse.move(1000, 500);
        await alignTranscript(s);
        await sleep(300);
    },
    async "02-record-sheet"(s) {
        await s.load();
        await s.go(".lt-empty__action");
        await s.go('.rpick__kind:has-text("Class")');
        await s.go('.rpick__choice:has-text("60")');
        await s.go("#rpick-notebook");
        await s.page.keyboard.type("BIO");
        await s.go('.rpick__chip:has-text("BIO 101")');
        await s.page.mouse.move(720, 880);
        await sleep(500);
    },
    async "03-review"(s) {
        await s.load();
        await openRecording(s, "BIO 101: Cell Biology", "REVIEW");
        await s.go('.study-tabs button:has-text("Practice quiz")');
        await s.go(s.page.locator(".study-choice").nth(1));
        await s.page.mouse.move(1200, 860);
        await sleep(500);
    },
    async "04-links"(s) {
        await s.load();
        await openRecording(s, "Acme project sync", "LINKS");
        await s.page.waitForSelector(".ml-row");
        await s.page.mouse.move(900, 860);
        await sleep(500);
    },
    async "05-notebooks"(s) {
        await s.load();
        await s.go('button.mode-btn:has-text("REWIND")');
        await s.go('.class-filter .class-chip:has-text("BIO 101")');
        await s.page.mouse.move(900, 880);
        await sleep(700);
    },
};

// ── Main ────────────────────────────────────────────────────────────────

async function runStore(server, names) {
    fs.mkdirSync(STILLS_OUT, { recursive: true });
    const browser = await chromium.launch();
    const context = await browser.newContext({ viewport: { width: STORE.w, height: STORE.h }, deviceScaleFactor: STORE.scale });
    const page = await context.newPage();
    page.on("pageerror", (e) => console.warn("  [page error]", e.message));
    console.log("App Store screenshots");
    for (const name of names) {
        const s = new Shot(page, name, "store", null, server.url);
        await STORE_SHOTS[name](s);
        const out = path.join(STILLS_OUT, `${name}.png`);
        await page.screenshot({ path: out, type: "png" });
        console.log(`  ${path.relative(REPO, out)}`);
    }
    await browser.close();
}

async function runClips(server, names) {
    fs.mkdirSync(CLIPS_OUT, { recursive: true });
    const browser = await chromium.launch({
        channel: "chromium", // new headless: full compositor
        args: [`--force-device-scale-factor=${FILM.scale}`, `--window-size=${FILM.w},${FILM.h + 100}`, "--hide-scrollbars"],
    });
    // FILM_TAKES takes per clip (default 1), plus up to 2 more when a take
    // drops frames mid-motion (more than 20 stutters) or fails; the take with
    // the fewest stutters is kept
    const takes = Math.max(1, Number(process.env.FILM_TAKES || 1));
    console.log("Film clips");
    const run = async (name, mode, out) => {
        for (let attempt = 1; ; attempt++) {
            const { page, context, cdp } = await filmPage(browser);
            page.on("pageerror", (e) => console.warn("  [page error]", e.message));
            const s = new Shot(page, name, mode, cdp, server.url, out);
            try {
                await CLIPS[name](s);
            } catch (e) {
                if (!(e instanceof StopShot)) {
                    await context.close();
                    if (attempt >= 3) throw e;
                    console.warn(`  ${name} (${mode}) failed, trying again: ${String(e).split("\n")[0]}`);
                    continue;
                }
            }
            await context.close();
            return s.stats;
        }
    };
    for (const name of names) {
        const done = [];
        while (done.length < takes || (done.length < takes + 2 && Math.min(...done.map((t) => t.stutters)) > 20)) {
            done.push(await run(name, "video", path.join(CLIPS_OUT, `${name}.take${done.length + 1}.mp4`)));
        }
        await run(name, "still", null);
        const best = done.reduce((a, b) =>
            b.stutters < a.stutters || (b.stutters === a.stutters && b.count / b.span > a.count / a.span) ? b : a,
        );
        for (const t of done) if (t !== best) fs.rmSync(t.out, { force: true });
        const final = path.join(CLIPS_OUT, `${name}.mp4`);
        fs.renameSync(best.out, final);
        console.log(`  → ${path.relative(REPO, final)} (take ${done.indexOf(best) + 1} of ${done.length})`);
    }
    await browser.close();
}

const args = process.argv.slice(2);
const wantStore = args.length === 0 || args.includes("stills");
const clipNames = args.length === 0 || args.includes("clips") ? Object.keys(CLIPS) : args.filter((a) => a in CLIPS);
const unknown = args.filter((a) => !(a in CLIPS) && a !== "stills" && a !== "clips");
if (unknown.length) {
    console.error(`Unknown: ${unknown.join(", ")}. Clips: ${Object.keys(CLIPS).join(", ")}`);
    process.exit(1);
}

const server = await startServer();
try {
    // Warm up Vite (first load optimizes dependencies and may reload)
    {
        const b = await chromium.launch();
        const p = await b.newPage();
        await p.goto(server.url, { waitUntil: "networkidle" });
        await p.waitForSelector(".agency-layout", { timeout: 60000 });
        await b.close();
    }
    if (wantStore) await runStore(server, Object.keys(STORE_SHOTS));
    if (clipNames.length) await runClips(server, clipNames);
} finally {
    server.stop();
}
