//! Build flavor and what it can do. The UI asks once at startup
//! (`get_build_capabilities`) and hides features this build doesn't have.
//!
//! - `dmg` (default): Developer ID, not sandboxed, owner's daily build. No
//!   Pro gating, video recording, AX capture and owner-infra settings on.
//! - `mas` (`--features mas`): Mac App Store, App Sandbox. See
//!   docs/MAC_APP_STORE_BUILD.md.

use serde::Serialize;

#[cfg(feature = "mas")]
pub const FLAVOR: &str = "mas";
#[cfg(not(feature = "mas"))]
pub const FLAVOR: &str = "dmg";

pub const IS_MAS: bool = cfg!(feature = "mas");
/// m1: ffmpeg screen video + frame extraction
pub const VIDEO_RECORDING: bool = !IS_MAS;
/// m2: Accessibility (AX) text capture from other apps
pub const ACCESSIBILITY_CAPTURE: bool = !IS_MAS;
/// m14: admin console / dev tools (hidden in the App Store build)
pub const OWNER_INFRA: bool = !IS_MAS;

#[derive(Debug, Clone, Serialize)]
pub struct BuildCapabilities {
    /// "mas" | "dmg"
    pub flavor: &'static str,
    pub sandboxed: bool,
    pub video_recording: bool,
    pub accessibility_capture: bool,
    pub owner_infra: bool,
    /// StoreKit purchases available (MAS build)
    pub storekit: bool,
    /// AI features need noFriction Pro (MAS build only)
    pub pro_gating: bool,
    /// Apple on-device model (Foundation Models) usable right now
    pub apple_intelligence: bool,
    /// Why the Apple model isn't usable (empty when it is)
    pub apple_intelligence_reason: String,
    pub version: &'static str,
    /// Build number (src-tauri/build_number.txt, bumped by `npm run build`)
    pub build: String,
}

/// Build number baked in at compile time.
pub fn build_number() -> String {
    include_str!("../build_number.txt").trim().to_string()
}

pub fn capabilities() -> BuildCapabilities {
    let (apple_ok, apple_reason) = crate::store::apple_model_availability();
    BuildCapabilities {
        flavor: FLAVOR,
        sandboxed: IS_MAS,
        video_recording: VIDEO_RECORDING,
        accessibility_capture: ACCESSIBILITY_CAPTURE,
        owner_infra: OWNER_INFRA,
        storekit: IS_MAS,
        pro_gating: IS_MAS,
        apple_intelligence: apple_ok,
        apple_intelligence_reason: apple_reason,
        version: env!("CARGO_PKG_VERSION"),
        build: build_number(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_number_is_numeric() {
        let b = super::build_number();
        assert!(!b.is_empty() && b.chars().all(|c| c.is_ascii_digit()), "{:?}", b);
    }
}

#[tauri::command]
pub async fn get_build_capabilities() -> Result<BuildCapabilities, String> {
    Ok(capabilities())
}
