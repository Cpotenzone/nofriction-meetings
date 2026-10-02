//! macOS notifications (tauri-plugin-notification) for events the user must
//! see while the window is in the background — today the meeting-end
//! countdown ("Meeting seems to have ended — stopping in 30 s").
//!
//! - Posting goes through tauri-plugin-notification (NSUserNotificationCenter
//!   on macOS). No entitlement is needed and it works in the App Sandbox.
//! - Permission is requested at the first recording of each launch (macOS
//!   only prompts once, ever), never at app launch.
//! - Clicking the notification activates the app; the Swift bridge reports
//!   activation and, while a notification is fresh, we show + focus the main
//!   window (it may be hidden: closing it only hides it).
//!
//! Notification text never contains transcript content.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// How long after posting a click (app activation) should bring the window up.
#[cfg_attr(test, allow(dead_code))]
const CLICK_WINDOW: Duration = Duration::from_secs(90);

static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
static LAST_POSTED: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));
static PERMISSION_ASKED: AtomicBool = AtomicBool::new(false);

#[cfg(all(target_os = "macos", not(test)))]
mod ffi {
    use std::os::raw::{c_char, c_void};
    pub type Callback = extern "C" fn(*mut c_void, *const c_char);
    extern "C" {
        pub fn nf_notifications_request(ctx: *mut c_void, cb: Callback);
        pub fn nf_observe_app_activation(cb: Callback);
    }
}

/// Title for the meeting-end countdown notification.
pub fn meeting_end_title(countdown_secs: i64) -> String {
    format!("Meeting seems to have ended — stopping in {} s", countdown_secs.max(0))
}

/// Body: the detector's reason (app/window/calendar names only, never
/// transcript text) plus what clicking does.
pub fn meeting_end_body(reason: &str) -> String {
    let reason = reason.trim();
    let mut first = reason.chars();
    let reason = match first.next() {
        Some(c) => c.to_uppercase().collect::<String>() + first.as_str(),
        None => "No meeting activity detected".to_string(),
    };
    format!("{}. Click to keep recording or stop now.", reason)
}

/// Remember the app handle and start listening for app activation (the
/// result of clicking a notification). Call once from `setup`.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        ffi::nf_observe_app_activation(on_app_activated);
    }
}

#[cfg(all(target_os = "macos", not(test)))]
extern "C" fn on_app_activated(_ctx: *mut std::os::raw::c_void, _json: *const std::os::raw::c_char) {
    let fresh = LAST_POSTED.lock().map(|t| t.elapsed() < CLICK_WINDOW).unwrap_or(false);
    if !fresh {
        return;
    }
    *LAST_POSTED.lock() = None;
    if let Some(app) = APP.get().cloned() {
        // Off the main thread: window calls dispatch back to it.
        tauri::async_runtime::spawn(async move { focus_main_window(&app) });
    }
}

pub fn focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Ask macOS for notification permission once per launch, at the first
/// recording. macOS shows its prompt only the first time ever.
pub fn request_permission_once() {
    if PERMISSION_ASKED.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use std::os::raw::{c_char, c_void};
        extern "C" fn done(_ctx: *mut c_void, json: *const c_char) {
            if json.is_null() {
                return;
            }
            let s = unsafe { std::ffi::CStr::from_ptr(json) }.to_string_lossy().into_owned();
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            if v["ok"].as_bool() == Some(true) {
                if v["asked"].as_bool() == Some(true) {
                    log::info!(
                        "🔔 Notification permission: {}",
                        if v["granted"].as_bool() == Some(true) { "granted" } else { "not granted" }
                    );
                }
            } else {
                log::warn!("🔔 Notification permission request failed: {}", v["error"].as_str().unwrap_or("?"));
            }
        }
        unsafe { ffi::nf_notifications_request(std::ptr::null_mut(), done) };
    }
}

/// Post a notification. Returns false if the plugin refused it.
pub fn post(app: &AppHandle, title: &str, body: &str) -> bool {
    use tauri_plugin_notification::NotificationExt;
    match app.notification().builder().title(title).body(body).show() {
        Ok(()) => {
            *LAST_POSTED.lock() = Some(Instant::now());
            true
        }
        Err(e) => {
            log::warn!("🔔 Could not post notification: {}", e);
            false
        }
    }
}

/// Meeting-end countdown started while the window isn't focused.
pub fn notify_meeting_end(app: &AppHandle, reason: &str, countdown_secs: i64) {
    if post(app, &meeting_end_title(countdown_secs), &meeting_end_body(reason)) {
        log::info!("🔔 Posted meeting-end notification");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_end_copy() {
        assert_eq!(meeting_end_title(30), "Meeting seems to have ended — stopping in 30 s");
        assert_eq!(meeting_end_title(-3), "Meeting seems to have ended — stopping in 0 s");
        assert_eq!(
            meeting_end_body("Zoom stopped using the microphone"),
            "Zoom stopped using the microphone. Click to keep recording or stop now."
        );
        assert_eq!(
            meeting_end_body("no one has spoken for 3 min"),
            "No one has spoken for 3 min. Click to keep recording or stop now."
        );
        assert_eq!(meeting_end_body("  "), "No meeting activity detected. Click to keep recording or stop now.");
    }
}
