//! Automatic meeting-end detection for ANY active recording (manual or
//! triggered).
//!
//! While a recording runs, a monitor task samples a few signals every 2s and
//! feeds them to a pure, unit-tested state machine (`Detector`):
//!
//! - **Call app released the mic** (strongest): Core Audio process objects
//!   (macOS 14.2+) say which other processes are running audio input. A known
//!   call app (Zoom, Teams, Webex, Slack, FaceTime, Discord, or a browser for
//!   web meetings) that held the mic for ≥2 min during the recording (so a
//!   brief dictation in a browser can't arm it) and has stopped for ≥20s,
//!   with nobody speaking for 60s, ends the meeting. If speech cancels that
//!   countdown, the call app must be seen holding the mic again before a
//!   release counts. Unavailable on older macOS → signal skipped.
//! - **Meeting window gone** (secondary): a call window (e.g. "Zoom Meeting",
//!   a Teams call, a "Meet - …" tab) seen during the recording has been gone
//!   ≥20s AND nobody has spoken for 60s. Titles need Screen Recording
//!   permission; without it no window is ever seen and the signal is inert.
//! - **Calendar**: the recording matched a calendar event, it is past the
//!   event's end + 2 min, and nobody has spoken for 60s.
//! - **Sustained silence**: no real (post-hallucination-filter) speech for N
//!   minutes (setting, default 3) while transcription is running. Before
//!   anyone has spoken at all the bar is 10 min, so a recording started
//!   early (waiting room) isn't cut off.
//!
//! On detection the UI gets `meeting-end-detected` and shows a 30s countdown
//! with Keep recording (snooze) / Stop now. If nobody answers, the monitor
//! emits `meeting-end-auto-stop` (the UI stops the recording through the
//! user's own Stop path) and falls back to a backend stop if the UI doesn't.
//! Junk segments transcribed after the detected end point are then trimmed —
//! only text the hallucination filter classifies as junk, never real speech.

use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use crate::core_audio::AudioProcess;

pub const SETTING_ENABLED: &str = "auto_stop_on_meeting_end";
pub const SETTING_SILENCE_MINUTES: &str = "auto_stop_silence_minutes";
pub const DEFAULT_SILENCE_MINUTES: i64 = 3;

// ─── Speech activity (fed by the transcription providers) ───────────────────

#[derive(Default)]
struct SpeechActivity {
    last_real: Option<DateTime<Utc>>,
    filtered: u64,
}

static SPEECH: Lazy<Mutex<SpeechActivity>> = Lazy::new(|| Mutex::new(SpeechActivity::default()));

/// A final segment passed the hallucination filter (real speech ending at `at`).
pub fn note_real_speech(at: DateTime<Utc>) {
    let mut s = SPEECH.lock();
    if s.last_real.map(|l| at > l).unwrap_or(true) {
        s.last_real = Some(at);
    }
}

/// A final segment was dropped as junk.
pub fn note_filtered_segment() {
    SPEECH.lock().filtered += 1;
}

pub fn last_real_speech() -> Option<DateTime<Utc>> {
    SPEECH.lock().last_real
}

fn reset_speech() {
    *SPEECH.lock() = SpeechActivity::default();
}

// ─── Call apps (by bundle id) ────────────────────────────────────────────────

/// Bundle-id prefixes (lowercase) of apps whose mic use means "in a call".
/// Browsers count because Meet/Zoom/Teams run in them; their mic is held by
/// a helper process whose bundle id extends the browser's.
const CALL_APPS: &[(&str, &str)] = &[
    ("us.zoom.", "Zoom"),
    ("com.microsoft.teams", "Microsoft Teams"),
    ("cisco-systems.spark", "Webex"),
    ("com.webex.", "Webex"),
    ("com.cisco.webex", "Webex"),
    ("com.tinyspeck.slackmacgap", "Slack"),
    ("com.apple.facetime", "FaceTime"),
    ("com.apple.avconferenced", "FaceTime"),
    ("com.hnc.discord", "Discord"),
    ("com.google.chrome", "Google Chrome"),
    ("com.apple.safari", "Safari"),
    ("com.apple.webkit.gpu", "Safari"),
    ("company.thebrowser.browser", "Arc"),
    ("com.microsoft.edgemac", "Microsoft Edge"),
    ("org.mozilla.firefox", "Firefox"),
    ("com.brave.browser", "Brave"),
    ("com.vivaldi.vivaldi", "Vivaldi"),
    ("com.operasoftware.opera", "Opera"),
    ("co.teamport.around", "Around"),
    ("com.whereby", "Whereby"),
    ("com.skype.", "Skype"),
    ("com.logmein.goto", "GoTo Meeting"),
    ("com.ringcentral", "RingCentral"),
    ("com.amazon.chime", "Amazon Chime"),
    ("net.whatsapp", "WhatsApp"),
    ("desktop.whatsapp", "WhatsApp"),
    ("ru.keepcoder.telegram", "Telegram"),
];

/// Our own bundle id prefix: never counts as a call app.
const OWN_BUNDLE_PREFIX: &str = "com.nofriction.";

/// Display name of the call app a bundle id belongs to, if any.
pub fn call_app_name(bundle_id: &str) -> Option<&'static str> {
    let b = bundle_id.to_lowercase();
    if b.is_empty() || b.starts_with(OWN_BUNDLE_PREFIX) || b.starts_with("ai.nofriction.") {
        return None;
    }
    CALL_APPS.iter().find(|(prefix, _)| b.starts_with(prefix)).map(|(_, name)| *name)
}

/// Call apps (display names) using audio input right now, excluding our own
/// process.
pub fn active_call_apps(procs: &[AudioProcess], own_pid: i32) -> BTreeSet<String> {
    procs
        .iter()
        .filter(|p| p.running_input && p.pid != own_pid)
        .filter_map(|p| call_app_name(&p.bundle_id))
        .map(str::to_string)
        .collect()
}

// ─── Meeting windows (by owner + title) ──────────────────────────────────────

/// Label for a window that is an active call/meeting, if it is one.
pub fn meeting_window_label(owner: &str, title: &str) -> Option<String> {
    let o = owner.to_lowercase();
    let t = title.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    let label = if o.contains("zoom")
        && (t.contains("zoom meeting") || t == "meeting" || t.contains("zoom webinar"))
    {
        "Zoom meeting"
    } else if o.contains("teams") && (t.contains("meeting") || t.contains("call")) {
        "Teams call"
    } else if o.contains("webex") && t.contains("meeting") {
        "Webex meeting"
    } else if o.contains("slack") && t.contains("huddle") {
        "Slack huddle"
    } else if t.starts_with("meet - ") || t.contains("google meet") || t.contains("meet.google.com") {
        "Google Meet"
    } else if t.contains("zoom meeting") {
        "Zoom meeting"
    } else if t.contains("microsoft teams") && (t.contains("meeting") || t.contains("call")) {
        "Teams call"
    } else if t.contains("whereby") {
        "Whereby"
    } else {
        return None;
    };
    Some(label.to_string())
}

// ─── Signal source (injectable for tests) ────────────────────────────────────

pub trait SignalSource: Send + Sync {
    /// Call apps using the mic now; None = signal unavailable on this system.
    fn call_apps(&self) -> Option<BTreeSet<String>>;
    /// Meeting windows on screen now; None = signal unavailable.
    fn meeting_windows(&self) -> Option<BTreeSet<String>>;
}

/// The real system: Core Audio process objects + the window server.
pub struct SystemSignals {
    own_pid: i32,
}

impl SystemSignals {
    pub fn new() -> Self {
        Self { own_pid: std::process::id() as i32 }
    }
}

impl Default for SystemSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalSource for SystemSignals {
    fn call_apps(&self) -> Option<BTreeSet<String>> {
        crate::core_audio::audio_processes().map(|p| active_call_apps(&p, self.own_pid))
    }

    fn meeting_windows(&self) -> Option<BTreeSet<String>> {
        let windows = crate::privacy_filter::on_screen_windows()?;
        // No titles at all = no Screen Recording permission: unavailable
        if windows.iter().all(|(_, t)| t.is_empty()) {
            return None;
        }
        Some(windows.iter().filter_map(|(o, t)| meeting_window_label(o, t)).collect())
    }
}

// ─── Decision logic (pure) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    MicReleased,
    WindowClosed,
    CalendarEnded,
    Silence,
}

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub silence: Duration,
    /// Silence needed before anyone has spoken in this recording.
    pub silence_before_first_speech: Duration,
    pub mic_grace: Duration,
    /// How long a call app must hold the mic (continuously) before its
    /// release counts. Short use (browser dictation) never arms the signal.
    pub min_mic_hold: Duration,
    pub window_grace: Duration,
    pub calendar_grace: Duration,
    /// Quiet time that must back up the secondary signals.
    pub corroborating_quiet: Duration,
    pub countdown: Duration,
    pub snooze: Duration,
}

impl DetectorConfig {
    pub fn with_silence_minutes(minutes: i64) -> Self {
        Self {
            silence: Duration::minutes(minutes.clamp(1, 60)),
            silence_before_first_speech: Duration::minutes(10),
            mic_grace: Duration::seconds(20),
            min_mic_hold: Duration::minutes(2),
            window_grace: Duration::seconds(20),
            calendar_grace: Duration::minutes(2),
            corroborating_quiet: Duration::seconds(60),
            countdown: Duration::seconds(30),
            snooze: Duration::minutes(10),
        }
    }
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self::with_silence_minutes(DEFAULT_SILENCE_MINUTES)
    }
}

/// One sample of the world.
#[derive(Debug, Clone)]
pub struct Observation {
    pub now: DateTime<Utc>,
    pub call_apps: Option<BTreeSet<String>>,
    pub meeting_windows: Option<BTreeSet<String>>,
    pub last_real_speech: Option<DateTime<Utc>>,
    pub transcription_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pending {
    pub kind: SignalKind,
    /// Human-readable reason for the banner.
    pub reason: String,
    pub detected_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    /// Best estimate of when the meeting actually ended.
    pub end_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Detected(Pending),
    Cancelled(&'static str),
    AutoStop(Pending),
}

pub struct Detector {
    cfg: DetectorConfig,
    calendar: Option<(DateTime<Utc>, String)>,
    /// Call apps that held the mic for ≥ `min_mic_hold` (their release counts).
    seen_apps: BTreeSet<String>,
    /// Call apps holding the mic right now, and since when.
    mic_held_since: BTreeMap<String, DateTime<Utc>>,
    apps_absent_since: Option<DateTime<Utc>>,
    seen_windows: BTreeSet<String>,
    windows_absent_since: Option<DateTime<Utc>>,
    /// Silence is measured from max(this, last real speech).
    quiet_baseline: DateTime<Utc>,
    heard_speech: bool,
    pending: Option<Pending>,
    snooze_until: Option<DateTime<Utc>>,
    done: bool,
}

impl Detector {
    pub fn new(
        cfg: DetectorConfig,
        started_at: DateTime<Utc>,
        calendar: Option<(DateTime<Utc>, String)>,
    ) -> Self {
        Self {
            cfg,
            calendar,
            seen_apps: BTreeSet::new(),
            mic_held_since: BTreeMap::new(),
            apps_absent_since: None,
            seen_windows: BTreeSet::new(),
            windows_absent_since: None,
            quiet_baseline: started_at,
            heard_speech: false,
            pending: None,
            snooze_until: None,
            done: false,
        }
    }

    pub fn set_silence_minutes(&mut self, minutes: i64) {
        self.cfg.silence = Duration::minutes(minutes.clamp(1, 60));
    }

    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    /// Forget accumulated evidence (recording paused, detection disabled):
    /// signals must be observed afresh and silence restarts from `now`.
    pub fn reset(&mut self, now: DateTime<Utc>) {
        self.pending = None;
        self.forget_mic_evidence();
        self.forget_window_evidence();
        self.quiet_baseline = now;
    }

    /// "Keep recording": cancel any countdown, and suppress the silence and
    /// calendar signals for the snooze period. Mic/window signals need a new
    /// observation (call app seen again, then released) so they can fire as
    /// soon as there is genuinely new evidence. Returns whether a countdown
    /// was running.
    pub fn snooze(&mut self, now: DateTime<Utc>) -> bool {
        let was_pending = self.pending.is_some();
        self.reset(now);
        self.snooze_until = Some(now + self.cfg.snooze);
        was_pending
    }

    /// A call app must be seen holding the mic (long enough) again before a
    /// release counts.
    fn forget_mic_evidence(&mut self) {
        self.seen_apps.clear();
        self.mic_held_since.clear();
        self.apps_absent_since = None;
    }

    /// A meeting window must be seen again (and then close) before its
    /// absence counts.
    fn forget_window_evidence(&mut self) {
        self.seen_windows.clear();
        self.windows_absent_since = None;
    }

    fn fire(&mut self, now: DateTime<Utc>, kind: SignalKind, reason: String, end_at: DateTime<Utc>) -> Action {
        let p = Pending {
            kind,
            reason,
            detected_at: now,
            deadline: now + self.cfg.countdown,
            end_at: end_at.min(now),
        };
        self.pending = Some(p.clone());
        Action::Detected(p)
    }

    pub fn tick(&mut self, o: &Observation) -> Action {
        let now = o.now;
        if self.done {
            return Action::None;
        }

        // Fold in what the call apps / windows are doing
        if let Some(apps) = &o.call_apps {
            // Per-app continuous hold: an app that drops out restarts its clock
            self.mic_held_since.retain(|a, _| apps.contains(a));
            for a in apps {
                let since = *self.mic_held_since.entry(a.clone()).or_insert(now);
                if now - since >= self.cfg.min_mic_hold {
                    self.seen_apps.insert(a.clone());
                }
            }
            if apps.is_empty() {
                if !self.seen_apps.is_empty() && self.apps_absent_since.is_none() {
                    self.apps_absent_since = Some(now);
                }
            } else {
                self.apps_absent_since = None;
            }
        }
        if let Some(wins) = &o.meeting_windows {
            if wins.is_empty() {
                if !self.seen_windows.is_empty() && self.windows_absent_since.is_none() {
                    self.windows_absent_since = Some(now);
                }
            } else {
                self.seen_windows.extend(wins.iter().cloned());
                self.windows_absent_since = None;
            }
        }

        let speech = o.last_real_speech.filter(|t| *t > self.quiet_baseline);
        if speech.is_some() {
            self.heard_speech = true;
        }
        let quiet_since = speech.unwrap_or(self.quiet_baseline);
        let quiet = now - quiet_since;

        // A countdown is running: cancel on contrary evidence, else stop
        if let Some(p) = self.pending.clone() {
            if speech.map(|t| t > p.detected_at).unwrap_or(false) {
                self.pending = None;
                // The signal was wrong (people are still talking, e.g. an
                // in-room conversation after the call). Without this it would
                // re-fire on the very next quiet stretch.
                match p.kind {
                    SignalKind::MicReleased => self.forget_mic_evidence(),
                    SignalKind::WindowClosed => self.forget_window_evidence(),
                    _ => {}
                }
                return Action::Cancelled("speech resumed");
            }
            if p.kind == SignalKind::MicReleased && self.apps_absent_since.is_none() {
                self.pending = None;
                return Action::Cancelled("call app is using the microphone again");
            }
            if p.kind == SignalKind::WindowClosed && self.windows_absent_since.is_none() {
                self.pending = None;
                return Action::Cancelled("meeting window is back");
            }
            if now >= p.deadline {
                self.pending = None;
                self.done = true;
                return Action::AutoStop(p);
            }
            return Action::None;
        }

        let snoozed = self.snooze_until.map(|u| now < u).unwrap_or(false);

        if let Some(since) = self.apps_absent_since {
            if now - since >= self.cfg.mic_grace && quiet >= self.cfg.corroborating_quiet {
                let names = self.seen_apps.iter().cloned().collect::<Vec<_>>().join(", ");
                return self.fire(
                    now,
                    SignalKind::MicReleased,
                    format!("{} stopped using the microphone", names),
                    since,
                );
            }
        }

        if !snoozed {
            if let Some((end, title)) = self.calendar.clone() {
                if now >= end + self.cfg.calendar_grace && quiet >= self.cfg.corroborating_quiet {
                    let reason = if title.trim().is_empty() {
                        "the calendar event ended".to_string()
                    } else {
                        format!("\"{}\" was scheduled to end", title.trim())
                    };
                    return self.fire(now, SignalKind::CalendarEnded, reason, quiet_since.max(end.min(now)));
                }
            }
        }

        if let Some(since) = self.windows_absent_since {
            if now - since >= self.cfg.window_grace && quiet >= self.cfg.corroborating_quiet {
                let names = self.seen_windows.iter().cloned().collect::<Vec<_>>().join(", ");
                return self.fire(now, SignalKind::WindowClosed, format!("{} window closed", names), since);
            }
        }

        if !snoozed && o.transcription_active {
            let need = if self.heard_speech {
                self.cfg.silence
            } else {
                self.cfg.silence.max(self.cfg.silence_before_first_speech)
            };
            if quiet >= need {
                return self.fire(
                    now,
                    SignalKind::Silence,
                    format!("no one has spoken for {} min", need.num_minutes()),
                    quiet_since,
                );
            }
        }

        Action::None
    }
}

// ─── Runtime monitor ─────────────────────────────────────────────────────────

struct Monitor {
    meeting_id: String,
    detector: Mutex<Detector>,
}

static CURRENT: Lazy<Mutex<Option<Arc<Monitor>>>> = Lazy::new(|| Mutex::new(None));
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// (meeting id, end point) chosen when a detected end leads to a stop.
static TRIM_AFTER: Lazy<Mutex<Option<(String, DateTime<Utc>)>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, Serialize)]
pub struct DetectedPayload {
    pub meeting_id: String,
    pub kind: SignalKind,
    pub reason: String,
    /// Seconds left before the recording stops.
    pub countdown: i64,
    pub deadline: String,
}

impl DetectedPayload {
    fn new(meeting_id: &str, p: &Pending, now: DateTime<Utc>) -> Self {
        Self {
            meeting_id: meeting_id.to_string(),
            kind: p.kind,
            reason: p.reason.clone(),
            countdown: (p.deadline - now).num_seconds().max(0),
            deadline: p.deadline.to_rfc3339(),
        }
    }
}

async fn read_settings(app: &AppHandle) -> (bool, i64) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return (true, DEFAULT_SILENCE_MINUTES);
    };
    let enabled = state
        .settings
        .get(SETTING_ENABLED)
        .await
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);
    let minutes = state
        .settings
        .get(SETTING_SILENCE_MINUTES)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_SILENCE_MINUTES)
        .clamp(1, 60);
    (enabled, minutes)
}

/// Start watching the recording `meeting_id` for its end.
pub fn start_monitor(app: AppHandle, meeting_id: String, calendar: Option<(DateTime<Utc>, String)>) {
    reset_speech();
    *TRIM_AFTER.lock() = None;
    let my_gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let monitor = Arc::new(Monitor {
        meeting_id: meeting_id.clone(),
        detector: Mutex::new(Detector::new(DetectorConfig::default(), Utc::now(), calendar)),
    });
    *CURRENT.lock() = Some(monitor.clone());
    crate::tray_builder::set_auto_stop_status(&app, "Auto-stop: watching for meeting end", false);
    // The countdown notification needs permission: ask at the first
    // recording (macOS prompts only once), never at launch.
    crate::notifications::request_permission_once();

    tauri::async_runtime::spawn(async move {
        log::info!("🛑 Meeting-end monitor started for {}", meeting_id);
        let mut tick: u64 = 0;
        let (mut enabled, mut minutes) = read_settings(&app).await;
        monitor.detector.lock().set_silence_minutes(minutes);
        let mut was_enabled = enabled;
        let mut logged_unavailable = (false, false);

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if GENERATION.load(Ordering::SeqCst) != my_gen {
                break;
            }
            let Some(state) = app.try_state::<crate::AppState>() else { break };
            let (recording, paused) = {
                let engine = state.capture_engine.read();
                (engine.is_recording(), engine.is_paused())
            };
            let current = state.state_builder.read().current_meeting_id();
            if !recording || current.as_deref() != Some(meeting_id.as_str()) {
                break;
            }

            tick += 1;
            if tick % 5 == 0 {
                (enabled, minutes) = read_settings(&app).await;
                monitor.detector.lock().set_silence_minutes(minutes);
            }

            let now = Utc::now();
            if paused || !enabled {
                let had_pending = monitor.detector.lock().pending().is_some();
                monitor.detector.lock().reset(now);
                if had_pending {
                    log::info!("🛑 Meeting-end countdown cancelled (recording paused or auto-stop disabled)");
                    let _ = app.emit("meeting-end-cancelled", serde_json::json!({ "reason": "auto-stop paused" }));
                }
                if !enabled && was_enabled {
                    crate::tray_builder::set_auto_stop_status(&app, "Auto-stop: off", false);
                }
                was_enabled = enabled;
                continue;
            }
            if !was_enabled {
                crate::tray_builder::set_auto_stop_status(&app, "Auto-stop: watching for meeting end", false);
                was_enabled = true;
            }

            let (call_apps, meeting_windows) = tokio::task::spawn_blocking(|| {
                let src = SystemSignals::new();
                (src.call_apps(), src.meeting_windows())
            })
            .await
            .unwrap_or((None, None));
            if call_apps.is_none() && !logged_unavailable.0 {
                log::info!("🛑 Mic-release signal unavailable (needs macOS 14.2+ Core Audio process objects)");
                logged_unavailable.0 = true;
            }
            if meeting_windows.is_none() && !logged_unavailable.1 {
                log::info!("🛑 Meeting-window signal unavailable (no window titles; Screen Recording permission?)");
                logged_unavailable.1 = true;
            }

            let obs = Observation {
                now,
                call_apps,
                meeting_windows,
                last_real_speech: last_real_speech(),
                transcription_active: state.transcription_manager.is_active(),
            };
            let action = monitor.detector.lock().tick(&obs);
            match action {
                Action::None => {}
                Action::Detected(p) => {
                    log::info!(
                        "🛑 Meeting end detected (signal: {:?}); stopping in {}s unless kept",
                        p.kind,
                        (p.deadline - now).num_seconds()
                    );
                    let _ = app.emit("meeting-end-detected", DetectedPayload::new(&meeting_id, &p, now));
                    crate::tray_builder::set_auto_stop_status(
                        &app,
                        &format!("Ended? Stopping in {}s…", (p.deadline - now).num_seconds()),
                        true,
                    );
                    request_attention(&app, &p.reason, (p.deadline - now).num_seconds());
                }
                Action::Cancelled(why) => {
                    log::info!("🛑 Meeting-end countdown cancelled: {}", why);
                    let _ = app.emit("meeting-end-cancelled", serde_json::json!({ "reason": why }));
                    crate::tray_builder::set_auto_stop_status(&app, "Auto-stop: watching for meeting end", false);
                }
                Action::AutoStop(p) => {
                    log::info!("🛑 Auto-stopping recording {} (signal: {:?})", meeting_id, p.kind);
                    *TRIM_AFTER.lock() = Some((meeting_id.clone(), p.end_at));
                    let _ = app.emit(
                        "meeting-end-auto-stop",
                        serde_json::json!({
                            "meeting_id": meeting_id,
                            "kind": p.kind,
                            "reason": p.reason,
                            "end_at": p.end_at.to_rfc3339(),
                        }),
                    );
                    crate::tray_builder::set_auto_stop_status(&app, "Auto-stop: stopping recording…", false);
                    // The UI stops through the user's Stop path (video, AX
                    // unlink, notes). If it hasn't within a few seconds (window
                    // closed / webview asleep), stop from the backend.
                    let app2 = app.clone();
                    let mid = meeting_id.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
                        let Some(state) = app2.try_state::<crate::AppState>() else { return };
                        let still = state.capture_engine.read().is_recording()
                            && state.state_builder.read().current_meeting_id().as_deref() == Some(mid.as_str());
                        if still {
                            log::warn!("🛑 UI did not stop the recording; stopping from the backend");
                            if let Err(e) = crate::commands::stop_recording_from_backend(&app2).await {
                                log::error!("Backend auto-stop failed: {}", e);
                            }
                        }
                    });
                    break;
                }
            }
        }

        // Only the newest monitor owns the shared slot
        if GENERATION.load(Ordering::SeqCst) == my_gen {
            let mut cur = CURRENT.lock();
            if cur.as_ref().map(|m| m.meeting_id == meeting_id).unwrap_or(false) {
                *cur = None;
            }
        }
        log::info!("🛑 Meeting-end monitor stopped for {}", meeting_id);
    });
}

/// When the window isn't focused (hidden, minimized or another app in
/// front), bounce the Dock icon and post a notification with the reason;
/// clicking it brings the window (and the countdown banner) forward.
fn request_attention(app: &AppHandle, reason: &str, countdown_secs: i64) {
    let focused = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
        .unwrap_or(false);
    if focused {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
    }
    crate::notifications::notify_meeting_end(app, reason, countdown_secs);
}

/// Called by the stop path. Ends the monitor and returns the end point to
/// trim junk after, if this stop follows a detected meeting end (auto-stop,
/// or "Stop now" while the countdown was showing).
pub fn on_recording_stopped(meeting_id: Option<&str>) -> Option<DateTime<Utc>> {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let monitor = CURRENT.lock().take();
    let trim = TRIM_AFTER.lock().take();
    let mid = meeting_id?;
    if let Some((m, at)) = trim {
        if m == mid {
            return Some(at);
        }
    }
    monitor
        .filter(|m| m.meeting_id == mid)
        .and_then(|m| m.detector.lock().pending().map(|p| p.end_at))
}

/// Remove junk transcript lines (per the hallucination filter) that were
/// written after `end_at`. Real speech is never touched.
pub async fn trim_junk_after(db: &crate::database::DatabaseManager, meeting_id: &str, end_at: DateTime<Utc>) {
    match db
        .delete_transcripts_after_matching(meeting_id, end_at, crate::transcription::filter::is_junk)
        .await
    {
        Ok(0) => {}
        Ok(n) => log::info!("🧹 Trimmed {} junk transcript line(s) after the detected meeting end", n),
        Err(e) => log::warn!("Failed to trim post-meeting junk: {}", e),
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MeetingEndStatus {
    pub monitoring: bool,
    pub pending: Option<DetectedPayload>,
}

/// "Keep recording" from the banner or tray: cancel the countdown and
/// snooze detection.
pub fn keep_recording(app: &AppHandle) -> bool {
    let Some(m) = CURRENT.lock().clone() else { return false };
    let was_pending = m.detector.lock().snooze(Utc::now());
    log::info!("🛑 Meeting-end detection snoozed by user (countdown was running: {})", was_pending);
    let _ = app.emit("meeting-end-cancelled", serde_json::json!({ "reason": "kept recording" }));
    crate::tray_builder::set_auto_stop_status(app, "Auto-stop: snoozed 10 min", false);
    was_pending
}

#[tauri::command(rename_all = "camelCase")]
pub async fn meeting_end_keep_recording(app: AppHandle) -> Result<bool, String> {
    Ok(keep_recording(&app))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_end_status() -> Result<MeetingEndStatus, String> {
    let cur = CURRENT.lock().clone();
    let now = Utc::now();
    Ok(MeetingEndStatus {
        monitoring: cur.is_some(),
        pending: cur.and_then(|m| {
            let det = m.detector.lock();
            det.pending().map(|p| DetectedPayload::new(&m.meeting_id, p, now))
        }),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStopSettings {
    pub enabled: bool,
    pub silence_minutes: i64,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_auto_stop_settings(app: AppHandle) -> Result<AutoStopSettings, String> {
    let (enabled, silence_minutes) = read_settings(&app).await;
    Ok(AutoStopSettings { enabled, silence_minutes })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_auto_stop_settings(
    app: AppHandle,
    enabled: bool,
    silence_minutes: Option<i64>,
) -> Result<AutoStopSettings, String> {
    let state = app.try_state::<crate::AppState>().ok_or("App not ready")?;
    state
        .settings
        .set(SETTING_ENABLED, if enabled { "true" } else { "false" })
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;
    if let Some(m) = silence_minutes {
        state
            .settings
            .set(SETTING_SILENCE_MINUTES, &m.clamp(1, 60).to_string())
            .await
            .map_err(|e| format!("Failed to save setting: {}", e))?;
    }
    let (enabled, silence_minutes) = read_settings(&app).await;
    if let Some(m) = CURRENT.lock().clone() {
        m.detector.lock().set_silence_minutes(silence_minutes);
    }
    crate::tray_builder::set_auto_stop_checked(&app, enabled);
    let _ = app.emit("auto-stop-settings-changed", AutoStopSettings { enabled, silence_minutes });
    Ok(AutoStopSettings { enabled, silence_minutes })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-01T14:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn at(secs: i64) -> DateTime<Utc> {
        t0() + Duration::seconds(secs)
    }
    fn apps(names: &[&str]) -> Option<BTreeSet<String>> {
        Some(names.iter().map(|s| s.to_string()).collect())
    }
    fn obs(secs: i64, call: Option<BTreeSet<String>>, speech: Option<i64>) -> Observation {
        Observation {
            now: at(secs),
            call_apps: call,
            meeting_windows: None,
            last_real_speech: speech.map(at),
            transcription_active: true,
        }
    }

    /// Fake system for provider tests (no real Core Audio).
    struct FakeSignals {
        procs: Vec<AudioProcess>,
        own_pid: i32,
        windows: Vec<(String, String)>,
    }
    impl SignalSource for FakeSignals {
        fn call_apps(&self) -> Option<BTreeSet<String>> {
            Some(active_call_apps(&self.procs, self.own_pid))
        }
        fn meeting_windows(&self) -> Option<BTreeSet<String>> {
            Some(self.windows.iter().filter_map(|(o, t)| meeting_window_label(o, t)).collect())
        }
    }

    fn proc(pid: i32, bundle: &str, input: bool) -> AudioProcess {
        AudioProcess { pid, bundle_id: bundle.into(), running_input: input, running_output: true }
    }

    #[test]
    fn process_list_parsing() {
        let fake = FakeSignals {
            own_pid: 42,
            procs: vec![
                proc(42, "com.nofriction.meetings", true), // us
                proc(100, "us.zoom.xos", true),
                proc(101, "com.google.Chrome.helper", true),
                proc(102, "com.spotify.client", false),
                proc(103, "com.apple.Music", true), // not a call app
                proc(104, "com.microsoft.teams2", false), // running, mic off
                proc(105, "", true),
            ],
            windows: vec![
                ("zoom.us".into(), "Zoom Meeting".into()),
                ("Google Chrome".into(), "Meet - abc-defg-hij".into()),
                ("Finder".into(), "Downloads".into()),
            ],
        };
        let calls = fake.call_apps().unwrap();
        assert_eq!(calls, apps(&["Google Chrome", "Zoom"]).unwrap());
        let wins = fake.meeting_windows().unwrap();
        assert!(wins.contains("Zoom meeting") && wins.contains("Google Meet"));
        assert_eq!(wins.len(), 2);
    }

    #[test]
    fn call_app_names() {
        assert_eq!(call_app_name("us.zoom.xos"), Some("Zoom"));
        assert_eq!(call_app_name("com.microsoft.teams2"), Some("Microsoft Teams"));
        assert_eq!(call_app_name("Cisco-Systems.Spark"), Some("Webex"));
        assert_eq!(call_app_name("com.apple.FaceTime"), Some("FaceTime"));
        assert_eq!(call_app_name("com.nofriction.meetings"), None);
        assert_eq!(call_app_name("com.apple.Music"), None);
        assert_eq!(call_app_name(""), None);
    }

    #[test]
    fn window_labels() {
        assert_eq!(meeting_window_label("zoom.us", "Zoom Meeting").as_deref(), Some("Zoom meeting"));
        assert_eq!(meeting_window_label("zoom.us", "Zoom Workplace"), None);
        assert_eq!(
            meeting_window_label("Microsoft Teams", "Weekly sync | Meeting | Microsoft Teams").as_deref(),
            Some("Teams call")
        );
        assert_eq!(meeting_window_label("Slack", "Huddle in #eng").as_deref(), Some("Slack huddle"));
        assert_eq!(meeting_window_label("Arc", "Meet - xyz-abcd-efg").as_deref(), Some("Google Meet"));
        assert_eq!(meeting_window_label("Safari", "Inbox"), None);
        assert_eq!(meeting_window_label("zoom.us", ""), None);
    }

    #[test]
    fn mic_release_fires_after_grace_then_autostops() {
        let mut d = Detector::new(DetectorConfig::default(), t0(), None);
        assert_eq!(d.tick(&obs(0, apps(&["Zoom"]), Some(0))), Action::None);
        assert_eq!(d.tick(&obs(130, apps(&["Zoom"]), Some(120))), Action::None); // held 2+ min
        assert_eq!(d.tick(&obs(140, apps(&[]), Some(120))), Action::None); // released at 140
        assert_eq!(d.tick(&obs(150, apps(&[]), Some(120))), Action::None); // < 20s
        let a = d.tick(&obs(181, apps(&[]), Some(120)));
        let Action::Detected(p) = a else { panic!("expected detection, got {:?}", a) };
        assert_eq!(p.kind, SignalKind::MicReleased);
        assert_eq!(p.end_at, at(140));
        assert!(p.reason.contains("Zoom"));
        assert_eq!(d.tick(&obs(200, apps(&[]), Some(120))), Action::None); // counting down
        assert!(matches!(d.tick(&obs(212, apps(&[]), Some(120))), Action::AutoStop(_)));
        // Done: no further actions
        assert_eq!(d.tick(&obs(500, apps(&[]), Some(120))), Action::None);
    }

    #[test]
    fn mic_release_needs_corroborating_quiet() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        d.tick(&obs(0, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(130, apps(&["Zoom"]), Some(125)));
        d.tick(&obs(140, apps(&[]), Some(138)));
        // Released 21s+ ago, but someone spoke < 60s ago
        assert_eq!(d.tick(&obs(161, apps(&[]), Some(150))), Action::None);
        assert_eq!(d.tick(&obs(200, apps(&[]), Some(150))), Action::None);
        assert!(matches!(
            d.tick(&obs(211, apps(&[]), Some(150))),
            Action::Detected(Pending { kind: SignalKind::MicReleased, .. })
        ));
    }

    #[test]
    fn brief_browser_mic_use_never_arms_mic_release() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        // Dictation in Chrome for ~1 min, twice
        for s in (0..60).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&["Google Chrome"]), Some(0))), Action::None);
        }
        for s in (60..300).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&[]), Some(0))), Action::None);
        }
        for s in (300..400).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&["Google Chrome"]), Some(0))), Action::None);
        }
        for s in (400..900).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&[]), Some(0))), Action::None, "at {}", s);
        }
    }

    #[test]
    fn speech_cancel_clears_mic_evidence_so_it_does_not_refire() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        d.tick(&obs(0, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(130, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(140, apps(&[]), Some(0)));
        assert!(matches!(d.tick(&obs(161, apps(&[]), Some(0))), Action::Detected(_)));
        assert_eq!(d.tick(&obs(170, apps(&[]), Some(168))), Action::Cancelled("speech resumed"));
        // Quiet again for a long while, Zoom still off: no re-fire every tick
        for s in (172..1200).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&[]), Some(168))), Action::None, "at {}", s);
        }
        // Zoom genuinely used again, then released → fires again
        d.tick(&obs(1200, apps(&["Zoom"]), Some(168)));
        d.tick(&obs(1330, apps(&["Zoom"]), Some(168)));
        d.tick(&obs(1340, apps(&[]), Some(168)));
        assert!(matches!(d.tick(&obs(1361, apps(&[]), Some(168))), Action::Detected(_)));
    }

    #[test]
    fn mic_never_seen_does_not_fire() {
        let mut d = Detector::new(DetectorConfig::default(), t0(), None);
        for s in (0..170).step_by(2) {
            assert_eq!(d.tick(&obs(s, apps(&[]), Some(s))), Action::None);
        }
    }

    #[test]
    fn unavailable_signals_are_ignored() {
        // call_apps None (old macOS) never counts as "released"
        let mut d = Detector::new(DetectorConfig::default(), t0(), None);
        assert_eq!(d.tick(&obs(10, None, Some(9))), Action::None);
        assert_eq!(d.tick(&obs(100, None, Some(99))), Action::None);
    }

    #[test]
    fn countdown_cancelled_when_call_resumes_or_speech_returns() {
        let mut d = Detector::new(DetectorConfig::default(), t0(), None);
        d.tick(&obs(0, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(130, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(140, apps(&[]), Some(0)));
        assert!(matches!(d.tick(&obs(161, apps(&[]), Some(0))), Action::Detected(_)));
        assert_eq!(d.tick(&obs(165, apps(&["Zoom"]), Some(0))), Action::Cancelled("call app is using the microphone again"));
        assert!(d.pending().is_none());

        let mut d = Detector::new(DetectorConfig::default(), t0(), None);
        d.tick(&obs(0, apps(&["Teams"]), Some(0)));
        d.tick(&obs(130, apps(&["Teams"]), Some(0)));
        d.tick(&obs(140, apps(&[]), Some(0)));
        assert!(matches!(d.tick(&obs(161, apps(&[]), Some(0))), Action::Detected(_)));
        assert_eq!(d.tick(&obs(170, apps(&[]), Some(168))), Action::Cancelled("speech resumed"));
    }

    #[test]
    fn silence_fires_after_configured_minutes() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(3), t0(), None);
        d.tick(&obs(30, None, Some(30))); // someone spoke at 0:30
        assert_eq!(d.tick(&obs(30 + 179, None, Some(30))), Action::None);
        let a = d.tick(&obs(30 + 180, None, Some(30)));
        let Action::Detected(p) = a else { panic!("{:?}", a) };
        assert_eq!(p.kind, SignalKind::Silence);
        assert_eq!(p.end_at, at(30));
    }

    #[test]
    fn silence_needs_transcription_and_longer_before_first_speech() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(3), t0(), None);
        let mut o = obs(400, None, None);
        o.transcription_active = false;
        assert_eq!(d.tick(&o), Action::None); // can't judge silence without STT
        assert_eq!(d.tick(&obs(400, None, None)), Action::None); // nobody spoke yet: 10 min bar
        assert!(matches!(d.tick(&obs(600, None, None)), Action::Detected(_)));
    }

    #[test]
    fn calendar_needs_end_plus_grace_and_quiet() {
        let end = at(600);
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), Some((end, "Weekly".into())));
        assert_eq!(d.tick(&obs(700, None, Some(690))), Action::None); // before end+2m
        assert_eq!(d.tick(&obs(740, None, Some(735))), Action::None); // talking still
        let a = d.tick(&obs(800, None, Some(735)));
        let Action::Detected(p) = a else { panic!("{:?}", a) };
        assert_eq!(p.kind, SignalKind::CalendarEnded);
        assert!(p.reason.contains("Weekly"));
    }

    #[test]
    fn window_closed_needs_corroborating_quiet() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        let win = |secs, w: &[&str], speech| Observation {
            meeting_windows: apps(w),
            ..obs(secs, None, Some(speech))
        };
        d.tick(&win(0, &["Zoom meeting"], 0));
        assert_eq!(d.tick(&win(10, &[], 9)), Action::None);
        assert_eq!(d.tick(&win(40, &[], 39)), Action::None); // gone 30s but talking
        let a = d.tick(&win(100, &[], 39));
        assert!(matches!(a, Action::Detected(Pending { kind: SignalKind::WindowClosed, .. })), "{:?}", a);
        // Window back → cancel
        assert_eq!(d.tick(&win(105, &["Zoom meeting"], 39)), Action::Cancelled("meeting window is back"));
    }

    #[test]
    fn speech_cancel_clears_window_evidence_so_it_does_not_refire() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        let win = |secs, w: &[&str], speech| Observation {
            meeting_windows: apps(w),
            ..obs(secs, None, Some(speech))
        };
        d.tick(&win(0, &["Zoom meeting"], 0));
        d.tick(&win(10, &[], 0)); // window closed at 10
        let a = d.tick(&win(70, &[], 0));
        assert!(matches!(a, Action::Detected(Pending { kind: SignalKind::WindowClosed, .. })), "{:?}", a);
        // People keep talking in the room after the call
        assert_eq!(d.tick(&win(80, &[], 78)), Action::Cancelled("speech resumed"));
        // Quiet again for a long while, window still gone: no repeated banners
        for s in (82..1200).step_by(2) {
            assert_eq!(d.tick(&win(s, &[], 78)), Action::None, "at {}", s);
        }
        // A meeting window genuinely opens again, then closes → fires again
        d.tick(&win(1200, &["Google Meet"], 78));
        d.tick(&win(1210, &[], 78));
        assert!(matches!(
            d.tick(&win(1240, &[], 78)),
            Action::Detected(Pending { kind: SignalKind::WindowClosed, .. })
        ));
    }

    #[test]
    fn snooze_suppresses_silence_but_not_new_mic_release() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(3), t0(), None);
        d.tick(&obs(0, apps(&[]), Some(1)));
        assert!(matches!(d.tick(&obs(200, apps(&[]), Some(1))), Action::Detected(_)));
        assert!(d.snooze(at(205)));
        assert!(d.pending().is_none());
        // Still silent 5 minutes later: snoozed (10 min)
        assert_eq!(d.tick(&obs(205 + 300, apps(&[]), Some(1))), Action::None);
        // A NEW signal (call app seen then released) fires during the snooze
        d.tick(&obs(510, apps(&["Zoom"]), Some(1)));
        d.tick(&obs(640, apps(&["Zoom"]), Some(1)));
        d.tick(&obs(650, apps(&[]), Some(1)));
        assert!(matches!(
            d.tick(&obs(671, apps(&[]), Some(1))),
            Action::Detected(Pending { kind: SignalKind::MicReleased, .. })
        ));
    }

    #[test]
    fn snooze_expires_and_silence_rearms_from_snooze_time() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(3), t0(), None);
        d.tick(&obs(0, None, Some(1)));
        assert!(matches!(d.tick(&obs(200, None, Some(1))), Action::Detected(_)));
        d.snooze(at(200));
        assert_eq!(d.tick(&obs(799, None, Some(1))), Action::None);
        // After 10 min, silence counted from the snooze → fires (≥3 min)
        assert!(matches!(d.tick(&obs(801, None, Some(1))), Action::Detected(_)));
    }

    #[test]
    fn reset_clears_pending_and_evidence() {
        let mut d = Detector::new(DetectorConfig::with_silence_minutes(30), t0(), None);
        d.tick(&obs(0, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(130, apps(&["Zoom"]), Some(0)));
        d.tick(&obs(140, apps(&[]), Some(0)));
        assert!(matches!(d.tick(&obs(161, apps(&[]), Some(0))), Action::Detected(_)));
        d.reset(at(162));
        assert!(d.pending().is_none());
        // Zoom must be seen again before a release counts
        assert_eq!(d.tick(&obs(300, apps(&[]), Some(0))), Action::None);
    }

    #[test]
    fn speech_clock_tracks_latest() {
        reset_speech();
        note_real_speech(at(10));
        note_real_speech(at(5));
        assert_eq!(last_real_speech(), Some(at(10)));
        note_filtered_segment();
        reset_speech();
        assert_eq!(last_real_speech(), None);
    }
}
