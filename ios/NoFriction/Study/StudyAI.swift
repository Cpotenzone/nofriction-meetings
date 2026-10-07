import CryptoKit
import Foundation

/// The prompt input for a meeting: the transcript as notes and email use it
/// (lines already filtered at transcription time; filtered text is never
/// stored, so it can't be fed back), stricken spans as
/// `[stricken from the record]`, deleted words gone, each line with its time
/// in the lecture, plus the moment markers. Built on the main actor (SwiftData
/// models aren't Sendable); the value itself is Sendable.
struct StudyInput: Sendable, Equatable {
    struct Line: Sendable, Equatable { var ms: Int; var text: String }
    struct Mark: Sendable, Equatable { var ms: Int; var kind: MarkerKind; var note: String? }

    var title: String
    /// The recording's class ("BIO 101"), when it has one
    var courseName: String?
    var durationMs: Int
    var lines: [Line]
    var marks: [Mark]

    init(title: String, courseName: String? = nil, durationMs: Int, lines: [Line], marks: [Mark]) {
        self.title = title
        self.courseName = courseName
        self.durationMs = durationMs
        self.lines = lines
        self.marks = marks
    }

    @MainActor init(meeting m: Meeting) {
        let ms = { (d: Date) in max(0, Int((d.timeIntervalSince(m.startedAt) * 1000).rounded(.down))) }
        lines = m.orderedSegments.map { Line(ms: ms($0.start), text: RedactionText.plain($0.text)) }
        marks = m.orderedMarkers.map { Mark(ms: ms($0.at), kind: $0.markerKind, note: $0.note) }
        title = m.title
        courseName = m.courseName
        let last = (lines.map(\.ms) + marks.map(\.ms)).max() ?? 0
        durationMs = max(Int((m.duration ?? 0) * 1000), last)
    }

    /// `[m:ss] text`, consecutive strike placeholders once.
    var transcriptLines: [String] {
        var out: [String] = []
        var lastPlaceholder = false
        for l in lines {
            let t = l.text.trimmingCharacters(in: .whitespacesAndNewlines)
            if t.isEmpty { continue }
            let placeholder = t == RedactionText.placeholder
            if placeholder && lastPlaceholder { continue }
            lastPlaceholder = placeholder
            out.append("[\(StudyParse.clock(l.ms))] \(t)")
        }
        return out
    }

    var hasTranscript: Bool {
        lines.contains { let t = $0.text.trimmingCharacters(in: .whitespacesAndNewlines); return !t.isEmpty && t != RedactionText.placeholder }
    }

    func marksBlock(from: Int = .min, to: Int = .max) -> String {
        let out = marks.filter { $0.ms >= from && $0.ms <= to }.map { m -> String in
            var s = "[\(StudyParse.clock(m.ms))] \(m.kind.symbol) \(m.kind.label)"
            if let n = m.note?.trimmingCharacters(in: .whitespacesAndNewlines), !n.isEmpty { s += ": \(n)" }
            return s
        }
        return out.isEmpty ? "(none)" : out.joined(separator: "\n")
    }

    /// SHA-256 of the transcript exactly as the prompt shows it.
    var fingerprint: String {
        let data = Data(transcriptLines.map { $0 + "\n" }.joined().utf8)
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

/// Study guide prompts and generation (same prompts and rules as the Mac,
/// `src-tauri/src/study/prompt.rs`). Requests go through `AIClient` — Apple
/// on-device or the user's endpoint — with the caller's consent and Pro
/// checks already done (MeetingDetailView.requestAI).
extension MeetingAI {
    static let studyBase = """
        You turn a lecture transcript into study material for a student. The transcript comes from speech \
        recognition: it has no speaker labels and may contain recognition errors; don't repeat obvious errors. \
        Each line starts with its time in the lecture as [m:ss]. Use only what the lecture says; never invent \
        facts, names, numbers, dates or examples. Text shown as [stricken from the record] was removed by the \
        student: never guess at or mention what it said. The student marked some moments while listening \
        (STUDENT MARKS): moments marked ✎ On the test matter most, then ★ Important; make sure they are covered. \
        Reply with only one JSON object: no Markdown, no code fence, no text before or after it.
        """

    static func studySystem(_ kind: StudyKind) -> String {
        let task: String
        switch kind {
        case .summary:
            task = #"Write lecture notes: the main topics in the order they were taught, with the definitions, steps, examples and formulas given. 3 to 8 sections, 2 to 6 short bullets each. Shape: {"title": "short lecture title", "sections": [{"heading": "topic", "bullets": ["point", "point"]}]}"#
        case .terms:
            task = #"List the key terms the lecture introduced or relied on, each with a definition of one or two sentences taken from the lecture. 5 to 20 terms. Shape: {"terms": [{"term": "term", "definition": "definition"}]}"#
        case .flashcards:
            task = #"Write flashcards for studying: one fact, definition or step per card, the front a question or term, the back a short answer. 8 to 25 cards; cover the marked moments first. Shape: {"cards": [{"front": "question", "back": "answer"}]}"#
        case .quiz:
            task = #"Write a multiple-choice practice quiz: 5 to 10 questions, each with 4 choices and exactly one correct choice. "answer" is the 0-based index of the correct choice. "explanation" is one line saying why. "time" is the [m:ss] time of the transcript line the answer comes from, without brackets. Shape: {"questions": [{"question": "question", "choices": ["a", "b", "c", "d"], "answer": 0, "explanation": "why", "time": "12:34"}]}"#
        case .questions:
            task = #"Write questions the student could ask the instructor: points the lecture left unclear or skipped, and the moments marked ? Question (use the student's note when there is one). 3 to 8 questions. "time" is the [m:ss] time the question is about, without brackets. Shape: {"questions": [{"question": "question", "time": "12:34"}]}"#
        }
        return studyBase + "\n\n" + task
    }

    static let condenseSystem = """
        You condense part of a lecture transcript into study notes. The transcript comes from speech recognition \
        and may contain errors. Each line starts with its time as [m:ss]. Write at most 15 short lines. Start every \
        line with the [m:ss] time it comes from. Keep definitions, key terms, steps, examples, formulas, anything \
        the lecturer stresses ("this will be on the exam"), points that sound unclear, and everything said near \
        the STUDENT MARKS. Use only what the transcript says. Text shown as [stricken from the record] was removed \
        by the student: never guess at or mention what it said. Plain text lines only.
        """

    static func studyHeader(_ input: StudyInput) -> String {
        let title = input.title.trimmingCharacters(in: .whitespacesAndNewlines)
        var s = "Lecture: \(title.isEmpty ? "Untitled" : title)\n"
        if let c = input.courseName?.trimmingCharacters(in: .whitespacesAndNewlines), !c.isEmpty { s += "Class: \(c)\n" }
        if input.durationMs > 0 { s += "Length: \(StudyParse.clock(input.durationMs))\n" }
        return s
    }

    static func studyUserMessage(_ input: StudyInput, condensed: Bool, body: String) -> String {
        "\(studyHeader(input))\nSTUDENT MARKS:\n\(input.marksBlock())\n\n"
            + (condensed ? "LECTURE NOTES (condensed from the transcript, with times)" : "TRANSCRIPT") + ":\n\(body)"
    }

    static func condenseMessage(_ input: StudyInput, chunk: String, part: Int, parts: Int) -> String {
        var marks = "(none)"
        if let span = chunkSpan(chunk) { marks = input.marksBlock(from: span.0 - 5_000, to: span.1 + 30_000) }
        return "\(studyHeader(input))Part \(part) of \(parts)\n\nSTUDENT MARKS:\n\(marks)\n\nTRANSCRIPT:\n\(chunk)"
    }

    static func retryNote(_ kind: StudyKind, why: String) -> String {
        "Your previous answer for the \(kind.noun) couldn't be used (\(why)). Answer again with only the JSON object in the shape described, and nothing else."
    }

    // MARK: Budgets

    static func studyMaxTokens(_ kind: StudyKind, contextTokens: Int) -> Int {
        let want: Int
        switch kind {
        case .summary: want = 1600
        case .terms: want = 1200
        case .flashcards: want = 1600
        case .quiz: want = 2000
        case .questions: want = 800
        }
        return min(want, max(256, contextTokens * 3 / 10))
    }

    static func condenseMaxTokens(contextTokens: Int) -> Int { min(700, max(200, contextTokens / 5)) }

    /// Characters of material that fit beside the fixed text and the answer.
    static func bodyBudget(contextTokens: Int, maxTokens: Int, fixed: String) -> Int {
        let fixedTokens = Int((Double(fixed.count) / ContextFit.charsPerToken).rounded(.up))
        let avail = max(0, contextTokens - maxTokens - ContextFit.reserveTokens - fixedTokens)
        return max(600, Int(Double(avail) * ContextFit.charsPerToken * 0.85))
    }

    /// Whole lines into chunks of at most `budget` characters.
    static func chunkLines(_ lines: [String], budget: Int) -> [String] {
        let budget = max(200, budget)
        var out: [String] = []
        var cur = ""
        for l in lines {
            if l.count > budget {
                if !cur.isEmpty { out.append(cur); cur = "" }
                var rest = Substring(l)
                while !rest.isEmpty { out.append(String(rest.prefix(budget))); rest = rest.dropFirst(budget) }
                continue
            }
            if !cur.isEmpty && cur.count + l.count + 1 > budget { out.append(cur); cur = "" }
            cur += (cur.isEmpty ? "" : "\n") + l
        }
        if !cur.isEmpty { out.append(cur) }
        return out
    }

    static func chunkSpan(_ chunk: String) -> (Int, Int)? {
        let times = chunk.split(whereSeparator: \.isNewline).compactMap { line -> Int? in
            guard line.hasPrefix("["), let end = line.firstIndex(of: "]") else { return nil }
            return StudyParse.parseClock(String(line[...end]))
        }
        guard let a = times.min(), let b = times.max() else { return nil }
        return (a, b)
    }

    // MARK: Generation

    struct StudyProgress: Sendable, Equatable {
        var done: Int
        var total: Int
        var label: String
    }

    /// One completion. Tests pass a mock; the app passes `AIClient.shared`.
    typealias Complete = @Sendable ([ChatMessage], Int, Double) async throws -> String

    static func liveComplete(_ endpoint: AIEndpoint) -> Complete {
        { messages, maxTokens, temperature in
            try await AIClient.shared.complete(messages, maxTokens: maxTokens, temperature: temperature, endpoint: endpoint)
        }
    }

    /// Errors about the answer itself (retried, then reported for that part).
    /// Everything else (consent, configuration, network, key) stops the run.
    static func isAnswerError(_ e: Error) -> Bool {
        guard let e = e as? AIError else { return false }
        switch e {
        case .emptyAnswer, .truncated, .badResponse, .refused: return true
        default: return false
        }
    }

    /// One part couldn't be made (its answer was unusable twice); the others go on.
    struct StudyPartFailure: LocalizedError, Equatable, Sendable {
        let kind: StudyKind
        let why: String
        var errorDescription: String? {
            "Couldn't make the \(kind.noun): \(why) (asked twice). Try again, or choose a larger model in Settings."
        }
    }

    enum StudyFailure: LocalizedError {
        case noTranscript
        case condense(part: Int, of: Int)
        var errorDescription: String? {
            switch self {
            case .noTranscript: "This recording has no transcript to make a study guide from."
            case .condense(let p, let n): "Couldn't condense part \(p) of \(n) of the lecture (asked twice). Try again, or choose a model with a larger context window."
            }
        }
    }

    /// Make the requested parts. Returns, per part, the validated JSON or a
    /// message saying why it couldn't be made (asked twice). AI access
    /// errors throw and stop the run.
    static func studyGuide(_ input: StudyInput, kinds: [StudyKind] = StudyKind.allCases, contextTokens: Int,
                           complete: Complete, progress: @Sendable (StudyProgress) async -> Void = { _ in }) async throws
        -> [(StudyKind, Result<String, StudyPartFailure>)] {
        guard input.hasTranscript else { throw StudyFailure.noTranscript }
        var total = kinds.count
        var done = 0

        // Fit: condense chunk by chunk until the material fits the smallest budget
        let fixed = studyUserMessage(input, condensed: true, body: "")
        let finalBudget = kinds.map { bodyBudget(contextTokens: contextTokens, maxTokens: studyMaxTokens($0, contextTokens: contextTokens), fixed: studySystem($0) + fixed) }.min() ?? 600
        var lines = input.transcriptLines
        var condensed = false
        for round in 0..<3 {
            if lines.reduce(0, { $0 + $1.count + 1 }) <= finalBudget { break }
            let cmax = condenseMaxTokens(contextTokens: contextTokens)
            let chunkBudget = bodyBudget(contextTokens: contextTokens, maxTokens: cmax,
                                         fixed: condenseSystem + condenseMessage(input, chunk: "", part: 99, parts: 99) + input.marksBlock())
            let chunks = chunkLines(lines, budget: chunkBudget)
            if round > 0 && chunks.count <= 1 && condensed { break }
            total += chunks.count
            var notes: [String] = []
            for (i, chunk) in chunks.enumerated() {
                await progress(StudyProgress(done: done, total: total, label: "Reading the lecture (\(i + 1) of \(chunks.count))…"))
                let msgs = [ChatMessage(role: "system", content: condenseSystem),
                            ChatMessage(role: "user", content: condenseMessage(input, chunk: chunk, part: i + 1, parts: chunks.count))]
                var got: [String]?
                for attempt in 0..<2 {
                    do {
                        let raw = try await complete(msgs, cmax, attempt == 0 ? 0.2 : 0.3)
                        if let ls = try? StudyParse.condensedLines(raw, maxLines: 20) { got = ls; break }
                    } catch {
                        if !isAnswerError(error) { throw error }
                    }
                }
                guard let got else { throw StudyFailure.condense(part: i + 1, of: chunks.count) }
                notes += got
                done += 1
            }
            lines = notes
            condensed = true
        }

        let user = studyUserMessage(input, condensed: condensed, body: lines.joined(separator: "\n"))
        var out: [(StudyKind, Result<String, StudyPartFailure>)] = []
        for kind in kinds {
            await progress(StudyProgress(done: done, total: total, label: "Writing the \(kind.noun)…"))
            var why = ""
            var result: Result<String, StudyPartFailure>?
            for attempt in 0..<2 {
                let text = attempt == 0 ? user : user + "\n\n" + retryNote(kind, why: why)
                let raw: String
                do {
                    raw = try await complete([ChatMessage(role: "system", content: studySystem(kind)),
                                              ChatMessage(role: "user", content: text)],
                                             studyMaxTokens(kind, contextTokens: contextTokens), attempt == 0 ? 0.2 : 0.3)
                } catch {
                    if !isAnswerError(error) { throw error }
                    why = error.localizedDescription
                    continue
                }
                do {
                    result = .success(try StudyParse.validate(kind, raw: raw, durationMs: input.durationMs))
                    break
                } catch {
                    why = (error as? StudyParse.Failure)?.errorDescription ?? "the answer couldn't be used"
                }
            }
            out.append((kind, result ?? .failure(StudyPartFailure(kind: kind, why: why))))
            done += 1
        }
        await progress(StudyProgress(done: total, total: total, label: "Done"))
        return out
    }
}
