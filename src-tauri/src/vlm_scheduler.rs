//! VLM Scheduler - Background worker for automatic frame analysis
//!
//! Runs on a configurable timer and processes pending frames with VLM.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::database::DatabaseManager;
use crate::settings::SettingsManager;

/// VLM Scheduler status
#[derive(Debug, Clone, serde::Serialize)]
pub struct VLMSchedulerStatus {
    pub running: bool,
    pub enabled: bool,
    pub interval_secs: u32,
    pub frames_processed: u64,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub pending_frames: i64,
}

use crate::prompt_manager::PromptManager;

/// VLM Scheduler - manages background processing
pub struct VLMScheduler {
    running: Arc<AtomicBool>,
    enabled: Arc<AtomicBool>,
    interval_secs: Arc<RwLock<u32>>,
    frames_processed: Arc<AtomicU64>,
    last_run: Arc<RwLock<Option<DateTime<Utc>>>>,
    database: Arc<DatabaseManager>,
    settings: Arc<SettingsManager>,
    prompt_manager: Arc<PromptManager>,
}

impl VLMScheduler {
    pub fn new(
        database: Arc<DatabaseManager>,
        settings: Arc<SettingsManager>,
        prompt_manager: Arc<PromptManager>,
    ) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            enabled: Arc::new(AtomicBool::new(false)),
            interval_secs: Arc::new(RwLock::new(120)),
            frames_processed: Arc::new(AtomicU64::new(0)),
            last_run: Arc::new(RwLock::new(None)),
            database,
            settings,
            prompt_manager,
        }
    }

    /// Set enabled state
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        log::info!("VLM Scheduler enabled: {}", enabled);
    }

    /// Set processing interval
    pub fn set_interval(&self, secs: u32) {
        let clamped = secs.clamp(30, 600);
        *self.interval_secs.write() = clamped;
        log::info!("VLM Scheduler interval: {}s", clamped);
    }

    /// Get current status
    pub async fn get_status(&self) -> VLMSchedulerStatus {
        let pending = self.database.count_unsynced_frames().await.unwrap_or(0);
        let interval = *self.interval_secs.read();
        let last = self.last_run.read().clone();

        let next = if self.enabled.load(Ordering::SeqCst) && self.running.load(Ordering::SeqCst) {
            last.map(|l| l + chrono::Duration::seconds(interval as i64))
        } else {
            None
        };

        VLMSchedulerStatus {
            running: self.running.load(Ordering::SeqCst),
            enabled: self.enabled.load(Ordering::SeqCst),
            interval_secs: interval,
            frames_processed: self.frames_processed.load(Ordering::SeqCst),
            last_run: last.map(|l| l.to_rfc3339()),
            next_run: next.map(|n| n.to_rfc3339()),
            pending_frames: pending,
        }
    }

    /// Start the scheduler loop
    pub fn start(&self) {
        if self.running.load(Ordering::SeqCst) {
            log::info!("VLM Scheduler already running");
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        log::info!("VLM Scheduler starting...");

        let running = self.running.clone();
        let enabled = self.enabled.clone();
        let interval_secs = self.interval_secs.clone();
        let frames_processed = self.frames_processed.clone();
        let last_run = self.last_run.clone();
        let database = self.database.clone();
        let settings = self.settings.clone();

        let prompt_manager = self.prompt_manager.clone();

        tokio::spawn(async move {
            Self::run_loop(
                running,
                enabled,
                interval_secs,
                frames_processed,
                last_run,
                database,
                settings,
                prompt_manager,
            )
            .await;
        });
    }

    /// Stop the scheduler loop
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        log::info!("VLM Scheduler stopped");
    }

    /// Main processing loop
    #[allow(clippy::too_many_arguments)]
    async fn run_loop(
        running: Arc<AtomicBool>,
        enabled: Arc<AtomicBool>,
        interval_secs: Arc<RwLock<u32>>,
        frames_processed: Arc<AtomicU64>,
        last_run: Arc<RwLock<Option<DateTime<Utc>>>>,
        database: Arc<DatabaseManager>,
        settings: Arc<SettingsManager>,
        prompt_manager: Arc<PromptManager>,
    ) {
        log::info!("VLM Scheduler loop started");

        while running.load(Ordering::SeqCst) {
            // Sleep for the configured interval
            let interval = *interval_secs.read();
            tokio::time::sleep(std::time::Duration::from_secs(interval as u64)).await;

            // Check if still running and enabled
            if !running.load(Ordering::SeqCst) {
                break;
            }

            if !enabled.load(Ordering::SeqCst) {
                continue;
            }

            // Load settings to get VLM config
            let app_settings = match settings.get_all().await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("VLM Scheduler: Failed to load settings: {}", e);
                    continue;
                }
            };

            // Check if VLM processing is enabled in settings
            if !app_settings.vlm_auto_process {
                continue;
            }

            // Get pending frames
            let pending = match database.get_pending_frames(10).await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("VLM Scheduler: Failed to get pending frames: {}", e);
                    continue;
                }
            };

            if pending.is_empty() {
                log::debug!("VLM Scheduler: No pending frames");
                *last_run.write() = Some(Utc::now());
                continue;
            }

            log::info!("VLM Scheduler: Processing {} frames...", pending.len());

            // Check VLM availability (centralized API)
            if !crate::vlm_client::vlm_is_available().await {
                log::warn!("VLM Scheduler: VLM API not available, skipping");
                continue;
            }

            // Determine active theme and load prompt
            let active_theme = app_settings.active_theme;
            // The prompt key convention is "{theme}_context_analysis"
            // Fallback to "frame_analysis" if specific theme prompt not found
            let prompt_key = format!("{}_context_analysis", active_theme);

            let prompt_text = match prompt_manager.get_prompt(&prompt_key).await {
                Ok(Some(p)) => {
                    log::info!(
                        "VLM Scheduler: Using prompt '{}' for theme '{}'",
                        p.name,
                        active_theme
                    );
                    p.system_prompt
                }
                Ok(None) => {
                    // Try fallback
                    log::warn!(
                        "VLM Scheduler: Prompt '{}' not found, falling back to 'frame_analysis'",
                        prompt_key
                    );
                    match prompt_manager.get_prompt("frame_analysis").await {
                        Ok(Some(p)) => p.system_prompt,
                        _ => {
                            log::error!("VLM Scheduler: Fallback prompt 'frame_analysis' not found. Using hardcoded default.");
                            r#"Analyze this screenshot and describe what the user is doing. 
                            Respond in JSON format with these fields:
                            {
                              "app_name": "name of the main application visible",
                              "window_title": "title of the window or document",
                              "category": "one of: development, communication, research, system, other",
                              "summary": "brief description of what the user is doing",
                              "focus_area": "specific task or project",
                              "visible_files": [],
                              "confidence": 0.5
                            }
                            Only respond with valid JSON."#.to_string()
                        }
                    }
                }
                Err(e) => {
                    log::error!("VLM Scheduler: Failed to get prompt: {}", e);
                    continue;
                }
            };

            // Process frames
            let mut processed = 0;
            for frame in pending {
                match crate::vlm_client::vlm_analyze_frame(&frame.frame_path, &prompt_text).await {
                    Ok(context) => {
                        // Create activity log entry
                        let activity = crate::database::ActivityLogEntry {
                            id: None,
                            start_time: frame.captured_at,
                            end_time: None,
                            duration_seconds: None,
                            app_name: context.app_name,
                            window_title: context.window_title,
                            category: context.category,
                            summary: context.summary,
                            focus_area: context.focus_area,
                            visible_files: if context.visible_files.is_empty() {
                                None
                            } else {
                                Some(context.visible_files.join(", "))
                            },
                            confidence: Some(context.confidence),
                            frame_ids: Some(frame.id.to_string()),
                            pinecone_id: None,
                            supabase_id: None,
                            synced_at: None,
                        };

                        if let Ok(activity_id) = database.add_activity(&activity).await {
                            // Phase 3: Extract and store entities
                            if let Some(entities_json) = context.entities {
                                if let Some(obj) = entities_json.as_object() {
                                    for (entity_type, list) in obj {
                                        // Only process arrays as entity lists (skip single fields like app_name)
                                        if let Some(items) = list.as_array() {
                                            for item in items {
                                                // Item must have a 'name' field to be a valid entity
                                                if let Some(name) =
                                                    item.get("name").and_then(|s| s.as_str())
                                                {
                                                    // Extract confidence if present, else use context confidence
                                                    let conf = item
                                                        .get("confidence")
                                                        .and_then(|c| c.as_f64()) // Handle number
                                                        .or_else(|| {
                                                            item.get("confidence").and_then(|s| {
                                                                s.as_str().map(|_| 0.8)
                                                            })
                                                        }) // Handle string (high/med/low) - simplified
                                                        .map(|f| f as f32)
                                                        .unwrap_or(context.confidence);

                                                    let _ = database
                                                        .add_entity(
                                                            activity_id,
                                                            entity_type,
                                                            name,
                                                            Some(item),
                                                            conf,
                                                            Some(&active_theme),
                                                        )
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let _ = database.mark_frame_analyzed(frame.id).await;
                            processed += 1;
                        }
                    }
                    Err(e) => {
                        log::warn!("VLM Scheduler: Failed to analyze frame {}: {}", frame.id, e);
                    }
                }
            }

            frames_processed.fetch_add(processed, Ordering::SeqCst);
            *last_run.write() = Some(Utc::now());
            log::info!("VLM Scheduler: Processed {} frames", processed);
        }

        log::info!("VLM Scheduler loop stopped");
    }
}
