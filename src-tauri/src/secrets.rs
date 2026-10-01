//! Secret storage: macOS Keychain (generic passwords; login keychain in the
//! DMG build, data-protection keychain in the Mac App Store build).
//!
//! Nothing secret lives in SQLite or `.env` any more. Two services:
//! - `com.nofriction.meetings.ai`      — AI provider keys, account = provider id
//! - `com.nofriction.meetings.secrets` — everything else (transcription
//!   keys), account = the old settings key name
//!
//! Values are never logged and never returned to the frontend; the UI only
//! ever sees `{configured, last4}` (see [`status`]).
//!
//! The store is injectable ([`set_store`]) so tests never touch the real
//! keychain.

use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const AI_SERVICE: &str = "com.nofriction.meetings.ai";
pub const SECRETS_SERVICE: &str = "com.nofriction.meetings.secrets";

/// Settings keys whose values are secrets. `SettingsManager::get/set/delete`
/// transparently route these to the Keychain (service `SECRETS_SERVICE`,
/// account = the key itself).
pub const SECRET_SETTING_KEYS: &[&str] = &[
    "deepgram_api_key",
    "gemini_api_key",
    "gladia_api_key",
    "google_stt_key_json",
];

/// Secrets of removed server-side integrations (Supabase, Pinecone, the
/// ingest pipeline; the app is client-only now). Deleted from the Keychain
/// and from plaintext settings on startup; never read.
pub const REMOVED_SERVICE_SECRETS: &[&str] = &[
    "pinecone_api_key",
    "supabase_connection_string",
    "ingest_bearer_token",
];

/// Non-secret settings rows of the same removed integrations.
pub const REMOVED_SERVICE_SETTINGS: &[&str] = &[
    "pinecone_index_host",
    "pinecone_namespace",
    "enable_ingest",
    "ingest_base_url",
];

/// Plaintext settings rows from removed features (Castle Chat, TheBrain login,
/// the never-wired "remote AI" settings, remote VLM bearer token). They are
/// deleted on startup and not migrated: nothing reads them any more.
pub const LEGACY_PURGE_KEYS: &[&str] = &[
    "vlm_bearer_token",
    "ai_remote_key",
    "ai_remote_url",
    "ai_provider",
    "castle_api_key",
    "castle_enabled",
    "castle_url",
    "castle_model",
    "thebrain_token",
    "thebrain_username",
];

/// `.env` variables that map onto a secret setting key (import, then remove).
const ENV_SECRET_MAP: &[(&str, &str)] = &[
    ("DEEPGRAM_API_KEY", "deepgram_api_key"),
    ("GEMINI_API_KEY", "gemini_api_key"),
    ("GLADIA_API_KEY", "gladia_api_key"),
];

/// `.env` secrets for removed features: stripped from the file, not imported.
const ENV_PURGE: &[&str] = &[
    "THEBRAIN_PASSWORD",
    "THEBRAIN_EMAIL",
    "REMOTE_INTELLIGENCE_TOKEN",
    "REMOTE_INTELLIGENCE_URL",
    "REMOTE_INTELLIGENCE_ENABLED",
    "CASTLE_CHAT_API_KEY",
    "VLM_BEARER_TOKEN",
    "PINECONE_API_KEY",
    "SUPABASE_CONNECTION_STRING",
    "INGEST_BEARER_TOKEN",
];

pub fn is_secret_setting(key: &str) -> bool {
    SECRET_SETTING_KEYS.contains(&key)
}

/// Heuristic guard for the generic `set_setting` command: refuse to write
/// anything that looks like a credential into plaintext settings.
pub fn looks_secret(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["key", "token", "password", "secret", "credential", "connection_string"]
        .iter()
        .any(|w| k.contains(w))
}

// ---------------------------------------------------------------------------
// Store abstraction
// ---------------------------------------------------------------------------

pub trait SecretStore: Send + Sync {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

/// The real store: macOS Keychain, generic password items.
///
/// - Developer ID (DMG) build: the file-based login keychain.
/// - Mac App Store build (`mas`): the data-protection keychain (Apple's
///   recommendation for sandboxed apps: items are scoped to the app's
///   `application-identifier`, no ACL prompts, and no clash with the DMG
///   build's items on a Mac that has both). The data-protection keychain
///   needs the `com.apple.application-identifier` entitlement, which only a
///   provisioning profile grants; a profile-less local test build gets
///   errSecMissingEntitlement and falls back to the login keychain.
pub struct KeychainStore;

#[cfg(target_os = "macos")]
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;

#[cfg(all(target_os = "macos", feature = "mas"))]
static DP_UNAVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "macos")]
fn use_data_protection() -> bool {
    #[cfg(feature = "mas")]
    {
        !DP_UNAVAILABLE.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(feature = "mas"))]
    {
        false
    }
}

/// Run `f` against the data-protection keychain (MAS) and, if the build
/// lacks the entitlement for it, against the login keychain from then on.
#[cfg(target_os = "macos")]
fn with_keychain<T>(
    f: impl Fn(Option<security_framework::passwords::PasswordOptions>) -> Result<T, security_framework::base::Error>,
    service: &str,
    account: &str,
) -> Result<T, security_framework::base::Error> {
    if use_data_protection() {
        let mut o = security_framework::passwords::PasswordOptions::new_generic_password(service, account);
        o.use_protected_keychain();
        match f(Some(o)) {
            Err(e) if e.code() == ERR_SEC_MISSING_ENTITLEMENT => {
                #[cfg(feature = "mas")]
                DP_UNAVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
                log::warn!(
                    "🔐 Data-protection keychain unavailable (no application-identifier entitlement; \
                     local test build without a provisioning profile). Using the login keychain."
                );
            }
            other => return other,
        }
    }
    f(None)
}

#[cfg(target_os = "macos")]
impl SecretStore for KeychainStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        use security_framework::passwords::{generic_password, get_generic_password};
        let r = with_keychain(
            |o| match o {
                Some(o) => generic_password(o),
                None => get_generic_password(service, account),
            },
            service,
            account,
        );
        match r {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "Keychain item is not valid UTF-8".to_string()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(format!("Keychain read failed (OSStatus {})", e.code())),
        }
    }

    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        use security_framework::passwords::{set_generic_password, set_generic_password_options};
        with_keychain(
            |o| match o {
                Some(o) => set_generic_password_options(value.as_bytes(), o),
                None => set_generic_password(service, account, value.as_bytes()),
            },
            service,
            account,
        )
        .map_err(|e| format!("Keychain write failed (OSStatus {})", e.code()))
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        use security_framework::passwords::{delete_generic_password, delete_generic_password_options};
        let r = with_keychain(
            |o| match o {
                Some(o) => delete_generic_password_options(o),
                None => delete_generic_password(service, account),
            },
            service,
            account,
        );
        match r {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("Keychain delete failed (OSStatus {})", e.code())),
        }
    }
}

#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(not(target_os = "macos"))]
impl SecretStore for KeychainStore {
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, String> {
        Err("Keychain is only available on macOS".into())
    }
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Err("Keychain is only available on macOS".into())
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), String> {
        Err("Keychain is only available on macOS".into())
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<(String, String), String>>,
    pub fail_writes: bool,
}

impl MemoryStore {
    pub fn failing() -> Self {
        Self { items: Mutex::default(), fail_writes: true }
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        Ok(self.items.lock().get(&(service.to_string(), account.to_string())).cloned())
    }
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        if self.fail_writes {
            return Err("write refused".into());
        }
        self.items
            .lock()
            .insert((service.to_string(), account.to_string()), value.to_string());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        self.items.lock().remove(&(service.to_string(), account.to_string()));
        Ok(())
    }
}

static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();
/// Read-through cache so hot paths (settings reads, AI calls) don't hit the
/// keychain every time. `None` = known absent.
static CACHE: Lazy<RwLock<HashMap<(String, String), Option<String>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Install a store (tests / alternative backends). First call wins.
pub fn set_store(store: Box<dyn SecretStore>) {
    let _ = STORE.set(store);
}

fn store() -> &'static dyn SecretStore {
    STORE
        .get_or_init(|| {
            if cfg!(test) {
                Box::new(MemoryStore::default())
            } else {
                Box::new(KeychainStore)
            }
        })
        .as_ref()
}

fn cache_key(service: &str, account: &str) -> (String, String) {
    (service.to_string(), account.to_string())
}

/// Read a secret. Errors are logged (without the value) and treated as absent.
pub fn get(service: &str, account: &str) -> Option<String> {
    if let Some(v) = CACHE.read().get(&cache_key(service, account)) {
        return v.clone();
    }
    match store().get(service, account) {
        Ok(v) => {
            let v = v.filter(|s| !s.is_empty());
            CACHE.write().insert(cache_key(service, account), v.clone());
            v
        }
        Err(e) => {
            log::warn!("🔐 {} / {}: {}", service, account, e);
            None
        }
    }
}

/// Store a secret. An empty value deletes it.
pub fn set(service: &str, account: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return delete(service, account);
    }
    store().set(service, account, value)?;
    CACHE
        .write()
        .insert(cache_key(service, account), Some(value.to_string()));
    log::info!("🔐 Saved {} to Keychain ({})", account, service);
    Ok(())
}

pub fn delete(service: &str, account: &str) -> Result<(), String> {
    store().delete(service, account)?;
    CACHE.write().insert(cache_key(service, account), None);
    Ok(())
}

/// What the UI may know about a secret.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq)]
pub struct SecretStatus {
    pub configured: bool,
    pub last4: Option<String>,
}

pub fn status_of(value: Option<&str>) -> SecretStatus {
    match value.filter(|v| !v.is_empty()) {
        Some(v) => {
            let chars: Vec<char> = v.chars().collect();
            // Very short secrets: don't reveal any of it
            let last4 = if chars.len() >= 12 {
                Some(chars[chars.len() - 4..].iter().collect())
            } else {
                None
            };
            SecretStatus { configured: true, last4 }
        }
        None => SecretStatus { configured: false, last4: None },
    }
}

pub fn status(service: &str, account: &str) -> SecretStatus {
    status_of(get(service, account).as_deref())
}

/// `••••1234` for display, never more.
pub fn masked(value: Option<&str>) -> Option<String> {
    let s = status_of(value);
    if !s.configured {
        return None;
    }
    Some(match s.last4 {
        Some(l) => format!("••••{}", l),
        None => "••••".to_string(),
    })
}

// ---------------------------------------------------------------------------
// One-time migration of plaintext secrets
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
pub struct MigrationPlan {
    /// Setting keys whose value is now in the Keychain
    pub imported: Vec<String>,
    /// Settings rows to delete (migrated, empty, duplicate or legacy)
    pub delete_rows: Vec<String>,
    /// Keys we could not store; the plaintext row is kept so nothing is lost
    pub failed: Vec<String>,
}

/// Decide what to do with plaintext settings rows. Writes secrets into
/// `store`; returns which rows the caller must delete. Pure apart from the
/// store, so it is unit-testable.
pub fn plan_settings_migration(rows: &[(String, String)], store: &dyn SecretStore) -> MigrationPlan {
    let mut plan = MigrationPlan::default();
    for (key, value) in rows {
        if LEGACY_PURGE_KEYS.contains(&key.as_str()) {
            plan.delete_rows.push(key.clone());
            continue;
        }
        if !is_secret_setting(key) {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            plan.delete_rows.push(key.clone());
            continue;
        }
        match store.get(SECRETS_SERVICE, key) {
            // Keychain already has one: it wins, drop the plaintext copy
            Ok(Some(existing)) if !existing.is_empty() => {
                plan.delete_rows.push(key.clone());
                continue;
            }
            Ok(_) => {}
            Err(_) => {
                plan.failed.push(key.clone());
                continue;
            }
        }
        let stored = store
            .set(SECRETS_SERVICE, key, value)
            .and_then(|_| store.get(SECRETS_SERVICE, key));
        match stored {
            Ok(Some(back)) if back == value => {
                plan.imported.push(key.clone());
                plan.delete_rows.push(key.clone());
            }
            _ => plan.failed.push(key.clone()),
        }
    }
    plan
}

#[derive(Debug, Default, PartialEq)]
pub struct EnvMigration {
    pub imported: Vec<String>,
    /// File content with secret lines removed
    pub remaining: String,
    /// Whether the file needs rewriting
    pub changed: bool,
    /// True when nothing but comments/blank lines remain
    pub now_empty: bool,
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let t = t.strip_prefix("export ").unwrap_or(t);
    let (k, v) = t.split_once('=')?;
    let v = v.trim();
    let v = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(v);
    Some((k.trim().to_string(), v.to_string()))
}

/// Import secrets from a `.env` body into `store` and return the body with
/// those lines (and dead-feature secrets) removed. Lines whose value could
/// not be stored are kept.
pub fn plan_env_migration(content: &str, store: &dyn SecretStore) -> EnvMigration {
    let mut out = EnvMigration::default();
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        let Some((name, value)) = parse_env_line(line) else {
            kept.push(line);
            continue;
        };
        if ENV_PURGE.contains(&name.as_str()) {
            out.changed = true;
            continue;
        }
        let Some((_, setting_key)) = ENV_SECRET_MAP.iter().find(|(n, _)| *n == name) else {
            kept.push(line);
            continue;
        };
        if value.trim().is_empty() {
            out.changed = true;
            continue;
        }
        let already = matches!(store.get(SECRETS_SERVICE, setting_key), Ok(Some(ref v)) if !v.is_empty());
        let ok = already
            || (store.set(SECRETS_SERVICE, setting_key, value.trim()).is_ok()
                && matches!(store.get(SECRETS_SERVICE, setting_key), Ok(Some(ref v)) if v == value.trim()));
        if ok {
            if !already {
                out.imported.push(setting_key.to_string());
            }
            out.changed = true;
        } else {
            kept.push(line);
        }
    }
    out.now_empty = kept
        .iter()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with('#'));
    out.remaining = if kept.is_empty() { String::new() } else { kept.join("\n") + "\n" };
    out
}

/// Startup migration: SQLite plaintext → Keychain, then delete plaintext rows.
pub async fn migrate_settings(settings: &crate::settings::SettingsManager) {
    let rows = match settings.raw_rows().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("🔐 Secret migration skipped: could not read settings ({})", e);
            return;
        }
    };
    let plan = plan_settings_migration(&rows, store());
    let mut deleted = 0;
    for key in &plan.delete_rows {
        match settings.raw_delete(key).await {
            Ok(()) => deleted += 1,
            Err(e) => log::warn!("🔐 Could not delete plaintext setting {}: {}", key, e),
        }
    }
    if deleted > 0 {
        // The deleted rows' old pages are still in the WAL until a checkpoint
        if let Err(e) = settings.scrub_wal().await {
            log::warn!("🔐 Plaintext secret rows deleted, but the WAL wasn't cleared yet: {}", e);
        }
    }
    // Fresh values may have been written straight to the store
    CACHE.write().clear();
    if !plan.imported.is_empty() {
        log::info!("🔐 Moved {} secret(s) from SQLite to Keychain: {:?}", plan.imported.len(), plan.imported);
    }
    if !plan.failed.is_empty() {
        log::warn!(
            "🔐 Could not move {:?} to Keychain; plaintext kept for now, will retry next launch",
            plan.failed
        );
    }
}

/// Startup migration of `~/.nofriction-meetings/.env` (legacy desktop builds).
/// Compiled out of the Mac App Store build (m5): the sandbox has no such file.
#[cfg(not(feature = "mas"))]
pub fn migrate_home_env() {
    let Some(home) = dirs::home_dir() else { return };
    let path = home.join(".nofriction-meetings").join(".env");
    let Ok(content) = std::fs::read_to_string(&path) else { return };
    let m = plan_env_migration(&content, store());
    CACHE.write().clear();
    if !m.changed {
        return;
    }
    let result = if m.now_empty {
        std::fs::remove_file(&path)
    } else {
        std::fs::write(&path, &m.remaining)
    };
    match result {
        Ok(()) => log::info!(
            "🔐 Imported {:?} from {} into Keychain and removed plaintext secrets",
            m.imported,
            path.display()
        ),
        Err(e) => log::warn!("🔐 Could not rewrite {}: {}", path.display(), e),
    }
}

/// What the removed-integration cleanup did (names only, never values).
#[derive(Debug, Default, PartialEq)]
pub struct RemovedServicePurge {
    /// Keychain items that existed and were deleted
    pub keychain_deleted: Vec<String>,
    /// Keychain items that exist but could not be deleted (retried next launch)
    pub keychain_failed: Vec<String>,
    /// Settings rows to delete
    pub delete_rows: Vec<String>,
}

/// Plan the cleanup of Supabase/Pinecone/ingest leftovers: delete their
/// Keychain items from `store` and list their settings rows for deletion.
/// Only touches the names in [`REMOVED_SERVICE_SECRETS`] and
/// [`REMOVED_SERVICE_SETTINGS`].
pub fn plan_removed_service_purge(
    rows: &[(String, String)],
    store: &dyn SecretStore,
) -> RemovedServicePurge {
    let mut out = RemovedServicePurge::default();
    for key in REMOVED_SERVICE_SECRETS {
        match store.get(SECRETS_SERVICE, key) {
            Ok(Some(_)) => match store.delete(SECRETS_SERVICE, key) {
                Ok(()) => out.keychain_deleted.push(key.to_string()),
                Err(_) => out.keychain_failed.push(key.to_string()),
            },
            Ok(None) => {}
            Err(_) => out.keychain_failed.push(key.to_string()),
        }
    }
    for (key, _) in rows {
        if REMOVED_SERVICE_SECRETS.contains(&key.as_str())
            || REMOVED_SERVICE_SETTINGS.contains(&key.as_str())
        {
            out.delete_rows.push(key.clone());
        }
    }
    out
}

/// Startup cleanup (idempotent): remove Supabase/Pinecone/ingest secrets from
/// the Keychain and their settings rows from SQLite. Never touches meeting
/// data. Logs names only.
pub async fn purge_removed_services(settings: &crate::settings::SettingsManager) {
    let rows = match settings.raw_rows().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("🔐 Removed-integration cleanup skipped: could not read settings ({})", e);
            return;
        }
    };
    let plan = plan_removed_service_purge(&rows, store());
    for key in REMOVED_SERVICE_SECRETS {
        CACHE.write().remove(&cache_key(SECRETS_SERVICE, key));
    }
    let mut rows_deleted = Vec::new();
    for key in &plan.delete_rows {
        match settings.raw_delete(key).await {
            Ok(()) => rows_deleted.push(key.clone()),
            Err(e) => log::warn!("🔐 Could not delete setting {}: {}", key, e),
        }
    }
    if !plan.keychain_deleted.is_empty() {
        log::info!(
            "🔐 Removed unused Keychain item(s) from removed integrations: {:?}",
            plan.keychain_deleted
        );
    }
    if !plan.keychain_failed.is_empty() {
        log::warn!(
            "🔐 Could not remove Keychain item(s) {:?}; will retry next launch",
            plan.keychain_failed
        );
    }
    if !rows_deleted.is_empty() {
        if let Err(e) = settings.scrub_wal().await {
            log::warn!("🔐 Removed settings rows, but the WAL wasn't cleared yet: {}", e);
        }
        log::info!(
            "🔐 Removed unused setting(s) from removed integrations: {:?}",
            rows_deleted
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn migrates_secret_rows_and_purges_legacy() {
        let store = MemoryStore::default();
        let plan = plan_settings_migration(
            &rows(&[
                ("deepgram_api_key", "dg-1234567890abcdef"),
                ("gladia_api_key", ""),
                ("castle_api_key", "whatever"),
                ("thebrain_token", "jwt"),
                ("active_theme", "personal"),
            ]),
            &store,
        );
        assert_eq!(plan.imported, vec!["deepgram_api_key"]);
        assert!(plan.delete_rows.contains(&"deepgram_api_key".to_string()));
        assert!(plan.delete_rows.contains(&"gladia_api_key".to_string()));
        assert!(plan.delete_rows.contains(&"castle_api_key".to_string()));
        assert!(plan.delete_rows.contains(&"thebrain_token".to_string()));
        assert!(!plan.delete_rows.contains(&"active_theme".to_string()));
        assert_eq!(
            store.get(SECRETS_SERVICE, "deepgram_api_key").unwrap().as_deref(),
            Some("dg-1234567890abcdef")
        );
    }

    #[tokio::test]
    async fn migrated_plaintext_key_leaves_no_copy_in_db_or_wal() {
        let dir = std::env::temp_dir().join(format!("nf-secrets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("nofriction_meetings.db");
        let db = crate::database::DatabaseManager::new(&db_path).await.unwrap();
        db.run_migrations().await.unwrap();
        let settings = crate::settings::SettingsManager::new(db.get_pool());
        settings.init().await.unwrap();
        crate::redaction::wal_checkpoint_truncate(db.pool()).await.unwrap();
        let secret = "dg-walscrubcanary0123456789";
        sqlx::query("INSERT INTO settings (key, value) VALUES ('deepgram_api_key', ?)")
            .bind(secret)
            .execute(db.pool())
            .await
            .unwrap();

        migrate_settings(&settings).await;

        assert!(settings.raw_rows().await.unwrap().iter().all(|(k, _)| k != "deepgram_api_key"));
        for suffix in ["", "-wal"] {
            let p = std::path::PathBuf::from(format!("{}{}", db_path.display(), suffix));
            if let Ok(bytes) = std::fs::read(&p) {
                assert!(
                    !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                    "plaintext key still in {}",
                    p.display()
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keychain_value_wins_over_plaintext() {
        let store = MemoryStore::default();
        store.set(SECRETS_SERVICE, "gemini_api_key", "AIzaNEW").unwrap();
        let plan = plan_settings_migration(&rows(&[("gemini_api_key", "AIzaOLD")]), &store);
        assert!(plan.imported.is_empty());
        assert_eq!(plan.delete_rows, vec!["gemini_api_key"]);
        assert_eq!(store.get(SECRETS_SERVICE, "gemini_api_key").unwrap().as_deref(), Some("AIzaNEW"));
    }

    #[test]
    fn failed_write_keeps_plaintext() {
        let store = MemoryStore::failing();
        let plan = plan_settings_migration(&rows(&[("deepgram_api_key", "dg-abc")]), &store);
        assert_eq!(plan.failed, vec!["deepgram_api_key"]);
        assert!(plan.delete_rows.is_empty());
    }

    #[test]
    fn env_migration_strips_secrets_and_keeps_rest() {
        let store = MemoryStore::default();
        let env = "# config\nDEEPGRAM_API_KEY=\"dg-secret-value\"\nLOG_LEVEL=debug\nexport THEBRAIN_PASSWORD=hunter2\nPINECONE_API_KEY=pc-old\n";
        let m = plan_env_migration(env, &store);
        assert!(m.changed);
        assert_eq!(m.imported, vec!["deepgram_api_key"]);
        assert!(!m.remaining.contains("dg-secret-value"));
        assert!(!m.remaining.contains("hunter2"));
        assert!(!m.remaining.contains("pc-old"));
        assert!(m.remaining.contains("LOG_LEVEL=debug"));
        assert!(!m.now_empty);
        assert_eq!(
            store.get(SECRETS_SERVICE, "deepgram_api_key").unwrap().as_deref(),
            Some("dg-secret-value")
        );
    }

    #[test]
    fn env_with_only_secrets_becomes_empty() {
        let store = MemoryStore::default();
        let m = plan_env_migration("# hi\nGEMINI_API_KEY=AIzaXYZ\n", &store);
        assert!(m.now_empty);
    }

    #[test]
    fn env_failed_write_keeps_line() {
        let store = MemoryStore::failing();
        let m = plan_env_migration("GEMINI_API_KEY=AIzaXYZ\n", &store);
        assert!(m.remaining.contains("AIzaXYZ"));
        assert!(!m.now_empty);
    }

    #[test]
    fn status_never_reveals_more_than_last4() {
        let s = status_of(Some("sk-proj-abcdefghijklmnop1234"));
        assert_eq!(s, SecretStatus { configured: true, last4: Some("1234".into()) });
        assert_eq!(status_of(Some("short")).last4, None);
        assert!(!status_of(Some("")).configured);
        assert_eq!(masked(Some("sk-proj-abcdefghijklmnop1234")).unwrap(), "••••1234");
    }

    #[test]
    fn looks_secret_guard() {
        assert!(looks_secret("openai_api_key"));
        assert!(looks_secret("gladia_api_key"));
        assert!(!looks_secret("active_theme"));
    }

    #[test]
    fn purges_removed_service_secrets_and_rows_only() {
        let store = MemoryStore::default();
        store.set(SECRETS_SERVICE, "pinecone_api_key", "pc-123").unwrap();
        store.set(SECRETS_SERVICE, "ingest_bearer_token", "tok").unwrap();
        store.set(SECRETS_SERVICE, "deepgram_api_key", "dg-keep").unwrap();
        let plan = plan_removed_service_purge(
            &rows(&[
                ("pinecone_index_host", "idx.pinecone.io"),
                ("enable_ingest", "true"),
                ("supabase_connection_string", ""),
                ("active_theme", "personal"),
            ]),
            &store,
        );
        assert_eq!(plan.keychain_deleted, vec!["pinecone_api_key", "ingest_bearer_token"]);
        assert!(plan.keychain_failed.is_empty());
        assert_eq!(
            plan.delete_rows,
            vec!["pinecone_index_host", "enable_ingest", "supabase_connection_string"]
        );
        assert_eq!(store.get(SECRETS_SERVICE, "pinecone_api_key").unwrap(), None);
        assert_eq!(
            store.get(SECRETS_SERVICE, "deepgram_api_key").unwrap().as_deref(),
            Some("dg-keep")
        );
        // Idempotent
        let again = plan_removed_service_purge(&[], &store);
        assert!(again.keychain_deleted.is_empty());
    }
}
