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
use std::collections::HashMap;
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

/// One capture source's dedup gate + open state. Each display or window
/// being captured gets its own lane, so interleaved frames from different
/// sources don't look like constant change to each other.
struct Lane {
    gate: DedupGate,
    acc: StateAccumulator,
}

/// State builder for converting frames into states
pub struct StateBuilder {
    config: StateConfig,
    lanes: Mutex<HashMap<String, Lane>>,
    meeting_id: Mutex<Option<String>>,
}

impl StateBuilder {
    /// Create a new state builder with default config
    pub fn new() -> Self {
        Self::with_config(StateConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: StateConfig) -> Self {
        Self {
            config,
            lanes: Mutex::new(HashMap::new()),
            meeting_id: Mutex::new(None),
        }
    }

    /// Start tracking a new meeting
    pub fn start_meeting(&self, meeting_id: &str) {
        *self.meeting_id.lock() = Some(meeting_id.to_string());
        self.lanes.lock().clear();
    }

    /// The meeting currently being tracked, if any
    pub fn current_meeting_id(&self) -> Option<String> {
        self.meeting_id.lock().clone()
    }

    /// End meeting and finalize all open states (one per source)
    pub fn end_meeting(&self) -> Vec<ScreenState> {
        *self.meeting_id.lock() = None;
        let mut lanes = self.lanes.lock();
        let finished = lanes
            .values_mut()
            .filter_map(|lane| Self::finalize(&mut lane.acc))
            .collect();
        lanes.clear();
        finished
    }

    /// Process a frame from `source` (e.g. "display:1", "window:4242") and
    /// determine if it's a state boundary for that source.
    pub fn process_frame(
        &self,
        source: &str,
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

        let mut lanes = self.lanes.lock();
        let lane = lanes.entry(source.to_string()).or_insert_with(|| Lane {
            gate: DedupGate::with_config(self.config.dedup.clone()),
            acc: StateAccumulator {
                current_state: None,
                pending_keyframe: None,
            },
        });

        let dedup_result = lane.gate.check_frame(&image);

        let current_duration = lane
            .acc
            .current_state
            .as_ref()
            .map(|st| (timestamp - st.start_ts).num_milliseconds().max(0) as u64);

        let is_boundary = match current_duration {
            None => true,
            Some(d) if d >= self.config.max_state_duration_ms => true,
            Some(d) if d < self.config.min_state_duration_ms => false,
            Some(_) => !dedup_result.is_duplicate,
        };

        if is_boundary {
            let completed = Self::finalize(&mut lane.acc);
            let new_state_id = Self::open_new_state(&mut lane.acc, &meeting_id, timestamp, image, &dedup_result);
            FrameProcessResult::NewState {
                completed_state: completed,
                new_state_id,
            }
        } else {
            let state = lane
                .acc
                .current_state
                .as_mut()
                .expect("non-boundary implies an open state");
            state.end_ts = Some(timestamp);
            if matches!(dedup_result.reason, DedupReason::MotionNoise) {
                state.flags.high_motion = true;
            }
            FrameProcessResult::Extended {
                state_id: state.state_id.clone(),
                new_end_ts: timestamp,
            }
        }
    }

    /// Take the pending keyframe for `source` (for saving)
    pub fn take_pending_keyframe(&self, source: &str) -> Option<Arc<DynamicImage>> {
        self.lanes
            .lock()
            .get_mut(source)
            .and_then(|lane| lane.acc.pending_keyframe.take())
    }

    /// Current state ids across sources (for monitoring)
    pub fn current_state_ids(&self) -> Vec<String> {
        self.lanes
            .lock()
            .values()
            .filter_map(|l| l.acc.current_state.as_ref().map(|s| s.state_id.clone()))
            .collect()
    }

    fn finalize(acc: &mut StateAccumulator) -> Option<ScreenState> {
        acc.current_state.take().map(|mut state| {
            if state.end_ts.is_none() {
                state.end_ts = Some(Utc::now());
            }
            state
        })
    }

    fn open_new_state(
        acc: &mut StateAccumulator,
        meeting_id: &str,
        timestamp: DateTime<Utc>,
        image: Arc<DynamicImage>,
        dedup_result: &DedupResult,
    ) -> String {
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
        self.lanes.lock().clear();
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
        let result = builder.process_frame("display:1", img, Utc::now());

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
        builder.process_frame("display:1", img.clone(), Utc::now());

        // Same frame -> extend
        let result = builder.process_frame("display:1", img, Utc::now());

        match result {
            FrameProcessResult::Extended { .. } => {}
            _ => panic!("Expected Extended for duplicate frame"),
        }
    }

    #[test]
    fn test_sources_dedupe_independently() {
        let builder = StateBuilder::new();
        builder.start_meeting("test_meeting");
        let a = create_test_image(20);
        let b = create_test_image(230);
        let t = Utc::now();
        builder.process_frame("display:1", a.clone(), t);
        builder.process_frame("window:7", b.clone(), t);
        // Interleaved but unchanged per source → both extend
        let later = t + chrono::Duration::seconds(2);
        assert!(matches!(
            builder.process_frame("display:1", a, later),
            FrameProcessResult::Extended { .. }
        ));
        assert!(matches!(
            builder.process_frame("window:7", b, later),
            FrameProcessResult::Extended { .. }
        ));
        assert_eq!(builder.end_meeting().len(), 2);
    }
}
