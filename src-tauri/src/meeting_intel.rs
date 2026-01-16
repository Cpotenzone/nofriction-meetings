// Meeting Intelligence Module
// Provides state detection and intelligence generation for meetings

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Meeting mode determines UI and intelligence behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingMode {
    Pre,     // Before meeting starts
    Live,    // During active meeting
    CatchUp, // Joined late, need catch-up
}

impl Default for MeetingMode {
    fn default() -> Self {
        MeetingMode::Pre
    }
}

/// Current meeting state as resolved from multiple signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingState {
    pub meeting_id: Option<String>,
    pub mode: MeetingMode,
    pub minutes_since_start: i32,
    pub minutes_until_start: i32,
    pub confidence: f32,
    pub title: String,
    pub attendees: Vec<String>,
    pub is_transcript_running: bool,
    pub is_meeting_window_active: bool,
}

impl Default for MeetingState {
    fn default() -> Self {
        Self {
            meeting_id: None,
            mode: MeetingMode::Pre,
            minutes_since_start: 0,
            minutes_until_start: 0,
            confidence: 0.0,
            title: String::new(),
            attendees: Vec::new(),
            is_transcript_running: false,
            is_meeting_window_active: false,
        }
    }
}

/// Calendar event for meeting detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub attendees: Vec<String>,
    pub description: Option<String>,
    pub meeting_url: Option<String>,
}

/// Signal weights for meeting state resolution
#[derive(Debug, Clone)]
pub struct DetectionWeights {
    pub meeting_window: f32,
    pub transcript_running: f32,
    pub calendar_match: f32,
    pub audio_active: f32,
}

impl Default for DetectionWeights {
    fn default() -> Self {
        Self {
            meeting_window: 0.4,
            transcript_running: 0.3,
            calendar_match: 0.2,
            audio_active: 0.1,
        }
    }
}

/// Resolves meeting state from multiple signals
pub struct MeetingStateResolver {
    weights: DetectionWeights,
}

impl MeetingStateResolver {
    pub fn new() -> Self {
        Self {
            weights: DetectionWeights::default(),
        }
    }

    /// Resolve current meeting state
    pub fn resolve(
        &self,
        now: DateTime<Utc>,
        calendar_events: &[CalendarEvent],
        transcript_running: bool,
        active_window: Option<&str>,
        audio_active: bool,
    ) -> MeetingState {
        // Check if active window is a meeting app
        let is_meeting_window = active_window
            .map(|w| Self::is_meeting_app(w))
            .unwrap_or(false);

        // Find relevant calendar event
        let current_event = self.find_current_or_upcoming_event(now, calendar_events);

        // Calculate confidence score
        let mut confidence = 0.0;
        if is_meeting_window {
            confidence += self.weights.meeting_window;
        }
        if transcript_running {
            confidence += self.weights.transcript_running;
        }
        if current_event.is_some() {
            confidence += self.weights.calendar_match;
        }
        if audio_active {
            confidence += self.weights.audio_active;
        }

        // Determine timing
        let (minutes_since_start, minutes_until_start, meeting_active) =
            if let Some(ref event) = current_event {
                let since_start = (now - event.start_time).num_minutes() as i32;
                let until_start = (event.start_time - now).num_minutes() as i32;
                let is_active = now >= event.start_time && now <= event.end_time;
                (since_start.max(0), until_start.max(0), is_active)
            } else {
                (0, 0, false)
            };

        // Determine mode based on rules
        let mode = self.determine_mode(
            confidence,
            minutes_since_start,
            minutes_until_start,
            meeting_active,
            transcript_running,
        );

        MeetingState {
            meeting_id: current_event.as_ref().map(|e| e.id.clone()),
            mode,
            minutes_since_start,
            minutes_until_start,
            confidence,
            title: current_event
                .as_ref()
                .map(|e| e.title.clone())
                .unwrap_or_default(),
            attendees: current_event
                .as_ref()
                .map(|e| e.attendees.clone())
                .unwrap_or_default(),
            is_transcript_running: transcript_running,
            is_meeting_window_active: is_meeting_window,
        }
    }

    /// Determine meeting mode based on signals
    fn determine_mode(
        &self,
        confidence: f32,
        minutes_since_start: i32,
        minutes_until_start: i32,
        meeting_active: bool,
        transcript_running: bool,
    ) -> MeetingMode {
        // If meeting is active and we're >2 minutes in → CATCH_UP by default
        if (confidence >= 0.5 || meeting_active) && minutes_since_start >= 2 {
            return MeetingMode::CatchUp;
        }

        // If meeting is actively running
        if confidence >= 0.5 || transcript_running {
            return MeetingMode::Live;
        }

        // If meeting starts in next 30 minutes
        if minutes_until_start > 0 && minutes_until_start <= 30 {
            return MeetingMode::Pre;
        }

        // Default to Pre
        MeetingMode::Pre
    }

    /// Check if window name indicates a meeting app
    fn is_meeting_app(window_name: &str) -> bool {
        let lower = window_name.to_lowercase();
        lower.contains("zoom")
            || lower.contains("teams")
            || lower.contains("meet")
            || lower.contains("webex")
            || lower.contains("slack huddle")
            || lower.contains("facetime")
    }

    /// Find current or upcoming calendar event
    fn find_current_or_upcoming_event(
        &self,
        now: DateTime<Utc>,
        events: &[CalendarEvent],
    ) -> Option<CalendarEvent> {
        // First check for currently active meeting
        for event in events {
            if now >= event.start_time && now <= event.end_time {
                return Some(event.clone());
            }
        }

        // Then check for upcoming meeting in next 30 minutes
        let threshold = now + Duration::minutes(30);
        let mut upcoming: Option<&CalendarEvent> = None;

        for event in events {
            if event.start_time > now && event.start_time <= threshold {
                if upcoming.is_none() || event.start_time < upcoming.unwrap().start_time {
                    upcoming = Some(event);
                }
            }
        }

        upcoming.cloned()
    }
}

impl Default for MeetingStateResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meeting_app_detection() {
        assert!(MeetingStateResolver::is_meeting_app("Zoom Meeting"));
        assert!(MeetingStateResolver::is_meeting_app("Microsoft Teams"));
        assert!(MeetingStateResolver::is_meeting_app(
            "Google Meet - Meeting"
        ));
        assert!(!MeetingStateResolver::is_meeting_app("VS Code"));
        assert!(!MeetingStateResolver::is_meeting_app("Safari"));
    }
}
