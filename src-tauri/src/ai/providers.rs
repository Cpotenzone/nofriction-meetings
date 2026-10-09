//! Apple on-device and explicit custom endpoint policy, redaction and
//! per-model heuristics (context window, vision). Spec: docs/AI_PROVIDERS.md.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    OpenAI,
    Anthropic,
    /// Apple Foundation Models (on-device, macOS 26+), via the Swift bridge
    Apple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyNeed {
    Required,
    Optional,
    None,
}

#[derive(Debug)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub protocol: Protocol,
    /// Empty for `custom` (user-entered)
    pub base_url: &'static str,
    /// Empty: credentials never choose an endpoint.
    pub key_prefixes: &'static [&'static str],
    pub key_url: &'static str,
    pub key: KeyNeed,
    /// Runs on the user's own machine/network: no consent, http allowed
    pub local: bool,
    /// Base URL may be changed by the user
    pub editable_url: bool,
    /// Default model preference, exact ids, first match wins
    pub preferred: &'static [&'static str],
    /// Fallback: first model id containing one of these
    pub preferred_contains: &'static [&'static str],
}

pub const DEFAULT_PROVIDER: &str = "custom";

/// The Apple on-device preset (key-less fallback when no provider is set up).
pub const APPLE_PROVIDER: &str = "apple";
pub const APPLE_MODEL: &str = "apple-on-device";
/// Not a URL: marks the on-device provider in `AiConfig::base_url`.
pub const APPLE_BASE_URL: &str = "apple://on-device";
/// Foundation Models' context window (tokens, prompt + answer).
pub const APPLE_CONTEXT_TOKENS: usize = 4_096;

pub static PRESETS: &[Preset] = &[
    Preset {
        id: APPLE_PROVIDER,
        name: "Apple on-device",
        protocol: Protocol::Apple,
        base_url: APPLE_BASE_URL,
        key_prefixes: &[],
        key_url: "",
        key: KeyNeed::None,
        local: true,
        editable_url: false,
        preferred: &[APPLE_MODEL],
        preferred_contains: &[],
    },
    Preset {
        id: "custom",
        name: "Custom (OpenAI-compatible)",
        protocol: Protocol::OpenAI,
        base_url: "",
        key_prefixes: &[],
        key_url: "",
        key: KeyNeed::Optional,
        // Decided per URL: private hosts are local, public HTTPS hosts are cloud
        local: false,
        editable_url: true,
        preferred: &[],
        preferred_contains: &[],
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

// ---------------------------------------------------------------------------
// Endpoint presets (UI convenience over the one custom endpoint)
// ---------------------------------------------------------------------------

/// A named provider preset. Picking one only pre-fills the custom endpoint's
/// base URL and model in the settings form; the user still pastes their own
/// key, saves, and consents before any meeting content is sent. No preset is
/// selected or active until the user clicks it, and the saved configuration
/// remains an ordinary custom endpoint (same Keychain binding, same consent).
///
/// This table is the only place in the Mac source where provider hosts may
/// appear (`scripts/check-ai-provider-policy.py` enforces that, and checks
/// the iOS table carries the same hosts and models).
#[derive(Debug, Serialize)]
pub struct EndpointPreset {
    pub id: &'static str,
    pub name: &'static str,
    /// OpenAI-compatible chat-completions base (the app appends `/chat/completions`)
    pub base_url: &'static str,
    pub default_model: &'static str,
    /// Other model ids worth trying, for the hint under the model field
    pub model_hint: &'static str,
    /// Where the user creates their own API key ("Get a key" link)
    pub key_url: &'static str,
    /// One line shown on the card
    pub note: &'static str,
}

pub static ENDPOINT_PRESETS: &[EndpointPreset] = &[
    // OpenAI Chat Completions API. Base URL and endpoint per the API reference
    // (https://developers.openai.com/api/docs/changelog lists gpt-6-luna,
    // gpt-6.1-sol and gpt-6-astra under v1/chat/completions, Sep 2026;
    // model catalogue: https://developers.openai.com/api/docs/models).
    EndpointPreset {
        id: "openai",
        name: "ChatGPT (OpenAI)",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-6-luna",
        model_hint: "gpt-6.1-sol, gpt-6-astra",
        key_url: "https://platform.openai.com/api-keys",
        note: "Your OpenAI API key; billed by OpenAI.",
    },
    // Anthropic's OpenAI SDK compatibility layer: base URL
    // https://api.anthropic.com/v1/ with the Claude key as a Bearer token
    // (https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk).
    // Model ids: https://platform.claude.com/docs/en/models/overview.
    EndpointPreset {
        id: "anthropic",
        name: "Anthropic (Claude)",
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-sonnet-5-5",
        model_hint: "claude-haiku-5-5, claude-opus-5-5",
        key_url: "https://platform.claude.com/settings/keys",
        note: "Your Claude API key; billed by Anthropic.",
    },
    // xAI: OpenAI-compatible base https://api.x.ai/v1 with Bearer auth
    // (https://docs.x.ai/docs/guides/chat). xAI marks /v1/chat/completions as
    // deprecated in favour of /v1/responses but keeps it available
    // (https://docs.x.ai/developers/model-capabilities/text/comparison).
    // Model ids: https://docs.x.ai/docs/models.
    EndpointPreset {
        id: "xai",
        name: "Grok (xAI)",
        base_url: "https://api.x.ai/v1",
        default_model: "grok-4.7",
        model_hint: "grok-4.3",
        key_url: "https://console.x.ai",
        note: "Your xAI API key; billed by xAI.",
    },
    // Mistral: POST https://api.mistral.ai/v1/chat/completions with Bearer auth
    // (https://docs.mistral.ai/api/); model aliases mistral-large-latest and
    // mistral-small-latest appear in that reference.
    EndpointPreset {
        id: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        default_model: "mistral-large-latest",
        model_hint: "mistral-small-latest",
        key_url: "https://console.mistral.ai/api-keys",
        note: "Your Mistral API key; billed by Mistral.",
    },
];

pub fn endpoint_preset(id: &str) -> Option<&'static EndpointPreset> {
    ENDPOINT_PRESETS.iter().find(|p| p.id == id)
}

/// The preset whose base URL is exactly the configured one (after
/// normalization), if any. Derived from the URL, never stored: a user who
/// edits the URL to a proxy is no longer "on" the preset.
pub fn preset_for_url(base_url: &str) -> Option<&'static EndpointPreset> {
    let base = check_base_url(base_url).ok()?;
    ENDPOINT_PRESETS.iter().find(|p| p.base_url == base)
}

/// Provider id to use for per-service heuristics (context window, vision,
/// token-parameter quirks): the matched preset's id, else the provider id.
pub fn provider_hint(provider: &str, base_url: Option<&str>) -> &'static str {
    base_url
        .and_then(preset_for_url)
        .map(|p| p.id)
        .unwrap_or(match preset(provider) {
            Some(p) => p.id,
            None => "custom",
        })
}

/// Display name for a configured endpoint: the preset name when the URL is a
/// preset's, else the provider's generic name.
pub fn display_name(provider: &str, base_url: Option<&str>) -> String {
    if let Some(p) = base_url.and_then(preset_for_url) {
        return p.name.to_string();
    }
    preset(provider).map(|p| p.name.to_string()).unwrap_or_else(|| provider.to_string())
}

// ---------------------------------------------------------------------------
// Paste-a-key
// ---------------------------------------------------------------------------

/// Clean up what people paste: whitespace, quotes, `Bearer `, and a leading
/// `OPENAI_API_KEY=` / `export X=` style assignment.
pub fn normalize_key(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("export ") {
        s = rest.trim();
    }
    // NAME=value (env-file style). Only when NAME looks like an identifier.
    if let Some((name, value)) = s.split_once('=') {
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && name.chars().any(|c| c.is_ascii_uppercase())
        {
            s = value.trim();
        }
    }
    let s = s.trim_matches(|c| c == '"' || c == '\'' || c == '`').trim();
    let s = if s.len() >= 7 && s[..7].eq_ignore_ascii_case("bearer ") {
        s[7..].trim()
    } else {
        s
    };
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Basic sanity check on a normalized key.
pub fn validate_key_shape(key: &str) -> Result<(), String> {
    if key.len() < 8 {
        return Err("That doesn't look like an API key (too short).".into());
    }
    if key.len() > 512 {
        return Err("That doesn't look like an API key (too long).".into());
    }
    if !key.chars().all(|c| c.is_ascii_graphic()) {
        return Err("API keys contain only plain letters, digits and symbols.".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Detection {
    pub provider: Option<String>,
    pub name: Option<String>,
    /// e.g. bare `sk-` could also be DeepSeek
    pub alternatives: Vec<String>,
}

/// Retained compatibility response: credentials never choose a destination.
pub fn detect_provider(_key: &str) -> Detection {
    // A credential never chooses a destination. The user must enter a URL.
    Detection { provider: None, name: None, alternatives: vec![] }
}

// ---------------------------------------------------------------------------
// URL policy
// ---------------------------------------------------------------------------

/// Hosts where plain http is acceptable: loopback, `*.local`, RFC1918,
/// Tailscale CGNAT 100.64.0.0/10 and `*.ts.net`.
pub fn is_private_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") || h.ends_with(".ts.net") {
        return true;
    }
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                let o = v4.octets();
                v4.is_loopback()
                    || o[0] == 10
                    || (o[0] == 172 && (16..=31).contains(&o[1]))
                    || (o[0] == 192 && o[1] == 168)
                    || (o[0] == 100 && (64..=127).contains(&o[1]))
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
        };
    }
    false
}

/// Validate and normalize a base URL. HTTPS anywhere; HTTP only for private
/// hosts. No credentials, query or fragment. Trailing slash removed.
pub fn check_base_url(raw: &str) -> Result<String, String> {
    let url = url::Url::parse(raw.trim()).map_err(|_| "Enter a full URL, e.g. https://host/v1".to_string())?;
    let host = url.host_str().ok_or("The URL has no host")?.to_string();
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Don't put credentials in the URL; paste the key separately.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("The base URL can't have ?query or #fragment.".into());
    }
    match url.scheme() {
        "https" => {}
        "http" if is_private_host(&host) => {}
        "http" => {
            return Err("Cloud endpoints must use https://. Plain http is only allowed for localhost, *.local, private (LAN/Tailscale) addresses.".into())
        }
        other => return Err(format!("Unsupported URL scheme '{}'", other)),
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Is this base URL on the user's own machine/network?
pub fn url_is_local(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(is_private_host))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

static KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(sk-ant-[A-Za-z0-9_\-]{6,}|sk-(?:proj|svcacct|or)-[A-Za-z0-9_\-]{6,}|sk-[A-Za-z0-9_\-]{12,}|xai-[A-Za-z0-9_\-]{8,}|gsk_[A-Za-z0-9_\-]{8,}|pplx-[A-Za-z0-9_\-]{8,}|AIza[0-9A-Za-z_\-]{16,})",
    )
    .unwrap()
});
static BEARER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)(bearer\s+)[^\s"',;]+"#).unwrap());
static HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)((?:x-api-key|x-goog-api-key|api[_-]?key)["']?\s*[:=]\s*["']?)[^\s"',;&]+"#).unwrap());
static QUERY_KEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"([?&](?:key|api_key|token)=)[^&\s"']+"#).unwrap());
static URL_PASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([a-z][a-z0-9+.\-]*://[^:/\s@]+:)[^@\s]+@").unwrap());

/// Remove anything that looks like a credential from a string destined for
/// a log line or the UI.
pub fn redact(s: &str) -> String {
    let s = KEY_RE.replace_all(s, "[redacted-key]");
    let s = BEARER_RE.replace_all(&s, "${1}[redacted]");
    let s = HEADER_RE.replace_all(&s, "${1}[redacted]");
    let s = QUERY_KEY_RE.replace_all(&s, "${1}[redacted]");
    let s = URL_PASS_RE.replace_all(&s, "${1}[redacted]@");
    s.into_owned()
}

/// `redact`, plus removal of a specific known secret (any shape).
pub fn redact_with(s: &str, secret: Option<&str>) -> String {
    let s = match secret {
        Some(k) if k.len() >= 6 => s.replace(k, "[redacted-key]"),
        _ => s.to_string(),
    };
    redact(&s)
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Filter out ids that are not chat/completions models.
pub fn is_chat_model(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    const EXCLUDE: &[&str] = &[
        "embed", "whisper", "tts", "dall-e", "davinci", "babbage", "moderation", "audio", "realtime",
        "transcribe", "gpt-image", "-image", "imagen", "sora", "veo", "lyria", "computer-use",
        "codex", "deep-research", "rerank", "guard", "aqa", "turbo-instruct", "search-api",
        "text-bison", "chat-bison", "-live", "native-audio",
    ];
    // OpenAI "-pro" reasoning models are Responses-API only
    let responses_only = (l.starts_with("o1-pro") || l.starts_with("o3-pro"))
        || (l.starts_with("gpt-") && l.contains("-pro"));
    !responses_only && !EXCLUDE.iter().any(|x| l.contains(x))
}

/// Pick the default model from a provider's model list.
pub fn pick_default_model(p: &Preset, models: &[String]) -> Option<String> {
    let chat: Vec<&String> = models.iter().filter(|m| is_chat_model(m)).collect();
    for want in p.preferred {
        if let Some(m) = chat.iter().find(|m| m.as_str() == *want || m.strip_prefix("models/") == Some(want)) {
            return Some((*m).clone());
        }
    }
    for needle in p.preferred_contains {
        if let Some(m) = chat.iter().find(|m| m.contains(needle)) {
            return Some((*m).clone());
        }
    }
    chat.first().map(|m| (*m).clone())
}

/// Pick a vision-capable default, if the list has one.
pub fn pick_vision_model(p: &Preset, models: &[String], text_model: Option<&str>) -> Option<String> {
    if let Some(t) = text_model {
        if supports_vision(p.id, t) == Some(true) {
            return Some(t.to_string());
        }
    }
    models
        .iter()
        .filter(|m| is_chat_model(m))
        .find(|m| supports_vision(p.id, m) == Some(true))
        .cloned()
}

/// Does this model accept images? `None` = can't tell.
pub fn supports_vision(provider: &str, model: &str) -> Option<bool> {
    let m = model.to_ascii_lowercase();
    let has = |xs: &[&str]| xs.iter().any(|x| m.contains(x));
    match provider {
        "anthropic" => Some(!m.contains("claude-2") && !m.contains("instant")),
        "openai" => Some(has(&["gpt-4o", "gpt-4.1", "gpt-5", "gpt-6", "o1", "o3", "o4", "gpt-4-turbo", "vision"])),
        "gemini" => Some(m.contains("gemini") && !m.contains("embedding")),
        "xai" => Some(has(&["vision", "grok-4", "grok-2-vision"])),
        "groq" => Some(has(&["llama-4", "vision"])),
        "mistral" => Some(has(&["pixtral", "mistral-medium", "mistral-small-2503", "mistral-small-latest", "magistral-medium"])),
        "deepseek" => Some(false),
        APPLE_PROVIDER => Some(false),
        "perplexity" => None,
        "together" => Some(has(&["vision", "-vl", "llama-4"])),
        "ollama" | "lmstudio" => Some(has(&[
            "vl", "vision", "llava", "gemma3", "minicpm-v", "moondream", "bakllava", "llama4", "mistral-small3",
        ])),
        _ => None, // openrouter, custom: unknown
    }
}

/// Context window (tokens) for fitting prompts. Conservative; capped at 128K
/// so we never ship megabytes of transcript to a 1M-token model by accident.
pub fn context_window(provider: &str, model: &str, reported: Option<usize>) -> usize {
    const CAP: usize = 128_000;
    if let Some(r) = reported.filter(|r| *r > 1024) {
        return r.min(CAP);
    }
    let m = model.to_ascii_lowercase();
    let known = match provider {
        // Claude 4.6+ and GPT-6 report 1M; the 128K cap below still applies
        "anthropic" => Some(1_000_000),
        "gemini" => Some(1_000_000),
        "openai" if m.starts_with("gpt-3.5") => Some(16_000),
        "openai" => Some(128_000),
        "xai" | "groq" | "mistral" | "perplexity" => Some(128_000),
        "deepseek" => Some(64_000),
        // Local servers often run with a small default context (Ollama 4-8K)
        "ollama" | "lmstudio" => Some(8_192),
        APPLE_PROVIDER => return APPLE_CONTEXT_TOKENS,
        _ => None,
    };
    known.unwrap_or(32_768).min(CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(k: &str) -> Option<String> {
        detect_provider(k).provider
    }

    #[test]
    fn no_key_prefix_selects_a_service() {
        for key in ["sk-test-user-key", "AIza-test-user-key", "gsk_test-user-key", "custom-user-key"] {
            assert_eq!(det(key), None);
        }
        assert_eq!(PRESETS.iter().map(|p| p.id).collect::<Vec<_>>(), vec!["apple", "custom"]);
        assert_eq!(preset("custom").unwrap().base_url, "");
        assert!(PRESETS.iter().all(|p| !p.base_url.starts_with("http")));
        for legacy in ["gemini", "openai", "anthropic", "ollama", "lmstudio", "deepseek"] {
            assert!(preset(legacy).is_none());
        }
    }

    #[test]
    fn endpoint_presets_are_static_https_and_never_selected_by_default() {
        assert_eq!(
            ENDPOINT_PRESETS.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec!["openai", "anthropic", "xai", "mistral"]
        );
        for p in ENDPOINT_PRESETS {
            assert_eq!(check_base_url(p.base_url).unwrap(), p.base_url, "{}", p.id);
            assert!(p.base_url.starts_with("https://"), "{}", p.id);
            assert!(!url_is_local(p.base_url), "{}: a preset is a public endpoint and needs consent", p.id);
            assert!(p.key_url.starts_with("https://"), "{}", p.id);
            assert!(!p.default_model.trim().is_empty(), "{}", p.id);
            assert_eq!(preset_for_url(p.base_url).map(|x| x.id), Some(p.id));
            assert_eq!(preset_for_url(&format!("{}/", p.base_url)).map(|x| x.id), Some(p.id));
            // Presets never become a provider id: the saved connection stays "custom"
            assert!(preset(p.id).is_none(), "{}", p.id);
        }
        assert!(preset_for_url("https://proxy.example.com/v1").is_none());
        assert!(preset_for_url("http://127.0.0.1:11434/v1").is_none());
        assert_eq!(provider_hint("custom", Some("https://api.openai.com/v1/")), "openai");
        assert_eq!(provider_hint("custom", Some("https://proxy.example.com/v1")), "custom");
        assert_eq!(provider_hint("custom", None), "custom");
        assert_eq!(provider_hint("apple", None), "apple");
        assert_eq!(display_name("custom", Some("https://api.anthropic.com/v1")), "Anthropic (Claude)");
        assert_eq!(display_name("custom", Some("https://proxy.example.com/v1")), "Custom (OpenAI-compatible)");
        // Nothing is pre-selected: the provider table and a fresh config have no preset URL
        assert!(PRESETS.iter().all(|p| preset_for_url(p.base_url).is_none()));
    }

    #[test]
    fn normalizes_pasted_keys() {
        assert_eq!(normalize_key("  \"sk-proj-abc123\"\n"), "sk-proj-abc123");
        assert_eq!(normalize_key("Bearer xai-abc"), "xai-abc");
        assert_eq!(normalize_key("OPENAI_API_KEY=sk-abc"), "sk-abc");
        assert_eq!(normalize_key("export GROQ_API_KEY='gsk_abc'"), "gsk_abc");
        assert!(validate_key_shape("short").is_err());
        assert!(validate_key_shape("sk-proj-abcdefgh").is_ok());
        assert!(validate_key_shape("sk-proj-abc\u{e9}defgh").is_err());
    }

    #[test]
    fn url_policy() {
        assert_eq!(check_base_url("https://ai.example.com/v1/").unwrap(), "https://ai.example.com/v1");
        assert!(check_base_url("http://localhost:11434/v1").is_ok());
        assert!(check_base_url("http://127.0.0.1:1234/v1").is_ok());
        assert!(check_base_url("http://192.168.1.20:8000/v1").is_ok());
        assert!(check_base_url("http://10.0.0.5/v1").is_ok());
        assert!(check_base_url("http://172.20.0.5/v1").is_ok());
        assert!(check_base_url("http://100.101.1.2:8443").is_ok());
        assert!(check_base_url("http://box.tail1234.ts.net:8443").is_ok());
        assert!(check_base_url("http://mac-mini.local:11434/v1").is_ok());
        assert!(check_base_url("http://api.example.com/v1").is_err());
        assert!(check_base_url("http://172.32.0.1/v1").is_err());
        assert!(check_base_url("http://100.128.0.1/v1").is_err());
        assert!(check_base_url("https://user:pw@example.com/v1").is_err());
        assert!(check_base_url("https://example.com/v1?key=x").is_err());
        assert!(check_base_url("ftp://example.com").is_err());
        assert!(check_base_url("not a url").is_err());
        assert!(url_is_local("http://localhost:11434/v1"));
        assert!(!url_is_local("https://ai.example.com/v1"));
    }

    #[test]
    fn redaction() {
        let s = "401 for key sk-proj-ABCDEFGHIJKLMNOP and Authorization: Bearer xyz.123 x-api-key: sk-ant-api03-SECRETSECRET ?key=AIzaSyABCDEFGHIJKLMNOPQRS postgres://u:hunter2@db:5432";
        let r = redact(s);
        assert!(!r.contains("ABCDEFGHIJKLMNOP"), "{}", r);
        assert!(!r.contains("xyz.123"), "{}", r);
        assert!(!r.contains("SECRETSECRET"), "{}", r);
        assert!(!r.contains("AIzaSyABCDEFGHIJKLMNOPQRS"), "{}", r);
        assert!(!r.contains("hunter2"), "{}", r);
        assert!(r.contains("401 for key"));
        assert!(!redact_with("echo: my-custom-token-123", Some("my-custom-token-123")).contains("token-123"));
        assert!(!redact("gsk_abcdefghijkl xai-abcdefghijkl pplx-abcdefghijkl").contains("abcdefghijkl"));
    }

    #[test]
    fn custom_models_come_only_from_the_supplied_list() {
        let p = preset("custom").unwrap();
        assert!(pick_default_model(p, &[]).is_none());
        assert_eq!(pick_default_model(p, &["user-model".into()]).as_deref(), Some("user-model"));
    }

    #[test]
    fn context_windows() {
        assert_eq!(context_window("custom", "x", None), 32_768);
        assert_eq!(context_window("ollama", "qwen3:8b", None), 8_192);
        assert_eq!(context_window("gemini", "gemini-2.5-flash", None), 128_000);
        assert_eq!(context_window("anthropic", "claude-haiku-4-5", Some(200_000)), 128_000);
        assert_eq!(context_window("custom", "x", Some(16_000)), 16_000);
    }
}
