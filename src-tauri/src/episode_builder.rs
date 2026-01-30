// noFriction Meetings - Episode Builder
// Groups ScreenStates into DocumentEpisodes based on app/window continuity
//
// An episode represents continuous focus on a single document/window.
// Episode boundaries are created when:
// 1. App name changes
// 2. Window title changes significantly
// 3. Maximum episode duration exceeded

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state_builder::ScreenState;

/// Configuration for episode building
#[derive(Debug, Clone)]
pub struct EpisodeConfig {
    /// Maximum episode duration before forcing boundary (ms)
    pub max_episode_duration_ms: u64,

    /// Whether to treat window title changes as episode boundaries
    pub title_change_is_boundary: bool,

    /// Minimum similarity for window titles to be considered same episode
    pub title_similarity_threshold: f32,
}

impl Default for EpisodeConfig {
    fn default() -> Self {
        Self {
            max_episode_duration_ms: 300_000, // 5 minutes
            title_change_is_boundary: true,
            title_similarity_threshold: 0.8,
        }
    }
}

/// Document episode representing continuous focus on same app/window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEpisode {
    pub episode_id: String,
    pub meeting_id: String,
    pub start_ts: DateTime<Utc>,
    pub end_ts: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub document_fingerprint: Option<String>,
    pub state_ids: Vec<String>,
    pub state_count: i32,
}

impl DocumentEpisode {
    pub fn new(
        meeting_id: &str,
        start_ts: DateTime<Utc>,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Self {
        Self {
            episode_id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            start_ts,
            end_ts: None,
            app_name: app_name.map(String::from),
            window_title: window_title.map(String::from),
            document_fingerprint: None,
            state_ids: Vec::new(),
            state_count: 0,
        }
    }

    /// Duration in milliseconds
    pub fn duration_ms(&self) -> i64 {
        match self.end_ts {
            Some(end) => (end - self.start_ts).num_milliseconds(),
            None => 0,
        }
    }
}

/// Result of processing a state
#[derive(Debug)]
pub enum EpisodeProcessResult {
    /// State added to current episode
    Extended { episode_id: String },
    /// New episode started
    NewEpisode {
        completed_episode: Option<DocumentEpisode>,
        new_episode_id: String,
    },
    /// No active meeting
    Inactive,
}

/// Accumulator for current episode state
struct EpisodeAccumulator {
    current_episode: Option<DocumentEpisode>,
    sequence_num: i32,
}

/// Episode builder for grouping states
pub struct EpisodeBuilder {
    config: EpisodeConfig,
    meeting_id: Mutex<Option<String>>,
    accumulator: Mutex<EpisodeAccumulator>,
}

impl EpisodeBuilder {
    pub fn new() -> Self {
        Self::with_config(EpisodeConfig::default())
    }

    pub fn with_config(config: EpisodeConfig) -> Self {
        Self {
            config,
            meeting_id: Mutex::new(None),
            accumulator: Mutex::new(EpisodeAccumulator {
                current_episode: None,
                sequence_num: 0,
            }),
        }
    }

    /// Start tracking a new meeting
    pub fn start_meeting(&self, meeting_id: &str) {
        *self.meeting_id.lock() = Some(meeting_id.to_string());
        let mut acc = self.accumulator.lock();
        acc.current_episode = None;
        acc.sequence_num = 0;
    }

    /// End meeting and finalize current episode
    pub fn end_meeting(&self) -> Option<DocumentEpisode> {
        *self.meeting_id.lock() = None;
        self.finalize_current_episode()
    }

    /// Finalize all episodes and return them (for batch save at end of meeting)
    pub fn finalize_all(&self) -> Vec<DocumentEpisode> {
        let mut episodes = Vec::new();
        if let Some(episode) = self.finalize_current_episode() {
            episodes.push(episode);
        }
        *self.meeting_id.lock() = None;
        episodes
    }

    /// Process a new screen state
    pub fn process_state(&self, state: &ScreenState) -> EpisodeProcessResult {
        let meeting_id = match self.meeting_id.lock().clone() {
            Some(id) => id,
            None => return EpisodeProcessResult::Inactive,
        };

        // Check if we need to start a new episode
        let should_start_new = {
            let acc = self.accumulator.lock();
            match &acc.current_episode {
                None => true,
                Some(episode) => {
                    // Check for app change
                    if self.app_changed(episode, state) {
                        true
                    }
                    // Check for significant title change
                    else if self.config.title_change_is_boundary
                        && self.title_changed(episode, state)
                    {
                        true
                    }
                    // Check for max duration
                    else if self.max_duration_exceeded(episode, state) {
                        true
                    } else {
                        false
                    }
                }
            }
        };

        if should_start_new {
            // Finalize current and start new
            let completed = self.finalize_current_episode();
            let new_episode_id = self.open_new_episode(
                &meeting_id,
                state.start_ts,
                state.app_name.as_deref(),
                state.window_title.as_deref(),
            );

            // Link state to new episode
            self.add_state_to_episode(&state.state_id, state.end_ts.unwrap_or(state.start_ts));

            EpisodeProcessResult::NewEpisode {
                completed_episode: completed,
                new_episode_id,
            }
        } else {
            // Extend current episode
            self.add_state_to_episode(&state.state_id, state.end_ts.unwrap_or(state.start_ts));

            let episode_id = {
                let acc = self.accumulator.lock();
                acc.current_episode.as_ref().map(|e| e.episode_id.clone())
            };

            EpisodeProcessResult::Extended {
                episode_id: episode_id.unwrap_or_default(),
            }
        }
    }

    /// Get current episode ID
    pub fn current_episode_id(&self) -> Option<String> {
        self.accumulator
            .lock()
            .current_episode
            .as_ref()
            .map(|e| e.episode_id.clone())
    }

    /// Check if app name changed
    fn app_changed(&self, episode: &DocumentEpisode, state: &ScreenState) -> bool {
        match (&episode.app_name, &state.app_name) {
            (Some(ep_app), Some(st_app)) => ep_app != st_app,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        }
    }

    /// Check if window title changed significantly
    fn title_changed(&self, episode: &DocumentEpisode, state: &ScreenState) -> bool {
        match (&episode.window_title, &state.window_title) {
            (Some(ep_title), Some(st_title)) => {
                // Simple comparison - could use Levenshtein or similar
                let similarity = self.title_similarity(ep_title, st_title);
                similarity < self.config.title_similarity_threshold
            }
            (None, Some(_)) => false, // Going from no title to having one isn't boundary
            (Some(_), None) => false, // Going from title to no title isn't boundary
            (None, None) => false,
        }
    }

    /// Simple title similarity (could be enhanced with edit distance)
    fn title_similarity(&self, a: &str, b: &str) -> f32 {
        if a == b {
            return 1.0;
        }

        // Simple word overlap for now
        let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
        let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();

        if a_words.is_empty() || b_words.is_empty() {
            return if a.is_empty() && b.is_empty() {
                1.0
            } else {
                0.0
            };
        }

        let intersection = a_words.intersection(&b_words).count();
        let union = a_words.union(&b_words).count();

        intersection as f32 / union as f32
    }

    /// Check if max duration exceeded
    fn max_duration_exceeded(&self, episode: &DocumentEpisode, state: &ScreenState) -> bool {
        let duration = (state.start_ts - episode.start_ts).num_milliseconds() as u64;
        duration >= self.config.max_episode_duration_ms
    }

    /// Finalize current episode
    fn finalize_current_episode(&self) -> Option<DocumentEpisode> {
        let mut acc = self.accumulator.lock();
        acc.current_episode.take()
    }

    /// Open a new episode
    fn open_new_episode(
        &self,
        meeting_id: &str,
        start_ts: DateTime<Utc>,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> String {
        let mut acc = self.accumulator.lock();

        let episode = DocumentEpisode::new(meeting_id, start_ts, app_name, window_title);
        let episode_id = episode.episode_id.clone();

        acc.current_episode = Some(episode);
        acc.sequence_num = 0;

        episode_id
    }

    /// Add state to current episode
    fn add_state_to_episode(&self, state_id: &str, end_ts: DateTime<Utc>) {
        let mut acc = self.accumulator.lock();
        if let Some(ref mut episode) = acc.current_episode {
            episode.state_ids.push(state_id.to_string());
            episode.state_count += 1;
            episode.end_ts = Some(end_ts);
            acc.sequence_num += 1;
        }
    }
}

impl Default for EpisodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state(meeting_id: &str, app: Option<&str>, title: Option<&str>) -> ScreenState {
        use crate::state_builder::{StateFlags, StateType};

        ScreenState {
            state_id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            start_ts: Utc::now(),
            end_ts: Some(Utc::now()),
            app_name: app.map(String::from),
            window_title: title.map(String::from),
            phash: "test".to_string(),
            delta_score: 0.0,
            keyframe_path: None,
            state_type: StateType::Other,
            flags: StateFlags::default(),
        }
    }

    #[test]
    fn test_first_state_creates_episode() {
        let builder = EpisodeBuilder::new();
        builder.start_meeting("test_meeting");

        let state = create_test_state("test_meeting", Some("VSCode"), Some("main.rs"));
        let result = builder.process_state(&state);

        match result {
            EpisodeProcessResult::NewEpisode { new_episode_id, .. } => {
                assert!(!new_episode_id.is_empty());
            }
            _ => panic!("Expected NewEpisode"),
        }
    }

    #[test]
    fn test_same_app_extends_episode() {
        let builder = EpisodeBuilder::new();
        builder.start_meeting("test_meeting");

        let state1 = create_test_state("test_meeting", Some("VSCode"), Some("main.rs"));
        let state2 = create_test_state("test_meeting", Some("VSCode"), Some("main.rs"));

        builder.process_state(&state1);
        let result = builder.process_state(&state2);

        match result {
            EpisodeProcessResult::Extended { .. } => {}
            _ => panic!("Expected Extended"),
        }
    }

    #[test]
    fn test_app_change_creates_new_episode() {
        let builder = EpisodeBuilder::new();
        builder.start_meeting("test_meeting");

        let state1 = create_test_state("test_meeting", Some("VSCode"), Some("main.rs"));
        let state2 = create_test_state("test_meeting", Some("Chrome"), Some("Google"));

        builder.process_state(&state1);
        let result = builder.process_state(&state2);

        match result {
            EpisodeProcessResult::NewEpisode {
                completed_episode, ..
            } => {
                assert!(completed_episode.is_some());
            }
            _ => panic!("Expected NewEpisode"),
        }
    }
}
