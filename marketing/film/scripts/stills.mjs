// Renders single frames from a composition (one bundle, many stills).
//
//   node scripts/stills.mjs <CompositionId> <outDir> <frame>[:name] ...
//   node scripts/stills.mjs Film ../out/test 200 400:rewind
//
// Writes PNGs named <name or frame>.png. Used for layout checks. Like
// render-all.mjs, it renders with Playwright's cached headless shell
// (REMOTION_BROWSER picks another), so nothing is downloaded.

import { bundle } from "@remotion/bundler";
import { renderStill, selectComposition } from "@remotion/renderer";
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");

export function localBrowser() {
  if (process.env.REMOTION_BROWSER) return process.env.REMOTION_BROWSER;
  const cache = join(homedir(), "Library", "Caches", "ms-playwright");
  if (!existsSync(cache)) return null;
  for (const d of readdirSync(cache).filter((d) => d.startsWith("chromium_headless_shell-")).sort().reverse()) {
    for (const arch of ["chrome-headless-shell-mac-arm64", "chrome-headless-shell-mac-x64"]) {
      const exe = join(cache, d, arch, "chrome-headless-shell");
      if (existsSync(exe)) return exe;
    }
  }
  return null;
}

export async function renderStills(id, outDir, frames, { scale = 1 } = {}) {
  const browserExecutable = localBrowser();
  const serveUrl = await bundle({ entryPoint: join(FILM, "src", "index.ts"), publicDir: join(FILM, "public") });
  const composition = await selectComposition({ serveUrl, id, inputProps: { music: false }, browserExecutable });
  mkdirSync(outDir, { recursive: true });
  const files = [];
  for (const f of frames) {
    const [frame, name] = String(f).split(":");
    const output = join(outDir, `${name ?? frame}.png`);
    await renderStill({ serveUrl, composition, frame: Number(frame), output, imageFormat: "png", scale, inputProps: { music: false }, overwrite: true, browserExecutable });
    files.push(output);
    console.log(output);
  }
  return files;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [id, out, ...frames] = process.argv.slice(2);
  if (!id || !out || frames.length === 0) {
    console.error("usage: node scripts/stills.mjs <CompositionId> <outDir> <frame>[:name] ...");
    process.exit(1);
  }
  await renderStills(id, resolve(out), frames);
}
