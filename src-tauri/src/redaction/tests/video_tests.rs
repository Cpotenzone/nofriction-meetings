//! Screen video blanking: the durable job queue (fake ffmpeg: retry,
//! backoff, resume, never under the redaction lock) and, on the DMG build,
//! real ffmpeg (partial re-encode, idempotency, per-chunk offsets).

use super::*;
use crate::redaction::video_jobs::{self, BlankError, BlankReport, JobEvent, VideoOps};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Stand-in for ffmpeg: fails the first `fail` calls, optionally blocks
/// until released, reports 40% progress.
struct FakeOps {
    calls: AtomicUsize,
    fail: usize,
    error: BlankError,
    started: AtomicBool,
    release: AtomicBool,
    block: bool,
}

impl FakeOps {
    fn new(fail: usize, error: BlankError, block: bool) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail,
            error,
            started: AtomicBool::new(false),
            release: AtomicBool::new(false),
            block,
        })
    }
}

impl VideoOps for FakeOps {
    fn blank(
        &self,
        _env: &RedactionEnv,
        _meeting_id: &str,
        _ranges: &[(DateTime<Utc>, DateTime<Utc>)],
        progress: &dyn Fn(f32),
        cancel: &AtomicBool,
    ) -> Result<BlankReport, BlankError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.started.store(true, Ordering::SeqCst);
        if self.block {
            let t = std::time::Instant::now();
            while !self.release.load(Ordering::SeqCst) && t.elapsed().as_secs() < 30 {
                if cancel.load(Ordering::SeqCst) {
                    return Err(BlankError::Cancelled);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        progress(0.4);
        if n <= self.fail {
            Err(self.error.clone())
        } else {
            Ok(BlankReport { chunks_rewritten: 1, ranges_already_blank: 0 })
        }
    }
}

async fn queue_job(f: &Fixture, redaction_id: Option<&str>) {
    let mut tx = f.db.pool().begin().await.unwrap();
    let t = Utc::now() - chrono::Duration::minutes(5);
    video_jobs::enqueue(&mut tx, "m1", &[(t, t + chrono::Duration::seconds(3))], redaction_id).await.unwrap();
    tx.commit().await.unwrap();
}

async fn run_with(
    f: &Fixture,
    env: &RedactionEnv,
    ops: Arc<dyn VideoOps>,
    now: DateTime<Utc>,
) -> (video_jobs::RunSummary, Vec<JobEvent>) {
    let events: Arc<Mutex<Vec<JobEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let ev2 = events.clone();
    let s = video_jobs::run_due(
        f.db.pool(),
        env,
        ops,
        now,
        Arc::new(AtomicBool::new(false)),
        Arc::new(move |e| ev2.lock().unwrap().push(e)),
    )
    .await
    .unwrap();
    let ev = events.lock().unwrap().clone();
    (s, ev)
}

#[test]
fn backoff_doubles_from_30s_and_caps_at_an_hour() {
    let secs: Vec<i64> = (1..=10).map(|a| video_jobs::backoff(a).num_seconds()).collect();
    assert_eq!(secs, vec![30, 60, 120, 240, 480, 960, 1920, 3600, 3600, 3600]);
}

#[test]
fn ranges_are_merged_before_queueing() {
    let t = Utc::now();
    let s = |x: i64| t + chrono::Duration::seconds(x);
    let m = video_jobs::merge_ranges(&[(s(10), s(12)), (s(0), s(2)), (s(2), s(3)), (s(11), s(15)), (s(40), s(41))]);
    assert_eq!(m, vec![(s(0), s(3)), (s(10), s(15)), (s(40), s(41))]);
}

#[tokio::test]
async fn failed_job_is_retried_with_backoff_then_parked_until_launch() {
    let f = setup().await;
    queue_job(&f, None).await;
    let ops = FakeOps::new(100, BlankError::Failed("encoder broke".into()), false);
    let (s, ev) = run_with(&f, &f.env, ops.clone(), Utc::now()).await;
    assert_eq!(s.meetings_failed, 1);
    assert!(ev.iter().any(|e| e.status == "failed" && e.error.as_deref() == Some("encoder broke")));
    let job = &video_jobs::list(f.db.pool(), Some("m1")).await.unwrap()[0];
    assert_eq!((job.status.as_str(), job.attempts), ("failed", 1));
    let next = parse_ts(job.next_attempt_at.as_deref().unwrap()).unwrap();
    let wait = (next - Utc::now()).num_seconds();
    assert!((25..=30).contains(&wait), "first retry in ~30 s, got {}", wait);
    // Not due before then: no tight loop
    let (s, _) = run_with(&f, &f.env, ops.clone(), Utc::now() + chrono::Duration::seconds(10)).await;
    assert_eq!(s, video_jobs::RunSummary::default());
    assert_eq!(ops.calls.load(Ordering::SeqCst), 1);
    // Each later attempt waits twice as long, until it's parked
    for attempt in 2..=video_jobs::MAX_AUTO_ATTEMPTS {
        let far = Utc::now() + video_jobs::backoff(attempt - 1) + chrono::Duration::seconds(5);
        let (s, _) = run_with(&f, &f.env, ops.clone(), far).await;
        assert_eq!(s.meetings_failed, 1, "attempt {}", attempt);
    }
    let job = &video_jobs::list(f.db.pool(), Some("m1")).await.unwrap()[0];
    assert_eq!(job.attempts, video_jobs::MAX_AUTO_ATTEMPTS);
    assert_eq!(job.next_attempt_at, None, "parked: no more automatic retries");
    let (s, _) = run_with(&f, &f.env, ops.clone(), Utc::now() + chrono::Duration::days(365)).await;
    assert_eq!(s, video_jobs::RunSummary::default());
    // The next launch (or Retry) gives it a fresh attempt
    video_jobs::prepare_for_launch(f.db.pool()).await.unwrap();
    let ok = FakeOps::new(0, BlankError::Failed(String::new()), false);
    let (s, ev) = run_with(&f, &f.env, ok, Utc::now()).await;
    assert_eq!(s.jobs_done, 1);
    assert!(ev.iter().any(|e| e.status == "done" && e.remaining == 0));
    assert!(video_jobs::list(f.db.pool(), None).await.unwrap().is_empty(), "done jobs leave no row");
}

#[tokio::test]
async fn interrupted_job_resumes_at_launch() {
    let f = setup().await;
    queue_job(&f, None).await;
    // The app quit or crashed mid-job
    sqlx::query("UPDATE video_blank_jobs SET status = 'running', attempts = 1").execute(f.db.pool()).await.unwrap();
    let ops = FakeOps::new(0, BlankError::Failed(String::new()), false);
    let (s, _) = run_with(&f, &f.env, ops.clone(), Utc::now()).await;
    assert_eq!(s, video_jobs::RunSummary::default(), "a running job isn't picked twice");
    video_jobs::prepare_for_launch(f.db.pool()).await.unwrap();
    let (s, _) = run_with(&f, &f.env, ops.clone(), Utc::now()).await;
    assert_eq!(s.jobs_done, 1);
    assert_eq!(ops.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn missing_ffmpeg_keeps_the_job_with_a_persistent_warning() {
    let f = setup().await;
    queue_job(&f, None).await;
    let ops = FakeOps::new(1, BlankError::ToolMissing("ffmpeg isn't installed".into()), false);
    let (_, ev) = run_with(&f, &f.env, ops, Utc::now()).await;
    assert!(ev.iter().any(|e| e.status == "failed" && e.tool_missing));
    let jobs = video_jobs::list(f.db.pool(), Some("m1")).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].tool_missing);
    // Retry button makes it due now
    assert_eq!(video_jobs::retry_now(f.db.pool(), Some("m1")).await.unwrap(), 1);
    let ok = FakeOps::new(0, BlankError::Failed(String::new()), false);
    assert_eq!(run_with(&f, &f.env, ok, Utc::now()).await.0.jobs_done, 1);
}

#[tokio::test]
async fn quitting_mid_job_puts_it_back_without_counting_an_attempt() {
    let f = setup().await;
    queue_job(&f, None).await;
    let ops = FakeOps::new(1, BlankError::Cancelled, false);
    run_with(&f, &f.env, ops, Utc::now()).await;
    let job = &video_jobs::list(f.db.pool(), Some("m1")).await.unwrap()[0];
    assert_eq!((job.status.as_str(), job.attempts), ("pending", 0));
}

#[tokio::test]
async fn jobs_of_a_meeting_being_recorded_wait() {
    let f = setup().await;
    queue_job(&f, None).await;
    let rec = RedactionEnv { recording_meetings: vec!["m1".into()], ..f.env.clone() };
    let ops = FakeOps::new(0, BlankError::Failed(String::new()), false);
    assert_eq!(run_with(&f, &rec, ops.clone(), Utc::now()).await.0, video_jobs::RunSummary::default());
    assert_eq!(ops.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run_with(&f, &f.env, ops, Utc::now()).await.0.jobs_done, 1);
}

#[tokio::test]
async fn progress_is_reported_and_strike_marker_tracks_the_pending_video() {
    let f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let rec = strike_screens(f.db.pool(), &f.env, "m1", &[a], None).await.unwrap().record.unwrap();
    queue_job(&f, Some(&rec.id)).await;
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(tl.redactions[0].video_pending);
    let ops = FakeOps::new(0, BlankError::Failed(String::new()), false);
    let (_, ev) = run_with(&f, &f.env, ops, Utc::now()).await;
    let statuses: Vec<(String, Option<u32>)> = ev.iter().map(|e| (e.status.clone(), e.percent)).collect();
    assert_eq!(
        statuses,
        vec![("running".into(), Some(0)), ("running".into(), Some(40)), ("done".into(), Some(100))]
    );
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(!tl.redactions[0].video_pending);
}

/// The bug that made the app look hung: screen video work ran inside the
/// app-wide redaction lock, so every other edit waited minutes behind
/// ffmpeg. Now the job runs outside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_video_job_never_blocks_other_edits() {
    let f = setup().await;
    queue_job(&f, None).await;
    let ops = FakeOps::new(0, BlankError::Failed(String::new()), true);
    let pool = f.db.pool().clone();
    let env = f.env.clone();
    let ops2: Arc<dyn VideoOps> = ops.clone();
    let worker = tokio::spawn(async move {
        video_jobs::run_due(&pool, &env, ops2, Utc::now(), Arc::new(AtomicBool::new(false)), Arc::new(|_| {})).await
    });
    let t = std::time::Instant::now();
    while !ops.started.load(Ordering::SeqCst) {
        assert!(t.elapsed().as_secs() < 10, "job didn't start");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // While ffmpeg "runs": delete + undo, delete + commit, strike, screens
    let quick = std::time::Duration::from_secs(3);
    let text = "please remove secretword now";
    let id = add_line(&f, text).await;
    let p = tokio::time::timeout(quick, request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "secretword"))))
        .await
        .expect("delete request waited on the video job")
        .unwrap();
    tokio::time::timeout(quick, undo_delete(f.db.pool(), &p.id)).await.expect("undo waited").unwrap();
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "secretword"))).await.unwrap();
    tokio::time::timeout(quick, commit_pending(f.db.pool(), &f.env, &p.id))
        .await
        .expect("commit waited on the video job")
        .unwrap();
    let a = add_screen(&f, "a", 10).await;
    tokio::time::timeout(quick, strike_screens(f.db.pool(), &f.env, "m1", &[a], None))
        .await
        .expect("strike waited on the video job")
        .unwrap();
    assert!(!worker.is_finished(), "the job was still running the whole time");
    ops.release.store(true, Ordering::SeqCst);
    let s = worker.await.unwrap().unwrap();
    assert_eq!(s.jobs_done, 1);
}

// ─── Real ffmpeg (DMG) ───────────────────────────────────────────────────

#[cfg(not(feature = "mas"))]
mod ffmpeg {
    use super::*;
    use crate::redaction::video_blank::{self, ChunkPlan, Method};
    use std::process::Command;

    /// A test-pattern clip encoded like the recorder (VideoToolbox H.264,
    /// 3 Mb/s, 15 fps). None if this machine can't make one.
    pub(super) fn recorder_like_clip(path: &Path, secs: u32, size: &str) -> Option<()> {
        let ffmpeg = crate::video_recorder::find_tool("ffmpeg")?;
        crate::video_recorder::find_tool("ffprobe")?;
        let ok = Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=size={}:rate=15:duration={}", size, secs))
            .args(["-c:v", "h264_videotoolbox", "-b:v", "3M", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
            .arg(path)
            .status()
            .ok()?
            .success();
        (ok && video_blank::probe_duration(path).is_ok()).then_some(())
    }

    fn nb_frames(path: &Path) -> String {
        let out = Command::new(crate::video_recorder::find_tool("ffprobe").unwrap())
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=nb_frames", "-of", "csv=p=0"])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// (pts, size) of every packet: equal entries outside the re-encoded
    /// GOPs prove the rest was stream-copied, not re-encoded.
    fn packets(path: &Path) -> Vec<(f64, i64)> {
        let out = Command::new(crate::video_recorder::find_tool("ffprobe").unwrap())
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,size", "-of", "csv=p=0"])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut it = l.split(',');
                Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
            })
            .collect()
    }

    fn plan_for(chunk: &Path, a: f64, b: f64) -> ChunkPlan {
        ChunkPlan {
            chunk: chunk.to_path_buf(),
            start: Utc::now() - chrono::Duration::hours(1),
            duration: video_blank::probe_duration(chunk).unwrap(),
            ranges: vec![(a, b)],
        }
    }

    fn black(path: &Path, t: f64) -> bool {
        video_blank::frame_is_black(path, t).expect("decodable frame")
    }

    #[test]
    fn partial_reencode_blacks_only_the_range_and_keeps_length_and_frames() {
        // A space and a quote in the path, like "Application Support"
        let dir = std::env::temp_dir().join(format!("nf vid's {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let chunk = dir.join("chunk_001.mov");
        if recorder_like_clip(&chunk, 30, "640x360").is_none() {
            eprintln!("can't encode a VideoToolbox test clip here; skipping");
            return;
        }
        let before_frames = nb_frames(&chunk);
        let before_dur = video_blank::probe_duration(&chunk).unwrap();
        let before_packets = packets(&chunk);
        let never = AtomicBool::new(false);
        let (method, skipped) = video_blank::blank_chunk(&dir, &plan_for(&chunk, 10.3, 14.2), &|_| {}, &never).unwrap();
        assert_eq!(method, Some(Method::Partial));
        assert_eq!(skipped, 0);
        assert_eq!(nb_frames(&chunk), before_frames, "same number of frames");
        assert!((video_blank::probe_duration(&chunk).unwrap() - before_dur).abs() < 0.05, "same length");
        for t in [10.4, 12.0, 14.1] {
            assert!(black(&chunk, t), "{} s is black", t);
        }
        for t in [2.0, 9.5, 10.1, 14.5, 20.0, 29.5] {
            assert!(!black(&chunk, t), "{} s is untouched", t);
        }
        // Outside the GOPs around the range (keyframes every 0.8 s), every
        // packet is byte-for-byte the same size at the same time: copied
        let after_packets = packets(&chunk);
        assert_eq!(before_packets.len(), after_packets.len());
        let outside = |p: &&(f64, i64)| p.0 < 9.5 || p.0 >= 15.3;
        let a: Vec<_> = before_packets.iter().filter(outside).collect();
        let b: Vec<_> = after_packets.iter().filter(outside).collect();
        assert_eq!(a, b);
        // No temp files left behind; the range is recorded
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(names.iter().all(|n| !n.contains(".blank-")), "{:?}", names);
        assert!(video_blank::covered(&video_blank::blanked_ranges(&dir, &chunk), 10.3, 14.2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blanking_the_same_range_again_doesnt_reencode() {
        let dir = std::env::temp_dir().join(format!("nf-vid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let chunk = dir.join("chunk_001.mov");
        if recorder_like_clip(&chunk, 12, "320x240").is_none() {
            return;
        }
        let never = AtomicBool::new(false);
        let plan = plan_for(&chunk, 4.0, 6.0);
        assert!(video_blank::blank_chunk(&dir, &plan, &|_| {}, &never).unwrap().0.is_some());
        let bytes = std::fs::read(&chunk).unwrap();
        // Retried (e.g. a crash before the job row was removed): skipped
        let (method, skipped) = video_blank::blank_chunk(&dir, &plan, &|_| {}, &never).unwrap();
        assert_eq!((method, skipped), (None, 1));
        // A sub-range of a blanked range: skipped too
        let (method, _) = video_blank::blank_chunk(&dir, &plan_for(&chunk, 4.5, 5.5), &|_| {}, &never).unwrap();
        assert_eq!(method, None);
        assert_eq!(std::fs::read(&chunk).unwrap(), bytes, "file untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The owner's case: an older build already blanked the range with a
    /// full re-encode, then its database step failed, so the delete stayed
    /// pending with no record of the blanking. Retrying must not re-encode.
    #[test]
    fn range_blanked_by_an_older_build_is_detected_not_reencoded() {
        let dir = std::env::temp_dir().join(format!("nf-vid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mov");
        if recorder_like_clip(&src, 12, "320x240").is_none() {
            return;
        }
        let chunk = dir.join("chunk_001.mov");
        // Exactly what the old code ran (padded range, whole chunk)
        let ok = Command::new(crate::video_recorder::find_tool("ffmpeg").unwrap())
            .args(["-v", "error", "-y", "-i"])
            .arg(&src)
            .args(["-vf", "drawbox=x=0:y=0:w=iw:h=ih:color=black:t=fill:enable='between(t,4.000,6.000)'"])
            .args(["-c:v", "h264_videotoolbox", "-b:v", "3M", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an"])
            .arg(&chunk)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let bytes = std::fs::read(&chunk).unwrap();
        let never = AtomicBool::new(false);
        let (method, skipped) = video_blank::blank_chunk(&dir, &plan_for(&chunk, 4.0, 6.0), &|_| {}, &never).unwrap();
        assert_eq!((method, skipped), (None, 1), "detected as already black");
        assert_eq!(std::fs::read(&chunk).unwrap(), bytes, "not re-encoded");
        assert!(video_blank::covered(&video_blank::blanked_ranges(&dir, &chunk), 4.0, 6.0), "now recorded");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each chunk is blanked at its own offset (its recorded start time).
    #[tokio::test]
    async fn job_blanks_each_chunk_at_its_own_offset() {
        let mut f = setup().await;
        f.env.video_enabled = true;
        let vdir = video_blank::video_dir(&f.env, "m1");
        std::fs::create_dir_all(&vdir).unwrap();
        let c1 = vdir.join("chunk_001.mov");
        let c2 = vdir.join("chunk_002.mov");
        if recorder_like_clip(&c1, 6, "320x240").is_none() || recorder_like_clip(&c2, 6, "320x240").is_none() {
            return;
        }
        let t = Utc::now() - chrono::Duration::minutes(10);
        video_blank::set_chunk_start(&vdir, &c1, t).unwrap();
        video_blank::set_chunk_start(&vdir, &c2, t + chrono::Duration::seconds(6)).unwrap();
        // Wall clock [t+5.0, t+7.0] → padded [3.5, 8.5]: c1 3.5–6, c2 0–2.5
        let mut tx = f.db.pool().begin().await.unwrap();
        video_jobs::enqueue(&mut tx, "m1", &[(t + chrono::Duration::seconds(5), t + chrono::Duration::seconds(7))], None)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let s = run_jobs(&f).await;
        assert_eq!(s.jobs_done, 1, "{:?}", video_jobs::list(f.db.pool(), None).await);
        assert!(!black(&c1, 2.5));
        assert!(black(&c1, 4.5));
        assert!(black(&c1, 5.8));
        assert!(black(&c2, 0.2));
        assert!(black(&c2, 2.0));
        assert!(!black(&c2, 3.5));
    }

    /// Timings for the report: the old whole-chunk re-encode vs the partial
    /// one, on a 30-minute 1080p recording-like file, blanking one minute.
    ///   cargo test --lib bench_partial_vs_full -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_partial_vs_full_on_30_minutes() {
        let dir = std::env::temp_dir().join(format!("nf-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mov");
        let t = std::time::Instant::now();
        if recorder_like_clip(&src, 1800, "1920x1080").is_none() {
            eprintln!("can't encode; skipping");
            return;
        }
        println!("generated 30 min 1080p clip in {:.1}s ({} MB)", t.elapsed().as_secs_f64(), std::fs::metadata(&src).unwrap().len() / 1_000_000);
        let ffmpeg = crate::video_recorder::find_tool("ffmpeg").unwrap();
        // Old: re-encode the whole chunk (the command the old code ran)
        let full = dir.join("full.mov");
        let t = std::time::Instant::now();
        assert!(Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(&src)
            .args(["-vf", "drawbox=x=0:y=0:w=iw:h=ih:color=black:t=fill:enable='between(t,600.000,660.000)'"])
            .args(["-c:v", "h264_videotoolbox", "-b:v", "3M", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an"])
            .arg(&full)
            .status()
            .unwrap()
            .success());
        println!("old full re-encode (1 min blanked): {:.1}s", t.elapsed().as_secs_f64());
        for (label, a, b) in [("5 s", 600.0, 605.0), ("1 min", 600.0, 660.0), ("12 min", 1080.0, 1800.0)] {
            let chunk = dir.join("chunk_001.mov");
            std::fs::copy(&src, &chunk).unwrap();
            let _ = std::fs::remove_file(dir.join("blanked.json"));
            let never = AtomicBool::new(false);
            let t = std::time::Instant::now();
            let (m, _) = video_blank::blank_chunk(&dir, &plan_for(&chunk, a, b), &|_| {}, &never).unwrap();
            println!("new partial ({} blanked): {:.1}s ({:?})", label, t.elapsed().as_secs_f64(), m);
            assert!(black(&chunk, (a + b) / 2.0));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
