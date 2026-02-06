// noFriction Meetings - Deduplication Gate
// Custom perceptual hashing + delta scoring for frame deduplication
//
// This module provides O(1) per-frame duplicate detection using:
// 1. Average Hash (aHash) - custom implementation for thread safety
// 2. Delta scoring for pixel-level changes
//
// The combination eliminates 80-95% of frame storage in static sessions.

use image::{DynamicImage, GrayImage};

/// Custom 64-bit average hash that is Send + Sync
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AverageHash {
    hash: u64,
}

impl AverageHash {
    /// Compute average hash from an image
    pub fn compute(image: &DynamicImage) -> Self {
        // Resize to 8x8 grayscale
        let thumb = image
            .resize_exact(8, 8, image::imageops::FilterType::Nearest)
            .to_luma8();

        // Compute mean pixel value
        let pixels: Vec<u8> = thumb.pixels().map(|p| p.0[0]).collect();
        let mean: u64 = pixels.iter().map(|&p| p as u64).sum::<u64>() / 64;

        // Build hash: 1 if pixel > mean, 0 otherwise
        let mut hash: u64 = 0;
        for (i, &pixel) in pixels.iter().enumerate() {
            if pixel as u64 > mean {
                hash |= 1 << i;
            }
        }

        Self { hash }
    }

    /// Compute Hamming distance between two hashes
    pub fn distance(&self, other: &Self) -> u32 {
        (self.hash ^ other.hash).count_ones()
    }

    /// Convert to base64 string for storage
    pub fn to_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(self.hash.to_le_bytes())
    }

    /// Parse from base64 string
    pub fn from_base64(s: &str) -> Option<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
        if bytes.len() != 8 {
            return None;
        }
        let hash = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Some(Self { hash })
    }
}

/// Configuration for deduplication thresholds
#[derive(Debug, Clone)]
pub struct DedupConfig {
    /// Hamming distance threshold for aHash (0-64)
    /// Lower = more strict (more state boundaries)
    pub hash_threshold: u32,

    /// Pixel delta threshold (0.0-1.0)
    /// Higher = more tolerant (fewer boundaries)
    pub delta_threshold: f32,

    /// Size to downscale for delta computation
    pub delta_size: u32,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            hash_threshold: 8,
            delta_threshold: 0.02,
            delta_size: 32,
        }
    }
}

/// Result of deduplication check
#[derive(Debug, Clone)]
pub struct DedupResult {
    /// Whether this frame is a duplicate of the previous
    pub is_duplicate: bool,

    /// Computed perceptual hash
    pub ahash: AverageHash,

    /// Hamming distance from previous hash
    pub hamming_distance: u32,

    /// Pixel delta score (0.0-1.0)
    pub delta_score: f32,

    /// Reason for the decision
    pub reason: DedupReason,
}

#[derive(Debug, Clone, Copy)]
pub enum DedupReason {
    /// First frame, no previous to compare
    FirstFrame,
    /// Hash distance below threshold
    HashSimilar,
    /// Hash different but delta below threshold (noise)
    DeltaSimilar,
    /// Both hash and delta indicate change
    SignificantChange,
    /// Hash similar but delta high (motion blur, cursor)
    MotionNoise,
}

/// Deduplication gate for frame processing
/// This struct is Send + Sync safe
pub struct DedupGate {
    config: DedupConfig,
    last_ahash: Option<AverageHash>,
    last_thumbnail: Option<GrayImage>,
}

// Explicitly mark as Send + Sync since all fields are thread-safe
unsafe impl Send for DedupGate {}
unsafe impl Sync for DedupGate {}

impl DedupGate {
    /// Create a new deduplication gate with default config
    pub fn new() -> Self {
        Self::with_config(DedupConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: DedupConfig) -> Self {
        Self {
            config,
            last_ahash: None,
            last_thumbnail: None,
        }
    }

    /// Check if a frame is a duplicate of the previous
    /// Returns dedup result with hash and scores
    pub fn check_frame(&mut self, image: &DynamicImage) -> DedupResult {
        // Compute average hash (custom implementation, thread-safe)
        let ahash = AverageHash::compute(image);

        // Create grayscale thumbnail for delta scoring
        let thumbnail = self.create_thumbnail(image);

        // Compare with previous
        let (is_duplicate, hamming_distance, delta_score, reason) =
            match (&self.last_ahash, &self.last_thumbnail) {
                (Some(last_hash), Some(last_thumb)) => {
                    let hamming = ahash.distance(last_hash);
                    let delta = self.compute_delta(&thumbnail, last_thumb);

                    let (is_dup, reason) = self.decide_duplicate(hamming, delta);
                    (is_dup, hamming, delta, reason)
                }
                _ => {
                    // First frame
                    (false, 0, 0.0, DedupReason::FirstFrame)
                }
            };

        // Update state for next comparison
        self.last_ahash = Some(ahash.clone());
        self.last_thumbnail = Some(thumbnail);

        DedupResult {
            is_duplicate,
            ahash,
            hamming_distance,
            delta_score,
            reason,
        }
    }

    /// Reset the gate (for new meeting)
    pub fn reset(&mut self) {
        self.last_ahash = None;
        self.last_thumbnail = None;
    }

    /// Create grayscale thumbnail for delta computation
    fn create_thumbnail(&self, image: &DynamicImage) -> GrayImage {
        let size = self.config.delta_size;
        image
            .resize_exact(size, size, image::imageops::FilterType::Nearest)
            .to_luma8()
    }

    /// Compute mean absolute pixel difference between thumbnails
    fn compute_delta(&self, current: &GrayImage, previous: &GrayImage) -> f32 {
        let (w, h) = current.dimensions();
        let total_pixels = (w * h) as f32;

        if total_pixels == 0.0 {
            return 1.0; // Treat as different if invalid
        }

        let mut total_diff: u32 = 0;

        for y in 0..h {
            for x in 0..w {
                let curr_pixel = current.get_pixel(x, y).0[0] as i32;
                let prev_pixel = previous.get_pixel(x, y).0[0] as i32;
                total_diff += (curr_pixel - prev_pixel).unsigned_abs();
            }
        }

        // Normalize to 0.0-1.0 range
        (total_diff as f32) / (total_pixels * 255.0)
    }

    /// Decision logic for duplicate detection
    fn decide_duplicate(&self, hamming: u32, delta: f32) -> (bool, DedupReason) {
        let hash_similar = hamming <= self.config.hash_threshold;
        let delta_similar = delta <= self.config.delta_threshold;

        // Extreme delta (> 50%) is always a significant change, regardless of hash
        // This handles edge cases like solid color transitions where hashes may collide
        let delta_extreme = delta > 0.5;

        match (hash_similar, delta_similar, delta_extreme) {
            // Both indicate similarity -> definite duplicate
            (true, true, false) => (true, DedupReason::HashSimilar),

            // Hash different but pixels similar -> probably noise/compression
            (false, true, false) => (true, DedupReason::DeltaSimilar),

            // Hash similar but pixels moderately different -> motion/cursor/scroll
            (true, false, false) => (true, DedupReason::MotionNoise),

            // Extreme delta always means significant change
            (_, _, true) => (false, DedupReason::SignificantChange),

            // Both different -> significant change, new state
            (false, false, false) => (false, DedupReason::SignificantChange),
        }
    }

    /// Get hash as string for storage
    pub fn hash_to_string(hash: &AverageHash) -> String {
        hash.to_base64()
    }

    /// Parse hash from string
    pub fn hash_from_string(s: &str) -> Option<AverageHash> {
        AverageHash::from_base64(s)
    }
}

impl Default for DedupGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_identical_frames_are_duplicates() {
        let mut gate = DedupGate::new();

        // Create a simple test image
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(100, 100, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));

        // First frame - not a duplicate (no previous)
        let result1 = gate.check_frame(&img);
        assert!(!result1.is_duplicate);

        // Same frame again - should be duplicate
        let result2 = gate.check_frame(&img);
        assert!(result2.is_duplicate);
    }

    #[test]
    fn test_different_frames_not_duplicates() {
        let mut gate = DedupGate::new();

        // Create two very different images
        let img1 = DynamicImage::ImageRgb8(RgbImage::from_fn(100, 100, |_, _| Rgb([0, 0, 0])));

        let img2 =
            DynamicImage::ImageRgb8(RgbImage::from_fn(100, 100, |_, _| Rgb([255, 255, 255])));

        gate.check_frame(&img1);
        let result = gate.check_frame(&img2);

        assert!(!result.is_duplicate);
    }

    #[test]
    fn test_hash_serialization() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(100, 100, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));

        let hash = AverageHash::compute(&img);
        let serialized = hash.to_base64();
        let deserialized = AverageHash::from_base64(&serialized).unwrap();

        assert_eq!(hash, deserialized);
    }
}
