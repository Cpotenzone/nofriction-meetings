// noFriction Meetings - State Builder
// Manages ScreenState accumulation and boundary detection
//
// Converts raw frame stream into stable UI states with:
// - Single keyframe per state
// - Duration tracking (start_ts, end_ts)
// - State type classification
// - Flags for motion/blur/scroll detection

use chrono::{DateTime, Utc};
use image::DynamicImage;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::dedupe_gate::{DedupConfig, DedupGate, DedupReason, DedupResult};

/// Configuration for state building
#[derive(Debug, Clone)]
pub struct StateConfig {
    /// Deduplication configuration
    pub dedup: DedupConfig,

    /// Minimum state duration before allowing new state (ms)
    pub min_state_duration_ms: u64,

    /// Maximum state duration before forcing checkpoint (ms)
    pub max_state_duration_ms: u64,

    /// Whether stateful capture is enabled
    pub enabled: bool,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            dedup: DedupConfig::default(),
            min_state_duration_ms: 500,
            max_state_duration_ms: 60000,
            enabled: true,
        }
    }
}

/// State type classification
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    TextDoc,
    Browser,
    Slide,
    Terminal,
    Video,
    Other,
}

impl Default for StateType {
    fn default() -> Self {
        Self::Other
    }
}

/// Flags for state characteristics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateFlags {
    pub high_motion: bool,
    pub blurry: bool,
    pub low_text: bool,
    pub scroll_like: bool,
}

/// A stable UI state (keyframe + duration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenState {
    pub state_id: String,
    pub meeting_id: String,
    pub start_ts: DateTime<Utc>,
    pub end_ts: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub phash: String,
    pub delta_score: f32,
    pub keyframe_path: Option<PathBuf>,
    pub state_type: StateType,
    pub flags: StateFlags,
}

impl ScreenState {
    /// Create a new state
    pub fn new(meeting_id: &str, start_ts: DateTime<Utc>, phash: String) -> Self {
        Self {
            state_id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            start_ts,
            end_ts: None,
            app_name: None,
            window_title: None,
            phash,
            delta_score: 0.0,
            keyframe_path: None,
            state_type: StateType::Other,
            flags: StateFlags::default(),
        }
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        self.end_ts
            .map(|end| (end - self.start_ts).num_milliseconds())
    }
}

/// Result of processing a frame
#[derive(Debug)]
pub enum FrameProcessResult {
    /// Frame was duplicate, state extended
    Extended {
        state_id: String,
        new_end_ts: DateTime<Utc>,
    },
    /// New state created
    NewState {
        completed_state: Option<ScreenState>,
        new_state_id: String,
    },
    /// Stateful capture disabled, pass through
    PassThrough,
}

/// State accumulator for tracking current state
struct StateAccumulator {
    current_state: Option<ScreenState>,
    pending_keyframe: Option<Arc<DynamicImage>>,
}

/// State builder for converting frames into states
pub struct StateBuilder {
    config: StateConfig,
    dedup_gate: Mutex<DedupGate>,
    accumulator: Mutex<StateAccumulator>,
    meeting_id: Mutex<Option<String>>,
}

impl StateBuilder {
    /// Create a new state builder with default config
    pub fn new() -> Self {
        Self::with_config(StateConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: StateConfig) -> Self {
        let dedup_gate = DedupGate::with_config(config.dedup.clone());

        Self {
            config,
            dedup_gate: Mutex::new(dedup_gate),
            accumulator: Mutex::new(StateAccumulator {
                current_state: None,
                pending_keyframe: None,
            }),
            meeting_id: Mutex::new(None),
        }
    }

    /// Start tracking a new meeting
    pub fn start_meeting(&self, meeting_id: &str) {
        *self.meeting_id.lock() = Some(meeting_id.to_string());
        self.dedup_gate.lock().reset();
        let mut acc = self.accumulator.lock();
        acc.current_state = None;
        acc.pending_keyframe = None;
    }

    /// End meeting and finalize current state
    pub fn end_meeting(&self) -> Option<ScreenState> {
        *self.meeting_id.lock() = None;
        self.finalize_current_state()
    }

    /// Process a frame and determine if it's a state boundary
    /// Returns the processing result with state information
    pub fn process_frame(
        &self,
        image: Arc<DynamicImage>,
        timestamp: DateTime<Utc>,
    ) -> FrameProcessResult {
        if !self.config.enabled {
            return FrameProcessResult::PassThrough;
        }

        let meeting_id = match self.meeting_id.lock().clone() {
            Some(id) => id,
            None => return FrameProcessResult::PassThrough,
        };

        // Run deduplication check
        let dedup_result = self.dedup_gate.lock().check_frame(&image);

        // Check for forced state boundary (max duration)
        let force_boundary = {
            let acc = self.accumulator.lock();
            if let Some(ref state) = acc.current_state {
                let duration = (timestamp - state.start_ts).num_milliseconds() as u64;
                duration >= self.config.max_state_duration_ms
            } else {
                false
            }
        };

        // Check for suppressed boundary (min duration)
        let suppress_boundary = {
            let acc = self.accumulator.lock();
            if let Some(ref state) = acc.current_state {
                let duration = (timestamp - state.start_ts).num_milliseconds() as u64;
                duration < self.config.min_state_duration_ms
            } else {
                false
            }
        };

        // Decision: extend or new state?
        let is_boundary = if force_boundary {
            true
        } else if suppress_boundary {
            false
        } else {
            !dedup_result.is_duplicate
        };

        if is_boundary {
            // State boundary - finalize current and start new
            let completed = self.finalize_current_state();
            let new_state_id = self.open_new_state(&meeting_id, timestamp, image, &dedup_result);

            FrameProcessResult::NewState {
                completed_state: completed,
                new_state_id,
            }
        } else {
            // Extend current state
            let mut acc = self.accumulator.lock();
            if let Some(ref mut state) = acc.current_state {
                state.end_ts = Some(timestamp);

                // Update flags based on dedup reason
                match dedup_result.reason {
                    DedupReason::MotionNoise => {
                        state.flags.high_motion = true;
                    }
                    DedupReason::DeltaSimilar => {
                        // Possibly scroll-like but need text comparison
                    }
                    _ => {}
                }

                FrameProcessResult::Extended {
                    state_id: state.state_id.clone(),
                    new_end_ts: timestamp,
                }
            } else {
                // No current state, start one
                drop(acc);
                let new_state_id =
                    self.open_new_state(&meeting_id, timestamp, image, &dedup_result);

                FrameProcessResult::NewState {
                    completed_state: None,
                    new_state_id,
                }
            }
        }
    }

    /// Get the current pending keyframe (for saving)
    pub fn take_pending_keyframe(&self) -> Option<Arc<DynamicImage>> {
        self.accumulator.lock().pending_keyframe.take()
    }

    /// Get current state info (for monitoring)
    pub fn current_state_id(&self) -> Option<String> {
        self.accumulator
            .lock()
            .current_state
            .as_ref()
            .map(|s| s.state_id.clone())
    }

    /// Finalize and return the current state
    fn finalize_current_state(&self) -> Option<ScreenState> {
        let mut acc = self.accumulator.lock();

        if let Some(mut state) = acc.current_state.take() {
            // Ensure end_ts is set
            if state.end_ts.is_none() {
                state.end_ts = Some(Utc::now());
            }
            Some(state)
        } else {
            None
        }
    }

    /// Open a new state
    fn open_new_state(
        &self,
        meeting_id: &str,
        timestamp: DateTime<Utc>,
        image: Arc<DynamicImage>,
        dedup_result: &DedupResult,
    ) -> String {
        let mut acc = self.accumulator.lock();

        let phash_str = DedupGate::hash_to_string(&dedup_result.ahash);
        let mut state = ScreenState::new(meeting_id, timestamp, phash_str);
        state.delta_score = dedup_result.delta_score;
        state.end_ts = Some(timestamp); // Initially same as start

        let state_id = state.state_id.clone();

        acc.current_state = Some(state);
        acc.pending_keyframe = Some(image);

        state_id
    }

    /// Update config at runtime
    pub fn update_config(&mut self, config: StateConfig) {
        self.dedup_gate.lock().reset();
        self.config = config;
    }
}

impl Default for StateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn create_test_image(seed: u8) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgb8(RgbImage::from_fn(
            100,
            100,
            |_, _| Rgb([seed, seed, seed]),
        )))
    }

    #[test]
    fn test_first_frame_creates_state() {
        let builder = StateBuilder::new();
        builder.start_meeting("test_meeting");

        let img = create_test_image(128);
        let result = builder.process_frame(img, Utc::now());

        match result {
            FrameProcessResult::NewState { new_state_id, .. } => {
                assert!(!new_state_id.is_empty());
            }
            _ => panic!("Expected NewState for first frame"),
        }
    }

    #[test]
    fn test_duplicate_frame_extends_state() {
        let builder = StateBuilder::new();
        builder.start_meeting("test_meeting");

        let img = create_test_image(128);

        // First frame -> new state
        builder.process_frame(img.clone(), Utc::now());

        // Same frame -> extend
        let result = builder.process_frame(img, Utc::now());

        match result {
            FrameProcessResult::Extended { .. } => {}
            _ => panic!("Expected Extended for duplicate frame"),
        }
    }
}
