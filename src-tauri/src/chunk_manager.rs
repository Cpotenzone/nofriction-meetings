// Chunk Manager Module
// Manages video storage, retention policies, and disk usage

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::video_recorder::VideoChunk;

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_bytes: u64,
    pub video_bytes: u64,
    pub frames_bytes: u64,
    pub meetings_count: u32,
    pub chunks_count: u32,
    pub oldest_meeting: Option<DateTime<Utc>>,
    pub disk_limit_bytes: u64,
    pub usage_percent: f32,
}

/// Retention policy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Days to keep video recordings
    pub video_retention_days: u32,
    /// Days to keep extracted frames
    pub frames_retention_days: u32,
    /// Maximum disk usage in GB
    pub max_disk_gb: f64,
    /// Enable LRU eviction when over limit
    pub enable_lru_eviction: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            video_retention_days: 7,
            frames_retention_days: 1,
            max_disk_gb: 50.0,
            enable_lru_eviction: true,
        }
    }
}

/// Meeting storage info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingStorage {
    pub meeting_id: String,
    pub created_at: DateTime<Utc>,
    pub video_bytes: u64,
    pub frames_bytes: u64,
    pub total_bytes: u64,
    pub chunk_count: u32,
    pub path: PathBuf,
}

/// Chunk manager for storage and retention
pub struct ChunkManager {
    /// Base storage directory
    storage_dir: PathBuf,
    /// Retention policy
    policy: RetentionPolicy,
}

impl ChunkManager {
    pub fn new(storage_dir: PathBuf, policy: RetentionPolicy) -> Self {
        Self {
            storage_dir,
            policy,
        }
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats, String> {
        let mut total_bytes = 0u64;
        let mut video_bytes = 0u64;
        let mut frames_bytes = 0u64;
        let mut meetings_count = 0u32;
        let mut chunks_count = 0u32;
        let mut oldest: Option<DateTime<Utc>> = None;

        // Scan meeting directories
        if let Ok(entries) = std::fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.file_name().map_or(false, |n| n.len() == 36) {
                    // UUID-like directory name
                    meetings_count += 1;

                    // Get creation time
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(created) = meta.created() {
                            let dt: DateTime<Utc> = created.into();
                            if oldest.is_none() || dt < oldest.unwrap() {
                                oldest = Some(dt);
                            }
                        }
                    }

                    // Scan video directory
                    let video_dir = path.join("video");
                    if video_dir.exists() {
                        if let Ok(files) = std::fs::read_dir(&video_dir) {
                            for file in files.flatten() {
                                if let Ok(meta) = file.metadata() {
                                    let size = meta.len();
                                    video_bytes += size;
                                    total_bytes += size;
                                    if file.path().extension().map_or(false, |e| e == "mov") {
                                        chunks_count += 1;
                                    }
                                }
                            }
                        }
                    }

                    // Scan frames directory
                    let frames_dir = path.join("frames");
                    if frames_dir.exists() {
                        if let Ok(files) = std::fs::read_dir(&frames_dir) {
                            for file in files.flatten() {
                                if let Ok(meta) = file.metadata() {
                                    let size = meta.len();
                                    frames_bytes += size;
                                    total_bytes += size;
                                }
                            }
                        }
                    }
                }
            }
        }

        let disk_limit_bytes = (self.policy.max_disk_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        let usage_percent = if disk_limit_bytes > 0 {
            (total_bytes as f32 / disk_limit_bytes as f32) * 100.0
        } else {
            0.0
        };

        Ok(StorageStats {
            total_bytes,
            video_bytes,
            frames_bytes,
            meetings_count,
            chunks_count,
            oldest_meeting: oldest,
            disk_limit_bytes,
            usage_percent,
        })
    }

    /// Get storage info for all meetings
    pub fn list_meetings(&self) -> Result<Vec<MeetingStorage>, String> {
        let mut meetings = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let meeting_id = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if meeting_id.len() != 36 {
                        continue; // Not a meeting directory
                    }

                    let mut video_bytes = 0u64;
                    let mut frames_bytes = 0u64;
                    let mut chunk_count = 0u32;
                    let mut created_at = Utc::now();

                    // Get directory creation time
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(created) = meta.created() {
                            created_at = created.into();
                        }
                    }

                    // Count video bytes and chunks
                    let video_dir = path.join("video");
                    if video_dir.exists() {
                        if let Ok(files) = std::fs::read_dir(&video_dir) {
                            for file in files.flatten() {
                                if let Ok(meta) = file.metadata() {
                                    video_bytes += meta.len();
                                    if file.path().extension().map_or(false, |e| e == "mov") {
                                        chunk_count += 1;
                                    }
                                }
                            }
                        }
                    }

                    // Count frames bytes
                    let frames_dir = path.join("frames");
                    if frames_dir.exists() {
                        if let Ok(files) = std::fs::read_dir(&frames_dir) {
                            for file in files.flatten() {
                                if let Ok(meta) = file.metadata() {
                                    frames_bytes += meta.len();
                                }
                            }
                        }
                    }

                    meetings.push(MeetingStorage {
                        meeting_id,
                        created_at,
                        video_bytes,
                        frames_bytes,
                        total_bytes: video_bytes + frames_bytes,
                        chunk_count,
                        path,
                    });
                }
            }
        }

        // Sort by creation time, newest first
        meetings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(meetings)
    }

    /// Apply retention policy
    pub fn apply_retention(&self) -> Result<(u32, u64), String> {
        let mut deleted_meetings = 0u32;
        let mut freed_bytes = 0u64;

        let now = Utc::now();
        let video_cutoff = now - Duration::days(self.policy.video_retention_days as i64);
        let frames_cutoff = now - Duration::days(self.policy.frames_retention_days as i64);

        let meetings = self.list_meetings()?;

        for meeting in &meetings {
            // Delete old videos
            if meeting.created_at < video_cutoff {
                if let Ok(bytes) = self.delete_meeting(&meeting.meeting_id) {
                    freed_bytes += bytes;
                    deleted_meetings += 1;
                }
                continue;
            }

            // Delete old frames (keep video)
            if meeting.created_at < frames_cutoff {
                let frames_dir = meeting.path.join("frames");
                if frames_dir.exists() {
                    if let Ok(size) = Self::dir_size(&frames_dir) {
                        freed_bytes += size;
                    }
                    let _ = std::fs::remove_dir_all(&frames_dir);
                }
            }
        }

        log::info!(
            "Retention cleanup: {} meetings, {} bytes freed",
            deleted_meetings,
            freed_bytes
        );
        Ok((deleted_meetings, freed_bytes))
    }

    /// Apply LRU eviction if over disk limit
    pub fn apply_lru_eviction(&self) -> Result<(u32, u64), String> {
        if !self.policy.enable_lru_eviction {
            return Ok((0, 0));
        }

        let stats = self.get_stats()?;
        if stats.usage_percent < 100.0 {
            return Ok((0, 0));
        }

        let mut deleted = 0u32;
        let mut freed = 0u64;
        let target_bytes = stats.total_bytes - stats.disk_limit_bytes;

        // Get meetings sorted oldest first
        let mut meetings = self.list_meetings()?;
        meetings.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        for meeting in meetings {
            if freed >= target_bytes {
                break;
            }

            if let Ok(bytes) = self.delete_meeting(&meeting.meeting_id) {
                freed += bytes;
                deleted += 1;
            }
        }

        log::info!("LRU eviction: {} meetings, {} bytes freed", deleted, freed);
        Ok((deleted, freed))
    }

    /// Delete a meeting's storage
    pub fn delete_meeting(&self, meeting_id: &str) -> Result<u64, String> {
        let path = self.storage_dir.join(meeting_id);
        if !path.exists() {
            return Ok(0);
        }

        let size = Self::dir_size(&path).unwrap_or(0);
        std::fs::remove_dir_all(&path).map_err(|e| format!("Failed to delete meeting: {}", e))?;

        log::info!("Deleted meeting {} ({} bytes)", meeting_id, size);
        Ok(size)
    }

    /// Get video chunks for a meeting
    pub fn get_chunks(&self, meeting_id: &str) -> Result<Vec<VideoChunk>, String> {
        let video_dir = self.storage_dir.join(meeting_id).join("video");
        if !video_dir.exists() {
            return Ok(Vec::new());
        }

        let mut chunks = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&video_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "mov") {
                    let filename = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");

                    let chunk_number = filename
                        .strip_prefix("chunk_")
                        .and_then(|n| n.parse::<u32>().ok())
                        .unwrap_or(0);

                    let meta = entry.metadata().ok();
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let created = meta
                        .and_then(|m| m.created().ok())
                        .map(|t| t.into())
                        .unwrap_or_else(Utc::now);

                    chunks.push(VideoChunk {
                        chunk_number,
                        path,
                        start_time: created,
                        end_time: None,
                        size_bytes: size,
                        duration_secs: 0.0,
                    });
                }
            }
        }

        chunks.sort_by_key(|c| c.chunk_number);
        Ok(chunks)
    }

    /// Format bytes as human-readable string
    pub fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
        let mut size = 0u64;

        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    size += Self::dir_size(&path)?;
                } else {
                    size += entry.metadata()?.len();
                }
            }
        }

        Ok(size)
    }
}

impl Default for ChunkManager {
    fn default() -> Self {
        let storage_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.nofriction.meetings");
        Self::new(storage_dir, RetentionPolicy::default())
    }
}
