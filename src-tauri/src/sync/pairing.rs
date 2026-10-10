//! Pairing and keys (docs/SYNC.md "Pairing").
//!
//! - The Mac's TLS identity: a self-signed certificate (`rcgen`) whose
//!   private key and certificate live only in the Keychain.
//! - One-time pairing codes: 10 Crockford base32 characters, 5 minutes,
//!   one use, thrown away after 5 wrong attempts. In memory only.
//! - Device secrets: 32 random bytes per paired device, Keychain only.
//!
//! Nothing here is ever logged.

use super::protocol as p;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// Keychain service for sync keys (TLS identity, device secrets)
pub const SYNC_SERVICE: &str = "com.nofriction.meetings.sync";
const TLS_KEY: &str = "tls-key";
const TLS_CERT: &str = "tls-cert";
pub const CODE_TTL: Duration = Duration::from_secs(5 * 60);
pub const CODE_MAX_ATTEMPTS: u32 = 5;
const CODE_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The Mac's TLS identity
#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl Identity {
    /// SHA-256 of the certificate, lowercase hex (what the iPhone pins)
    pub fn fingerprint(&self) -> String {
        hex(&Sha256::digest(&self.cert_der))
    }

    pub fn generate() -> Result<Identity, String> {
        let mut params = rcgen::CertificateParams::new(vec!["nofriction-sync.local".to_string()])
            .map_err(|e| format!("Couldn't make the sync certificate: {}", e))?;
        params.distinguished_name.push(rcgen::DnType::CommonName, "noFriction sync");
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2099, 12, 31);
        let key = rcgen::KeyPair::generate().map_err(|e| format!("Couldn't make the sync key: {}", e))?;
        let cert = params.self_signed(&key).map_err(|e| format!("Couldn't make the sync certificate: {}", e))?;
        Ok(Identity { cert_der: cert.der().to_vec(), key_der: key.serialize_der() })
    }

    /// The stored identity, or a new one saved to the Keychain
    pub fn load_or_create() -> Result<Identity, String> {
        let stored = (
            crate::secrets::get(SYNC_SERVICE, TLS_CERT).and_then(|s| p::unb64(&s)),
            crate::secrets::get(SYNC_SERVICE, TLS_KEY).and_then(|s| p::unb64(&s)),
        );
        if let (Some(cert_der), Some(key_der)) = stored {
            return Ok(Identity { cert_der, key_der });
        }
        let id = Identity::generate()?;
        crate::secrets::set(SYNC_SERVICE, TLS_KEY, &p::b64(&id.key_der))?;
        crate::secrets::set(SYNC_SERVICE, TLS_CERT, &p::b64(&id.cert_der))?;
        Ok(id)
    }

    pub fn server_config(&self) -> Result<std::sync::Arc<rustls::ServerConfig>, String> {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| format!("TLS setup failed: {}", e))?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone())),
            )
            .map_err(|e| format!("TLS setup failed: {}", e))?;
        Ok(std::sync::Arc::new(config))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    rand::rng().fill_bytes(&mut buf);
    buf
}

// ── Pairing codes ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PendingPair {
    pub code: String,
    pub expires: Instant,
    pub attempts: u32,
}

impl PendingPair {
    pub fn new() -> PendingPair {
        let bytes = random_bytes::<10>();
        let code = bytes.iter().map(|b| CODE_ALPHABET[(*b as usize) % 32] as char).collect();
        PendingPair { code, expires: Instant::now() + CODE_TTL, attempts: 0 }
    }
}

impl Default for PendingPair {
    fn default() -> Self {
        Self::new()
    }
}

/// Codes are read from a QR or typed: compare case-insensitively and
/// ignore separators, in constant time.
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum CodeCheck {
    Ok,
    Wrong,
    Expired,
}

/// Check a code against the pending pairing; a wrong code counts an
/// attempt, and the 5th (or an expired code) throws the pairing away.
pub fn check_code(slot: &mut Option<PendingPair>, code: &str, now: Instant) -> CodeCheck {
    let Some(pending) = slot.as_mut() else { return CodeCheck::Expired };
    if now >= pending.expires {
        *slot = None;
        return CodeCheck::Expired;
    }
    if p::ct_eq(normalize_code(code).as_bytes(), pending.code.as_bytes()) {
        *slot = None; // one use
        return CodeCheck::Ok;
    }
    pending.attempts += 1;
    if pending.attempts >= CODE_MAX_ATTEMPTS {
        *slot = None;
    }
    CodeCheck::Wrong
}

/// `nfsync:1?id=…&n=…&fp=…&h=…&p=…&c=…`
pub fn pairing_link(mac_id: &str, name: &str, fingerprint: &str, hosts: &[String], port: u16, code: &str) -> String {
    format!(
        "nfsync:1?id={}&n={}&fp={}&h={}&p={}&c={}",
        mac_id,
        urlencoding::encode(name),
        fingerprint,
        urlencoding::encode(&hosts.join(",")),
        port,
        code
    )
}

/// The pairing link as an SVG QR code
pub fn qr_svg(link: &str) -> Result<String, String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::with_error_correction_level(link.as_bytes(), qrcode::EcLevel::M)
        .map_err(|e| format!("Couldn't make the QR code: {}", e))?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

// ── Device secrets ────────────────────────────────────────────────────────

fn account(device_id: &str) -> String {
    format!("device:{}", device_id)
}

pub fn new_secret() -> [u8; 32] {
    random_bytes::<32>()
}

pub fn store_secret(device_id: &str, secret: &[u8]) -> Result<(), String> {
    crate::secrets::set(SYNC_SERVICE, &account(device_id), &p::b64(secret))
}

pub fn load_secret(device_id: &str) -> Option<Vec<u8>> {
    crate::secrets::get(SYNC_SERVICE, &account(device_id)).and_then(|s| p::unb64(&s)).filter(|s| s.len() == 32)
}

pub fn delete_secret(device_id: &str) -> Result<(), String> {
    crate::secrets::delete(SYNC_SERVICE, &account(device_id))
}

// ── This Mac ──────────────────────────────────────────────────────────────

/// The Mac's name as the user set it ("Casey's MacBook Pro")
pub fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    {
        use core_foundation::base::TCFType;
        use core_foundation::string::{CFString, CFStringRef};
        #[link(name = "SystemConfiguration", kind = "framework")]
        extern "C" {
            fn SCDynamicStoreCopyComputerName(store: *const std::ffi::c_void, encoding: *mut u32) -> CFStringRef;
        }
        let name = unsafe {
            let r = SCDynamicStoreCopyComputerName(std::ptr::null(), std::ptr::null_mut());
            (!r.is_null()).then(|| CFString::wrap_under_create_rule(r).to_string())
        };
        if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
            return n;
        }
    }
    "Mac".to_string()
}

/// The Mac's LAN IPv4 addresses (hints in the pairing link; Bonjour is
/// tried first). Interfaces only, no network traffic.
pub fn lan_addresses() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 {
            return out;
        }
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            if !ifa.ifa_addr.is_null()
                && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET
                && (ifa.ifa_flags & libc::IFF_UP as u32) != 0
                && (ifa.ifa_flags & libc::IFF_LOOPBACK as u32) == 0
            {
                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                if super::server::is_local_addr(&std::net::IpAddr::V4(ip)) && !out.contains(&ip.to_string()) {
                    out.push(ip.to_string());
                }
            }
            cur = ifa.ifa_next;
        }
        libc::freeifaddrs(ifap);
    }
    out
}
