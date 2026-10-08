import Foundation
import SwiftData

// Chat with your recordings (docs/TOPICS_AND_CHAT.md). Threads and messages
// live only in the app's store. An answer carries the passages it cited
// (`ChatCitation`, with an excerpt of transcript or notes text), so a
// Delete, Strike or Delete Recording purges the messages that cite that
// recording and flags the thread (docs/REDACTION.md).

/// What a question is asked about. Same four choices as the Mac:
/// All recordings · this Notebook · this Topic · this recording.
enum ChatScope: Equatable, Hashable, Sendable {
    case all
    case notebook(String)
    case topic(key: String, label: String)
    case recording(id: UUID, title: String)

    static let allLabel = "All recordings"

    var kind: String {
        switch self {
        case .all: "all"
        case .notebook: "notebook"
        case .topic: "topic"
        case .recording: "recording"
        }
    }

    var value: String? {
        switch self {
        case .all: nil
        case .notebook(let n): n
        case .topic(let key, _): key
        case .recording(let id, _): id.uuidString
        }
    }

    /// Shown with every answer: "All recordings", "Notebook · BIO 101", …
    var label: String {
        switch self {
        case .all: Self.allLabel
        case .notebook(let n): "\(Notebook.label) · \(n)"
        case .topic(_, let l): "\(Topic.label) · \(l)"
        case .recording(_, let t): "Recording · \(t)"
        }
    }

    /// Short form for the scope button
    var shortLabel: String {
        switch self {
        case .all: Self.allLabel
        case .notebook(let n): n
        case .topic(_, let l): l
        case .recording(_, let t): t
        }
    }

    var systemImage: String {
        switch self {
        case .all: "rectangle.stack"
        case .notebook: "book.closed"
        case .topic: "tag"
        case .recording: "waveform"
        }
    }

    /// The recordings this scope covers (a Topic scope uses the index's
    /// merged groups, so near-duplicate spellings count).
    @MainActor func filter(_ meetings: [Meeting], topics: TopicIndex) -> [Meeting] {
        switch self {
        case .all: return meetings
        case .notebook(let n): return meetings.filter { Notebook.matches($0.courseName, filter: n) }
        case .topic(let key, _):
            let ids = topics.meetingIDs(forKey: key)
            return meetings.filter { ids.contains($0.id) }
        case .recording(let id, _): return meetings.filter { $0.id == id }
        }
    }

    /// Rebuild a stored scope; a recording scope needs the title from the store.
    static func stored(kind: String, value: String?, label: String) -> ChatScope {
        switch kind {
        case "notebook": return .notebook(value ?? "")
        case "topic": return .topic(key: value ?? "", label: label)
        case "recording":
            guard let v = value, let id = UUID(uuidString: v) else { return .all }
            return .recording(id: id, title: label)
        default: return .all
        }
    }
}

/// One passage an answer cited. `timestamp` is the moment in the recording
/// (wall clock, like `Segment.start`), so a tap opens the transcript there.
struct ChatCitation: Codable, Hashable, Sendable, Identifiable {
    /// The [n] in the answer
    var n: Int
    var meetingID: UUID
    var title: String
    var timestamp: Date
    /// Seconds from the recording's start (for the chip)
    var offset: Double
    /// Transcript / Notes / Marker
    var kind: String
    var excerpt: String

    var id: Int { n }
}

@Model
final class ChatThread {
    @Attribute(.unique) var id: UUID
    var title: String
    var createdAt: Date
    var updatedAt: Date
    /// `ChatScope.kind` / `.value` / `.shortLabel` at the thread's start
    var scopeKind: String
    var scopeValue: String?
    var scopeLabel: String
    /// A recording this thread cited was deleted or edited; `flagNote` says so
    var flagged: Bool = false
    var flagNote: String?
    @Relationship(deleteRule: .cascade, inverse: \ChatThreadMessage.thread) var messages: [ChatThreadMessage] = []

    init(id: UUID = UUID(), scope: ChatScope, createdAt: Date = .now) {
        self.id = id
        self.title = "New chat"
        self.createdAt = createdAt
        self.updatedAt = createdAt
        self.scopeKind = scope.kind
        self.scopeValue = scope.value
        self.scopeLabel = scope.shortLabel
    }

    var scope: ChatScope { ChatScope.stored(kind: scopeKind, value: scopeValue, label: scopeLabel) }
    var orderedMessages: [ChatThreadMessage] { messages.sorted { ($0.createdAt, $0.id.uuidString) < ($1.createdAt, $1.id.uuidString) } }
}

/// One turn. `citationsJSON` is `[ChatCitation]` for an assistant message.
@Model
final class ChatThreadMessage {
    enum Role: String { case user, assistant }

    @Attribute(.unique) var id: UUID
    var role: String
    var content: String
    var createdAt: Date
    /// The scope the question was answered in ("Notebook · BIO 101")
    var scopeLabel: String?
    var citationsJSON: String?
    var thread: ChatThread?

    init(id: UUID = UUID(), role: Role, content: String, scopeLabel: String? = nil, citations: [ChatCitation] = [], createdAt: Date = .now) {
        self.id = id
        self.role = role.rawValue
        self.content = content
        self.createdAt = createdAt
        self.scopeLabel = scopeLabel
        self.citations = citations
    }

    var isUser: Bool { role == Role.user.rawValue }

    var citations: [ChatCitation] {
        get {
            guard let data = citationsJSON?.data(using: .utf8) else { return [] }
            return (try? JSONDecoder().decode([ChatCitation].self, from: data)) ?? []
        }
        set {
            citationsJSON = newValue.isEmpty ? nil : (try? JSONEncoder().encode(newValue)).flatMap { String(data: $0, encoding: .utf8) }
        }
    }

    func cites(_ meetingID: UUID) -> Bool { citations.contains { $0.meetingID == meetingID } }
}

/// Threads, turns, memory and the purge.
@MainActor
enum ChatStore {
    /// Turns (user + assistant pairs) sent back as conversation memory
    static let memoryTurns = 8
    static let maxQuestionLength = 2000

    static func newThread(scope: ChatScope, context: ModelContext) -> ChatThread {
        let t = ChatThread(scope: scope)
        context.insert(t)
        try? context.save()
        return t
    }

    static func append(_ message: ChatThreadMessage, to thread: ChatThread, context: ModelContext) {
        context.insert(message)
        message.thread = thread
        thread.updatedAt = message.createdAt
        if thread.title == "New chat", message.isUser {
            thread.title = Self.title(from: message.content)
        }
        try? context.save()
    }

    /// First line of the first question, ≤ 48 characters
    static func title(from question: String) -> String {
        let line = question.split(whereSeparator: \.isNewline).first.map(String.init) ?? question
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.isEmpty { return "New chat" }
        return t.count > 48 ? String(t.prefix(47)).trimmingCharacters(in: .whitespaces) + "…" : t
    }

    /// The last `memoryTurns` turns as plain messages, oldest first, without
    /// the citations (the model gets passages fresh each time).
    static func memory(_ thread: ChatThread, turns: Int = memoryTurns) -> [ChatMessage] {
        let msgs = thread.orderedMessages
        return Array(msgs.suffix(turns * 2)).map { ChatMessage(role: $0.isUser ? "user" : "assistant", content: $0.content) }
    }

    static func delete(_ thread: ChatThread, context: ModelContext) {
        context.delete(thread)
        try? context.save()
    }

    /// Purge for a recording (deleted, or its transcript edited): every
    /// assistant message citing it goes (answers quote the transcript), the
    /// thread is flagged, and a thread scoped to that recording is flagged
    /// too. User questions are the user's own words and stay. Returns how
    /// many messages were deleted. Doesn't save: the caller's transaction does.
    @discardableResult
    static func purge(meetingID: UUID, title: String?, deleted: Bool, context: ModelContext) -> Int {
        let threads = (try? context.fetch(FetchDescriptor<ChatThread>())) ?? []
        var removed = 0
        let what = title.map { "“\($0)”" } ?? "A recording"
        for thread in threads {
            var hit = false
            for m in thread.messages where !m.isUser && m.cites(meetingID) {
                context.delete(m)
                removed += 1
                hit = true
            }
            if thread.scopeKind == "recording", thread.scopeValue == meetingID.uuidString {
                hit = true
            }
            if hit {
                thread.flagged = true
                thread.flagNote = deleted
                    ? "\(what) was deleted. Answers that cited it were removed."
                    : "\(what) was edited. Answers that cited it were removed; ask again for a fresh answer."
            }
        }
        return removed
    }
}
