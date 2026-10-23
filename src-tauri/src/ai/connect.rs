//! Connecting an AI in one step (docs/AI_PROVIDERS.md "Connecting").
//!
//! - `ai_connect`: **Connect** / **Paste key and connect**. Saves the endpoint,
//!   the key and the model, then sends the one-word connection test, all from
//!   one explicit click. With `fromClipboard` the key goes from the clipboard
//!   straight to the Keychain; the screen never holds it.
//! - `ai_sign_in`: a preset's own sign-in (OpenRouter's OAuth PKCE). The
//!   browser opens the service's page; a loopback-only listener on this Mac
//!   receives the one-time code; the code is exchanged for the user's own
//!   key; then the same connect step. `ai_sign_in_cancel` stops it.
//! - `ai_find_local`: AI servers already running on this Mac (Ollama, LM
//!   Studio, Jan, llama.cpp), asked for their model list on loopback only.
//!
//! All of these run only from an explicit click: never at startup, on save
//! or on picking a card. None sends meeting content: the test is the word
//! "Hi", the sign-in and the model lists carry nothing of the user's. Meeting
//! content still waits for consent (public endpoints) as before.

use super::client::ProbeOutcome;
use super::commands;
use super::config::{self, Kind};
use super::providers;
use base64::Engine as _;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

/// What one Connect click did.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectResult {
    /// The test answered
    pub ok: bool,
    /// connected | wrong_key | no_credit | unreachable | bad_url | model_missing | no_key | other
    pub class: String,
    /// Plain words for the user (the key is never in it)
    pub message: String,
    /// The endpoint is public and meeting content still needs the user's yes
    pub needs_consent: bool,
    /// "ChatGPT (OpenAI)", "OpenRouter (every model)", "Custom (OpenAI-compatible)"
    pub name: String,
    pub model: String,
}

/// Save endpoint + key + model (as the separate Save does), make it the text
/// AI, then run the connection test.
async fn connect(base_url: &str, key: Option<String>, model: &str) -> Result<ConnectResult, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Enter a model.".into());
    }
    let info = commands::ai_set_custom_endpoint("custom".into(), base_url.into()).await?;
    let base = info.base_url.ok_or("Enter your endpoint's base URL.")?;
    if let Some(k) = key {
        commands::ai_save_key(k, Some("custom".into()), base.clone()).await?;
    }
    commands::ai_set_active("custom".into(), Some(model.to_string()), Kind::Text).await?;
    let outcome: ProbeOutcome = commands::ai_test("custom".into()).await?;
    let cfg = config::snapshot();
    log::info!("AI: connect to {} finished: {}", providers::preset_for_url(&base).map_or("custom endpoint", |p| p.id), outcome.class);
    Ok(ConnectResult {
        ok: outcome.ok,
        class: outcome.class.to_string(),
        message: outcome.message,
        needs_consent: !cfg.is_local("custom") && !cfg.provider("custom").consent,
        name: cfg.display_name("custom"),
        model: model.to_string(),
    })
}

/// The clipboard's text, read only on the user's click.
#[cfg(target_os = "macos")]
fn clipboard_text() -> Option<String> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb.is_null() {
            return None;
        }
        let kind: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"public.utf8-plain-text".as_ptr()];
        let s: *mut Object = msg_send![pb, stringForType: kind];
        if s.is_null() {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

#[cfg(not(target_os = "macos"))]
fn clipboard_text() -> Option<String> {
    None
}

/// A key copied from a provider's page, cleaned the way a paste is, or
/// what to say when the clipboard holds something else. Shape only (never
/// which service): one token of at least 20 key characters. A password, a
/// sentence or a web address is refused, so it is never sent anywhere.
pub fn key_from_clipboard(text: Option<&str>, host: &str) -> Result<String, String> {
    let copy_first = || format!("Copy your key on {host} first, then click again.");
    let raw = text.map(str::trim).filter(|t| !t.is_empty()).ok_or_else(copy_first)?;
    // What a paste may legitimately carry around the key
    let mut core = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').trim();
    for prefix in ["export ", "Bearer ", "bearer "] {
        if let Some(rest) = core.strip_prefix(prefix) {
            core = rest.trim();
        }
    }
    if core.chars().any(char::is_whitespace) {
        return Err(copy_first());
    }
    let key = providers::normalize_key(raw);
    let key_chars = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '+' | '/' | '=');
    if key.len() < 20 || key.contains("://") || !key.chars().all(key_chars) {
        return Err(copy_first());
    }
    providers::validate_key_shape(&key).map_err(|_| copy_first())?;
    Ok(key)
}

/// **Connect** (key typed or pasted) and **Paste key and connect**
/// (`fromClipboard`). One explicit click: save, then the one-word test.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_connect(base_url: String, model: String, key: Option<String>, from_clipboard: bool) -> Result<ConnectResult, String> {
    let base = providers::check_base_url(&base_url)?;
    let key = if from_clipboard {
        let host = url::Url::parse(&base).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
        let key_host = providers::preset_for_url(&base)
            .and_then(|p| url::Url::parse(p.key_url).ok())
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or(host);
        Some(key_from_clipboard(clipboard_text().as_deref(), &key_host)?)
    } else {
        let key = key.map(|k| providers::normalize_key(&k)).filter(|k| !k.is_empty());
        // Checked before the endpoint changes: a bad key must not wipe a working connection
        if let Some(k) = &key {
            providers::validate_key_shape(k)?;
        }
        key
    };
    connect(&base, key, &model).await
}

// ─── Sign-in (OAuth PKCE, loopback redirect) ─────────────────────────────

/// One sign-in at a time; a new one or Cancel stops the running one. The id
/// keeps a finishing flow from clearing a newer flow's slot.
static SIGN_IN: Lazy<Mutex<Option<(u64, watch::Sender<bool>)>>> = Lazy::new(|| Mutex::new(None));
static SIGN_IN_IDS: AtomicU64 = AtomicU64::new(1);

/// Resolves when Cancel was pressed; never when the flow just ended.
async fn cancelled(mut rx: watch::Receiver<bool>) {
    if rx.wait_for(|c| *c).await.is_err() {
        std::future::pending::<()>().await;
    }
}

pub const CANCELLED: &str = "SIGN_IN_CANCELLED: Sign-in cancelled.";

/// How long the browser page may take (signing up included).
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The secrets of one sign-in: the PKCE verifier (kept here) and its S256
/// challenge (sent), and `state` against forged callbacks.
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl Pkce {
    pub fn new() -> Self {
        use rand::RngCore;
        let mut v = [0u8; 48];
        let mut st = [0u8; 18];
        rand::rng().fill_bytes(&mut v);
        rand::rng().fill_bytes(&mut st);
        Self::from_verifier(b64url(&v), b64url(&st))
    }

    pub fn from_verifier(verifier: String, state: String) -> Self {
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        Self { verifier, challenge, state }
    }

    /// The service's page with everything it needs to come back here.
    pub fn authorize_url(&self, sign_in_url: &str, callback: &str) -> Result<String, String> {
        let mut u = url::Url::parse(sign_in_url).map_err(|_| "The sign-in address is invalid.".to_string())?;
        u.query_pairs_mut()
            .append_pair("callback_url", callback)
            .append_pair("code_challenge", &self.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &self.state)
            .append_pair("key_label", "noFriction");
        Ok(u.into())
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// What the browser brought back to the loopback page.
#[derive(Debug, PartialEq)]
pub enum Callback {
    Code(String),
    /// The user said no (the page reports `error`)
    Denied,
    /// Not our callback (a favicon, a stray or forged request): answer it
    /// and keep waiting, so nobody but the real callback can end the sign-in
    Other,
}

/// Read the request line `GET /callback?code=…&state=… HTTP/1.1`. A code
/// counts only with our `state`.
pub fn parse_callback(request: &str, state: &str) -> Callback {
    let Some(target) = request.lines().next().and_then(|l| l.split_whitespace().nth(1)) else { return Callback::Other };
    let Ok(u) = url::Url::parse(&format!("http://localhost{}", target)) else { return Callback::Other };
    if u.path() != "/callback" {
        return Callback::Other;
    }
    let get = |k: &str| u.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
    match (get("code"), get("state"), get("error")) {
        (Some(code), Some(s), _) if s == state && !code.is_empty() && code.len() <= 2048 => Callback::Code(code),
        (None, _, Some(_)) => Callback::Denied,
        _ => Callback::Other,
    }
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>noFriction</title>\
<body style=\"font:16px -apple-system,sans-serif;margin:15vh auto;max-width:28em;text-align:center\">";

fn page(text: &str) -> String {
    let body = format!("{PAGE}<h2>{text}</h2><p>You can close this tab and go back to noFriction.</p></body>");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
        body.len(),
        body
    )
}

/// Loopback listeners for the redirect: 127.0.0.1 on a free port, and ::1
/// on the same port when the Mac has it (browsers may resolve `localhost`
/// either way). Nothing off this Mac can reach them.
pub struct Loopback {
    v4: TcpListener,
    v6: Option<TcpListener>,
    pub port: u16,
}

impl Loopback {
    pub async fn bind() -> Result<Self, String> {
        let v4 = TcpListener::bind("127.0.0.1:0").await.map_err(|e| format!("Couldn't start the sign-in: {}", e))?;
        let port = v4.local_addr().map_err(|e| format!("Couldn't start the sign-in: {}", e))?.port();
        let v6 = TcpListener::bind(format!("[::1]:{}", port)).await.ok();
        Ok(Self { v4, v6, port })
    }

    pub fn callback_url(&self) -> String {
        format!("http://localhost:{}/callback", self.port)
    }

    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
        match &self.v6 {
            Some(v6) => tokio::select! {
                r = self.v4.accept() => r.map(|(s, _)| s),
                r = v6.accept() => r.map(|(s, _)| s),
            },
            None => self.v4.accept().await.map(|(s, _)| s),
        }
    }

    /// Wait for the browser to come back with the code. Each connection is
    /// served on its own, so a silent preconnect can't hold up the callback.
    pub async fn wait_for_code(&self, state: &str) -> Result<String, String> {
        let (tx, mut rx) = mpsc::channel::<Result<String, String>>(4);
        loop {
            tokio::select! {
                Some(result) = rx.recv() => return result,
                accepted = self.accept() => {
                    let Ok(stream) = accepted else {
                        // Out of descriptors or similar: don't spin
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    };
                    let (tx, state) = (tx.clone(), state.to_string());
                    tokio::spawn(async move {
                        if let Some(result) = serve(stream, &state).await {
                            let _ = tx.send(result).await;
                        }
                    });
                }
            }
        }
    }
}

/// One request on the loopback page: the callback ends the wait, anything
/// else gets an answer and is ignored.
async fn serve(mut stream: tokio::net::TcpStream, state: &str) -> Option<Result<String, String>> {
    let mut buf = vec![0u8; 8192];
    let n = match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        _ => return None,
    };
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    match parse_callback(&request, state) {
        Callback::Code(code) => {
            let _ = stream.write_all(page("Signed in").as_bytes()).await;
            Some(Ok(code))
        }
        Callback::Denied => {
            let _ = stream.write_all(page("Sign-in didn't finish").as_bytes()).await;
            Some(Err("The sign-in was cancelled in the browser.".into()))
        }
        Callback::Other => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            None
        }
    }
}

/// Trade the one-time code for the user's own key.
pub async fn exchange(key_exchange_url: &str, code: &str, pkce: &Pkce) -> Result<String, String> {
    let body = serde_json::json!({
        "code": code,
        "code_verifier": pkce.verifier,
        "code_challenge_method": "S256",
    });
    let resp = super::client::http()
        .post(key_exchange_url)
        .timeout(Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|_| "Couldn't reach the service to finish signing in. Check your internet connection.".to_string())?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await.map_err(|_| "The service's answer couldn't be read.".to_string())?;
    if status == 403 || status == 400 {
        return Err("The service didn't accept the sign-in (it may have taken too long). Try again.".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("The service couldn't finish the sign-in (HTTP {}). Try again in a moment.", status));
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "The service's answer couldn't be read.".to_string())?;
    let key = v["key"].as_str().map(providers::normalize_key).ok_or("The service didn't send a key.")?;
    providers::validate_key_shape(&key).map_err(|_| "The service sent an unusable key.".to_string())?;
    Ok(key)
}

/// The sign-in itself, with the browser step injected (tests drive it).
/// Cancel stops it at any point before the key comes back.
pub async fn sign_in_flow<F>(sign_in_url: &str, key_exchange_url: &str, open: F, cancel: watch::Receiver<bool>) -> Result<String, String>
where
    F: FnOnce(String) -> Result<(), String>,
{
    let pkce = Pkce::new();
    let loopback = Loopback::bind().await?;
    open(pkce.authorize_url(sign_in_url, &loopback.callback_url())?)?;
    let code = tokio::select! {
        r = tokio::time::timeout(SIGN_IN_TIMEOUT, loopback.wait_for_code(&pkce.state)) => {
            r.map_err(|_| "The sign-in timed out. Try again.".to_string())??
        }
        _ = cancelled(cancel.clone()) => return Err(CANCELLED.into()),
    };
    drop(loopback);
    tokio::select! {
        r = exchange(key_exchange_url, &code, &pkce) => r,
        _ = cancelled(cancel) => Err(CANCELLED.into()),
    }
}

/// **Sign in with …**: the preset's sign-in in the browser, then connect with
/// the key it issues and the preset's default model.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_sign_in(preset: String) -> Result<ConnectResult, String> {
    let p = providers::endpoint_preset(&preset)
        .filter(|p| !p.sign_in_url.is_empty() && !p.key_exchange_url.is_empty())
        .ok_or("That service has no sign-in. Paste a key instead.")?;
    let id = SIGN_IN_IDS.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = watch::channel(false);
    if let Some((_, old)) = SIGN_IN.lock().replace((id, tx)) {
        let _ = old.send(true);
    }
    let open = |url: String| tauri_plugin_opener::open_url(url, None::<&str>).map_err(|_| "Couldn't open your browser.".to_string());
    let result = sign_in_flow(p.sign_in_url, p.key_exchange_url, open, rx.clone()).await;
    // Only this flow's own slot: a newer sign-in keeps its Cancel
    {
        let mut slot = SIGN_IN.lock();
        if slot.as_ref().is_some_and(|(i, _)| *i == id) {
            slot.take();
        }
    }
    let key = result?;
    if *rx.borrow() {
        return Err(CANCELLED.into());
    }
    log::info!("AI: signed in to {}", p.id);
    connect(p.base_url, Some(key), p.default_model).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ai_sign_in_cancel() -> Result<(), String> {
    if let Some((_, tx)) = SIGN_IN.lock().take() {
        let _ = tx.send(true);
    }
    Ok(())
}

// ─── AI already on this Mac ──────────────────────────────────────────────

/// Servers people run for local AI, at their usual ports (all
/// OpenAI-compatible at `/v1`).
pub const LOCAL_SERVERS: &[(&str, u16)] = &[("Ollama", 11434), ("LM Studio", 1234), ("Jan", 1337), ("llama.cpp", 8080)];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LocalServer {
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Chat models in a `/v1/models` answer (embedding models skipped).
pub fn local_models(body: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else { return Vec::new() };
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| !id.to_ascii_lowercase().contains("embed"))
        .take(200)
        .map(String::from)
        .collect()
}

async fn ask_local(name: &str, port: u16) -> Option<LocalServer> {
    let base = format!("http://127.0.0.1:{}/v1", port);
    let resp = super::client::http().get(format!("{}/models", base)).timeout(Duration::from_millis(1500)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let models = local_models(&resp.bytes().await.ok()?);
    (!models.is_empty()).then(|| LocalServer { name: name.to_string(), base_url: base, models })
}

/// **Find AI on this Mac**: which local servers answer, with their models.
/// Loopback only, on click.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_find_local() -> Result<Vec<LocalServer>, String> {
    let found = futures_util::future::join_all(LOCAL_SERVERS.iter().map(|(n, p)| ask_local(n, *p))).await;
    Ok(found.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_is_rfc7636_s256() {
        // RFC 7636 appendix B
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(), "st".into());
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let a = Pkce::new();
        let b = Pkce::new();
        assert_ne!(a.verifier, b.verifier);
        assert!(a.verifier.len() >= 43 && a.verifier.len() <= 128, "verifier length per RFC 7636");
        let url = a.authorize_url("https://sign-in.example/auth", "http://localhost:5000/callback").unwrap();
        assert!(url.starts_with("https://sign-in.example/auth?callback_url=http%3A%2F%2Flocalhost%3A5000%2Fcallback&code_challenge="));
        assert!(url.contains("&code_challenge_method=S256&state="));
        assert!(!url.contains(&a.verifier), "the verifier never leaves this Mac before the exchange");
    }

    #[test]
    fn callback_parsing() {
        let ok = "GET /callback?code=abc123&state=s1 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(parse_callback(ok, "s1"), Callback::Code("abc123".into()));
        // Without our state a code is ignored (and doesn't end the sign-in)
        assert_eq!(parse_callback("GET /callback?code=abc&state=other HTTP/1.1", "s1"), Callback::Other);
        assert_eq!(parse_callback("GET /callback?code=abc HTTP/1.1", "s1"), Callback::Other);
        assert_eq!(parse_callback("GET /callback HTTP/1.1", "s1"), Callback::Other);
        assert_eq!(parse_callback("GET /callback?error=access_denied HTTP/1.1", "s1"), Callback::Denied);
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1", "s1"), Callback::Other);
        assert_eq!(parse_callback("", "s1"), Callback::Other);
        let long = format!("GET /callback?code={}&state=s1 HTTP/1.1", "x".repeat(5000));
        assert_eq!(parse_callback(&long, "s1"), Callback::Other);
    }

    #[test]
    fn clipboard_keys_are_cleaned_and_sentences_refused() {
        assert_eq!(key_from_clipboard(Some("  \"test-key-123456789012\"\n"), "platform.openai.com").unwrap(), "test-key-123456789012");
        assert_eq!(key_from_clipboard(Some("OPENAI_API_KEY=test-key-123456789012"), "x").unwrap(), "test-key-123456789012");
        assert_eq!(key_from_clipboard(Some("Bearer test-key-123456789012"), "x").unwrap(), "test-key-123456789012");
        assert_eq!(key_from_clipboard(Some("export OPENAI_API_KEY=test-key-123456789012"), "x").unwrap(), "test-key-123456789012");
        // A copied password or anything with whitespace inside is not a key
        assert!(key_from_clipboard(Some("Tr0ub4dor&3!horse#battery"), "x").is_err());
        assert!(key_from_clipboard(Some("abcdefghij\nklmnopqrst\nuvw"), "x").is_err());
        assert!(key_from_clipboard(Some("abcdefghijklmnop"), "x").is_err(), "under 20 characters");
        let e = key_from_clipboard(None, "platform.openai.com").unwrap_err();
        assert_eq!(e, "Copy your key on platform.openai.com first, then click again.");
        assert!(key_from_clipboard(Some("Here are the notes from the meeting today"), "x").is_err());
        assert!(key_from_clipboard(Some("https://platform.openai.com/api-keys"), "x").is_err());
        assert!(key_from_clipboard(Some("short"), "x").is_err());
    }

    #[test]
    fn local_model_lists() {
        let body = br#"{"object":"list","data":[{"id":"llama3.2:3b"},{"id":"nomic-embed-text"},{"id":"qwen3:8b"}]}"#;
        assert_eq!(local_models(body), vec!["llama3.2:3b", "qwen3:8b"]);
        assert!(local_models(b"not json").is_empty());
        assert!(local_models(br#"{"data":[]}"#).is_empty());
    }

    /// A stand-in for the service: answers the key exchange.
    async fn exchange_stub(status: &'static str, body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/api/v1/auth/keys", l.local_addr().unwrap().port());
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let resp = format!("HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", status, body.len(), body);
            s.write_all(resp.as_bytes()).await.unwrap();
            req
        });
        (url, h)
    }

    #[tokio::test]
    async fn sign_in_round_trip_through_the_loopback_page() {
        let (exchange_url, stub) = exchange_stub("200 OK", r#"{"key":"sk-or-v1-test-key-1234567890"}"#).await;
        let (_tx, rx) = watch::channel(false);
        // The "browser": follow the authorize URL's callback with a code and our state
        let open = |url: String| {
            let u = url::Url::parse(&url).unwrap();
            let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
            assert_eq!(q["code_challenge_method"], "S256");
            let callback = format!("{}?code=one-time-code&state={}", q["callback_url"], q["state"]);
            tokio::spawn(async move {
                let host = callback.split('/').nth(2).unwrap().replace("localhost", "127.0.0.1");
                // A silent preconnect that never sends: must not hold up the callback
                let _idle = tokio::net::TcpStream::connect(&host).await.unwrap();
                // A stray request and a forged one: the listener keeps waiting
                for stray in ["GET /favicon.ico HTTP/1.1\r\n\r\n", "GET /callback?code=evil&state=wrong HTTP/1.1\r\n\r\n"] {
                    let mut s = tokio::net::TcpStream::connect(&host).await.unwrap();
                    s.write_all(stray.as_bytes()).await.unwrap();
                    let mut sink = Vec::new();
                    let _ = s.read_to_end(&mut sink).await;
                }
                let path = &callback[callback.find("/callback").unwrap()..];
                let mut s = tokio::net::TcpStream::connect(&host).await.unwrap();
                s.write_all(format!("GET {} HTTP/1.1\r\nHost: localhost\r\n\r\n", path).as_bytes()).await.unwrap();
                let mut page = String::new();
                let _ = s.read_to_string(&mut page).await;
                assert!(page.contains("Signed in"), "{}", page);
            });
            Ok(())
        };
        let key = sign_in_flow("https://sign-in.example/auth", &exchange_url, open, rx).await.unwrap();
        assert_eq!(key, "sk-or-v1-test-key-1234567890");
        let req = stub.await.unwrap();
        assert!(req.starts_with("POST /api/v1/auth/keys"), "{}", req);
        assert!(req.contains("\"code\":\"one-time-code\""));
        assert!(req.contains("\"code_verifier\":\""));
        assert!(req.contains("\"code_challenge_method\":\"S256\""));
    }

    #[tokio::test]
    async fn sign_in_reports_a_refused_code_and_can_be_cancelled() {
        let (exchange_url, _stub) = exchange_stub("403 Forbidden", r#"{"error":"Invalid code"}"#).await;
        let pkce = Pkce::new();
        let e = exchange(&exchange_url, "stale", &pkce).await.unwrap_err();
        assert!(e.contains("didn't accept the sign-in"), "{}", e);

        let (tx, rx) = watch::channel(false);
        let flow = tokio::spawn(sign_in_flow("https://sign-in.example/auth", "https://unused.example/keys", |_| Ok(()), rx));
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(true).unwrap();
        let e = flow.await.unwrap().unwrap_err();
        assert!(e.starts_with("SIGN_IN_CANCELLED:"), "{}", e);

        // A flow whose sender just goes away (it finished elsewhere) is not cancelled
        let (tx, rx) = watch::channel(false);
        drop(tx);
        let quiet = tokio::time::timeout(Duration::from_millis(100), cancelled(rx)).await;
        assert!(quiet.is_err(), "dropping the sender is not a cancel");
    }
}
