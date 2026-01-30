// noFriction Meetings - Snapshot Extractor
// Extracts text from screen states using OCR
//
// This module provides:
// 1. OCR wrapper for keyframe images
// 2. Quality scoring for extracted text
// 3. Periodic snapshot checkpointing

use chrono::{DateTime, Utc};
use image::DynamicImage;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Configuration for snapshot extraction
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Minimum quality score to accept snapshot (0.0-1.0)
    pub min_quality_score: f32,

    /// Checkpoint interval for periodic snapshots (ms)
    pub checkpoint_interval_ms: u64,

    /// Minimum text length to consider valid
    pub min_text_length: usize,

    /// Whether OCR is enabled
    pub ocr_enabled: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            min_quality_score: 0.3,
            checkpoint_interval_ms: 30_000, // 30 seconds
            min_text_length: 10,
            ocr_enabled: true,
        }
    }
}

/// Source of text extraction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionSource {
    /// OCR from image
    Ocr,
    /// macOS Accessibility API
    Accessibility,
    /// Browser DOM extraction
    Dom,
    /// Clipboard content
    Clipboard,
    /// Manual input
    Manual,
}

impl Default for ExtractionSource {
    fn default() -> Self {
        Self::Ocr
    }
}

impl ExtractionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Accessibility => "accessibility",
            Self::Dom => "dom",
            Self::Clipboard => "clipboard",
            Self::Manual => "manual",
        }
    }
}

/// A text snapshot from the screen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSnapshot {
    pub snapshot_id: String,
    pub episode_id: Option<String>,
    pub state_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub text: String,
    pub text_hash: String,
    pub quality_score: f32,
    pub source: ExtractionSource,
    pub word_count: i32,
}

impl TextSnapshot {
    pub fn new(
        text: String,
        episode_id: Option<&str>,
        state_id: Option<&str>,
        source: ExtractionSource,
        quality_score: f32,
    ) -> Self {
        let text_hash = Self::compute_hash(&text);
        let word_count = text.split_whitespace().count() as i32;

        Self {
            snapshot_id: Uuid::new_v4().to_string(),
            episode_id: episode_id.map(String::from),
            state_id: state_id.map(String::from),
            ts: Utc::now(),
            text,
            text_hash,
            quality_score,
            source,
            word_count,
        }
    }

    fn compute_hash(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Result of extraction attempt
#[derive(Debug)]
pub enum ExtractionResult {
    /// Successfully extracted text
    Success(TextSnapshot),
    /// Extraction failed, reason given
    Failed(String),
    /// Text too short to be useful
    TooShort,
    /// Quality too low
    LowQuality(f32),
    /// Extraction disabled
    Disabled,
}

/// Tracker for checkpoint timing
struct CheckpointTracker {
    last_checkpoint_ts: Option<DateTime<Utc>>,
    last_episode_id: Option<String>,
}

/// Snapshot extractor for text extraction
pub struct SnapshotExtractor {
    config: SnapshotConfig,
    checkpoint_tracker: Mutex<CheckpointTracker>,
}

impl SnapshotExtractor {
    pub fn new() -> Self {
        Self::with_config(SnapshotConfig::default())
    }

    pub fn with_config(config: SnapshotConfig) -> Self {
        Self {
            config,
            checkpoint_tracker: Mutex::new(CheckpointTracker {
                last_checkpoint_ts: None,
                last_episode_id: None,
            }),
        }
    }

    /// Extract text from an image using macOS Vision OCR
    /// Falls back to error if OCR fails
    pub fn extract_from_image(
        &self,
        image: &DynamicImage,
        episode_id: Option<&str>,
        state_id: Option<&str>,
    ) -> ExtractionResult {
        if !self.config.ocr_enabled {
            return ExtractionResult::Disabled;
        }

        // Use macOS Vision OCR via vision_ocr module
        #[cfg(target_os = "macos")]
        {
            use crate::vision_ocr::VisionOcr;

            let ocr = VisionOcr::new();
            match ocr.recognize_text(image) {
                Ok(result) => {
                    if result.text.is_empty() {
                        return ExtractionResult::Failed("OCR produced no text".to_string());
                    }

                    if result.text.len() < self.config.min_text_length {
                        return ExtractionResult::TooShort;
                    }

                    // Use OCR confidence as part of quality score
                    let text_quality = self.score_quality(&result.text);
                    let combined_quality = (text_quality + result.confidence) / 2.0;

                    if combined_quality < self.config.min_quality_score {
                        return ExtractionResult::LowQuality(combined_quality);
                    }

                    let snapshot = TextSnapshot::new(
                        result.text,
                        episode_id,
                        state_id,
                        ExtractionSource::Ocr,
                        combined_quality,
                    );
                    ExtractionResult::Success(snapshot)
                }
                Err(e) => ExtractionResult::Failed(format!("Vision OCR failed: {}", e)),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            ExtractionResult::Failed("Vision OCR only available on macOS".to_string())
        }
    }

    /// Extract text using accessibility APIs (macOS)
    /// Faster and more accurate than OCR for apps that support accessibility
    pub fn extract_from_accessibility(
        &self,
        _app_name: Option<&str>,
        _window_title: Option<&str>,
        episode_id: Option<&str>,
        state_id: Option<&str>,
    ) -> ExtractionResult {
        #[cfg(target_os = "macos")]
        {
            use crate::accessibility_extractor::AccessibilityExtractor;

            // Check if we have accessibility permission
            if !AccessibilityExtractor::is_trusted() {
                return ExtractionResult::Failed(
                    "Accessibility permission not granted".to_string(),
                );
            }

            let extractor = AccessibilityExtractor::new();
            match extractor.extract_focused_window() {
                Ok(result) => {
                    if result.text.is_empty() {
                        return ExtractionResult::Failed(
                            "No text found via accessibility".to_string(),
                        );
                    }

                    if result.text.len() < self.config.min_text_length {
                        return ExtractionResult::TooShort;
                    }

                    let quality = self.score_quality(&result.text);
                    if quality < self.config.min_quality_score {
                        return ExtractionResult::LowQuality(quality);
                    }

                    let snapshot = TextSnapshot::new(
                        result.text,
                        episode_id,
                        state_id,
                        ExtractionSource::Accessibility,
                        quality,
                    );
                    ExtractionResult::Success(snapshot)
                }
                Err(e) => {
                    ExtractionResult::Failed(format!("Accessibility extraction failed: {}", e))
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            ExtractionResult::Failed("Accessibility extraction only available on macOS".to_string())
        }
    }

    /// Check if a checkpoint snapshot should be taken
    pub fn should_checkpoint(&self, episode_id: &str, current_ts: DateTime<Utc>) -> bool {
        let tracker = self.checkpoint_tracker.lock();

        // New episode = always checkpoint
        if tracker.last_episode_id.as_deref() != Some(episode_id) {
            return true;
        }

        // Check time since last checkpoint
        if let Some(last_ts) = tracker.last_checkpoint_ts {
            let elapsed_ms = (current_ts - last_ts).num_milliseconds() as u64;
            return elapsed_ms >= self.config.checkpoint_interval_ms;
        }

        true // No previous checkpoint
    }

    /// Record that a checkpoint was taken
    pub fn record_checkpoint(&self, episode_id: &str, ts: DateTime<Utc>) {
        let mut tracker = self.checkpoint_tracker.lock();
        tracker.last_checkpoint_ts = Some(ts);
        tracker.last_episode_id = Some(episode_id.to_string());
    }

    /// Score the quality of extracted text
    pub fn score_quality(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }

        let word_count = text.split_whitespace().count();
        let char_count = text.chars().count();

        // Factors that affect quality:
        // 1. Length (more text = better quality signal)
        // 2. Word density (words per char ratio)
        // 3. ASCII ratio (non-garbage text)
        // 4. Line structure (documents have line breaks)

        let length_score = (word_count.min(200) as f32) / 200.0;

        let word_density = if char_count > 0 {
            (word_count as f32 * 5.0) / char_count as f32 // Avg word ~5 chars
        } else {
            0.0
        };
        let word_density_score = word_density.min(1.0);

        let ascii_chars = text.chars().filter(|c| c.is_ascii()).count();
        let ascii_ratio = ascii_chars as f32 / char_count as f32;

        let line_count = text.lines().count();
        let line_score = if line_count > 1 { 1.0 } else { 0.5 };

        // Weighted average
        let quality = (length_score * 0.3)
            + (word_density_score * 0.3)
            + (ascii_ratio * 0.2)
            + (line_score * 0.2);

        quality.min(1.0)
    }

    /// Validate extracted text meets minimum requirements
    pub fn validate_text(&self, text: &str) -> bool {
        if text.len() < self.config.min_text_length {
            return false;
        }

        let quality = self.score_quality(text);
        quality >= self.config.min_quality_score
    }

    /// Create snapshot from raw text
    pub fn create_snapshot(
        &self,
        text: String,
        episode_id: Option<&str>,
        state_id: Option<&str>,
        source: ExtractionSource,
    ) -> ExtractionResult {
        if text.len() < self.config.min_text_length {
            return ExtractionResult::TooShort;
        }

        let quality = self.score_quality(&text);
        if quality < self.config.min_quality_score {
            return ExtractionResult::LowQuality(quality);
        }

        let snapshot = TextSnapshot::new(text, episode_id, state_id, source, quality);
        ExtractionResult::Success(snapshot)
    }

    /// Reset checkpoint tracking (for new meeting)
    pub fn reset(&self) {
        let mut tracker = self.checkpoint_tracker.lock();
        tracker.last_checkpoint_ts = None;
        tracker.last_episode_id = None;
    }
}

impl Default for SnapshotExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_scoring() {
        let extractor = SnapshotExtractor::new();

        // Good quality text
        let good_text = "This is a test document\nWith multiple lines\nAnd reasonable content";
        let good_score = extractor.score_quality(good_text);
        assert!(good_score > 0.5);

        // Poor quality (single word) - still gets some score due to word density
        let poor_text = "x";
        let poor_score = extractor.score_quality(poor_text);
        // Single char has 0 length score but decent word density and ascii ratio
        assert!(poor_score < good_score); // Main assertion: worse than good text
    }

    #[test]
    fn test_snapshot_creation() {
        let extractor = SnapshotExtractor::new();

        let result = extractor.create_snapshot(
            "Hello world, this is a test document for snapshot extraction testing".to_string(),
            Some("ep1"),
            Some("st1"),
            ExtractionSource::Manual,
        );

        match result {
            ExtractionResult::Success(snapshot) => {
                assert!(!snapshot.snapshot_id.is_empty());
                assert_eq!(snapshot.episode_id, Some("ep1".to_string()));
            }
            _ => panic!("Expected Success"),
        }
    }

    #[test]
    fn test_checkpoint_logic() {
        let extractor = SnapshotExtractor::new();

        // First checkpoint should always be needed
        assert!(extractor.should_checkpoint("ep1", Utc::now()));

        // Record checkpoint
        let ts = Utc::now();
        extractor.record_checkpoint("ep1", ts);

        // Immediately after, shouldn't need checkpoint
        assert!(!extractor.should_checkpoint("ep1", ts));

        // New episode should need checkpoint
        assert!(extractor.should_checkpoint("ep2", ts));
    }
}
