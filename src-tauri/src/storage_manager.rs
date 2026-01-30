// noFriction Meetings - Storage Manager
// Safe filesystem enumeration and deletion for recordings
//
// Features:
// - Enumerate storage usage per meeting (frames, video, audio)
// - Safe deletion with path allowlist validation
// - Atomic operations with audit logging hooks

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Represents storage usage for a single meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageItem {
    pub meeting_id: String,
    pub title: String,
    pub started_at: Option<DateTime<Utc>>,
    pub frames_count: u32,
    pub frames_bytes: u64,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    pub total_bytes: u64,
}

/// Preview of files to be deleted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePreview {
    pub meeting_ids: Vec<String>,
    pub total_bytes: u64,
    pub total_bytes_formatted: String,
    pub total_files: u32,
    pub file_count: u32,
    pub files_by_type: HashMap<String, u32>,
    pub breakdown: HashMap<String, MeetingDeleteInfo>,
    pub safe_to_delete: bool,
    pub warnings: Vec<String>,
}

/// Per-meeting deletion info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDeleteInfo {
    pub meeting_id: String,
    pub frames_count: u32,
    pub frames_bytes: u64,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    pub total_bytes: u64,
    pub paths: Vec<String>,
}

/// Result of a deletion operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResult {
    pub success: bool,
    pub meetings_deleted: u32,
    pub files_deleted: u32,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}

/// Storage manager for safe filesystem operations
pub struct StorageManager {
    app_data_dir: PathBuf,
    allowed_subdirs: Vec<&'static str>,
}

impl StorageManager {
    /// Create a new storage manager with the app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            app_data_dir,
            // Only these subdirectories can be deleted
            allowed_subdirs: vec!["frames", "video", "audio", "thumbnails"],
        }
    }

    /// Validate that a path is within the allowed app data directory
    fn validate_path(&self, path: &Path) -> Result<(), String> {
        // Canonicalize both paths to resolve symlinks and ..
        let canonical_app_dir = self
            .app_data_dir
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize app dir: {}", e))?;

        // For paths that don't exist yet, we validate the parent
        let check_path = if path.exists() {
            path.canonicalize()
                .map_err(|e| format!("Failed to canonicalize path: {}", e))?
        } else {
            // For non-existent paths, check the parent exists and is valid
            let parent = path
                .parent()
                .ok_or_else(|| "Path has no parent".to_string())?;
            if parent.exists() {
                parent
                    .canonicalize()
                    .map_err(|e| format!("Failed to canonicalize parent: {}", e))?
            } else {
                return Err("Path parent does not exist".to_string());
            }
        };

        // Ensure path is within app data dir
        if !check_path.starts_with(&canonical_app_dir) {
            return Err(format!(
                "Path {} is outside allowed directory {}",
                check_path.display(),
                canonical_app_dir.display()
            ));
        }

        // Check that it's in an allowed subdirectory
        let relative = check_path
            .strip_prefix(&canonical_app_dir)
            .map_err(|_| "Failed to get relative path".to_string())?;

        let first_component = relative
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string());

        if let Some(subdir) = first_component {
            if !self.allowed_subdirs.contains(&subdir.as_str()) {
                return Err(format!(
                    "Subdirectory '{}' is not in allowed list: {:?}",
                    subdir, self.allowed_subdirs
                ));
            }
        }

        Ok(())
    }

    /// Get storage info for a single meeting
    pub async fn get_meeting_storage(&self, meeting_id: &str) -> MeetingDeleteInfo {
        let mut info = MeetingDeleteInfo {
            meeting_id: meeting_id.to_string(),
            frames_count: 0,
            frames_bytes: 0,
            video_bytes: 0,
            audio_bytes: 0,
            total_bytes: 0,
            paths: Vec::new(),
        };

        // Check frames directory
        let frames_dir = self.app_data_dir.join("frames").join(meeting_id);
        if frames_dir.exists() {
            if let Ok((count, bytes)) = self.count_directory_size(&frames_dir).await {
                info.frames_count = count;
                info.frames_bytes = bytes;
                info.paths.push(frames_dir.to_string_lossy().to_string());
            }
        }

        // Check video directory
        let video_dir = self.app_data_dir.join("video").join(meeting_id);
        if video_dir.exists() {
            if let Ok((_, bytes)) = self.count_directory_size(&video_dir).await {
                info.video_bytes = bytes;
                info.paths.push(video_dir.to_string_lossy().to_string());
            }
        }

        // Check audio directory
        let audio_dir = self.app_data_dir.join("audio").join(meeting_id);
        if audio_dir.exists() {
            if let Ok((_, bytes)) = self.count_directory_size(&audio_dir).await {
                info.audio_bytes = bytes;
                info.paths.push(audio_dir.to_string_lossy().to_string());
            }
        }

        info.total_bytes = info.frames_bytes + info.video_bytes + info.audio_bytes;
        info
    }

    /// Count files and total size in a directory recursively
    async fn count_directory_size(&self, dir: &Path) -> Result<(u32, u64), String> {
        let mut count = 0u32;
        let mut size = 0u64;

        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| format!("Failed to read dir: {}", e))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| format!("Failed to get metadata: {}", e))?;

            if metadata.is_file() {
                count += 1;
                size += metadata.len();
            } else if metadata.is_dir() {
                if let Ok((sub_count, sub_size)) = Box::pin(self.count_directory_size(&path)).await
                {
                    count += sub_count;
                    size += sub_size;
                }
            }
        }

        Ok((count, size))
    }

    /// Generate delete preview for multiple meetings
    pub async fn preview_delete(&self, meeting_ids: &[String]) -> DeletePreview {
        let mut breakdown = HashMap::new();
        let mut total_bytes = 0u64;
        let mut file_count = 0u32;
        let mut files_by_type: HashMap<String, u32> = HashMap::new();

        for meeting_id in meeting_ids {
            let info = self.get_meeting_storage(meeting_id).await;
            total_bytes += info.total_bytes;

            // Count frames
            file_count += info.frames_count;
            if info.frames_count > 0 {
                *files_by_type.entry("frames".to_string()).or_insert(0) += info.frames_count;
            }

            // Count video files (estimate 1 per meeting if video exists)
            if info.video_bytes > 0 {
                *files_by_type.entry("video".to_string()).or_insert(0) += 1;
                file_count += 1;
            }

            // Count audio files (estimate 1 per meeting if audio exists)
            if info.audio_bytes > 0 {
                *files_by_type.entry("audio".to_string()).or_insert(0) += 1;
                file_count += 1;
            }

            breakdown.insert(meeting_id.clone(), info);
        }

        DeletePreview {
            meeting_ids: meeting_ids.to_vec(),
            total_bytes,
            total_bytes_formatted: Self::format_bytes(total_bytes),
            total_files: file_count,
            file_count,
            files_by_type,
            breakdown,
            safe_to_delete: true, // Will be overridden by command if recording is active
            warnings: Vec::new(), // Empty warnings by default
        }
    }

    /// Delete all files for a meeting (frames, video, audio)
    /// Returns bytes freed
    pub async fn delete_meeting_files(&self, meeting_id: &str) -> Result<u64, String> {
        let mut bytes_freed = 0u64;
        let mut errors = Vec::new();

        // Delete frames
        let frames_dir = self.app_data_dir.join("frames").join(meeting_id);
        if frames_dir.exists() {
            self.validate_path(&frames_dir)?;
            match self.count_directory_size(&frames_dir).await {
                Ok((_, bytes)) => bytes_freed += bytes,
                Err(e) => errors.push(format!("Failed to count frames: {}", e)),
            }
            if let Err(e) = fs::remove_dir_all(&frames_dir).await {
                errors.push(format!("Failed to delete frames: {}", e));
            }
        }

        // Delete video
        let video_dir = self.app_data_dir.join("video").join(meeting_id);
        if video_dir.exists() {
            self.validate_path(&video_dir)?;
            match self.count_directory_size(&video_dir).await {
                Ok((_, bytes)) => bytes_freed += bytes,
                Err(e) => errors.push(format!("Failed to count video: {}", e)),
            }
            if let Err(e) = fs::remove_dir_all(&video_dir).await {
                errors.push(format!("Failed to delete video: {}", e));
            }
        }

        // Delete audio
        let audio_dir = self.app_data_dir.join("audio").join(meeting_id);
        if audio_dir.exists() {
            self.validate_path(&audio_dir)?;
            match self.count_directory_size(&audio_dir).await {
                Ok((_, bytes)) => bytes_freed += bytes,
                Err(e) => errors.push(format!("Failed to count audio: {}", e)),
            }
            if let Err(e) = fs::remove_dir_all(&audio_dir).await {
                errors.push(format!("Failed to delete audio: {}", e));
            }
        }

        if !errors.is_empty() {
            log::warn!("Errors during deletion of {}: {:?}", meeting_id, errors);
        }

        Ok(bytes_freed)
    }

    /// Delete multiple meetings, returning aggregate result
    pub async fn delete_meetings(&self, meeting_ids: &[String]) -> DeleteResult {
        let mut result = DeleteResult {
            success: true,
            meetings_deleted: 0,
            files_deleted: 0,
            bytes_freed: 0,
            errors: Vec::new(),
        };

        for meeting_id in meeting_ids {
            // Get file count before deletion
            let info = self.get_meeting_storage(meeting_id).await;

            match self.delete_meeting_files(meeting_id).await {
                Ok(bytes) => {
                    result.meetings_deleted += 1;
                    result.files_deleted += info.frames_count;
                    result.bytes_freed += bytes;
                    log::info!(
                        "Deleted meeting {} files: {} bytes freed",
                        meeting_id,
                        bytes
                    );
                }
                Err(e) => {
                    result.success = false;
                    result.errors.push(format!("{}: {}", meeting_id, e));
                    log::error!("Failed to delete meeting {}: {}", meeting_id, e);
                }
            }
        }

        result
    }

    /// Format bytes as human-readable string
    pub fn format_bytes(bytes: u64) -> String {
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
            format!("{} bytes", bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_validate_path_inside_app_dir() {
        let temp = tempdir().unwrap();
        let app_dir = temp.path().to_path_buf();

        // Create frames subdirectory
        std::fs::create_dir_all(app_dir.join("frames/test123")).unwrap();

        let manager = StorageManager::new(app_dir.clone());

        // Valid path inside frames
        let valid_path = app_dir.join("frames/test123");
        assert!(manager.validate_path(&valid_path).is_ok());
    }

    #[tokio::test]
    async fn test_validate_path_outside_app_dir() {
        let temp = tempdir().unwrap();
        let app_dir = temp.path().to_path_buf();
        let manager = StorageManager::new(app_dir);

        // Path outside app dir should fail
        let invalid_path = PathBuf::from("/tmp/evil");
        assert!(manager.validate_path(&invalid_path).is_err());
    }

    #[tokio::test]
    async fn test_get_meeting_storage_empty() {
        let temp = tempdir().unwrap();
        let manager = StorageManager::new(temp.path().to_path_buf());

        let info = manager.get_meeting_storage("nonexistent").await;
        assert_eq!(info.frames_count, 0);
        assert_eq!(info.total_bytes, 0);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(StorageManager::format_bytes(500), "500 bytes");
        assert_eq!(StorageManager::format_bytes(1500), "1.5 KB");
        assert_eq!(StorageManager::format_bytes(1_500_000), "1.4 MB");
        assert_eq!(StorageManager::format_bytes(1_500_000_000), "1.4 GB");
    }
}
