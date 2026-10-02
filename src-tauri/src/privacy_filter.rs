// noFriction Meetings - Privacy Filter
// CRITICAL: Ensures private/incognito browser windows are NEVER captured
//
// Blocked content:
// - Safari Private Browsing
// - Chrome Incognito
// - Firefox Private Browsing
// - Edge InPrivate
// - Password managers (1Password, Keychain Access, etc.)

#[cfg(target_os = "macos")]
use objc::runtime::Object;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};

/// List of window title patterns that indicate private browsing
const PRIVATE_PATTERNS: &[&str] = &[
    "Private",          // Safari Private Browsing
    "Incognito",        // Chrome Incognito
    "InPrivate",        // Edge InPrivate
    "Private Browsing", // Firefox
    "Private Window",   // Generic
];

/// Apps that should NEVER be captured
const BLOCKED_APPS: &[&str] = &[
    "1Password",
    "Keychain Access",
    "Bitwarden",
    "LastPass",
    "Dashlane",
    "KeePassXC",
    "Authy",
    "Terminal", // May contain sensitive commands
];

/// Check if the frontmost window is a private/incognito browser window
#[cfg(target_os = "macos")]
pub fn is_private_window() -> bool {
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front_app: *mut Object = msg_send![workspace, frontmostApplication];

        if front_app.is_null() {
            return false;
        }

        // Get app name
        let name_ns: *mut Object = msg_send![front_app, localizedName];
        if name_ns.is_null() {
            return false;
        }

        let name_utf8: *const std::os::raw::c_char = msg_send![name_ns, UTF8String];
        if name_utf8.is_null() {
            return false;
        }

        let app_name = std::ffi::CStr::from_ptr(name_utf8)
            .to_string_lossy()
            .to_string();

        // Check if it's a blocked app
        for blocked in BLOCKED_APPS {
            if app_name.to_lowercase().contains(&blocked.to_lowercase()) {
                log::info!("🔒 Privacy filter: Blocked app detected - {}", app_name);
                return true;
            }
        }

        // Check if it's a browser (Safari, Chrome, Firefox, Edge, Arc, Brave)
        let browser_names = [
            "Safari", "Chrome", "Firefox", "Edge", "Arc", "Brave", "Opera",
        ];
        let is_browser = browser_names.iter().any(|b| app_name.contains(b));

        if !is_browser {
            return false;
        }

        // For browsers, check window title for private indicators
        if let Some(title) = get_frontmost_window_title() {
            for pattern in PRIVATE_PATTERNS {
                if title.contains(pattern) {
                    log::info!("🔒 Privacy filter: Private window detected - {}", title);
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(not(target_os = "macos"))]
pub fn is_private_window() -> bool {
    false
}

/// Title of the frontmost window of the frontmost app, via
/// `CGWindowListCopyWindowInfo` (m3: no AppleScript / System Events, so no
/// Apple Events entitlement and it works in the App Sandbox). The window list
/// is ordered front to back, so the first normal-layer window owned by the
/// frontmost app's pid is its front window. Window names are only reported
/// with Screen Recording permission, which the capture features need anyway.
#[cfg(target_os = "macos")]
fn get_frontmost_window_title() -> Option<String> {
    let pid: i32 = unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front_app: *mut Object = msg_send![workspace, frontmostApplication];
        if front_app.is_null() {
            return None;
        }
        msg_send![front_app, processIdentifier]
    };
    front_window_title_for_pid(pid)
}

#[cfg(target_os = "macos")]
pub fn front_window_title_for_pid(pid: i32) -> Option<String> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowOwnerPID,
    };

    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let (k_pid, k_layer, k_name) = unsafe {
        (
            CFString::wrap_under_get_rule(kCGWindowOwnerPID),
            CFString::wrap_under_get_rule(kCGWindowLayer),
            CFString::wrap_under_get_rule(kCGWindowName),
        )
    };
    for raw in windows.get_all_values() {
        if raw.is_null() {
            continue;
        }
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(raw as CFDictionaryRef) };
        let num = |k: &CFString| {
            dict.find(k)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
        };
        if num(&k_pid) != Some(pid as i64) || num(&k_layer) != Some(0) {
            continue;
        }
        let title = dict
            .find(&k_name)
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();
        return if title.is_empty() { None } else { Some(title) };
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn get_frontmost_window_title() -> Option<String> {
    None
}

/// (owner app name, window title) for every normal on-screen window, front
/// to back. Titles are empty without Screen Recording permission, so callers
/// must treat "no titled windows" as "signal unavailable". None if the
/// window server can't be queried.
#[cfg(target_os = "macos")]
pub fn on_screen_windows() -> Option<Vec<(String, String)>> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowOwnerName,
    };

    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let (k_layer, k_name, k_owner) = unsafe {
        (
            CFString::wrap_under_get_rule(kCGWindowLayer),
            CFString::wrap_under_get_rule(kCGWindowName),
            CFString::wrap_under_get_rule(kCGWindowOwnerName),
        )
    };
    let mut out = Vec::new();
    for raw in windows.get_all_values() {
        if raw.is_null() {
            continue;
        }
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(raw as CFDictionaryRef) };
        let layer = dict
            .find(&k_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64());
        if layer != Some(0) {
            continue;
        }
        let text = |k: &CFString| {
            dict.find(k)
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
                .unwrap_or_default()
        };
        out.push((text(&k_owner), text(&k_name)));
    }
    Some(out)
}

#[cfg(not(target_os = "macos"))]
pub fn on_screen_windows() -> Option<Vec<(String, String)>> {
    None
}

/// Master check: should we skip capture right now?
pub fn should_skip_capture() -> bool {
    if is_private_window() {
        log::debug!("🔒 Skipping capture - private/sensitive content detected");
        return true;
    }
    false
}

/// Check if a specific app is on the blocklist
pub fn is_blocked_app(app_name: &str) -> bool {
    for blocked in BLOCKED_APPS {
        if app_name.to_lowercase().contains(&blocked.to_lowercase()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_apps() {
        assert!(is_blocked_app("1Password 8"));
        assert!(is_blocked_app("Keychain Access"));
        assert!(!is_blocked_app("Visual Studio Code"));
    }
}
