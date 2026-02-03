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

/// Get the title of the frontmost window using AppleScript (simple, reliable)
#[cfg(target_os = "macos")]
fn get_frontmost_window_title() -> Option<String> {
    use std::process::Command;

    // Use AppleScript to get the frontmost window title
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events"
                set frontApp to first application process whose frontmost is true
                tell frontApp
                    try
                        return name of window 1
                    on error
                        return ""
                    end try
                end tell
            end tell"#,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn get_frontmost_window_title() -> Option<String> {
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
