import Foundation

// Local retrieval for Chat (docs/TOPICS_AND_CHAT.md). No network, no
// embeddings: the scoped recordings' transcript lines, note sections and
// marker notes are split into passages and ranked by term match against the
// question; the best ones that fit the token budget go into the prompt.
// Pure and Sendable: `ChatSource` is built on the main actor from a
// Meeting, everything else works on plain values.

/// One recording's text, as retrieval reads it.
struct ChatSource: Equatable, Sendable {
    struct Line: Equatable, Sendable { var ms: Int; var text: String }
    struct Mark: Equatable, Sendable { var ms: Int; var label: String; var note: String }

    var meetingID: UUID
    var title: String
    var startedAt: Date
    var notebook: String?
    var kind: RecordingKind
    var lines: [Line]
    var notes: String?
    var marks: [Mark]

    init(meetingID: UUID, title: String, startedAt: Date, notebook: String? = nil, kind: RecordingKind = .meeting,
         lines: [Line], notes: String? = nil, marks: [Mark] = []) {
        self.meetingID = meetingID
        self.title = title
        self.startedAt = startedAt
        self.notebook = notebook
        self.kind = kind
        self.lines = lines
        self.notes = notes
        self.marks = marks
    }

    @MainActor init(meeting m: Meeting) {
        let ms = { (d: Date) in max(0, Int((d.timeIntervalSince(m.startedAt) * 1000).rounded(.down))) }
        meetingID = m.id
        title = m.title
        startedAt = m.startedAt
        notebook = m.courseName
        kind = m.kind
        // Stricken spans read as the placeholder; deleted words are gone
        lines = m.orderedSegments.map { Line(ms: ms($0.start), text: RedactionText.plain($0.text)) }
        notes = m.aiNotes
        marks = m.orderedMarkers.compactMap { mk in
            guard let n = mk.note?.trimmingCharacters(in: .whitespaces), !n.isEmpty else { return nil }
            return Mark(ms: ms(mk.at), label: mk.markerKind.label(for: m.kind), note: n)
        }
    }
}

/// A candidate for the prompt: a window of transcript lines, one notes
/// section, or one marker note.
struct ChatPassage: Equatable, Sendable, Identifiable {
    enum Kind: String, Sendable { case transcript = "Transcript", notes = "Notes", marker = "Marker" }

    var id: String
    var meetingID: UUID
    var title: String
    var startedAt: Date
    var notebook: String?
    var kind: Kind
    /// Milliseconds into the recording (0 for notes)
    var ms: Int
    var text: String

    var timestamp: Date { startedAt.addingTimeInterval(Double(ms) / 1000) }

    /// "[3] Title · Oct 6, 2026 · Transcript 12:34 · Notebook: BIO 101"
    func header(n: Int) -> String {
        var s = "[\(n)] \(title) · \(startedAt.formatted(date: .abbreviated, time: .omitted)) · \(kind.rawValue)"
        if kind != .notes { s += " \(StudyParse.clock(ms))" }
        if let notebook { s += " · \(Notebook.label): \(notebook)" }
        return s
    }
}

enum ChatRetrieval {
    /// Characters per transcript window
    static let windowChars = 420
    static let maxPassages = 12
    static let maxPerMeeting = 5
    static let excerptLength = 140

    /// Words that carry no meaning for matching
    static let stopwords: Set<String> = [
        "a", "an", "the", "and", "or", "but", "of", "to", "in", "on", "at", "by", "for", "with", "about", "from", "as",
        "is", "are", "was", "were", "be", "been", "being", "am", "do", "does", "did", "have", "has", "had", "it", "its",
        "this", "that", "these", "those", "there", "here", "what", "which", "who", "whom", "whose", "when", "where", "why",
        "how", "did", "we", "i", "you", "he", "she", "they", "them", "our", "us", "my", "me", "your", "his", "her", "their",
        "say", "said", "says", "tell", "told", "talk", "talked", "discuss", "discussed", "mention", "mentioned",
        "anything", "something", "everything", "any", "some", "all", "not", "no", "yes", "if", "then", "than", "so",
        "can", "could", "would", "should", "will", "shall", "may", "might", "please", "get", "got", "about", "up", "out",
        "recording", "recordings", "meeting", "meetings", "transcript", "notes", "note", "summarize", "summary",
    ]

    // MARK: Passages

    static func passages(from sources: [ChatSource]) -> [ChatPassage] {
        var out: [ChatPassage] = []
        for s in sources {
            // Transcript: windows of consecutive lines
            var window: [ChatSource.Line] = []
            var size = 0
            var index = 0
            func flush() {
                let text = window.map(\.text).joined(separator: " ")
                if !text.trimmingCharacters(in: .whitespaces).isEmpty, text != RedactionText.placeholder, let first = window.first {
                    out.append(ChatPassage(id: "\(s.meetingID.uuidString)-t\(index)", meetingID: s.meetingID, title: s.title,
                                           startedAt: s.startedAt, notebook: s.notebook, kind: .transcript, ms: first.ms, text: text))
                    index += 1
                }
                window = []
                size = 0
            }
            for l in s.lines {
                let t = l.text.trimmingCharacters(in: .whitespacesAndNewlines)
                if t.isEmpty { continue }
                if size > 0 && size + t.count > windowChars { flush() }
                window.append(.init(ms: l.ms, text: t))
                size += t.count + 1
            }
            if !window.isEmpty { flush() }
            // Notes: one passage per "## section" (or paragraph)
            if let notes = s.notes {
                for (i, section) in noteSections(notes).enumerated() {
                    out.append(ChatPassage(id: "\(s.meetingID.uuidString)-n\(i)", meetingID: s.meetingID, title: s.title,
                                           startedAt: s.startedAt, notebook: s.notebook, kind: .notes, ms: 0, text: section))
                }
            }
            for (i, m) in s.marks.enumerated() {
                out.append(ChatPassage(id: "\(s.meetingID.uuidString)-m\(i)", meetingID: s.meetingID, title: s.title,
                                       startedAt: s.startedAt, notebook: s.notebook, kind: .marker, ms: m.ms,
                                       text: "\(m.label): \(m.note)"))
            }
        }
        return out
    }

    /// Markdown notes split at headings; a long section at blank lines.
    static func noteSections(_ md: String) -> [String] {
        var sections: [String] = []
        var current: [String] = []
        for line in md.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("#") {
                let s = current.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
                if !s.isEmpty { sections.append(s) }
                current = [String(line)]
            } else {
                current.append(String(line))
            }
        }
        let s = current.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        if !s.isEmpty { sections.append(s) }
        return sections.flatMap { sec -> [String] in
            guard sec.count > windowChars * 2 else { return [sec] }
            return sec.components(separatedBy: "\n\n").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        }
    }

    // MARK: Ranking

    /// Lowercased, diacritics folded word terms of 2+ characters, stopwords
    /// out, simple plurals singularized. Numbers stay ("q4", "2026").
    static func terms(_ text: String) -> [String] {
        let folded = text.folding(options: [.diacriticInsensitive, .caseInsensitive, .widthInsensitive], locale: nil).lowercased()
        let scalars = folded.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) || $0 == "'" ? Character($0) : " " }
        return String(scalars).split(separator: " ").map { String($0).replacingOccurrences(of: "'", with: "") }
            .filter { $0.count >= 2 && !stopwords.contains($0) }
            .map(Topic.singular)
    }

    /// Passages that match at least one query term, best first. Score: for
    /// each term, its count in the passage (capped) weighted by how rare it
    /// is across all passages; a passage with every term, or the question's
    /// words in a row, scores extra; recent recordings break ties.
    static func rank(_ passages: [ChatPassage], query: String) -> [(passage: ChatPassage, score: Double)] {
        let qTerms = Array(Set(terms(query)))
        guard !qTerms.isEmpty, !passages.isEmpty else { return [] }
        let docs = passages.map { terms($0.text + " " + $0.title) }
        var df: [String: Int] = [:]
        for d in docs { for t in Set(d) where qTerms.contains(t) { df[t, default: 0] += 1 } }
        let n = Double(passages.count)
        let phrase = qTerms.count > 1 ? query.lowercased() : nil
        var out: [(ChatPassage, Double)] = []
        for (i, p) in passages.enumerated() {
            var counts: [String: Int] = [:]
            for t in docs[i] where qTerms.contains(t) { counts[t, default: 0] += 1 }
            guard !counts.isEmpty else { continue }
            var score = 0.0
            for (t, c) in counts {
                let idf = log(1 + n / Double(df[t] ?? 1))
                score += Double(min(c, 3)) * idf
            }
            if counts.count == qTerms.count { score *= 1.5 }
            if let phrase, p.text.lowercased().contains(phrase) { score *= 1.5 }
            if p.kind == .notes { score *= 1.1 }
            out.append((p, score))
        }
        return out.sorted { a, b in
            if a.1 != b.1 { return a.1 > b.1 }
            if a.0.startedAt != b.0.startedAt { return a.0.startedAt > b.0.startedAt }
            return a.0.id < b.0.id
        }.map { (passage: $0.0, score: $0.1) }
    }

    /// Best passages that fit `budgetChars`, at most `maxPerMeeting` from
    /// one recording and `maxPassages` in all, in reading order (recording
    /// by date, then time) so the model sees them in sequence.
    static func select(_ ranked: [ChatPassage], budgetChars: Int, maxPerMeeting: Int = maxPerMeeting, max: Int = maxPassages) -> [ChatPassage] {
        var used = 0
        var perMeeting: [UUID: Int] = [:]
        var picked: [ChatPassage] = []
        for p in ranked {
            guard picked.count < max else { break }
            guard perMeeting[p.meetingID, default: 0] < maxPerMeeting else { continue }
            let cost = p.text.count + 80
            if used + cost > budgetChars { continue }
            picked.append(p)
            used += cost
            perMeeting[p.meetingID, default: 0] += 1
        }
        return picked.sorted { a, b in
            if a.startedAt != b.startedAt { return a.startedAt < b.startedAt }
            if a.kind != b.kind { return a.kind == .notes }
            return a.ms < b.ms
        }
    }

    /// Passages for a question. When no term matches (a question with only
    /// common words, "summarize everything"), the most recent recordings'
    /// notes and opening lines stand in.
    static func retrieve(_ sources: [ChatSource], query: String, budgetChars: Int) -> [ChatPassage] {
        let all = passages(from: sources)
        let ranked = rank(all, query: query).map(\.passage)
        if !ranked.isEmpty { return select(ranked, budgetChars: budgetChars) }
        let recent = all.sorted { a, b in
            if a.startedAt != b.startedAt { return a.startedAt > b.startedAt }
            if a.kind != b.kind { return a.kind == .notes }
            return a.ms < b.ms
        }
        return select(recent, budgetChars: budgetChars, maxPerMeeting: 2)
    }

    /// Characters of passages that fit beside the prompt and the answer.
    static func budgetChars(contextTokens: Int, maxTokens: Int, fixed: String) -> Int {
        MeetingAI.bodyBudget(contextTokens: contextTokens, maxTokens: maxTokens, fixed: fixed)
    }
}
