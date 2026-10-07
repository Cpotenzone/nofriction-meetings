// Renders single frames from a composition (one bundle, many stills).
//
//   node scripts/stills.mjs <CompositionId> <outDir> <frame>[:name] ...
//   node scripts/stills.mjs Film ../out/test 200 1100:rewind
//
// Writes PNGs named <name or frame>.png. Used for layout checks, the poster
// frame and the website stills (scripts/render-all.mjs).

import { bundle } from "@remotion/bundler";
import { renderStill, selectComposition } from "@remotion/renderer";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");

export async function renderStills(id, outDir, frames, { scale = 1 } = {}) {
  const serveUrl = await bundle({ entryPoint: join(FILM, "src", "index.ts"), publicDir: join(FILM, "public") });
  const composition = await selectComposition({ serveUrl, id, inputProps: { music: false } });
  mkdirSync(outDir, { recursive: true });
  const files = [];
  for (const f of frames) {
    const [frame, name] = String(f).split(":");
    const output = join(outDir, `${name ?? frame}.png`);
    await renderStill({ serveUrl, composition, frame: Number(frame), output, imageFormat: "png", scale, inputProps: { music: false }, overwrite: true });
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
