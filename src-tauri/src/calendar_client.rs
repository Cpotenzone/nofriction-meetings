// noFriction Meetings - Calendar Client Module
// Uses macOS EventKit framework to fetch calendar events for meeting detection
//
// Requires com.apple.security.personal-information.calendars entitlement

use chrono::{DateTime, Duration, TimeZone, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Calendar event from EventKit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEventNative {
    /// Unique event identifier
    pub event_id: String,
    /// Event title
    pub title: String,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: DateTime<Utc>,
    /// Location (physical or URL)
    pub location: Option<String>,
    /// Attendee email addresses
    pub attendees: Vec<String>,
    /// Calendar name this event belongs to
    pub calendar_name: String,
    /// Whether this is an all-day event
    pub is_all_day: bool,
    /// Meeting URL if present (Zoom, Meet, Teams)
    pub meeting_url: Option<String>,
    /// Event notes/description
    pub notes: Option<String>,
}

/// Calendar access status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarAccessStatus {
    Authorized,
    Denied,
    Restricted,
    NotDetermined,
    Unknown,
}

/// Configuration for calendar client
#[derive(Debug, Clone)]
pub struct CalendarConfig {
    /// How far ahead to fetch events (hours)
    pub lookahead_hours: i64,
    /// How far back to fetch events (hours)
    pub lookbehind_hours: i64,
    /// Calendar names to include (empty = all)
    pub included_calendars: Vec<String>,
    /// Calendar names to exclude
    pub excluded_calendars: Vec<String>,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            lookahead_hours: 24,
            lookbehind_hours: 2,
            included_calendars: Vec::new(),
            excluded_calendars: vec!["Birthdays".to_string(), "Holidays".to_string()],
        }
    }
}

/// Cache for calendar events
struct CalendarCache {
    events: Vec<CalendarEventNative>,
    last_fetched: Option<DateTime<Utc>>,
    cache_duration_secs: i64,
}

impl Default for CalendarCache {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            last_fetched: None,
            cache_duration_secs: 300, // 5 minutes
        }
    }
}

/// Calendar client for macOS EventKit
pub struct CalendarClient {
    config: CalendarConfig,
    cache: Arc<RwLock<CalendarCache>>,
}

impl CalendarClient {
    pub fn new() -> Self {
        Self::with_config(CalendarConfig::default())
    }

    pub fn with_config(config: CalendarConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(CalendarCache::default())),
        }
    }

    /// Check calendar access status
    #[cfg(target_os = "macos")]
    pub fn check_access() -> CalendarAccessStatus {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let ek_class = match Class::get("EKEventStore") {
                Some(c) => c,
                None => return CalendarAccessStatus::Unknown,
            };

            // EKAuthorizationStatusForEntityType: 0 = Events
            let status: i64 = msg_send![ek_class, authorizationStatusForEntityType: 0i64];

            match status {
                0 => CalendarAccessStatus::NotDetermined,
                1 => CalendarAccessStatus::Restricted,
                2 => CalendarAccessStatus::Denied,
                3 => CalendarAccessStatus::Authorized,
                _ => CalendarAccessStatus::Unknown,
            }
        }
    }

    /// Request calendar access (will prompt user)
    #[cfg(target_os = "macos")]
    pub async fn request_access() -> Result<bool, String> {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        // First check current status
        let initial_status = Self::check_access();
        if initial_status == CalendarAccessStatus::Authorized {
            return Ok(true);
        }
        if initial_status == CalendarAccessStatus::Denied
            || initial_status == CalendarAccessStatus::Restricted
        {
            return Ok(false);
        }

        unsafe {
            let ek_class = Class::get("EKEventStore").ok_or("EKEventStore not found")?;
            let store: *mut Object = msg_send![ek_class, alloc];
            let store: *mut Object = msg_send![store, init];

            if store.is_null() {
                return Err("Failed to create EKEventStore".to_string());
            }

            // Request access by calling requestAccessToEntityType:completion:
            // Since block closures are complex, we'll trigger the request by simply
            // accessing calendar data which will prompt for permission on first use.
            // The actual permission prompt is triggered by EventKit internal machinery.
            // We just need to create the store and try to access calendars.
            let _calendars: *mut Object = msg_send![store, calendarsForEntityType: 0i64];

            // Poll for status change (the request_access call triggers the system prompt)
            for _ in 0..100 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                let status = Self::check_access();
                if status != CalendarAccessStatus::NotDetermined {
                    return Ok(status == CalendarAccessStatus::Authorized);
                }
            }

            // Timeout - check final status
            Ok(Self::check_access() == CalendarAccessStatus::Authorized)
        }
    }

    /// Fetch events for today (with caching)
    #[cfg(target_os = "macos")]
    pub fn fetch_events(&self) -> Result<Vec<CalendarEventNative>, String> {
        // Check cache first
        {
            let cache = self.cache.read();
            if let Some(last_fetched) = cache.last_fetched {
                let elapsed = (Utc::now() - last_fetched).num_seconds();
                if elapsed < cache.cache_duration_secs {
                    return Ok(cache.events.clone());
                }
            }
        }

        // Fetch fresh events
        let events = self.fetch_events_internal()?;

        // Update cache
        {
            let mut cache = self.cache.write();
            cache.events = events.clone();
            cache.last_fetched = Some(Utc::now());
        }

        Ok(events)
    }

    /// Internal event fetching from EventKit
    #[cfg(target_os = "macos")]
    fn fetch_events_internal(&self) -> Result<Vec<CalendarEventNative>, String> {
        use objc::runtime::{Class, Object, BOOL, YES};
        use objc::{msg_send, sel, sel_impl};
        use std::ptr;

        unsafe {
            // Check access first
            if Self::check_access() != CalendarAccessStatus::Authorized {
                return Err("Calendar access not authorized".to_string());
            }

            // Create event store
            let ek_class = Class::get("EKEventStore").ok_or("EKEventStore not found")?;
            let store: *mut Object = msg_send![ek_class, alloc];
            let store: *mut Object = msg_send![store, init];

            if store.is_null() {
                return Err("Failed to create EKEventStore".to_string());
            }

            // Get date range
            let now = Utc::now();
            let start_date = now - Duration::hours(self.config.lookbehind_hours);
            let end_date = now + Duration::hours(self.config.lookahead_hours);

            // Create NSDate objects
            let nsdate_class = Class::get("NSDate").ok_or("NSDate not found")?;
            let start_interval = start_date.timestamp() as f64 - 978307200.0; // Convert to NSDate reference
            let end_interval = end_date.timestamp() as f64 - 978307200.0;

            let start_nsdate: *mut Object =
                msg_send![nsdate_class, dateWithTimeIntervalSinceReferenceDate: start_interval];
            let end_nsdate: *mut Object =
                msg_send![nsdate_class, dateWithTimeIntervalSinceReferenceDate: end_interval];

            // Get all calendars for events
            let calendars: *mut Object = msg_send![store, calendarsForEntityType: 0i64];
            if calendars.is_null() {
                return Ok(Vec::new());
            }

            // Create predicate
            let predicate: *mut Object = msg_send![store, predicateForEventsWithStartDate:start_nsdate endDate:end_nsdate calendars:calendars];
            if predicate.is_null() {
                return Err("Failed to create event predicate".to_string());
            }

            // Fetch events
            let events_array: *mut Object = msg_send![store, eventsMatchingPredicate: predicate];
            if events_array.is_null() {
                return Ok(Vec::new());
            }

            let count: usize = msg_send![events_array, count];
            let mut events = Vec::with_capacity(count);

            for i in 0..count {
                let event: *mut Object = msg_send![events_array, objectAtIndex: i];
                if event.is_null() {
                    continue;
                }

                // Get calendar name
                let calendar: *mut Object = msg_send![event, calendar];
                let calendar_name = if !calendar.is_null() {
                    let title: *mut Object = msg_send![calendar, title];
                    nsstring_to_rust(title)
                } else {
                    String::new()
                };

                // Check exclusions
                if self
                    .config
                    .excluded_calendars
                    .iter()
                    .any(|c| c == &calendar_name)
                {
                    continue;
                }

                // Check inclusions if specified
                if !self.config.included_calendars.is_empty()
                    && !self
                        .config
                        .included_calendars
                        .iter()
                        .any(|c| c == &calendar_name)
                {
                    continue;
                }

                // Get event title
                let title_obj: *mut Object = msg_send![event, title];
                let title = nsstring_to_rust(title_obj);

                // Get event ID
                let event_id_obj: *mut Object = msg_send![event, eventIdentifier];
                let event_id = nsstring_to_rust(event_id_obj);

                // Get start/end dates
                let start_obj: *mut Object = msg_send![event, startDate];
                let end_obj: *mut Object = msg_send![event, endDate];

                let start_time = nsdate_to_chrono(start_obj);
                let end_time = nsdate_to_chrono(end_obj);

                // Get location
                let location_obj: *mut Object = msg_send![event, location];
                let location = if !location_obj.is_null() {
                    let s = nsstring_to_rust(location_obj);
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                };

                // Get notes
                let notes_obj: *mut Object = msg_send![event, notes];
                let notes = if !notes_obj.is_null() {
                    let s = nsstring_to_rust(notes_obj);
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                };

                // Check if all-day
                let is_all_day: BOOL = msg_send![event, isAllDay];

                // Try to extract meeting URL from location or notes
                let meeting_url = extract_meeting_url(&location, &notes);

                // Get attendees
                let attendees_array: *mut Object = msg_send![event, attendees];
                let mut attendees = Vec::new();
                if !attendees_array.is_null() {
                    let att_count: usize = msg_send![attendees_array, count];
                    for j in 0..att_count {
                        let attendee: *mut Object = msg_send![attendees_array, objectAtIndex: j];
                        if !attendee.is_null() {
                            let url: *mut Object = msg_send![attendee, URL];
                            if !url.is_null() {
                                let email_str: *mut Object = msg_send![url, absoluteString];
                                let email = nsstring_to_rust(email_str);
                                // Remove mailto: prefix
                                let email =
                                    email.strip_prefix("mailto:").unwrap_or(&email).to_string();
                                if !email.is_empty() {
                                    attendees.push(email);
                                }
                            }
                        }
                    }
                }

                events.push(CalendarEventNative {
                    event_id,
                    title,
                    start_time,
                    end_time,
                    location,
                    attendees,
                    calendar_name,
                    is_all_day: is_all_day == YES,
                    meeting_url,
                    notes,
                });
            }

            // Sort by start time
            events.sort_by(|a, b| a.start_time.cmp(&b.start_time));

            Ok(events)
        }
    }

    /// Get currently active or upcoming event
    pub fn get_current_event(&self) -> Option<CalendarEventNative> {
        let events = self.fetch_events().ok()?;
        let now = Utc::now();

        // First check for active meeting
        for event in &events {
            if now >= event.start_time && now <= event.end_time {
                return Some(event.clone());
            }
        }

        // Then check for upcoming in next 15 minutes
        let soon = now + Duration::minutes(15);
        for event in &events {
            if event.start_time > now && event.start_time <= soon {
                return Some(event.clone());
            }
        }

        None
    }

    /// Clear the event cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write();
        cache.events.clear();
        cache.last_fetched = None;
    }

    /// Non-macOS stubs
    #[cfg(not(target_os = "macos"))]
    pub fn check_access() -> CalendarAccessStatus {
        CalendarAccessStatus::Unknown
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn request_access() -> Result<bool, String> {
        Err("Calendar access only available on macOS".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn fetch_events(&self) -> Result<Vec<CalendarEventNative>, String> {
        Err("Calendar access only available on macOS".to_string())
    }
}

impl Default for CalendarClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract meeting URL from location or notes
fn extract_meeting_url(location: &Option<String>, notes: &Option<String>) -> Option<String> {
    let text = format!(
        "{} {}",
        location.as_deref().unwrap_or(""),
        notes.as_deref().unwrap_or("")
    );

    // Common meeting URL patterns
    let patterns = [
        "zoom.us/j/",
        "meet.google.com/",
        "teams.microsoft.com/",
        "webex.com/",
        "whereby.com/",
    ];

    for pattern in patterns {
        if let Some(pos) = text.find(pattern) {
            // Find the start of the URL
            let start = text[..pos].rfind("http").unwrap_or(pos);
            // Find the end of the URL
            let end = text[pos..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '>' || c == '<')
                .map(|e| pos + e)
                .unwrap_or(text.len());

            return Some(text[start..end].to_string());
        }
    }

    None
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
        let description: *mut objc::runtime::Object = msg_send![nsstring, stringValue];
        if !description.is_null() {
            let utf8: *const i8 = msg_send![description, UTF8String];
            if !utf8.is_null() {
                return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
            }
        }
        return String::new();
    }

    // Fallback: try description
    let description: *mut objc::runtime::Object = msg_send![nsstring, description];
    if !description.is_null() {
        let utf8: *const i8 = msg_send![description, UTF8String];
        if !utf8.is_null() {
            return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
        }
    }

    String::new()
}

/// Helper to convert NSDate to chrono DateTime
#[cfg(target_os = "macos")]
unsafe fn nsdate_to_chrono(nsdate: *mut objc::runtime::Object) -> DateTime<Utc> {
    use objc::{msg_send, sel, sel_impl};

    if nsdate.is_null() {
        return Utc::now();
    }

    // Get seconds since reference date (Jan 1, 2001)
    let interval: f64 = msg_send![nsdate, timeIntervalSinceReferenceDate];
    // Convert to Unix timestamp (add seconds from 1970 to 2001)
    let unix_ts = interval + 978307200.0;

    Utc.timestamp_opt(unix_ts as i64, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = CalendarConfig::default();
        assert_eq!(config.lookahead_hours, 24);
        assert_eq!(config.lookbehind_hours, 2);
        assert!(config.excluded_calendars.contains(&"Birthdays".to_string()));
    }

    #[test]
    fn test_extract_meeting_url() {
        let location = Some("https://zoom.us/j/123456789".to_string());
        let notes = None;
        let url = extract_meeting_url(&location, &notes);
        assert!(url.is_some());
        assert!(url.unwrap().contains("zoom.us"));
    }

    #[test]
    fn test_extract_meeting_url_from_notes() {
        let location = None;
        let notes = Some("Join meeting: https://meet.google.com/abc-defg-hij".to_string());
        let url = extract_meeting_url(&location, &notes);
        assert!(url.is_some());
        assert!(url.unwrap().contains("meet.google.com"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_check_access() {
        // Just verify this doesn't crash
        let _ = CalendarClient::check_access();
    }
}
