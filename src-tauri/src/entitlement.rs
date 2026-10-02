//! noFriction Pro entitlement (Mac App Store build).
//!
//! Free: recording, transcription, calendar/people, screenshots, export.
//! Pro: every LLM call (notes, summaries, action items, emails, chat,
//! briefings, live intel, catch-up), user-invoked or background.
//!
//! [`require_pro`] is checked once, in `ai::client::complete`, which every AI
//! call goes through. The state comes from StoreKit 2
//! (`Transaction.currentEntitlements`, verified on-device; no server) via
//! `crate::store`, and is refreshed by the `Transaction.updates` listener.
//!
//! The Developer ID (DMG) build is the owner's personal build: the check is
//! compiled out there and always passes. There is no bypass flag in the MAS
//! build.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Entitlement {
    #[serde(rename = "isPro", default)]
    pub is_pro: bool,
    #[serde(rename = "productId", default)]
    pub product_id: Option<String>,
    /// ISO-8601
    #[serde(default)]
    pub expiration: Option<String>,
    #[serde(rename = "willRenew", default)]
    pub will_renew: Option<bool>,
    /// false until StoreKit has answered once this launch
    #[serde(default)]
    pub loaded: bool,
    /// Set when StoreKit returned an error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static STATE: Lazy<RwLock<Entitlement>> = Lazy::new(|| RwLock::new(Entitlement::default()));

pub fn current() -> Entitlement {
    STATE.read().clone()
}

/// Store a fresh entitlement; returns true when Pro status changed.
pub fn set(e: Entitlement) -> bool {
    let mut s = STATE.write();
    let changed = s.is_pro != e.is_pro || s.product_id != e.product_id || s.expiration != e.expiration;
    *s = Entitlement { loaded: true, ..e };
    changed
}

/// Gate for every LLM call. `Err(PRO_REQUIRED)` when the MAS build has no
/// active noFriction Pro subscription. Always `Ok` in the DMG build.
#[cfg(feature = "mas")]
pub async fn require_pro() -> Result<(), crate::ai::AiError> {
    if !STATE.read().loaded {
        // First AI call before StoreKit answered: ask now (cached on-device,
        // works offline).
        crate::store::refresh_entitlement().await;
    }
    if STATE.read().is_pro {
        Ok(())
    } else {
        Err(crate::ai::AiError::ProRequired)
    }
}

#[cfg(not(feature = "mas"))]
pub async fn require_pro() -> Result<(), crate::ai::AiError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_reports_changes() {
        assert!(set(Entitlement { is_pro: true, product_id: Some("p".into()), ..Default::default() }));
        assert!(!set(Entitlement { is_pro: true, product_id: Some("p".into()), ..Default::default() }));
        assert!(current().loaded);
        assert!(set(Entitlement::default()));
    }

    #[test]
    fn parses_bridge_json() {
        let e: Entitlement = serde_json::from_str(
            r#"{"isPro":true,"productId":"com.nofriction.meetings.pro.yearly","expiration":"2027-01-01T00:00:00Z","willRenew":true}"#,
        )
        .unwrap();
        assert!(e.is_pro);
        assert_eq!(e.will_renew, Some(true));
    }

    #[cfg(not(feature = "mas"))]
    #[test]
    fn dmg_never_gates() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert!(rt.block_on(require_pro()).is_ok());
    }
}
