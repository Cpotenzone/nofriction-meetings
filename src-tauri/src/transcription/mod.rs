use crate::database::DatabaseManager;
use crate::live_intel_agent::LiveIntelAgent;
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;

pub mod deepgram;
pub mod gemini;
pub mod gladia;
pub mod google_stt;

/// Core trait for all transcription providers
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Start the connection/session
    fn start(&self);

    /// Stop the connection/session
    fn stop(&self);

    /// Send audio chunk (f32 samples, sample rate, channels)
    fn process_audio(&self, samples: &[f32], sample_rate: u32, channels: u16);

    /// Check if connected/active
    fn is_active(&self) -> bool;

    /// Update API Key configuration
    fn set_api_key(&self, key: String);

    /// Set context (meeting ID, database, etc.)
    fn set_context(
        &self,
        app_handle: AppHandle,
        database: Arc<DatabaseManager>,
        meeting_id: String,
        live_intel_agent: Arc<RwLock<LiveIntelAgent>>,
    );
}

/// Enum for supported providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Deepgram,
    Gemini,
    Gladia,
    GoogleSTT,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::Deepgram
    }
}

/// Manager to switch between providers safely
pub struct TranscriptionManager {
    current_provider: Arc<RwLock<Box<dyn TranscriptionProvider>>>,
    provider_type: Arc<RwLock<ProviderType>>,
    /// Per-provider API key store — survives provider switches
    api_keys: Arc<RwLock<HashMap<ProviderType, String>>>,
}

impl TranscriptionManager {
    pub fn new() -> Self {
        // Default to Deepgram initially
        let default_provider = deepgram::DeepgramProvider::new();

        Self {
            current_provider: Arc::new(RwLock::new(Box::new(default_provider))),
            provider_type: Arc::new(RwLock::new(ProviderType::Deepgram)),
            api_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Switch the active provider (re-applies stored API key automatically)
    pub fn switch_provider(&self, provider_type: ProviderType) {
        // Stop current provider first
        self.stop();

        let new_provider: Box<dyn TranscriptionProvider> = match provider_type {
            ProviderType::Deepgram => Box::new(deepgram::DeepgramProvider::new()),
            ProviderType::Gemini => Box::new(gemini::GeminiProvider::new()),
            ProviderType::Gladia => Box::new(gladia::GladiaProvider::new()),
            ProviderType::GoogleSTT => Box::new(google_stt::GoogleSTTProvider::new()),
        };

        // Re-apply stored API key for this provider type (if any)
        if let Some(key) = self.api_keys.read().get(&provider_type) {
            new_provider.set_api_key(key.clone());
            log::info!("Re-applied stored API key for {:?}", provider_type);
        }

        *self.current_provider.write() = new_provider;
        *self.provider_type.write() = provider_type;
        log::info!("Switched transcription provider to {:?}", provider_type);
    }

    pub fn get_provider_type(&self) -> ProviderType {
        *self.provider_type.read()
    }

    /// Store an API key for a specific provider type (persists across switches)
    pub fn set_api_key_for_provider(&self, provider_type: ProviderType, key: String) {
        self.api_keys.write().insert(provider_type, key.clone());
        // If this is the currently active provider, also set it on the live instance
        if *self.provider_type.read() == provider_type {
            self.current_provider.read().set_api_key(key);
        }
    }

    /// Check if a key exists for a given provider type
    pub fn has_key_for_provider(&self, provider_type: ProviderType) -> bool {
        self.api_keys
            .read()
            .get(&provider_type)
            .map(|k| !k.is_empty())
            .unwrap_or(false)
    }

    // Proxy methods
    pub fn start(&self) {
        self.current_provider.read().start();
    }

    pub fn stop(&self) {
        self.current_provider.read().stop();
    }

    pub fn process_audio(&self, samples: &[f32], sample_rate: u32, channels: u16) {
        self.current_provider
            .read()
            .process_audio(samples, sample_rate, channels);
    }

    pub fn is_active(&self) -> bool {
        self.current_provider.read().is_active()
    }

    /// Set API key on the current active provider AND store it for persistence
    pub fn set_api_key(&self, key: String) {
        let provider_type = *self.provider_type.read();
        self.api_keys.write().insert(provider_type, key.clone());
        self.current_provider.read().set_api_key(key);
    }

    pub fn set_context(
        &self,
        app_handle: AppHandle,
        database: Arc<DatabaseManager>,
        meeting_id: String,
        live_intel_agent: Arc<RwLock<LiveIntelAgent>>,
    ) {
        self.current_provider.read().set_context(
            app_handle,
            database,
            meeting_id,
            live_intel_agent,
        );
    }
}
