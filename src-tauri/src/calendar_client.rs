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
    /// Attendees with display names (EKParticipant), organizer included
    #[serde(default)]
    pub participants: Vec<CalendarParticipant>,
}

/// A person on a calendar invite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarParticipant {
    pub email: String,
    /// Display name from the invite, when the calendar provides one
    pub name: Option<String>,
    pub is_organizer: bool,
    /// The calendar owner (you)
    pub is_self: bool,
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

/// `EKAuthorizationStatus` → our status. 3 is `Authorized` before macOS 14
/// and `FullAccess` from 14 (same value); 4 is `WriteOnly` (macOS 14+), which
/// can add events but can't read them, so it is not access for us.
pub fn status_from_ek(status: i64) -> CalendarAccessStatus {
    match status {
        0 => CalendarAccessStatus::NotDetermined,
        1 => CalendarAccessStatus::Restricted,
        2 => CalendarAccessStatus::Denied,
        3 => CalendarAccessStatus::Authorized,
        4 => CalendarAccessStatus::Denied,
        _ => CalendarAccessStatus::Unknown,
    }
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
        use objc::runtime::Class;
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let ek_class = match Class::get("EKEventStore") {
                Some(c) => c,
                None => return CalendarAccessStatus::Unknown,
            };

            // EKAuthorizationStatusForEntityType: 0 = Events
            let status: i64 = msg_send![ek_class, authorizationStatusForEntityType: 0i64];

            status_from_ek(status)
        }
    }

    /// Request calendar access (will prompt user)
    /// Uses a stateless no-op completion block + polling to avoid ObjC block copy crashes.
    /// The completion block passed to EventKit is copied via XPC to CalendarDaemon;
    /// embedding Rust state in the block causes crashes in _Block_copy.
    #[cfg(target_os = "macos")]
    pub async fn request_access() -> Result<bool, String> {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};
        use std::os::raw::c_void;

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

            // Stateless block — no captured data, safe for _Block_copy.
            // We poll check_access() for the result instead of using the callback.
            #[repr(C)]
            struct BlockLiteral {
                isa: *const c_void,
                flags: i32,
                reserved: i32,
                invoke: unsafe extern "C" fn(*mut BlockLiteral, bool, *mut Object),
                descriptor: *const BlockDescriptor,
            }

            #[repr(C)]
            struct BlockDescriptor {
                reserved: u64,
                size: u64,
            }

            static DESCRIPTOR: BlockDescriptor = BlockDescriptor {
                reserved: 0,
                size: std::mem::size_of::<BlockLiteral>() as u64,
            };

            unsafe extern "C" fn noop_invoke(
                _block: *mut BlockLiteral,
                _granted: bool,
                _error: *mut Object,
            ) {
                // No-op — result is obtained via polling check_access()
            }

            extern "C" {
                static _NSConcreteStackBlock: *const c_void;
            }

            let mut block = BlockLiteral {
                isa: _NSConcreteStackBlock,
                flags: 0, // No copy/dispose helpers needed (no captured state)
                reserved: 0,
                invoke: noop_invoke,
                descriptor: &DESCRIPTOR,
            };

            // Fire the permission request — macOS shows the consent dialog.
            // requestFullAccessToEventsWithCompletion: is macOS 14+; on
            // 12.3–13 it doesn't exist (an unrecognized selector aborts), so
            // use the older entity-type request there.
            let full_access: objc::runtime::BOOL =
                msg_send![store, respondsToSelector: sel!(requestFullAccessToEventsWithCompletion:)];
            if full_access == objc::runtime::YES {
                let _: () = msg_send![store, requestFullAccessToEventsWithCompletion: &mut block as *mut BlockLiteral as *mut c_void];
            } else {
                // EKEntityTypeEvent = 0
                let _: () = msg_send![store, requestAccessToEntityType: 0u64 completion: &mut block as *mut BlockLiteral as *mut c_void];
            }
        }

        // Poll for the user's response (permission dialog is async)
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            match Self::check_access() {
                CalendarAccessStatus::Authorized => return Ok(true),
                CalendarAccessStatus::Denied | CalendarAccessStatus::Restricted => {
                    return Ok(false)
                }
                _ => continue,
            }
        }

        // 30 second timeout — check one final time
        Ok(Self::check_access() == CalendarAccessStatus::Authorized)
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

    /// Internal event fetching from EventKit (around now)
    fn fetch_events_internal(&self) -> Result<Vec<CalendarEventNative>, String> {
        let now = Utc::now();
        self.fetch_events_between(
            now - Duration::hours(self.config.lookbehind_hours),
            now + Duration::hours(self.config.lookahead_hours),
        )
    }

    /// Fetch events in an arbitrary window (used to backfill past meetings).
    /// EventKit caps a single predicate at 4 years; callers pass far less.
    #[cfg(target_os = "macos")]
    pub fn fetch_events_between(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<CalendarEventNative>, String> {
        use objc::runtime::{Class, Object, BOOL, YES};
        use objc::{msg_send, sel, sel_impl};

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

                // Get attendees (EKParticipant: URL=mailto:, name, isCurrentUser)
                let read_participant = |p: *mut Object, is_organizer: bool| -> Option<CalendarParticipant> {
                    if p.is_null() {
                        return None;
                    }
                    let url: *mut Object = msg_send![p, URL];
                    if url.is_null() {
                        return None;
                    }
                    let raw: *mut Object = msg_send![url, absoluteString];
                    let raw = nsstring_to_rust(raw);
                    let email = raw.strip_prefix("mailto:").unwrap_or(&raw);
                    let email = urlencoding::decode(email).map(|e| e.into_owned()).unwrap_or_else(|_| email.to_string());
                    if email.is_empty() || !email.contains('@') {
                        return None;
                    }
                    let name_obj: *mut Object = msg_send![p, name];
                    let name = if name_obj.is_null() { String::new() } else { nsstring_to_rust(name_obj) };
                    let is_self: BOOL = msg_send![p, isCurrentUser];
                    Some(CalendarParticipant {
                        email: email.to_lowercase(),
                        // Some providers put the email in the name field
                        name: Some(name).filter(|n| !n.is_empty() && !n.contains('@')),
                        is_organizer,
                        is_self: is_self == YES,
                    })
                };

                let mut participants: Vec<CalendarParticipant> = Vec::new();
                let organizer_obj: *mut Object = msg_send![event, organizer];
                if let Some(org) = read_participant(organizer_obj, true) {
                    participants.push(org);
                }
                let attendees_array: *mut Object = msg_send![event, attendees];
                if !attendees_array.is_null() {
                    let att_count: usize = msg_send![attendees_array, count];
                    for j in 0..att_count {
                        let attendee: *mut Object = msg_send![attendees_array, objectAtIndex: j];
                        if let Some(p) = read_participant(attendee, false) {
                            if let Some(existing) = participants.iter_mut().find(|e| e.email == p.email) {
                                existing.name = existing.name.take().or(p.name);
                                existing.is_self |= p.is_self;
                            } else {
                                participants.push(p);
                            }
                        }
                    }
                }
                let attendees: Vec<String> = participants
                    .iter()
                    .filter(|p| !p.is_organizer || !p.is_self)
                    .map(|p| p.email.clone())
                    .collect();

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
                    participants,
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
    pub fn fetch_events_between(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEventNative>, String> {
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

    #[test]
    fn ek_status_mapping() {
        assert_eq!(status_from_ek(0), CalendarAccessStatus::NotDetermined);
        assert_eq!(status_from_ek(1), CalendarAccessStatus::Restricted);
        assert_eq!(status_from_ek(2), CalendarAccessStatus::Denied);
        assert_eq!(status_from_ek(3), CalendarAccessStatus::Authorized);
        // Write-only can't read events: not access
        assert_eq!(status_from_ek(4), CalendarAccessStatus::Denied);
        assert_eq!(status_from_ek(9), CalendarAccessStatus::Unknown);
    }

    /// calendar_client finds EKEventStore by name; EventKit is linked in
    /// build.rs so the class always resolves. Looking the class up doesn't
    /// touch calendar data or ask for access.
    #[cfg(target_os = "macos")]
    #[test]
    fn eventkit_is_linked() {
        assert!(objc::runtime::Class::get("EKEventStore").is_some());
    }
}
