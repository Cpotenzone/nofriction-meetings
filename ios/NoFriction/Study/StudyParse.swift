import Foundation

/// The five parts of a study guide (same kinds and JSON shapes as the Mac,
/// `src-tauri/src/study/parse.rs`).
enum StudyKind: String, CaseIterable, Codable, Sendable {
    case summary, terms, flashcards, quiz, questions

    var label: String {
        switch self {
        case .summary: "Summary"
        case .terms: "Key terms"
        case .flashcards: "Flashcards"
        case .quiz: "Practice quiz"
        case .questions: "Questions to ask"
        }
    }

    /// For progress and errors: "the practice quiz"
    var noun: String {
        switch self {
        case .summary: "summary"
        case .terms: "key terms"
        case .flashcards: "flashcards"
        case .quiz: "practice quiz"
        case .questions: "questions to ask"
        }
    }
}

// MARK: - Validated shapes (what is stored and shown)

struct StudySummary: Codable, Equatable, Sendable {
    struct Section: Codable, Equatable, Sendable {
        var heading: String
        var bullets: [String]
    }
    var title: String?
    var sections: [Section]
}

struct StudyTerms: Codable, Equatable, Sendable {
    struct Term: Codable, Equatable, Sendable {
        var term: String
        var definition: String
    }
    var terms: [Term]
}

struct StudyCards: Codable, Equatable, Sendable {
    struct Card: Codable, Equatable, Hashable, Sendable {
        var front: String
        var back: String
    }
    var cards: [Card]
}

struct StudyQuiz: Codable, Equatable, Sendable {
    struct Item: Codable, Equatable, Sendable {
        var question: String
        var choices: [String]
        /// 0-based index of the correct choice
        var answer: Int
        var explanation: String
        /// Transcript time the answer comes from (ms from the meeting start)
        var atMs: Int?

        enum CodingKeys: String, CodingKey { case question, choices, answer, explanation, atMs = "at_ms" }
    }
    var questions: [Item]
}

struct StudyAsks: Codable, Equatable, Sendable {
    struct Item: Codable, Equatable, Sendable {
        var question: String
        var atMs: Int?
        enum CodingKeys: String, CodingKey { case question, atMs = "at_ms" }
    }
    var questions: [Item]
}

/// Model output → validated study material. The answer is untrusted: it is
/// parsed as JSON, every field is checked and cleaned, and only the cleaned
/// values are stored and shown (as plain text). Never crashes on bad input,
/// and never logs it.
enum StudyParse {
    enum Failure: LocalizedError, Equatable {
        case noJSON, unreadable, nothingUsable(String)
        var errorDescription: String? {
            switch self {
            case .noJSON: "The answer had no JSON in it"
            case .unreadable: "The answer's JSON couldn't be read"
            case .nothingUsable(let what): "The answer had no usable \(what)"
            }
        }
    }

    static let maxSections = 12, maxBullets = 10, maxTerms = 40, maxCards = 60, maxQuiz = 20, maxQuestions = 20
    static let minChoices = 2, maxChoices = 6
    private static let short = 160, medium = 400, long = 600

    // MARK: JSON extraction

    static func stripThinking(_ s: String) -> String {
        var out = s
        while let open = out.range(of: "<think>") {
            if let close = out.range(of: "</think>", range: open.upperBound..<out.endIndex) {
                out.removeSubrange(open.lowerBound..<close.upperBound)
            } else {
                out.removeSubrange(open.lowerBound..<out.endIndex)
            }
        }
        return out
    }

    /// The first complete JSON object or array (string-aware bracket matching).
    static func firstJSONValue(_ s: String) -> Substring? {
        guard let start = s.firstIndex(where: { $0 == "{" || $0 == "[" }) else { return nil }
        var depth = 0
        var inString = false
        var escaped = false
        var i = start
        while i < s.endIndex {
            let c = s[i]
            if inString {
                if escaped { escaped = false } else if c == "\\" { escaped = true } else if c == "\"" { inString = false }
            } else {
                switch c {
                case "\"": inString = true
                case "{", "[": depth += 1
                case "}", "]":
                    depth -= 1
                    if depth == 0 { return s[start...i] }
                default: break
                }
            }
            i = s.index(after: i)
        }
        return nil
    }

    /// `[1, 2,]` → `[1, 2]`, outside strings. Used only after a strict parse failed.
    static func removeTrailingCommas(_ s: Substring) -> String {
        let chars = Array(s)
        var out = ""
        var inString = false
        var escaped = false
        for (i, c) in chars.enumerated() {
            if inString {
                out.append(c)
                if escaped { escaped = false } else if c == "\\" { escaped = true } else if c == "\"" { inString = false }
                continue
            }
            if c == "\"" { inString = true }
            if c == "," {
                let next = chars[(i + 1)...].first { !$0.isWhitespace }
                if next == "}" || next == "]" { continue }
            }
            out.append(c)
        }
        return out
    }

    static func extractJSON(_ raw: String) throws -> Any {
        let text = stripThinking(raw)
        guard let candidate = firstJSONValue(text) else { throw Failure.noJSON }
        if let v = try? JSONSerialization.jsonObject(with: Data(candidate.utf8)) { return v }
        if let v = try? JSONSerialization.jsonObject(with: Data(removeTrailingCommas(candidate).utf8)) { return v }
        throw Failure.unreadable
    }

    // MARK: Cleaning

    private static func isBool(_ n: NSNumber) -> Bool { CFGetTypeID(n) == CFBooleanGetTypeID() }

    /// Trimmed, one line, no control characters, leading list markers
    /// removed, at most `max` characters (cut with "…"). Empty → nil.
    static func clean(_ v: Any?, max: Int) -> String? {
        var s: String
        if let str = v as? String {
            s = str
        } else if let n = v as? NSNumber, !isBool(n) {
            s = n.stringValue
        } else {
            return nil
        }
        var scalars = String.UnicodeScalarView()
        for u in s.unicodeScalars { scalars.append(u.properties.generalCategory == .control ? " " : u) }
        s = String(scalars).split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        while true {
            var t = Substring(s)
            if let f = t.first, "-*•–".contains(f), t.dropFirst().first == " " {
                t = t.dropFirst(2)
            } else {
                let digits = t.prefix { $0.isASCII && $0.isNumber }.count
                let rest = t.dropFirst(digits)
                if (1...2).contains(digits), rest.hasPrefix(". ") || rest.hasPrefix(") ") { t = rest.dropFirst(2) }
            }
            let trimmed = String(t).trimmingCharacters(in: .whitespaces)
            if trimmed.count == s.count { break }
            s = trimmed
        }
        if s.isEmpty { return nil }
        if s.count > max { s = String(s.prefix(max - 1)).trimmingCharacters(in: .whitespaces) + "…" }
        return s
    }

    private static func field(_ o: [String: Any], _ names: [String]) -> Any? {
        for n in names { if let v = o[n], !(v is NSNull) { return v } }
        return nil
    }

    private static func text(_ o: [String: Any], _ names: [String], _ max: Int) -> String? {
        clean(field(o, names), max: max)
    }

    private static func list(_ v: Any, _ keys: [String]) -> [Any]? {
        if let a = v as? [Any] { return a }
        if let o = v as? [String: Any] { return field(o, keys) as? [Any] }
        return nil
    }

    private static func key(_ s: String) -> String { s.lowercased().filter { $0.isLetter || $0.isNumber } }

    /// "12:34", "1:02:03", "[12:34]" or seconds → ms.
    static func parseClock(_ s: String) -> Int? {
        let t = s.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "[]")).trimmingCharacters(in: .whitespaces)
        if t.isEmpty { return nil }
        if !t.contains(":") {
            guard let secs = Double(t.hasSuffix("s") ? String(t.dropLast()) : t), secs.isFinite, secs >= 0 else { return nil }
            return Int((secs * 1000).rounded())
        }
        let parts = t.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count <= 3, !parts.contains(where: { $0.trimmingCharacters(in: .whitespaces).isEmpty }) else { return nil }
        var total = 0.0
        for (i, p) in parts.enumerated() {
            guard let n = Double(p.trimmingCharacters(in: .whitespaces)), n.isFinite, n >= 0, i == 0 || n < 60 else { return nil }
            total = total * 60 + n
        }
        return Int((total * 1000).rounded())
    }

    /// 754_000 → "12:34", 3_725_000 → "1:02:05"
    static func clock(_ ms: Int) -> String {
        TimeInterval(max(0, ms) / 1000).clock
    }

    private static func time(_ o: [String: Any], durationMs: Int) -> Int? {
        var ms: Int?
        if let n = o["at_ms"] as? NSNumber, !isBool(n) {
            ms = n.intValue
        } else if let v = field(o, ["time", "t", "timestamp", "at", "source_time"]) {
            if let s = v as? String { ms = parseClock(s) } else if let n = v as? NSNumber, !isBool(n) { ms = Int((n.doubleValue * 1000).rounded()) }
        }
        guard let ms, ms >= 0, durationMs <= 0 || ms <= durationMs + 60_000 else { return nil }
        return ms
    }

    private static func answerIndex(_ v: Any?, choices: [String]) -> Int? {
        if let n = v as? NSNumber, !isBool(n) {
            let d = n.doubleValue
            guard d == d.rounded(), d >= 0, Int(d) < choices.count else { return nil }
            return Int(d)
        }
        guard let s = (v as? String)?.trimmingCharacters(in: .whitespaces) else { return nil }
        let letter = s.trimmingCharacters(in: CharacterSet(charactersIn: ").:")).trimmingCharacters(in: .whitespaces)
        if letter.count == 1, let c = letter.uppercased().unicodeScalars.first {
            if (65...90).contains(c.value) {
                let i = Int(c.value) - 65
                return i < choices.count ? i : nil
            }
            if let d = Int(letter) { return d < choices.count ? d : nil }
        }
        let k = key(s)
        return choices.firstIndex { key($0) == k }
    }

    // MARK: Per kind

    static func summary(_ v: Any) throws -> StudySummary {
        let title = (v as? [String: Any]).flatMap { text($0, ["title", "topic"], short) }
        guard let sections = list(v, ["sections", "topics", "notes"]) else { throw Failure.nothingUsable("sections") }
        var out: [StudySummary.Section] = []
        for s in sections.prefix(maxSections * 2) {
            guard let o = s as? [String: Any], let heading = text(o, ["heading", "title", "topic", "name"], short) else { continue }
            let bullets = ((field(o, ["bullets", "points", "notes", "items"]) as? [Any]) ?? []).compactMap { clean($0, max: medium) }
            if bullets.isEmpty { continue }
            out.append(.init(heading: heading, bullets: Array(bullets.prefix(maxBullets))))
            if out.count == maxSections { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("sections") }
        return StudySummary(title: title, sections: out)
    }

    static func terms(_ v: Any) throws -> StudyTerms {
        guard let items = list(v, ["terms", "key_terms", "glossary"]) else { throw Failure.nothingUsable("terms") }
        var seen = Set<String>()
        var out: [StudyTerms.Term] = []
        for t in items {
            guard let o = t as? [String: Any], let term = text(o, ["term", "word", "name"], short),
                  let def = text(o, ["definition", "meaning", "def"], long) else { continue }
            if seen.insert(key(term)).inserted { out.append(.init(term: term, definition: def)) }
            if out.count == maxTerms { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("terms") }
        return StudyTerms(terms: out)
    }

    static func flashcards(_ v: Any) throws -> StudyCards {
        guard let items = list(v, ["cards", "flashcards"]) else { throw Failure.nothingUsable("flashcards") }
        var seen = Set<String>()
        var out: [StudyCards.Card] = []
        for c in items {
            guard let o = c as? [String: Any], let front = text(o, ["front", "question", "q", "term"], medium),
                  let back = text(o, ["back", "answer", "a", "definition"], long), key(front) != key(back) else { continue }
            if seen.insert(key(front)).inserted { out.append(.init(front: front, back: back)) }
            if out.count == maxCards { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("flashcards") }
        return StudyCards(cards: out)
    }

    static func quiz(_ v: Any, durationMs: Int) throws -> StudyQuiz {
        guard let items = list(v, ["questions", "quiz"]) else { throw Failure.nothingUsable("questions") }
        var out: [StudyQuiz.Item] = []
        for q in items {
            guard let o = q as? [String: Any], let question = text(o, ["question", "q", "prompt"], medium),
                  let raw = field(o, ["choices", "options", "answers"]) as? [Any] else { continue }
            let choices = raw.compactMap { clean($0, max: short) }
            guard choices.count == raw.count, (minChoices...maxChoices).contains(choices.count),
                  Set(choices.map(key)).count == choices.count,
                  let answer = answerIndex(field(o, ["answer", "correct", "correct_index", "answer_index"]), choices: choices) else { continue }
            out.append(.init(question: question, choices: choices, answer: answer,
                             explanation: text(o, ["explanation", "why", "reason"], medium) ?? "",
                             atMs: time(o, durationMs: durationMs)))
            if out.count == maxQuiz { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("questions") }
        return StudyQuiz(questions: out)
    }

    static func asks(_ v: Any, durationMs: Int) throws -> StudyAsks {
        guard let items = list(v, ["questions", "ask"]) else { throw Failure.nothingUsable("questions") }
        var seen = Set<String>()
        var out: [StudyAsks.Item] = []
        for q in items {
            var question: String?
            var at: Int?
            if let o = q as? [String: Any] {
                question = text(o, ["question", "q", "text"], medium)
                at = time(o, durationMs: durationMs)
            } else if q is String {
                question = clean(q, max: medium)
            }
            guard let question, seen.insert(key(question)).inserted else { continue }
            out.append(.init(question: question, atMs: at))
            if out.count == maxQuestions { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("questions") }
        return StudyAsks(questions: out)
    }

    /// Validate a model answer for `kind`; returns the cleaned material as
    /// JSON (the only thing stored).
    static func validate(_ kind: StudyKind, raw: String, durationMs: Int) throws -> String {
        let v = try extractJSON(raw)
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys]
        let data: Data
        switch kind {
        case .summary: data = try enc.encode(summary(v))
        case .terms: data = try enc.encode(terms(v))
        case .flashcards: data = try enc.encode(flashcards(v))
        case .quiz: data = try enc.encode(quiz(v, durationMs: durationMs))
        case .questions: data = try enc.encode(asks(v, durationMs: durationMs))
        }
        return String(decoding: data, as: UTF8.self)
    }

    static func decode<T: Decodable>(_ type: T.Type, _ json: String) -> T? {
        try? JSONDecoder().decode(type, from: Data(json.utf8))
    }

    /// Condensed notes from one chunk: plain lines, each cleaned, keeping a
    /// leading [m:ss] time. Empty → error.
    static func condensedLines(_ raw: String, maxLines: Int) throws -> [String] {
        var out: [String] = []
        for line in stripThinking(raw).split(whereSeparator: \.isNewline) {
            let l = line.trimmingCharacters(in: .whitespaces)
            if l.isEmpty || l.hasPrefix("```") { continue }
            var time: Substring?
            var rest = Substring(l)
            if l.hasPrefix("["), let end = l.firstIndex(of: "]"), parseClock(String(l[...end])) != nil {
                time = l[...end]
                rest = l[l.index(after: end)...]
            }
            guard let body = clean(String(rest), max: medium) else { continue }
            out.append(time.map { "\($0) \(body)" } ?? body)
            if out.count == maxLines { break }
        }
        if out.isEmpty { throw Failure.nothingUsable("notes") }
        return out
    }
}
