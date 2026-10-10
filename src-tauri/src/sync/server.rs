//! The Mac's sync listener and one session's state machine (docs/SYNC.md
//! "Session"). The iPhone drives; the Mac answers. Transcript text and keys
//! are never logged.

use super::pairing::{self, CodeCheck, Identity, PendingPair};
use super::protocol::{self as p, codes, Msg, Phase};
use super::store;
use crate::redaction::RedactionEnv;
use parking_lot::Mutex;
use sqlx::{Pool, Sqlite};
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};

/// Longest wait for the next message before a session is dropped
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub type ProCheck = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;
pub type EnvFn = Arc<dyn Fn() -> RedactionEnv + Send + Sync>;
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// Loopback, RFC 1918, link-local, CGNAT/Tailscale (100.64/10), IPv6 ULA
/// and link-local. Anything else is refused.
pub fn is_local_addr(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local_addr(&IpAddr::V4(v4));
            }
            let s = v6.segments();
            v6.is_loopback() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

pub struct Server {
    pub pool: Pool<Sqlite>,
    pub identity: Identity,
    pub mac_id: String,
    pub mac_name: String,
    pub pairing: Mutex<Option<PendingPair>>,
    pub pro: ProCheck,
    pub env: EnvFn,
    pub notify: Option<Notify>,
}

/// What one session did (counts only)
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SessionSummary {
    pub device_id: Option<String>,
    pub paired: bool,
    pub received: usize,
    pub sent: usize,
}

#[derive(Debug)]
pub struct SessionError {
    pub code: &'static str,
    pub message: String,
}

impl SessionError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        SessionError { code, message: message.into() }
    }
}

async fn read<S: AsyncRead + Unpin>(s: &mut S) -> Result<Msg, SessionError> {
    match tokio::time::timeout(READ_TIMEOUT, p::read_msg(s)).await {
        Ok(Ok(m)) => Ok(m),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
            let code = if e.to_string().contains("version") { codes::VERSION } else { codes::PROTOCOL };
            Err(SessionError::new(code, e.to_string()))
        }
        Ok(Err(e)) => Err(SessionError::new(codes::PROTOCOL, format!("connection closed: {}", e))),
        Err(_) => Err(SessionError::new(codes::PROTOCOL, "timed out")),
    }
}

async fn send<S: AsyncWrite + Unpin>(s: &mut S, m: &Msg) -> Result<(), SessionError> {
    p::write_msg(s, m).await.map_err(|e| SessionError::new(codes::PROTOCOL, format!("send failed: {}", e)))
}

impl Server {
    /// Accept connections until `shutdown` turns true.
    pub async fn serve(self: Arc<Self>, listener: tokio::net::TcpListener, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = match self.identity.server_config() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Sync: {}", e);
                return;
            }
        };
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                accepted = listener.accept() => {
                    let Ok((tcp, addr)) = accepted else { continue };
                    if !is_local_addr(&addr.ip()) {
                        log::warn!("Sync: refused a connection from outside the local network");
                        continue;
                    }
                    let server = self.clone();
                    let acceptor = acceptor.clone();
                    tokio::spawn(async move {
                        let tls = match tokio::time::timeout(READ_TIMEOUT, acceptor.accept(tcp)).await {
                            Ok(Ok(t)) => t,
                            _ => return, // wrong pin on the iPhone, or not TLS
                        };
                        let mut tls = tls;
                        match server.session(&mut tls).await {
                            Ok(sum) => log::info!(
                                "Sync session done: {} received, {} sent{}",
                                sum.received, sum.sent, if sum.paired { " (paired)" } else { "" }
                            ),
                            Err(e) => log::warn!("Sync session ended: {} ({})", e.code, e.message),
                        }
                        use tokio::io::AsyncWriteExt;
                        let _ = tls.shutdown().await;
                        if let Some(n) = &server.notify { n(); }
                    });
                }
            }
        }
    }

    /// One session over an established (TLS) stream.
    pub async fn session<S: AsyncRead + AsyncWrite + Unpin>(&self, s: &mut S) -> Result<SessionSummary, SessionError> {
        let result = self.session_inner(s).await;
        if let Err(e) = &result {
            let _ = send(s, &Msg::error(e.code, &e.message)).await;
        }
        result
    }

    async fn session_inner<S: AsyncRead + AsyncWrite + Unpin>(&self, s: &mut S) -> Result<SessionSummary, SessionError> {
        let mut sum = SessionSummary::default();
        match read(s).await? {
            Msg::Pair { code, device_id, name } => {
                if !(self.pro)().await {
                    return Err(SessionError::new(codes::PRO_REQUIRED, "noFriction Pro is needed to sync"));
                }
                let check = pairing::check_code(&mut self.pairing.lock(), &code, Instant::now());
                match check {
                    CodeCheck::Ok => {}
                    CodeCheck::Wrong => return Err(SessionError::new(codes::BAD_CODE, "That pairing code isn't right")),
                    CodeCheck::Expired => {
                        return Err(SessionError::new(codes::EXPIRED_CODE, "That pairing code expired. Show a new one on your Mac."))
                    }
                }
                let Some(device_id) = p::wire_id(&device_id) else {
                    return Err(SessionError::new(codes::PROTOCOL, "bad device id"));
                };
                let name: String = name.trim().chars().take(80).collect();
                let secret = pairing::new_secret();
                pairing::store_secret(&device_id, &secret).map_err(|e| SessionError::new(codes::PROTOCOL, e))?;
                store::add_device(&self.pool, &device_id, if name.is_empty() { "iPhone" } else { &name })
                    .await
                    .map_err(|e| SessionError::new(codes::PROTOCOL, e))?;
                send(s, &Msg::Paired { device_id: self.mac_id.clone(), name: self.mac_name.clone(), secret: p::b64(&secret) }).await?;
                sum.device_id = Some(device_id);
                sum.paired = true;
                Ok(sum)
            }
            Msg::Hello { device_id, nonce } => {
                let device_id = p::wire_id(&device_id).ok_or_else(|| SessionError::new(codes::PROTOCOL, "bad device id"))?;
                let nonce_p = p::unb64(&nonce).filter(|n| n.len() == 32).ok_or_else(|| SessionError::new(codes::PROTOCOL, "bad nonce"))?;
                let secret = match (store::device_known(&self.pool, &device_id).await, pairing::load_secret(&device_id)) {
                    (true, Some(sec)) => sec,
                    _ => return Err(SessionError::new(codes::UNKNOWN_DEVICE, "This Mac doesn't know this device. Pair again.")),
                };
                if !(self.pro)().await {
                    store::mark_synced(&self.pool, &device_id, Some("noFriction Pro isn't active on this Mac")).await;
                    return Err(SessionError::new(codes::PRO_REQUIRED, "noFriction Pro isn't active on the Mac"));
                }
                let nonce_m = pairing::random_bytes::<32>();
                let proof_m = p::mac_proof(&secret, &nonce_p, &nonce_m);
                send(s, &Msg::Challenge { nonce: p::b64(&nonce_m), proof: p::b64(&proof_m) }).await?;
                let Msg::Auth { proof } = read(s).await? else {
                    return Err(SessionError::new(codes::PROTOCOL, "expected auth"));
                };
                let expected = p::phone_proof(&secret, &nonce_m, &nonce_p);
                let ok = p::unb64(&proof).map(|got| p::ct_eq(&got, &expected)).unwrap_or(false);
                if !ok {
                    return Err(SessionError::new(codes::BAD_PROOF, "Authentication failed. Pair again."));
                }
                send(s, &Msg::Welcome { device_id: self.mac_id.clone(), name: self.mac_name.clone() }).await?;
                sum.device_id = Some(device_id.clone());
                let token_key = p::token_key(&secret);
                let result = self.sync_loop(s, &device_id, &token_key, &mut sum).await;
                match &result {
                    Ok(()) => store::mark_synced(&self.pool, &device_id, None).await,
                    Err(e) => store::mark_synced(&self.pool, &device_id, Some(&e.message)).await,
                }
                result.map(|_| sum)
            }
            _ => Err(SessionError::new(codes::PROTOCOL, "expected hello or pair")),
        }
    }

    /// Read `blob` messages until `blobs_end`; save each complete, checked
    /// file. Returns the ids that didn't arrive or didn't check out.
    async fn receive_blobs<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        s: &mut S,
        device: &str,
        asked: &[p::ScreenItem],
        sum: &mut SessionSummary,
    ) -> Result<Vec<String>, SessionError> {
        let mut assembler = p::BlobAssembler::default();
        let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut failed: Vec<String> = Vec::new();
        loop {
            match read(s).await? {
                Msg::Blob { id, off, data, last } => {
                    let item = asked.iter().find(|w| w.id == id);
                    match assembler.feed(&id, off, &data, last, item) {
                        Ok(None) => {}
                        Ok(Some(bytes)) => {
                            let env = (self.env)();
                            match store::add_screen(&self.pool, &env, device, item.expect("checked"), &bytes).await {
                                Ok(_) => {
                                    sum.received += 1;
                                    done.insert(id);
                                }
                                Err(e) => {
                                    log::warn!("Sync: couldn't save a screen: {}", e);
                                    failed.push(id);
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("Sync: a screen's file was refused: {}", e);
                            failed.push(id);
                        }
                    }
                }
                Msg::BlobsEnd { .. } => break,
                _ => return Err(SessionError::new(codes::PROTOCOL, "expected blobs")),
            }
        }
        for w in asked {
            if !done.contains(&w.id) && !failed.contains(&w.id) {
                failed.push(w.id.clone());
            }
        }
        Ok(failed)
    }

    async fn sync_loop<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        s: &mut S,
        device: &str,
        token_key: &[u8],
        sum: &mut SessionSummary,
    ) -> Result<(), SessionError> {
        let mut retry: Vec<String> = Vec::new();
        let mut want: Vec<p::ScreenItem> = Vec::new();
        loop {
            match read(s).await? {
                Msg::Batch { phase, items, last, .. } => {
                    sum.received += items.len();
                    // Removals arrive in their own phase; content in a
                    // removals batch would break the ordering rule
                    let items: Vec<p::Item> = match phase {
                        Phase::Removals => items.into_iter().filter(|i| i.is_removal()).collect(),
                        Phase::Changes => items,
                    };
                    let env = (self.env)();
                    let report = store::apply(&self.pool, &env, device, token_key, items).await;
                    for e in &report.errors {
                        log::warn!("Sync: couldn't apply an item: {}", e);
                    }
                    retry.extend(report.retry);
                    want.extend(report.want);
                    if last {
                        let asked: Vec<p::ScreenItem> = std::mem::take(&mut want);
                        let ids = asked.iter().map(|w| w.id.clone()).collect();
                        send(s, &Msg::Applied { retry: std::mem::take(&mut retry), want: ids }).await?;
                        if !asked.is_empty() {
                            // The files of the photos and screens we asked for
                            let failed = self.receive_blobs(s, device, &asked, sum).await?;
                            send(s, &Msg::applied(failed)).await?;
                        }
                        if let Some(n) = &self.notify {
                            n();
                        }
                    }
                }
                Msg::Pull { since } => {
                    let (items, upto) = store::collect(&self.pool, device, since.max(0), token_key)
                        .await
                        .map_err(|e| SessionError::new(codes::PROTOCOL, e))?;
                    sum.sent += items.len();
                    let chunks = p::batches(items);
                    let n = chunks.len();
                    for (i, chunk) in chunks.into_iter().enumerate() {
                        let last = i + 1 == n;
                        send(s, &Msg::Batch { phase: Phase::Changes, items: chunk, last, upto: last.then_some(upto) }).await?;
                    }
                }
                Msg::Want { ids } => {
                    let mut missing = Vec::new();
                    for id in ids {
                        match store::screen_file(&self.pool, &id).await {
                            Some(bytes) => {
                                for chunk in p::blob_chunks(&id, &bytes) {
                                    send(s, &chunk).await?;
                                }
                            }
                            None => missing.push(id),
                        }
                    }
                    send(s, &Msg::BlobsEnd { missing }).await?;
                }
                Msg::Done {} => return Ok(()),
                Msg::Error { code, message } => {
                    return Err(SessionError::new(codes::PROTOCOL, format!("device reported {}: {}", code, message)))
                }
                _ => return Err(SessionError::new(codes::PROTOCOL, "unexpected message")),
            }
        }
    }
}
