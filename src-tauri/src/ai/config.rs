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
use sha2::{Digest, Sha256};
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
    /// User-entered endpoint for the custom connection
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

    /// Effective address: Apple on-device marker or explicit custom URL.
    pub fn base_url(&self, id: &str) -> Option<String> {
        let p = preset(id)?;
        let over = self.providers.get(id).and_then(|s| s.base_url.clone()).filter(|u| !u.is_empty());
        match (p.editable_url, over) {
            (true, Some(u)) => Some(u),
            _ if !p.base_url.is_empty() => Some(p.base_url.to_string()),
            _ => None,
        }
    }

    /// Locality is decided by the user-entered URL, never a service name.
    pub fn is_local(&self, id: &str) -> bool {
        match preset(id) {
            Some(p) if p.protocol == providers::Protocol::Apple => true,
            Some(p) if p.local => self.base_url(id).map(|u| providers::url_is_local(&u)).unwrap_or(true),
            Some(p) if p.id == "custom" => self.base_url(id).map(|u| providers::url_is_local(&u)).unwrap_or(false),
            _ => false,
        }
    }

    /// No credential is read from the old shared custom slot. The normalized
    /// destination determines its Keychain account, including for stale snapshots.
    pub fn credential_account(&self, id: &str) -> Option<String> {
        if id != "custom" { return None; }
        let base = providers::check_base_url(&self.base_url(id)?).ok()?;
        Some(format!("custom:{}", format!("{:x}", Sha256::digest(base.as_bytes()))))
    }

    pub fn check_endpoint(&self, id: &str, expected: &str) -> Result<(), String> {
        let current = self.base_url(id).ok_or("Set your endpoint first")?;
        if providers::check_base_url(&current)? != providers::check_base_url(expected)? {
            return Err("The AI endpoint changed. Review the current destination and try again.".into());
        }
        Ok(())
    }

    /// Called under the config read lock, so an endpoint transition cannot race
    /// this expected-destination check and the Keychain write.
    pub fn save_key_with<F>(&self, id: &str, expected: &str, write: F) -> Result<(), String>
    where F: FnOnce(&str) -> Result<(), String> {
        self.check_endpoint(id, expected)?;
        let account = self.credential_account(id).ok_or("Configure a custom endpoint first")?;
        write(&account)
    }

    /// Bind approval to the destination actually shown in the dialog.
    pub fn grant_endpoint_consent(&mut self, id: &str, expected: &str) -> Result<(), String> {
        if id != "custom" { return Err("Only an explicitly configured endpoint can receive this approval".into()); }
        let expected = providers::check_base_url(expected)?;
        let current = self.base_url(id).ok_or("Set your endpoint before granting consent")?;
        if current != expected { return Err("The AI endpoint changed. Close this dialog and review the new destination.".into()); }
        self.provider_mut(id).consent = true;
        Ok(())
    }

    /// A new destination must not inherit credentials, consent or model selection.
    /// `forget_key` is injected so this transition is testable without Keychain writes.
    pub fn change_endpoint<F>(&mut self, id: &str, url: Option<String>, forget_key: F) -> Result<(), String>
    where F: FnOnce() -> Result<(), String> {
        if self.provider(id).base_url == url { return Ok(()); }
        forget_key()?;
        let st = self.provider_mut(id);
        st.base_url = url;
        st.consent = false;
        st.models.clear();
        st.model = None;
        st.vision_model = None;
        if self.text.as_ref().map_or(false, |s| s.provider == id) { self.text = None; }
        if self.vision.as_ref().map_or(false, |s| s.provider == id) { self.vision = None; }
        Ok(())
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
        let configured = |selection: &Selection| {
            preset(&selection.provider).is_some()
                && self.base_url(&selection.provider).map_or(false, |u| selection.provider == "apple" || providers::check_base_url(&u).is_ok())
                && !selection.model.trim().is_empty()
        };
        match kind {
            // Preserve saved records, but never fall through a removed selection
            // to another service. Only a fresh/unselected config may use Apple.
            Kind::Text => match &self.text {
                Some(s) => configured(s).then(|| s.clone()),
                None => apple_fallback(),
            },
            Kind::Vision => match &self.vision {
                Some(s) => configured(s).then(|| s.clone()),
                None => {
                    let t = self.text.as_ref().filter(|s| configured(s))?;
                    (self.vision_capable(&t.provider, &t.model) != Some(false)).then(|| t.clone())
                }
            },
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

pub fn with_snapshot<T>(f: impl FnOnce(&AiConfig) -> T) -> T {
    f(&CONFIG.read())
}

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

pub fn api_key(cfg: &AiConfig, provider: &str) -> Option<String> {
    let account = cfg.credential_account(provider)?;
    secrets::get(secrets::AI_SERVICE, &account)
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
        KeyNeed::Required => api_key(cfg, provider).is_some(),
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
            let cfg = AiConfig::default();
            *CONFIG.write() = cfg.clone();
            let _ = persist(&cfg).await;
            true
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn set_for_tests(cfg: AiConfig) {
    *CONFIG.write() = cfg;
}

#[cfg(test)]
mod tests {
    #[test]
    fn credentials_are_endpoint_bound_and_stale_saves_are_rejected() {
        let mut c = super::AiConfig::default();
        c.provider_mut("custom").base_url = Some("https://first.example.com/v1".into());
        let old_snapshot = c.clone();
        let old_account = c.credential_account("custom").unwrap();
        assert_ne!(old_account, "custom");
        c.change_endpoint("custom", Some("https://second.example.com/v1".into()), || Ok(())).unwrap();
        let new_account = c.credential_account("custom").unwrap();
        assert_ne!(old_account, new_account);
        // Resolving from an old snapshot can only select the old account, never
        // the credential subsequently written for a different endpoint.
        assert_eq!(old_snapshot.credential_account("custom").unwrap(), old_account);
        assert!(c.save_key_with("custom", "https://first.example.com/v1", |_| panic!("must not write a stale dialog key")).is_err());
        c.save_key_with("custom", "https://second.example.com/v1/", |account| { assert_eq!(account, new_account); Ok(()) }).unwrap();
        assert!(c.check_endpoint("custom", "https://first.example.com/v1").is_err());
        assert!(c.credential_account("gemini").is_none());
        assert!(super::AiConfig::default().credential_account("custom").is_none());
    }

    #[test]
    fn approval_is_bound_to_the_endpoint_shown() {
        let mut c = super::AiConfig::default();
        assert!(c.grant_endpoint_consent("custom", "https://first.example.com/v1").is_err());
        c.provider_mut("custom").base_url = Some("https://first.example.com/v1".into());
        assert!(c.grant_endpoint_consent("custom", "https://first.example.com/v1").is_ok());
        c.change_endpoint("custom", Some("https://second.example.com/v1".into()), || Ok(())).unwrap();
        assert!(c.grant_endpoint_consent("custom", "https://first.example.com/v1").is_err());
        assert!(!c.provider("custom").consent);
        assert!(c.grant_endpoint_consent("gemini", "https://second.example.com/v1").is_err());
        assert!(c.grant_endpoint_consent("custom", "https://second.example.com/v1").is_ok());
    }

    use super::*;

    #[test]
    fn fresh_config_has_no_network_endpoint() {
        let c = AiConfig::default();
        assert!(c.base_url("custom").is_none());
        assert!(c.selection(Kind::Vision).is_none());
        assert!(c.selection(Kind::Text).map_or(true, |s| s.provider == "apple"));
        assert!(c.providers.is_empty());
    }

    #[test]
    fn legacy_named_selections_fail_closed_without_deleting_records() {
        for id in ["gemini", "openai", "anthropic", "deepseek", "ollama", "lmstudio"] {
            let mut c = AiConfig::default();
            c.text = Some(Selection { provider: id.into(), model: "saved-model".into() });
            c.vision = c.text.clone();
            c.provider_mut(id).base_url = Some("https://legacy.example.com/v1".into());
            c.provider_mut(id).consent = true;
            assert!(c.base_url(id).is_none());
            assert!(c.selection(Kind::Text).is_none());
            assert!(c.selection(Kind::Vision).is_none());
            assert_eq!(c.provider(id).base_url.as_deref(), Some("https://legacy.example.com/v1"));
        }
    }

    #[test]
    fn custom_selection_requires_explicit_endpoint_and_model() {
        let mut c = AiConfig::default();
        c.text = Some(Selection { provider: "custom".into(), model: "user-model".into() });
        assert!(c.selection(Kind::Text).is_none());
        c.provider_mut("custom").base_url = Some("http://127.0.0.1:8000/v1".into());
        assert!(c.is_local("custom"));
        assert!(c.selection(Kind::Text).is_some());
        c.provider_mut("custom").base_url = Some("https://user.example.com/v1".into());
        assert!(!c.is_local("custom"));
        assert!(!c.provider("custom").consent);
        c.text.as_mut().unwrap().model.clear();
        assert!(c.selection(Kind::Text).is_none());
    }
    #[test]
    fn endpoint_change_forgets_the_old_key_and_consent_before_activation() {
        let mut c = AiConfig::default();
        c.provider_mut("custom").base_url = Some("https://first.example.com/v1".into());
        c.provider_mut("custom").consent = true;
        c.provider_mut("custom").model = Some("old-model".into());
        c.text = Some(Selection { provider: "custom".into(), model: "old-model".into() });
        let mut key_present = true;
        c.change_endpoint("custom", Some("https://second.example.com/v1".into()), || { key_present = false; Ok(()) }).unwrap();
        assert!(!key_present);
        assert!(!c.provider("custom").consent);
        assert!(c.provider("custom").model.is_none());
        assert!(c.text.is_none());
        c.change_endpoint("custom", Some("https://second.example.com/v1".into()), || panic!("same URL must retain its key")).unwrap();
        assert!(c.change_endpoint("custom", Some("https://third.example.com/v1".into()), || Err("Keychain deletion failed".into())).is_err());
        assert_eq!(c.base_url("custom").as_deref(), Some("https://second.example.com/v1"));
    }

    #[test]
    fn invalid_saved_custom_url_cannot_become_active() {
        let mut c = AiConfig::default();
        c.provider_mut("custom").base_url = Some("http://public.example.com/v1".into());
        c.provider_mut("custom").consent = true;
        c.text = Some(Selection { provider: "custom".into(), model: "saved".into() });
        assert!(c.selection(Kind::Text).is_none());
    }

}
