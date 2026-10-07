// Renders every deliverable into marketing/out/ and checks it with ffprobe.
//
//   node scripts/render-all.mjs            # everything
//   node scripts/render-all.mjs film       # just the hero film (+ poster, stills)
//   node scripts/render-all.mjs previews   # just the two App Store previews
//
// 1. Synthesizes the score if missing (scripts/make-score.mjs).
// 2. Links footage/audio into public/ and writes src/footage.json.
// 3. Renders a high-quality master per composition with Remotion.
// 4. Encodes the deliverables with ffmpeg:
//    - film MP4: H.264 High, yuv420p, +faststart, AAC (size cap 25 MB)
//    - film WebM: VP9 2-pass + Opus (size cap 12 MB), with Remotion's bundled
//      ffmpeg (Homebrew's build often lacks libvpx)
//    - app previews: Apple's spec (H.264 High@4.0, 10-12 Mbps, 30 fps,
//      stereo AAC 256 kbps 48 kHz)
// 5. Renders the poster and website stills (PNG, palette-compressed < 500 KB).

import { bundle } from "@remotion/bundler";
import { renderMedia, renderStill, selectComposition } from "@remotion/renderer";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, statSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const FILM = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(FILM, "..", "out");
const MASTERS = join(OUT, "masters");
const STILLS = join(FILM, "stills");
const VPX_FFMPEG = join(FILM, "node_modules", "@remotion", "compositor-darwin-arm64", "ffmpeg");
const which = process.argv[2] ?? "all";

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
if (!existsSync(join(OUT, "audio", "score-film.wav")) || !existsSync(join(OUT, "audio", "score-preview.wav"))) {
  run("node", [join(FILM, "scripts", "make-score.mjs")]);
}
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
  const composition = await selectComposition({ serveUrl, id, inputProps });
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

async function stills(id, frames, { maxKB = 480, width } = {}) {
  const composition = await selectComposition({ serveUrl, id, inputProps: { music: false } });
  for (const [frame, name] of frames) {
    const tmp = join(MASTERS, `${name}.raw.png`);
    await renderStill({ serveUrl, composition, frame, output: tmp, imageFormat: "png", inputProps: { music: false }, overwrite: true });
    const out = join(STILLS, `${name}.png`);
    // libimagequant palette PNG; step the quality down until it fits
    for (const q of [92, 85, 78, 70, 60]) {
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
  const film = await selectComposition({ serveUrl, id: "Film", inputProps: { music: false } });
  await renderStill({ serveUrl, composition: film, frame: 13 * 72 + 120, output: poster, imageFormat: "png", inputProps: { music: false }, overwrite: true });

  await stills("Film", [
    [200, "film-01-moments"],
    [4 * 72 + 170, "film-02-record"],
    [13 * 72 + 120, "film-03-rewind"],
    [20 * 72 + 180, "film-04-offline"],
    [23 * 72 + 140, "film-05-end-card"],
  ]);

  console.log("\nFilm deliverables:");
  for (const f of [mp4, webm]) report(f);
  console.log(`  ${poster.replace(OUT + "/", "out/")} | ${(statSync(poster).size / 1000).toFixed(0)} KB`);
  if (statSync(mp4).size > 25e6) console.warn("  !! MP4 is over 25 MB");
  if (statSync(webm).size > 12e6) console.warn("  !! WebM is over 12 MB");
}

if (which === "all" || which === "previews") {
  // Apple app preview spec (developer.apple.com/help/app-store-connect/reference/app-preview-specifications):
  // 15-30 s, <= 30 fps, H.264 High Profile up to Level 4.0 at 10-12 Mbps,
  // .mp4/.mov/.m4v, <= 500 MB, stereo AAC 256 kbps at 44.1/48 kHz.
  const jobs = [
    { id: "PreviewIPhone", file: "nofriction-app-preview-iphone-6.9in-886x1920.mp4", w: 886, h: 1920 },
    { id: "PreviewMac", file: "nofriction-app-preview-mac-1920x1080.mp4", w: 1920, h: 1080 },
  ];
  console.log("\nApp previews:");
  for (const job of jobs) {
    const m = await master(job.id);
    const out = join(OUT, job.file);
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
