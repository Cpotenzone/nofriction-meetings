use crate::database::DatabaseManager;
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
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

    /// Send audio chunk (f32 samples, sample rate)
    fn process_audio(&self, samples: &[f32], sample_rate: u32);

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
    );
}

/// Enum for supported providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl TranscriptionManager {
    pub fn new() -> Self {
        // Default to Deepgram initially
        let default_provider = deepgram::DeepgramProvider::new();

        Self {
            current_provider: Arc::new(RwLock::new(Box::new(default_provider))),
            provider_type: Arc::new(RwLock::new(ProviderType::Deepgram)),
        }
    }

    /// Switch the active provider
    pub fn switch_provider(&self, provider_type: ProviderType) {
        // Stop current provider first
        self.stop();

        let new_provider: Box<dyn TranscriptionProvider> = match provider_type {
            ProviderType::Deepgram => Box::new(deepgram::DeepgramProvider::new()),
            ProviderType::Gemini => Box::new(gemini::GeminiProvider::new()),
            ProviderType::Gladia => Box::new(gladia::GladiaProvider::new()),
            ProviderType::GoogleSTT => Box::new(google_stt::GoogleSTTProvider::new()),
        };

        *self.current_provider.write() = new_provider;
        *self.provider_type.write() = provider_type;
        log::info!("Switched transcription provider to {:?}", provider_type);
    }

    pub fn get_provider_type(&self) -> ProviderType {
        *self.provider_type.read()
    }

    // Proxy methods
    pub fn start(&self) {
        self.current_provider.read().start();
    }

    pub fn stop(&self) {
        self.current_provider.read().stop();
    }

    pub fn process_audio(&self, samples: &[f32], sample_rate: u32) {
        self.current_provider
            .read()
            .process_audio(samples, sample_rate);
    }

    pub fn is_active(&self) -> bool {
        self.current_provider.read().is_active()
    }

    pub fn set_api_key(&self, key: String) {
        self.current_provider.read().set_api_key(key);
    }

    pub fn set_context(
        &self,
        app_handle: AppHandle,
        database: Arc<DatabaseManager>,
        meeting_id: String,
    ) {
        self.current_provider
            .read()
            .set_context(app_handle, database, meeting_id);
    }
}
