// noFriction Meetings - Human Interaction Loop
// Provides prompts and confirmations for user engagement with recording sessions
//
// Features:
// - 30-minute check-in for manual recordings
// - Meeting-end confirmation (extend/snooze/end)
// - Break detection prompt (silence detection)
// - Frontend notification integration

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Types of interaction prompts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptType {
    /// Periodic check-in for manual recordings
    CheckIn,
    /// Confirmation when meeting end is detected
    MeetingEnd,
    /// Alert when long silence is detected
    BreakDetected,
    /// Storage limit approaching
    StorageWarning,
    /// Idle detection prompt
    IdleWarning,
}

/// User response to a prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptResponse {
    /// Continue / extend recording
    Continue,
    /// End recording now
    EndRecording,
    /// Snooze/dismiss for a period
    Snooze,
    /// User dismissed without action
    Dismissed,
    /// Prompt expired without response
    Expired,
}

/// A pending prompt waiting for user response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPrompt {
    pub id: String,
    pub prompt_type: PromptType,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub meeting_id: Option<String>,
}

/// Configuration for interaction loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionConfig {
    /// Enable 30-minute check-ins for manual recordings
    pub check_in_enabled: bool,
    /// Check-in interval in minutes
    pub check_in_interval_mins: u32,
    /// Enable meeting-end confirmations
    pub meeting_end_prompt_enabled: bool,
    /// Enable break detection prompts
    pub break_detection_enabled: bool,
    /// Silence threshold for break detection (seconds)
    pub break_silence_threshold_secs: u32,
    /// Enable storage warnings
    pub storage_warning_enabled: bool,
    /// Storage warning threshold (GB remaining)
    pub storage_warning_threshold_gb: f64,
    /// Prompt expiration time (seconds)
    pub prompt_timeout_secs: u32,
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            check_in_enabled: true,
            check_in_interval_mins: 30,
            meeting_end_prompt_enabled: true,
            break_detection_enabled: false, // Off by default - can be annoying
            break_silence_threshold_secs: 180, // 3 minutes
            storage_warning_enabled: true,
            storage_warning_threshold_gb: 5.0,
            prompt_timeout_secs: 60, // 1 minute to respond
        }
    }
}

/// Callback for prompt events
pub type PromptCallback = Arc<dyn Fn(PendingPrompt) + Send + Sync>;
pub type ResponseCallback = Arc<dyn Fn(PendingPrompt, PromptResponse) + Send + Sync>;

/// Statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InteractionStats {
    pub prompts_shown: u64,
    pub responses_continue: u64,
    pub responses_end: u64,
    pub responses_snooze: u64,
    pub responses_dismissed: u64,
    pub responses_expired: u64,
}

/// Interaction Loop Manager
pub struct InteractionLoop {
    config: Arc<RwLock<InteractionConfig>>,
    is_running: Arc<AtomicBool>,
    pending_prompts: Arc<RwLock<Vec<PendingPrompt>>>,
    stats: Arc<RwLock<InteractionStats>>,
    prompts_shown: Arc<AtomicU64>,
    last_check_in: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_audio_activity: Arc<RwLock<Option<DateTime<Utc>>>>,
    on_prompt: Arc<RwLock<Option<PromptCallback>>>,
    on_response: Arc<RwLock<Option<ResponseCallback>>>,
    app_handle: Option<AppHandle>,
}

impl InteractionLoop {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(InteractionConfig::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            pending_prompts: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(InteractionStats::default())),
            prompts_shown: Arc::new(AtomicU64::new(0)),
            last_check_in: Arc::new(RwLock::new(None)),
            last_audio_activity: Arc::new(RwLock::new(None)),
            on_prompt: Arc::new(RwLock::new(None)),
            on_response: Arc::new(RwLock::new(None)),
            app_handle: None,
        }
    }

    pub fn with_config(config: InteractionConfig) -> Self {
        let loop_mgr = Self::new();
        *loop_mgr.config.write() = config;
        loop_mgr
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app_handle = Some(app);
    }

    /// Set callback for when prompts are shown
    pub fn on_prompt(&self, callback: PromptCallback) {
        *self.on_prompt.write() = Some(callback);
    }

    /// Set callback for when user responds
    pub fn on_response(&self, callback: ResponseCallback) {
        *self.on_response.write() = Some(callback);
    }

    /// Get current configuration
    pub fn get_config(&self) -> InteractionConfig {
        self.config.read().clone()
    }

    /// Update configuration
    pub fn update_config(&self, config: InteractionConfig) {
        *self.config.write() = config;
        log::info!("💬 Interaction loop config updated");
    }

    /// Get statistics
    pub fn get_stats(&self) -> InteractionStats {
        self.stats.read().clone()
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Start the interaction loop
    pub fn start(&self) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err("Interaction loop already running".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        *self.last_check_in.write() = Some(Utc::now());

        log::info!("💬 Interaction loop started");

        // Start the check-in loop
        let is_running = self.is_running.clone();
        let config = self.config.clone();
        let pending_prompts = self.pending_prompts.clone();
        let stats = self.stats.clone();
        let prompts_shown = self.prompts_shown.clone();
        let last_check_in = self.last_check_in.clone();
        let on_prompt = self.on_prompt.clone();
        let app_handle = self.app_handle.clone();

        std::thread::spawn(move || {
            while is_running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(60)); // Check every minute

                let cfg = config.read().clone();
                let now = Utc::now();

                // Check if it's time for a check-in
                if cfg.check_in_enabled {
                    let last = last_check_in.read().clone();
                    let interval = Duration::minutes(cfg.check_in_interval_mins as i64);

                    if let Some(last_time) = last {
                        if now - last_time >= interval {
                            // Create check-in prompt
                            let prompt = PendingPrompt {
                                id: uuid::Uuid::new_v4().to_string(),
                                prompt_type: PromptType::CheckIn,
                                title: "Still recording?".to_string(),
                                message: format!(
                                    "You've been recording for {} minutes. Continue?",
                                    cfg.check_in_interval_mins
                                ),
                                created_at: now,
                                expires_at: Some(
                                    now + Duration::seconds(cfg.prompt_timeout_secs as i64),
                                ),
                                meeting_id: None,
                            };

                            log::info!("💬 Showing check-in prompt");
                            stats.write().prompts_shown += 1;
                            prompts_shown.fetch_add(1, Ordering::Relaxed);
                            pending_prompts.write().push(prompt.clone());

                            *last_check_in.write() = Some(now);

                            // Emit to frontend
                            if let Some(ref app) = app_handle {
                                let _ = app.emit("interaction-prompt", prompt.clone());
                            }

                            // Call callback
                            if let Some(ref cb) = *on_prompt.read() {
                                cb(prompt);
                            }
                        }
                    }
                }

                // Clean up expired prompts
                let _timeout = Duration::seconds(cfg.prompt_timeout_secs as i64);
                pending_prompts.write().retain(|p| {
                    if let Some(expires) = p.expires_at {
                        if now > expires {
                            stats.write().responses_expired += 1;
                            log::debug!("💬 Prompt expired: {}", p.id);
                            return false;
                        }
                    }
                    true
                });
            }

            log::info!("💬 Interaction loop stopped");
        });

        Ok(())
    }

    /// Stop the interaction loop
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        log::info!("💬 Interaction loop stopping...");
    }

    /// Show a meeting-end prompt
    pub fn show_meeting_end_prompt(&self, meeting_id: &str) {
        let cfg = self.config.read();
        if !cfg.meeting_end_prompt_enabled {
            return;
        }

        let now = Utc::now();
        let prompt = PendingPrompt {
            id: uuid::Uuid::new_v4().to_string(),
            prompt_type: PromptType::MeetingEnd,
            title: "Meeting ended?".to_string(),
            message: "It looks like your meeting has ended. Would you like to stop recording?"
                .to_string(),
            created_at: now,
            expires_at: Some(now + Duration::seconds(cfg.prompt_timeout_secs as i64)),
            meeting_id: Some(meeting_id.to_string()),
        };

        log::info!("💬 Showing meeting-end prompt for: {}", meeting_id);
        self.stats.write().prompts_shown += 1;
        self.prompts_shown.fetch_add(1, Ordering::Relaxed);
        self.pending_prompts.write().push(prompt.clone());

        if let Some(ref app) = self.app_handle {
            let _ = app.emit("interaction-prompt", prompt.clone());
        }

        if let Some(ref cb) = *self.on_prompt.read() {
            cb(prompt);
        }
    }

    /// Show a break detection prompt
    pub fn show_break_prompt(&self, silence_duration_secs: u64) {
        let cfg = self.config.read();
        if !cfg.break_detection_enabled {
            return;
        }

        let now = Utc::now();
        let prompt = PendingPrompt {
            id: uuid::Uuid::new_v4().to_string(),
            prompt_type: PromptType::BreakDetected,
            title: "On a break?".to_string(),
            message: format!(
                "No audio detected for {} minutes. Pause recording?",
                silence_duration_secs / 60
            ),
            created_at: now,
            expires_at: Some(now + Duration::seconds(cfg.prompt_timeout_secs as i64)),
            meeting_id: None,
        };

        log::info!("💬 Showing break-detection prompt");
        self.stats.write().prompts_shown += 1;
        self.pending_prompts.write().push(prompt.clone());

        if let Some(ref app) = self.app_handle {
            let _ = app.emit("interaction-prompt", prompt.clone());
        }

        if let Some(ref cb) = *self.on_prompt.read() {
            cb(prompt);
        }
    }

    /// Show storage warning
    pub fn show_storage_warning(&self, available_gb: f64) {
        let cfg = self.config.read();
        if !cfg.storage_warning_enabled {
            return;
        }

        let now = Utc::now();
        let prompt = PendingPrompt {
            id: uuid::Uuid::new_v4().to_string(),
            prompt_type: PromptType::StorageWarning,
            title: "Storage Running Low".to_string(),
            message: format!(
                "Only {:.1} GB remaining. Consider freeing up space.",
                available_gb
            ),
            created_at: now,
            expires_at: Some(now + Duration::seconds(120)), // 2 minutes for storage warning
            meeting_id: None,
        };

        log::warn!(
            "💬 Showing storage warning: {:.1} GB remaining",
            available_gb
        );
        self.stats.write().prompts_shown += 1;
        self.pending_prompts.write().push(prompt.clone());

        if let Some(ref app) = self.app_handle {
            let _ = app.emit("interaction-prompt", prompt.clone());
        }
    }

    /// Respond to a prompt
    pub fn respond(&self, prompt_id: &str, response: PromptResponse) -> Result<(), String> {
        let mut prompts = self.pending_prompts.write();
        let idx = prompts
            .iter()
            .position(|p| p.id == prompt_id)
            .ok_or_else(|| format!("Prompt not found: {}", prompt_id))?;

        let prompt = prompts.remove(idx);

        // Update stats
        {
            let mut stats = self.stats.write();
            match response {
                PromptResponse::Continue => stats.responses_continue += 1,
                PromptResponse::EndRecording => stats.responses_end += 1,
                PromptResponse::Snooze => stats.responses_snooze += 1,
                PromptResponse::Dismissed => stats.responses_dismissed += 1,
                PromptResponse::Expired => stats.responses_expired += 1,
            }
        }

        log::info!("💬 Prompt {} responded: {:?}", prompt_id, response);

        // Emit response to frontend
        if let Some(ref app) = self.app_handle {
            let _ = app.emit("interaction-response", (&prompt, response));
        }

        // Call callback
        if let Some(ref cb) = *self.on_response.read() {
            cb(prompt, response);
        }

        Ok(())
    }

    /// Get pending prompts
    pub fn get_pending_prompts(&self) -> Vec<PendingPrompt> {
        self.pending_prompts.read().clone()
    }

    /// Reset last check-in time (call when manually starting recording)
    pub fn reset_check_in(&self) {
        *self.last_check_in.write() = Some(Utc::now());
        log::debug!("💬 Check-in timer reset");
    }

    /// Update audio activity timestamp (for break detection)
    pub fn update_audio_activity(&self) {
        *self.last_audio_activity.write() = Some(Utc::now());
    }
}

impl Default for InteractionLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = InteractionConfig::default();
        assert!(config.check_in_enabled);
        assert_eq!(config.check_in_interval_mins, 30);
        assert!(config.meeting_end_prompt_enabled);
        assert!(!config.break_detection_enabled);
    }

    #[test]
    fn test_interaction_loop_lifecycle() {
        let loop_mgr = InteractionLoop::new();
        assert!(!loop_mgr.is_running());

        loop_mgr.start().unwrap();
        assert!(loop_mgr.is_running());

        loop_mgr.stop();
        // Note: stop is async, just check it doesn't panic
    }

    #[test]
    fn test_stats() {
        let loop_mgr = InteractionLoop::new();
        let stats = loop_mgr.get_stats();
        assert_eq!(stats.prompts_shown, 0);
    }
}
