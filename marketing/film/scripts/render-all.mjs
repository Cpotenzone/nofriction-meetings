// Renders every deliverable into marketing/out/ and checks it with ffprobe.
//
//   node scripts/render-all.mjs            # everything
//   node scripts/render-all.mjs film       # just the hero film (+ poster, stills)
//   node scripts/render-all.mjs previews   # just the App Store previews
//
// 1. Synthesizes the score (scripts/make-score.mjs).
// 2. Links footage/audio into public/ and writes src/footage.json.
// 3. Renders a high-quality master per composition with Remotion.
// 4. Encodes the deliverables with ffmpeg:
//    - film MP4: H.264 High, yuv420p, +faststart, AAC (size cap 25 MB),
//      also copied to out/hero.mp4 (what infra/site/deploy.sh uploads)
//    - film WebM: VP9 2-pass + Opus (size cap 12 MB), with Remotion's bundled
//      ffmpeg (Homebrew's build often lacks libvpx), also out/hero.webm
//    - app previews (out/previews/): Apple's spec (H.264 High@4.0,
//      10-12 Mbps, 30 fps, stereo AAC 256 kbps 48 kHz). The Mac preview
//      only; IOS_PREVIEW=1 adds the iPhone one, which still uses the
//      pre-redesign iOS footage (README).
// 5. Renders the poster and the film stills (PNG, palette-compressed < 300 KB).
//
// Nothing is downloaded: the fonts are the Mac's own (src/theme.ts), and
// Chromium is Playwright's cached headless shell (REMOTION_BROWSER picks
// another), so Remotion never fetches its own browser.

import { bundle } from "@remotion/bundler";
import { renderMedia, renderStill, selectComposition } from "@remotion/renderer";
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync, unlinkSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(FILM, "..", "out");
const PREVIEWS = join(OUT, "previews");
const MASTERS = join(OUT, "masters");

/** Playwright's cached chrome-headless-shell (newest first), or REMOTION_BROWSER. */
function localBrowser() {
  if (process.env.REMOTION_BROWSER) return process.env.REMOTION_BROWSER;
  const cache = join(homedir(), "Library", "Caches", "ms-playwright");
  if (!existsSync(cache)) return null;
  const dirs = readdirSync(cache).filter((d) => d.startsWith("chromium_headless_shell-")).sort().reverse();
  for (const d of dirs) {
    for (const arch of ["chrome-headless-shell-mac-arm64", "chrome-headless-shell-mac-x64"]) {
      const exe = join(cache, d, arch, "chrome-headless-shell");
      if (existsSync(exe)) return exe;
    }
  }
  return null;
}
const browserExecutable = localBrowser();
if (!browserExecutable) {
  console.error("No local Chromium: run `npx playwright install chromium-headless-shell` in mac-harness/ or set REMOTION_BROWSER.");
  process.exit(1);
}
console.log(`Browser: ${browserExecutable}`);
const STILLS = join(FILM, "stills");
const VPX_FFMPEG = join(FILM, "node_modules", "@remotion", "compositor-darwin-arm64", "ffmpeg");
const which = process.argv[2] ?? "all";
// The Rewind scene (bars 4-5), after the Important pin lands
const POSTER_FRAME = 4 * 72 + 112;

const run = (cmd, args, { cwd, env } = {}) => {
  console.log(`$ ${cmd.split("/").pop()} ${args.join(" ")}`);
  execFileSync(cmd, args, { cwd, stdio: ["ignore", "inherit", "inherit"], env: { ...process.env, ...env } });
};
// Remotion's ffmpeg loads its dylibs from its own folder (as Remotion runs it)
const VPX_DIR = dirname(VPX_FFMPEG);
const runVpx = (args) => run(VPX_FFMPEG, args, { cwd: VPX_DIR, env: { DYLD_LIBRARY_PATH: VPX_DIR } });
const mb = (f) => (statSync(f).size / 1e6).toFixed(2);

mkdirSync(MASTERS, { recursive: true });
mkdirSync(STILLS, { recursive: true });
mkdirSync(PREVIEWS, { recursive: true });
run("node", [join(FILM, "scripts", "make-score.mjs")]);
run("node", [join(FILM, "scripts", "link-media.mjs")]);

console.log("Bundling…");
const serveUrl = await bundle({ entryPoint: join(FILM, "src", "index.ts"), publicDir: join(FILM, "public") });

async function master(id) {
  const inputProps = { music: true };
  const output = join(MASTERS, `${id}.mp4`);
  // REUSE_MASTERS=1: re-encode the deliverables from the last masters
  if (process.env.REUSE_MASTERS === "1" && existsSync(output)) {
    console.log(`  ${id}: reusing ${output}`);
    return output;
  }
  const composition = await selectComposition({ serveUrl, id, inputProps, browserExecutable });
  let last = -1;
  await renderMedia({
    serveUrl,
    composition,
    inputProps,
    codec: "h264",
    crf: 10,
    pixelFormat: "yuv420p",
    x264Preset: "medium",
    audioCodec: "aac",
    audioBitrate: "320k",
    outputLocation: output,
    overwrite: true,
    concurrency: 6,
    browserExecutable,
    onProgress: ({ progress }) => {
      const p = Math.floor(progress * 10);
      if (p !== last) { last = p; console.log(`  ${id}: ${p * 10}%`); }
    },
  });
  return output;
}

const probe = (f) =>
  JSON.parse(execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type,codec_name,profile,level,width,height,r_frame_rate,avg_frame_rate,pix_fmt,sample_rate,channels,bit_rate:format=duration,size,bit_rate", "-of", "json", f]).toString());

function report(f) {
  const j = probe(f);
  const v = j.streams.find((s) => s.codec_type === "video");
  const a = j.streams.find((s) => s.codec_type === "audio");
  const line = [
    f.replace(OUT + "/", "out/"),
    `${v.width}x${v.height}`,
    `${v.codec_name}${v.profile ? ` ${v.profile}` : ""}${v.level > 0 ? ` L${v.level / 10}` : ""}`,
    v.pix_fmt,
    `${v.r_frame_rate} fps`,
    `${Number(j.format.duration).toFixed(2)} s`,
    `${(Number(j.format.bit_rate) / 1e6).toFixed(2)} Mbps`,
    a ? `${a.codec_name} ${a.channels}ch ${a.sample_rate} Hz ${a.bit_rate ? Math.round(a.bit_rate / 1000) + " kbps" : ""}` : "no audio",
    `${mb(f)} MB`,
  ];
  console.log("  " + line.join(" | "));
  return { v, a, j };
}

async function stills(id, frames, { maxKB = 290, width } = {}) {
  const composition = await selectComposition({ serveUrl, id, inputProps: { music: false }, browserExecutable });
  for (const [frame, name] of frames) {
    const tmp = join(MASTERS, `${name}.raw.png`);
    await renderStill({ serveUrl, composition, frame, output: tmp, imageFormat: "png", inputProps: { music: false }, overwrite: true, browserExecutable });
    const out = join(STILLS, `${name}.png`);
    // libimagequant palette PNG; step the quality down until it fits
    for (const q of [92, 85, 78, 70, 60, 50, 40]) {
      let img = sharp(tmp);
      if (width) img = img.resize({ width });
      await img.png({ palette: true, quality: q, effort: 10, compressionLevel: 9, dither: 0.6 }).toFile(out);
      if (statSync(out).size <= maxKB * 1000) break;
    }
    unlinkSync(tmp);
    console.log(`  ${out.replace(FILM + "/", "")}: ${(statSync(out).size / 1000).toFixed(0)} KB`);
  }
}

if (which === "all" || which === "film") {
  const m = await master("Film");
  const mp4 = join(OUT, "nofriction-launch-film-1080p.mp4");
  const webm = join(OUT, "nofriction-launch-film-1080p.webm");
  const poster = join(OUT, "nofriction-launch-film-poster.png");

  // MP4: quality-capped CRF so it stays under 25 MB
  run("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", "-i", m,
    "-c:v", "libx264", "-preset", "slow", "-crf", "19", "-maxrate", "2800k", "-bufsize", "5600k",
    "-profile:v", "high", "-level", "4.1", "-pix_fmt", "yuv420p", "-r", "30", "-g", "60",
    "-c:a", "aac", "-b:a", "160k", "-ar", "48000", "-ac", "2", "-movflags", "+faststart", mp4]);

  // WebM: VP9 two-pass to a target bitrate so it stays under 12 MB
  const passlog = join(MASTERS, "vp9pass");
  const vp9 = ["-c:v", "libvpx-vp9", "-b:v", "1250k", "-minrate", "500k", "-maxrate", "2000k", "-row-mt", "1", "-tile-columns", "2", "-deadline", "good", "-pix_fmt", "yuv420p", "-r", "30", "-g", "120", "-passlogfile", passlog];
  runVpx(["-hide_banner", "-loglevel", "error", "-y", "-i", m, ...vp9, "-pass", "1", "-cpu-used", "4", "-an", "-f", "webm", "/dev/null"]);
  runVpx(["-hide_banner", "-loglevel", "error", "-y", "-i", m, ...vp9, "-pass", "2", "-cpu-used", "1", "-c:a", "libopus", "-b:a", "96k", "-ar", "48000", webm]);

  // Poster: the signature Rewind shot, with its headline
  const film = await selectComposition({ serveUrl, id: "Film", inputProps: { music: false }, browserExecutable });
  await renderStill({ serveUrl, composition: film, frame: POSTER_FRAME, output: poster, imageFormat: "png", inputProps: { music: false }, overwrite: true, browserExecutable });

  // What infra/site/deploy.sh copies into site/video/
  copyFileSync(mp4, join(OUT, "hero.mp4"));
  copyFileSync(webm, join(OUT, "hero.webm"));
  copyFileSync(poster, join(OUT, "poster.png"));

  await stills("Film", [
    [100, "film-01-record"],
    [2 * 72 + 64, "film-02-mark"],
    [POSTER_FRAME, "film-03-rewind"],
    [6 * 72 + 110, "film-04-review"],
    [8 * 72 + 120, "film-05-chat"],
    [10 * 72 + 50, "film-06-sync"],
    [11 * 72 + 56, "film-07-privacy"],
    [12 * 72 + 100, "film-08-end-card"],
  ]);

  console.log("\nFilm deliverables:");
  for (const f of [mp4, webm, join(OUT, "hero.mp4"), join(OUT, "hero.webm")]) report(f);
  console.log(`  ${poster.replace(OUT + "/", "out/")} | ${(statSync(poster).size / 1000).toFixed(0)} KB`);
  if (statSync(mp4).size > 25e6) console.warn("  !! MP4 is over 25 MB");
  if (statSync(webm).size > 12e6) console.warn("  !! WebM is over 12 MB");
}

if (which === "all" || which === "previews") {
  // Apple app preview spec (developer.apple.com/help/app-store-connect/reference/app-preview-specifications):
  // 15-30 s, <= 30 fps, H.264 High Profile up to Level 4.0 at 10-12 Mbps,
  // .mp4/.mov/.m4v, <= 500 MB, stereo AAC 256 kbps at 44.1/48 kHz.
  const jobs = [
    // The iPhone preview's footage is the pre-redesign iOS app: opt in only
    ...(process.env.IOS_PREVIEW === "1" ? [{ id: "PreviewIPhone", file: "nofriction-app-preview-iphone-6.9in-886x1920.mp4", w: 886, h: 1920 }] : []),
    { id: "PreviewMac", file: "nofriction-app-preview-mac-1920x1080.mp4", w: 1920, h: 1080 },
  ];
  console.log("\nApp previews:");
  for (const job of jobs) {
    const m = await master(job.id);
    const out = join(PREVIEWS, job.file);
    run("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", "-i", m,
      "-c:v", "libx264", "-preset", "slow", "-profile:v", "high", "-level", "4.0",
      "-b:v", "11M", "-minrate", "10M", "-maxrate", "12M", "-bufsize", "12M", "-x264-params", "nal-hrd=vbr",
      "-pix_fmt", "yuv420p", "-r", "30", "-g", "30", "-vf", `scale=${job.w}:${job.h}:flags=lanczos,setsar=1`,
      "-c:a", "aac", "-b:a", "256k", "-ar", "48000", "-ac", "2", "-movflags", "+faststart", out]);
    const { v, a, j } = report(out);
    const d = Number(j.format.duration);
    const problems = [];
    if (v.width !== job.w || v.height !== job.h) problems.push(`resolution ${v.width}x${v.height}`);
    if (v.codec_name !== "h264" || !/High/.test(v.profile) || v.level > 40) problems.push(`codec ${v.codec_name} ${v.profile} L${v.level}`);
    if (eval(v.r_frame_rate) > 30) problems.push(`fps ${v.r_frame_rate}`);
    if (d < 15 || d > 30) problems.push(`duration ${d}`);
    if (!a || a.codec_name !== "aac" || a.channels !== 2 || !["44100", "48000"].includes(a.sample_rate)) problems.push("audio");
    if (statSync(out).size > 500e6) problems.push("size");
    console.log(problems.length ? `  !! ${job.file}: ${problems.join(", ")}` : `  OK ${job.file} meets Apple's app preview spec`);
  }
}
writeFileSync(join(MASTERS, "LAST_RENDER.txt"), new Date().toISOString() + "\n");
