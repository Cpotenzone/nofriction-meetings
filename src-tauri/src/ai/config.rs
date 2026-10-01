//! Non-secret AI configuration: active text/vision provider + model, per
//! provider base URL / model / consent / quirks. Stored as one JSON blob in
//! the `settings` table under `ai_config`. Keys live in the Keychain
//! (`secrets::AI_SERVICE`, account = provider id), never here.

use super::providers::{self, preset, KeyNeed};
use crate::secrets;
use crate::settings::SettingsManager;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

pub const SETTINGS_KEY: &str = "ai_config";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Vision,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Selection {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderState {
    /// Override for editable presets (custom / ollama / lmstudio)
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub vision_model: Option<String>,
    /// App Review 5.1.2(i): user agreed to send meeting content here
    #[serde(default)]
    pub consent: bool,
    /// Models that rejected `max_tokens`/`temperature` (use
    /// `max_completion_tokens`, no temperature)
    #[serde(default)]
    pub completion_tokens_models: Vec<String>,
    /// Last model list we fetched (for pickers; refreshed on demand)
    #[serde(default)]
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub text: Option<Selection>,
    #[serde(default)]
    pub vision: Option<Selection>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderState>,
}

impl AiConfig {
    pub fn provider(&self, id: &str) -> ProviderState {
        self.providers.get(id).cloned().unwrap_or_default()
    }

    pub fn provider_mut(&mut self, id: &str) -> &mut ProviderState {
        self.providers.entry(id.to_string()).or_default()
    }

    /// Effective base URL for a provider (preset default or user override).
    pub fn base_url(&self, id: &str) -> Option<String> {
        let p = preset(id)?;
        let over = self.providers.get(id).and_then(|s| s.base_url.clone()).filter(|u| !u.is_empty());
        match (p.editable_url, over) {
            (true, Some(u)) => Some(u),
            _ if !p.base_url.is_empty() => Some(p.base_url.to_string()),
            _ => None,
        }
    }

    /// Local = on this machine / LAN / tailnet: no consent needed.
    ///
    /// Decided by the *configured URL*, not the preset label: Ollama and
    /// LM Studio let the user point the URL at any https host, and a remote
    /// host is a cloud endpoint that needs consent like any other (mirrors
    /// iOS `URLPolicy.needsConsent`). Apple on-device never leaves the Mac.
    pub fn is_local(&self, id: &str) -> bool {
        match preset(id) {
            Some(p) if p.protocol == providers::Protocol::Apple => true,
            Some(p) if p.local => self.base_url(id).map(|u| providers::url_is_local(&u)).unwrap_or(true),
            Some(p) if p.id == "custom" => self.base_url(id).map(|u| providers::url_is_local(&u)).unwrap_or(false),
            _ => false,
        }
    }

    pub fn model_info(&self, provider: &str, model: &str) -> Option<ModelInfo> {
        self.providers
            .get(provider)
            .and_then(|s| s.models.iter().find(|m| m.id == model).cloned())
    }

    /// Model accepts images? Uses reported capability, then heuristics.
    pub fn vision_capable(&self, provider: &str, model: &str) -> Option<bool> {
        self.model_info(provider, model)
            .and_then(|m| m.vision)
            .or_else(|| providers::supports_vision(provider, model))
    }

    /// The selection for `kind`, applying the vision→text fallback: when no
    /// vision provider is chosen, use the text model if it might take images.
    pub fn selection(&self, kind: Kind) -> Option<Selection> {
        match kind {
            // Key-less fallback: Apple's on-device model when nothing is set
            // up and Apple Intelligence is available (macOS 26+).
            Kind::Text => self.text.clone().or_else(apple_fallback),
            Kind::Vision => self.vision.clone().or_else(|| {
                let t = self.text.clone()?;
                match self.vision_capable(&t.provider, &t.model) {
                    Some(false) => None,
                    _ => Some(t),
                }
            }),
        }
    }
}

fn apple_fallback() -> Option<Selection> {
    crate::store::apple_model_available().then(|| Selection {
        provider: providers::APPLE_PROVIDER.to_string(),
        model: providers::APPLE_MODEL.to_string(),
    })
}

static CONFIG: Lazy<RwLock<AiConfig>> = Lazy::new(|| RwLock::new(AiConfig::default()));
static SETTINGS: OnceLock<Arc<SettingsManager>> = OnceLock::new();

pub fn snapshot() -> AiConfig {
    CONFIG.read().clone()
}

/// Mutate and persist.
pub async fn update<F: FnOnce(&mut AiConfig)>(f: F) -> Result<AiConfig, String> {
    let cfg = {
        let mut c = CONFIG.write();
        f(&mut c);
        c.clone()
    };
    persist(&cfg).await?;
    Ok(cfg)
}

/// Mutate in memory and persist in the background (for hot paths).
pub fn update_background<F: FnOnce(&mut AiConfig)>(f: F) {
    let cfg = {
        let mut c = CONFIG.write();
        f(&mut c);
        c.clone()
    };
    tauri::async_runtime::spawn(async move {
        if let Err(e) = persist(&cfg).await {
            log::warn!("AI config not saved: {}", e);
        }
    });
}

async fn persist(cfg: &AiConfig) -> Result<(), String> {
    let Some(settings) = SETTINGS.get() else { return Ok(()) };
    let json = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    settings.set(SETTINGS_KEY, &json).await.map_err(|e| e.to_string())
}

pub fn api_key(provider: &str) -> Option<String> {
    secrets::get(secrets::AI_SERVICE, provider)
}

/// Provider is usable: has a key when it needs one, and a base URL.
pub fn is_ready(cfg: &AiConfig, provider: &str) -> bool {
    let Some(p) = preset(provider) else { return false };
    if p.protocol == providers::Protocol::Apple {
        return crate::store::apple_model_available();
    }
    if cfg.base_url(provider).is_none() {
        return false;
    }
    match p.key {
        KeyNeed::Required => api_key(provider).is_some(),
        _ => true,
    }
}

/// Load config at startup. Returns true when this is the first launch with
/// the new provider layer (no saved config).
pub async fn init(settings: Arc<SettingsManager>) -> bool {
    let _ = SETTINGS.set(settings.clone());
    let saved = settings.get(SETTINGS_KEY).await.ok().flatten();
    match saved.and_then(|s| serde_json::from_str::<AiConfig>(&s).ok()) {
        Some(cfg) => {
            *CONFIG.write() = cfg;
            false
        }
        None => {
            let mut cfg = AiConfig::default();
            // Carry over a custom Ollama host from the old VLM URL setting
            // (it pointed at Ollama's native root; the OpenAI API is /v1).
            if let Ok(Some(url)) = settings.get("vlm_base_url").await {
                let url = url.trim().trim_end_matches('/');
                if !url.is_empty() && !url.contains("targon.com") && !url.contains("localhost:11434") {
                    let candidate = if url.ends_with("/v1") { url.to_string() } else { format!("{}/v1", url) };
                    if let Ok(ok) = providers::check_base_url(&candidate) {
                        // Carrying the URL over is fine (the user typed it),
                        // but a non-private host is a cloud endpoint: it keeps
                        // consent = false, so the first send asks, and
                        // autodetect_local() won't auto-select it.
                        log::info!("AI: carried over Ollama endpoint from old VLM URL setting");
                        cfg.provider_mut("ollama").base_url = Some(ok);
                    }
                }
            }
            *CONFIG.write() = cfg.clone();
            let _ = persist(&cfg).await;
            true
        }
    }
}

/// The Ollama URL to probe for auto-selection, if any. Never one off the
/// user's own machine/network: auto-selecting it would route meeting
/// content to a remote host without the consent prompt (e.g. an old
/// `vlm_base_url` pointing at a cloud host, carried over by `init`).
fn autodetect_candidate(cfg: &AiConfig) -> Option<String> {
    if cfg.text.is_some() {
        return None;
    }
    let base = cfg.base_url("ollama")?;
    if !cfg.is_local("ollama") {
        log::info!("AI: carried-over Ollama URL is not local; not auto-selecting it (needs consent)");
        return None;
    }
    Some(base)
}

/// First launch after the upgrade: if a local Ollama is running, select it so
/// existing installs keep working without a key. Never picks a cloud
/// provider automatically.
pub async fn autodetect_local() {
    let Some(base) = autodetect_candidate(&snapshot()) else { return };
    match super::client::list_models("ollama", &base, None).await {
        Ok(models) if !models.is_empty() => {
            let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
            let p = preset("ollama").unwrap();
            let Some(text) = providers::pick_default_model(p, &ids) else { return };
            let vision = providers::pick_vision_model(p, &ids, Some(&text));
            let _ = update(|c| {
                let st = c.provider_mut("ollama");
                st.models = models.clone();
                st.model = Some(text.clone());
                st.vision_model = vision.clone();
                if c.text.is_none() {
                    c.text = Some(Selection { provider: "ollama".into(), model: text.clone() });
                }
                if c.vision.is_none() {
                    if let Some(v) = vision.clone() {
                        c.vision = Some(Selection { provider: "ollama".into(), model: v });
                    }
                }
            })
            .await;
            log::info!("AI: local Ollama found, using it for text ({})", text);
        }
        _ => log::info!("AI: no provider configured yet (add a key in Settings → AI Engine)"),
    }
}

#[cfg(test)]
pub(crate) fn set_for_tests(cfg: AiConfig) {
    *CONFIG.write() = cfg;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_falls_back_to_text_only_when_capable() {
        let mut c = AiConfig::default();
        c.text = Some(Selection { provider: "ollama".into(), model: "qwen3:8b".into() });
        assert!(c.selection(Kind::Vision).is_none());
        c.text = Some(Selection { provider: "openai".into(), model: "gpt-4o-mini".into() });
        assert_eq!(c.selection(Kind::Vision).unwrap().model, "gpt-4o-mini");
        c.vision = Some(Selection { provider: "ollama".into(), model: "qwen3-vl:8b".into() });
        assert_eq!(c.selection(Kind::Vision).unwrap().provider, "ollama");
    }

    #[test]
    fn custom_locality_follows_url() {
        let mut c = AiConfig::default();
        assert!(!c.is_local("custom"));
        c.provider_mut("custom").base_url = Some("http://192.168.1.5:8000/v1".into());
        assert!(c.is_local("custom"));
        c.provider_mut("custom").base_url = Some("https://llm.example.com/v1".into());
        assert!(!c.is_local("custom"));
        assert!(c.is_local("ollama"));
        assert!(c.is_local("lmstudio"));
        assert!(c.is_local("apple"));
        assert!(!c.is_local("openai"));
        // Cloud presets ignore overrides
        c.provider_mut("openai").base_url = Some("https://evil.example.com".into());
        assert_eq!(c.base_url("openai").unwrap(), "https://api.openai.com/v1");
    }

    #[test]
    fn local_presets_with_remote_url_need_consent() {
        let mut c = AiConfig::default();
        // Preset default (localhost) and LAN/tailnet overrides stay local
        assert!(c.is_local("ollama"));
        c.provider_mut("ollama").base_url = Some("http://10.0.0.7:11434/v1".into());
        assert!(c.is_local("ollama"));
        c.provider_mut("ollama").base_url = Some("http://box.local:11434/v1".into());
        assert!(c.is_local("ollama"));
        // A public host behind the "local" preset is cloud: consent required
        c.provider_mut("ollama").base_url = Some("https://ollama.example.com/v1".into());
        assert!(!c.is_local("ollama"));
        c.provider_mut("lmstudio").base_url = Some("https://lm.example.com/v1".into());
        assert!(!c.is_local("lmstudio"));
        // Back to the preset default -> local again
        c.provider_mut("ollama").base_url = None;
        assert!(c.is_local("ollama"));
    }

    #[test]
    fn autodetect_never_selects_a_remote_carried_over_url() {
        let mut c = AiConfig::default();
        assert_eq!(autodetect_candidate(&c).as_deref(), Some("http://localhost:11434/v1"));
        c.provider_mut("ollama").base_url = Some("http://192.168.1.20:11434/v1".into());
        assert!(autodetect_candidate(&c).is_some());
        // An old vlm_base_url on a public host, migrated by init()
        c.provider_mut("ollama").base_url = Some("https://gpu.example.com/v1".into());
        assert!(autodetect_candidate(&c).is_none());
        assert!(!c.provider("ollama").consent);
    }
}
