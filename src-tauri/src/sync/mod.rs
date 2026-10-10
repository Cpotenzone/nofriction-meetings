//! Sync with your iPhone: device to device on the local network, no server
//! (docs/SYNC.md). A noFriction Pro feature.
//!
//! The Mac advertises `_nofriction._tcp` with Bonjour and runs a TLS
//! listener only while sync is on and Pro is active
//! (`entitlement::require_pro_feature(ProFeature::Sync)`, always Ok in the
//! DMG build). The only place that starts them is [`start`], called by the
//! Sync settings and, at launch, by [`start_if_enabled`].

pub mod bonjour;
pub mod merge;
pub mod pairing;
pub mod protocol;
pub mod server;
pub mod store;

#[cfg(test)]
mod tests;

use crate::entitlement::{require_pro_feature, ProFeature};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub const CHANGED_EVENT: &str = "sync-changed";

struct Running {
    port: u16,
    shutdown: tokio::sync::watch::Sender<bool>,
    server: Arc<server::Server>,
    _bonjour: Option<bonjour::Registration>,
}

static RUNNING: Lazy<Mutex<Option<Running>>> = Lazy::new(|| Mutex::new(None));

pub fn pro_check() -> server::ProCheck {
    Arc::new(|| Box::pin(async { require_pro_feature(ProFeature::Sync).await.is_ok() }))
}

/// Start the listener and Bonjour (no-op if running). Requires Pro.
pub async fn start(
    pool: &Pool<Sqlite>,
    env: server::EnvFn,
    notify: Option<server::Notify>,
) -> Result<u16, String> {
    require_pro_feature(ProFeature::Sync).await?;
    if let Some(r) = RUNNING.lock().as_ref() {
        return Ok(r.port);
    }
    let identity = pairing::Identity::load_or_create()?;
    let mac_id = store::device_id(pool).await?;
    let listener = match tokio::net::TcpListener::bind("[::]:0").await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind("0.0.0.0:0").await.map_err(|e| format!("Couldn't start sync: {}", e))?,
    };
    let port = listener.local_addr().map_err(|e| format!("Couldn't start sync: {}", e))?.port();
    let srv = Arc::new(server::Server {
        pool: pool.clone(),
        identity,
        mac_id: mac_id.clone(),
        mac_name: pairing::computer_name(),
        pairing: Mutex::new(None),
        pro: pro_check(),
        env,
        notify,
    });
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(srv.clone().serve(listener, rx));
    let bonjour = match bonjour::register(port, &mac_id) {
        Ok(r) => Some(r),
        Err(e) => {
            log::warn!("Sync: {}", e);
            None
        }
    };
    let mut slot = RUNNING.lock();
    if let Some(r) = slot.as_ref() {
        // Another start won the race
        let _ = tx.send(true);
        return Ok(r.port);
    }
    *slot = Some(Running { port, shutdown: tx.clone(), server: srv, _bonjour: bonjour });
    drop(slot);
    log::info!("Sync: listening on the local network");
    // Stop advertising if Pro lapses
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            if *tx.borrow() || RUNNING.lock().is_none() {
                break;
            }
            if require_pro_feature(ProFeature::Sync).await.is_err() {
                log::info!("Sync: Pro isn't active; stopped listening");
                stop();
                break;
            }
        }
    });
    Ok(port)
}

pub fn stop() {
    if let Some(r) = RUNNING.lock().take() {
        let _ = r.shutdown.send(true);
        log::info!("Sync: stopped");
    }
}

pub fn is_running() -> bool {
    RUNNING.lock().is_some()
}

/// At launch: start only if the user turned sync on and Pro is active.
pub fn start_if_enabled(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let pool = state.database.pool().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if store::kv_get(&pool, "enabled").await.as_deref() != Some("1") {
            return;
        }
        if let Err(e) = start(&pool, env_fn(&app), Some(notify_fn(&app))).await {
            log::info!("Sync not started: {}", e);
        }
    });
}

fn env_fn(app: &tauri::AppHandle) -> server::EnvFn {
    let app = app.clone();
    Arc::new(move || crate::redaction::commands::env_from(&app))
}

fn notify_fn(app: &tauri::AppHandle) -> server::Notify {
    use tauri::Emitter;
    let app = app.clone();
    Arc::new(move || {
        let _ = app.emit(CHANGED_EVENT, ());
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub enabled: bool,
    pub running: bool,
    /// Pro is active (always true in the DMG build)
    pub pro: bool,
    pub devices: Vec<store::Device>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingInfo {
    /// The pairing link (also encoded in the QR code)
    pub link: String,
    pub qr_svg: String,
    pub expires_in_secs: u64,
}

pub async fn status(pool: &Pool<Sqlite>) -> Result<SyncStatus, String> {
    let devices = store::list_devices(pool).await?;
    let last_synced_at = devices.iter().filter_map(|d| d.last_sync_at.clone()).max();
    Ok(SyncStatus {
        enabled: store::kv_get(pool, "enabled").await.as_deref() == Some("1"),
        running: is_running(),
        pro: require_pro_feature(ProFeature::Sync).await.is_ok(),
        devices,
        last_synced_at,
    })
}

pub mod commands {
    use super::*;
    use crate::AppState;
    use tauri::{AppHandle, State};

    #[tauri::command]
    pub async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
        status(state.database.pool()).await
    }

    #[tauri::command]
    pub async fn sync_set_enabled(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<SyncStatus, String> {
        let pool = state.database.pool();
        if enabled {
            require_pro_feature(ProFeature::Sync).await?;
            start(pool, env_fn(&app), Some(notify_fn(&app))).await?;
            store::kv_set(pool, "enabled", "1").await?;
        } else {
            store::kv_set(pool, "enabled", "0").await?;
            stop();
        }
        status(pool).await
    }

    /// Show a pairing code (QR + link) valid for 5 minutes, one use.
    #[tauri::command]
    pub async fn sync_pair_start(app: AppHandle, state: State<'_, AppState>) -> Result<PairingInfo, String> {
        require_pro_feature(ProFeature::Sync).await?;
        let pool = state.database.pool();
        let port = start(pool, env_fn(&app), Some(notify_fn(&app))).await?;
        store::kv_set(pool, "enabled", "1").await?;
        let (srv, code) = {
            let guard = RUNNING.lock();
            let r = guard.as_ref().ok_or("Sync isn't running")?;
            let pending = pairing::PendingPair::new();
            let code = pending.code.clone();
            *r.server.pairing.lock() = Some(pending);
            (r.server.clone(), code)
        };
        let link = pairing::pairing_link(
            &srv.mac_id,
            &srv.mac_name,
            &srv.identity.fingerprint(),
            &pairing::lan_addresses(),
            port,
            &code,
        );
        Ok(PairingInfo { qr_svg: pairing::qr_svg(&link)?, link, expires_in_secs: pairing::CODE_TTL.as_secs() })
    }

    #[tauri::command]
    pub async fn sync_pair_cancel() -> Result<(), String> {
        if let Some(r) = RUNNING.lock().as_ref() {
            *r.server.pairing.lock() = None;
        }
        Ok(())
    }

    /// Forget a device: its secret leaves the Keychain; it must pair again.
    /// Allowed without Pro.
    #[tauri::command]
    pub async fn sync_forget(state: State<'_, AppState>, device_id: String) -> Result<SyncStatus, String> {
        let pool = state.database.pool();
        pairing::delete_secret(&device_id)?;
        store::remove_device(pool, &device_id).await?;
        status(pool).await
    }
}
