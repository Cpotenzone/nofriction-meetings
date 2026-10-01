//! Tauri commands for the AI settings panel. Keys go in, never come out:
//! the UI only ever sees `{configured, last4}`.

use super::client::{self, AiError};
use super::config::{self, Kind, ModelInfo, Selection};
use super::providers::{self, preset, KeyNeed, Protocol, PRESETS};
use crate::secrets;
use serde::Serialize;

#[derive(Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: Option<String>,
    pub key_url: String,
    pub key: KeyNeed,
    pub local: bool,
    pub editable_url: bool,
    pub configured: bool,
    pub last4: Option<String>,
    pub consent: bool,
    pub needs_consent: bool,
    pub model: Option<String>,
    pub vision_model: Option<String>,
    pub models: Vec<ModelInfo>,
    pub active_text: bool,
    pub active_vision: bool,
}

fn provider_info(cfg: &config::AiConfig, id: &str) -> Option<ProviderInfo> {
    let p = preset(id)?;
    let st = cfg.provider(id);
    let key_status = secrets::status(secrets::AI_SERVICE, id);
    let local = cfg.is_local(id);
    let apple = p.protocol == Protocol::Apple;
    let configured = match p.key {
        _ if apple => crate::store::apple_model_available(),
        KeyNeed::Required => key_status.configured,
        _ => st.model.is_some() || !st.models.is_empty(),
    };
    let apple_models = || {
        vec![ModelInfo {
            id: providers::APPLE_MODEL.to_string(),
            context: Some(providers::APPLE_CONTEXT_TOKENS),
            vision: Some(false),
        }]
    };
    let vision_sel = cfg.selection(Kind::Vision);
    Some(ProviderInfo {
        id: id.to_string(),
        name: p.name.to_string(),
        protocol: p.protocol,
        base_url: cfg.base_url(id),
        key_url: p.key_url.to_string(),
        key: p.key,
        local,
        editable_url: p.editable_url,
        configured,
        last4: key_status.last4,
        consent: st.consent,
        needs_consent: !local && !st.consent,
        model: if apple && configured { Some(providers::APPLE_MODEL.to_string()) } else { st.model.clone() },
        vision_model: st.vision_model.clone(),
        models: if apple && configured { apple_models() } else { st.models.clone() },
        active_text: cfg.selection(Kind::Text).as_ref().map_or(false, |s| s.provider == id),
        active_vision: vision_sel.as_ref().map_or(false, |s| s.provider == id),
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_list_providers() -> Result<Vec<ProviderInfo>, String> {
    let cfg = config::snapshot();
    Ok(PRESETS.iter().filter_map(|p| provider_info(&cfg, p.id)).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_detect_provider(key: String) -> Result<providers::Detection, String> {
    Ok(providers::detect_provider(&providers::normalize_key(&key)))
}

#[derive(Serialize)]
pub struct SaveKeyResult {
    pub provider: String,
    pub name: String,
    pub models: Vec<String>,
    pub model: Option<String>,
    pub vision_model: Option<String>,
    pub needs_consent: bool,
    pub last4: Option<String>,
}

/// Detect → validate (list models) → store in Keychain → pick default model
/// → make active. Errors are classed strings (AI_WRONG_KEY:, …).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_save_key(key: String, provider: Option<String>) -> Result<SaveKeyResult, String> {
    let key = providers::normalize_key(&key);
    providers::validate_key_shape(&key)?;
    let detection = providers::detect_provider(&key);
    let explicit = provider.filter(|p| !p.is_empty());
    let mut provider_id = explicit
        .clone()
        .or(detection.provider.clone())
        .ok_or("UNKNOWN_PROVIDER: We couldn't tell which service this key is for. Pick the provider from the list.")?;
    let p = preset(&provider_id).ok_or("Unknown provider")?;
    if p.key == KeyNeed::None {
        return Err(format!("{} runs locally and doesn't use an API key.", p.name));
    }
    let cfg = config::snapshot();
    let base = cfg
        .base_url(&provider_id)
        .ok_or("AI_BAD_URL: Set the endpoint URL for the custom provider first.")?;

    let mut result = client::list_models(&provider_id, &base, Some(&key)).await;
    // A bare sk- key may be DeepSeek: try once before giving up
    if matches!(result, Err(AiError::WrongKey(_)))
        && explicit.is_none()
        && detection.alternatives.iter().any(|a| a == "deepseek")
    {
        let ds_base = cfg.base_url("deepseek").unwrap_or_default();
        if let Ok(models) = client::list_models("deepseek", &ds_base, Some(&key)).await {
            provider_id = "deepseek".into();
            result = Ok(models);
        }
    }
    let models = result.map_err(String::from)?;
    let p = preset(&provider_id).unwrap();

    secrets::set(secrets::AI_SERVICE, &provider_id, &key)?;

    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let st = cfg.provider(&provider_id);
    let model = st
        .model
        .clone()
        .filter(|m| ids.contains(m))
        .or_else(|| providers::pick_default_model(p, &ids));
    let vision_model = st
        .vision_model
        .clone()
        .filter(|m| ids.contains(m))
        .or_else(|| {
            let cap = |m: &str| {
                models
                    .iter()
                    .find(|x| x.id == m)
                    .and_then(|x| x.vision)
                    .or_else(|| providers::supports_vision(&provider_id, m))
            };
            match model.as_deref() {
                Some(t) if cap(t) == Some(true) => Some(t.to_string()),
                _ => ids.iter().find(|m| providers::is_chat_model(m) && cap(m) == Some(true)).cloned(),
            }
        });

    let pid = provider_id.clone();
    let cfg = config::update(|c| {
        let st = c.provider_mut(&pid);
        st.models = models.clone();
        st.model = model.clone();
        st.vision_model = vision_model.clone();
        // Connecting a provider makes it the active one
        if let Some(m) = model.clone() {
            c.text = Some(Selection { provider: pid.clone(), model: m });
        }
        match vision_model.clone() {
            Some(v) => c.vision = Some(Selection { provider: pid.clone(), model: v }),
            None => {
                if c.vision.as_ref().map_or(false, |s| s.provider == pid) {
                    c.vision = None;
                }
            }
        }
    })
    .await?;

    log::info!("AI: connected {} ({} models, default {:?})", p.name, ids.len(), model);
    Ok(SaveKeyResult {
        provider: provider_id.clone(),
        name: p.name.to_string(),
        models: ids,
        model,
        vision_model,
        needs_consent: !cfg.is_local(&provider_id) && !cfg.provider(&provider_id).consent,
        last4: secrets::status(secrets::AI_SERVICE, &provider_id).last4,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_delete_key(provider: String) -> Result<AiStatus, String> {
    preset(&provider).ok_or("Unknown provider")?;
    secrets::delete(secrets::AI_SERVICE, &provider)?;
    config::update(|c| {
        if c.text.as_ref().map_or(false, |s| s.provider == provider) {
            c.text = None;
        }
        if c.vision.as_ref().map_or(false, |s| s.provider == provider) {
            c.vision = None;
        }
        let st = c.provider_mut(&provider);
        st.consent = false;
        st.models.clear();
        st.model = None;
        st.vision_model = None;
    })
    .await?;
    log::info!("AI: removed key for {}", provider);
    ai_status().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_set_active(provider: String, model: Option<String>, kind: Kind) -> Result<AiStatus, String> {
    let p = preset(&provider).ok_or("Unknown provider")?;
    let cfg = config::snapshot();
    if !config::is_ready(&cfg, &provider) {
        return Err(format!("Connect {} first (add its key or URL).", p.name));
    }
    let st = cfg.provider(&provider);
    let model = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .or(match kind {
            Kind::Text => st.model.clone(),
            Kind::Vision => st.vision_model.clone().or(st.model.clone()),
        })
        .ok_or("Pick a model")?;
    if model.len() > 200 || model.chars().any(|c| c.is_control()) {
        return Err("That isn't a valid model name".into());
    }
    if kind == Kind::Vision && cfg.vision_capable(&provider, &model) == Some(false) {
        return Err(format!("{} can't read images; pick a vision model.", model));
    }
    config::update(|c| {
        let sel = Selection { provider: provider.clone(), model: model.clone() };
        let st = c.provider_mut(&provider);
        match kind {
            Kind::Text => {
                st.model = Some(model.clone());
                c.text = Some(sel);
            }
            Kind::Vision => {
                st.vision_model = Some(model.clone());
                c.vision = Some(sel);
            }
        }
    })
    .await?;
    ai_status().await
}

/// Turn vision off (fall back to the text model if it takes images).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_clear_vision() -> Result<AiStatus, String> {
    config::update(|c| c.vision = None).await?;
    ai_status().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_set_custom_endpoint(provider: String, base_url: String) -> Result<ProviderInfo, String> {
    let p = preset(&provider).ok_or("Unknown provider")?;
    if !p.editable_url {
        return Err(format!("{}'s endpoint is fixed.", p.name));
    }
    let url = if base_url.trim().is_empty() {
        None // back to the preset default
    } else {
        Some(providers::check_base_url(&base_url)?)
    };
    let changed = config::snapshot().provider(&provider).base_url != url;
    config::update(|c| {
        let st = c.provider_mut(&provider);
        st.base_url = url.clone();
        if changed {
            // A different server: re-ask consent and refresh models
            st.consent = false;
            st.models.clear();
        }
    })
    .await?;
    provider_info(&config::snapshot(), &provider).ok_or_else(|| "Unknown provider".into())
}

fn key_for(provider: &str) -> Option<String> {
    config::api_key(provider)
}

/// Fetch the model list (uses the saved key). Also refreshes the cache.
/// For keyless providers (Ollama/LM Studio/custom) this is the "connect".
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_list_models(provider: String) -> Result<Vec<ModelInfo>, String> {
    let p = preset(&provider).ok_or("Unknown provider")?;
    let cfg = config::snapshot();
    let base = cfg.base_url(&provider).ok_or("AI_BAD_URL: Set the endpoint URL first.")?;
    let key = key_for(&provider);
    if p.key == KeyNeed::Required && key.is_none() {
        return Err(AiError::NoKey(p.name.to_string()).to_string());
    }
    let models = client::list_models(&provider, &base, key.as_deref()).await.map_err(String::from)?;
    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let default = providers::pick_default_model(p, &ids);
    let vision = providers::pick_vision_model(p, &ids, default.as_deref());
    let m2 = models.clone();
    config::update(|c| {
        let st = c.provider_mut(&provider);
        st.models = m2;
        if st.model.as_ref().map_or(true, |m| !ids.contains(m)) {
            st.model = default.clone();
        }
        if st.vision_model.as_ref().map_or(true, |m| !ids.contains(m)) {
            st.vision_model = vision.clone();
        }
    })
    .await?;
    Ok(models)
}

#[derive(Serialize)]
pub struct TestResult {
    pub ok: bool,
    /// connected | wrong_key | no_credit | unreachable | bad_url | model_missing | other
    pub class: String,
    pub message: String,
    pub model_count: usize,
}

/// Test connection without sending any meeting content (lists models and
/// checks the chosen model exists), so it needs no consent.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_test(provider: String) -> Result<TestResult, String> {
    match ai_list_models(provider.clone()).await {
        Ok(models) => {
            let cfg = config::snapshot();
            let st = cfg.provider(&provider);
            let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
            let missing = st.model.as_deref().filter(|m| !ids.contains(m));
            Ok(match missing {
                Some(m) => TestResult {
                    ok: false,
                    class: "model_missing".into(),
                    message: format!("Connected, but the model '{}' isn't available on this account.", m),
                    model_count: models.len(),
                },
                None => TestResult {
                    ok: true,
                    class: "connected".into(),
                    message: format!("Connected · {} models", models.len()),
                    model_count: models.len(),
                },
            })
        }
        Err(e) => {
            let class = e
                .split(':')
                .next()
                .map(|p| match p {
                    "AI_WRONG_KEY" => "wrong_key",
                    "AI_NO_CREDIT" => "no_credit",
                    "AI_UNREACHABLE" => "unreachable",
                    "AI_BAD_URL" => "bad_url",
                    "AI_NO_KEY" => "no_key",
                    _ => "other",
                })
                .unwrap_or("other");
            Ok(TestResult { ok: false, class: class.into(), message: e, model_count: 0 })
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_grant_consent(provider: String) -> Result<AiStatus, String> {
    preset(&provider).ok_or("Unknown provider")?;
    config::update(|c| c.provider_mut(&provider).consent = true).await?;
    log::info!("AI: user allowed sending meeting content to {}", provider);
    ai_status().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_revoke_consent(provider: String) -> Result<AiStatus, String> {
    preset(&provider).ok_or("Unknown provider")?;
    config::update(|c| c.provider_mut(&provider).consent = false).await?;
    log::info!("AI: consent revoked for {}", provider);
    ai_status().await
}

#[derive(Serialize)]
pub struct ActiveInfo {
    pub provider: String,
    pub name: String,
    pub model: String,
    pub local: bool,
    pub consent: bool,
    /// ready | consent_required | no_key | bad_url | ...
    pub state: String,
}

#[derive(Serialize)]
pub struct AiStatus {
    pub text: Option<ActiveInfo>,
    pub vision: Option<ActiveInfo>,
    pub text_ready: bool,
    pub vision_ready: bool,
    /// Plain-language "what leaves this device" line for the active provider
    pub what_leaves: String,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_status() -> Result<AiStatus, String> {
    let cfg = config::snapshot();
    // Status checks must not trigger the consent dialog, so this does not
    // go through client::resolve (which emits the consent event).
    let text = active_info(&cfg, Kind::Text);
    let vision = active_info(&cfg, Kind::Vision);
    let what_leaves = match &text {
        None => "No AI provider set up. Nothing leaves this device.".to_string(),
        Some(t) if t.provider == providers::APPLE_PROVIDER => {
            "AI runs on Apple's on-device model. Meeting content never leaves this Mac.".to_string()
        }
        Some(t) if t.local => format!(
            "AI runs on {} on your own machine or network. Meeting content doesn't leave your control.",
            t.name
        ),
        Some(t) => format!(
            "When you use AI features, the transcript, meeting title, attendee names{} are sent to {} with your API key. Nothing is sent to noFriction.",
            if vision.is_some() { " and (for screen features) screenshots" } else { "" },
            t.name
        ),
    };
    Ok(AiStatus {
        text_ready: text.as_ref().map_or(false, |t| t.state == "ready"),
        vision_ready: vision.as_ref().map_or(false, |t| t.state == "ready"),
        text,
        vision,
        what_leaves,
    })
}

/// Active selection for `kind` with its readiness state (no network, no
/// consent event).
fn active_info(cfg: &config::AiConfig, kind: Kind) -> Option<ActiveInfo> {
    let sel = cfg.selection(kind)?;
    let p = preset(&sel.provider)?;
    let local = cfg.is_local(&sel.provider);
    let consent = cfg.provider(&sel.provider).consent;
    let state = if !local && !consent {
        "consent_required"
    } else if cfg.base_url(&sel.provider).is_none() {
        "bad_url"
    } else if !config::is_ready(cfg, &sel.provider) {
        "no_key"
    } else {
        "ready"
    };
    Some(ActiveInfo {
        provider: sel.provider.clone(),
        name: p.name.to_string(),
        model: sel.model,
        local,
        consent,
        state: state.to_string(),
    })
}
