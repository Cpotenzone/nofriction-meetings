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
    /// The matched preset's name when the URL is a preset's, else the generic name
    pub name: String,
    /// Preset id matching the configured URL (derived, never stored); None = custom/none
    pub preset: Option<String>,
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
    let key_status = secrets::status_of(config::api_key(cfg, id).as_deref());
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
    let matched = cfg.base_url(id).as_deref().and_then(providers::preset_for_url);
    Some(ProviderInfo {
        id: id.to_string(),
        name: cfg.display_name(id),
        preset: matched.map(|m| m.id.to_string()),
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

/// The static preset table (name, base URL, default model, key page, note).
/// Pure data for the settings form: nothing is selected, saved or contacted.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_list_presets() -> Result<&'static [providers::EndpointPreset], String> {
    Ok(providers::ENDPOINT_PRESETS)
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

/// Save a user-supplied custom-endpoint key locally. No detection or network probe.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_save_key(key: String, provider: Option<String>, expected_base_url: String) -> Result<SaveKeyResult, String> {
    if provider.as_deref() != Some("custom") {
        return Err("AI_NO_PROVIDER: Enter your custom endpoint URL first. Keys never select a service.".into());
    }
    let key = providers::normalize_key(&key);
    providers::validate_key_shape(&key)?;
    let cfg = config::with_snapshot(|cfg| {
        cfg.save_key_with("custom", &expected_base_url, |account| secrets::set(secrets::AI_SERVICE, account, &key))?;
        Ok::<_, String>(cfg.clone())
    })?;
    let st = cfg.provider("custom");
    Ok(SaveKeyResult {
        provider: "custom".into(),
        name: cfg.display_name("custom"),
        models: st.models.iter().map(|m| m.id.clone()).collect(),
        model: st.model,
        vision_model: st.vision_model,
        needs_consent: !cfg.is_local("custom") && !st.consent,
        last4: secrets::status_of(Some(&key)).last4,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_delete_key(provider: String) -> Result<AiStatus, String> {
    preset(&provider).ok_or("Unknown provider")?;
    let mut deleted = Ok(());
    config::update(|c| {
        if let Some(account) = c.credential_account(&provider) {
            deleted = secrets::delete(secrets::AI_SERVICE, &account);
            if deleted.is_err() { return; }
        }
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
    deleted?;
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
        .or(if p.protocol == Protocol::Apple { Some(providers::APPLE_MODEL.into()) } else { match kind {
            Kind::Text => st.model.clone(),
            Kind::Vision => st.vision_model.clone().or(st.model.clone()),
        } })
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
        None // no endpoint; there is no network default
    } else {
        Some(providers::check_base_url(&base_url)?)
    };
    let mut result = Ok(());
    config::update(|c| {
        let old_account = c.credential_account(&provider);
        result = c.change_endpoint(&provider, url.clone(), || {
            if let Some(account) = old_account { secrets::delete(secrets::AI_SERVICE, &account) } else { Ok(()) }
        });
    }).await?;
    result?;
    provider_info(&config::snapshot(), &provider).ok_or_else(|| "Unknown provider".into())
}

/// Fetch the model list (uses the saved key). Also refreshes the cache.
/// This is an explicit user-requested model refresh, never a startup probe.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_list_models(provider: String) -> Result<Vec<ModelInfo>, String> {
    let p = preset(&provider).ok_or("Unknown provider")?;
    let cfg = config::snapshot();
    let base = cfg.base_url(&provider).ok_or("AI_BAD_URL: Set the endpoint URL first.")?;
    let key = config::api_key(&cfg, &provider);
    if p.key == KeyNeed::Required && key.is_none() {
        return Err(AiError::NoKey(p.name.to_string()).to_string());
    }
    let models = client::list_models(&provider, &base, key.as_deref()).await.map_err(String::from)?;
    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let default = providers::pick_default_model(p, &ids);
    let vision = providers::pick_vision_model(p, &ids, default.as_deref());
    let m2 = models.clone();
    let mut destination_unchanged = Ok(());
    config::update(|c| {
        if provider == "custom" {
            destination_unchanged = c.check_endpoint(&provider, &base);
            if destination_unchanged.is_err() { return; }
        }
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
    destination_unchanged?;
    Ok(models)
}

/// Test connection: one fixed word ("Hi") with a one-token answer to the saved
/// endpoint with the saved key. It carries no meeting content, so it is the
/// only request allowed before consent, and it runs only on this explicit
/// command (never at startup or on save).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_test(provider: String) -> Result<client::ProbeOutcome, String> {
    let p = preset(&provider).ok_or("Unknown provider")?;
    let cfg = config::snapshot();
    let model = cfg
        .provider(&provider)
        .model
        .clone()
        .or(if p.protocol == Protocol::Apple { Some(providers::APPLE_MODEL.into()) } else { None })
        .ok_or("Enter a model and save the connection first.")?;
    let host = cfg
        .base_url(&provider)
        .and_then(|u| url::Url::parse(&u).ok())
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_else(|| p.name.to_string());
    let target = match client::resolve_for_probe(&cfg, &provider, &model) {
        Ok(t) => t,
        Err(e) => return Ok(client::ProbeOutcome::from_error(&e, &host)),
    };
    let outcome = client::probe(&target).await;
    log::info!("AI: connection test for {} ({}): {}", provider, target.hint, outcome.class);
    Ok(outcome)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_grant_consent(provider: String, expected_base_url: String) -> Result<AiStatus, String> {
    preset(&provider).ok_or("Unknown provider")?;
    let mut result = Ok(());
    config::update(|c| result = c.grant_endpoint_consent(&provider, &expected_base_url)).await?;
    result?;
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
        name: if p.protocol == Protocol::Apple { p.name.to_string() } else { cfg.display_name(&sel.provider) },
        model: sel.model,
        local,
        consent,
        state: state.to_string(),
    })
}
