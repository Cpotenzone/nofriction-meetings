//! Wire protocol for Sync with your iPhone (docs/SYNC.md).
//!
//! Frames are a 4-byte big-endian length and that many bytes of JSON. The
//! JSON is canonical (sorted keys, no whitespace, absent optionals omitted,
//! integers only) so the Swift side produces the same bytes; golden fixtures
//! in `ios/NoFrictionTests/SyncFixtures/` hold both sides to it.

use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const VERSION: u32 = 1;
/// Largest frame either side accepts
pub const MAX_FRAME: usize = 16 << 20;
/// Items per batch
pub const BATCH_MAX: usize = 500;
/// Largest photo or screen either side sends or accepts
pub const MAX_BLOB: usize = 16 << 20;
/// Bytes per `blob` message
pub const BLOB_CHUNK: usize = 256 << 10;

// ═══════════════════════════════════════════════════════════════════════════
// Messages
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Removals,
    Changes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Msg {
    /// iPhone → Mac, first message of a pairing session
    Pair { code: String, device_id: String, name: String },
    /// Mac → iPhone: the shared secret (base64), sent once over the pinned channel
    Paired { device_id: String, name: String, secret: String },
    /// iPhone → Mac, first message of a sync session
    Hello { device_id: String, nonce: String },
    Challenge { nonce: String, proof: String },
    Auth { proof: String },
    Welcome { device_id: String, name: String },
    Batch {
        phase: Phase,
        items: Vec<Item>,
        last: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upto: Option<i64>,
    },
    Pull { since: i64 },
    /// After the last batch of a phase. `retry`: ids the sender should send
    /// again next time (they couldn't be applied now). `want`: photos and
    /// screens whose files the receiver needs (`blob` messages follow).
    Applied {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        retry: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        want: Vec<String>,
    },
    /// iPhone → Mac after a pull: the files of these screens, please
    Want { ids: Vec<String> },
    /// One chunk of a photo/screen file (base64), in order
    Blob { id: String, off: i64, data: String, last: bool },
    /// After the blobs asked for; `missing`: ids that couldn't be sent
    BlobsEnd {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        missing: Vec<String>,
    },
    Done {},
    Error { code: String, message: String },
}

impl Msg {
    pub fn applied(retry: Vec<String>) -> Msg {
        Msg::Applied { retry, want: Vec::new() }
    }

    pub fn error(code: &str, message: &str) -> Msg {
        Msg::Error { code: code.into(), message: message.into() }
    }
}

/// Error codes (docs/SYNC.md "Session")
pub mod codes {
    pub const UNKNOWN_DEVICE: &str = "unknown_device";
    pub const BAD_PROOF: &str = "bad_proof";
    pub const BAD_CODE: &str = "bad_code";
    pub const EXPIRED_CODE: &str = "expired_code";
    pub const PRO_REQUIRED: &str = "pro_required";
    pub const SYNC_OFF: &str = "sync_off";
    pub const VERSION: &str = "version";
    pub const PROTOCOL: &str = "protocol";
}

// ═══════════════════════════════════════════════════════════════════════════
// Items
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Item {
    Recording(RecordingItem),
    Line(LineItem),
    Edit(EditItem),
    Strike(StrikeItem),
    Notes(NotesItem),
    Mark(MarkItem),
    #[serde(rename = "ref")]
    Ref(RefItem),
    Topic(TopicItem),
    Screen(ScreenItem),
    Gone(GoneItem),
}

impl Item {
    /// Removals travel (and are applied) before content
    pub fn is_removal(&self) -> bool {
        matches!(self, Item::Edit(_) | Item::Strike(_) | Item::Gone(_))
    }

    /// Application order inside a batch: strikes, edits, gones, then
    /// recordings before what hangs off them.
    pub fn order(&self) -> u8 {
        match self {
            Item::Strike(_) => 0,
            Item::Edit(_) => 1,
            Item::Gone(_) => 2,
            Item::Recording(_) => 3,
            Item::Line(_) => 4,
            Item::Notes(_) => 5,
            Item::Mark(_) => 6,
            Item::Ref(_) => 7,
            Item::Topic(_) => 8,
            Item::Screen(_) => 9,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Calendar {
    pub fn is_empty(&self) -> bool {
        *self == Calendar::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonEntry {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// "organizer" | "attendee"
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordingItem {
    pub id: String,
    pub title: String,
    pub started: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<i64>,
    /// meeting | class | personal
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notebook: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cal: Option<Calendar>,
    #[serde(default)]
    pub people: Vec<PersonEntry>,
    #[serde(rename = "mod")]
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineItem {
    pub id: String,
    pub rec: String,
    /// Strike markers in canonical form (`⟦stricken:<uuid>⟧`)
    pub text: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dur: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// "screen": heard from what was playing on screen (iPhone screen
    /// capture, `Segment.source`); absent = the microphone
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
}

/// A line as it now is on the sender, without its words: `w:<hash>` per
/// remaining word, `m:<uuid>` per strike marker (docs/SYNC.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditItem {
    pub id: String,
    pub rec: String,
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrikeItem {
    pub id: String,
    pub rec: String,
    /// words | line
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    pub created: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The line that holds the marker
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Screens it removed (target `screen`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
}

/// A photo or screen. Its file travels separately (`want` / `blob`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenItem {
    pub id: String,
    pub rec: String,
    /// When it was taken (wall clock), where Rewind shows it
    pub at: i64,
    /// Until when it was on screen (Mac screens), if known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    /// photo | screen (`Snapshot.source`; every Mac screen is `screen`)
    pub src: String,
    /// jpg | png
    pub ext: String,
    pub size: i64,
    /// SHA-256 of the file, lowercase hex
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotesItem {
    pub rec: String,
    pub md: String,
    pub made: i64,
    pub stale: bool,
    #[serde(rename = "mod")]
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkItem {
    pub id: String,
    pub rec: String,
    pub at: i64,
    /// important | question | test
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created: i64,
    #[serde(rename = "mod")]
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefItem {
    pub id: String,
    pub rec: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created: i64,
    #[serde(rename = "mod")]
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicItem {
    pub id: String,
    pub rec: String,
    pub label: String,
    pub key: String,
    /// Confidence 0–1000 (per mille: no floats on the wire)
    pub conf: i64,
    /// ai | user
    pub source: String,
    pub created: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoneItem {
    /// recording | line | mark | ref | topic | notes
    pub entity: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rec: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Canonical JSON and framing
// ═══════════════════════════════════════════════════════════════════════════

/// Compact JSON with object keys sorted (byte order), whatever serde_json's
/// map type is.
pub fn canonical_json(v: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &serde_json::Value, out: &mut String) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(&map[k.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// One message as canonical JSON bytes, with `"v"`.
pub fn encode(msg: &Msg) -> Vec<u8> {
    let mut value = serde_json::to_value(msg).expect("messages always serialize");
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("v".into(), serde_json::Value::from(VERSION));
    }
    canonical_json(&value).into_bytes()
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    Version(u64),
    Json(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Version(v) => write!(f, "unsupported protocol version {}", v),
            DecodeError::Json(e) => write!(f, "malformed message: {}", e),
        }
    }
}

pub fn decode(bytes: &[u8]) -> Result<Msg, DecodeError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| DecodeError::Json(e.to_string()))?;
    let v = value.get("v").and_then(|v| v.as_u64()).unwrap_or(0);
    if v != VERSION as u64 {
        return Err(DecodeError::Version(v));
    }
    serde_json::from_value(value).map_err(|e| DecodeError::Json(e.to_string()))
}

/// A complete frame: length prefix + body.
pub fn frame(msg: &Msg) -> Vec<u8> {
    let body = encode(msg);
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

pub async fn write_msg<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, msg: &Msg) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    w.write_all(&frame(msg)).await?;
    w.flush().await
}

pub async fn read_msg<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Msg> {
    use tokio::io::AsyncReadExt;
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_FRAME {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body).await?;
    decode(&body).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Split items into batches of at most [`BATCH_MAX`]; always at least one
/// (an empty last batch says "nothing").
pub fn batches(items: Vec<Item>) -> Vec<Vec<Item>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    let mut it = items.into_iter().peekable();
    while it.peek().is_some() {
        out.push(it.by_ref().take(BATCH_MAX).collect());
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// Ids
// ═══════════════════════════════════════════════════════════════════════════

/// Any UUID spelling (hyphenated or 32 hex, any case) → lowercase hyphenated
/// wire form. None for ids that aren't UUIDs (they don't sync).
pub fn wire_id(local: &str) -> Option<String> {
    uuid::Uuid::parse_str(local.trim()).ok().map(|u| u.hyphenated().to_string())
}

/// Wire id → the Mac's 32-hex form (redactions, markers, references, topics, lines)
pub fn simple_id(wire: &str) -> Option<String> {
    uuid::Uuid::parse_str(wire.trim()).ok().map(|u| u.simple().to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// Strike markers in line text
// ═══════════════════════════════════════════════════════════════════════════

static WIRE_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"⟦stricken:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})⟧")
        .expect("marker regex")
});
static MAC_MARKER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"⟦strickenid([0-9a-f]{32})⟧").expect("marker regex"));
static WIRE_MARKER_FULL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^⟦stricken:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})⟧$")
        .expect("marker regex")
});

pub fn wire_marker(id_wire: &str) -> String {
    format!("⟦stricken:{}⟧", id_wire)
}

/// Mac line text → wire text
pub fn text_to_wire(mac: &str) -> String {
    MAC_MARKER_RE
        .replace_all(mac, |c: &regex::Captures| {
            wire_id(&c[1]).map(|w| wire_marker(&w)).unwrap_or_else(|| c[0].to_string())
        })
        .into_owned()
}

/// Wire line text → Mac text
pub fn text_from_wire(wire: &str) -> String {
    WIRE_MARKER_RE
        .replace_all(wire, |c: &regex::Captures| {
            simple_id(&c[1]).map(|s| crate::redaction::marker_token(&s)).unwrap_or_default()
        })
        .into_owned()
}

/// The wire id if this whitespace token is exactly one wire marker
pub fn wire_marker_id(token: &str) -> Option<String> {
    WIRE_MARKER_FULL.captures(token).and_then(|c| wire_id(&c[1]))
}

// ═══════════════════════════════════════════════════════════════════════════
// Line tokens and their hashes (edit items)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// A whitespace-delimited word (UTF-16 offsets into the text)
    Word { text: String, start16: usize, end16: usize },
    /// A strike marker (wire id)
    Marker { id: String, start16: usize, end16: usize },
}

impl Tok {
    pub fn start16(&self) -> usize {
        match self {
            Tok::Word { start16, .. } | Tok::Marker { start16, .. } => *start16,
        }
    }
    pub fn end16(&self) -> usize {
        match self {
            Tok::Word { end16, .. } | Tok::Marker { end16, .. } => *end16,
        }
    }
}

/// Tokens of a line in wire form (Unicode White_Space separates words; a
/// token that is exactly one marker is a marker).
pub fn tokens(wire_text: &str) -> Vec<Tok> {
    tokens_by(wire_text, wire_marker_id)
}

/// The wire id if this whitespace token is exactly one Mac marker
pub fn mac_marker_id(token: &str) -> Option<String> {
    crate::redaction::marker_ids(token)
        .first()
        .filter(|id| crate::redaction::marker_token(id) == token)
        .and_then(|id| wire_id(id))
}

/// Tokens with a given marker recognizer (the Mac's own text uses
/// [`mac_marker_id`], so offsets are into the Mac's text).
pub fn tokens_by(wire_text: &str, marker_id: impl Fn(&str) -> Option<String>) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut u16pos = 0usize;
    let mut cur = String::new();
    let mut cur_start = 0usize;
    let flush = |cur: &mut String, start: usize, end: usize, out: &mut Vec<Tok>| {
        if cur.is_empty() {
            return;
        }
        let t = std::mem::take(cur);
        match marker_id(&t) {
            Some(id) => out.push(Tok::Marker { id, start16: start, end16: end }),
            None => out.push(Tok::Word { text: t, start16: start, end16: end }),
        }
    };
    for c in wire_text.chars() {
        if c.is_whitespace() {
            flush(&mut cur, cur_start, u16pos, &mut out);
        } else {
            if cur.is_empty() {
                cur_start = u16pos;
            }
            cur.push(c);
        }
        u16pos += c.len_utf16();
    }
    flush(&mut cur, cur_start, u16pos, &mut out);
    out
}

type HmacSha256 = Hmac<Sha256>;

pub fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().into()
}

/// The key for word hashes in edit items, derived from the device secret
pub fn token_key(secret: &[u8]) -> [u8; 32] {
    hmac(secret, &[b"nfsync-v1 tokens"])
}

pub fn word_hash(token_key: &[u8], word: &str) -> String {
    let h = hmac(token_key, &[word.as_bytes()]);
    h[..8].iter().map(|b| format!("{:02x}", b)).collect()
}

/// `keep` for an edit item: the line as it now is, words hashed
pub fn keep_list(token_key: &[u8], wire_text: &str) -> Vec<String> {
    tokens(wire_text)
        .into_iter()
        .map(|t| match t {
            Tok::Word { text, .. } => format!("w:{}", word_hash(token_key, &text)),
            Tok::Marker { id, .. } => format!("m:{}", id),
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Authentication
// ═══════════════════════════════════════════════════════════════════════════

pub fn mac_proof(secret: &[u8], nonce_phone: &[u8], nonce_mac: &[u8]) -> [u8; 32] {
    hmac(secret, &[b"nfsync-v1 mac", nonce_phone, nonce_mac])
}

pub fn phone_proof(secret: &[u8], nonce_mac: &[u8], nonce_phone: &[u8]) -> [u8; 32] {
    hmac(secret, &[b"nfsync-v1 phone", nonce_mac, nonce_phone])
}

/// Constant-time equality for proofs and codes
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn unb64(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

// ═══════════════════════════════════════════════════════════════════════════
// Time
// ═══════════════════════════════════════════════════════════════════════════

pub fn ms(dt: chrono::DateTime<chrono::Utc>) -> i64 {
    dt.timestamp_millis()
}

pub fn from_ms(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// RFC 3339 or SQLite `datetime('now')` text → ms
pub fn parse_time_ms(s: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
        .map(|n| n.and_utc().timestamp_millis())
}

// ═══════════════════════════════════════════════════════════════════════════
// Photo / screen files
// ═══════════════════════════════════════════════════════════════════════════

/// "jpg" or "png" from the file's first bytes; None for anything else
/// (never accepted, never sent).
pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else {
        None
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

/// A file as `blob` messages of at most [`BLOB_CHUNK`] bytes (one empty
/// message for an empty file).
pub fn blob_chunks(id: &str, bytes: &[u8]) -> Vec<Msg> {
    if bytes.is_empty() {
        return vec![Msg::Blob { id: id.into(), off: 0, data: String::new(), last: true }];
    }
    let n = bytes.len().div_ceil(BLOB_CHUNK);
    bytes
        .chunks(BLOB_CHUNK)
        .enumerate()
        .map(|(i, c)| Msg::Blob { id: id.into(), off: (i * BLOB_CHUNK) as i64, data: b64(c), last: i + 1 == n })
        .collect()
}

/// Reassembles one file from its chunks; checks order, size and checksum.
#[derive(Debug, Default)]
pub struct BlobAssembler {
    bufs: std::collections::HashMap<String, Vec<u8>>,
}

impl BlobAssembler {
    /// Feed one chunk. `Ok(Some(bytes))` when the file is complete and
    /// matches `expected` (size, sha256, JPEG/PNG); `Err` drops the file.
    pub fn feed(&mut self, id: &str, off: i64, data: &str, last: bool, expected: Option<&ScreenItem>) -> Result<Option<Vec<u8>>, String> {
        let chunk = unb64(data).ok_or("bad chunk")?;
        let buf = self.bufs.entry(id.to_string()).or_default();
        if off < 0 || off as usize != buf.len() {
            self.bufs.remove(id);
            return Err("chunk out of order".into());
        }
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_BLOB {
            self.bufs.remove(id);
            return Err("file too large".into());
        }
        if !last {
            return Ok(None);
        }
        let bytes = self.bufs.remove(id).unwrap_or_default();
        let Some(item) = expected else { return Err("a file nobody asked for".into()) };
        if bytes.len() as i64 != item.size || sha256_hex(&bytes) != item.sha {
            return Err("checksum mismatch".into());
        }
        if image_ext(&bytes) != Some(item.ext.as_str()) {
            return Err("not a JPEG or PNG image".into());
        }
        Ok(Some(bytes))
    }
}
