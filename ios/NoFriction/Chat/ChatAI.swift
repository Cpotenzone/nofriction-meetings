import Foundation

/// Chat prompts, the answer call and citation mapping. Requests go through
/// `AIClient`; the caller has done the Pro and consent checks (`ChatView`).
extension MeetingAI {
    static let chatSystem = """
        You answer the user's questions about their own recordings (meetings, classes and personal recordings) \
        using only the PASSAGES given: transcript excerpts (from speech recognition, with no speaker labels and \
        possible recognition errors), notes and the user's marker notes. Each passage starts with its number in \
        brackets, the recording's title, date and the time in the recording. Answer in Markdown, short and \
        concrete. After each statement, cite the passage it comes from as [n] (several as [1][3]). Use only the \
        passages; never invent names, numbers, dates or decisions. If the passages don't answer the question, say \
        what they do cover and that the rest wasn't found in the recordings. Text shown as \
        [stricken from the record] was removed by the user: never guess at it. Earlier turns of this conversation \
        may be given; their facts came from passages shown then.
        """

    static func chatMaxTokens(contextTokens: Int) -> Int { min(900, max(256, contextTokens / 8)) }

    static func chatPassagesBlock(_ passages: [ChatPassage]) -> String {
        passages.enumerated().map { i, p in "\(p.header(n: i + 1))\n\(p.text)" }.joined(separator: "\n\n")
    }

    static func chatUserMessage(question: String, scopeLabel: String, passages: [ChatPassage]) -> String {
        let block = passages.isEmpty ? "(no matching passages)" : chatPassagesBlock(passages)
        return "SCOPE: \(scopeLabel)\n\nPASSAGES:\n\(block)\n\nQUESTION: \(question)"
    }

    struct ChatAnswer: Equatable, Sendable {
        var text: String
        var citations: [ChatCitation]
    }

    enum ChatFailure: LocalizedError, Equatable {
        case nothingToSearch
        var errorDescription: String? { "There's nothing to ask about in this scope yet: record something first, or widen the scope." }
    }

    /// One answer: retrieve passages for the question from `sources`, send
    /// them with the conversation memory, map the [n] marks to citations.
    static func chat(question: String, scopeLabel: String, sources: [ChatSource], memory: [ChatMessage],
                     contextTokens: Int, complete: Complete) async throws -> ChatAnswer {
        guard !sources.isEmpty else { throw ChatFailure.nothingToSearch }
        let maxTokens = chatMaxTokens(contextTokens: contextTokens)
        let memoryChars = memory.reduce(0) { $0 + $1.content.count }
        let fixed = chatSystem + chatUserMessage(question: question, scopeLabel: scopeLabel, passages: []) + String(repeating: " ", count: memoryChars)
        let budget = ChatRetrieval.budgetChars(contextTokens: contextTokens, maxTokens: maxTokens, fixed: fixed)
        let passages = ChatRetrieval.retrieve(sources, query: question, budgetChars: budget)
        var messages = [ChatMessage(role: "system", content: chatSystem)]
        messages += memory
        messages.append(ChatMessage(role: "user", content: chatUserMessage(question: question, scopeLabel: scopeLabel, passages: passages)))
        let raw = try await complete(messages, maxTokens, 0.2)
        let text = StudyParse.stripThinking(raw).trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { throw AIError.emptyAnswer }
        return ChatAnswer(text: text, citations: citations(in: text, passages: passages))
    }

    /// `[n]` marks in the answer → the passages they point at, in order of
    /// first mention, each once; numbers with no passage are ignored.
    static func citations(in text: String, passages: [ChatPassage]) -> [ChatCitation] {
        var seen = Set<Int>()
        var out: [ChatCitation] = []
        let chars = Array(text)
        var i = 0
        while i < chars.count {
            if chars[i] == "[" {
                var j = i + 1
                var digits = ""
                while j < chars.count, chars[j].isNumber { digits.append(chars[j]); j += 1 }
                if j < chars.count, chars[j] == "]", let n = Int(digits), n >= 1, n <= passages.count, seen.insert(n).inserted {
                    let p = passages[n - 1]
                    out.append(ChatCitation(n: n, meetingID: p.meetingID, title: p.title, timestamp: p.timestamp,
                                            offset: Double(p.ms) / 1000, kind: p.kind.rawValue, excerpt: excerpt(p.text)))
                }
                i = max(j, i + 1)
            } else {
                i += 1
            }
        }
        return out
    }

    static func excerpt(_ text: String) -> String {
        let one = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return one.count > ChatRetrieval.excerptLength ? String(one.prefix(ChatRetrieval.excerptLength - 1)).trimmingCharacters(in: .whitespaces) + "…" : one
    }
}

/// Questions offered on an empty chat, from the scope's titles and topics.
/// No AI call.
enum ChatSuggestions {
    struct Recording: Equatable, Sendable {
        var title: String
        var kind: RecordingKind
        var startedAt: Date
    }

    static let max = 4

    static func questions(scope: ChatScope, recordings: [Recording], topics: [String]) -> [String] {
        let recent = recordings.sorted { $0.startedAt > $1.startedAt }
        var out: [String] = []
        func add(_ q: String) { if !out.contains(q), out.count < max { out.append(q) } }
        switch scope {
        case .recording(_, let title):
            let kind = recent.first { $0.title == title }?.kind ?? .meeting
            add(kind == .class ? "What were the key concepts in “\(title)”?" : "What was decided in “\(title)”?")
            add(kind == .class ? "What did the instructor say will be on the test?" : "What are the action items from “\(title)”?")
            for t in topics.prefix(2) { add("What was said about \(t)?") }
            add("What questions were left open?")
        case .topic(_, let label):
            add("Summarize what was said about \(label).")
            add("What was decided about \(label), and when?")
            if let r = recent.first { add("What did “\(r.title)” say about \(label)?") }
            add("What's still open about \(label)?")
        case .notebook(let name):
            add("What are the main themes in \(name) so far?")
            for t in topics.prefix(2) { add("What was said about \(t) in \(name)?") }
            if let r = recent.first {
                add(r.kind == .class ? "What did the last class in \(name) cover?" : "What was decided in “\(r.title)”?")
            }
            add("What deadlines were mentioned in \(name)?")
        case .all:
            for t in topics.prefix(2) { add("What was said about \(t)?") }
            if let r = recent.first {
                add(r.kind == .class ? "What did “\(r.title)” cover?" : "What was decided in “\(r.title)”?")
            }
            add("What action items are still open?")
            add("What deadlines are coming up?")
        }
        return out
    }
}
