//! Timed recording: "how long?" (15 / 30 / 60 / 90 min, or no limit) and the
//! backend timer that stops the recording at the deadline. See
//! docs/TIMED_RECORDING_AND_NOTEBOOKS.md.
//!
//! - Every start arms a plan, whichever path started it. A click on a Record
//!   button passes the picker's choice; the menu shortcut, the tray and the
//!   command palette pass nothing and get the remembered choice
//!   (`recording_default_duration`, "none" until the user picks one), and
//!   the remembered type (recording_kind.rs).
//! - The deadline is wall-clock time from the start (pauses don't move it:
//!   a class ends when it ends). Its length is stored on the meeting
//!   (`meetings.planned_minutes`).
//! - The timer runs here, so it works with the window closed. At the
//!   deadline it asks the UI to stop through the user's own Stop path and,
//!   if the UI hasn't within a few seconds, stops from the backend
//!   (`stop_recording_from_backend`). Both end in `stop_recording_core`, so
//!   `end_meeting` runs and the report is written.
//! - A plan belongs to one meeting and one generation. A stop clears it, a
//!   new start replaces it, and a timer task whose generation is gone exits:
//!   it can never stop a different recording.
//! - 5 minutes before the end (2 for a 15-minute plan) the UI gets a
//!   warning with +15 min / No limit, and a notification is posted when the
//!   window isn't in front. macOS notifications from the plugin have no
//!   action buttons, so the choices are in the app (banner, capture bar)
//!   and in the tray menu.
//!
//! Nothing here touches transcript text.

use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Setting key for the remembered choice ("15", "30", "60", "90", "none").
pub const SETTING_DEFAULT_DURATION: &str = "recording_default_duration";
/// The picker's choices, in order (keys 1–5). `None` is "No limit".
pub const CHOICES: [Limit; 5] = [
    Limit::Minutes(15),
    Limit::Minutes(30),
    Limit::Minutes(60),
    Limit::Minutes(90),
    Limit::NoLimit,
];
/// "+15 min"
pub const EXTEND_MINUTES: i64 = 15;
/// Longest plan accepted (a carried-over segment can be any length up to this).
pub const MAX_MINUTES: i64 = 12 * 60;
/// How long the UI gets to stop through its own Stop path before the backend does.
const UI_STOP_GRACE_SECS: u64 = 5;

// ─── Limit ───────────────────────────────────────────────────────────────────

/// A recording length: some minutes, or no limit. Wire/setting format: "60" / "none".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Minutes(i64),
    NoLimit,
}

impl Limit {
    /// "15" → 15 min, "none" → no limit. Anything else (0, negative, over
    /// `MAX_MINUTES`, not a number) is `None`.
    pub fn parse(s: &str) -> Option<Limit> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") {
            return Some(Limit::NoLimit);
        }
        match s.parse::<i64>() {
            Ok(m) if (1..=MAX_MINUTES).contains(&m) => Some(Limit::Minutes(m)),
            _ => None,
        }
    }

    pub fn as_setting(&self) -> String {
        match self {
            Limit::Minutes(m) => m.to_string(),
            Limit::NoLimit => "none".to_string(),
        }
    }

    pub fn minutes(&self) -> Option<i64> {
        match self {
            Limit::Minutes(m) => Some(*m),
            Limit::NoLimit => None,
        }
    }

    /// One of the five picker choices (only those are remembered).
    pub fn is_choice(&self) -> bool {
        CHOICES.contains(self)
    }

    /// "60 min" / "No limit" (tray labels)
    pub fn label(&self) -> String {
        match self {
            Limit::Minutes(m) => format!("{} min", m),
            Limit::NoLimit => "No limit".to_string(),
        }
    }
}

/// The remembered choice from its stored value. Missing or unreadable → no
/// limit, so a start that skips the picker never cuts a recording short
/// unless the user chose a length before.
pub fn remembered_from(stored: Option<&str>) -> Limit {
    stored
        .and_then(Limit::parse)
        .filter(|l| l.is_choice())
        .unwrap_or(Limit::NoLimit)
}

/// Warn this long before the end: 2 minutes for plans of 15 minutes or
/// less, 5 minutes otherwise.
pub fn warn_lead(planned_minutes: i64) -> Duration {
    if planned_minutes <= 15 {
        Duration::minutes(2)
    } else {
        Duration::minutes(5)
    }
}

// ─── Plan (pure) ─────────────────────────────────────────────────────────────

/// The time plan of the recording in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub meeting_id: String,
    pub generation: u64,
    pub started_at: DateTime<Utc>,
    /// None: no limit
    pub deadline: Option<DateTime<Utc>>,
    /// The warning for the current deadline was given
    pub warned: bool,
    /// The deadline passed and a stop was requested
    pub stopping: bool,
}

impl Plan {
    /// Planned length in whole minutes (rounded up), None without a limit.
    pub fn planned_minutes(&self) -> Option<i64> {
        self.deadline.map(|d| {
            let secs = (d - self.started_at).num_seconds().max(0);
            (secs + 59) / 60
        })
    }

    pub fn remaining_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        self.deadline.map(|d| (d - now).num_seconds().max(0))
    }

    fn lead(&self) -> Duration {
        warn_lead(self.planned_minutes().unwrap_or(i64::MAX))
    }
}

/// What the timer should do on this tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Tick {
    /// This timer's plan is gone (stopped, replaced): exit, do nothing.
    Stale,
    /// No limit (any more): nothing to do.
    Idle,
    /// Keep counting.
    Running,
    /// Time to warn: this many seconds are left.
    Warn { seconds_left: i64 },
    /// The deadline passed: stop the recording (returned once).
    Stop,
}

/// Status for the UI (capture bar, banner, tray).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub meeting_id: String,
    pub started_at: String,
    pub planned_minutes: Option<i64>,
    pub deadline: Option<String>,
    pub remaining_seconds: Option<i64>,
    pub warned: bool,
}

/// The one plan that can exist at a time, plus the generation counter that
/// keys timers to it. Pure (time is passed in) so it's unit-tested.
#[derive(Debug, Default)]
pub struct Registry {
    current: Option<Plan>,
    generation: u64,
}

impl Registry {
    /// A recording started: replace any plan with this one. Returns the
    /// generation the new timer must present on every tick.
    pub fn arm(&mut self, meeting_id: &str, now: DateTime<Utc>, limit: Limit) -> u64 {
        self.generation += 1;
        let deadline = limit.minutes().map(|m| now + Duration::minutes(m));
        self.current = Some(Plan {
            meeting_id: meeting_id.to_string(),
            generation: self.generation,
            started_at: now,
            deadline,
            warned: false,
            stopping: false,
        });
        self.generation
    }

    /// The recording stopped (any path): forget the plan. Its timer sees
    /// `Stale` on its next tick.
    pub fn disarm(&mut self) -> Option<Plan> {
        self.generation += 1;
        self.current.take()
    }

    /// Forget the plan only if it is still `generation` (a timer noticed its
    /// recording is gone).
    pub fn disarm_generation(&mut self, generation: u64) -> bool {
        if self.current.as_ref().map(|p| p.generation) == Some(generation) {
            self.disarm();
            true
        } else {
            false
        }
    }

    pub fn current(&self) -> Option<&Plan> {
        self.current.as_ref()
    }

    pub fn tick(&mut self, generation: u64, now: DateTime<Utc>) -> Tick {
        let Some(plan) = self.current.as_mut().filter(|p| p.generation == generation) else {
            return Tick::Stale;
        };
        let Some(deadline) = plan.deadline else { return Tick::Idle };
        if plan.stopping {
            return Tick::Running;
        }
        if now >= deadline {
            plan.stopping = true;
            return Tick::Stop;
        }
        if !plan.warned && now >= deadline - plan.lead() {
            plan.warned = true;
            return Tick::Warn { seconds_left: (deadline - now).num_seconds().max(0) };
        }
        Tick::Running
    }

    /// The deadline passed and the stop wasn't overtaken by an extension or
    /// a real stop: the backend fallback may stop `generation`'s recording.
    pub fn should_force_stop(&self, generation: u64) -> bool {
        self.current.as_ref().map(|p| p.generation == generation && p.stopping).unwrap_or(false)
    }

    fn plan_for(&mut self, meeting_id: Option<&str>) -> Result<&mut Plan, String> {
        let plan = self.current.as_mut().ok_or("Nothing is being recorded.")?;
        if let Some(id) = meeting_id {
            if plan.meeting_id != id {
                return Err("That recording has already stopped.".to_string());
            }
        }
        Ok(plan)
    }

    /// "+15 min": move the deadline (from now, if it already passed) and
    /// re-arm the warning. `meeting_id` (from the UI) must match the plan, so
    /// a stale click can't extend a newer recording.
    pub fn extend(&mut self, meeting_id: Option<&str>, now: DateTime<Utc>, minutes: i64) -> Result<Status, String> {
        let plan = self.plan_for(meeting_id)?;
        let deadline = plan.deadline.ok_or("This recording has no time limit.")?;
        let max_end = plan.started_at + Duration::minutes(MAX_MINUTES);
        let new_deadline = (deadline.max(now) + Duration::minutes(minutes.clamp(1, MAX_MINUTES))).min(max_end);
        if new_deadline <= deadline {
            return Err(format!("A recording can be at most {} hours long.", MAX_MINUTES / 60));
        }
        plan.deadline = Some(new_deadline);
        plan.stopping = false;
        plan.warned = new_deadline - now <= plan.lead();
        Ok(self.status(now).expect("plan exists"))
    }

    /// "No limit": keep recording until the user stops.
    pub fn remove_limit(&mut self, meeting_id: Option<&str>, now: DateTime<Utc>) -> Result<Status, String> {
        let plan = self.plan_for(meeting_id)?;
        plan.deadline = None;
        plan.warned = false;
        plan.stopping = false;
        Ok(self.status(now).expect("plan exists"))
    }

    pub fn status(&self, now: DateTime<Utc>) -> Option<Status> {
        self.current.as_ref().map(|p| Status {
            meeting_id: p.meeting_id.clone(),
            started_at: p.started_at.to_rfc3339(),
            planned_minutes: p.planned_minutes(),
            deadline: p.deadline.map(|d| d.to_rfc3339()),
            remaining_seconds: p.remaining_seconds(now),
            warned: p.warned,
        })
    }
}

/// Warning notification: ("5 minutes left in this recording", "It stops at 10:45. …").
pub fn warning_copy(seconds_left: i64, stops_at_local: &str) -> (String, String) {
    let minutes = ((seconds_left + 59) / 60).max(1);
    let title = if minutes == 1 {
        "1 minute left in this recording".to_string()
    } else {
        format!("{} minutes left in this recording", minutes)
    };
    let body = format!(
        "It stops at {}. Click to add 15 minutes or remove the limit.",
        stops_at_local
    );
    (title, body)
}

// ─── Runtime ─────────────────────────────────────────────────────────────────

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));
/// For the stop path, which has no handle of its own.
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// The remembered choice (Settings), for starts that skip the picker.
pub async fn remembered(app: &AppHandle) -> Limit {
    let Some(state) = app.try_state::<crate::AppState>() else { return Limit::NoLimit };
    let stored = state.settings.get(SETTING_DEFAULT_DURATION).await.ok().flatten();
    remembered_from(stored.as_deref())
}

/// Remember a picker (or tray submenu) choice and show it on the tray.
pub async fn remember(app: &AppHandle, limit: Limit) {
    if !limit.is_choice() {
        return;
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        if let Err(e) = state.settings.set(SETTING_DEFAULT_DURATION, &limit.as_setting()).await {
            log::warn!("Could not save the recording length: {}", e);
        }
    }
    refresh_start_label(app).await;
}

/// "Meeting, 60 min": what a tray / shortcut start records (remembered type
/// and length).
pub fn start_label(kind: crate::recording_kind::RecordingKind, limit: Limit) -> String {
    format!("{}, {}", kind.label(), limit.label())
}

/// Show the remembered type and length on the tray's Start Recording item.
pub async fn refresh_start_label(app: &AppHandle) {
    let limit = remembered(app).await;
    let kind = match app.try_state::<crate::AppState>() {
        Some(state) => crate::recording_kind::remembered(&state.settings).await,
        None => Default::default(),
    };
    crate::tray_builder::set_start_recording_label(app, &start_label(kind, limit));
}

/// Tray label at launch.
pub async fn refresh_tray(app: &AppHandle) {
    refresh_start_label(app).await;
    publish(app);
}

fn is_recording_meeting(app: &AppHandle, meeting_id: &str) -> Option<bool> {
    let state = app.try_state::<crate::AppState>()?;
    let recording = state.capture_engine.read().is_recording();
    let current = state.state_builder.read().current_meeting_id();
    Some(recording && current.as_deref() == Some(meeting_id))
}

/// Tell the UI where the plan stands.
fn publish(app: &AppHandle) {
    let status = REGISTRY.lock().status(Utc::now());
    let _ = app.emit("timed-recording-changed", status);
}

/// A recording started (called by `start_recording` once capture runs).
pub fn arm(app: &AppHandle, meeting_id: &str, limit: Limit) {
    let _ = APP.set(app.clone());
    let generation = REGISTRY.lock().arm(meeting_id, Utc::now(), limit);
    log::info!("⏱️ Recording {} planned: {}", meeting_id, limit.label());
    publish(app);
    if limit.minutes().is_none() {
        return;
    }
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut last_tray_minute: Option<i64> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            match is_recording_meeting(&app, &meeting_id) {
                None => break,
                Some(false) => {
                    // Gone without the stop path seeing it: drop our plan only
                    if REGISTRY.lock().disarm_generation(generation) {
                        publish(&app);
                    }
                    break;
                }
                Some(true) => {}
            }
            let now = Utc::now();
            let tick = REGISTRY.lock().tick(generation, now);
            match tick {
                Tick::Stale | Tick::Idle => break,
                Tick::Running => {}
                Tick::Warn { seconds_left } => warn(&app, seconds_left),
                Tick::Stop => {
                    request_stop(&app, generation, &meeting_id);
                    continue;
                }
            }
            // Tray line once a minute
            let minute = REGISTRY.lock().status(now).and_then(|s| s.remaining_seconds).map(|s| (s + 59) / 60);
            if minute != last_tray_minute {
                last_tray_minute = minute;
                publish(&app);
            }
        }
        log::info!("⏱️ Timer ended for {}", meeting_id);
    });
}

/// The recording stopped (called by `stop_recording_core`, every stop path).
pub fn on_recording_stopped() {
    let had = REGISTRY.lock().disarm().is_some();
    if had {
        if let Some(app) = APP.get() {
            publish(app);
        }
    }
}

fn warn(app: &AppHandle, seconds_left: i64) {
    let status = REGISTRY.lock().status(Utc::now());
    let Some(status) = status else { return };
    log::info!("⏱️ {}s left in recording {}", seconds_left, status.meeting_id);
    let _ = app.emit("timed-recording-warning", &status);
    publish(app);
    // In front: the in-app banner is enough. Otherwise bounce + notify;
    // clicking the notification brings the window (and the banner) up.
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
    let stops_at = status
        .deadline
        .as_deref()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&chrono::Local).format("%-I:%M %p").to_string())
        .unwrap_or_else(|| "the time you chose".to_string());
    let (title, body) = warning_copy(seconds_left, &stops_at);
    if crate::notifications::post(app, &title, &body) {
        log::info!("🔔 Posted time-limit warning");
    }
}

/// Deadline reached: the UI stops through the user's Stop path; if it
/// hasn't within a few seconds, the backend stops (same core path).
fn request_stop(app: &AppHandle, generation: u64, meeting_id: &str) {
    let planned = REGISTRY.lock().current().and_then(|p| p.planned_minutes());
    log::info!("⏱️ Time limit reached for {}; stopping", meeting_id);
    let _ = app.emit(
        "timed-recording-auto-stop",
        serde_json::json!({ "meetingId": meeting_id, "plannedMinutes": planned }),
    );
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(UI_STOP_GRACE_SECS)).await;
        let still_ours = REGISTRY.lock().should_force_stop(generation);
        if still_ours && is_recording_meeting(&app, &meeting_id) == Some(true) {
            log::warn!("⏱️ UI did not stop the timed recording; stopping from the backend");
            if let Err(e) = crate::commands::stop_recording_from_backend(&app).await {
                log::error!("Timed auto-stop failed: {}", e);
            }
        }
    });
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// Picker defaults: the remembered type and length, and recent notebooks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPrefs {
    /// "meeting" | "class" | "personal" ("meeting" until one is picked)
    pub default_kind: String,
    /// "15" | "30" | "60" | "90" | "none"
    pub default_duration: String,
    /// Most recent first; derived from saved recordings (deleting a
    /// recording removes its notebook from this list once no other
    /// recording has it).
    pub recent_notebooks: Vec<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_record_prefs(app: AppHandle) -> Result<RecordPrefs, String> {
    let state = app.try_state::<crate::AppState>().ok_or("App not ready")?;
    let recent_notebooks = state
        .database
        .recent_notebooks(crate::notebooks::RECENT_LIMIT)
        .await
        .map_err(|e| format!("Failed to list notebooks: {}", e))?;
    Ok(RecordPrefs {
        default_kind: crate::recording_kind::remembered(&state.settings).await.as_str().to_string(),
        default_duration: remembered(&app).await.as_setting(),
        recent_notebooks,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_timed_recording_status() -> Result<Option<Status>, String> {
    Ok(REGISTRY.lock().status(Utc::now()))
}

/// "+15 min" (UI passes the meeting it shows; the tray passes none).
#[tauri::command(rename_all = "camelCase")]
pub async fn extend_timed_recording(app: AppHandle, meeting_id: Option<String>) -> Result<Status, String> {
    extend(&app, meeting_id.as_deref())
}

/// "No limit"
#[tauri::command(rename_all = "camelCase")]
pub async fn remove_timed_recording_limit(app: AppHandle, meeting_id: Option<String>) -> Result<Status, String> {
    remove_limit(&app, meeting_id.as_deref())
}

pub fn extend(app: &AppHandle, meeting_id: Option<&str>) -> Result<Status, String> {
    let status = REGISTRY.lock().extend(meeting_id, Utc::now(), EXTEND_MINUTES)?;
    log::info!("⏱️ Recording {} extended to {:?} min", status.meeting_id, status.planned_minutes);
    store_planned_minutes(app, &status);
    publish(app);
    Ok(status)
}

pub fn remove_limit(app: &AppHandle, meeting_id: Option<&str>) -> Result<Status, String> {
    let status = REGISTRY.lock().remove_limit(meeting_id, Utc::now())?;
    log::info!("⏱️ Recording {}: time limit removed", status.meeting_id);
    store_planned_minutes(app, &status);
    publish(app);
    Ok(status)
}

/// Keep `meetings.planned_minutes` in step with the plan.
fn store_planned_minutes(app: &AppHandle, status: &Status) {
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let db = state.database.clone();
    let id = status.meeting_id.clone();
    let minutes = status.planned_minutes;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = db.set_meeting_planned_minutes(&id, minutes).await {
            log::warn!("Could not save the planned length: {}", e);
        }
    });
}

/// What `start_recording` receives from the UI. All optional: a start with
/// no plan (menu shortcut, tray, command palette) uses the remembered type
/// and length.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPlan {
    /// "15" | "30" | "60" | "90" | "none", or any whole minutes up to 12 h
    /// (a carried-over segment). Missing: the remembered choice.
    pub duration: Option<String>,
    /// "meeting" | "class" | "personal". Missing: the remembered type.
    pub recording_kind: Option<String>,
    /// The optional notebook (stored in `meetings.class_name`)
    #[serde(alias = "className")]
    pub notebook: Option<String>,
    /// Save `duration` (and `recording_kind`, when given) as the remembered
    /// choice (picker, tray submenu).
    #[serde(default)]
    pub remember: bool,
}

/// The limit a start should use.
pub fn resolve_limit(plan: Option<&StartPlan>, remembered: Limit) -> Result<Limit, String> {
    match plan.and_then(|p| p.duration.as_deref()) {
        None => Ok(remembered),
        Some(s) => Limit::parse(s).ok_or_else(|| format!("Unknown recording length: {}", s)),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-06T10:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn at(secs: i64) -> DateTime<Utc> {
        t0() + Duration::seconds(secs)
    }
    fn min(m: i64) -> i64 {
        m * 60
    }

    #[test]
    fn limit_parsing_and_remembered_default() {
        assert_eq!(Limit::parse("15"), Some(Limit::Minutes(15)));
        assert_eq!(Limit::parse(" none "), Some(Limit::NoLimit));
        assert_eq!(Limit::parse("NONE"), Some(Limit::NoLimit));
        assert_eq!(Limit::parse("13"), Some(Limit::Minutes(13)), "carried-over segments");
        for bad in ["0", "-5", "721", "abc", "", "1.5"] {
            assert_eq!(Limit::parse(bad), None, "{:?}", bad);
        }
        assert_eq!(remembered_from(None), Limit::NoLimit, "never chosen: no limit");
        assert_eq!(remembered_from(Some("60")), Limit::Minutes(60));
        assert_eq!(remembered_from(Some("none")), Limit::NoLimit);
        assert_eq!(remembered_from(Some("13")), Limit::NoLimit, "only picker choices are remembered");
        assert_eq!(remembered_from(Some("junk")), Limit::NoLimit);
        assert!(Limit::Minutes(90).is_choice() && !Limit::Minutes(45).is_choice());
        assert_eq!(Limit::Minutes(60).as_setting(), "60");
        assert_eq!(Limit::NoLimit.as_setting(), "none");
        assert_eq!(Limit::NoLimit.label(), "No limit");
    }

    #[test]
    fn start_plan_resolution() {
        let remembered = Limit::Minutes(30);
        assert_eq!(resolve_limit(None, remembered), Ok(Limit::Minutes(30)), "hotkey / tray / palette");
        let p = StartPlan { notebook: Some("BIO 101".into()), ..Default::default() };
        assert_eq!(resolve_limit(Some(&p), remembered), Ok(Limit::Minutes(30)));
        let p = StartPlan { duration: Some("none".into()), ..Default::default() };
        assert_eq!(resolve_limit(Some(&p), remembered), Ok(Limit::NoLimit));
        let p = StartPlan { duration: Some("90".into()), ..Default::default() };
        assert_eq!(resolve_limit(Some(&p), remembered), Ok(Limit::Minutes(90)));
        let p = StartPlan { duration: Some("forever".into()), ..Default::default() };
        assert!(resolve_limit(Some(&p), remembered).is_err());
        // The UI's JSON shape
        let p: StartPlan =
            serde_json::from_str(r#"{"duration":"60","recordingKind":"class","notebook":"BIO 101","remember":true}"#).unwrap();
        assert_eq!(
            (p.duration.as_deref(), p.recording_kind.as_deref(), p.notebook.as_deref(), p.remember),
            (Some("60"), Some("class"), Some("BIO 101"), true)
        );
        // An older UI's field name still reads as the notebook
        let p: StartPlan = serde_json::from_str(r#"{"className":"BIO 101"}"#).unwrap();
        assert_eq!(p.notebook.as_deref(), Some("BIO 101"));
        let p: StartPlan = serde_json::from_str("{}").unwrap();
        assert!(p.duration.is_none() && p.recording_kind.is_none() && !p.remember);
        // The tray item shows what a shortcut start records
        use crate::recording_kind::RecordingKind;
        assert_eq!(start_label(RecordingKind::Meeting, Limit::Minutes(60)), "Meeting, 60 min");
        assert_eq!(start_label(RecordingKind::Class, Limit::NoLimit), "Class, No limit");
    }

    #[test]
    fn warns_once_then_stops_at_the_deadline() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(60));
        assert_eq!(r.current().unwrap().planned_minutes(), Some(60));
        assert_eq!(r.tick(g, at(min(54))), Tick::Running);
        assert_eq!(r.tick(g, at(min(55))), Tick::Warn { seconds_left: 300 }, "5 min before the end");
        assert_eq!(r.tick(g, at(min(55) + 1)), Tick::Running, "warned once");
        assert_eq!(r.tick(g, at(min(60) - 1)), Tick::Running);
        assert_eq!(r.tick(g, at(min(60))), Tick::Stop);
        assert!(r.should_force_stop(g));
        assert_eq!(r.tick(g, at(min(60) + 1)), Tick::Running, "Stop is returned once");
    }

    #[test]
    fn fifteen_minute_plans_warn_two_minutes_ahead() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(15));
        assert_eq!(r.tick(g, at(min(12) + 59)), Tick::Running);
        assert_eq!(r.tick(g, at(min(13))), Tick::Warn { seconds_left: 120 });
        assert_eq!(warn_lead(15), Duration::minutes(2));
        assert_eq!(warn_lead(30), Duration::minutes(5));
    }

    #[test]
    fn no_limit_never_stops() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::NoLimit);
        assert_eq!(r.tick(g, at(min(600))), Tick::Idle);
        assert_eq!(r.current().unwrap().planned_minutes(), None);
        assert!(r.extend(None, at(10), 15).is_err(), "nothing to extend");
    }

    #[test]
    fn timer_never_stops_another_meeting() {
        let mut r = Registry::default();
        let g1 = r.arm("m1", t0(), Limit::Minutes(15));
        // m1 is stopped by hand, then m2 starts with its own plan
        assert_eq!(r.disarm().map(|p| p.meeting_id), Some("m1".to_string()));
        let g2 = r.arm("m2", at(min(1)), Limit::Minutes(60));
        assert_ne!(g1, g2);
        // m1's timer wakes at m1's old deadline: its plan is gone
        assert_eq!(r.tick(g1, at(min(15))), Tick::Stale);
        assert_eq!(r.tick(g1, at(min(500))), Tick::Stale);
        assert!(!r.should_force_stop(g1));
        // m2 is untouched
        assert_eq!(r.tick(g2, at(min(15))), Tick::Running);
        assert_eq!(r.current().unwrap().meeting_id, "m2");
        // A stale timer can't drop the new plan either
        assert!(!r.disarm_generation(g1));
        assert!(r.current().is_some());
        // A new start replaces a plan even without a stop in between
        let g3 = r.arm("m3", at(min(2)), Limit::Minutes(30));
        assert_eq!(r.tick(g2, at(min(61))), Tick::Stale);
        assert_eq!(r.tick(g3, at(min(3))), Tick::Running);
    }

    #[test]
    fn manual_stop_cancels_the_timer() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(30));
        r.disarm();
        assert_eq!(r.tick(g, at(min(30))), Tick::Stale);
        assert!(!r.should_force_stop(g));
        assert!(r.status(at(1)).is_none());
        assert!(r.extend(None, at(1), 15).is_err());
    }

    #[test]
    fn extend_moves_the_deadline_and_rearms_the_warning() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(30));
        assert_eq!(r.tick(g, at(min(25))), Tick::Warn { seconds_left: 300 });
        let s = r.extend(Some("m1"), at(min(26)), 15).unwrap();
        assert_eq!(s.planned_minutes, Some(45));
        assert_eq!(s.remaining_seconds, Some(min(19)));
        assert!(!s.warned);
        assert_eq!(r.tick(g, at(min(30))), Tick::Running, "old deadline no longer stops");
        assert_eq!(r.tick(g, at(min(40))), Tick::Warn { seconds_left: 300 }, "warned again before the new end");
        assert_eq!(r.tick(g, at(min(45))), Tick::Stop);
    }

    #[test]
    fn extend_after_the_deadline_counts_from_now_and_cancels_the_forced_stop() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(15));
        assert_eq!(r.tick(g, at(min(15))), Tick::Stop);
        assert!(r.should_force_stop(g));
        // "+15" arrives 3 s later, inside the UI grace period
        let s = r.extend(None, at(min(15) + 3), 15).unwrap();
        assert_eq!(s.remaining_seconds, Some(min(15)));
        assert!(!r.should_force_stop(g), "the backend fallback must not stop it now");
        assert_eq!(r.tick(g, at(min(16))), Tick::Running);
    }

    #[test]
    fn extend_and_remove_need_the_right_meeting() {
        let mut r = Registry::default();
        r.arm("m2", t0(), Limit::Minutes(30));
        assert!(r.extend(Some("m1"), at(1), 15).is_err(), "a stale click for m1");
        assert!(r.remove_limit(Some("m1"), at(1)).is_err());
        assert_eq!(r.current().unwrap().planned_minutes(), Some(30));
    }

    #[test]
    fn extend_is_capped() {
        let mut r = Registry::default();
        let _ = r.arm("m1", t0(), Limit::Minutes(MAX_MINUTES - 5));
        let s = r.extend(None, at(1), 15).unwrap();
        assert_eq!(s.planned_minutes, Some(MAX_MINUTES));
        assert!(r.extend(None, at(2), 15).is_err());
    }

    #[test]
    fn remove_limit_stops_the_countdown() {
        let mut r = Registry::default();
        let g = r.arm("m1", t0(), Limit::Minutes(15));
        assert_eq!(r.tick(g, at(min(15))), Tick::Stop);
        let s = r.remove_limit(Some("m1"), at(min(15) + 2)).unwrap();
        assert_eq!((s.planned_minutes, s.deadline, s.remaining_seconds), (None, None, None));
        assert!(!r.should_force_stop(g));
        assert_eq!(r.tick(g, at(min(120))), Tick::Idle);
    }

    #[test]
    fn copy_and_tray_text() {
        assert_eq!(
            warning_copy(300, "10:45 AM"),
            (
                "5 minutes left in this recording".to_string(),
                "It stops at 10:45 AM. Click to add 15 minutes or remove the limit.".to_string()
            )
        );
        assert_eq!(warning_copy(40, "x").0, "1 minute left in this recording");
        let mut r = Registry::default();
        assert!(r.status(at(61)).is_none());
        r.arm("m1", t0(), Limit::Minutes(30));
        let left = r.status(at(61)).and_then(|s| s.remaining_seconds).unwrap();
        assert_eq!((left + 59) / 60, 29);
        r.arm("m2", t0(), Limit::NoLimit);
        let s = r.status(at(61)).unwrap();
        assert!(s.remaining_seconds.is_none());
    }
}
