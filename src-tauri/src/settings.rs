// noFriction Meetings - Settings Manager
// Persistent settings storage using SQLite

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub deepgram_api_key: Option<String>,
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
    // Knowledge base settings
    pub supabase_connection_string: Option<String>,
    pub pinecone_api_key: Option<String>,
    pub pinecone_index_host: Option<String>,
}

impl AppSettings {
    /// Create with sensible defaults
    pub fn with_defaults() -> Self {
        Self {
            deepgram_api_key: None,
            selected_microphone: None,
            selected_monitor: None,
            auto_start_recording: false,
            show_notifications: true,
            capture_microphone: true,          // Mic on by default
            capture_system_audio: true,        // System audio ON for meeting capture
            capture_screen: false,             // Screen capture OFF by default (reduces CPU)
            always_on_capture: false,          // Not always-on by default
            queue_frames_for_vlm: false,       // VLM OFF by default (saves resources)
            frame_capture_interval_ms: 5000,   // 5 sec instead of 1 (5x less disk I/O)
            supabase_connection_string: None,
            pinecone_api_key: None,
            pinecone_index_host: None,
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
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM settings WHERE key = ?"
        )
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
        self.set("capture_microphone", if enabled { "true" } else { "false" }).await
    }

    /// Set capture system audio toggle
    pub async fn set_capture_system_audio(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("capture_system_audio", if enabled { "true" } else { "false" }).await
    }

    /// Set capture screen toggle
    pub async fn set_capture_screen(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("capture_screen", if enabled { "true" } else { "false" }).await
    }

    /// Set always-on capture toggle
    pub async fn set_always_on_capture(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("always_on_capture", if enabled { "true" } else { "false" }).await
    }

    /// Set queue frames for VLM toggle
    pub async fn set_queue_frames_for_vlm(&self, enabled: bool) -> Result<(), sqlx::Error> {
        self.set("queue_frames_for_vlm", if enabled { "true" } else { "false" }).await
    }

    /// Set frame capture interval
    pub async fn set_frame_capture_interval(&self, ms: u32) -> Result<(), sqlx::Error> {
        self.set("frame_capture_interval_ms", &ms.to_string()).await
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
}
