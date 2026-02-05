// noFriction Meetings - Accessibility Capture Service
// Continuous background capture of window text via accessibility APIs
//
// Similar to audio transcription, this creates a running "visual transcript"
// of what the user is doing by extracting text from focused windows.

use crate::accessibility_extractor::AccessibilityExtractor;
use crate::database::DatabaseManager;
use crate::pinecone_client::PineconeClient;
use crate::settings::SettingsManager;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Configuration for accessibility capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityCaptureConfig {
    /// Whether capture is enabled
    pub enabled: bool,
    /// Capture interval in seconds
    pub interval_secs: u32,
    /// Minimum word count to save (skip very short content)
    pub min_word_count: u32,
    /// Whether to deduplicate identical content
    pub deduplicate: bool,
}

impl Default for AccessibilityCaptureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 10,
            min_word_count: 5,
            deduplicate: true,
        }
    }
}

/// Statistics for accessibility capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityCaptureStats {
    /// Whether capture is currently running
    pub running: bool,
    /// Total captures performed
    pub capture_count: u64,
    /// Captures saved (after dedup)
    pub saved_count: u64,
    /// Captures skipped due to dedup
    pub skipped_count: u64,
    /// Last capture timestamp
    pub last_capture: Option<DateTime<Utc>>,
    /// Last captured app name
    pub last_app: Option<String>,
}

/// Accessibility Capture Service
/// Runs in background, capturing window text at intervals
pub struct AccessibilityCaptureService {
    running: Arc<AtomicBool>,
    capture_count: Arc<AtomicU64>,
    saved_count: Arc<AtomicU64>,
    skipped_count: Arc<AtomicU64>,
    last_text_hash: Arc<RwLock<u64>>,
    last_capture: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_app: Arc<RwLock<Option<String>>>,
    config: Arc<RwLock<AccessibilityCaptureConfig>>,
}

impl AccessibilityCaptureService {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            capture_count: Arc::new(AtomicU64::new(0)),
            saved_count: Arc::new(AtomicU64::new(0)),
            skipped_count: Arc::new(AtomicU64::new(0)),
            last_text_hash: Arc::new(RwLock::new(0)),
            last_capture: Arc::new(RwLock::new(None)),
            last_app: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(AccessibilityCaptureConfig::default())),
        }
    }

    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get current statistics
    pub fn get_stats(&self) -> AccessibilityCaptureStats {
        AccessibilityCaptureStats {
            running: self.running.load(Ordering::SeqCst),
            capture_count: self.capture_count.load(Ordering::SeqCst),
            saved_count: self.saved_count.load(Ordering::SeqCst),
            skipped_count: self.skipped_count.load(Ordering::SeqCst),
            last_capture: *self.last_capture.read(),
            last_app: self.last_app.read().clone(),
        }
    }

    /// Update configuration
    pub fn update_config(&self, config: AccessibilityCaptureConfig) {
        *self.config.write() = config;
    }

    /// Start the capture service
    pub fn start(
        &self,
        database: Arc<DatabaseManager>,
        settings: Arc<SettingsManager>,
        pinecone: Arc<RwLock<PineconeClient>>,
    ) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Accessibility capture already running".to_string());
        }

        // Check accessibility permission
        if !AccessibilityExtractor::is_trusted() {
            log::warn!("Accessibility capture: Permission not granted");
            // Request permission but don't block
            AccessibilityExtractor::request_permission_with_prompt();
            return Err("Accessibility permission required".to_string());
        }

        self.running.store(true, Ordering::SeqCst);
        log::info!("📝 Starting accessibility capture service");

        // Clone refs for the async task
        let running = self.running.clone();
        let capture_count = self.capture_count.clone();
        let saved_count = self.saved_count.clone();
        let skipped_count = self.skipped_count.clone();
        let last_text_hash = self.last_text_hash.clone();
        let last_capture = self.last_capture.clone();
        let last_app = self.last_app.clone();
        let config = self.config.clone();
        let pinecone = pinecone.clone();

        // Spawn background task
        tokio::spawn(async move {
            let extractor = AccessibilityExtractor::new();

            while running.load(Ordering::SeqCst) {
                // Get current config
                let cfg = config.read().clone();

                if !cfg.enabled {
                    // Sleep and check again
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                // Also check settings
                let settings_enabled = match settings.get_all().await {
                    Ok(s) => s.accessibility_capture_enabled,
                    Err(_) => false,
                };

                if !settings_enabled {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                // Perform capture
                match extractor.extract_focused_window() {
                    Ok(result) => {
                        capture_count.fetch_add(1, Ordering::SeqCst);
                        *last_capture.write() = Some(Utc::now());
                        *last_app.write() = result.app_name.clone();

                        // Calculate word count from text
                        let word_count = result.text.split_whitespace().count();

                        // Check word count threshold
                        if word_count < cfg.min_word_count as usize {
                            log::debug!(
                                "📝 Skipping capture: {} words < {} minimum",
                                word_count,
                                cfg.min_word_count
                            );
                            skipped_count.fetch_add(1, Ordering::SeqCst);
                        } else if cfg.deduplicate {
                            // Hash the text for dedup
                            let mut hasher = DefaultHasher::new();
                            result.text.hash(&mut hasher);
                            let new_hash = hasher.finish();

                            // Format text with context for vector DB and FTS
                            let context_text = format!(
                                "[App: {}] {}\n{}",
                                result.app_name.as_deref().unwrap_or("Unknown"),
                                result.window_title.as_deref().unwrap_or(""),
                                result.text
                            );

                            let prev_hash = *last_text_hash.read();
                            if new_hash == prev_hash {
                                log::debug!("📝 Skipping duplicate content");
                                skipped_count.fetch_add(1, Ordering::SeqCst);
                            } else {
                                // Save to database
                                *last_text_hash.write() = new_hash;
                                // Calculate quality score based on word count and accessibility status
                                let quality_score = if result.is_accessible && word_count > 20 {
                                    0.9
                                } else if result.is_accessible {
                                    0.7
                                } else {
                                    0.3
                                };
                                if let Err(e) = database
                                    .add_text_snapshot(
                                        &Uuid::new_v4().to_string(),
                                        None, // episode_id
                                        None, // state_id
                                        Utc::now(),
                                        &result.text,
                                        &format!("{:x}", new_hash),
                                        quality_score,
                                        "accessibility",
                                    )
                                    .await
                                {
                                    log::warn!("📝 Failed to save text snapshot: {}", e);
                                } else {
                                    saved_count.fetch_add(1, Ordering::SeqCst);
                                    log::debug!(
                                        "📝 Saved snapshot: {} words from {}",
                                        word_count,
                                        result.app_name.as_deref().unwrap_or("unknown")
                                    );

                                    // Trigger Pinecone embedding
                                    // Trigger Pinecone embedding
                                    let pinecone_config_opt = { pinecone.read().get_config() };

                                    if let Some(pinecone_config) = pinecone_config_opt {
                                        let id = format!("acc_{}", Uuid::new_v4());
                                        let metadata = serde_json::json!({
                                            "source": "accessibility",
                                            "app_name": result.app_name,
                                            "window_title": result.window_title,
                                            "timestamp": Utc::now().to_rfc3339(),
                                            "text": result.text.chars().take(1000).collect::<String>(), // Truncate for metadata
                                        });

                                        let _ = crate::pinecone_client::pinecone_upsert_generic(
                                            &pinecone_config,
                                            &id,
                                            &context_text,
                                            &metadata,
                                        )
                                        .await
                                        .map_err(|e| {
                                            log::warn!("📝 Pinecone upsert failed: {}", e)
                                        });
                                    }
                                }
                            }
                        } else {
                            // No dedup, always save
                            let mut hasher = DefaultHasher::new();
                            result.text.hash(&mut hasher);
                            let hash = hasher.finish();

                            // Format text with context for vector DB and FTS
                            let context_text = format!(
                                "[App: {}] {}\n{}",
                                result.app_name.as_deref().unwrap_or("Unknown"),
                                result.window_title.as_deref().unwrap_or(""),
                                result.text
                            );

                            // Calculate quality score
                            let quality_score = if result.is_accessible && word_count > 20 {
                                0.9
                            } else if result.is_accessible {
                                0.7
                            } else {
                                0.3
                            };

                            if let Err(e) = database
                                .add_text_snapshot(
                                    &Uuid::new_v4().to_string(),
                                    None,
                                    None,
                                    Utc::now(),
                                    &result.text,
                                    &format!("{:x}", hash),
                                    quality_score,
                                    "accessibility",
                                )
                                .await
                            {
                                log::warn!("📝 Failed to save text snapshot: {}", e);
                            } else {
                                saved_count.fetch_add(1, Ordering::SeqCst);

                                // Trigger Pinecone embedding
                                let pinecone_config_opt = { pinecone.read().get_config() };

                                if let Some(pinecone_config) = pinecone_config_opt {
                                    let id = format!("acc_{}", Uuid::new_v4());
                                    let metadata = serde_json::json!({
                                        "source": "accessibility",
                                        "app_name": result.app_name,
                                        "window_title": result.window_title,
                                        "timestamp": Utc::now().to_rfc3339(),
                                        "text": result.text.chars().take(1000).collect::<String>(),
                                    });

                                    let _ = crate::pinecone_client::pinecone_upsert_generic(
                                        &pinecone_config,
                                        &id,
                                        &context_text,
                                        &metadata,
                                    )
                                    .await
                                    .map_err(|e| log::warn!("📝 Pinecone upsert failed: {}", e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::debug!("📝 Capture failed: {}", e);
                    }
                }

                // Sleep for the configured interval
                tokio::time::sleep(Duration::from_secs(cfg.interval_secs as u64)).await;
            }

            log::info!("📝 Accessibility capture service stopped");
        });

        Ok(())
    }

    /// Stop the capture service
    pub fn stop(&self) {
        if self.running.load(Ordering::SeqCst) {
            log::info!("📝 Stopping accessibility capture service");
            self.running.store(false, Ordering::SeqCst);
        }
    }
}

impl Default for AccessibilityCaptureService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AccessibilityCaptureConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.interval_secs, 10);
        assert_eq!(config.min_word_count, 5);
        assert!(config.deduplicate);
    }

    #[test]
    fn test_service_creation() {
        let service = AccessibilityCaptureService::new();
        assert!(!service.is_running());

        let stats = service.get_stats();
        assert!(!stats.running);
        assert_eq!(stats.capture_count, 0);
    }
}
