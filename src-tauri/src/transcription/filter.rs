//! Shared post-filter for FINAL transcript segments (all providers).
//!
//! Speech models — Whisper above all — invent text on silence, room noise or
//! a still-open call window: stock phrases ("Thank you.", "Bye-bye.") and
//! loops of them ("Bye-bye. Bye-bye. Bye-bye. Bye-by…", cut mid-word by the
//! utterance cap). This module decides whether a final segment is real
//! speech, and cleans repetition loops out of otherwise real text.
//!
//! Tokenisation treats hyphens and punctuation as spaces, so "Bye-bye." is
//! the two tokens `bye bye`.

/// Context the caller knows about a segment beyond its text.
#[derive(Debug, Default, Clone, Copy)]
pub struct Signals {
    /// The audio barely cleared the adaptive speech threshold.
    pub low_energy: bool,
    /// The decoder's average token log-probability was low.
    pub low_confidence: bool,
    /// The previous final segment (recently) was dropped as junk.
    pub after_junk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Real speech; the text may have had repetition loops collapsed.
    Keep(String),
    /// Hallucination / junk; the reason is for debug logs only.
    Drop(&'static str),
}

/// Whole-utterance phrases that are never meeting speech (YouTube outros
/// from Whisper's training data, and the lone "you" it emits on silence).
const ALWAYS_JUNK: &[&str] = &[
    "",
    "you",
    "thanks for watching",
    "thank you for watching",
    "thank you so much for watching",
    "thank you very much for watching",
    "thanks for watching bye",
    "please subscribe",
    "like and subscribe",
    "subscribe",
    "see you in the next video",
];

/// Stock phrases that ARE sometimes real ("Thank you." at the end of a
/// call), so a single occurrence is only dropped with corroboration
/// (low energy, low confidence, or right after other junk). Repeated —
/// "Bye-bye.", "thank you thank you" — they are always dropped.
const STOCK_PHRASES: &[&str] = &[
    "bye",
    "goodbye",
    "bye now",
    "thank you",
    "thanks",
    "thank you very much",
    "thank you so much",
    // Genuine sign-offs that Whisper also invents on silence: dropped
    // only on weak audio / after junk, or when repeated
    "see you next time",
    "see you",
    "see you later",
    "see you soon",
    "talk soon",
    "take care",
    "okay",
    "ok",
    "so",
    "oh",
    "uh",
    "um",
    "hmm",
    "mm",
    "ah",
];

/// Tokens whose runs are a hallucination signature even when short.
/// ("no no no" is real speech; "you you you" is not.) Words people really
/// do repeat — "Yeah, yeah, yeah.", "Hello? Hello? Hello?" — are not here;
/// they only count as a loop at 5+ repeats.
const LOOP_VOCAB: &[&str] = &[
    "bye", "you", "thank", "thanks", "okay", "ok", "so", "oh", "uh", "um", "hmm", "mm", "ah", "the",
    "i", "a", "and",
];

/// A word repeated this many times in a row inside longer text is a loop.
const IN_TEXT_LOOP_MIN: usize = 4;

/// Lowercased word tokens; hyphens and punctuation split words, inner
/// apostrophes are kept ("that's").
pub fn tokens(text: &str) -> Vec<String> {
    let mapped: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' || c == '’' {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect();
    mapped
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '\'' || c == '’').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Collapse consecutive repeats of any 1–4 token phrase to one occurrence
/// ("bye bye bye" → "bye", "thank you thank you" → "thank you").
pub fn collapse_runs(tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tokens.to_vec();
    loop {
        let mut changed = false;
        for n in 1..=4usize {
            let mut i = 0;
            while i + 2 * n <= out.len() {
                if out[i..i + n] == out[i + n..i + 2 * n] {
                    out.drain(i + n..i + 2 * n);
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
        if !changed {
            return out;
        }
    }
}

/// Most frequent token and its count.
fn dominant(tokens: &[String]) -> Option<(&str, usize)> {
    let mut best: Option<(&str, usize)> = None;
    for t in tokens {
        let c = tokens.iter().filter(|x| *x == t).count();
        if best.map(|(_, bc)| c > bc).unwrap_or(true) {
            best = Some((t.as_str(), c));
        }
    }
    best
}

/// Drop a trailing fragment cut mid-word by the utterance cap
/// ("… bye bye by"): a last token that is a strict prefix of the repeated
/// token, in text that doesn't end on sentence punctuation.
fn strip_truncated_tail(text: &str, mut toks: Vec<String>) -> Vec<String> {
    let ends_cleanly = text
        .trim_end()
        .chars()
        .last()
        .map(|c| matches!(c, '.' | '!' | '?'))
        .unwrap_or(false)
        && !text.trim_end().ends_with("...");
    if ends_cleanly || toks.len() < 3 {
        return toks;
    }
    let last = toks.last().cloned().unwrap_or_default();
    let body = &toks[..toks.len() - 1];
    if let Some((dom, count)) = dominant(body) {
        if count >= 2 && dom.len() > last.len() && dom.starts_with(last.as_str()) {
            toks.pop();
        }
    }
    toks
}

/// True when the tokens are nothing but stock phrases back to back
/// ("okay bye", "thank you bye").
fn all_stock(toks: &[String]) -> bool {
    fn go(toks: &[String], depth: usize) -> bool {
        if toks.is_empty() {
            return true;
        }
        if depth > 12 {
            return false;
        }
        STOCK_PHRASES.iter().chain(ALWAYS_JUNK.iter()).any(|p| {
            let pt: Vec<&str> = p.split(' ').filter(|s| !s.is_empty()).collect();
            !pt.is_empty()
                && pt.len() <= toks.len()
                && toks[..pt.len()].iter().zip(&pt).all(|(a, b)| a == b)
                && go(&toks[pt.len()..], depth + 1)
        })
    }
    !toks.is_empty() && toks.len() <= 12 && go(toks, 0)
}

/// Classify one final segment.
pub fn classify(text: &str, signals: Signals) -> Verdict {
    let toks = strip_truncated_tail(text, tokens(text));
    if toks.is_empty() {
        return Verdict::Drop("empty");
    }
    if ALWAYS_JUNK.contains(&toks.join(" ").as_str()) {
        return Verdict::Drop("stock phrase");
    }

    // Long loops inside the text collapse to one copy first, so real words
    // around a loop survive ("Talk Thursday. Bye-bye. Bye-bye. …")
    let clean = collapse_loops_in_text(text);
    let looped = clean != text;
    let ctoks = strip_truncated_tail(&clean, tokens(&clean));
    if ctoks.is_empty() {
        return Verdict::Drop("empty");
    }

    // One token making up most of a short utterance: a decoder loop.
    // (A stutter around real words — "I I I don't I I haven't" — has more
    // than one other word and is left alone.)
    if let Some((dom, count)) = dominant(&ctoks) {
        let ratio = count as f32 / ctoks.len() as f32;
        let others: std::collections::BTreeSet<&String> = ctoks.iter().filter(|t| t.as_str() != dom).collect();
        if count >= 3 && ratio >= 0.7 && others.len() <= 1 && (LOOP_VOCAB.contains(&dom) || count >= 5) {
            return Verdict::Drop("repetition loop");
        }
    }

    let collapsed = collapse_runs(&ctoks);
    if ALWAYS_JUNK.contains(&collapsed.join(" ").as_str()) {
        return Verdict::Drop("repeated stock phrase");
    }
    if all_stock(&collapsed) {
        if looped || collapsed.len() < ctoks.len() {
            return Verdict::Drop("repeated stock phrase");
        }
        if signals.low_energy || signals.low_confidence || signals.after_junk {
            return Verdict::Drop("stock phrase on weak audio");
        }
    }

    Verdict::Keep(clean)
}

/// True when `classify` would drop the text with no extra context — used
/// to decide what may be trimmed after a detected meeting end.
pub fn is_junk(text: &str) -> bool {
    matches!(classify(text, Signals::default()), Verdict::Drop(_))
}

/// True when the text contains a repetition loop (or is junk outright);
/// such text must never prime the next decode.
pub fn is_repetitive(text: &str) -> bool {
    is_junk(text) || collapse_loops_in_text(text) != text
}

/// Collapse loops of ≥4 identical words/short phrases inside otherwise
/// real text to one occurrence, and drop a truncated copy at the very end:
/// "Talk Thursday. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-by" →
/// "Talk Thursday. Bye-bye." Returns `text` unchanged when there's no loop.
pub fn collapse_loops_in_text(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let keys: Vec<String> = words.iter().map(|w| tokens(w).join(" ")).collect();
    let mut keep = vec![true; words.len()];
    // Kept word index → replacement text (carries the run's final punctuation)
    let mut rewrite: Vec<(usize, String)> = Vec::new();
    let mut changed = false;

    let mut i = 0;
    while i < words.len() {
        let mut advanced = false;
        for n in 1..=3usize {
            if i + n > words.len() || keys[i..i + n].iter().any(|k| k.is_empty()) {
                continue;
            }
            let unit = &keys[i..i + n];
            let mut reps = 1;
            while i + (reps + 1) * n <= words.len() && keys[i + reps * n..i + (reps + 1) * n] == *unit {
                reps += 1;
            }
            if reps >= IN_TEXT_LOOP_MIN {
                let end = i + reps * n;
                for k in keep.iter_mut().take(end).skip(i + n) {
                    *k = false;
                }
                // "no no no no, that's wrong" → "no, that's wrong": the kept
                // copy ends with the punctuation that closed the run.
                let last_kept = i + n - 1;
                let (_, run_punct) = split_trailing_punct(words[end - 1]);
                if !run_punct.is_empty() {
                    let (core, _) = split_trailing_punct(words[last_kept]);
                    rewrite.push((last_kept, format!("{}{}", core, run_punct)));
                }
                // A cut-off copy right after the loop, at the end of the text
                if end + 1 == words.len() && !keys[end].is_empty() {
                    let frag = keys[end].as_str();
                    let joined = unit.join(" ");
                    if frag.len() < joined.len() && joined.starts_with(frag) {
                        keep[end] = false;
                    }
                }
                changed = true;
                i = end;
                advanced = true;
                break;
            }
        }
        if !advanced {
            i += 1;
        }
    }

    if !changed {
        return text.to_string();
    }
    let mut out: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    for (idx, w) in rewrite {
        out[idx] = w;
    }
    out.into_iter()
        .zip(keep)
        .filter_map(|(w, k)| k.then_some(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Split a whitespace word into (text, trailing punctuation):
/// "no," → ("no", ","), "Bye-bye." → ("Bye-bye", "."), "ok" → ("ok", "").
fn split_trailing_punct(word: &str) -> (&str, &str) {
    let core_len = word
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    word.split_at(core_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dropped(t: &str) -> bool {
        matches!(classify(t, Signals::default()), Verdict::Drop(_))
    }
    fn kept(t: &str) -> String {
        match classify(t, Signals::default()) {
            Verdict::Keep(s) => s,
            Verdict::Drop(why) => panic!("{:?} dropped ({})", t, why),
        }
    }

    #[test]
    fn tokenizes_hyphens_as_spaces() {
        assert_eq!(tokens("Bye-bye."), vec!["bye", "bye"]);
        assert_eq!(tokens("That's wrong!"), vec!["that's", "wrong"]);
    }

    #[test]
    fn drops_repetition_loops() {
        assert!(dropped("bye bye bye bye"));
        assert!(dropped("Bye. Bye. Bye."));
        assert!(dropped("thank you thank you"));
        assert!(dropped("you you you"));
        assert!(dropped("Thank you. Thank you. Thank you."));
    }

    #[test]
    fn drops_real_world_bye_bye_lines() {
        // Verbatim from a meeting left recording after the call ended
        assert!(dropped("Bye-bye."));
        assert!(dropped("Bye-bye. Bye-bye."));
        assert!(dropped("Bye-bye. Bye-bye. Bye-bye."));
        assert!(dropped("Bye-bye. Bye-bye. Bye-bye. Bye-bye."));
        assert!(dropped(
            "Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-by"
        ));
        assert!(dropped(
            "Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-by…"
        ));
    }

    #[test]
    fn keeps_real_sentences() {
        assert_eq!(kept("bye for now, talk Thursday"), "bye for now, talk Thursday");
        assert_eq!(kept("no no no, that's wrong"), "no no no, that's wrong");
        assert_eq!(kept("No, no, no."), "No, no, no.");
        assert_eq!(kept("I I I don't I I haven't"), "I I I don't I I haven't");
        assert_eq!(
            kept("Thank you for joining the call"),
            "Thank you for joining the call"
        );
        assert_eq!(kept("So the budget is approved."), "So the budget is approved.");
    }

    #[test]
    fn single_stock_phrase_needs_corroboration() {
        // A lone "Thank you." on clear audio may be real
        assert_eq!(kept("Thank you."), "Thank you.");
        assert_eq!(kept("Okay."), "Okay.");
        let weak = Signals { low_energy: true, ..Default::default() };
        assert!(matches!(classify("Thank you.", weak), Verdict::Drop(_)));
        let streak = Signals { after_junk: true, ..Default::default() };
        assert!(matches!(classify("Bye.", streak), Verdict::Drop(_)));
        let unsure = Signals { low_confidence: true, ..Default::default() };
        assert!(matches!(classify("Okay.", unsure), Verdict::Drop(_)));
    }

    #[test]
    fn stock_phrase_sequences() {
        assert!(dropped("Okay. Bye-bye."));
        assert!(dropped("Okay. Bye-bye. Bye-bye. Bye-bye. Bye-bye."));
        // A real farewell on clear audio stays…
        assert_eq!(kept("Thank you. Bye."), "Thank you. Bye.");
        // …but not in the middle of a run of junk
        let streak = Signals { after_junk: true, ..Default::default() };
        assert!(matches!(classify("Thank you. Bye.", streak), Verdict::Drop(_)));
    }

    #[test]
    fn always_junk_phrases() {
        assert!(!dropped("Thank you, Maria, that helps.")); // real
        assert!(dropped("Thanks for watching!"));
        assert!(dropped("you"));
        assert!(dropped("..."));
    }

    #[test]
    fn genuine_closings_and_repeats_survive_clear_audio() {
        assert_eq!(kept("See you next time."), "See you next time.");
        assert_eq!(kept("Okay, see you next time!"), "Okay, see you next time!");
        let weak = Signals { low_energy: true, ..Default::default() };
        assert!(matches!(classify("See you next time.", weak), Verdict::Drop(_)));
        let streak = Signals { after_junk: true, ..Default::default() };
        assert!(matches!(classify("See you later.", streak), Verdict::Drop(_)));
        assert!(dropped("See you next time. See you next time."), "repeated is junk");
        // Real emphatic repeats
        assert_eq!(kept("Yeah, yeah, yeah."), "Yeah, yeah, yeah.");
        assert_eq!(kept("Hello? Hello? Hello? Hi"), "Hello? Hello? Hello? Hi");
        // …but a long decoder loop is still collapsed to one copy
        assert_eq!(kept("yeah yeah yeah yeah yeah yeah"), "yeah");
        // YouTube outros stay junk outright
        assert!(dropped("See you in the next video."));
    }

    #[test]
    fn collapses_loops_inside_real_text() {
        assert_eq!(
            kept("Great, talk Thursday. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-by"),
            "Great, talk Thursday. Bye-bye."
        );
        assert_eq!(
            kept("we should ship it bye bye bye bye bye"),
            "we should ship it bye"
        );
        // Natural repetition (< 4) is left alone
        assert_eq!(kept("very very very good"), "very very very good");
        assert!(is_repetitive("ok so we ship bye bye bye bye bye"));
        assert!(!is_repetitive("ok so we ship on Friday"));
    }

    #[test]
    fn collapse_keeps_the_runs_trailing_punctuation() {
        assert_eq!(collapse_loops_in_text("no no no no, that's wrong"), "no, that's wrong");
        assert_eq!(kept("no no no no, that's wrong"), "no, that's wrong");
        assert_eq!(collapse_loops_in_text("we agreed yes yes yes yes. Next"), "we agreed yes. Next");
        assert_eq!(collapse_loops_in_text("Wait? Wait? Wait? Wait? Okay"), "Wait? Okay");
        // Kept copy's own punctuation survives when the run ends bare
        assert_eq!(collapse_loops_in_text("so, so, so, so we ship"), "so, we ship");
        assert_eq!(collapse_loops_in_text("right, right right right then"), "right, then");
        // Multi-word unit
        assert_eq!(
            collapse_loops_in_text("ok thank you thank you thank you thank you! Bye"),
            "ok thank you! Bye"
        );
        assert_eq!(split_trailing_punct("no,"), ("no", ","));
        assert_eq!(split_trailing_punct("Bye-bye…"), ("Bye-bye", "…"));
        assert_eq!(split_trailing_punct("ok"), ("ok", ""));
    }

    #[test]
    fn collapse_runs_phrases() {
        let t = tokens("thank you thank you thank you");
        assert_eq!(collapse_runs(&t), vec!["thank", "you"]);
        let t = tokens("a b a b c");
        assert_eq!(collapse_runs(&t), vec!["a", "b", "c"]);
    }
}
