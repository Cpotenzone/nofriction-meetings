import Foundation

/// Cleans final transcript segments before they're saved.
///
/// Speech recognizers fed long stretches of silence or room noise (a
/// recording left running after the meeting) hallucinate loops like
/// "Bye-bye. Bye-bye. Bye-bye. Bye-by". This filter:
/// - drops short segments that are only a repeated filler word
///   (≥ 3 repeats making up ≥ 70 % of the words) or a filler phrase loop,
/// - drops filler-only segments ("Bye-bye.", "Thank you.") heard on near
///   silence, or repeated verbatim from the previous segment,
/// - collapses loops inside longer segments ("so the plan is bye bye bye
///   bye" → "so the plan is bye"), keeping word timings aligned,
/// and keeps real sentences untouched ("Bye for now, talk Thursday.",
/// "No no no, that's wrong.").
///
/// Hyphens count as word breaks ("Bye-bye" = "bye bye") and a truncated
/// last word ("Bye-by") counts as the word it was cut from.
enum TranscriptFilter {
    enum Outcome: Equatable {
        /// Save this. `substantive` is false for filler-only text ("Okay.",
        /// "Bye-bye.") — kept, but it isn't evidence the meeting is still on.
        case keep(text: String, words: [WordTiming], substantive: Bool)
        case drop(Reason)
    }

    enum Reason: Equatable {
        case empty
        case repeatedFiller
        case fillerLoop
        case fillerOnSilence
        case duplicateFiller
    }

    /// Words recognizers produce on silence / noise, or that carry no content alone.
    static let fillers: Set<String> = [
        "bye", "byebye", "goodbye", "thank", "thanks", "you", "okay", "ok",
        "uh", "um", "umm", "uhm", "hmm", "hm", "mm", "mmm", "mhm", "huh",
        "ah", "oh", "yeah", "so", "and", "the", "a", "hello", "hi", "hey",
        "music", "applause", "laughter", "silence", "for", "watching",
    ]

    /// Fillers people genuinely repeat ("Yeah, yeah, yeah.", "Hello? Hello?
    /// Hello? Hi"): a short run of these is speech, not a loop. They still
    /// count as filler (dropped on near silence / as a duplicate) and a
    /// 4+ repeat is still a loop.
    static let repeatableFillers: Set<String> = ["yeah", "hello", "hi", "hey"]

    /// Genuine sign-offs that recognizers also invent on silence: kept on
    /// clear audio, dropped only on near silence or as a duplicate.
    static let closings: Set<String> = [
        "see you next time", "see you", "see you later", "see you soon", "talk soon", "take care",
    ]

    /// Longest segment the "repeated filler" drop rule applies to.
    static let shortSegmentWords = 24

    struct Token: Equatable {
        /// Lowercased letters/digits/apostrophes
        var norm: String
        /// The word plus attached punctuation, UTF-16, in the original text
        var range: NSRange
    }

    // MARK: - Entry point

    /// - Parameters:
    ///   - nearSilence: the input level stayed below the speech threshold
    ///     while this was "heard" (or the meeting has already been detected
    ///     as over).
    ///   - previous: text of the last segment that was kept, if any.
    static func clean(_ text: String, words: [WordTiming] = [], nearSilence: Bool = false, previous: String? = nil) -> Outcome {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let toks = tokens(trimmed)
        guard !toks.isEmpty else { return .drop(.empty) }
        let norms = toks.map(\.norm)
        let fillerOnly = norms.allSatisfy { fillers.contains($0) }

        // 1. Mostly one filler word, repeated: "bye bye bye", "you you you you"
        if norms.count <= shortSegmentWords, let (word, count) = mostCommon(norms),
           count >= 3, Double(count) >= 0.7 * Double(norms.count), fillers.contains(word),
           !repeatableFillers.contains(word) {
            return .drop(.repeatedFiller)
        }
        // 2. A filler phrase looping: "thank you thank you thank you"
        if fillerOnly, norms.count <= shortSegmentWords,
           let run = loops(norms).max(by: { $0.covered < $1.covered }),
           Double(run.covered) >= 0.7 * Double(norms.count) {
            return .drop(.fillerLoop)
        }
        // 3. Filler-only (or a lone sign-off) on near silence, or the same again
        if fillerOnly || closings.contains(norms.joined(separator: " ")) {
            if nearSilence { return .drop(.fillerOnSilence) }
            if let previous, tokens(previous).map(\.norm) == norms { return .drop(.duplicateFiller) }
        }

        // 4. Collapse loops inside a real segment
        var out = RedactionText.SpliceResult(
            text: trimmed,
            timings: AppleOnDeviceRecognizerEngine.rebase(words, from: text, to: trimmed),
            removedTimings: [])
        for run in loops(norms).reversed() {
            let keepEnd = toks[run.start + run.unit - 1].range
            let lastEnd = toks[run.end - 1].range
            let from = keepEnd.location + keepEnd.length
            let remove = NSRange(location: from, length: lastEnd.location + lastEnd.length - from)
            // The loop's last word may carry the punctuation that belongs to
            // the sentence ("no no no no, that's wrong"): keep it on the
            // surviving word unless that word already ends in punctuation.
            let ns = trimmed as NSString
            let carried = trailingPunctuation(ns.substring(with: lastEnd))
            let keptHas = !trailingPunctuation(ns.substring(with: keepEnd)).isEmpty
            let step = RedactionText.splice(out.text, timings: out.timings, removing: remove)
            out.text = step.text
            out.timings = step.timings
            if !carried.isEmpty && !keptHas {
                let insert = (carried as NSString).length
                out.text = (out.text as NSString).replacingCharacters(in: NSRange(location: from, length: 0), with: carried)
                out.timings = out.timings.map { t in
                    var t = t
                    if t.location >= from { t.location += insert }
                    return t
                }
            }
        }
        let substantive = !tokens(out.text).allSatisfy { fillers.contains($0.norm) }
        return .keep(text: out.text, words: out.timings, substantive: substantive)
    }

    // MARK: - Tokens

    /// Words split on whitespace and hyphens. A trailing fragment cut off
    /// mid-word ("…Bye-bye. Bye-by") takes the normalized form of the
    /// recent word it's a prefix of.
    static func tokens(_ text: String) -> [Token] {
        let ns = text as NSString
        guard let regex = try? NSRegularExpression(pattern: "[^\\s\\-\u{2010}\u{2011}\u{2012}\u{2013}\u{2014}]+") else { return [] }
        var out: [Token] = []
        for match in regex.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            let piece = ns.substring(with: match.range)
            let norm = normalize(piece)
            if !norm.isEmpty { out.append(Token(norm: norm, range: match.range)) }
        }
        if out.count >= 2, let last = out.last {
            let raw = ns.substring(with: last.range)
            let cutOff = !(raw.last.map { ".?!,;:…".contains($0) } ?? false)
            if cutOff, let whole = out.dropLast().suffix(4).reversed().first(where: { $0.norm.count > last.norm.count && $0.norm.hasPrefix(last.norm) }) {
                out[out.count - 1].norm = whole.norm
            }
        }
        return out
    }

    /// Sentence punctuation at the end of a token ("no," → ",", "bye." → ".").
    static func trailingPunctuation(_ token: String) -> String {
        String(token.reversed().prefix { ".?!,;:…".contains($0) }.reversed())
    }

    static func normalize(_ word: String) -> String {
        let folded = word.lowercased().replacingOccurrences(of: "\u{2019}", with: "'")
        let kept = folded.unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) || $0 == "'" }
        return String(String.UnicodeScalarView(kept)).trimmingCharacters(in: CharacterSet(charactersIn: "'"))
    }

    // MARK: - Loops

    struct Loop: Equatable {
        /// First token of the loop
        var start: Int
        /// Words in the repeated unit
        var unit: Int
        /// One past the last token in the loop (incl. a trailing partial repeat)
        var end: Int
        var covered: Int { end - start }
    }

    /// Minimum repeats to call it a loop: single words need 4 ("no no no"
    /// is speech), phrases 3.
    static func minRepeats(unit: Int) -> Int { unit == 1 ? 4 : 3 }

    /// Non-overlapping runs of a 1–4 word unit repeated back to back,
    /// preferring the longest unit at each position.
    static func loops(_ n: [String]) -> [Loop] {
        var found: [Loop] = []
        var i = 0
        while i < n.count {
            var hit: Loop?
            for unit in stride(from: 4, through: 1, by: -1) where i + unit * minRepeats(unit: unit) <= n.count {
                let pattern = Array(n[i..<i + unit])
                var reps = 1
                while i + (reps + 1) * unit <= n.count, Array(n[i + reps * unit..<i + (reps + 1) * unit]) == pattern { reps += 1 }
                guard reps >= minRepeats(unit: unit) else { continue }
                var end = i + reps * unit
                // A partial repeat running to the end of the segment ("thank you thank")
                let tail = Array(n[end...])
                if !tail.isEmpty, tail.count < unit, Array(pattern.prefix(tail.count)) == tail { end = n.count }
                hit = Loop(start: i, unit: unit, end: end)
                break
            }
            if let hit {
                found.append(hit)
                i = hit.end
            } else {
                i += 1
            }
        }
        return found
    }

    private static func mostCommon(_ words: [String]) -> (String, Int)? {
        var counts: [String: Int] = [:]
        for w in words { counts[w, default: 0] += 1 }
        return counts.max { $0.value < $1.value || ($0.value == $1.value && $0.key > $1.key) }.map { ($0.key, $0.value) }
    }
}
