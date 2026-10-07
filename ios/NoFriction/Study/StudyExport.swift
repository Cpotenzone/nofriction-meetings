import CoreTransferable
import Foundation
import UniformTypeIdentifiers

/// Exports: flashcards as CSV (Anki and Quizlet import it) and the review
/// guide as Markdown, shared through the share sheet. Same output as the
/// Mac (`src-tauri/src/study/export.rs`).
enum StudyExport {
    /// One RFC 4180 field: always quoted, inner quotes doubled, line breaks
    /// and tabs as spaces (one card per line), and a leading apostrophe on
    /// anything a spreadsheet would run as a formula.
    static func csvField(_ s: String) -> String {
        let flat = String(s.map { $0 == "\r" || $0 == "\n" || $0 == "\r\n" || $0 == "\t" ? " " : $0 })
            .trimmingCharacters(in: .whitespaces)
        var risky = false
        if let f = flat.first {
            if f == "=" || f == "+" || f == "@" {
                risky = true
            } else if f == "-" {
                let next = flat.dropFirst().first
                risky = !(next.map { $0.isASCII && $0.isNumber } ?? false) && next != "." && next != " "
            }
        }
        let body = risky ? "'" + flat : flat
        return "\"" + body.replacingOccurrences(of: "\"", with: "\"\"") + "\""
    }

    /// `front,back` rows, CRLF line ends, no header row.
    static func flashcardsCSV(_ cards: [StudyCards.Card]) -> String {
        cards.map { csvField($0.front) + "," + csvField($0.back) + "\r\n" }.joined()
    }

    /// Model text inside Markdown: escape everything that could become markup.
    static func mdEscape(_ s: String) -> String {
        var out = ""
        for c in s {
            switch c {
            case "\\", "`", "*", "_", "[", "]", "<", ">", "#", "|", "!", "~":
                out.append("\\")
                out.append(c)
            case "\r", "\n", "\r\n":
                out.append(" ")
            default:
                out.append(c)
            }
        }
        return out
    }

    struct Mark: Equatable {
        var ms: Int
        var kind: MarkerKind
        var note: String?
    }

    /// "# Study guide: …" for a Class, "# Review guide: …" otherwise; the ✎
    /// marker's label follows the type.
    static func guideMarkdown(title: String, kind: RecordingKind = .meeting, when: String, summary: StudySummary?,
                              terms: StudyTerms?, cards: StudyCards?, quiz: StudyQuiz?, asks: StudyAsks?,
                              marks: [Mark]) -> String {
        var out = "# \(kind.guideTitle): \(mdEscape(title))\n\n"
        if !when.isEmpty { out += "\(mdEscape(when))\n\n" }
        if let summary {
            out += "## Summary\n\n"
            if let t = summary.title { out += "*\(mdEscape(t))*\n\n" }
            for s in summary.sections {
                out += "### \(mdEscape(s.heading))\n\n"
                for b in s.bullets { out += "- \(mdEscape(b))\n" }
                out += "\n"
            }
        }
        if let terms {
            out += "## Key terms\n\n"
            for t in terms.terms { out += "- **\(mdEscape(t.term))**: \(mdEscape(t.definition))\n" }
            out += "\n"
        }
        if !marks.isEmpty {
            out += "## Marked moments\n\n"
            for m in marks {
                out += "- \(StudyParse.clock(m.ms)) \(m.kind.symbol) \(m.kind.label(for: kind))"
                if let n = m.note?.trimmingCharacters(in: .whitespacesAndNewlines), !n.isEmpty { out += ": \(mdEscape(n))" }
                out += "\n"
            }
            out += "\n"
        }
        let confused = marks.filter { $0.kind == .question }
        if !(asks?.questions.isEmpty ?? true) || !confused.isEmpty {
            out += "## Questions to ask\n\n"
            for q in asks?.questions ?? [] {
                out += "- \(mdEscape(q.question))" + (q.atMs.map { " (\(StudyParse.clock($0)))" } ?? "") + "\n"
            }
            for m in confused {
                let note = m.note?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                out += "- You marked \(StudyParse.clock(m.ms)) as confusing" + (note.isEmpty ? "" : ": \(mdEscape(note))") + "\n"
            }
            out += "\n"
        }
        if let cards, !cards.cards.isEmpty {
            out += "## Flashcards\n\n"
            for c in cards.cards { out += "- **\(mdEscape(c.front))**: \(mdEscape(c.back))\n" }
            out += "\n"
        }
        if let quiz, !quiz.questions.isEmpty {
            out += "## Practice quiz\n\n"
            for (i, q) in quiz.questions.enumerated() {
                out += "\(i + 1). \(mdEscape(q.question))\n"
                for (j, c) in q.choices.enumerated() { out += "   - \(letter(j))) \(mdEscape(c))\n" }
            }
            out += "\n### Answer key\n\n"
            for (i, q) in quiz.questions.enumerated() {
                var line = "\(i + 1). \(letter(q.answer))"
                if !q.explanation.isEmpty { line += ": \(mdEscape(q.explanation))" }
                if let at = q.atMs { line += " (\(StudyParse.clock(at)))" }
                out += line + "\n"
            }
            out += "\n"
        }
        out += kind == .class
            ? "_Made with noFriction from the lecture transcript. AI can make mistakes; check against the lecture._\n"
            : "_Made with noFriction from the transcript. AI can make mistakes; check against the recording._\n"
        return out
    }

    static func letter(_ i: Int) -> String { String(UnicodeScalar(UInt8(65 + min(max(i, 0), 25)))) }

    /// "Biology 101: Cells" → "Biology 101 Cells"
    static func fileStem(_ title: String) -> String {
        let s = String(title.map { $0.isLetter || $0.isNumber || $0 == " " || $0 == "-" || $0 == "_" ? $0 : " " })
            .split(separator: " ").joined(separator: " ")
        let stem = String(s.prefix(60))
        return stem.isEmpty ? "Recording" : stem
    }
}

/// A file for the share sheet, made in memory (no copy is left in the app's
/// folders to purge later).
struct StudyExportFile: Transferable {
    let name: String
    let data: Data
    let type: UTType

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(exportedContentType: .commaSeparatedText) { $0.data }
            .suggestedFileName { $0.name }
            .exportingCondition { $0.type == .commaSeparatedText }
        DataRepresentation(exportedContentType: UTType(filenameExtension: "md") ?? .plainText) { $0.data }
            .suggestedFileName { $0.name }
            .exportingCondition { $0.type != .commaSeparatedText }
    }

    static func csv(_ cards: [StudyCards.Card], title: String) -> StudyExportFile {
        StudyExportFile(name: "\(StudyExport.fileStem(title)) flashcards.csv",
                        data: Data(StudyExport.flashcardsCSV(cards).utf8), type: .commaSeparatedText)
    }

    static func markdown(_ text: String, title: String, kind: RecordingKind = .meeting) -> StudyExportFile {
        StudyExportFile(name: "\(StudyExport.fileStem(title)) \(kind.guideTitle.lowercased()).md", data: Data(text.utf8),
                        type: UTType(filenameExtension: "md") ?? .plainText)
    }
}

// MARK: - Flashcard and quiz state (pure; unit-tested)

/// A pass over cards 0..<n; "Again" cards come back next round until all are known.
struct FlashcardDeckState: Equatable {
    var order: [Int]
    var pos = 0
    var flipped = false
    var known: [Int] = []
    var again: [Int] = []
    var round = 1

    init(count: Int) { order = Array(0..<max(0, count)) }

    var isDone: Bool { pos >= order.count }
    var current: Int? { isDone ? nil : order[pos] }

    mutating func flip() { if !isDone { flipped.toggle() } }

    mutating func mark(known isKnown: Bool) {
        guard let card = current else { return }
        known.removeAll { $0 == card }
        if isKnown { known.append(card) } else { again.append(card) }
        pos += 1
        flipped = false
        if pos >= order.count, !again.isEmpty {
            order = again
            again = []
            pos = 0
            round += 1
        }
    }

    mutating func shuffle<G: RandomNumberGenerator>(using g: inout G) {
        guard !isDone else { return }
        order = Array(order[..<pos]) + order[pos...].shuffled(using: &g)
        flipped = false
    }
}

struct QuizRunState: Equatable {
    var pos = 0
    var picked: [Int?]
    var finished: Bool

    init(count: Int) {
        picked = Array(repeating: nil, count: max(0, count))
        finished = count <= 0
    }

    mutating func pick(_ choice: Int, in qs: [StudyQuiz.Item]) {
        guard !finished, pos < qs.count, picked[pos] == nil, qs[pos].choices.indices.contains(choice) else { return }
        picked[pos] = choice
    }

    mutating func next(in qs: [StudyQuiz.Item]) {
        guard !finished, pos < picked.count, picked[pos] != nil else { return }
        if pos + 1 >= qs.count { finished = true } else { pos += 1 }
    }

    func score(in qs: [StudyQuiz.Item]) -> (correct: Int, total: Int) {
        (zip(qs, picked).filter { $0.1 == $0.0.answer }.count, qs.count)
    }
}
