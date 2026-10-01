//! Bring-your-own-key AI provider layer (spec: docs/AI_PROVIDERS.md).
//!
//! - `providers`: presets, key auto-detect, URL policy, redaction, heuristics
//! - `config`:    active text/vision provider + per-provider settings, consent
//! - `client`:    OpenAI-compatible + Anthropic adapters with guardrails
//! - `commands`:  Tauri commands for the settings UI
//!
//! Keys live in the Keychain (`crate::secrets`). No servers of ours.

pub mod client;
pub mod commands;
pub mod config;
pub mod providers;

pub use client::{AiError, Msg, Opts, Part, Role};
pub use config::Kind;

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::Emitter;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static LAST_CONSENT_EVENT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

/// Frontend event carrying the provider id that needs consent.
pub const CONSENT_EVENT: &str = "ai-consent-required";

pub fn set_app_handle(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// Ask the UI to show the consent dialog (at most once a minute per
/// provider, so background jobs don't spam it).
pub fn emit_consent_required(provider: &str) {
    let Some(app) = APP.get() else { return };
    let map = LAST_CONSENT_EVENT.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let mut m = map.lock();
        if let Some(t) = m.get(provider) {
            if t.elapsed() < Duration::from_secs(60) {
                return;
            }
        }
        m.insert(provider.to_string(), Instant::now());
    }
    let _ = app.emit(CONSENT_EVENT, provider.to_string());
}

/// Text completion on the active text provider.
pub async fn complete_text(msgs: Vec<Msg>, opts: Opts) -> Result<String, AiError> {
    client::complete(Kind::Text, msgs, opts).await
}

/// Completion with image(s) on the active vision provider.
pub async fn complete_vision(msgs: Vec<Msg>, opts: Opts) -> Result<String, AiError> {
    client::complete(Kind::Vision, msgs, opts).await
}

/// A provider is selected, configured and allowed for this kind (no network).
pub fn is_ready(kind: Kind) -> bool {
    client::resolve(kind).is_ok()
}
