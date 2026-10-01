import Foundation

/// Pure text operations for Delete / Strike (docs/REDACTION.md). No SwiftData
/// writes here, so it's all unit-testable.
///
/// A strike leaves a marker token in the segment text: `⟦stricken:<uuid>⟧`.
/// The token carries only the id of its `Redaction` record (time covered, when,
/// reason), never the removed words. The UI renders it as a dark bar; exports
/// and AI prompts render `[stricken from the record]`.
enum RedactionText {
    static let placeholder = "[stricken from the record]"
    static let screenPlaceholder = "[screen stricken from the record]"

    static func markerToken(_ id: UUID) -> String { "⟦stricken:\(id.uuidString)⟧" }

    private static let markerRegex = try! NSRegularExpression(
        pattern: "⟦stricken:([0-9A-Fa-f-]{36})⟧")

    // MARK: Tokens

    struct Token: Equatable {
        enum Kind: Equatable { case word, marker(UUID) }
        let kind: Kind
        /// UTF-16 range in the segment text
        let range: NSRange
        let text: String

        var isWord: Bool { kind == .word }
    }

    /// Whitespace-separated words (punctuation stays attached to its word) and
    /// marker tokens, in order.
    static func tokens(_ text: String) -> [Token] {
        let ns = text as NSString
        var out: [Token] = []
        var cursor = 0

        func words(in range: NSRange) {
            var i = range.location
            let end = range.location + range.length
            while i < end {
                while i < end, isSpace(ns.character(at: i)) { i += 1 }
                let start = i
                while i < end, !isSpace(ns.character(at: i)) { i += 1 }
                if i > start {
                    let r = NSRange(location: start, length: i - start)
                    out.append(Token(kind: .word, range: r, text: ns.substring(with: r)))
                }
            }
        }

        for match in markerRegex.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            words(in: NSRange(location: cursor, length: match.range.location - cursor))
            let id = UUID(uuidString: ns.substring(with: match.range(at: 1))) ?? UUID()
            out.append(Token(kind: .marker(id), range: match.range, text: ns.substring(with: match.range)))
            cursor = match.range.location + match.range.length
        }
        words(in: NSRange(location: cursor, length: ns.length - cursor))
        return out
    }

    private static func isSpace(_ c: unichar) -> Bool {
        guard let scalar = UnicodeScalar(c) else { return false }
        return CharacterSet.whitespacesAndNewlines.contains(scalar)
    }

    /// Maximal runs of word tokens between markers (index ranges into `tokens`).
    static func wordRuns(_ tokens: [Token]) -> [ClosedRange<Int>] {
        var runs: [ClosedRange<Int>] = []
        var start: Int?
        for (i, t) in tokens.enumerated() {
            if t.isWord {
                if start == nil { start = i }
            } else if let s = start {
                runs.append(s...(i - 1))
                start = nil
            }
        }
        if let s = start { runs.append(s...(tokens.count - 1)) }
        return runs
    }

    /// True if `range` covers only words (selection may never swallow a marker).
    static func isWordsOnly(_ tokens: [Token], _ range: ClosedRange<Int>) -> Bool {
        range.lowerBound >= 0 && range.upperBound < tokens.count && tokens[range].allSatisfy(\.isWord)
    }

    static func span(_ tokens: [Token], _ range: ClosedRange<Int>) -> NSRange {
        let a = tokens[range.lowerBound].range
        let b = tokens[range.upperBound].range
        return NSRange(location: a.location, length: b.location + b.length - a.location)
    }

    // MARK: Splice

    struct SpliceResult: Equatable {
        var text: String
        /// Timings still in the text, offsets updated
        var timings: [WordTiming]
        /// Timings of the removed words (for the audio range)
        var removedTimings: [WordTiming]
    }

    /// Remove `range` (UTF-16) from `text`, optionally inserting a marker in
    /// its place, and close up whitespace: exactly one space between the parts
    /// that remain, none at the ends.
    static func splice(_ text: String, timings: [WordTiming] = [], removing range: NSRange, inserting insert: String? = nil) -> SpliceResult {
        let ns = text as NSString
        let leftRaw = ns.substring(to: range.location)
        let rightRaw = ns.substring(from: range.location + range.length)
        let left = leftRaw.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        let right = rightRaw.replacingOccurrences(of: "^\\s+", with: "", options: .regularExpression)
        let parts = [left, insert ?? "", right].filter { !$0.isEmpty }
        let out = parts.joined(separator: " ")

        let leftEnd = (left as NSString).length
        let rightStartOld = range.location + range.length + ((rightRaw as NSString).length - (right as NSString).length)
        let rightStartNew = (out as NSString).length - (right as NSString).length
        let delta = rightStartNew - rightStartOld

        var kept: [WordTiming] = []
        var removed: [WordTiming] = []
        for t in timings {
            if t.location + t.length <= leftEnd {
                kept.append(t)
            } else if t.location >= rightStartOld {
                var moved = t
                moved.location += delta
                kept.append(moved)
            } else {
                removed.append(t)
            }
        }
        return SpliceResult(text: out, timings: kept, removedTimings: removed)
    }

    /// Splice several token ranges out of one text (each replaced by `insert`
    /// if given). Ranges are processed from the end so offsets stay valid.
    static func splice(_ text: String, timings: [WordTiming], removingTokenRanges ranges: [ClosedRange<Int>], inserting insert: String?) -> SpliceResult {
        let toks = tokens(text)
        var result = SpliceResult(text: text, timings: timings, removedTimings: [])
        for r in ranges.sorted(by: { $0.lowerBound > $1.lowerBound }) {
            let step = splice(result.text, timings: result.timings, removing: span(toks, r), inserting: insert)
            result.text = step.text
            result.timings = step.timings
            result.removedTimings += step.removedTimings
        }
        return result
    }

    // MARK: Rendering

    enum Piece: Equatable {
        case text(String)
        case marker(UUID)
    }

    /// Text runs and markers, for the transcript view.
    static func pieces(_ text: String) -> [Piece] {
        var out: [Piece] = []
        var words: [String] = []
        for t in tokens(text) {
            switch t.kind {
            case .word: words.append(t.text)
            case .marker(let id):
                if !words.isEmpty { out.append(.text(words.joined(separator: " "))); words = [] }
                out.append(.marker(id))
            }
        }
        if !words.isEmpty { out.append(.text(words.joined(separator: " "))) }
        return out
    }

    /// Segment text with markers rendered as `[stricken from the record]`.
    static func plain(_ text: String) -> String {
        let ns = text as NSString
        return markerRegex.stringByReplacingMatches(in: text, range: NSRange(location: 0, length: ns.length),
                                                    withTemplate: NSRegularExpression.escapedTemplate(for: placeholder))
    }

    /// The marker id if this text is nothing but one marker.
    static func onlyMarker(_ text: String) -> UUID? {
        let t = tokens(text)
        if t.count == 1, case .marker(let id) = t[0].kind { return id }
        return nil
    }

    // MARK: AI outputs

    /// Common long words that would still over-match as a single removed
    /// word. Same list as the Mac (`redaction.rs` LONG_STOPWORDS).
    static let longStopwords: Set<String> = [
        "across", "actually", "against", "almost", "already", "always", "another", "anybody", "anyone",
        "anything", "anyway", "around", "basically", "because", "become", "before", "behind", "believe",
        "better", "between", "beyond", "certain", "certainly", "change", "changes", "coming", "company",
        "couldn't", "definitely", "different", "doesn't", "during", "either", "enough", "especially",
        "everybody", "everyone", "everything", "exactly", "follow", "getting", "having", "honestly",
        "however", "important", "inside", "instead", "itself", "keeping", "literally", "little", "looking",
        "making", "meeting", "meetings", "minute", "minutes", "moment", "myself", "nobody", "nothing",
        "number", "obviously", "online", "others", "outside", "people", "perhaps", "please", "pretty",
        "probably", "problem", "question", "questions", "rather", "really", "reason", "saying", "second",
        "seconds", "should", "shouldn't", "similar", "simply", "something", "sometimes", "somewhere",
        "started", "talking", "thanks", "things", "thinking", "though", "thought", "through", "together",
        "tomorrow", "totally", "toward", "towards", "trying", "understand", "unless", "usually", "whatever",
        "whether", "within", "without", "wouldn't", "yesterday", "yourself", "themselves", "ourselves",
        "himself", "herself", "therefore", "although"
    ]

    /// Whether removed text is specific enough to rewrite in the AI notes:
    /// two or more words, or one word of 6+ characters that isn't a common
    /// word. Rewriting every match of "plan" or "team" would mangle
    /// unrelated notes, so a common word only marks the notes stale.
    /// Same rule as the Mac (docs/REDACTION.md).
    static func isDistinctive(_ removed: String) -> Bool {
        let words = removed
            .split(whereSeparator: { $0.isWhitespace })
            .map { $0.lowercased().trimmingCharacters(in: CharacterSet.alphanumerics.inverted) }
            .filter { !$0.isEmpty }
        switch words.count {
        case 0: return false
        case 1: return words[0].count >= 6 && !longStopwords.contains(words[0])
        default: return true
        }
    }

    /// Redact every occurrence of each removed phrase (exact and
    /// case-insensitive, on word boundaries, any whitespace between words).
    /// Only distinctive phrases are rewritten (`isDistinctive`). Strike
    /// passes the placeholder; Delete passes "" (no trace) and the leftover
    /// spacing is closed up.
    static func redact(_ notes: String, phrases: [String], replacement: String) -> String {
        var out = notes
        let cleaned = phrases
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines.union(.punctuationCharacters).union(.symbols)) }
            .filter { !$0.isEmpty && !$0.contains("⟦") && isDistinctive($0) }
        for phrase in Set(cleaned).sorted(by: { $0.count > $1.count }) {
            let words = phrase.split(whereSeparator: { $0.isWhitespace }).map { NSRegularExpression.escapedPattern(for: String($0)) }
            var pattern = words.joined(separator: "\\s+")
            if let f = phrase.unicodeScalars.first, CharacterSet.alphanumerics.contains(f) { pattern = "(?<![\\p{L}\\p{N}_])" + pattern }
            if let l = phrase.unicodeScalars.last, CharacterSet.alphanumerics.contains(l) { pattern += "(?![\\p{L}\\p{N}_])" }
            guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { continue }
            out = re.stringByReplacingMatches(in: out, range: NSRange(location: 0, length: (out as NSString).length),
                                              withTemplate: NSRegularExpression.escapedTemplate(for: replacement))
        }
        if replacement.isEmpty && out != notes {
            out = out.replacingOccurrences(of: "[ \\t]{2,}", with: " ", options: .regularExpression)
            out = out.replacingOccurrences(of: "[ \\t]+([,.;:!?])", with: "$1", options: .regularExpression)
        }
        return out
    }

    // MARK: Whole transcript (exports, prompts, search)

    struct Entry: Equatable {
        let time: Date
        let text: String
    }

    /// Transcript lines in time order, markers as placeholders, struck screens
    /// interleaved at their capture time. Consecutive lines that are each the
    /// same single marker (a struck run of lines) collapse into one.
    static func entries(_ m: Meeting) -> [Entry] {
        var items: [(Date, Int, String)] = []
        var lastMarker: UUID?
        for s in m.orderedSegments {
            if let id = onlyMarker(s.text) {
                if id == lastMarker { continue }
                lastMarker = id
            } else {
                lastMarker = nil
            }
            items.append((s.start, 0, plain(s.text)))
        }
        for r in m.screenStrikes {
            items.append((r.coveredFrom ?? r.createdAt, 1, screenPlaceholder))
        }
        return items.sorted { ($0.0, $0.1) < ($1.0, $1.1) }.map { Entry(time: $0.0, text: $0.2) }
    }

    static func plainTranscript(_ m: Meeting) -> String {
        entries(m).map(\.text).joined(separator: " ")
    }
}
