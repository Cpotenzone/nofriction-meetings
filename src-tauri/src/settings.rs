// noFriction Meetings - Settings Manager
// Persistent settings storage using SQLite

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub deepgram_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub gladia_api_key: Option<String>,
    pub google_stt_key_json: Option<String>,
    pub transcription_provider: String, // "deepgram", "gemini", "gladia", "google_stt"
    pub selected_microphone: Option<String>,
    pub selected_monitor: Option<u32>,
    pub auto_start_recording: bool,
    pub show_notifications: bool,
    // Capture mode settings
    pub capture_microphone: bool,
    pub capture_system_audio: bool,
    pub capture_screen: bool,
    pub always_on_capture: bool,
    pub queue_frames_for_vlm: bool,
    pub frame_capture_interval_ms: u32,
    // VLM auto-processing settings
    pub vlm_auto_process: bool,
    pub vlm_process_interval_secs: u32,
    // AI chat settings
    pub ai_chat_model: Option<String>,
    // Activity theme settings
    pub active_theme: String,
    pub prospecting_interval_ms: u32,
    pub fundraising_interval_ms: u32,
    pub product_dev_interval_ms: u32,
    pub admin_interval_ms: u32,
    pub personal_interval_ms: u32,
    // Knowledge base settings
    pub supabase_connection_string: Option<String>,
    pub pinecone_api_key: Option<String>,
    pub pinecone_index_host: Option<String>,
    pub pinecone_namespace: Option<String>,
    // Intelligence Pipeline settings
    pub enable_ingest: Option<bool>,
    pub ingest_base_url: Option<String>,
    pub ingest_bearer_token: Option<String>,
    // VLM API settings (centralized service)
    pub vlm_base_url: Option<String>,
    pub vlm_bearer_token: Option<String>,
    pub vlm_model_primary: Option<String>,
    pub vlm_model_fallback: Option<String>,
    // Stateful Screen Ingest - Dedup thresholds (Phase 1)
    pub dedup_hash_threshold: Option<u32>, // Hamming distance threshold for hash comparison
    pub dedup_delta_threshold: Option<f64>, // Mean pixel delta threshold (0.0 - 1.0)
    pub dedup_enabled: Option<bool>,       // Enable/disable dedup pipeline
    pub snapshot_interval_secs: Option<u64>, // Periodic checkpoint interval
    // Accessibility capture settings
    pub accessibility_capture_enabled: bool,
    pub accessibility_capture_interval_secs: u32,
    // AI Provider settings
    pub ai_provider: String, // "local" or "remote"
    pub ai_remote_url: Option<String>,
    pub ai_remote_key: Option<String>,
}

impl AppSettings {
    /// Create with sensible defaults
    pub fn with_defaults() -> Self {
        Self {
            deepgram_api_key: None,
            gemini_api_key: None,
            gladia_api_key: None,
            google_stt_key_json: None,
            transcription_provider: "deepgram".to_string(),
            selected_microphone: None,
            selected_monitor: None,
            auto_start_recording: false,
            show_notifications: true,
            capture_microphone: true,                // Mic on by default
            capture_system_audio: true,              // System audio ON for meeting capture
            capture_screen: false,                   // Screen capture OFF by default (reduces CPU)
            always_on_capture: false,                // Not always-on by default
            queue_frames_for_vlm: false,             // VLM OFF by default (saves resources)
            frame_capture_interval_ms: 5000,         // 5 sec instead of 1 (5x less disk I/O)
            vlm_auto_process: false,                 // Auto-processing OFF by default
            vlm_process_interval_secs: 120,          // 2 minutes default interval
            ai_chat_model: None,                     // Will use first available model
            active_theme: "prospecting".to_string(), // Default theme
            prospecting_interval_ms: 1500,           // 1.5 seconds
            fundraising_interval_ms: 1500,           // 1.5 seconds
            product_dev_interval_ms: 2000,           // 2 seconds
            admin_interval_ms: 2000,                 // 2 seconds
            personal_interval_ms: 3000,              // 3 seconds
            supabase_connection_string: None,
            pinecone_api_key: None,
            pinecone_index_host: None,
            pinecone_namespace: Some("default".to_string()),
            enable_ingest: Some(false), // Disabled by default
            ingest_base_url: None,
            ingest_bearer_token: None,
            vlm_base_url: Some("https://7wk6vrq9achr2djw.caas.targon.com".to_string()), // TheBrain Cloud API
            vlm_bearer_token: None,
            vlm_model_primary: Some("qwen2.5vl:7b".to_string()),
            vlm_model_fallback: Some("qwen2.5vl:3b".to_string()),
            // Stateful Screen Ingest defaults
            dedup_hash_threshold: Some(5), // 5 bits Hamming distance tolerance
            dedup_delta_threshold: Some(0.02), // 2% mean pixel delta tolerance
            dedup_enabled: Some(true),     // Dedup enabled by default
            snapshot_interval_secs: Some(30), // 30 second checkpoint interval
            // Accessibility capture defaults
            accessibility_capture_enabled: false, // OFF by default
            accessibility_capture_interval_secs: 10, // 10 seconds
            // AI Provider defaults
            ai_provider: "remote".to_string(), // Default to remote for reliability
            ai_remote_url: None,
            ai_remote_key: None,
        }
    }
}

/// Settings manager for persistent storage
pub struct SettingsManager {
    pool: Arc<SqlitePool>,
}

impl SettingsManager {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Initialize settings table
    pub async fn init(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// Get a setting value
    pub async fn get(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.pool.as_ref())
            .await?;

        Ok(result.map(|(v,)| v))
    }

    /// Set a setting value
    pub async fn set(&self, key: &str, value: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// Delete a setting
    pub async fn delete(&self, key: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    /// Get all settings as AppSettings struct
    pub async fn get_all(&self) -> Result<AppSettings, sqlx::Error> {
        let mut settings = AppSettings::with_defaults();

        if let Some(key) = self.get("deepgram_api_key").await? {
            settings.deepgram_api_key = Some(key);
        }
        if let Some(key) = self.get("gemini_api_key").await? {
            settings.gemini_api_key = Some(key);
        }
        if let Some(key) = self.get("gladia_api_key").await? {
            settings.gladia_api_key = Some(key);
        }
        if let Some(key) = self.get("google_stt_key_json").await? {
            settings.google_stt_key_json = Some(key);
        }
        if let Some(prov) = self.get("transcription_provider").await? {
            settings.transcription_provider = prov;
        }
        if let Some(mic) = self.get("selected_microphone").await? {
            settings.selected_microphone = Some(mic);
        }
        if let Some(monitor) = self.get("selected_monitor").await? {
            settings.selected_monitor = monitor.parse().ok();
        }
        if let Some(auto) = self.get("auto_start_recording").await? {
            settings.auto_start_recording = auto == "true";
        }
        if let Some(notify) = self.get("show_notifications").await? {
            settings.show_notifications = notify == "true";
        }
        // Capture settings
        if let Some(v) = self.get("capture_microphone").await? {
            settings.capture_microphone = v == "true";
        }
        if let Some(v) = self.get("capture_system_audio").await? {
            settings.capture_system_audio = v == "true";
        }
        if let Some(v) = self.get("capture_screen").await? {
            settings.capture_screen = v == "true";
        }
        if let Some(v) = self.get("always_on_capture").await? {
            settings.always_on_capture = v == "true";
        }
        if let Some(v) = self.get("queue_frames_for_vlm").await? {
            settings.queue_frames_for_vlm = v == "true";
        }
        if let Some(v) = self.get("frame_capture_interval_ms").await? {
            settings.frame_capture_interval_ms = v.parse().unwrap_or(1000);
        }
        // Knowledge base settings
        if let Some(v) = self.get("supabase_connection_string").await? {
            settings.supabase_connection_string = Some(v);
        }
        if let Some(v) = self.get("pinecone_api_key").await? {
            settings.pinecone_api_key = Some(v);
        }
        if let Some(v) = self.get("pinecone_index_host").await? {
            settings.pinecone_index_host = Some(v);
        }
        if let Some(v) = self.get("pinecone_namespace").await? {
            settings.pinecone_namespace = Some(v);
        }
        // VLM auto-processing settings
        if let Some(v) = self.get("vlm_auto_process").await? {
            settings.vlm_auto_process = v == "true";
        }
        if let Some(v) = self.get("vlm_process_interval_secs").await? {
            settings.vlm_process_interval_secs = v.parse().unwrap_or(120);
        }
        // AI chat model
        if let Some(v) = self.get("ai_chat_model").await? {
            settings.ai_chat_model = Some(v);
        }
        // Activity theme settings
        if let Some(v) = self.get("active_theme").await? {
            settings.active_theme = v;
        }
        if let Some(v) = self.get("prospecting_interval_ms").await? {
            settings.prospecting_interval_ms = v.parse().unwrap_or(1500);
        }
        if let Some(v) = self.get("fundraising_interval_ms").await? {
            settings.fundraising_interval_ms = v.parse().unwrap_or(1500);
        }
        if let Some(v) = self.get("product_dev_interval_ms").await? {
            settings.product_dev_interval_ms = v.parse().unwrap_or(2000);
        }
        if let Some(v) = self.get("admin_interval_ms").await? {
            settings.admin_interval_ms = v.parse().unwrap_or(2000);
        }
        if let Some(v) = self.get("personal_interval_ms").await? {
            settings.personal_interval_ms = v.parse().unwrap_or(3000);
        }

        // Intelligence Pipeline settings
        if let Some(v) = self.get("enable_ingest").await? {
            settings.enable_ingest = Some(v == "true");
        }
        if let Some(v) = self.get("ingest_base_url").await? {
            settings.ingest_base_url = Some(v);
        }
        if let Some(v) = self.get("ingest_bearer_token").await? {
            settings.ingest_bearer_token = Some(v);
        }

        // VLM API settings (centralized service)
        if let Some(v) = self.get("vlm_base_url").await? {
            settings.vlm_base_url = Some(v);
        }
        if let Some(v) = self.get("vlm_bearer_token").await? {
            settings.vlm_bearer_token = Some(v);
        }
        if let Some(v) = self.get("vlm_model_primary").await? {
            settings.vlm_model_primary = Some(v);
        }
        if let Some(v) = self.get("vlm_model_fallback").await? {
            settings.vlm_model_fallback = Some(v);
        }

        // Accessibility capture settings
        if let Some(v) = self.get("accessibility_capture_enabled").await? {
            settings.accessibility_capture_enabled = v == "true";
        }
        if let Some(v) = self.get("accessibility_capture_interval_secs").await? {
            settings.accessibility_capture_interval_secs = v.parse().unwrap_or(10);
        }

        // AI Provider settings
        if let Some(v) = self.get("ai_provider").await? {
            settings.ai_provider = v;
        }
        if let Some(v) = self.get("ai_remote_url").await? {
            settings.ai_remote_url = Some(v);
        }
        if let Some(v) = self.get("ai_remote_key").await? {
            settings.ai_remote_key = Some(v);
        }

        Ok(settings)
    }

    /// Save Deepgram API key
    pub async fn set_deepgram_api_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("deepgram_api_key", key).await
    }

    /// Get Deepgram API key
    pub async fn get_deepgram_api_key(&self) -> Result<Option<String>, sqlx::Error> {
        self.get("deepgram_api_key").await
    }

    /// Save Gemini API key
    pub async fn set_gemini_api_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("gemini_api_key", key).await
    }

    /// Save Gladia API key
    pub async fn set_gladia_api_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("gladia_api_key", key).await
    }

    /// Save Google STT key JSON
    pub async fn set_google_stt_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("google_stt_key_json", key).await
    }

    /// Set transcription provider
    pub async fn set_transcription_provider(&self, provider: &str) -> Result<(), sqlx::Error> {
        self.set("transcription_provider", provider).await
    }

    /// Save selected microphone
    pub async fn set_selected_microphone(&self, mic_id: &str) -> Result<(), sqlx::Error> {
        self.set("selected_microphone", mic_id).await
    }

    /// Get selected microphone
    pub async fn get_selected_microphone(&self) -> Result<Option<String>, sqlx::Error> {
        self.get("selected_microphone").await
    }

    /// Save selected monitor
    pub async fn set_selected_monitor(&self, monitor_id: u32) -> Result<(), sqlx::Error> {
        self.set("selected_monitor", &monitor_id.to_string()).await
    }

    /// Get selected monitor
    pub async fn get_selected_monitor(&self) -> Result<Option<u32>, sqlx::Error> {
        let value = self.get("selected_monitor").await?;
        Ok(value.and_then(|v| v.parse().ok()))
    }

    // ============================================
    // Capture Mode Settings
    // ============================================

    /// Set capture microphone toggle
    pub async fn set_capture_microphone(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("capture_microphone", if enabled { "true" } else { "false" })
            .await
    }

    /// Set capture system audio toggle
    pub async fn set_capture_system_audio(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set(
            "capture_system_audio",
            if enabled { "true" } else { "false" },
        )
        .await
    }

    /// Set capture screen toggle
    pub async fn set_capture_screen(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("capture_screen", if enabled { "true" } else { "false" })
            .await
    }

    /// Set always-on capture toggle
    pub async fn set_always_on_capture(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("always_on_capture", if enabled { "true" } else { "false" })
            .await
    }

    /// Set queue frames for VLM toggle
    pub async fn set_queue_frames_for_vlm(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set(
            "queue_frames_for_vlm",
            if enabled { "true" } else { "false" },
        )
        .await
    }

    /// Set frame capture interval
    pub async fn set_frame_capture_interval(&self, ms: u32) -> Result<(), sqlx::Error> {
        self.set("frame_capture_interval_ms", &ms.to_string()).await
    }

    // ============================================
    // Accessibility Capture Settings
    // ============================================

    /// Set accessibility capture enabled
    pub async fn set_accessibility_capture_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        self.set(
            "accessibility_capture_enabled",
            if enabled { "true" } else { "false" },
        )
        .await
    }

    /// Set accessibility capture interval
    pub async fn set_accessibility_capture_interval(&self, secs: u32) -> Result<(), sqlx::Error> {
        self.set("accessibility_capture_interval_secs", &secs.to_string())
            .await
    }

    // ============================================
    // Knowledge Base Settings
    // ============================================

    /// Set Supabase connection string
    pub async fn set_supabase_connection(&self, conn: &str) -> Result<(), sqlx::Error> {
        self.set("supabase_connection_string", conn).await
    }

    /// Set Pinecone API key
    pub async fn set_pinecone_api_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("pinecone_api_key", key).await
    }

    /// Set Pinecone index host
    pub async fn set_pinecone_index_host(&self, host: &str) -> Result<(), sqlx::Error> {
        self.set("pinecone_index_host", host).await
    }

    /// Set Pinecone namespace
    pub async fn set_pinecone_namespace(&self, namespace: &str) -> Result<(), sqlx::Error> {
        self.set("pinecone_namespace", namespace).await
    }

    // ============================================
    // VLM Auto-Processing Settings
    // ============================================

    /// Set VLM auto-processing enabled
    pub async fn set_vlm_auto_process(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("vlm_auto_process", if enabled { "true" } else { "false" })
            .await
    }

    /// Set VLM processing interval in seconds
    pub async fn set_vlm_process_interval(&self, secs: u32) -> Result<(), sqlx::Error> {
        let clamped = secs.clamp(30, 600); // 30s to 10min
        self.set("vlm_process_interval_secs", &clamped.to_string())
            .await
    }

    /// Set VLM base URL
    pub async fn set_vlm_base_url(&self, url: &str) -> Result<(), sqlx::Error> {
        self.set("vlm_base_url", url).await
    }

    // ============================================
    // AI Chat Settings
    // ============================================

    /// Set AI chat model
    pub async fn set_ai_chat_model(&self, model: &str) -> Result<(), sqlx::Error> {
        self.set("ai_chat_model", model).await
    }

    /// Get AI chat model
    pub async fn get_ai_chat_model(&self) -> Result<Option<String>, sqlx::Error> {
        self.get("ai_chat_model").await
    }

    // ============================================
    // Activity Theme Settings
    // ============================================

    /// Set active theme
    pub async fn set_active_theme(&self, theme: &str) -> Result<(), sqlx::Error> {
        self.set("active_theme", theme).await
    }

    /// Get active theme
    pub async fn get_active_theme(&self) -> Result<String, sqlx::Error> {
        Ok(self
            .get("active_theme")
            .await?
            .unwrap_or_else(|| "prospecting".to_string()))
    }

    /// Set screenshot interval for a specific theme
    pub async fn set_theme_interval(
        &self,
        theme: &str,
        interval_ms: u32,
    ) -> Result<(), sqlx::Error> {
        let key = format!("{}_interval_ms", theme);
        self.set(&key, &interval_ms.to_string()).await
    }

    /// Get screenshot interval for a specific theme
    pub async fn get_theme_interval(&self, theme: &str) -> Result<u32, sqlx::Error> {
        let key = format!("{}_interval_ms", theme);
        let default = match theme {
            "prospecting" => 1500,
            "fundraising" => 1500,
            "product_dev" => 2000,
            "admin" => 2000,
            "personal" => 3000,
            _ => 2000,
        };
        Ok(self
            .get(&key)
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(default))
    }

    // ============================================
    // AI Provider Settings
    // ============================================

    pub async fn set_ai_provider(&self, provider: &str) -> Result<(), sqlx::Error> {
        self.set("ai_provider", provider).await
    }

    pub async fn set_ai_remote_url(&self, url: &str) -> Result<(), sqlx::Error> {
        self.set("ai_remote_url", url).await
    }

    pub async fn set_ai_remote_key(&self, key: &str) -> Result<(), sqlx::Error> {
        self.set("ai_remote_key", key).await
    }
}
