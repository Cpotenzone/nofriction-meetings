//! Link detection and normalization (pure; no I/O).
//!
//! Shared rules with the iOS app (`ios/NoFriction/Links/LinkDetector.swift`).
//! Both run the cases in `detection_cases.json`, so a change here needs the
//! same change there. The rules are in docs/LINKS.md.
//!
//! - **Written**: `http(s)://…`, `www.…`, and bare domains whose last label is
//!   in [`BARE_TLDS`] (`example.com`, `khanacademy.org`). A bare domain needs
//!   a lowercase TLD unless the whole host is uppercase (`EXAMPLE.COM`), so
//!   `project.It` (two sentences run together) isn't a link. Emails, file
//!   names (`main.rs`), numbers, `Mr. Smith.` and paths (`/x/site.com`) are not.
//! - **Spoken** (transcripts only): "example dot com", "w w w dot …", "dot a
//!   i", and "slash word" path segments. A label can't be a common word
//!   ("the dot com bubble"), and "<name> at school dot edu" is an email.
//! - Strike markers are a hard boundary: no link is built across one.

use once_cell::sync::Lazy;
use regex::Regex;

/// Last labels accepted for a bare domain (no scheme, no `www.`). Common
/// file extensions that are also country codes (`.rs`, `.md`, `.py`, `.sh`,
/// `.pl`, `.zip`, `.mov`, `.app`) are left out on purpose.
pub const BARE_TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "mil", "int", "io", "ai", "co", "info", "biz", "dev", "me", "tv", "fm",
    "ly", "gg", "xyz", "tech", "site", "online", "blog", "news", "wiki", "page", "academy", "school", "education",
    "university", "college", "science", "museum", "health", "ac", "us", "uk", "ca", "au", "nz", "de", "fr", "es",
    "it", "nl", "se", "no", "dk", "fi", "ie", "ch", "at", "be", "jp", "kr", "cn", "in", "br", "mx", "eu", "il",
    "sg", "hk", "za", "ru",
];

/// TLDs accepted when spoken ("example dot com"): none is an everyday word.
pub const SPOKEN_TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "io", "ai", "co", "dev", "app", "info", "me", "tv", "us", "uk", "ca", "au",
    "de", "fr", "xyz",
];

/// Words that are never a spoken domain label or path segment.
pub const SPOKEN_STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "so", "to", "of", "in", "on", "at", "for", "with", "from", "by", "is",
    "was", "are", "were", "be", "been", "am", "it", "its", "this", "that", "these", "those", "my", "your", "our",
    "their", "his", "her", "we", "you", "they", "i", "he", "she", "me", "us", "them", "as", "if", "then", "than",
    "there", "here", "just", "like", "about", "into", "not", "no", "yes", "do", "does", "did", "go", "going", "use",
    "using", "called", "named", "polka", "dot", "slash", "period", "point",
];

/// Words before "at" that introduce a place, not an email's name ("it's
/// available at example dot com").
pub const SPOKEN_PLACE_WORDS: &[&str] = &[
    "look", "looking", "available", "posted", "online", "find", "found", "located", "hosted", "live", "site",
    "website", "page", "link", "up", "out", "more", "info", "details", "everything",
];

/// Query parameters dropped everywhere: tracking ids.
const TRACKING_PARAMS: &[&str] = &[
    "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "igshid", "mc_cid", "mc_eid", "_hsenc",
    "_hsmi", "mkt_tok",
];

/// Query parameters dropped everywhere: they can carry credentials (a token
/// on screen in an address bar is never copied into the Links list).
const SECRET_PARAMS: &[&str] = &[
    "access_token", "id_token", "refresh_token", "token", "auth", "auth_token", "authorization", "api_key",
    "apikey", "key", "password", "passwd", "pwd", "secret", "client_secret", "signature", "sig",
    "x-amz-signature", "x-amz-credential", "x-amz-security-token", "session", "sessionid", "session_id", "sid",
    "jwt",
];

/// Strike markers on both platforms (Mac `⟦strickenid<hex>⟧`, iOS
/// `⟦stricken:<uuid>⟧`).
static MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"⟦?strickenid[0-9a-f]{32}⟧?|⟦stricken:[0-9A-Fa-f-]{36}⟧").expect("marker regex")
});

/// What a marker becomes before detection: not a word, not URL text.
pub const MARKER_BOUNDARY: &str = " ⟦⟧ ";

/// Written candidates: a scheme URL, or a dotted host (with optional port and
/// path). Boundaries, TLDs and case are checked in code (same as iOS).
static CANDIDATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r#"(?i)(https?://[^\s<>"'`“”‘’⟦⟧]+)"#,
        r#"|((?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,24}(?::[0-9]{1,5})?(?:[/?#][^\s<>"'`“”‘’⟦⟧]*)?)"#
    ))
    .expect("candidate regex")
});

/// Characters stripped from the edges of a spoken token.
const TOKEN_PUNCT: &[char] = &[
    '.', ',', ';', ':', '!', '?', '"', '\'', '(', ')', '[', ']', '{', '}', '“', '”', '‘', '’', '…',
];

/// Most candidates looked at in one text (a pathological screen dump can't
/// stall listing).
const MAX_CANDIDATES: usize = 2000;

/// A normalized link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// Dedupe key: host without `www.`, port (unless default), path without
    /// trailing `/`, filtered query, and a `#/route` fragment. No scheme.
    pub key: String,
    /// What Open uses: the scheme (https unless written http), `www.` if it
    /// was there, then the key.
    pub url: String,
    /// Host without `www.` (plus `:port`), for display
    pub host: String,
    /// Everything after the host in `key` ("" for a site's home page)
    pub path: String,
    pub scheme: &'static str,
    pub www: bool,
}

/// One link found in a text, with where it starts (for ordering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub link: Normalized,
    pub pos: usize,
}

fn is_ascii_alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// Remove strike markers (they become a boundary no link crosses).
pub fn strip_markers(text: &str) -> String {
    if !text.contains("stricken") {
        return text.to_string();
    }
    MARKER_RE.replace_all(text, MARKER_BOUNDARY).into_owned()
}

/// Validate and normalize a link. `None` for anything that isn't an
/// http(s) URL or a bare host: other schemes (`javascript:`, `file:`,
/// `mailto:`), user info (`user:pass@`), whitespace, non-ASCII hosts, bad
/// ports, and hosts without a dot (except `localhost`).
pub fn normalize(input: &str) -> Option<Normalized> {
    let s = input.trim();
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let (scheme, rest) = if lower.starts_with("https://") {
        ("https", &s[8..])
    } else if lower.starts_with("http://") {
        ("http", &s[7..])
    } else {
        // Another scheme (`javascript:x`, `file:///`, `mailto:`) is rejected;
        // `host:port` is not a scheme.
        if let Some(colon) = s.find(':') {
            let head = &s[..colon];
            let looks_scheme = head.chars().next().map_or(false, |c| c.is_ascii_alphabetic())
                && head.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
            let after = &s[colon + 1..];
            let port_follows = after.chars().next().map_or(false, |c| c.is_ascii_digit());
            if looks_scheme && !port_follows {
                return None;
            }
        }
        ("https", s)
    };
    let auth_end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    let remainder = &rest[auth_end..];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host_raw, port) = match authority.rfind(':') {
        Some(i) => {
            let p = &authority[i + 1..];
            if p.is_empty() || p.len() > 5 || !p.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = p.parse().ok()?;
            if n == 0 || n > 65535 {
                return None;
            }
            (&authority[..i], Some(n))
        }
        None => (authority, None),
    };
    let mut host = host_raw.to_ascii_lowercase();
    if host.ends_with('.') {
        host.pop();
    }
    if !valid_host(&host) {
        return None;
    }
    let www = host.starts_with("www.") && host[4..].contains('.');
    let bare = if www { host[4..].to_string() } else { host.clone() };

    let (pathquery, fragment) = match remainder.find('#') {
        Some(i) => (&remainder[..i], Some(&remainder[i + 1..])),
        None => (remainder, None),
    };
    let (path, query) = match pathquery.find('?') {
        Some(i) => (&pathquery[..i], &pathquery[i + 1..]),
        None => (pathquery, ""),
    };
    let path = path.trim_end_matches('/');
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let name = p.split('=').next().unwrap_or("").to_ascii_lowercase();
            !(name.starts_with("utm_") || TRACKING_PARAMS.contains(&name.as_str()) || SECRET_PARAMS.contains(&name.as_str()))
        })
        .collect();
    let route = fragment.filter(|f| f.starts_with('/') || f.starts_with("!/"));

    let default_port = match scheme {
        "https" => 443,
        _ => 80,
    };
    let host_disp = match port {
        Some(p) if p != default_port => format!("{}:{}", bare, p),
        _ => bare,
    };
    let mut tail = path.to_string();
    if !kept.is_empty() {
        tail.push('?');
        tail.push_str(&kept.join("&"));
    }
    if let Some(r) = route {
        tail.push('#');
        tail.push_str(r);
    }
    let key = format!("{}{}", host_disp, tail);
    let url = format!("{}://{}{}", scheme, if www { "www." } else { "" }, key);
    Some(Normalized { key, url, host: host_disp, path: tail, scheme, www })
}

fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if host != "localhost" && !host.contains('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Only http and https links can be opened, and only well-formed ones.
pub fn is_openable(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && url == url.trim() && normalize(url).is_some()
}

/// Drop trailing punctuation a sentence put after a link, keeping a closing
/// bracket that belongs to the link (`/wiki/Foo_(bar)`).
fn trim_trailing(s: &str) -> &str {
    let mut out = s;
    loop {
        let Some(last) = out.chars().next_back() else { return out };
        let drop = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '…' => true,
            ')' => out.matches('(').count() < out.matches(')').count(),
            ']' => out.matches('[').count() < out.matches(']').count(),
            '}' => out.matches('{').count() < out.matches('}').count(),
            _ => false,
        };
        if !drop {
            return out;
        }
        out = &out[..out.len() - last.len_utf8()];
    }
}

/// Characters that, right before a bare host, mean it's part of something
/// else (an email, a path, an identifier).
fn blocks_before(c: char) -> bool {
    is_ascii_alnum(c) || "_-.@/\\$%+=~#&:".contains(c)
}

/// Check a bare-host candidate's host part: TLD list (any TLD after
/// `www.`), and lowercase TLD unless the whole host is uppercase.
fn bare_host_ok(host: &str) -> bool {
    let Some(tld) = host.rsplit('.').next() else { return false };
    let lower_host = host.to_ascii_lowercase();
    let www = lower_host.starts_with("www.") && lower_host[4..].contains('.');
    let tld_lower = tld.to_ascii_lowercase();
    if !www && !BARE_TLDS.contains(&tld_lower.as_str()) {
        return false;
    }
    if !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let all_upper = host.chars().all(|c| !c.is_ascii_lowercase());
    tld.chars().all(|c| c.is_ascii_lowercase()) || all_upper
}

/// Written links in `text` (markers already stripped).
fn detect_written(text: &str, out: &mut Vec<Found>) {
    for (n, m) in CANDIDATE_RE.find_iter(text).enumerate() {
        if n >= MAX_CANDIDATES {
            break;
        }
        let start = m.start();
        let before = text[..start].chars().next_back();
        let raw = m.as_str();
        let lower = raw.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            if before.map_or(false, is_ascii_alnum) {
                continue;
            }
            if let Some(link) = normalize(trim_trailing(raw)) {
                out.push(Found { link, pos: start });
            }
            continue;
        }
        if before.map_or(false, blocks_before) {
            continue;
        }
        let after = text[m.end()..].chars().next();
        // Split host[:port] from the path
        let host_end = raw.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(raw.len());
        let has_path = host_end < raw.len();
        let (hostport, path) = raw.split_at(host_end);
        let host = hostport.split(':').next().unwrap_or(hostport);
        let has_port = host.len() < hostport.len();
        let candidate: Option<String> = if bare_host_ok(host) {
            // A function call (`df.info()`), an email (`x.com@`), or more
            // identifier right after the host isn't a link
            if !has_path && after.map_or(false, |c| c == '(' || c == '@' || is_ascii_alnum(c) || c == '_' || c == '-') {
                None
            } else {
                Some(format!("{}{}", hostport, trim_trailing(path)))
            }
        } else if !has_path && !has_port {
            // `example.com.Next`: back off to the longest prefix that is a host
            let mut labels: Vec<&str> = host.split('.').collect();
            let mut found = None;
            while labels.len() > 2 {
                labels.pop();
                let h = labels.join(".");
                if bare_host_ok(&h) {
                    found = Some(h);
                    break;
                }
            }
            found
        } else {
            None
        };
        if let Some(c) = candidate {
            if let Some(link) = normalize(&c) {
                out.push(Found { link, pos: start });
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Tok {
    /// Byte offset of the token in the text
    pos: usize,
    /// Lowercased, with leading/trailing punctuation removed
    core: String,
    /// Punctuation was removed before / after the core
    lead: bool,
    trail: bool,
}

fn tokens(text: &str) -> Vec<Tok> {
    text.split_whitespace()
        .map(|piece| {
            // `split_whitespace` yields slices of `text`
            let pos = piece.as_ptr() as usize - text.as_ptr() as usize;
            let trimmed_start = piece.trim_start_matches(TOKEN_PUNCT);
            let core = trimmed_start.trim_end_matches(TOKEN_PUNCT);
            Tok {
                pos,
                core: core.to_lowercase(),
                lead: trimmed_start.len() < piece.len(),
                trail: core.len() < trimmed_start.len(),
            }
        })
        .collect()
}

fn is_label(core: &str) -> bool {
    !core.is_empty()
        && core.len() <= 63
        && core.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !core.starts_with('-')
        && !core.ends_with('-')
        && !SPOKEN_STOPWORDS.contains(&core)
}

/// The word before "at" in a spoken email: a name, not a common word or a
/// word that introduces a place.
fn is_email_name(t: &Tok) -> bool {
    !t.trail
        && !t.core.is_empty()
        && t.core.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-')
        && !SPOKEN_STOPWORDS.contains(&t.core.as_str())
        && !SPOKEN_PLACE_WORDS.contains(&t.core.as_str())
}

fn is_path_word(core: &str) -> bool {
    !core.is_empty()
        && core.chars().next().map_or(false, |c| c.is_ascii_alphanumeric())
        && core.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && !SPOKEN_STOPWORDS.contains(&core)
}

/// A spoken TLD at token `i`: one word ("com", "a.i.") or two single letters
/// ("a i"). Returns (tld, tokens used).
fn spoken_tld(toks: &[Tok], i: usize) -> Option<(String, usize)> {
    let t = toks.get(i)?;
    let mut word = t.core.clone();
    // "a.i." / "i.o." → "ai" / "io"
    if word.len() >= 3 && word.split('.').all(|p| p.len() == 1 && p.chars().all(|c| c.is_ascii_lowercase())) {
        word = word.replace('.', "");
    }
    if word.len() == 1 && !t.trail {
        if let Some(n) = toks.get(i + 1) {
            if n.core.len() == 1 && !n.lead {
                let two = format!("{}{}", word, n.core);
                if SPOKEN_TLDS.contains(&two.as_str()) {
                    return Some((two, 2));
                }
            }
        }
    }
    if SPOKEN_TLDS.contains(&word.as_str()) {
        return Some((word, 1));
    }
    None
}

/// Merge the spoken "www" forms into one token: "w w w", "w.w.w.",
/// "triple w", "dub dub dub".
fn merge_www(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let c = |k: usize| toks.get(k).map(|t| t.core.as_str()).unwrap_or("");
        let clean = |k: usize| toks.get(k).map_or(false, |t| !t.trail);
        let triple = (c(i) == "w" && c(i + 1) == "w" && c(i + 2) == "w")
            || (c(i) == "dub" && c(i + 1) == "dub" && c(i + 2) == "dub");
        let n = if triple && clean(i) && clean(i + 1) {
            3
        } else if c(i) == "triple" && c(i + 1) == "w" && clean(i) {
            2
        } else if c(i) == "w.w.w" {
            1
        } else {
            0
        };
        if n > 0 {
            out.push(Tok { pos: toks[i].pos, core: "www".into(), lead: toks[i].lead, trail: toks[i + n - 1].trail });
            i += n;
        } else {
            out.push(toks[i].clone());
            i += 1;
        }
    }
    out
}

/// Spoken links ("example dot com slash math") in `text` (markers stripped).
fn detect_spoken(text: &str, out: &mut Vec<Found>) {
    if !text.to_lowercase().contains("dot") {
        return;
    }
    let toks = merge_www(tokens(text));
    let mut i = 0;
    while i < toks.len() {
        if let Some((host, end)) = spoken_host_at(&toks, i) {
            // "<name> at school dot edu" is an email address ("is at" and
            // "available at" are places)
            let email = i >= 2
                && toks[i - 1].core == "at"
                && !toks[i - 1].lead
                && !toks[i - 1].trail
                && is_email_name(&toks[i - 2]);
            let mut j = end; // index after the TLD
            let mut path = String::new();
            if !toks[j - 1].trail {
                let mut segs = 0;
                while segs < 6
                    && toks.get(j).map_or(false, |t| t.core == "slash" && !t.lead && !t.trail)
                    && toks.get(j + 1).map_or(false, |t| !t.lead && is_path_word(&t.core))
                {
                    path.push('/');
                    path.push_str(&toks[j + 1].core);
                    let ends = toks[j + 1].trail;
                    j += 2;
                    segs += 1;
                    if ends {
                        break;
                    }
                }
            }
            if !email {
                if let Some(link) = normalize(&format!("{}{}", host, path)) {
                    out.push(Found { link, pos: toks[i].pos });
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

/// The longest "label dot label … dot tld" chain starting at token `i`.
/// Returns the host and the index after its last token.
fn spoken_host_at(toks: &[Tok], i: usize) -> Option<(String, usize)> {
    let first = toks.get(i)?;
    if !is_label(&first.core) || first.trail || first.core == "dot" {
        return None;
    }
    let mut labels = vec![first.core.clone()];
    let mut k = i + 1;
    let mut best: Option<(String, usize)> = None;
    // invariant: toks[k] should be "dot"
    while labels.len() <= 6 {
        let Some(dot) = toks.get(k) else { break };
        if dot.core != "dot" || dot.lead || dot.trail {
            break;
        }
        if let Some((tld, used)) = spoken_tld(toks, k + 1) {
            let last = &toks[k + used];
            if !toks[k + 1].lead {
                let host = format!("{}.{}", labels.join("."), tld);
                best = Some((host, k + 1 + used));
                // A TLD with punctuation after it ends the chain
                if last.trail || used == 2 {
                    break;
                }
            }
        }
        let Some(next) = toks.get(k + 1) else { break };
        if next.lead || next.trail || !is_label(&next.core) {
            break;
        }
        labels.push(next.core.clone());
        k += 2;
    }
    best
}

/// Every link in `text`, in text order. `spoken` adds spoken forms
/// (transcripts). Duplicates are kept (each is one mention).
pub fn detect(text: &str, spoken: bool) -> Vec<Found> {
    let text = strip_markers(text);
    let mut out = Vec::new();
    detect_written(&text, &mut out);
    if spoken {
        let mut said = Vec::new();
        detect_spoken(&text, &mut said);
        // A spoken match can't overlap a written one (positions are starts;
        // a written link is one whitespace-free run)
        for f in said {
            let overlaps = out.iter().any(|w| {
                let end = w.pos + text[w.pos..].find(char::is_whitespace).unwrap_or(text.len() - w.pos);
                f.pos >= w.pos && f.pos < end
            });
            if !overlaps {
                out.push(f);
            }
        }
        out.sort_by_key(|f| f.pos);
    }
    out
}
