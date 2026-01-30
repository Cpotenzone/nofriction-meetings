// noFriction Meetings - Timeline Builder
// Generates meeting timeline events from episodes and patches
//
// This module provides:
// 1. Topic clustering from episodes
// 2. Timeline event generation
// 3. Evidence reference linking
// 4. Summary generation for timeline segments

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::diff_builder::ChangeType;
use crate::episode_builder::DocumentEpisode;

/// Configuration for timeline building
#[derive(Debug, Clone)]
pub struct TimelineConfig {
    /// Minimum episode duration to include in timeline (ms)
    pub min_episode_duration_ms: i64,

    /// Gap threshold to split timeline segments (ms)
    pub segment_gap_threshold_ms: i64,

    /// Whether to include file-level events
    pub include_file_events: bool,

    /// Whether to include app switch events
    pub include_app_switches: bool,

    /// Whether to include content change events
    pub include_content_changes: bool,
}

impl Default for TimelineConfig {
    fn default() -> Self {
        Self {
            min_episode_duration_ms: 5_000,   // 5 seconds
            segment_gap_threshold_ms: 60_000, // 1 minute
            include_file_events: true,
            include_app_switches: true,
            include_content_changes: true,
        }
    }
}

/// Type of timeline event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventType {
    /// Started working on a document/file
    DocumentOpened,
    /// Finished working on a document
    DocumentClosed,
    /// Switched to a different application
    AppSwitch,
    /// Made content changes
    ContentEdit,
    /// Scrolled/navigated within document
    Navigation,
    /// Meeting started
    MeetingStart,
    /// Meeting ended
    MeetingEnd,
    /// Topic/focus changed
    TopicChange,
    /// Activity gap detected
    ActivityGap,
}

impl TimelineEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DocumentOpened => "document_opened",
            Self::DocumentClosed => "document_closed",
            Self::AppSwitch => "app_switch",
            Self::ContentEdit => "content_edit",
            Self::Navigation => "navigation",
            Self::MeetingStart => "meeting_start",
            Self::MeetingEnd => "meeting_end",
            Self::TopicChange => "topic_change",
            Self::ActivityGap => "activity_gap",
        }
    }

    /// Get a human-readable label
    pub fn label(&self) -> &'static str {
        match self {
            Self::DocumentOpened => "Opened",
            Self::DocumentClosed => "Closed",
            Self::AppSwitch => "Switched App",
            Self::ContentEdit => "Edited",
            Self::Navigation => "Navigated",
            Self::MeetingStart => "Meeting Started",
            Self::MeetingEnd => "Meeting Ended",
            Self::TopicChange => "Topic Changed",
            Self::ActivityGap => "Break",
        }
    }
}

/// A timeline event for the meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub event_id: String,
    pub meeting_id: String,
    pub ts: DateTime<Utc>,
    pub event_type: TimelineEventType,
    pub title: String,
    pub description: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub duration_ms: Option<i64>,
    /// Reference to source episode
    pub episode_id: Option<String>,
    /// Reference to source state for jump-to
    pub state_id: Option<String>,
    /// Topic/category for grouping
    pub topic: Option<String>,
    /// Importance score (0.0-1.0)
    pub importance: f32,
}

impl TimelineEvent {
    pub fn new(
        meeting_id: &str,
        ts: DateTime<Utc>,
        event_type: TimelineEventType,
        title: String,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            ts,
            event_type,
            title,
            description: None,
            app_name: None,
            window_title: None,
            duration_ms: None,
            episode_id: None,
            state_id: None,
            topic: None,
            importance: 0.5,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_app(mut self, app: &str) -> Self {
        self.app_name = Some(app.to_string());
        self
    }

    pub fn with_window(mut self, title: &str) -> Self {
        self.window_title = Some(title.to_string());
        self
    }

    pub fn with_duration(mut self, duration_ms: i64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_episode(mut self, episode_id: &str) -> Self {
        self.episode_id = Some(episode_id.to_string());
        self
    }

    pub fn with_state(mut self, state_id: &str) -> Self {
        self.state_id = Some(state_id.to_string());
        self
    }

    pub fn with_topic(mut self, topic: &str) -> Self {
        self.topic = Some(topic.to_string());
        self
    }

    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }
}

/// Topic information for clustering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicCluster {
    pub topic_id: String,
    pub name: String,
    pub description: Option<String>,
    pub start_ts: DateTime<Utc>,
    pub end_ts: Option<DateTime<Utc>>,
    pub episode_ids: Vec<String>,
    pub event_count: i32,
    pub total_duration_ms: i64,
}

/// Timeline accumulator state
struct TimelineAccumulator {
    events: Vec<TimelineEvent>,
    topics: HashMap<String, TopicCluster>,
    last_app: Option<String>,
    last_event_ts: Option<DateTime<Utc>>,
}

/// Timeline builder for generating meeting timelines
pub struct TimelineBuilder {
    config: TimelineConfig,
    meeting_id: Mutex<Option<String>>,
    accumulator: Mutex<TimelineAccumulator>,
}

impl TimelineBuilder {
    pub fn new() -> Self {
        Self::with_config(TimelineConfig::default())
    }

    pub fn with_config(config: TimelineConfig) -> Self {
        Self {
            config,
            meeting_id: Mutex::new(None),
            accumulator: Mutex::new(TimelineAccumulator {
                events: Vec::new(),
                topics: HashMap::new(),
                last_app: None,
                last_event_ts: None,
            }),
        }
    }

    /// Start building timeline for a meeting
    pub fn start_meeting(&self, meeting_id: &str, start_ts: DateTime<Utc>) {
        *self.meeting_id.lock() = Some(meeting_id.to_string());

        let mut acc = self.accumulator.lock();
        acc.events.clear();
        acc.topics.clear();
        acc.last_app = None;
        acc.last_event_ts = Some(start_ts);

        // Add meeting start event
        let event = TimelineEvent::new(
            meeting_id,
            start_ts,
            TimelineEventType::MeetingStart,
            "Meeting Started".to_string(),
        )
        .with_importance(1.0);

        acc.events.push(event);
    }

    /// End the meeting and finalize timeline
    pub fn end_meeting(&self, end_ts: DateTime<Utc>) -> Vec<TimelineEvent> {
        let meeting_id = match self.meeting_id.lock().clone() {
            Some(id) => id,
            None => return Vec::new(),
        };

        let mut acc = self.accumulator.lock();

        // Add meeting end event
        let event = TimelineEvent::new(
            &meeting_id,
            end_ts,
            TimelineEventType::MeetingEnd,
            "Meeting Ended".to_string(),
        )
        .with_importance(1.0);

        acc.events.push(event);

        // Return copy of events
        acc.events.clone()
    }

    /// Process an episode and generate timeline events
    pub fn process_episode(&self, episode: &DocumentEpisode) -> Vec<TimelineEvent> {
        let meeting_id = match self.meeting_id.lock().clone() {
            Some(id) => id,
            None => return Vec::new(),
        };

        let duration_ms = episode.duration_ms();

        // Skip short episodes
        if duration_ms < self.config.min_episode_duration_ms {
            return Vec::new();
        }

        let mut new_events = Vec::new();
        let mut acc = self.accumulator.lock();

        // Check for activity gap
        if let Some(last_ts) = acc.last_event_ts {
            let gap_ms = (episode.start_ts - last_ts).num_milliseconds();
            if gap_ms > self.config.segment_gap_threshold_ms {
                let gap_event = TimelineEvent::new(
                    &meeting_id,
                    last_ts,
                    TimelineEventType::ActivityGap,
                    format!("Break ({} min)", gap_ms / 60_000),
                )
                .with_duration(gap_ms)
                .with_importance(0.3);

                acc.events.push(gap_event.clone());
                new_events.push(gap_event);
            }
        }

        // Check for app switch
        if self.config.include_app_switches {
            if let Some(ref app_name) = episode.app_name {
                let app_changed = acc.last_app.as_ref() != Some(app_name);

                if app_changed {
                    let event = TimelineEvent::new(
                        &meeting_id,
                        episode.start_ts,
                        TimelineEventType::AppSwitch,
                        format!("Switched to {}", app_name),
                    )
                    .with_app(app_name)
                    .with_episode(&episode.episode_id)
                    .with_importance(0.6);

                    acc.events.push(event.clone());
                    new_events.push(event);
                    acc.last_app = Some(app_name.clone());
                }
            }
        }

        // Add document opened event
        if self.config.include_file_events {
            let title = self.extract_document_name(episode);
            let topic = self.infer_topic(episode);

            let mut event = TimelineEvent::new(
                &meeting_id,
                episode.start_ts,
                TimelineEventType::DocumentOpened,
                format!("Working on {}", title),
            )
            .with_duration(duration_ms)
            .with_episode(&episode.episode_id)
            .with_importance(self.calculate_importance(episode));

            if let Some(ref app) = episode.app_name {
                event = event.with_app(app);
            }
            if let Some(ref window) = episode.window_title {
                event = event.with_window(window);
            }
            if let Some(ref t) = topic {
                event = event.with_topic(t);
                self.update_topic_cluster(&mut acc, t, episode);
            }

            acc.events.push(event.clone());
            new_events.push(event);
        }

        acc.last_event_ts = episode.end_ts;

        new_events
    }

    /// Process a content change and generate timeline event
    pub fn process_change(
        &self,
        episode_id: &str,
        ts: DateTime<Utc>,
        change_type: ChangeType,
        lines_added: i32,
        lines_removed: i32,
    ) -> Option<TimelineEvent> {
        if !self.config.include_content_changes {
            return None;
        }

        let meeting_id = match self.meeting_id.lock().clone() {
            Some(id) => id,
            None => return None,
        };

        // Skip minor changes
        if lines_added == 0 && lines_removed == 0 {
            return None;
        }

        let (event_type, title, importance) = match change_type {
            ChangeType::ContentAdded => (
                TimelineEventType::ContentEdit,
                format!("Added {} lines", lines_added),
                0.7,
            ),
            ChangeType::ContentRemoved => (
                TimelineEventType::ContentEdit,
                format!("Removed {} lines", lines_removed),
                0.6,
            ),
            ChangeType::ContentChanged => (
                TimelineEventType::ContentEdit,
                format!("+{} -{} lines", lines_added, lines_removed),
                0.5,
            ),
            ChangeType::ScrollOnly | ChangeType::CursorOnly => {
                (TimelineEventType::Navigation, "Navigated".to_string(), 0.2)
            }
            ChangeType::NewDocument => (
                TimelineEventType::DocumentOpened,
                "Opened document".to_string(),
                0.8,
            ),
            _ => (
                TimelineEventType::ContentEdit,
                "Made changes".to_string(),
                0.4,
            ),
        };

        let event = TimelineEvent::new(&meeting_id, ts, event_type, title)
            .with_episode(episode_id)
            .with_importance(importance);

        let mut acc = self.accumulator.lock();
        acc.events.push(event.clone());

        Some(event)
    }

    /// Get all timeline events
    pub fn get_events(&self) -> Vec<TimelineEvent> {
        self.accumulator.lock().events.clone()
    }

    /// Get topic clusters
    pub fn get_topics(&self) -> Vec<TopicCluster> {
        self.accumulator.lock().topics.values().cloned().collect()
    }

    /// Extract document name from window title
    fn extract_document_name(&self, episode: &DocumentEpisode) -> String {
        if let Some(ref title) = episode.window_title {
            // Common patterns: "filename.ext - AppName" or "AppName - filename"
            let parts: Vec<&str> = title.split(" - ").collect();

            if parts.len() >= 2 {
                // Usually filename is shorter part
                let first = parts[0].trim();
                let last = parts.last().unwrap().trim();

                if first.contains('.') || first.len() < last.len() {
                    return first.to_string();
                } else {
                    return last.to_string();
                }
            }

            return title.clone();
        }

        episode
            .app_name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string())
    }

    /// Infer topic from window title and app
    fn infer_topic(&self, episode: &DocumentEpisode) -> Option<String> {
        let app = episode.app_name.as_deref().unwrap_or("");
        let title = episode.window_title.as_deref().unwrap_or("");

        // Code-related
        if app.contains("Code")
            || app.contains("IDE")
            || title.ends_with(".rs")
            || title.ends_with(".ts")
            || title.ends_with(".py")
            || title.ends_with(".js")
        {
            return Some("Coding".to_string());
        }

        // Documentation
        if app.contains("Notion")
            || app.contains("Confluence")
            || title.contains("README")
            || title.contains("Docs")
        {
            return Some("Documentation".to_string());
        }

        // Communication
        if app.contains("Slack")
            || app.contains("Teams")
            || app.contains("Discord")
            || app.contains("Messages")
        {
            return Some("Communication".to_string());
        }

        // Browsing
        if app.contains("Chrome")
            || app.contains("Safari")
            || app.contains("Firefox")
            || app.contains("Arc")
        {
            return Some("Research".to_string());
        }

        // Terminal
        if app.contains("Terminal") || app.contains("iTerm") || app.contains("Warp") {
            return Some("Terminal".to_string());
        }

        None
    }

    /// Calculate importance based on episode characteristics
    fn calculate_importance(&self, episode: &DocumentEpisode) -> f32 {
        let mut importance = 0.5f32;

        // Longer episodes are more important
        let duration_min = episode.duration_ms() as f32 / 60_000.0;
        importance += (duration_min / 10.0).min(0.3);

        // More states = more activity
        if episode.state_count > 5 {
            importance += 0.1;
        }

        // Code files are important
        if let Some(ref title) = episode.window_title {
            if title.contains(".rs") || title.contains(".ts") || title.contains(".py") {
                importance += 0.1;
            }
        }

        importance.min(1.0)
    }

    /// Update topic cluster with episode
    fn update_topic_cluster(
        &self,
        acc: &mut TimelineAccumulator,
        topic_name: &str,
        episode: &DocumentEpisode,
    ) {
        let cluster = acc
            .topics
            .entry(topic_name.to_string())
            .or_insert_with(|| TopicCluster {
                topic_id: Uuid::new_v4().to_string(),
                name: topic_name.to_string(),
                description: None,
                start_ts: episode.start_ts,
                end_ts: episode.end_ts,
                episode_ids: Vec::new(),
                event_count: 0,
                total_duration_ms: 0,
            });

        cluster.episode_ids.push(episode.episode_id.clone());
        cluster.event_count += 1;
        cluster.total_duration_ms += episode.duration_ms();

        // Update end time if newer
        if let Some(ep_end) = episode.end_ts {
            if cluster.end_ts.is_none() || ep_end > cluster.end_ts.unwrap() {
                cluster.end_ts = Some(ep_end);
            }
        }
    }

    /// Reset the builder
    pub fn reset(&self) {
        *self.meeting_id.lock() = None;
        let mut acc = self.accumulator.lock();
        acc.events.clear();
        acc.topics.clear();
        acc.last_app = None;
        acc.last_event_ts = None;
    }
}

impl Default for TimelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_episode(app: &str, title: &str, duration_secs: i64) -> DocumentEpisode {
        let start = Utc::now();
        DocumentEpisode {
            episode_id: Uuid::new_v4().to_string(),
            meeting_id: "test_meeting".to_string(),
            start_ts: start,
            end_ts: Some(start + Duration::seconds(duration_secs)),
            app_name: Some(app.to_string()),
            window_title: Some(title.to_string()),
            document_fingerprint: None,
            state_ids: vec!["state1".to_string()],
            state_count: 1,
        }
    }

    #[test]
    fn test_meeting_lifecycle() {
        let builder = TimelineBuilder::new();

        builder.start_meeting("test_meeting", Utc::now());
        let events = builder.end_meeting(Utc::now());

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, TimelineEventType::MeetingStart);
        assert_eq!(events[1].event_type, TimelineEventType::MeetingEnd);
    }

    #[test]
    fn test_episode_generates_events() {
        let builder = TimelineBuilder::new();
        builder.start_meeting("test_meeting", Utc::now());

        let episode = create_test_episode("Visual Studio Code", "main.rs - nofriction", 60);
        let events = builder.process_episode(&episode);

        // Should generate at least one event (document opened)
        assert!(!events.is_empty());
    }

    #[test]
    fn test_topic_inference() {
        let builder = TimelineBuilder::new();
        builder.start_meeting("test_meeting", Utc::now());

        // Code episode
        let code_episode = create_test_episode("Visual Studio Code", "main.rs", 60);
        builder.process_episode(&code_episode);

        let topics = builder.get_topics();
        assert!(topics.iter().any(|t| t.name == "Coding"));
    }

    #[test]
    fn test_app_switch_detection() {
        let builder = TimelineBuilder::new();
        builder.start_meeting("test_meeting", Utc::now());

        // First app
        let ep1 = create_test_episode("Visual Studio Code", "main.rs", 30);
        builder.process_episode(&ep1);

        // Switch to different app
        let ep2 = create_test_episode("Chrome", "Google", 30);
        let events = builder.process_episode(&ep2);

        // Should detect app switch
        assert!(events
            .iter()
            .any(|e| e.event_type == TimelineEventType::AppSwitch));
    }
}
