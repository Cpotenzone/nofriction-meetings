//! Rust side of the Swift bridge (swift/NoFrictionBridge, built by build.rs).
//!
//! - Apple on-device model (Foundation Models): both flavors.
//! - StoreKit 2 (products, purchase, entitlement, restore, Transaction.updates
//!   listener): Mac App Store build only (`--features mas`). The DMG build
//!   keeps the commands so the frontend can call them, but they report
//!   `STORE_UNAVAILABLE`.
//!
//! The bridge is a plain C ABI: async calls take `(ctx, callback)` and call
//! back once with a JSON object; the JSON pointer is only valid during the
//! callback.

use serde_json::Value;
#[cfg(target_os = "macos")]
use std::os::raw::{c_char, c_void};

/// Frontend event with the fresh entitlement whenever it changes.
pub const SUBSCRIPTION_EVENT: &str = "subscription-changed";
/// Apple's page for managing App Store subscriptions (macOS has no
/// in-app manage-subscriptions sheet).
pub const MANAGE_SUBSCRIPTIONS_URL: &str = "https://apps.apple.com/account/subscriptions";
pub const PRO_PRODUCT_IDS: &[&str] = &[
    "com.nofriction.meetings.pro.monthly",
    "com.nofriction.meetings.pro.yearly",
];

#[cfg(target_os = "macos")]
type Callback = extern "C" fn(*mut c_void, *const c_char);

#[cfg(target_os = "macos")]
#[cfg_attr(test, allow(dead_code))] // tests never call the real bridge
extern "C" {
    fn nf_free(p: *mut c_char);
    fn nf_apple_model_availability() -> *mut c_char;
    fn nf_apple_generate(
        instructions: *const c_char,
        prompt: *const c_char,
        max_tokens: i32,
        temperature: f64,
        ctx: *mut c_void,
        cb: Callback,
    );
}

#[cfg(all(target_os = "macos", feature = "mas"))]
extern "C" {
    fn nf_store_products(ids_json: *const c_char, ctx: *mut c_void, cb: Callback);
    fn nf_store_purchase(product_id: *const c_char, ctx: *mut c_void, cb: Callback);
    fn nf_store_entitlement(ctx: *mut c_void, cb: Callback);
    fn nf_store_restore(ctx: *mut c_void, cb: Callback);
    fn nf_store_start_listener(cb: Callback);
}

// ---------------------------------------------------------------------------
// Callback plumbing
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
type Reply = tokio::sync::oneshot::Sender<String>;

#[cfg(target_os = "macos")]
extern "C" fn reply_cb(ctx: *mut c_void, json: *const c_char) {
    if ctx.is_null() {
        return;
    }
    // Called exactly once per request: take ownership back
    let tx: Box<Reply> = unsafe { Box::from_raw(ctx as *mut Reply) };
    let s = if json.is_null() {
        r#"{"ok":false,"error":"empty reply"}"#.to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(json) }.to_string_lossy().into_owned()
    };
    let _ = tx.send(s);
}

/// Run one bridge call and wait for its JSON reply.
#[cfg(target_os = "macos")]
async fn call(f: impl FnOnce(*mut c_void, Callback)) -> Result<Value, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
    f(ctx, reply_cb);
    let raw = rx.await.map_err(|_| "bridge dropped the request".to_string())?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("bad bridge reply: {}", e))?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v)
    } else {
        Err(v["error"].as_str().unwrap_or("unknown error").to_string())
    }
}

#[cfg(target_os = "macos")]
fn cstring(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Apple on-device model
// ---------------------------------------------------------------------------

/// (available, reason). Cheap; cached for a few seconds because the AI
/// settings screen asks per provider row.
pub fn apple_model_availability() -> (bool, String) {
    #[cfg(any(test, not(target_os = "macos")))]
    {
        // Tests must not depend on whether this Mac has Apple Intelligence
        (false, "unavailable_in_tests".to_string())
    }
    #[cfg(all(not(test), target_os = "macos"))]
    {
        use once_cell::sync::Lazy;
        use parking_lot::Mutex;
        use std::time::{Duration, Instant};
        static CACHE: Lazy<Mutex<Option<(Instant, bool, String)>>> = Lazy::new(|| Mutex::new(None));
        if let Some((at, ok, reason)) = CACHE.lock().clone() {
            if at.elapsed() < Duration::from_secs(10) {
                return (ok, reason);
            }
        }
        let (ok, reason) = unsafe {
            let p = nf_apple_model_availability();
            if p.is_null() {
                (false, "bridge_error".to_string())
            } else {
                let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
                nf_free(p);
                let v: Value = serde_json::from_str(&s).unwrap_or_default();
                (
                    v["available"].as_bool().unwrap_or(false),
                    v["reason"].as_str().unwrap_or("").to_string(),
                )
            }
        };
        *CACHE.lock() = Some((Instant::now(), ok, reason.clone()));
        (ok, reason)
    }
}

pub fn apple_model_available() -> bool {
    apple_model_availability().0
}

/// One generation on the on-device model.
pub async fn apple_generate(
    instructions: &str,
    prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let (i, p) = (cstring(instructions), cstring(prompt));
        let temp = temperature.map(|t| t as f64).unwrap_or(-1.0);
        let v = call(|ctx, cb| unsafe {
            nf_apple_generate(i.as_ptr(), p.as_ptr(), max_tokens.min(i32::MAX as u32) as i32, temp, ctx, cb)
        })
        .await?;
        Ok(v["text"].as_str().unwrap_or("").to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (instructions, prompt, max_tokens, temperature);
        Err("Apple on-device model is macOS-only".into())
    }
}

// ---------------------------------------------------------------------------
// StoreKit (Mac App Store build)
// ---------------------------------------------------------------------------

#[cfg(feature = "mas")]
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Apply an entitlement JSON from the bridge and tell the UI if it changed.
#[cfg(feature = "mas")]
fn apply_entitlement(v: &Value) -> crate::entitlement::Entitlement {
    use tauri::Emitter;
    let e: crate::entitlement::Entitlement = serde_json::from_value(v.clone()).unwrap_or_default();
    let first = !crate::entitlement::current().loaded;
    let changed = crate::entitlement::set(e);
    let now = crate::entitlement::current();
    if changed || first {
        log::info!("💳 Pro entitlement: is_pro={} product={:?}", now.is_pro, now.product_id);
    }
    if let Some(app) = APP.get() {
        let _ = app.emit(SUBSCRIPTION_EVENT, &now);
    }
    now
}

#[cfg(feature = "mas")]
extern "C" fn listener_cb(_ctx: *mut c_void, json: *const c_char) {
    if json.is_null() {
        return;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(json) }.to_string_lossy().into_owned();
    if let Ok(v) = serde_json::from_str::<Value>(&s) {
        if v["ok"].as_bool() == Some(true) {
            apply_entitlement(&v);
        }
    }
}

/// Launch: start the `Transaction.updates` listener and load the current
/// entitlement. No-op in the DMG build.
pub fn start(app: tauri::AppHandle) {
    #[cfg(feature = "mas")]
    {
        let _ = APP.set(app);
        unsafe { nf_store_start_listener(listener_cb) };
        tauri::async_runtime::spawn(refresh_entitlement());
    }
    #[cfg(not(feature = "mas"))]
    {
        let _ = app;
    }
}

/// Re-read `Transaction.currentEntitlements` (cached on-device; works offline).
#[cfg(feature = "mas")]
pub async fn refresh_entitlement() -> crate::entitlement::Entitlement {
    match call(|ctx, cb| unsafe { nf_store_entitlement(ctx, cb) }).await {
        Ok(v) => apply_entitlement(&v),
        Err(e) => {
            log::warn!("💳 StoreKit entitlement check failed: {}", e);
            // Mark loaded so AI calls don't wait on StoreKit every time
            crate::entitlement::set(crate::entitlement::Entitlement {
                error: Some(e),
                ..crate::entitlement::current()
            });
            crate::entitlement::current()
        }
    }
}

#[cfg(not(feature = "mas"))]
const UNAVAILABLE: &str = "STORE_UNAVAILABLE: In-app purchases are only available in the Mac App Store build.";

/// `{products:[{id, displayName, description, displayPrice, period,
/// periodUnit, periodValue, introOffer?}]}`
#[tauri::command]
pub async fn store_products() -> Result<Value, String> {
    #[cfg(feature = "mas")]
    {
        let ids = cstring(&serde_json::to_string(PRO_PRODUCT_IDS).unwrap_or_default());
        let v = call(|ctx, cb| unsafe { nf_store_products(ids.as_ptr(), ctx, cb) }).await?;
        Ok(serde_json::json!({ "products": v["products"].clone() }))
    }
    #[cfg(not(feature = "mas"))]
    Err(UNAVAILABLE.into())
}

/// Buy; `{status: purchased|cancelled|pending, entitlement}`.
#[tauri::command(rename_all = "camelCase")]
pub async fn store_purchase(product_id: String) -> Result<Value, String> {
    #[cfg(feature = "mas")]
    {
        if !PRO_PRODUCT_IDS.contains(&product_id.as_str()) {
            return Err("Unknown product".into());
        }
        let id = cstring(&product_id);
        let v = call(|ctx, cb| unsafe { nf_store_purchase(id.as_ptr(), ctx, cb) }).await?;
        let status = v["status"].as_str().unwrap_or("unknown").to_string();
        let entitlement = if status == "purchased" {
            apply_entitlement(&v)
        } else {
            crate::entitlement::current()
        };
        log::info!("💳 Purchase {}: {}", product_id, status);
        Ok(serde_json::json!({ "status": status, "entitlement": entitlement }))
    }
    #[cfg(not(feature = "mas"))]
    {
        let _ = product_id;
        Err(UNAVAILABLE.into())
    }
}

/// Current entitlement. In the DMG build: always Pro (no gating).
#[tauri::command]
pub async fn store_entitlement() -> Result<crate::entitlement::Entitlement, String> {
    #[cfg(feature = "mas")]
    {
        Ok(refresh_entitlement().await)
    }
    #[cfg(not(feature = "mas"))]
    Ok(crate::entitlement::Entitlement { is_pro: true, loaded: true, ..Default::default() })
}

/// Restore Purchases (`AppStore.sync()`).
#[tauri::command]
pub async fn store_restore() -> Result<crate::entitlement::Entitlement, String> {
    #[cfg(feature = "mas")]
    {
        let v = call(|ctx, cb| unsafe { nf_store_restore(ctx, cb) }).await?;
        Ok(apply_entitlement(&v))
    }
    #[cfg(not(feature = "mas"))]
    Err(UNAVAILABLE.into())
}

/// Open Apple's subscription management page.
#[tauri::command]
pub async fn store_manage_subscriptions(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(MANAGE_SUBSCRIPTIONS_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Real on-device generation through the Swift bridge. Needs macOS 26 with
    /// Apple Intelligence on: `cargo test --lib store::tests -- --ignored`
    #[test]
    #[ignore]
    fn apple_generate_smoke() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let out = rt.block_on(super::apple_generate(
            "Answer in one word.",
            "What color is the sky on a clear day?",
            32,
            Some(0.0),
        ));
        println!("apple_generate → {:?}", out);
        assert!(out.map(|t| !t.trim().is_empty()).unwrap_or(false));
    }
}
