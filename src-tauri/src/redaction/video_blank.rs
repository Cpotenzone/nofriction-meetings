//! DMG screen video: blank time ranges with black frames (ffmpeg).
//!
//! Runs only from the background job worker (`video_jobs`), never under the
//! redaction `LOCK`. Cost scales with the removed span, not the meeting:
//!
//! 1. Ranges already blanked are skipped: each chunk's blanked ranges are
//!    recorded in `blanked.json` (times only), and a range with no record is
//!    decoded first and skipped if every frame is already black (e.g. an
//!    older build blanked it with a full re-encode but crashed before the
//!    database step). Fresh content stops that check at its first frame.
//! 2. Partial re-encode: for an H.264 chunk, only the GOPs around each range
//!    (from the keyframe at or before it to the keyframe at or after it) are
//!    re-encoded with the black box; everything else is stream-copied, and
//!    the pieces are joined with the concat demuxer. The re-encoded piece
//!    must have the source's exact codec configuration (SPS/PPS), so the
//!    output is one standard H.264 track. If it doesn't match, or the chunk
//!    isn't H.264, the whole chunk is re-encoded instead (slow, but correct).
//! 3. The result is verified (frame count, duration, black inside the range)
//!    before it atomically replaces the chunk.

use super::video_jobs::{BlankError, BlankReport, VideoOps};
use super::*;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Chunk start times may be approximate (older recordings: file creation
/// time), so ranges are padded on both sides.
pub const PAD_SECS: f64 = 1.5;
const SIDECAR: &str = "chunk_times.json";
const BLANKED: &str = "blanked.json";
/// A frame counts as black when every pixel of its 64×36 downscale is at
/// most this (black after blanking decodes to 0; real dark-mode screens with
/// any text are well above it).
const BLACK_MAX_LUMA: u8 = 6;
const PROBE_W: usize = 64;
const PROBE_H: usize = 36;

/// Serializes read-modify-write of the sidecar files (the recorder's
/// rotation thread and the blanking worker both write them).
static SIDECAR_LOCK: Lazy<parking_lot::Mutex<()>> = Lazy::new(|| parking_lot::Mutex::new(()));

pub fn video_dir(env: &RedactionEnv, meeting_id: &str) -> PathBuf {
    env.app_data_dir.join(meeting_id).join("video")
}

/// The meeting's screen video chunks, in order (temp files excluded).
pub fn list_chunks(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map_or(false, |e| e == "mov")
                        && !p.file_name().and_then(|n| n.to_str()).unwrap_or("").starts_with('.')
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn load_map(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn write_map(path: &Path, map: serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::Value::Object(map).to_string())
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Couldn't save {}: {}", path.display(), e))
}

fn chunk_name(chunk: &Path) -> String {
    chunk.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

/// Record a chunk's wall-clock start (the recorder writes it when the first
/// frame arrives; blanking re-saves it because re-encoding resets file times).
pub fn set_chunk_start(dir: &Path, chunk: &Path, start: DateTime<Utc>) -> Result<(), String> {
    let _g = SIDECAR_LOCK.lock();
    let path = dir.join(SIDECAR);
    let mut map = load_map(&path);
    map.insert(chunk_name(chunk), serde_json::Value::String(start.to_rfc3339()));
    write_map(&path, map).map_err(|e| format!("Couldn't save video chunk times: {}", e))
}

/// A chunk's wall-clock start: the recorded time, else the file's creation
/// time (recordings made before start times were recorded).
pub fn chunk_start(dir: &Path, chunk: &Path) -> Option<DateTime<Utc>> {
    if let Some(s) = load_map(&dir.join(SIDECAR)).get(&chunk_name(chunk)).and_then(|v| v.as_str()) {
        return parse_ts(s);
    }
    let meta = std::fs::metadata(chunk).ok()?;
    meta.created().ok().map(DateTime::<Utc>::from)
}

/// Ranges (ms from the chunk start) already blanked in a chunk.
pub fn blanked_ranges(dir: &Path, chunk: &Path) -> Vec<(i64, i64)> {
    load_map(&dir.join(BLANKED))
        .get(&chunk_name(chunk))
        .and_then(|v| serde_json::from_value::<Vec<(i64, i64)>>(v.clone()).ok())
        .unwrap_or_default()
}

fn record_blanked(dir: &Path, chunk: &Path, ranges: &[(f64, f64)]) -> Result<(), String> {
    if ranges.is_empty() {
        return Ok(());
    }
    let _g = SIDECAR_LOCK.lock();
    let path = dir.join(BLANKED);
    let mut map = load_map(&path);
    let mut all: Vec<(i64, i64)> = map
        .get(&chunk_name(chunk))
        .and_then(|v| serde_json::from_value::<Vec<(i64, i64)>>(v.clone()).ok())
        .unwrap_or_default();
    all.extend(ranges.iter().map(|(a, b)| ((a * 1000.0).floor() as i64, (b * 1000.0).ceil() as i64)));
    all.sort();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (a, b) in all {
        match merged.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    map.insert(chunk_name(chunk), serde_json::to_value(merged).unwrap_or_default());
    write_map(&path, map)
}

/// Is `[a, b]` (seconds) inside the union of recorded ranges?
pub fn covered(recorded: &[(i64, i64)], a: f64, b: f64) -> bool {
    let (mut a, b) = ((a * 1000.0).ceil() as i64, (b * 1000.0).floor() as i64);
    if a >= b {
        return true;
    }
    let mut v = recorded.to_vec();
    v.sort();
    for (s, e) in v {
        if s <= a && e > a {
            a = e;
            if a >= b {
                return true;
            }
        }
    }
    false
}

fn tool(name: &str) -> Result<PathBuf, BlankError> {
    crate::video_recorder::find_tool(name).ok_or_else(|| {
        BlankError::ToolMissing(format!(
            "{} isn't installed, so removed moments can't be blanked from the screen video yet. \
             Install it (brew install ffmpeg), then click Retry.",
            name
        ))
    })
}

pub fn probe_duration(path: &Path) -> Result<f64, String> {
    let ffprobe = crate::video_recorder::find_tool("ffprobe")
        .ok_or("ffprobe isn't installed, so the screen video can't be checked")?;
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe failed: {}", e))?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("Couldn't read the length of {}", path.display()))
}

/// One chunk's share of a job: padded, clamped, merged ranges in seconds
/// from the chunk start.
#[derive(Debug, Clone)]
pub struct ChunkPlan {
    pub chunk: PathBuf,
    pub start: DateTime<Utc>,
    pub duration: f64,
    pub ranges: Vec<(f64, f64)>,
}

/// Which chunks cover the given wall-clock ranges (each chunk's own start
/// time is its offset), padded by [`PAD_SECS`] and clamped to the chunk.
pub fn plan(
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Result<Vec<ChunkPlan>, String> {
    let dir = video_dir(env, meeting_id);
    let chunks = list_chunks(&dir);
    if chunks.is_empty() || ranges.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for chunk in chunks {
        let start = chunk_start(&dir, &chunk).ok_or_else(|| {
            format!("Couldn't tell when video {} started, so it can't be blanked safely", chunk.display())
        })?;
        let dur = probe_duration(&chunk)?;
        let mut rel: Vec<(f64, f64)> = Vec::new();
        for (a, b) in ranges {
            let ra = (*a - start).num_milliseconds() as f64 / 1000.0 - PAD_SECS;
            let rb = (*b - start).num_milliseconds() as f64 / 1000.0 + PAD_SECS;
            if rb > 0.0 && ra < dur {
                rel.push((ra.max(0.0), rb.min(dur)));
            }
        }
        rel.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (a, b) in rel {
            match merged.last_mut() {
                Some(l) if a <= l.1 => l.1 = l.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        if !merged.is_empty() {
            out.push(ChunkPlan { chunk, start, duration: dur, ranges: merged });
        }
    }
    Ok(out)
}

// ── ffprobe / ffmpeg helpers ─────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct StreamInfo {
    codec: String,
    width: i64,
    height: i64,
    nb_frames: Option<i64>,
    time_base: (i64, i64),
    bit_rate: Option<i64>,
    extradata: String,
    duration: f64,
}

fn probe_stream(path: &Path) -> Result<StreamInfo, String> {
    let ffprobe = crate::video_recorder::find_tool("ffprobe").ok_or("ffprobe isn't installed")?;
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_streams", "-show_data", "-show_format"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe failed: {}", e))?;
    if !out.status.success() {
        return Err(format!("Couldn't read {}", path.display()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut info = StreamInfo::default();
    let mut in_extradata = false;
    let mut section = "";
    for line in text.lines() {
        if line.starts_with('[') {
            section = if line == "[STREAM]" { "stream" } else if line == "[FORMAT]" { "format" } else { "" };
            in_extradata = false;
            continue;
        }
        if in_extradata {
            // "00000000: 0164 001f ffe1 ...  .d......" → hex words only
            if let Some((_, rest)) = line.split_once(": ") {
                let hex: String = rest.split("  ").next().unwrap_or("").split_whitespace().collect();
                info.extradata.push_str(&hex);
                continue;
            }
            in_extradata = false;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match (section, k) {
            ("stream", "codec_name") => info.codec = v.to_string(),
            ("stream", "width") => info.width = v.parse().unwrap_or(0),
            ("stream", "height") => info.height = v.parse().unwrap_or(0),
            ("stream", "nb_frames") => info.nb_frames = v.parse().ok(),
            ("stream", "bit_rate") => info.bit_rate = v.parse().ok(),
            ("stream", "time_base") => {
                if let Some((n, d)) = v.split_once('/') {
                    info.time_base = (n.parse().unwrap_or(0), d.parse().unwrap_or(0));
                }
            }
            ("stream", "extradata") => in_extradata = true,
            ("format", "duration") => info.duration = v.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    if info.codec.is_empty() {
        return Err(format!("{} has no video stream", path.display()));
    }
    Ok(info)
}

/// Keyframe times (seconds) of the first video stream, exact to its time base.
fn keyframes(path: &Path, info: &StreamInfo) -> Result<Vec<f64>, String> {
    let ffprobe = crate::video_recorder::find_tool("ffprobe").ok_or("ffprobe isn't installed")?;
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts,flags", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe failed: {}", e))?;
    let (num, den) = info.time_base;
    if num <= 0 || den <= 0 {
        return Err("Unknown video time base".into());
    }
    let mut k: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split(',');
            let pts = it.next()?.trim().parse::<i64>().ok()?;
            let flags = it.next().unwrap_or("");
            flags.contains('K').then(|| pts as f64 * num as f64 / den as f64)
        })
        .collect();
    k.sort_by(f64::total_cmp);
    k.dedup();
    if k.is_empty() {
        return Err("The video has no keyframes".into());
    }
    Ok(k)
}

/// Seconds formatted for ffmpeg, rounded down / up to the microsecond so a
/// cut lands on the right side of a keyframe.
fn secs_down(t: f64) -> String {
    format!("{:.6}", (t * 1e6).floor() / 1e6)
}
fn secs_up(t: f64) -> String {
    format!("{:.6}", (t * 1e6).ceil() / 1e6)
}

/// Run ffmpeg with `-progress pipe:1`; `total` seconds of output map to 0–1.
fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    total: f64,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<(), BlankError> {
    let mut child: Child = Command::new(ffmpeg)
        .args(["-v", "error", "-nostdin", "-nostats", "-progress", "pipe:1", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BlankError::Failed(format!("ffmpeg failed to start: {}", e)))?;
    let mut stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines() {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BlankError::Cancelled);
            }
            let Ok(line) = line else { break };
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<i64>().ok()) {
                if total > 0.0 && us > 0 {
                    progress((us as f64 / 1e6 / total).min(1.0) as f32);
                }
            }
        }
    }
    let status = child.wait().map_err(|e| BlankError::Failed(format!("ffmpeg failed: {}", e)))?;
    let stderr = err_thread.join().unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        return Err(BlankError::Cancelled);
    }
    if status.success() {
        Ok(())
    } else {
        Err(BlankError::Failed(stderr.trim().chars().take(400).collect()))
    }
}

/// Is the first frame at or after `t` black? `None`: couldn't decode.
pub fn frame_is_black(path: &Path, t: f64) -> Option<bool> {
    let ffmpeg = crate::video_recorder::find_tool("ffmpeg")?;
    let out = Command::new(ffmpeg)
        .args(["-v", "error", "-nostdin", "-ss", &secs_down(t.max(0.0)), "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-fps_mode", "passthrough", "-vf"])
        .arg(format!("scale={}:{}:flags=area,format=gray", PROBE_W, PROBE_H))
        .args(["-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.stdout.len() < PROBE_W * PROBE_H {
        return None;
    }
    Some(out.stdout.iter().all(|&p| p <= BLACK_MAX_LUMA))
}

/// Is every frame of `[a, b]` already black? Stops at the first frame that
/// isn't, so fresh content costs one short decode. `None`: couldn't decode.
pub fn is_black_range(path: &Path, a: f64, b: f64, cancel: &AtomicBool) -> Option<bool> {
    // Stay clear of the edges (the boundary frame may straddle the range)
    if b - a <= 0.5 {
        return frame_is_black(path, (a + b) / 2.0);
    }
    let (a, b) = (a + 0.15, b - 0.15);
    let ffmpeg = crate::video_recorder::find_tool("ffmpeg")?;
    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-nostdin", "-ss", &secs_down(a), "-i"])
        .arg(path)
        .args(["-t", &format!("{:.6}", b - a), "-map", "0:v:0", "-fps_mode", "passthrough", "-vf"])
        .arg(format!("scale={}:{}:flags=area,format=gray", PROBE_W, PROBE_H))
        .args(["-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let mut frame = vec![0u8; PROBE_W * PROBE_H];
    let mut frames = 0usize;
    let verdict = loop {
        if cancel.load(Ordering::SeqCst) {
            break None;
        }
        match out.read_exact(&mut frame) {
            Ok(()) => {
                frames += 1;
                if frame.iter().any(|&p| p > BLACK_MAX_LUMA) {
                    break Some(false);
                }
            }
            Err(_) => break if frames > 0 { Some(true) } else { None },
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    verdict
}

/// Paint whole frames black inside `ranges` (seconds of the output). A
/// lookup table (black luma, neutral chroma, in the stream's own range) is
/// ~8x faster than drawbox on 1080p, which dominated long blanks.
fn blackout_filter(ranges: &[(f64, f64)]) -> String {
    let enable = ranges
        .iter()
        .map(|(a, b)| format!("between(t,{:.3},{:.3})", a, b))
        .collect::<Vec<_>>()
        .join("+");
    format!(
        "lutyuv=y=minval:u=(minval+maxval)/2:v=(minval+maxval)/2:enable='{}'",
        enable
    )
}

fn tmp_path(chunk: &Path, suffix: &str) -> PathBuf {
    let stem = chunk.file_stem().and_then(|s| s.to_str()).unwrap_or("chunk");
    chunk.with_file_name(format!(".{}.blank-{}", stem, suffix))
}

/// Remove leftovers of an interrupted run for this chunk.
fn clean_tmp(chunk: &Path) {
    let stem = chunk.file_stem().and_then(|s| s.to_str()).unwrap_or("chunk").to_string();
    let prefix = format!(".{}.blank", stem);
    if let Some(dir) = chunk.parent() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if e.file_name().to_str().map_or(false, |n| n.starts_with(&prefix)) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

fn concat_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Why the partial path didn't run (the caller re-encodes the whole chunk).
enum Partial {
    Unsupported(String),
    Err(BlankError),
}

/// Re-encode only the GOPs around `ranges`, stream-copy the rest.
fn blank_partial(
    ffmpeg: &Path,
    chunk: &Path,
    info: &StreamInfo,
    ranges: &[(f64, f64)],
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<PathBuf, Partial> {
    if info.codec != "h264" {
        return Err(Partial::Unsupported(format!("codec {}", info.codec)));
    }
    if info.extradata.is_empty() {
        return Err(Partial::Unsupported("no codec configuration".into()));
    }
    let keys = keyframes(chunk, info).map_err(Partial::Unsupported)?;
    let dur = info.duration;
    // GOP-aligned segments [ks, ke) around each range; ke None = to the end
    let mut segs: Vec<(f64, Option<f64>, Vec<(f64, f64)>)> = Vec::new();
    for &(a, b) in ranges {
        // From the keyframe at or before the start to the first keyframe
        // strictly after the end (a frame exactly at `b` is inside the range)
        let ks = keys.iter().copied().filter(|k| *k <= a + 1e-6).last().unwrap_or(keys[0]);
        let ke = keys.iter().copied().find(|k| *k > b + 1e-6 && *k > ks);
        match segs.last_mut() {
            Some(last) if last.1.map_or(true, |e| ks <= e + 1e-6) => {
                last.1 = match (last.1, ke) {
                    (Some(x), Some(y)) => Some(x.max(y)),
                    _ => None,
                };
                last.2.push((a, b));
            }
            _ => segs.push((ks, ke, vec![(a, b)])),
        }
    }
    let total: f64 = segs.iter().map(|(s, e, _)| e.unwrap_or(dur) - s).sum::<f64>().max(0.001);
    let bitrate = info.bit_rate.filter(|b| *b > 100_000).unwrap_or(3_000_000);
    let mut done = 0.0;
    let mut mids = Vec::new();
    for (i, (ks, ke, rs)) in segs.iter().enumerate() {
        let len = ke.unwrap_or(dur) - ks;
        let rel: Vec<(f64, f64)> = rs.iter().map(|(a, b)| ((a - ks).max(0.0), b - ks)).collect();
        let mid = tmp_path(chunk, &format!("mid{}.mov", i));
        let mut matched = false;
        let mut last_err = String::new();
        // Same encoder/settings as the recorder, so the codec configuration
        // matches; a second try pins the bit rate the recorder uses.
        for br in [bitrate, 3_000_000] {
            let mut args: Vec<String> = vec!["-ss".into(), secs_down(*ks), "-i".into(), chunk.to_string_lossy().into()];
            if let Some(_e) = ke {
                // Stop just before the next keyframe (it is copied, not re-encoded)
                args.extend(["-t".into(), format!("{:.6}", (len - 0.001).max(0.001))]);
            }
            args.extend(
                [
                    "-map", "0:v:0", "-an", "-sn", "-dn", "-fps_mode", "passthrough", "-vf",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            args.push(blackout_filter(&rel));
            args.extend(["-c:v", "h264_videotoolbox", "-b:v"].iter().map(|s| s.to_string()));
            args.push(br.to_string());
            args.extend(["-pix_fmt".into(), "yuv420p".into(), mid.to_string_lossy().into()]);
            let base = done;
            let r = run_ffmpeg(ffmpeg, &args, len, &|f| progress(((base + f as f64 * len) / total * 0.9) as f32), cancel);
            match r {
                Err(BlankError::Cancelled) => return Err(Partial::Err(BlankError::Cancelled)),
                Err(BlankError::Failed(e)) | Err(BlankError::ToolMissing(e)) => {
                    last_err = e;
                    continue;
                }
                Ok(()) => {}
            }
            match probe_stream(&mid) {
                Ok(m) if m.extradata == info.extradata && m.width == info.width && m.height == info.height => {
                    matched = true;
                    break;
                }
                Ok(_) => last_err = "re-encoded piece has a different codec configuration".into(),
                Err(e) => last_err = e,
            }
            if br == 3_000_000 {
                break;
            }
        }
        if !matched {
            return Err(Partial::Unsupported(last_err));
        }
        done += len;
        mids.push((*ks, *ke, mid, len));
    }

    // Concat: copy [0, ks1) + mid1 + copy [ke1, ks2) + mid2 + … + copy [keN, end]
    let mut list = String::new();
    let mut cursor: Option<f64> = Some(keys[0]);
    for (ks, ke, mid, len) in &mids {
        if let Some(c) = cursor {
            if *ks > c + 1e-6 {
                list.push_str(&format!("file {}\n", concat_quote(chunk)));
                if c > keys[0] + 1e-6 {
                    list.push_str(&format!("inpoint {}\n", secs_up(c)));
                }
                list.push_str(&format!("outpoint {}\n", secs_down(*ks)));
            }
        }
        list.push_str(&format!("file {}\nduration {:.6}\n", concat_quote(mid), len));
        cursor = *ke;
    }
    if let Some(c) = cursor {
        list.push_str(&format!("file {}\ninpoint {}\n", concat_quote(chunk), secs_up(c)));
    }
    let list_path = tmp_path(chunk, "list.txt");
    std::fs::write(&list_path, list).map_err(|e| Partial::Err(BlankError::Failed(e.to_string())))?;
    let out = tmp_path(chunk, "out.mov");
    // auto_convert 0: pass packets through as they are. All pieces share one
    // codec configuration (checked above), so no Annex B round trip (which
    // would add in-band parameter sets to every keyframe) is needed.
    let args: Vec<String> = vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-auto_convert".into(),
        "0".into(),
        "-i".into(),
        list_path.to_string_lossy().into(),
        "-map".into(),
        "0:v:0".into(),
        "-c".into(),
        "copy".into(),
        out.to_string_lossy().into(),
    ];
    run_ffmpeg(ffmpeg, &args, 0.0, &|_| {}, cancel).map_err(|e| match e {
        BlankError::Cancelled => Partial::Err(BlankError::Cancelled),
        BlankError::Failed(m) | BlankError::ToolMissing(m) => Partial::Unsupported(format!("join failed: {}", m)),
    })?;
    progress(0.95);
    // Same frames, same length, one codec configuration
    let o = probe_stream(&out).map_err(Partial::Unsupported)?;
    let frames_ok = match (o.nb_frames, info.nb_frames) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    };
    if !frames_ok || (o.duration - dur).abs() > 0.25 || o.extradata != info.extradata {
        return Err(Partial::Unsupported(format!(
            "joined file didn't verify (frames {:?} vs {:?}, length {:.3} vs {:.3})",
            o.nb_frames, info.nb_frames, o.duration, dur
        )));
    }
    Ok(out)
}

/// The old path: re-encode the whole chunk with the ranges painted black.
fn blank_full(
    ffmpeg: &Path,
    chunk: &Path,
    dur: f64,
    ranges: &[(f64, f64)],
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<PathBuf, BlankError> {
    let out = tmp_path(chunk, "full.mov");
    let attempts: [&[&str]; 3] = [
        &["-c:v", "h264_videotoolbox", "-b:v", "3M"],
        &["-c:v", "libx264", "-crf", "23"],
        &["-c:v", "mpeg4", "-q:v", "4"],
    ];
    let mut last_err = String::new();
    for codec in attempts {
        let mut args: Vec<String> = vec!["-i".into(), chunk.to_string_lossy().into(), "-map".into(), "0:v:0".into()];
        args.extend(["-vf".into(), blackout_filter(ranges)]);
        args.extend(codec.iter().map(|s| s.to_string()));
        args.extend(["-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an"].iter().map(|s| s.to_string()));
        args.push(out.to_string_lossy().into());
        match run_ffmpeg(ffmpeg, &args, dur, &|f| progress(f * 0.95), cancel) {
            Ok(()) => {
                if probe_duration(&out).map(|d| (d - dur).abs() <= 1.0).unwrap_or(false) {
                    return Ok(out);
                }
                last_err = "re-encoded file didn't verify".into();
            }
            Err(BlankError::Cancelled) => return Err(BlankError::Cancelled),
            Err(BlankError::Failed(e)) | Err(BlankError::ToolMissing(e)) => last_err = e,
        }
    }
    let _ = std::fs::remove_file(&out);
    Err(BlankError::Failed(format!("Couldn't blank the screen video {} ({})", chunk.display(), last_err)))
}

/// How a chunk was blanked (for tests and logs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    Partial,
    Full,
}

/// Blank `ranges` (seconds from the chunk start) in one chunk and replace
/// it atomically. Ranges already recorded or already black are skipped.
/// Returns `None` if nothing needed re-encoding.
pub fn blank_chunk(
    dir: &Path,
    plan: &ChunkPlan,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<(Option<Method>, usize), BlankError> {
    let ffmpeg = tool("ffmpeg")?;
    tool("ffprobe")?;
    let chunk = &plan.chunk;
    clean_tmp(chunk);
    let recorded = blanked_ranges(dir, chunk);
    let mut todo: Vec<(f64, f64)> = Vec::new();
    let mut already: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in &plan.ranges {
        if covered(&recorded, a, b) {
            continue;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(BlankError::Cancelled);
        }
        match is_black_range(chunk, a, b, cancel) {
            Some(true) => already.push((a, b)),
            _ => todo.push((a, b)),
        }
    }
    let skipped = plan.ranges.len() - todo.len();
    // Black but unrecorded (an older build blanked it): record, don't redo
    record_blanked(dir, chunk, &already).map_err(BlankError::Failed)?;
    if todo.is_empty() {
        return Ok((None, skipped));
    }
    // Re-encoding resets file times: keep the start time in the sidecar
    set_chunk_start(dir, chunk, plan.start).map_err(BlankError::Failed)?;
    let info = probe_stream(chunk).map_err(BlankError::Failed)?;
    let (out, method) = match blank_partial(&ffmpeg, chunk, &info, &todo, progress, cancel) {
        Ok(out) => (out, Method::Partial),
        Err(Partial::Err(e)) => {
            clean_tmp(chunk);
            return Err(e);
        }
        Err(Partial::Unsupported(why)) => {
            log::info!("Partial re-encode not possible for {} ({}); re-encoding the whole chunk", chunk.display(), why);
            clean_tmp(chunk);
            let dur = if info.duration > 0.0 { info.duration } else { plan.duration };
            match blank_full(&ffmpeg, chunk, dur, &todo, progress, cancel) {
                Ok(out) => (out, Method::Full),
                Err(e) => {
                    clean_tmp(chunk);
                    return Err(e);
                }
            }
        }
    };
    // Every range must now be black (sampled: start, middle, end)
    for &(a, b) in &todo {
        let inset = ((b - a) / 4.0).min(0.2);
        let samples = [a + inset, (a + b) / 2.0, b - inset];
        if samples.iter().any(|t| frame_is_black(&out, *t) != Some(true)) {
            clean_tmp(chunk);
            return Err(BlankError::Failed(format!(
                "The blanked screen video {} didn't verify; it was left as it was",
                chunk.display()
            )));
        }
    }
    std::fs::rename(&out, chunk).map_err(|e| {
        clean_tmp(chunk);
        BlankError::Failed(format!("Couldn't replace the screen video {}: {}", chunk.display(), e))
    })?;
    clean_tmp(chunk);
    record_blanked(dir, chunk, &todo).map_err(BlankError::Failed)?;
    progress(1.0);
    Ok((Some(method), skipped))
}

/// Blank wall-clock `ranges` in every covering chunk of a meeting.
pub fn blank(
    env: &RedactionEnv,
    meeting_id: &str,
    ranges: &[(DateTime<Utc>, DateTime<Utc>)],
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<BlankReport, BlankError> {
    let dir = video_dir(env, meeting_id);
    if list_chunks(&dir).is_empty() {
        return Ok(BlankReport::default());
    }
    tool("ffprobe")?;
    tool("ffmpeg")?;
    let plans = plan(env, meeting_id, ranges).map_err(BlankError::Failed)?;
    let total: f64 = plans.iter().flat_map(|p| p.ranges.iter().map(|(a, b)| b - a)).sum::<f64>().max(0.001);
    let mut report = BlankReport::default();
    let mut done = 0.0;
    for p in &plans {
        let share: f64 = p.ranges.iter().map(|(a, b)| b - a).sum();
        let base = done;
        let (method, skipped) =
            blank_chunk(&dir, p, &|f| progress(((base + f as f64 * share) / total) as f32), cancel)?;
        if method.is_some() {
            report.chunks_rewritten += 1;
        }
        report.ranges_already_blank += skipped;
        done += share;
    }
    progress(1.0);
    Ok(report)
}

/// The real [`VideoOps`] (ffmpeg on the DMG build).
pub struct FfmpegOps;

impl VideoOps for FfmpegOps {
    fn blank(
        &self,
        env: &RedactionEnv,
        meeting_id: &str,
        ranges: &[(DateTime<Utc>, DateTime<Utc>)],
        progress: &dyn Fn(f32),
        cancel: &AtomicBool,
    ) -> Result<BlankReport, BlankError> {
        blank(env, meeting_id, ranges, progress, cancel)
    }
}
