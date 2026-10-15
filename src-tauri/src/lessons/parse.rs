//! Tolerant JSON extraction + text cleaning for untrusted bundle content.
//! Ported from `study::parse` (docs/LESSONS.md §4.6): every JSON file in a
//! `.lesson` bundle — including seed — is parsed tolerantly (trailing-comma
//! fallback, unknown fields ignored) then strictly validated and cleaned by
//! `schema::validate_*`. Nothing here panics on malformed input.

use serde_json::Value;

pub type ParseError = String;

// Text length caps (generous for a lesson, bounded against a hostile bundle).
pub const SHORT: usize = 200;
pub const MEDIUM: usize = 600;
pub const LONG: usize = 2000;

/// Remove `<think>…</think>` blocks (an unclosed one runs to the end).
fn strip_think(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(a) = out.find("<think>") {
        match out[a..].find("</think>") {
            Some(b) => out.replace_range(a..a + b + "</think>".len(), ""),
            None => out.truncate(a),
        }
    }
    out
}

/// The first complete JSON object or array in `s` (string-aware bracket
/// matching), so prose or code fences around it don't matter.
fn first_json_value(s: &str) -> Option<&str> {
    let start = s.find(|c| c == '{' || c == '[')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in s[start..].char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Trailing commas (`[1, 2,]`) are the most common small-model JSON slip.
/// Only applied after a strict parse failed. String-aware.
fn remove_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in chars.iter().enumerate() {
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        if c == ',' {
            let next = chars[i + 1..].iter().find(|x| !x.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Parse the JSON in untrusted text, tolerating think blocks, code fences,
/// surrounding prose and trailing commas.
pub fn extract_json(raw: &str) -> Result<Value, ParseError> {
    let text = strip_think(raw);
    let candidate = first_json_value(&text).ok_or("No JSON found in the content")?;
    serde_json::from_str::<Value>(candidate)
        .or_else(|_| serde_json::from_str::<Value>(&remove_trailing_commas(candidate)))
        .map_err(|_| "The JSON couldn't be read".to_string())
}

/// Trimmed, single-line, no control characters, at most `max` characters (cut
/// with "…"). Empty → None. This is the single text cleaner every authored
/// string flows through before storage; the UI then renders it as a text node.
pub fn clean_text(s: &str, max: usize) -> Option<String> {
    let s: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s = s.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if s.chars().count() > max {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        s = format!("{}…", cut.trim_end());
    }
    Some(s)
}

/// Clean a JSON string/number value as text. Non-string/number → None.
pub fn clean_value(v: &Value, max: usize) -> Option<String> {
    match v {
        Value::String(s) => clean_text(s, max),
        Value::Number(n) => clean_text(&n.to_string(), max),
        _ => None,
    }
}
