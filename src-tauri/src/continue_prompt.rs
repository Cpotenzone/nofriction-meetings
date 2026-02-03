// noFriction Meetings - Continue Recording Prompts
//
// Implements 30-minute prompts to continue recording
// with auto-stop after missed prompts

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Configuration for continue prompts
#[derive(Clone, Debug)]
pub struct ContinuePromptConfig {
    /// Interval between prompts (default: 30 minutes)
    pub prompt_interval_minutes: i64,
    /// Number of missed prompts before auto-stop
    pub max_missed_prompts: u32,
    /// Whether prompts are enabled
    pub enabled: bool,
}

impl Default for ContinuePromptConfig {
    fn default() -> Self {
        Self {
            prompt_interval_minutes: 30,
            max_missed_prompts: 2,
            enabled: true,
        }
    }
}

/// Event payload for continue prompts
#[derive(Clone, Debug, serde::Serialize)]
pub struct ContinuePromptEvent {
    /// When the recording started
    pub recording_started: DateTime<Utc>,
    /// How long recording has been active
    pub duration_minutes: i64,
    /// Number of prompts sent so far
    pub prompt_count: u32,
    /// Whether this is an auto-stop warning
    pub is_final_warning: bool,
}

/// Manages continue recording prompts
pub struct ContinuePromptManager {
    app_handle: Option<AppHandle>,
    is_recording: Arc<AtomicBool>,
    recording_start: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_prompt: Arc<RwLock<Option<DateTime<Utc>>>>,
    user_responded: Arc<AtomicBool>,
    missed_prompts: Arc<RwLock<u32>>,
    config: Arc<RwLock<ContinuePromptConfig>>,
    is_running: Arc<AtomicBool>,
}

impl ContinuePromptManager {
    pub fn new() -> Self {
        Self {
            app_handle: None,
            is_recording: Arc::new(AtomicBool::new(false)),
            recording_start: Arc::new(RwLock::new(None)),
            last_prompt: Arc::new(RwLock::new(None)),
            user_responded: Arc::new(AtomicBool::new(true)),
            missed_prompts: Arc::new(RwLock::new(0)),
            config: Arc::new(RwLock::new(ContinuePromptConfig::default())),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_app_handle(mut self, handle: AppHandle) -> Self {
        self.app_handle = Some(handle);
        self
    }

    /// Start monitoring for continue prompts
    pub fn start(&self) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err("Continue prompt manager already running".to_string());
        }

        self.is_running.store(true, Ordering::SeqCst);
        log::info!("⏰ Continue prompt manager started");

        let is_running = self.is_running.clone();
        let is_recording = self.is_recording.clone();
        let recording_start = self.recording_start.clone();
        let last_prompt = self.last_prompt.clone();
        let user_responded = self.user_responded.clone();
        let missed_prompts = self.missed_prompts.clone();
        let config = self.config.clone();
        let app_handle = self.app_handle.clone();

        std::thread::spawn(move || {
            while is_running.load(Ordering::SeqCst) {
                // Check every minute
                std::thread::sleep(std::time::Duration::from_secs(60));

                let cfg = config.read().clone();
                if !cfg.enabled {
                    continue;
                }

                // Only prompt if recording is active
                if !is_recording.load(Ordering::SeqCst) {
                    continue;
                }

                let now = Utc::now();
                let start = match *recording_start.read() {
                    Some(s) => s,
                    None => continue,
                };

                // Check if it's time for a prompt
                let last = last_prompt.read().unwrap_or(start);
                let minutes_since_last = (now - last).num_minutes();

                if minutes_since_last >= cfg.prompt_interval_minutes {
                    // Check if user responded to last prompt
                    if !user_responded.load(Ordering::SeqCst) {
                        let mut missed = missed_prompts.write();
                        *missed += 1;
                        log::warn!("⏰ User missed prompt #{}", *missed);

                        if *missed >= cfg.max_missed_prompts {
                            log::warn!("⏰ Max missed prompts reached, auto-stopping");
                            if let Some(ref app) = app_handle {
                                let _ = app.emit(
                                    "recording-auto-stop",
                                    serde_json::json!({
                                        "reason": "missed_prompts",
                                        "missed_count": *missed
                                    }),
                                );
                            }
                            // Reset state
                            *missed_prompts.write() = 0;
                            is_recording.store(false, Ordering::SeqCst);
                            continue;
                        }
                    }

                    // Send new prompt
                    let duration_minutes = (now - start).num_minutes();
                    let prompt_count = *missed_prompts.read() + 1;
                    let is_final = prompt_count >= cfg.max_missed_prompts;

                    let event = ContinuePromptEvent {
                        recording_started: start,
                        duration_minutes,
                        prompt_count,
                        is_final_warning: is_final,
                    };

                    log::info!(
                        "⏰ Sending continue prompt: {} minutes recorded",
                        duration_minutes
                    );

                    if let Some(ref app) = app_handle {
                        let _ = app.emit("continue-recording-prompt", event);
                    }

                    // Update state
                    *last_prompt.write() = Some(now);
                    user_responded.store(false, Ordering::SeqCst);
                }
            }

            log::info!("⏰ Continue prompt manager stopped");
        });

        Ok(())
    }

    /// Stop the prompt manager
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    /// Signal that recording has started
    pub fn recording_started(&self) {
        let now = Utc::now();
        self.is_recording.store(true, Ordering::SeqCst);
        *self.recording_start.write() = Some(now);
        *self.last_prompt.write() = Some(now);
        *self.missed_prompts.write() = 0;
        self.user_responded.store(true, Ordering::SeqCst);
        log::info!("⏰ Recording started, prompts will begin in 30 minutes");
    }

    /// Signal that recording has stopped
    pub fn recording_stopped(&self) {
        self.is_recording.store(false, Ordering::SeqCst);
        *self.recording_start.write() = None;
        *self.last_prompt.write() = None;
        *self.missed_prompts.write() = 0;
        log::info!("⏰ Recording stopped, prompts disabled");
    }

    /// User confirmed to continue recording
    pub fn user_confirmed(&self) {
        self.user_responded.store(true, Ordering::SeqCst);
        *self.missed_prompts.write() = 0;
        log::info!("⏰ User confirmed continue recording");
    }

    /// Update configuration
    pub fn update_config(&self, config: ContinuePromptConfig) {
        *self.config.write() = config;
    }
}

impl Default for ContinuePromptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ContinuePromptConfig::default();
        assert_eq!(config.prompt_interval_minutes, 30);
        assert_eq!(config.max_missed_prompts, 2);
        assert!(config.enabled);
    }
}
