// Frame Extractor Module
// Extracts frames from recorded video chunks on-demand
// Uses FFmpeg for extraction

use chrono::{DateTime, Utc};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

// VideoChunk type is available in video_recorder but we don't need it here

/// An extracted frame with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFrame {
    pub path: PathBuf,
    pub timestamp_secs: f64,
    pub chunk_number: u32,
    pub extracted_at: DateTime<Utc>,
    pub width: u32,
    pub height: u32,
}

/// Frame extraction strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtractionStrategy {
    /// Extract at specific timestamp
    AtTimestamp(f64),
    /// Extract frames at regular interval
    Interval { start: f64, end: f64, interval: f64 },
    /// Extract based on visual change detection
    Adaptive { threshold: f32 },
    /// Extract keyframes only
    Keyframes,
}

/// Frame extractor using FFmpeg
pub struct FrameExtractor {
    /// Cache directory for extracted frames
    cache_dir: PathBuf,
    /// JPEG quality (1-100)
    jpeg_quality: u32,
}

impl FrameExtractor {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            jpeg_quality: 85,
        }
    }

    /// Extract a single frame at a specific timestamp
    pub fn extract_at(
        &self,
        video_path: &Path,
        timestamp_secs: f64,
        meeting_id: &str,
    ) -> Result<ExtractedFrame, String> {
        // Create cache directory
        let frames_dir = self.cache_dir.join(meeting_id).join("frames");
        std::fs::create_dir_all(&frames_dir)
            .map_err(|e| format!("Failed to create frames directory: {}", e))?;

        // Output filename based on timestamp
        let frame_name = format!("frame_{:.3}.jpg", timestamp_secs);
        let output_path = frames_dir.join(&frame_name);

        // Skip if already extracted
        if output_path.exists() {
            return self.load_frame_metadata(&output_path, timestamp_secs, 0);
        }

        // Use ffmpeg to extract frame
        let status = Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.3}", timestamp_secs),
                "-i",
                video_path.to_str().unwrap(),
                "-vframes",
                "1",
                "-q:v",
                &format!("{}", (100 - self.jpeg_quality) / 3 + 1),
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            return Err(format!("ffmpeg failed: {}", stderr));
        }

        self.load_frame_metadata(&output_path, timestamp_secs, 0)
    }

    /// Extract multiple frames in a time range
    pub fn extract_range(
        &self,
        video_path: &Path,
        start_secs: f64,
        end_secs: f64,
        interval_secs: f64,
        meeting_id: &str,
    ) -> Result<Vec<ExtractedFrame>, String> {
        let mut frames = Vec::new();
        let mut t = start_secs;

        while t <= end_secs {
            match self.extract_at(video_path, t, meeting_id) {
                Ok(frame) => frames.push(frame),
                Err(e) => log::warn!("Failed to extract frame at {}: {}", t, e),
            }
            t += interval_secs;
        }

        Ok(frames)
    }

    /// Extract frames around a specific moment (for pin moments)
    pub fn extract_around_moment(
        &self,
        video_path: &Path,
        center_secs: f64,
        window_secs: f64,
        meeting_id: &str,
    ) -> Result<Vec<ExtractedFrame>, String> {
        let start = (center_secs - window_secs).max(0.0);
        let end = center_secs + window_secs;

        // Extract at 1-second intervals around the moment
        self.extract_range(video_path, start, end, 1.0, meeting_id)
    }

    /// Extract thumbnail for gallery view
    pub fn extract_thumbnail(
        &self,
        video_path: &Path,
        timestamp_secs: f64,
        meeting_id: &str,
        thumb_size: u32,
    ) -> Result<PathBuf, String> {
        let thumbs_dir = self.cache_dir.join(meeting_id).join("thumbnails");
        std::fs::create_dir_all(&thumbs_dir)
            .map_err(|e| format!("Failed to create thumbnails directory: {}", e))?;

        let thumb_name = format!("thumb_{:.3}.jpg", timestamp_secs);
        let output_path = thumbs_dir.join(&thumb_name);

        if output_path.exists() {
            return Ok(output_path);
        }

        // Extract and scale in one ffmpeg call
        let status = Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.3}", timestamp_secs),
                "-i",
                video_path.to_str().unwrap(),
                "-vframes",
                "1",
                "-vf",
                &format!("scale={}:-1", thumb_size),
                "-q:v",
                "4", // Lower quality for thumbnails
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

        if !status.status.success() {
            return Err("Failed to extract thumbnail".to_string());
        }

        Ok(output_path)
    }

    /// Generate thumbnails for entire video at regular intervals
    pub fn generate_timeline_thumbnails(
        &self,
        video_path: &Path,
        duration_secs: f64,
        interval_secs: f64,
        meeting_id: &str,
        thumb_size: u32,
    ) -> Result<Vec<PathBuf>, String> {
        let mut thumbnails = Vec::new();
        let mut t = 0.0;

        while t < duration_secs {
            match self.extract_thumbnail(video_path, t, meeting_id, thumb_size) {
                Ok(path) => thumbnails.push(path),
                Err(e) => log::warn!("Failed to extract thumbnail at {}: {}", t, e),
            }
            t += interval_secs;
        }

        Ok(thumbnails)
    }

    /// Extract frames with scene change detection (adaptive)
    pub fn extract_adaptive(
        &self,
        video_path: &Path,
        meeting_id: &str,
        sensitivity: f32,
    ) -> Result<Vec<ExtractedFrame>, String> {
        let frames_dir = self.cache_dir.join(meeting_id).join("frames");
        std::fs::create_dir_all(&frames_dir)
            .map_err(|e| format!("Failed to create frames directory: {}", e))?;

        // Use ffmpeg scene detection filter
        let output_pattern = frames_dir.join("scene_%04d.jpg");

        let status = Command::new("ffmpeg")
            .args([
                "-i",
                video_path.to_str().unwrap(),
                "-vf",
                &format!("select='gt(scene,{})',showinfo", sensitivity),
                "-vsync",
                "vfr",
                "-q:v",
                "3",
                output_pattern.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("Failed to run ffmpeg scene detection: {}", e))?;

        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            log::warn!("Scene detection: {}", stderr);
        }

        // Collect extracted frames
        let mut frames = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "jpg") {
                    if let Ok(frame) = self.load_frame_metadata(&path, 0.0, 0) {
                        frames.push(frame);
                    }
                }
            }
        }

        frames.sort_by(|a, b| a.timestamp_secs.partial_cmp(&b.timestamp_secs).unwrap());
        Ok(frames)
    }

    /// Load an extracted frame as image
    pub fn load_image(&self, frame: &ExtractedFrame) -> Result<DynamicImage, String> {
        image::open(&frame.path).map_err(|e| format!("Failed to load image: {}", e))
    }

    /// Clean up old cached frames
    pub fn cleanup_cache(&self, meeting_id: &str, max_age_hours: u64) -> Result<u64, String> {
        let frames_dir = self.cache_dir.join(meeting_id).join("frames");
        let cutoff =
            std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_hours * 3600);

        let mut removed = 0u64;

        if let Ok(entries) = std::fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified < cutoff {
                            if std::fs::remove_file(entry.path()).is_ok() {
                                removed += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Get video duration using ffprobe
    pub fn get_video_duration(&self, video_path: &Path) -> Result<f64, String> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                video_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        let duration_str = String::from_utf8_lossy(&output.stdout);
        duration_str
            .trim()
            .parse::<f64>()
            .map_err(|_| "Failed to parse duration".to_string())
    }

    fn load_frame_metadata(
        &self,
        path: &Path,
        timestamp_secs: f64,
        chunk_number: u32,
    ) -> Result<ExtractedFrame, String> {
        // Get image dimensions
        let (width, height) = if let Ok(img) = image::open(path) {
            (img.width(), img.height())
        } else {
            (0, 0)
        };

        Ok(ExtractedFrame {
            path: path.to_path_buf(),
            timestamp_secs,
            chunk_number,
            extracted_at: Utc::now(),
            width,
            height,
        })
    }
}

impl Default for FrameExtractor {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("com.nofriction.meetings");
        Self::new(cache_dir)
    }
}
