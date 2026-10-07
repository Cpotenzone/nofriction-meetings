//! The frontmost browser's address while recording (DMG build only; this
//! module isn't compiled into the `mas` build, which has no Accessibility).
//!
//! Every few seconds while a meeting records (not paused, screen capture on,
//! the setting on), if Accessibility is **already** granted (checked without
//! prompting) and Safari, Chrome, Arc, Edge or Brave is frontmost, the page
//! address is read from the browser's accessibility tree: the `AXURL` of its
//! web area, else the value of its address field. Page content is never
//! walked or read.
//!
//! Each new address (not the same page again) is stored as a
//! `text_snapshots` row with `source = 'browser_url'`, the meeting id, the
//! time, the browser and the window title. Tracking and credential query
//! parameters are stripped first ([`detect::normalize`]). Because these are
//! ordinary screen-text rows, Delete, Strike, time ranges and meeting delete
//! remove them like any other screen text. Addresses are never logged.

use super::{detect, BROWSER_URL_SOURCE};
use crate::AppState;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// How often the frontmost browser is checked.
pub const POLL: Duration = Duration::from_secs(3);
/// Most addresses stored for one recording.
pub const MAX_ROWS_PER_MEETING: usize = 2000;

/// Browsers read, by bundle id (release, beta and nightly channels).
pub const BROWSERS: &[(&str, &str)] = &[
    ("com.apple.Safari", "Safari"),
    ("com.apple.SafariTechnologyPreview", "Safari Technology Preview"),
    ("com.google.Chrome", "Google Chrome"),
    ("com.google.Chrome.beta", "Google Chrome Beta"),
    ("com.google.Chrome.dev", "Google Chrome Dev"),
    ("com.google.Chrome.canary", "Google Chrome Canary"),
    ("company.thebrowser.Browser", "Arc"),
    ("com.microsoft.edgemac", "Microsoft Edge"),
    ("com.microsoft.edgemac.Beta", "Microsoft Edge Beta"),
    ("com.microsoft.edgemac.Dev", "Microsoft Edge Dev"),
    ("com.microsoft.edgemac.Canary", "Microsoft Edge Canary"),
    ("com.brave.Browser", "Brave"),
    ("com.brave.Browser.beta", "Brave Beta"),
    ("com.brave.Browser.nightly", "Brave Nightly"),
];

pub fn browser_name(bundle_id: &str) -> Option<&'static str> {
    BROWSERS.iter().find(|(id, _)| *id == bundle_id).map(|(_, n)| *n)
}

/// Window titles of private browsing windows (same patterns as
/// `privacy_filter`). Safari doesn't mark private windows in the title.
pub fn looks_private(title: &str) -> bool {
    let t = title.to_lowercase();
    ["private browsing", "incognito", "inprivate", "private window", "— private", "- private"]
        .iter()
        .any(|p| t.contains(p))
}

/// An address-field value that is one web address (not a search being typed).
pub fn looks_like_address(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.contains(char::is_whitespace) {
        return false;
    }
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return detect::is_openable(v);
    }
    // A bare host the detector accepts as a whole (`example.com/path`)
    detect::detect(v, false).first().map_or(false, |f| f.pos == 0)
}

/// What one check of the frontmost browser found.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserPage {
    pub app_name: String,
    pub url: String,
    pub title: Option<String>,
}

/// What to store for one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    /// Normalized (tracking and credential parameters removed)
    pub url: String,
    pub key: String,
    pub app_name: String,
    pub title: Option<String>,
}

/// Dedupe and throttle for one recording: a page is stored when it differs
/// from the last one stored, up to [`MAX_ROWS_PER_MEETING`].
#[derive(Debug, Default)]
pub struct Tracker {
    last_key: Option<String>,
    stored: usize,
}

impl Tracker {
    pub fn next(&mut self, page: &BrowserPage) -> Option<Capture> {
        if page.title.as_deref().map_or(false, looks_private) {
            // Nothing from a private window; the next normal page is new
            self.last_key = None;
            return None;
        }
        let n = detect::normalize(&page.url)?;
        if !detect::is_openable(&n.url) {
            return None;
        }
        if self.last_key.as_deref() == Some(n.key.as_str()) || self.stored >= MAX_ROWS_PER_MEETING {
            return None;
        }
        self.last_key = Some(n.key.clone());
        self.stored += 1;
        Some(Capture {
            url: n.url,
            key: n.key,
            app_name: page.app_name.clone(),
            title: page.title.clone().filter(|t| !t.trim().is_empty()),
        })
    }
}

/// Store one address as a screen-text row of the meeting.
pub async fn store(
    db: &crate::database::DatabaseManager,
    meeting_id: &str,
    cap: &Capture,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(cap.url.as_bytes()).iter().take(8).map(|b| format!("{:02x}", b)).collect();
    db.add_text_snapshot_full(
        &uuid::Uuid::new_v4().to_string(),
        None,
        None,
        Some(meeting_id),
        at,
        &cap.url,
        &hash,
        1.0,
        BROWSER_URL_SOURCE,
        Some(&cap.app_name),
        cap.title.as_deref(),
    )
    .await
}

/// Start watching the frontmost browser for this recording. Stops by itself
/// when the recording stops (or another meeting starts).
pub fn start(app: AppHandle, meeting_id: String) {
    tauri::async_runtime::spawn(async move {
        let mut tracker = Tracker::default();
        // (recording this meeting, paused)
        let status = |state: &AppState| {
            let (recording, paused) = {
                let engine = state.capture_engine.read();
                (engine.is_recording(), engine.is_paused())
            };
            let current = state.state_builder.read().current_meeting_id();
            (recording && current.as_deref() == Some(meeting_id.as_str()), paused)
        };
        loop {
            tokio::time::sleep(POLL).await;
            let Some(state) = app.try_state::<AppState>() else { break };
            let (recording, paused) = status(&state);
            if !recording {
                break;
            }
            if paused {
                continue;
            }
            let screen_on = !matches!(state.settings.get("capture_screen").await, Ok(Some(v)) if v == "false");
            if !screen_on || !super::browser_capture_enabled(&state.settings).await {
                continue;
            }
            // Checked without prompting; nothing happens until it's granted
            if !crate::accessibility_extractor::AccessibilityExtractor::is_trusted() {
                continue;
            }
            let page = tokio::task::spawn_blocking(read_frontmost_browser).await.ok().flatten();
            let Some(page) = page else { continue };
            // The read can take a moment: store only if still recording, unpaused
            if status(&state) != (true, false) {
                continue;
            }
            if let Some(cap) = tracker.next(&page) {
                if let Err(e) = store(&state.database, &meeting_id, &cap, chrono::Utc::now()).await {
                    // Never the address itself
                    log::warn!("Couldn't save a browser address for the meeting: {}", e);
                }
            }
        }
    });
}

/// The frontmost app's page address, if it's a supported browser.
#[cfg(target_os = "macos")]
pub fn read_frontmost_browser() -> Option<BrowserPage> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe fn ns_string(s: *mut Object) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }

    objc::rc::autoreleasepool(|| {
        let (bundle, pid, name) = unsafe {
            let ws: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: *mut Object = msg_send![ws, frontmostApplication];
            if front.is_null() {
                return None;
            }
            let bundle: *mut Object = msg_send![front, bundleIdentifier];
            let pid: i32 = msg_send![front, processIdentifier];
            let name: *mut Object = msg_send![front, localizedName];
            (ns_string(bundle)?, pid, ns_string(name))
        };
        let known = browser_name(&bundle)?;
        let (url, title) = unsafe { ax::read_page(pid) }?;
        Some(BrowserPage { app_name: name.unwrap_or_else(|| known.to_string()), url, title })
    })
}

#[cfg(not(target_os = "macos"))]
pub fn read_frontmost_browser() -> Option<BrowserPage> {
    None
}

#[cfg(target_os = "macos")]
mod ax {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::collections::VecDeque;
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    /// Elements looked at per check, how deep, and children per element.
    const MAX_NODES: usize = 400;
    const MAX_DEPTH: usize = 30;
    const MAX_CHILDREN: isize = 100;
    /// Once an address field is found, how much further to look for the
    /// web area (whose `AXURL` is exact; the field may hide the scheme)
    const EXTRA_NODES: usize = 150;
    /// Each AX call waits at most this long for a busy browser (the system
    /// default is about 6 s, and it isn't inherited from the app element)
    const CALL_TIMEOUT_SECS: f32 = 0.25;
    /// One check gives up after this long
    const BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

    // Same signatures as accessibility_extractor.rs declares them
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
        fn AXUIElementCopyAttributeValue(element: *mut c_void, attribute: *const c_void, value: *mut *mut c_void) -> i32;
        fn AXUIElementSetMessagingTimeout(element: *mut c_void, timeout: f32) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFURLGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFArrayGetCount(array: CFTypeRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
        fn CFURLGetString(url: CFTypeRef) -> CFStringRef;
    }

    /// A +1 CF object, released when dropped.
    struct Owned(CFTypeRef);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }

    unsafe fn copy(el: CFTypeRef, attr: &'static str) -> Option<Owned> {
        let key = CFString::from_static_string(attr);
        let mut v: *mut c_void = std::ptr::null_mut();
        let key_ref: CFStringRef = key.as_concrete_TypeRef();
        if AXUIElementCopyAttributeValue(el as *mut c_void, key_ref as *const c_void, &mut v) != 0 || v.is_null() {
            return None;
        }
        Some(Owned(v as CFTypeRef))
    }

    unsafe fn text(v: CFTypeRef) -> Option<String> {
        let t = CFGetTypeID(v);
        if t == CFStringGetTypeID() {
            Some(CFString::wrap_under_get_rule(v as CFStringRef).to_string())
        } else if t == CFURLGetTypeID() {
            let s = CFURLGetString(v);
            (!s.is_null()).then(|| CFString::wrap_under_get_rule(s).to_string())
        } else {
            None
        }
    }

    unsafe fn attr_text(el: CFTypeRef, attr: &'static str) -> Option<String> {
        copy(el, attr).and_then(|o| text(o.0))
    }

    fn is_web(url: &str) -> bool {
        let l = url.to_ascii_lowercase();
        l.starts_with("https://") || l.starts_with("http://")
    }

    /// (address, window title) of the focused window of app `pid`.
    pub unsafe fn read_page(pid: i32) -> Option<(String, Option<String>)> {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return None;
        }
        let app = Owned(app as CFTypeRef);
        let started = std::time::Instant::now();
        // A busy browser can't stall the check: a short timeout on every
        // element asked, and a budget for the whole walk
        let short = |el: CFTypeRef| {
            AXUIElementSetMessagingTimeout(el as *mut c_void, CALL_TIMEOUT_SECS);
        };
        short(app.0);
        let window = copy(app.0, "AXFocusedWindow").or_else(|| copy(app.0, "AXMainWindow"))?;
        short(window.0);
        let title = attr_text(window.0, "AXTitle").filter(|t| !t.trim().is_empty());

        // Breadth-first over the browser's own UI. Children arrays are kept
        // until the walk ends (the queue borrows their elements).
        let mut keep: Vec<Owned> = Vec::new();
        let mut queue: VecDeque<(CFTypeRef, usize)> = VecDeque::from([(window.0, 0usize)]);
        let mut visited = 0;
        let mut address: Option<String> = None;
        let mut after_address = 0;
        while let Some((el, depth)) = queue.pop_front() {
            visited += 1;
            if visited > MAX_NODES || started.elapsed() > BUDGET {
                break;
            }
            short(el);
            if address.is_some() {
                after_address += 1;
                if after_address > EXTRA_NODES {
                    break;
                }
            }
            let role = attr_text(el, "AXRole").unwrap_or_default();
            match role.as_str() {
                "AXWebArea" => {
                    if let Some(u) = attr_text(el, "AXURL").filter(|u| is_web(u)) {
                        return Some((u, title));
                    }
                    // The page itself is never walked
                    continue;
                }
                "AXTextField" | "AXComboBox" => {
                    if address.is_none() {
                        address = attr_text(el, "AXValue").filter(|v| super::looks_like_address(v));
                    }
                    continue;
                }
                _ => {}
            }
            if depth >= MAX_DEPTH {
                continue;
            }
            if let Some(children) = copy(el, "AXChildren") {
                if CFGetTypeID(children.0) == CFArrayGetTypeID() {
                    let n = CFArrayGetCount(children.0).min(MAX_CHILDREN);
                    for i in 0..n {
                        let c = CFArrayGetValueAtIndex(children.0, i);
                        if !c.is_null() {
                            queue.push_back((c, depth + 1));
                        }
                    }
                }
                keep.push(children);
            }
        }
        drop(keep);
        address.map(|a| (a, title))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, title: Option<&str>) -> BrowserPage {
        BrowserPage { app_name: "Safari".into(), url: url.into(), title: title.map(String::from) }
    }

    #[test]
    fn tracker_stores_each_new_page_once_and_strips_tracking() {
        let mut t = Tracker::default();
        let a = t.next(&page("https://www.khanacademy.org/math?utm_source=x&token=abc", Some("Math | Khan Academy"))).unwrap();
        assert_eq!(a.url, "https://www.khanacademy.org/math");
        assert_eq!(a.key, "khanacademy.org/math");
        assert_eq!(a.title.as_deref(), Some("Math | Khan Academy"));
        // Same page (http/https and www merged): nothing new
        assert!(t.next(&page("http://khanacademy.org/math/", None)).is_none());
        // Another page, then back: stored again (a second visit)
        assert!(t.next(&page("https://example.edu/syllabus", None)).is_some());
        assert!(t.next(&page("https://khanacademy.org/math", None)).is_some());
    }

    #[test]
    fn tracker_skips_private_windows_and_non_web_pages() {
        let mut t = Tracker::default();
        assert!(t.next(&page("https://example.com/a", Some("Docs - Google Chrome (Incognito)"))).is_none());
        assert!(t.next(&page("https://example.com/a", Some("Private Browsing"))).is_none());
        for bad in ["file:///Users/me/notes.html", "javascript:alert(1)", "about:blank", "chrome://settings", "favorites://"] {
            assert!(t.next(&page(bad, None)).is_none(), "{}", bad);
        }
        assert!(t.next(&page("https://example.com/a", Some("Docs"))).is_some());
    }

    #[test]
    fn tracker_caps_rows_per_meeting() {
        let mut t = Tracker::default();
        for i in 0..MAX_ROWS_PER_MEETING {
            assert!(t.next(&page(&format!("https://example.com/p{}", i), None)).is_some());
        }
        assert!(t.next(&page("https://example.com/one-more", None)).is_none());
    }

    #[test]
    fn address_field_values_must_be_one_address() {
        assert!(looks_like_address("example.com/path"));
        assert!(looks_like_address("https://docs.example.org/a?b=1"));
        assert!(looks_like_address("khanacademy.org"));
        assert!(!looks_like_address("how to cite a website"));
        assert!(!looks_like_address("exam"));
        assert!(!looks_like_address("example.c"));
        assert!(!looks_like_address(""));
        assert!(!looks_like_address("javascript:alert(1)"));
    }

    #[test]
    fn only_known_browsers() {
        assert_eq!(browser_name("com.apple.Safari"), Some("Safari"));
        assert_eq!(browser_name("company.thebrowser.Browser"), Some("Arc"));
        assert!(browser_name("com.apple.finder").is_none());
        assert!(browser_name("com.nofriction.meetings").is_none());
    }
}
