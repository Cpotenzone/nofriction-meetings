// Video Recorder Module (DMG build only)
// Continuous screen recording with ffmpeg (AVFoundation main display),
// written as chunks of CHUNK_DURATION_SECS so a later screen delete/strike
// re-encodes a few minutes of video, not the whole meeting.

use chrono::{DateTime, Utc};
use parking_lot::{Condvar, Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Duration for each video chunk (5 minutes in seconds)
pub const CHUNK_DURATION_SECS: u64 = 300;
/// How long rotation waits for the next chunk's first frame before it stops
/// the previous one (so the two overlap instead of leaving a gap).
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(5);
/// How long a stopping chunk may take to finish writing before it's killed.
const STOP_WAIT: Duration = Duration::from_secs(15);

/// Video chunk metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoChunk {
    pub chunk_number: u32,
    pub path: PathBuf,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub size_bytes: u64,
    pub duration_secs: f64,
}

/// Pin moment bookmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinMoment {
    pub timestamp: DateTime<Utc>,
    pub offset_secs: f64,
    pub label: Option<String>,
    pub chunk_number: u32,
}

/// Recording session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    pub meeting_id: String,
    pub started_at: DateTime<Utc>,
    pub chunks: Vec<VideoChunk>,
    pub pin_moments: Vec<PinMoment>,
    pub is_active: bool,
}

/// One running ffmpeg process writing one chunk.
struct ChunkProc {
    number: u32,
    path: PathBuf,
    child: Child,
    /// Wall-clock time of the chunk's first frame, once ffmpeg reports it
    first_frame: Arc<(Mutex<Option<DateTime<Utc>>>, Condvar)>,
    reader: Option<JoinHandle<()>>,
}

impl ChunkProc {
    fn wait_first_frame(&self, timeout: Duration) -> Option<DateTime<Utc>> {
        let (lock, cv) = &*self.first_frame;
        let deadline = Instant::now() + timeout;
        let mut v = lock.lock();
        while v.is_none() {
            if cv.wait_until(&mut v, deadline).timed_out() {
                break;
            }
        }
        *v
    }
}

/// State shared with the rotation thread and the progress readers.
struct Shared {
    meeting_id: RwLock<Option<String>>,
    start_time: RwLock<Option<DateTime<Utc>>>,
    is_recording: AtomicBool,
    current_chunk: AtomicU32,
    current: Mutex<Option<ChunkProc>>,
    chunks: RwLock<Vec<VideoChunk>>,
    pin_moments: RwLock<Vec<PinMoment>>,
    stop: (Mutex<bool>, Condvar),
    rotation: Mutex<Option<JoinHandle<()>>>,
}

/// Video recorder using ffmpeg
pub struct VideoRecorder {
    /// Output directory for video files (`<dir>/<meeting>/video/`)
    output_dir: PathBuf,
    /// ffmpeg input arguments (the main display; tests use a test pattern)
    input_args: Vec<String>,
    chunk_secs: u64,
    shared: Arc<Shared>,
}

impl VideoRecorder {
    pub fn new(output_dir: PathBuf) -> Self {
        // AVFoundation numbers cameras before screens, so a fixed index like
        // "1" can open a webcam. Address the main display by name instead.
        let input = ["-f", "avfoundation", "-capture_cursor", "1", "-framerate", "15", "-i", "Capture screen 0:none"];
        Self::with_input(output_dir, input.iter().map(|s| s.to_string()).collect(), CHUNK_DURATION_SECS)
    }

    /// A recorder with a custom ffmpeg input and chunk length (tests).
    pub fn with_input(output_dir: PathBuf, input_args: Vec<String>, chunk_secs: u64) -> Self {
        Self {
            output_dir,
            input_args,
            chunk_secs: chunk_secs.max(1),
            shared: Arc::new(Shared {
                meeting_id: RwLock::new(None),
                start_time: RwLock::new(None),
                is_recording: AtomicBool::new(false),
                current_chunk: AtomicU32::new(0),
                current: Mutex::new(None),
                chunks: RwLock::new(Vec::new()),
                pin_moments: RwLock::new(Vec::new()),
                stop: (Mutex::new(false), Condvar::new()),
                rotation: Mutex::new(None),
            }),
        }
    }

    /// Start recording for a meeting
    pub fn start(&self, meeting_id: &str) -> Result<(), String> {
        if self.shared.is_recording.load(Ordering::SeqCst) {
            return Err("Recording already in progress".to_string());
        }
        let video_dir = self.output_dir.join(meeting_id).join("video");
        std::fs::create_dir_all(&video_dir).map_err(|e| format!("Failed to create video directory: {}", e))?;
        // A meeting can be recorded again: continue after its last chunk
        let first = crate::redaction::video_blank::list_chunks(&video_dir)
            .iter()
            .filter_map(|p| chunk_number(p))
            .max()
            .unwrap_or(0)
            + 1;

        *self.shared.meeting_id.write() = Some(meeting_id.to_string());
        *self.shared.start_time.write() = Some(Utc::now());
        self.shared.chunks.write().clear();
        self.shared.pin_moments.write().clear();
        *self.shared.stop.0.lock() = false;

        let proc = spawn_chunk(&self.shared, &self.input_args, first, &video_dir)?;
        *self.shared.current.lock() = Some(proc);
        self.shared.current_chunk.store(first, Ordering::SeqCst);
        self.shared.is_recording.store(true, Ordering::SeqCst);

        let shared = self.shared.clone();
        let input = self.input_args.clone();
        let secs = self.chunk_secs;
        let handle = std::thread::Builder::new()
            .name("video-chunk-rotation".into())
            .spawn(move || rotation_loop(shared, input, video_dir, secs))
            .map_err(|e| format!("Failed to start chunk rotation: {}", e))?;
        *self.shared.rotation.lock() = Some(handle);

        log::info!("Started video recording for meeting: {} ({}s chunks)", meeting_id, self.chunk_secs);
        Ok(())
    }

    /// Stop recording
    pub fn stop(&self) -> Result<RecordingSession, String> {
        if !self.shared.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress".to_string());
        }
        {
            let (lock, cv) = &self.shared.stop;
            *lock.lock() = true;
            cv.notify_all();
        }
        if let Some(h) = self.shared.rotation.lock().take() {
            let _ = h.join();
        }
        if let Some(p) = self.shared.current.lock().take() {
            stop_chunk(&self.shared, p);
        }
        self.shared.is_recording.store(false, Ordering::SeqCst);

        let meeting_id = self.shared.meeting_id.read().clone().unwrap_or_default();
        let started_at = self.shared.start_time.read().unwrap_or_else(Utc::now);
        let chunks = self.shared.chunks.read().clone();
        let pin_moments = self.shared.pin_moments.read().clone();
        log::info!("Stopped video recording. {} chunks, {} pins", chunks.len(), pin_moments.len());
        Ok(RecordingSession { meeting_id, started_at, chunks, pin_moments, is_active: false })
    }

    /// Pin the current moment
    pub fn pin_moment(&self, label: Option<String>) -> Result<PinMoment, String> {
        if !self.shared.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress".to_string());
        }
        let now = Utc::now();
        let start = self.shared.start_time.read().unwrap_or(now);
        let offset_secs = (now - start).num_milliseconds() as f64 / 1000.0;
        let chunk_number = self.shared.current_chunk.load(Ordering::SeqCst);
        let pin = PinMoment { timestamp: now, offset_secs, label, chunk_number };
        self.shared.pin_moments.write().push(pin.clone());
        log::info!("Pinned moment at {}s in chunk {}", offset_secs, chunk_number);
        Ok(pin)
    }

    /// Get current recording status
    pub fn get_status(&self) -> Option<RecordingSession> {
        if !self.shared.is_recording.load(Ordering::SeqCst) {
            return None;
        }
        Some(RecordingSession {
            meeting_id: self.shared.meeting_id.read().clone().unwrap_or_default(),
            started_at: self.shared.start_time.read().unwrap_or_else(Utc::now),
            chunks: self.shared.chunks.read().clone(),
            pin_moments: self.shared.pin_moments.read().clone(),
            is_active: true,
        })
    }

    /// Get path to video directory for a meeting
    pub fn get_video_dir(&self, meeting_id: &str) -> PathBuf {
        self.output_dir.join(meeting_id).join("video")
    }
}

fn chunk_number(p: &Path) -> Option<u32> {
    p.file_stem()?.to_str()?.strip_prefix("chunk_")?.parse().ok()
}

/// Start ffmpeg for one chunk. Its start time is saved twice: now (an
/// estimate) and again when ffmpeg reports the first frame (precise: the
/// moment minus the time already written), so blanking math uses each
/// chunk's real offset.
fn spawn_chunk(shared: &Arc<Shared>, input_args: &[String], number: u32, video_dir: &Path) -> Result<ChunkProc, String> {
    let path = video_dir.join(format!("chunk_{:03}.mov", number));
    let ffmpeg = find_tool("ffmpeg")
        .ok_or("ffmpeg not found — install with `brew install ffmpeg` to enable screen video")?;
    if let Err(e) = crate::redaction::video_blank::set_chunk_start(video_dir, &path, Utc::now()) {
        log::warn!("{}", e);
    }
    let mut child = Command::new(ffmpeg)
        .args(input_args)
        .args([
            "-c:v",
            "h264_videotoolbox", // Hardware H.264 encoder
            "-b:v",
            "3M",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-nostats",
            "-progress",
            "pipe:1",
            "-y",
        ])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    let first_frame: Arc<(Mutex<Option<DateTime<Utc>>>, Condvar)> = Arc::new((Mutex::new(None), Condvar::new()));
    let reader = child.stdout.take().map(|out| {
        let ff = first_frame.clone();
        let shared = shared.clone();
        let dir = video_dir.to_path_buf();
        let p = path.clone();
        std::thread::spawn(move || read_progress(out, ff, shared, dir, p, number))
    });
    shared.chunks.write().push(VideoChunk {
        chunk_number: number,
        path: path.clone(),
        start_time: Utc::now(),
        end_time: None,
        size_bytes: 0,
        duration_secs: 0.0,
    });
    log::info!("Started chunk {} recording", number);
    Ok(ChunkProc { number, path, child, first_frame, reader })
}

/// Drain ffmpeg's `-progress` output (it must be read, or ffmpeg blocks) and
/// record the first frame's wall-clock time.
fn read_progress(
    out: std::process::ChildStdout,
    first_frame: Arc<(Mutex<Option<DateTime<Utc>>>, Condvar)>,
    shared: Arc<Shared>,
    dir: PathBuf,
    path: PathBuf,
    number: u32,
) {
    let mut frame = 0i64;
    let mut out_us = 0i64;
    for line in BufReader::new(out).lines() {
        let Ok(line) = line else { break };
        if let Some(v) = line.strip_prefix("frame=") {
            frame = v.trim().parse().unwrap_or(frame);
        } else if let Some(v) = line.strip_prefix("out_time_us=") {
            out_us = v.trim().parse().unwrap_or(out_us);
        } else if line.starts_with("progress=") && frame > 0 {
            let (lock, cv) = &*first_frame;
            let mut ff = lock.lock();
            if ff.is_none() {
                let start = Utc::now() - chrono::Duration::microseconds(out_us.max(0));
                *ff = Some(start);
                cv.notify_all();
                if let Err(e) = crate::redaction::video_blank::set_chunk_start(&dir, &path, start) {
                    log::warn!("{}", e);
                }
                if let Some(c) = shared.chunks.write().iter_mut().find(|c| c.chunk_number == number) {
                    c.start_time = start;
                }
            }
        }
    }
}

/// Ask ffmpeg to finish the chunk ('q'), wait (bounded), then record its
/// end time and size.
fn stop_chunk(shared: &Arc<Shared>, mut p: ChunkProc) {
    if let Some(stdin) = p.child.stdin.as_mut() {
        let _ = stdin.write_all(b"q");
        let _ = stdin.flush();
    }
    let deadline = Instant::now() + STOP_WAIT;
    loop {
        match p.child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    log::warn!("ffmpeg exited with status: {}", status);
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                log::error!("ffmpeg didn't finish chunk {} in time; stopping it", p.number);
                let _ = p.child.kill();
                let _ = p.child.wait();
                break;
            }
        }
    }
    if let Some(r) = p.reader.take() {
        let _ = r.join();
    }
    let now = Utc::now();
    if let Some(c) = shared.chunks.write().iter_mut().find(|c| c.chunk_number == p.number) {
        c.end_time = Some(now);
        c.size_bytes = std::fs::metadata(&p.path).map(|m| m.len()).unwrap_or(0);
        c.duration_secs = (now - c.start_time).num_milliseconds() as f64 / 1000.0;
    }
}

/// Every `chunk_secs`: start the next chunk, wait for its first frame, then
/// stop the previous one, so consecutive chunks overlap slightly instead of
/// leaving a gap. If the next chunk can't start, the current one keeps
/// recording and rotation is tried again next interval.
fn rotation_loop(shared: Arc<Shared>, input_args: Vec<String>, video_dir: PathBuf, chunk_secs: u64) {
    log::info!("Chunk rotation started ({}s intervals)", chunk_secs);
    loop {
        let deadline = Instant::now() + Duration::from_secs(chunk_secs);
        {
            let (lock, cv) = &shared.stop;
            let mut stopped = lock.lock();
            while !*stopped {
                if cv.wait_until(&mut stopped, deadline).timed_out() {
                    break;
                }
            }
            if *stopped {
                break;
            }
        }
        let next = shared.current_chunk.load(Ordering::SeqCst) + 1;
        let mut newp = match spawn_chunk(&shared, &input_args, next, &video_dir) {
            Ok(p) => p,
            Err(e) => {
                log::error!("Couldn't start screen video chunk {}: {}", next, e);
                continue;
            }
        };
        let got = newp.wait_first_frame(FIRST_FRAME_WAIT);
        if got.is_none() {
            if let Ok(Some(status)) = newp.child.try_wait() {
                // The new process died: keep recording into the current chunk
                log::error!("Screen video chunk {} failed to start ({}); retrying next interval", next, status);
                if let Some(r) = newp.reader.take() {
                    let _ = r.join();
                }
                shared.chunks.write().retain(|c| c.chunk_number != next);
                let _ = std::fs::remove_file(&newp.path);
                continue;
            }
            log::warn!("Screen video chunk {} hasn't reported a frame yet; rotating anyway", next);
        }
        let old = shared.current.lock().replace(newp);
        shared.current_chunk.store(next, Ordering::SeqCst);
        if let Some(old) = old {
            stop_chunk(&shared, old);
        }
    }
    log::info!("Chunk rotation stopped");
}

impl Default for VideoRecorder {
    fn default() -> Self {
        let output_dir = crate::paths::app_data_dir();
        Self::new(output_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_chunk_creation() {
        let chunk = VideoChunk {
            chunk_number: 1,
            path: PathBuf::from("/tmp/chunk_001.mov"),
            start_time: Utc::now(),
            end_time: None,
            size_bytes: 0,
            duration_secs: 0.0,
        };
        assert_eq!(chunk.chunk_number, 1);
    }

    /// Real ffmpeg, a test pattern instead of the screen, 2-second chunks:
    /// rotation makes several chunks, each with a precise recorded start,
    /// back to back (overlapping slightly, never a real gap).
    #[test]
    fn chunks_rotate_with_precise_start_times_and_no_gap() {
        if find_tool("ffmpeg").is_none() || find_tool("ffprobe").is_none() {
            eprintln!("ffmpeg not installed; skipping rotation test");
            return;
        }
        let dir = std::env::temp_dir().join(format!("nf-rot-{}", uuid::Uuid::new_v4()));
        let input = ["-re", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=15"];
        let rec = VideoRecorder::with_input(dir.clone(), input.iter().map(|s| s.to_string()).collect(), 2);
        if rec.start("m1").is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5600));
        let session = rec.stop().unwrap();
        let vdir = dir.join("m1").join("video");
        let chunks = crate::redaction::video_blank::list_chunks(&vdir);
        if chunks.is_empty() || crate::redaction::video_blank::probe_duration(&chunks[0]).is_err() {
            eprintln!("this machine's ffmpeg can't encode with VideoToolbox; skipping");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        assert!(chunks.len() >= 2, "rotated into {} chunks", chunks.len());
        assert_eq!(session.chunks.len(), chunks.len());
        let mut prev_end: Option<DateTime<Utc>> = None;
        let mut prev_start: Option<DateTime<Utc>> = None;
        for c in &chunks {
            let start = crate::redaction::video_blank::chunk_start(&vdir, c).expect("recorded start");
            let dur = crate::redaction::video_blank::probe_duration(c).unwrap();
            assert!(dur > 0.5, "{} is {}s", c.display(), dur);
            if let (Some(pe), Some(ps)) = (prev_end, prev_start) {
                let gap = (start - pe).num_milliseconds();
                assert!(gap < 400, "gap between chunks: {} ms", gap);
                let step = (start - ps).num_milliseconds();
                eprintln!("{}: starts {} ms after the previous one; gap {} ms", c.display(), step, gap);
                assert!((1500..=4500).contains(&step), "chunk starts {} ms apart", step);
            }
            prev_start = Some(start);
            prev_end = Some(start + chrono::Duration::milliseconds((dur * 1000.0) as i64));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Locate a Homebrew/system CLI tool. Apps launched from Finder don't get the
/// shell PATH, so a bare `Command::new("ffmpeg")` fails there.
pub fn find_tool(name: &str) -> Option<std::path::PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .iter()
        .map(|dir| std::path::Path::new(dir).join(name))
        .find(|p| p.exists())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|d| d.join(name))
                    .find(|p| p.exists())
            })
        })
}
