// noFriction Meetings - Accessibility Extractor Module
// Uses macOS Accessibility API to extract live text from focused windows
//
// This provides faster, more accurate text extraction than OCR for apps that
// expose their content via the accessibility hierarchy.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use std::ffi::c_void;

/// Result from accessibility text extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityResult {
    /// Extracted text content
    pub text: String,
    /// Application name
    pub app_name: Option<String>,
    /// Window title
    pub window_title: Option<String>,
    /// Whether app supports accessibility
    pub is_accessible: bool,
    /// Processing duration in milliseconds
    pub duration_ms: u64,
    /// Extraction timestamp
    pub extracted_at: DateTime<Utc>,
}

impl Default for AccessibilityResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            app_name: None,
            window_title: None,
            is_accessible: false,
            duration_ms: 0,
            extracted_at: Utc::now(),
        }
    }
}

/// Configuration for accessibility extraction
#[derive(Debug, Clone)]
pub struct AccessibilityConfig {
    /// Maximum depth to traverse AX hierarchy
    pub max_depth: usize,
    /// Include hidden elements
    pub include_hidden: bool,
    /// Element roles to extract text from
    pub text_roles: Vec<String>,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            include_hidden: false,
            text_roles: vec![
                "AXStaticText".to_string(),
                "AXTextField".to_string(),
                "AXTextArea".to_string(),
                "AXButton".to_string(),
                "AXLink".to_string(),
                "AXHeading".to_string(),
                "AXCell".to_string(),
            ],
        }
    }
}

/// Accessibility text extractor for macOS
pub struct AccessibilityExtractor {
    config: AccessibilityConfig,
}

impl AccessibilityExtractor {
    pub fn new() -> Self {
        Self::with_config(AccessibilityConfig::default())
    }

    pub fn with_config(config: AccessibilityConfig) -> Self {
        Self { config }
    }

    /// Check if accessibility permission is granted
    #[cfg(target_os = "macos")]
    pub fn is_trusted() -> bool {
        use objc::runtime::{Class, Object, BOOL};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            // Create options dictionary with prompt = false
            let nsdict_class = Class::get("NSDictionary").unwrap();
            let nsstring_class = Class::get("NSString").unwrap();
            let nsnumber_class = Class::get("NSNumber").unwrap();

            // kAXTrustedCheckOptionPrompt key
            let key: *mut Object = msg_send![nsstring_class, stringWithUTF8String: "AXTrustedCheckOptionPrompt\0".as_ptr()];
            let value: *mut Object = msg_send![nsnumber_class, numberWithBool: false as BOOL];

            let options: *mut Object =
                msg_send![nsdict_class, dictionaryWithObject:value forKey:key];

            // Check if trusted (without prompting)
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
            }

            AXIsProcessTrustedWithOptions(options as *const c_void)
        }
    }

    /// Request accessibility permission (triggers macOS prompt)
    #[cfg(target_os = "macos")]
    pub fn request_permission_with_prompt() -> bool {
        use objc::runtime::{Class, Object, BOOL};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            // Create options dictionary with prompt = true
            let nsdict_class = Class::get("NSDictionary").unwrap();
            let nsstring_class = Class::get("NSString").unwrap();
            let nsnumber_class = Class::get("NSNumber").unwrap();

            // kAXTrustedCheckOptionPrompt key
            let key: *mut Object = msg_send![nsstring_class, stringWithUTF8String: "AXTrustedCheckOptionPrompt\0".as_ptr()];
            let value: *mut Object = msg_send![nsnumber_class, numberWithBool: true as BOOL];

            let options: *mut Object =
                msg_send![nsdict_class, dictionaryWithObject:value forKey:key];

            // Check if trusted (WITH prompting - this triggers the system dialog)
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
            }

            AXIsProcessTrustedWithOptions(options as *const c_void)
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn request_permission_with_prompt() -> bool {
        true // Non-macOS always granted
    }

    /// Extract text from the currently focused window
    #[cfg(target_os = "macos")]
    pub fn extract_focused_window(&self) -> Result<AccessibilityResult, String> {
        let start = std::time::Instant::now();

        // Check permission first
        if !Self::is_trusted() {
            return Err("Accessibility permission not granted".to_string());
        }

        unsafe {
            use objc::runtime::{Class, Object};
            use objc::{msg_send, sel, sel_impl};

            // Get the system-wide accessibility element
            let ax_class = Class::get("NSWorkspace").ok_or("NSWorkspace not found")?;
            let workspace: *mut Object = msg_send![ax_class, sharedWorkspace];

            // Get frontmost application
            let front_app: *mut Object = msg_send![workspace, frontmostApplication];
            if front_app.is_null() {
                return Ok(AccessibilityResult {
                    is_accessible: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    extracted_at: Utc::now(),
                    ..Default::default()
                });
            }

            // Get app name
            let app_name_obj: *mut Object = msg_send![front_app, localizedName];
            let app_name = nsstring_to_rust(app_name_obj);

            // Get process ID
            let pid: i32 = msg_send![front_app, processIdentifier];

            // Create AXUIElement for the application
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
                fn AXUIElementCopyAttributeValue(
                    element: *mut c_void,
                    attribute: *const c_void,
                    value: *mut *mut c_void,
                ) -> i32;
            }

            let ax_app = AXUIElementCreateApplication(pid);
            if ax_app.is_null() {
                return Ok(AccessibilityResult {
                    app_name: Some(app_name),
                    is_accessible: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    extracted_at: Utc::now(),
                    ..Default::default()
                });
            }

            // Get focused window
            let nsstring_class = Class::get("NSString").ok_or("NSString not found")?;
            let focused_window_key: *mut Object =
                msg_send![nsstring_class, stringWithUTF8String: "AXFocusedWindow\0".as_ptr()];

            let mut focused_window: *mut c_void = std::ptr::null_mut();
            let result = AXUIElementCopyAttributeValue(
                ax_app,
                focused_window_key as *const c_void,
                &mut focused_window,
            );

            if result != 0 || focused_window.is_null() {
                return Ok(AccessibilityResult {
                    app_name: Some(app_name),
                    is_accessible: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                    extracted_at: Utc::now(),
                    ..Default::default()
                });
            }

            // Get window title
            let title_key: *mut Object =
                msg_send![nsstring_class, stringWithUTF8String: "AXTitle\0".as_ptr()];
            let mut title_value: *mut c_void = std::ptr::null_mut();
            let _ = AXUIElementCopyAttributeValue(
                focused_window,
                title_key as *const c_void,
                &mut title_value,
            );
            let window_title = if !title_value.is_null() {
                Some(nsstring_to_rust(title_value as *mut Object))
            } else {
                None
            };

            // Filter out Incognito/Private windows
            if let Some(title) = &window_title {
                let lower = title.to_lowercase();
                if lower.contains("incognito") || lower.contains("private") {
                    log::info!("Ignoring Incognito/Private window: {}", title);
                    return Ok(AccessibilityResult {
                        app_name: Some(app_name),
                        window_title,
                        is_accessible: false,
                        duration_ms: start.elapsed().as_millis() as u64,
                        extracted_at: Utc::now(),
                        ..Default::default()
                    });
                }
            }

            // Extract text from window hierarchy
            let text = self.extract_text_from_element(focused_window, 0)?;

            Ok(AccessibilityResult {
                text,
                app_name: Some(app_name),
                window_title,
                is_accessible: true,
                duration_ms: start.elapsed().as_millis() as u64,
                extracted_at: Utc::now(),
            })
        }
    }

    /// Recursively extract text from an accessibility element
    #[cfg(target_os = "macos")]
    fn extract_text_from_element(
        &self,
        element: *mut c_void,
        depth: usize,
    ) -> Result<String, String> {
        if depth > self.config.max_depth || element.is_null() {
            return Ok(String::new());
        }

        unsafe {
            use objc::runtime::{Class, Object};
            use objc::{msg_send, sel, sel_impl};

            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXUIElementCopyAttributeValue(
                    element: *mut c_void,
                    attribute: *const c_void,
                    value: *mut *mut c_void,
                ) -> i32;
            }

            let nsstring_class = Class::get("NSString").ok_or("NSString not found")?;
            let mut texts = Vec::new();

            // Get role
            let role_key: *mut Object =
                msg_send![nsstring_class, stringWithUTF8String: "AXRole\0".as_ptr()];
            let mut role_value: *mut c_void = std::ptr::null_mut();
            let _ =
                AXUIElementCopyAttributeValue(element, role_key as *const c_void, &mut role_value);

            let role = if !role_value.is_null() {
                nsstring_to_rust(role_value as *mut Object)
            } else {
                String::new()
            };

            // Check if this role should have text extracted
            let should_extract = self.config.text_roles.iter().any(|r| r == &role);

            if should_extract {
                // Get value (text content)
                let value_key: *mut Object =
                    msg_send![nsstring_class, stringWithUTF8String: "AXValue\0".as_ptr()];
                let mut value: *mut c_void = std::ptr::null_mut();
                let _ =
                    AXUIElementCopyAttributeValue(element, value_key as *const c_void, &mut value);

                if !value.is_null() {
                    let text = nsstring_to_rust(value as *mut Object);
                    if !text.is_empty() {
                        texts.push(text);
                    }
                }

                // Also try AXTitle for buttons/links
                if role == "AXButton" || role == "AXLink" {
                    let title_key: *mut Object =
                        msg_send![nsstring_class, stringWithUTF8String: "AXTitle\0".as_ptr()];
                    let mut title: *mut c_void = std::ptr::null_mut();
                    let _ = AXUIElementCopyAttributeValue(
                        element,
                        title_key as *const c_void,
                        &mut title,
                    );

                    if !title.is_null() {
                        let text = nsstring_to_rust(title as *mut Object);
                        if !text.is_empty() {
                            texts.push(text);
                        }
                    }
                }
            }

            // Get children and recurse
            let children_key: *mut Object =
                msg_send![nsstring_class, stringWithUTF8String: "AXChildren\0".as_ptr()];
            let mut children: *mut c_void = std::ptr::null_mut();
            let result = AXUIElementCopyAttributeValue(
                element,
                children_key as *const c_void,
                &mut children,
            );

            if result == 0 && !children.is_null() {
                let children_array = children as *mut Object;
                let count: usize = msg_send![children_array, count];

                for i in 0..count {
                    let child: *mut Object = msg_send![children_array, objectAtIndex: i];
                    if let Ok(child_text) =
                        self.extract_text_from_element(child as *mut c_void, depth + 1)
                    {
                        if !child_text.is_empty() {
                            texts.push(child_text);
                        }
                    }
                }
            }

            Ok(texts.join("\n"))
        }
    }

    /// Non-macOS stub
    #[cfg(not(target_os = "macos"))]
    pub fn is_trusted() -> bool {
        false
    }

    #[cfg(not(target_os = "macos"))]
    pub fn extract_focused_window(&self) -> Result<AccessibilityResult, String> {
        Err("Accessibility extraction only available on macOS".to_string())
    }
}

impl Default for AccessibilityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to convert NSString to Rust String
/// Also handles NSNumber by converting to string representation
#[cfg(target_os = "macos")]
unsafe fn nsstring_to_rust(nsstring: *mut objc::runtime::Object) -> String {
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;

    if nsstring.is_null() {
        return String::new();
    }

    // Check if this is actually an NSString (or subclass)
    let nsstring_class = class!(NSString);
    let is_nsstring: bool = msg_send![nsstring, isKindOfClass: nsstring_class];

    if is_nsstring {
        let utf8: *const i8 = msg_send![nsstring, UTF8String];
        if utf8.is_null() {
            return String::new();
        }
        return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
    }

    // Check if this is an NSNumber - convert to string representation
    let nsnumber_class = class!(NSNumber);
    let is_nsnumber: bool = msg_send![nsstring, isKindOfClass: nsnumber_class];

    if is_nsnumber {
        // Get string description of the number
        let description: *mut objc::runtime::Object = msg_send![nsstring, stringValue];
        if !description.is_null() {
            let utf8: *const i8 = msg_send![description, UTF8String];
            if !utf8.is_null() {
                return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
            }
        }
        return String::new();
    }

    // For any other object type, try to get its description as a fallback
    let description: *mut objc::runtime::Object = msg_send![nsstring, description];
    if !description.is_null() {
        let utf8: *const i8 = msg_send![description, UTF8String];
        if !utf8.is_null() {
            return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AccessibilityConfig::default();
        assert_eq!(config.max_depth, 10);
        assert!(!config.include_hidden);
        assert!(!config.text_roles.is_empty());
    }

    #[test]
    fn test_result_default() {
        let result = AccessibilityResult::default();
        assert!(result.text.is_empty());
        assert!(!result.is_accessible);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_trusted_check() {
        // Just verify this doesn't crash
        let _ = AccessibilityExtractor::is_trusted();
    }
}
