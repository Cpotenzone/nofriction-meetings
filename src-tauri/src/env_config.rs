// noFriction Meetings - Environment Configuration Loader
// Loads default settings from .env file if present

use std::env;

pub struct EnvConfig {
    pub supabase_connection_string: Option<String>,
    pub pinecone_api_key: Option<String>,
    pub pinecone_index_host: Option<String>,
    pub pinecone_namespace: Option<String>,
    pub vlm_base_url: Option<String>,
    pub thebrain_email: Option<String>,
    pub thebrain_password: Option<String>,
    pub remote_intelligence_enabled: bool,
    pub remote_intelligence_url: Option<String>,
    pub remote_intelligence_token: Option<String>,
    // Transcription API Keys
    pub deepgram_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            supabase_connection_string: None,
            pinecone_api_key: None,
            pinecone_index_host: None,
            pinecone_namespace: Some("default".to_string()),
            vlm_base_url: Some("https://7wk68vrq9achr2djw.caas.targon.com".to_string()),
            thebrain_email: None,
            thebrain_password: None,
            remote_intelligence_enabled: false,
            remote_intelligence_url: None,
            remote_intelligence_token: None,
            deepgram_api_key: None,
            gemini_api_key: None,
        }
    }
}

impl EnvConfig {
    /// Load environment configuration from .env file
    pub fn load() -> Self {
        // Try to load .env from project root
        let _ = dotenvy::dotenv();

        // Also try from user's home directory
        if let Some(home) = dirs::home_dir() {
            let home_env = home.join(".nofriction-meetings").join(".env");
            if home_env.exists() {
                let _ = dotenvy::from_path(home_env);
            }
        }

        Self {
            supabase_connection_string: env::var("SUPABASE_CONNECTION_STRING").ok(),
            pinecone_api_key: env::var("PINECONE_API_KEY").ok(),
            pinecone_index_host: env::var("PINECONE_INDEX_HOST").ok(),
            pinecone_namespace: env::var("PINECONE_NAMESPACE")
                .ok()
                .or(Some("default".to_string())),
            vlm_base_url: env::var("VLM_BASE_URL").ok().or(Some(
                "https://7wk68vrq9achr2djw.caas.targon.com".to_string(),
            )),
            thebrain_email: env::var("THEBRAIN_EMAIL").ok(),
            thebrain_password: env::var("THEBRAIN_PASSWORD").ok(),
            remote_intelligence_enabled: env::var("REMOTE_INTELLIGENCE_ENABLED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(false),
            remote_intelligence_url: env::var("REMOTE_INTELLIGENCE_URL").ok(),
            remote_intelligence_token: env::var("REMOTE_INTELLIGENCE_TOKEN").ok(),
            deepgram_api_key: env::var("DEEPGRAM_API_KEY").ok().filter(|s| !s.is_empty()),
            gemini_api_key: env::var("GEMINI_API_KEY").ok().filter(|s| !s.is_empty()),
        }
    }

    /// Save current environment to ~/.nofriction-meetings/.env
    pub fn save_to_home(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::io::Write;

        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let config_dir = home.join(".nofriction-meetings");
        fs::create_dir_all(&config_dir)?;

        let env_path = config_dir.join(".env");
        let mut file = fs::File::create(env_path)?;

        writeln!(file, "# noFriction Meetings - User Configuration")?;
        writeln!(file, "# Auto-generated - Edit these values as needed\n")?;

        if let Some(ref val) = self.supabase_connection_string {
            writeln!(file, "SUPABASE_CONNECTION_STRING={}", val)?;
        }
        if let Some(ref val) = self.pinecone_api_key {
            writeln!(file, "PINECONE_API_KEY={}", val)?;
        }
        if let Some(ref val) = self.pinecone_index_host {
            writeln!(file, "PINECONE_INDEX_HOST={}", val)?;
        }
        if let Some(ref val) = self.pinecone_namespace {
            writeln!(file, "PINECONE_NAMESPACE={}", val)?;
        }
        if let Some(ref val) = self.vlm_base_url {
            writeln!(file, "VLM_BASE_URL={}", val)?;
        }
        if let Some(ref val) = self.thebrain_email {
            writeln!(file, "THEBRAIN_EMAIL={}", val)?;
        }
        if let Some(ref val) = self.thebrain_password {
            writeln!(file, "THEBRAIN_PASSWORD={}", val)?;
        }
        writeln!(
            file,
            "REMOTE_INTELLIGENCE_ENABLED={}",
            self.remote_intelligence_enabled
        )?;
        if let Some(ref val) = self.remote_intelligence_url {
            writeln!(file, "REMOTE_INTELLIGENCE_URL={}", val)?;
        }
        if let Some(ref val) = self.remote_intelligence_token {
            writeln!(file, "REMOTE_INTELLIGENCE_TOKEN={}", val)?;
        }

        Ok(())
    }
}
