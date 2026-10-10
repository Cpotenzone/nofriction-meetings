//! noFriction Pro entitlement (Mac App Store build).
//!
//! The Free/Pro list is `docs/PRO.md` (owner decision 2026-10-10).
//! Free: recording, microphone transcription, screens, marks, notebooks,
//! Rewind, search, Links, calendar/people, Delete/Strike, JSON export.
//! Pro: every LLM call (notes, follow-up email, review guides, chat, topics,
//! automatic notes), plus the non-AI Pro features in [`ProFeature`]
//! (Sync with your iPhone, Export to Obsidian).
//!
//! Two gates, one rule (gated only in the `mas` build):
//! - [`require_pro`] is the single AI gate, checked once in
//!   `ai::client::complete`, which every AI call goes through.
//! - [`require_pro_feature`] is for non-AI Pro features. Its error is
//!   `PRO_REQUIRED:<key>: …`, so the frontend opens the paywall naming that
//!   feature (`src/lib/pro.ts`).
//!
//! The state comes from StoreKit 2
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

// ---------------------------------------------------------------------------
// Non-AI Pro features
// ---------------------------------------------------------------------------

/// true in the Mac App Store build, where Pro is enforced.
pub const GATED: bool = cfg!(feature = "mas");

/// A Pro feature that isn't an AI call. The key is shared with the frontend
/// paywall (`src/lib/pro.ts` → `ProFeature`) and iOS (`ProFeature.swift`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProFeature {
    /// iPhone ↔ Mac sync over the local network
    Sync,
    /// Export to Obsidian (manual and automatic export to a vault folder)
    Obsidian,
}

impl ProFeature {
    pub const ALL: [ProFeature; 2] = [ProFeature::Sync, ProFeature::Obsidian];

    /// Stable key, in the error string and the paywall.
    pub fn key(self) -> &'static str {
        match self {
            ProFeature::Sync => "sync",
            ProFeature::Obsidian => "obsidian",
        }
    }

    /// Name as the UI says it (DESIGN.md vocabulary, sentence case).
    pub fn label(self) -> &'static str {
        match self {
            ProFeature::Sync => "Sync",
            ProFeature::Obsidian => "Export to Obsidian",
        }
    }
}

/// `Err` from [`require_pro_feature`]. Displays as
/// `PRO_REQUIRED:<key>: <Label> is part of noFriction Pro.`; the frontend
/// turns that into the paywall for `<key>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProRequired(pub ProFeature);

impl std::fmt::Display for ProRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PRO_REQUIRED:{}: {} is part of noFriction Pro.", self.0.key(), self.0.label())
    }
}

impl std::error::Error for ProRequired {}

impl From<ProRequired> for String {
    fn from(e: ProRequired) -> String {
        e.to_string()
    }
}

/// The rule, without global state: free only when this build isn't gated.
pub fn check_feature(gated: bool, is_pro: bool, feature: ProFeature) -> Result<(), ProRequired> {
    if !gated || is_pro {
        Ok(())
    } else {
        Err(ProRequired(feature))
    }
}

/// Gate for non-AI Pro features (Sync, Export to Obsidian). Same rule as
/// [`require_pro`]: enforced only in the `mas` build, always `Ok` in the DMG
/// build. AI calls keep using [`require_pro`].
#[cfg(feature = "mas")]
pub async fn require_pro_feature(feature: ProFeature) -> Result<(), ProRequired> {
    if !STATE.read().loaded {
        crate::store::refresh_entitlement().await;
    }
    check_feature(GATED, STATE.read().is_pro, feature)
}

#[cfg(not(feature = "mas"))]
pub async fn require_pro_feature(feature: ProFeature) -> Result<(), ProRequired> {
    check_feature(GATED, false, feature)
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
        for f in ProFeature::ALL {
            assert!(rt.block_on(require_pro_feature(f)).is_ok(), "{:?} gated in the DMG build", f);
        }
    }

    #[test]
    fn gated_only_in_mas() {
        assert_eq!(GATED, cfg!(feature = "mas"));
    }

    #[test]
    fn feature_rule() {
        for f in ProFeature::ALL {
            assert!(check_feature(false, false, f).is_ok(), "ungated build must pass");
            assert!(check_feature(true, true, f).is_ok(), "Pro must pass");
            assert_eq!(check_feature(true, false, f), Err(ProRequired(f)));
        }
    }

    #[test]
    fn feature_error_names_the_feature() {
        // The frontend parses PRO_REQUIRED:<key>: (src/lib/pro.ts)
        assert_eq!(
            ProRequired(ProFeature::Obsidian).to_string(),
            "PRO_REQUIRED:obsidian: Export to Obsidian is part of noFriction Pro."
        );
        assert_eq!(
            String::from(ProRequired(ProFeature::Sync)),
            "PRO_REQUIRED:sync: Sync is part of noFriction Pro."
        );
        for f in ProFeature::ALL {
            let k = f.key();
            assert!(!k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", k);
        }
    }
}
