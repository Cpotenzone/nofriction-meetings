//! Security-scoped bookmarks (m6) for folders the user picks (Obsidian vault).
//!
//! In the App Sandbox, a folder chosen in the open panel is readable only for
//! the rest of that launch. To keep access across launches we store an
//! app-scoped security bookmark (entitlements
//! `files.user-selected.read-write` + `files.bookmarks.app-scope`) and, at
//! startup, resolve it and call `startAccessingSecurityScopedResource`.
//!
//! The Developer ID build isn't sandboxed; bookmarks work there too, and if
//! anything fails it falls back to the plain path.

use base64::Engine;
use std::path::PathBuf;

/// `NSURLBookmarkCreationWithSecurityScope`
#[cfg(target_os = "macos")]
const CREATION_WITH_SECURITY_SCOPE: u64 = 1 << 11;
/// `NSURLBookmarkResolutionWithSecurityScope`
#[cfg(target_os = "macos")]
const RESOLUTION_WITH_SECURITY_SCOPE: u64 = 1 << 10;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub path: PathBuf,
    /// The bookmark should be re-created (the folder moved/renamed)
    pub stale: bool,
    /// startAccessingSecurityScopedResource returned YES
    pub accessing: bool,
}

#[cfg(target_os = "macos")]
unsafe fn nsstring(s: &str) -> *mut objc::runtime::Object {
    use objc::{class, msg_send, sel, sel_impl};
    let c = std::ffi::CString::new(s).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

#[cfg(target_os = "macos")]
unsafe fn error_text(err: *mut objc::runtime::Object) -> String {
    use objc::{msg_send, sel, sel_impl};
    if err.is_null() {
        return "unknown error".into();
    }
    let desc: *mut objc::runtime::Object = msg_send![err, localizedDescription];
    if desc.is_null() {
        return "unknown error".into();
    }
    let utf8: *const std::os::raw::c_char = msg_send![desc, UTF8String];
    if utf8.is_null() {
        return "unknown error".into();
    }
    std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
}

/// Create an app-scoped security bookmark for `path`, base64-encoded.
#[cfg(target_os = "macos")]
pub fn create(path: &str) -> Result<String, String> {
    use objc::rc::autoreleasepool;
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    autoreleasepool(|| unsafe {
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: nsstring(path)];
        if url.is_null() {
            return Err("invalid path".into());
        }
        let mut err: *mut Object = std::ptr::null_mut();
        let nil: *mut Object = std::ptr::null_mut();
        let data: *mut Object = msg_send![url,
            bookmarkDataWithOptions: CREATION_WITH_SECURITY_SCOPE
            includingResourceValuesForKeys: nil
            relativeToURL: nil
            error: &mut err];
        if data.is_null() {
            return Err(format!("Could not create bookmark: {}", error_text(err)));
        }
        let len: usize = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        if bytes.is_null() || len == 0 {
            return Err("empty bookmark".into());
        }
        let slice = std::slice::from_raw_parts(bytes, len);
        Ok(base64::engine::general_purpose::STANDARD.encode(slice))
    })
}

/// Resolve a base64 bookmark and start accessing it. Access lasts for the
/// life of the process (we never call stopAccessing; the vault is used
/// throughout the session).
#[cfg(target_os = "macos")]
pub fn resolve_and_access(b64: &str) -> Result<Resolved, String> {
    use objc::rc::autoreleasepool;
    use objc::runtime::{Object, BOOL, NO};
    use objc::{class, msg_send, sel, sel_impl};
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("bad bookmark encoding: {}", e))?;
    autoreleasepool(|| unsafe {
        let data: *mut Object = msg_send![class!(NSData), dataWithBytes: raw.as_ptr() as *const std::os::raw::c_void length: raw.len()];
        let mut stale: BOOL = NO;
        let mut err: *mut Object = std::ptr::null_mut();
        let nil: *mut Object = std::ptr::null_mut();
        let url: *mut Object = msg_send![class!(NSURL),
            URLByResolvingBookmarkData: data
            options: RESOLUTION_WITH_SECURITY_SCOPE
            relativeToURL: nil
            bookmarkDataIsStale: &mut stale
            error: &mut err];
        if url.is_null() {
            return Err(format!("Could not resolve bookmark: {}", error_text(err)));
        }
        let accessing: BOOL = msg_send![url, startAccessingSecurityScopedResource];
        let p: *mut Object = msg_send![url, path];
        if p.is_null() {
            return Err("bookmark has no path".into());
        }
        let utf8: *const std::os::raw::c_char = msg_send![p, UTF8String];
        let path = std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned();
        Ok(Resolved { path: PathBuf::from(path), stale: stale != NO, accessing: accessing != NO })
    })
}

#[cfg(not(target_os = "macos"))]
pub fn create(_path: &str) -> Result<String, String> {
    Err("security-scoped bookmarks are macOS-only".into())
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_and_access(_b64: &str) -> Result<Resolved, String> {
    Err("security-scoped bookmarks are macOS-only".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn roundtrip_for_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        // Unsandboxed test process: creation may be refused on some systems;
        // only check the round trip when it succeeds.
        if let Ok(b64) = super::create(path.to_str().unwrap()) {
            let r = super::resolve_and_access(&b64).unwrap();
            assert_eq!(r.path.canonicalize().unwrap(), path);
        }
    }
}
