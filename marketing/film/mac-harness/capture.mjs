// Films the Mac app's real UI (the 3.7 window: Record · Recordings · Chat,
// running in the harness with mocked data) for the launch film and the Mac
// App Store app preview. The screenshots are capture-store-3.7.mjs.
//
//   npm --prefix marketing/film/mac-harness install   # playwright 1.58.2 (cached Chromium 1208)
//   node marketing/film/mac-harness/make-frames.mjs   # once: the fictional slides to frames/
//   node marketing/film/mac-harness/capture.mjs                  # every clip (video + 4K still)
//   node marketing/film/mac-harness/capture.mjs mac-03-rewind    # some clips
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
const RAW = path.join(os.tmpdir(), "nofriction-film-raw");

// Film: a 1440x810 window rendered at 3840x2160
// (FILM_RES=1080 renders at 1920x1080 instead)
const HD = process.env.FILM_RES === "1080";
const FILM = { w: 1440, h: 810, scale: HD ? 4 / 3 : 8 / 3 };
const OUT = HD ? { w: 1920, h: 1080 } : { w: 3840, h: 2160 };
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
    /** mode: "video" (record) or "still" (4K PNG at the key state) */
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
        await this.page.goto(`${this.url}?film=1`, { waitUntil: "networkidle" });
        await this.page.waitForSelector(".topbar", { timeout: 20000 });
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
async function mode(s, label) {
    await s.go(`.topbar__mode:has-text("${label}")`);
    await s.wait(300);
}

async function openRecording(s, title, tab = null) {
    await mode(s, "Recordings");
    await s.go(s.page.locator(".meeting-item", { hasText: title }).first());
    await s.wait(600);
    if (tab) await s.go(s.page.locator(".recording-detail > .deck-tabs .deck-tab", { hasText: tab }).first());
    await s.wait(500);
}

/** Every screen of the open recording loaded and decoded before the camera rolls. */
async function rewindReady(s) {
    await s.imagesLoaded(".rewind-thumbnails .thumbnail img", 6);
    await s.imagesLoaded(".rewind-frame-preview img", 1);
    const n = await s.page.locator(".rewind-thumbnails .thumbnail").count();
    for (let i = n - 1; i >= 0; i--) {
        await s.page.locator(".rewind-thumbnails .thumbnail").nth(i).click();
        await s.imagesLoaded(".rewind-frame-preview img", 1);
        await s.wait(120);
    }
    await s.page.locator(".rewind-thumbnails .thumbnail").first().click();
    await s.wait(500);
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
        if (fits.length) panel.scrollTop = Math.min(...fits);
    });
    await s.wait(300);
}

const thumb = (s, i) => s.page.locator(".rewind-thumbnails .thumbnail").nth(i);
const pin = (s, kind) => s.page.locator(`.study-pins .study-pin.is-${kind}`).first();

/** A point on the Rewind timeline slider, `f` (0..1) along it. */
async function sliderAt(s, f) {
    const b = await s.page.locator(".timeline-slider").boundingBox();
    // the thumb's travel is inset by its radius (about 8 px)
    return { x: b.x + 8 + (b.width - 16) * f, y: b.y + b.height / 2 };
}

// ── Film clips (the 3.7 window: Record · Recordings · Chat) ─────────────
//
// Cut notes (s) are in ../src/clips.ts.

const CLIPS = {
    // Record → What is it? Class · 60 min · BIO 101 → Start recording → the
    // capture bar, the first words and screens arriving
    async "mac-01-record"(s) {
        await s.load();
        await s.moveTo({ x: 720, y: 640 }, 1);
        await s.slowmo();
        await s.record();
        await s.hold(600);
        await s.click(".lt-empty__action", { move: 380 });
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
        await s.moveTo({ x: 1100, y: 700 }, 600);
        await s.hold(5200);
        await s.cut();
    },

    // While recording a class: Mark → On the test → a note → Stop → the
    // recording opens in Recordings, its Rewind with the new mark
    async "mac-02-mark-stop"(s) {
        await s.load();
        // 4 lines in; "Write that one down. It will be on the exam." is being said
        await s.slowmo();
        await s.harness("startLive", "class", { prefill: 4, backdateSec: 150, duration: "60" });
        await s.wait(1300);
        await s.moveTo({ x: 1000, y: 500 }, 1);
        await s.record();
        await s.hold(1400);
        await s.click(".study-mark__btn", { move: 450 });
        await s.hold(900);
        await s.click(".study-mark__card .study-kind.is-test", { move: 380 });
        await s.hold(500);
        await s.click(".study-mark__card .study-input", { move: 300 });
        await s.type("Pump: 3 Na out, 2 K in", 55);
        await s.hold(700);
        await s.key();
        await s.page.keyboard.press("Enter");
        await s.hold(1400);
        await s.click(".topbar__stop", { move: 520 });
        await s.hold(1500);
        await s.click(pin(s, "test"), { move: 600 });
        await s.hold(2200);
        await s.cut();
    },

    // The signature shot: Recordings → a meeting → Rewind, scrubbing the
    // timeline: every screen in step with what was said, the marks as pins
    async "mac-03-rewind"(s) {
        await s.load();
        await openRecording(s, "Acme project sync");
        await rewindReady(s);
        await s.moveTo({ x: 700, y: 420 }, 1);
        await s.record();
        await s.hold(600);
        // drag the playhead from 0:10 to the roadmap, past the decision
        const a = await sliderAt(s, 0.01);
        await s.moveTo(a, 450);
        await s.wait(80);
        await s.page.mouse.down();
        const b = await sliderAt(s, 0.17);
        const steps = 40;
        for (let i = 1; i <= steps; i++) {
            const t = i / steps;
            const e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
            await s.page.mouse.move(a.x + (b.x - a.x) * e, a.y);
            await s.wait(1400 / steps);
        }
        await s.page.mouse.up();
        s.mouse = b;
        await s.hold(900);
        await s.click(pin(s, "important"), { move: 380 }); // ★ 3:32 the decision, annotated slide
        await s.hold(1500);
        if (s.mode !== "video") await alignTranscript(s);
        await s.key();
        await s.click(thumb(s, 3), { move: 420 }); // sign-ups chart, 4:36
        await s.hold(1000);
        await s.click(pin(s, "question"), { move: 420 }); // ? beta feedback
        await s.hold(1300);
        await s.cut();
    },

    // A meeting's Notes: summary, decisions, action items, scrolled
    async "mac-04-notes"(s) {
        await s.load();
        await openRecording(s, "Acme project sync", "Notes");
        await s.page.waitForSelector(".mn-notes, .mn-section", { timeout: 10000 });
        await s.page.evaluate(() => {
            let el = document.querySelector(".mn-notes, .mn-section");
            while (el && !(el.scrollHeight > el.clientHeight + 4 && /(auto|scroll)/.test(getComputedStyle(el).overflowY))) el = el.parentElement;
            el?.setAttribute("data-film-scroll", "1");
        });
        await s.moveTo({ x: 1000, y: 500 }, 1);
        await s.record();
        await s.key();
        await s.hold(1500);
        if (await s.page.locator("[data-film-scroll]").count()) await s.scrollTo("[data-film-scroll]", "end", 3000);
        await s.hold(1200);
        await s.cut();
    },

    // The lecture's Study guide: flip a flashcard, answer a quiz question
    async "mac-05-review"(s) {
        await s.load();
        await openRecording(s, "BIO 101: Cell Biology", "Study");
        await s.page.waitForSelector(".study-tabs");
        await s.moveTo({ x: 900, y: 700 }, 1);
        await s.record();
        await s.hold(900);
        await s.click('.study-tabs .deck-tab:has-text("Flashcards")', { move: 450 });
        await s.hold(700);
        await s.click(".study-card", { move: 420 });
        await s.hold(1300);
        await s.click('.study-tabs .deck-tab:has-text("Practice quiz")', { move: 450 });
        await s.hold(900);
        await s.click(s.page.locator(".study-choice").nth(1), { move: 420 });
        await s.hold(1900);
        await s.key();
        await s.cut();
    },

    // Chat: ask across recordings; the answer cites the moments
    async "mac-06-chat"(s) {
        await s.load();
        await mode(s, "Chat");
        await s.page.waitForSelector("textarea");
        await s.moveTo({ x: 720, y: 600 }, 1);
        await s.slowmo();
        await s.record();
        await s.hold(600);
        await s.click(s.page.locator("textarea").first(), { move: 450 });
        await s.type("What's going to be on the BIO 101 exam?", 45);
        await s.hold(300);
        await s.page.keyboard.press("Enter");
        await s.page.waitForSelector(".rc-sources__toggle", { timeout: 10000 * s.k });
        await s.hold(1300);
        await s.click(s.page.locator(".rc-sources__toggle").last(), { move: 450 });
        await s.hold(2200);
        await s.key();
        await s.cut();
    },

    // Settings → Sync → Pair a device: the code the iPhone scans
    async "mac-07-sync"(s) {
        await s.load();
        await s.moveTo({ x: 900, y: 500 }, 1);
        await s.slowmo();
        await s.record();
        await s.hold(500);
        await s.click(".topbar__gear", { move: 450 });
        await s.hold(600);
        await s.click(s.page.locator(".settings__nav-item", { hasText: /^Sync$/ }).first(), { move: 450 });
        await s.hold(800);
        await s.click('button:has-text("Pair a device")', { move: 450 });
        await s.page.waitForSelector(".sync-qr svg", { timeout: 10000 * s.k });
        await s.hold(2400);
        await s.key();
        await s.cut();
    },

    // The idle Record view: "Ready when you are" and the privacy promise
    async "mac-08-home"(s) {
        await s.load();
        await s.moveTo({ x: 720, y: 790 }, 1);
        await s.record();
        await s.key();
        await s.hold(5000);
        await s.cut();
    },

    // Recordings filtered to a notebook
    async "mac-09-notebooks"(s) {
        await s.load();
        await mode(s, "Recordings");
        await s.page.waitForSelector(".meeting-item");
        await s.wait(800); // the view's fade-in
        await s.moveTo({ x: 300, y: 600 }, 1);
        await s.record();
        await s.hold(900);
        await s.click(s.page.locator(".class-filter .class-chip", { hasText: "BIO 101" }).first(), { move: 600 });
        await s.hold(1200);
        await s.click(s.page.locator(".meeting-item", { hasText: "BIO 101: Cell Biology" }).first(), { move: 500 });
        await s.hold(2200);
        await s.key();
        await s.cut();
    },
};

// ── Main ────────────────────────────────────────────────────────────────

async function runClips(server, names) {
    fs.mkdirSync(CLIPS_OUT, { recursive: true });
    const browser = await chromium.launch({
        channel: "chromium", // new headless: full compositor
        args: [`--force-device-scale-factor=${FILM.scale}`, `--window-size=${FILM.w},${FILM.h + 100}`, "--hide-scrollbars"],
    });
    // FILM_TAKES takes per clip (default 1), plus up to 2 more when a take
    // drops frames mid-motion (more than FILM_MAX_STUTTERS, default 40) or
    // fails; the take with the fewest stutters is kept. (At 4K a take sends
    // 250+ source frames a second, so a few dozen short gaps don't show.)
    const takes = Math.max(1, Number(process.env.FILM_TAKES || 1));
    const MAX_STUTTERS = Number(process.env.FILM_MAX_STUTTERS || 40);
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
        while (done.length < takes || (done.length < takes + 2 && Math.min(...done.map((t) => t.stutters)) > MAX_STUTTERS)) {
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
const clipNames = args.length === 0 || args.includes("clips") ? Object.keys(CLIPS) : args.filter((a) => a in CLIPS);
const unknown = args.filter((a) => !(a in CLIPS) && a !== "clips");
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
        await p.waitForSelector(".topbar", { timeout: 60000 });
        await b.close();
    }
    if (clipNames.length) await runClips(server, clipNames);
} finally {
    server.stop();
}
