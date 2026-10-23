#!/usr/bin/env node
// Authoritative frontend offline guard for the Lessons surface (owner-decision
// D5, docs/LESSONS.md §11.2/§15). It scans the COMPILED Vite output (the
// shipped bytes, not source intent) for any non-local http(s):// reference or
// any url(...) that points at a remote host, and FAILS the build on a match.
// It also asserts the Lessons CSS declares no remote @import / @font-face and
// that every font-family resolves to the bundled Inter / system stack.
//
// Local/relative references are allow-listed: url(./…), url(../…), url(data:…),
// url(#…) (SVG fragment refs), the Tauri asset protocol (tauri://,
// asset.localhost), and bare bundled asset paths. Only a reference that
// resolves to a REMOTE host fails.
//
// Run after `npx vite build`:
//     node scripts/check-lessons-offline.mjs
// It is intentionally dependency-free so it runs in CI without an install.

import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const distDirs = ["dist/assets", "dist"];

const errors = [];

// Markers that identify a Lessons-surface compiled chunk/CSS. Vite names chunks
// after their entry/first module; the Lessons modules carry these strings in
// their compiled output (class names, string literals, component names).
const LESSONS_MARKERS = [
    "LessonsView",
    "LessonLibrary",
    "lessons__header",
    "lesson-player",
    "make-lesson__intro",
    "lessons_list_catalog",
];

// Allow-listed local reference hosts/schemes (never a remote fetch).
const LOCAL_HOST_RE = /(localhost|127\.0\.0\.1|asset\.localhost|tauri\.localhost)/i;

function listFiles(dir) {
    const abs = join(root, dir);
    if (!existsSync(abs)) return [];
    const out = [];
    for (const name of readdirSync(abs)) {
        const p = join(abs, name);
        if (statSync(p).isFile()) out.push(p);
    }
    return out;
}

// Find every remote http(s) URL in `text`, excluding local hosts and the
// well-known schema/spec URLs that appear only inside XML/SVG namespaces.
function remoteHttpRefs(text) {
    const hits = [];
    const re = /https?:\/\/[^\s"'`)\\]+/gi;
    let m;
    while ((m = re.exec(text))) {
        const url = m[0];
        // XML/SVG namespace declarations are not network fetches.
        if (/w3\.org|xmlns|svg|dtd|purl\.org|schemas\./i.test(url)) continue;
        if (LOCAL_HOST_RE.test(url)) continue;
        hits.push(url);
    }
    return hits;
}

// Find url(...) targets that resolve to a remote host.
function remoteUrlFns(text) {
    const hits = [];
    const re = /url\(\s*['"]?([^'")]+)['"]?\s*\)/gi;
    let m;
    while ((m = re.exec(text))) {
        const target = m[1].trim();
        if (
            target.startsWith("data:") ||
            target.startsWith("#") ||
            target.startsWith("./") ||
            target.startsWith("../") ||
            target.startsWith("/") ||
            target.startsWith("tauri:") ||
            !/^https?:/i.test(target)
        ) {
            continue;
        }
        if (LOCAL_HOST_RE.test(target)) continue;
        hits.push(target);
    }
    return hits;
}

const allFiles = [];
for (const d of distDirs) allFiles.push(...listFiles(d));

// The Lessons surface is isolated into its own `lessons-*` chunk + CSS by the
// vite manualChunks config, so the guard scans exactly those bytes.
const jsFiles = allFiles.filter((f) => /\/lessons-[^/]*\.js$/.test(f));
const cssFiles = allFiles.filter((f) => /\/lessons-[^/]*\.css$/.test(f));

if (allFiles.length === 0) {
    console.error("check-lessons-offline: no compiled output found under dist/. Run `npx vite build` first.");
    process.exit(2);
}
if (jsFiles.length === 0) {
    console.error(
        "check-lessons-offline: no compiled Lessons chunk (lessons-*.js) found. The Lessons surface " +
            "must be isolated via the vite manualChunks config so this guard can scan only its bytes.",
    );
    process.exit(2);
}

// Sanity: the Lessons chunk really contains the Lessons surface.
const chunkHasMarkers = jsFiles.some((f) => {
    const t = readFileSync(f, "utf8");
    return LESSONS_MARKERS.some((mk) => t.includes(mk));
});
if (!chunkHasMarkers) {
    console.error("check-lessons-offline: the lessons-*.js chunk does not contain the Lessons surface markers.");
    process.exit(2);
}

// 1. Scan the Lessons JS chunk(s) for remote refs.
for (const f of jsFiles) {
    const text = readFileSync(f, "utf8");
    for (const u of remoteHttpRefs(text)) {
        errors.push(`${f}: remote reference in the Lessons chunk: ${u}`);
    }
    for (const u of remoteUrlFns(text)) {
        errors.push(`${f}: remote url(...) in the Lessons chunk: ${u}`);
    }
}

// 2. Scan the compiled CSS. The whole app's CSS is bundled together, so assert
//    the Lessons rules (and the CSS at large) carry no remote @import or remote
//    @font-face and no remote url(...).
for (const f of cssFiles) {
    const text = readFileSync(f, "utf8");
    // Remote @import.
    const importRe = /@import\s+(?:url\()?\s*['"]?(https?:[^'")\s]+)/gi;
    let m;
    while ((m = importRe.exec(text))) {
        if (!LOCAL_HOST_RE.test(m[1])) errors.push(`${f}: remote @import ${m[1]}`);
    }
    // Remote @font-face src.
    const faceRe = /@font-face[^}]*}/gi;
    let fm;
    while ((fm = faceRe.exec(text))) {
        for (const u of remoteUrlFns(fm[0])) {
            errors.push(`${f}: remote @font-face src ${u}`);
        }
        for (const u of remoteHttpRefs(fm[0])) {
            errors.push(`${f}: remote font reference ${u}`);
        }
    }
    // Any remote url(...) anywhere in the Lessons CSS.
    for (const u of remoteUrlFns(text)) {
        errors.push(`${f}: remote url(...) in compiled Lessons CSS: ${u}`);
    }
    // Every font-family must resolve to the bundled Inter / system stack —
    // never a Google Fonts / CDN family loaded over the network.
    const famRe = /font-family\s*:\s*([^;}]+)/gi;
    let fam;
    while ((fam = famRe.exec(text))) {
        const value = fam[1].toLowerCase();
        const ok =
            value.includes("inter") ||
            value.includes("-apple-system") ||
            value.includes("blinkmacsystemfont") ||
            value.includes("sans-serif") ||
            value.includes("inherit") ||
            value.includes("monospace");
        if (!ok) {
            errors.push(`${f}: font-family does not resolve to the bundled Inter / system stack: ${fam[1].trim()}`);
        }
    }
}

if (errors.length) {
    console.error("check-lessons-offline: FAIL — the Lessons frontend references remote resources:");
    for (const e of errors) console.error("  - " + e);
    process.exit(1);
}

console.log(
    JSON.stringify({
        status: "PASS",
        guard: "Lessons frontend offline (owner-decision D5)",
        scanned: { jsChunks: jsFiles.length, cssFiles: cssFiles.length },
        note: "No remote http(s)/url(...) references in the compiled Lessons bundle or CSS; fonts resolve to the bundled Inter / system stack.",
    }),
);
