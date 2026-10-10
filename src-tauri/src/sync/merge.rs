//! Merging a line edit (docs/SYNC.md "Line edits never carry removed text").
//!
//! Pure: given this device's tokens of a line and the sender's `keep` list,
//! work out what the sender removed (runs of our words missing from `keep`)
//! and which of its strike markers we don't have yet. The caller applies the
//! result through its own purge path. The same algorithm runs on iOS
//! (`SyncMerge.swift`); the shared cases in `SyncFixtures/merge_cases.json`
//! hold both to it.

use super::protocol::{word_hash, Tok};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepTok {
    Word(String),
    Marker(String),
}

/// `w:<16 hex>` / `m:<uuid>`; None if any entry is malformed (the whole edit
/// is then ignored).
pub fn parse_keep(keep: &[String]) -> Option<Vec<KeepTok>> {
    keep.iter()
        .map(|k| {
            if let Some(h) = k.strip_prefix("w:") {
                (h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit())).then(|| KeepTok::Word(h.to_ascii_lowercase()))
            } else if let Some(m) = k.strip_prefix("m:") {
                super::protocol::wire_id(m).map(KeepTok::Marker)
            } else {
                None
            }
        })
        .collect()
}

/// One change to this device's copy of the line, in its own UTF-16 offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Remove the words in `[start16, end16)`. `marker`: put this strike
    /// marker (wire id) where they were. `strike`: the sender struck them
    /// (AI outputs get the stricken placeholder), else it deleted them.
    Remove { start16: usize, end16: usize, marker: Option<String>, strike: bool },
    /// Insert strike markers at `at16` (the words were already gone here)
    Insert { at16: usize, markers: Vec<String> },
}

impl Op {
    fn pos(&self) -> usize {
        match self {
            Op::Remove { start16, .. } => *start16,
            Op::Insert { at16, .. } => *at16,
        }
    }
}

/// What to do to this line. `ops` are sorted from the end of the line to
/// the start, so applying them in order keeps earlier offsets valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinePlan {
    pub ops: Vec<Op>,
    /// Words to remove (for logs and tests; counts only)
    pub removed_words: usize,
}

impl LinePlan {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Plan the merge of `keep` into `local` (this device's tokens).
pub fn plan(local: &[Tok], keep: &[KeepTok], token_key: &[u8]) -> LinePlan {
    let hashes: Vec<Option<String>> = local
        .iter()
        .map(|t| match t {
            Tok::Word { text, .. } => Some(word_hash(token_key, text)),
            Tok::Marker { .. } => None,
        })
        .collect();
    let same = |i: usize, j: usize| -> bool {
        match (&local[i], &keep[j]) {
            (Tok::Word { .. }, KeepTok::Word(h)) => hashes[i].as_deref() == Some(h.as_str()),
            (Tok::Marker { id, .. }, KeepTok::Marker(k)) => id == k,
            _ => false,
        }
    };

    // Longest common subsequence (lines are short; O(n·m) is fine)
    let (n, m) = (local.len(), keep.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if same(i, j) { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same(i, j) {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let local_markers: HashSet<&str> = local
        .iter()
        .filter_map(|t| match t {
            Tok::Marker { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();

    let mut out = LinePlan::default();
    // Gaps: before the first pair, between pairs, after the last
    let mut bounds: Vec<(Option<usize>, Option<usize>, usize, usize)> = Vec::new(); // (prev local, prev keep, next local, next keep)
    let mut prev: Option<(usize, usize)> = None;
    for &(pi, pj) in pairs.iter().chain(std::iter::once(&(n, m))) {
        bounds.push((prev.map(|p| p.0), prev.map(|p| p.1), pi, pj));
        prev = Some((pi, pj));
    }
    for (pl, pk, nl, nk) in bounds {
        let lo_l = pl.map(|x| x + 1).unwrap_or(0);
        let lo_k = pk.map(|x| x + 1).unwrap_or(0);
        // New markers from the sender in this gap
        let new_markers: Vec<String> = (lo_k..nk)
            .filter_map(|k| match &keep[k] {
                KeepTok::Marker(id) if !local_markers.contains(id.as_str()) => Some(id.clone()),
                _ => None,
            })
            .collect();
        // Our words in this gap that the sender no longer has, in runs split by our markers
        let mut runs: Vec<(usize, usize)> = Vec::new(); // token index ranges, inclusive
        let mut start: Option<usize> = None;
        for t in lo_l..nl {
            match &local[t] {
                Tok::Word { .. } => {
                    if start.is_none() {
                        start = Some(t);
                    }
                }
                Tok::Marker { .. } => {
                    if let Some(s) = start.take() {
                        runs.push((s, t - 1));
                    }
                }
            }
        }
        if let Some(s) = start {
            runs.push((s, nl - 1));
        }
        let strike = !new_markers.is_empty();
        let mut markers = new_markers.into_iter();
        for (k, (a, b)) in runs.iter().enumerate() {
            out.removed_words += b - a + 1;
            out.ops.push(Op::Remove {
                start16: local[*a].start16(),
                end16: local[*b].end16(),
                marker: if k == 0 { markers.next() } else { None },
                strike,
            });
            if k == 0 {
                let rest: Vec<String> = markers.by_ref().collect();
                if !rest.is_empty() {
                    out.ops.push(Op::Insert { at16: local[*b].end16(), markers: rest });
                }
            }
        }
        if runs.is_empty() {
            let rest: Vec<String> = markers.collect();
            if !rest.is_empty() {
                let at16 = pl.map(|p| local[p].end16()).unwrap_or(0);
                out.ops.push(Op::Insert { at16, markers: rest });
            }
        }
    }
    // From the end of the line backwards; at the same offset an insert
    // (which sits after a run) goes first
    out.ops.sort_by(|a, b| {
        b.pos().cmp(&a.pos()).then_with(|| match (a, b) {
            (Op::Insert { .. }, Op::Remove { .. }) => std::cmp::Ordering::Less,
            (Op::Remove { .. }, Op::Insert { .. }) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        })
    });
    out
}

/// Insert marker tokens at UTF-16 offset `at16` with single spaces around
/// them (pure; used on the Mac for markers whose words were already gone).
pub fn insert_markers(text: &str, at16: usize, tokens: &[String]) -> String {
    let mut byte = text.len();
    let mut u = 0usize;
    for (b, c) in text.char_indices() {
        if u >= at16 {
            byte = b;
            break;
        }
        u += c.len_utf16();
    }
    let left = text[..byte].trim_end();
    let right = text[byte..].trim_start();
    let mid = tokens.join(" ");
    [left, mid.as_str(), right].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Apply a plan to a line in wire form (the reference both platforms'
/// shared merge cases are checked against).
pub fn apply_to_wire_text(text: &str, plan: &LinePlan) -> String {
    fn byte_at(text: &str, off16: usize) -> usize {
        let mut u = 0usize;
        for (b, c) in text.char_indices() {
            if u >= off16 {
                return b;
            }
            u += c.len_utf16();
        }
        text.len()
    }
    let mut t = text.to_string();
    for op in &plan.ops {
        match op {
            Op::Remove { start16, end16, marker, .. } => {
                let (s, e) = (byte_at(&t, *start16), byte_at(&t, *end16));
                let left = t[..s].trim_end();
                let right = t[e..].trim_start();
                let mid = marker.as_deref().map(super::protocol::wire_marker).unwrap_or_default();
                t = [left, mid.as_str(), right].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
            }
            Op::Insert { at16, markers } => {
                let toks: Vec<String> = markers.iter().map(|m| super::protocol::wire_marker(m)).collect();
                t = insert_markers(&t, *at16, &toks);
            }
        }
    }
    t
}
