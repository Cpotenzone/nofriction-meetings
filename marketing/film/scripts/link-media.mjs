// Links the gitignored media into Remotion's public dir and writes
// src/footage.json (size, fps, duration of every clip) with ffprobe.
//
//   node scripts/link-media.mjs
//
// public/footage -> ../../out/footage   (capture output)
// public/audio   -> ../../out/audio     (scripts/make-score.mjs output)

import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, readdirSync, symlinkSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(FILM, "..", "out");

for (const name of ["footage", "audio"]) {
  const target = join(OUT, name);
  mkdirSync(target, { recursive: true });
  const link = join(FILM, "public", name);
  if (existsSync(link) || (() => { try { return lstatSync(link).isSymbolicLink(); } catch { return false; } })()) unlinkSync(link);
  symlinkSync(relative(join(FILM, "public"), target), link);
}

const manifest = {};
const walk = (dir) => {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) { if (e.name !== "raw") walk(p); }
    else if (/\.(mp4|mov|png|jpg)$/i.test(e.name) && !/\.take\d*\./.test(e.name)) {
      const rel = relative(join(OUT, "footage"), p);
      let json;
      try {
        json = JSON.parse(
          execFileSync("ffprobe", ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate:format=duration", "-of", "json", p], { stdio: ["ignore", "pipe", "ignore"] }).toString(),
        );
      } catch {
        console.warn(`skipped (unreadable, maybe still being written): ${rel}`);
        continue;
      }
      const s = json.streams?.[0] ?? {};
      const [n, d] = String(s.r_frame_rate ?? "0/1").split("/").map(Number);
      manifest[rel] = { w: s.width, h: s.height, fps: d ? n / d : 0, dur: Number(json.format?.duration ?? 0) };
    }
  }
};
walk(join(OUT, "footage"));
writeFileSync(join(FILM, "src", "footage.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`footage.json: ${Object.keys(manifest).length} files`);
