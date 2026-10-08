import Foundation
import SwiftData

// Topics (docs/TOPICS_AND_CHAT.md): what a recording was about, as 1–4
// short noun phrases. The AI names them when notes are made (and on
// demand); the user can rename, remove and add. Same vocabulary as the Mac:
// a "Topics" chip row beside Notebooks, "Group by: Date · Notebook · Topic".

/// One topic on one recording. AI topics are replaced by a re-run; user
/// topics (added or renamed by hand) are never touched by the AI. Deleted
/// with the recording (cascade); AI topics are also deleted when the
/// transcript is edited (Delete / Strike), see docs/REDACTION.md.
@Model
final class MeetingTopic {
    enum Source: String { case ai, user }

    @Attribute(.unique) var id: UUID
    /// Shown label ("Q4 roadmap")
    var label: String
    /// Normalized label (`Topic.key`), used to match topics across recordings
    var key: String
    /// 0…1; user topics are 1
    var confidence: Double
    /// `ai` | `user`
    var source: String
    var createdAt: Date
    var meeting: Meeting?

    init(id: UUID = UUID(), label: String, confidence: Double, source: Source, createdAt: Date = .now) {
        self.id = id
        self.label = label
        self.key = Topic.key(label)
        self.confidence = min(1, max(0, confidence))
        self.source = source.rawValue
        self.createdAt = createdAt
    }

    var isUser: Bool { source == Source.user.rawValue }
}

/// Topic vocabulary and the pure label / key rules (unit-tested).
enum Topic {
    static let label = "Topic"
    static let filterTitle = "Topics"
    static let groupByTitle = "Group by"
    static let maxLabelLength = 40
    static let maxLabelWords = 6
    /// Most AI topics per recording
    static let maxAI = 4
    /// Most topics per recording in total (AI + user)
    static let maxPerRecording = 8
    /// Shown on a row in the Recordings list
    static let chipsPerRow = 2
    /// AI topics below this confidence are dropped
    static let minConfidence = 0.3

    /// Labels that name nothing ("meeting", "discussion"): never a topic.
    static let generic: Set<String> = [
        "meeting", "meetings", "discussion", "conversation", "recording", "class", "lecture", "notes", "misc",
        "miscellaneous", "other", "general", "various", "various topics", "topics", "topic", "overview",
        "introduction", "summary", "update", "updates", "agenda", "call", "talk", "session", "chat", "personal",
    ]

    /// Only articles and connectives: dropped from keys so "the Q4 roadmap"
    /// and "Q4 roadmap" match. Content words always stay.
    static let keyStopwords: Set<String> = ["a", "an", "the", "of", "and", "for", "to", "in", "on", "at", "with", "about"]

    /// Trimmed, whitespace collapsed, control characters dropped, trailing
    /// punctuation and list markers removed, ≤ 40 characters. Empty → nil.
    static func normalizeLabel(_ input: String?) -> String? {
        guard let input else { return nil }
        var s = input.unicodeScalars
            .filter { !CharacterSet.controlCharacters.subtracting(.whitespacesAndNewlines).contains($0) }
            .map(String.init).joined()
            .split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        while let f = s.first, "-*•–#".contains(f) { s = String(s.dropFirst()).trimmingCharacters(in: .whitespaces) }
        s = s.trimmingCharacters(in: CharacterSet(charactersIn: " .,;:!\"'“”‘’"))
        if s.count > maxLabelLength {
            s = String(s.prefix(maxLabelLength)).trimmingCharacters(in: .whitespaces)
        }
        return s.isEmpty ? nil : s
    }

    /// Lowercased, diacritics folded, anything but letters and digits
    /// becomes a space, articles dropped, simple plurals singularized:
    /// "The Q4 Roadmaps" → "q4 roadmap". Empty labels give "".
    static func key(_ label: String) -> String {
        let folded = label.folding(options: [.diacriticInsensitive, .caseInsensitive, .widthInsensitive], locale: nil).lowercased()
        let scalars = folded.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) ? Character($0) : " " }
        let words = String(scalars).split(separator: " ").map(String.init).filter { !keyStopwords.contains($0) }
        let kept = words.isEmpty
            ? String(scalars).split(separator: " ").map(String.init)   // a label of only stopwords keeps them
            : words
        return kept.map(singular).joined(separator: " ")
    }

    /// "roadmaps" → "roadmap", "classes" → "class", "status" stays.
    static func singular(_ w: String) -> String {
        guard w.count > 3, w.hasSuffix("s"), !w.hasSuffix("ss"), !w.hasSuffix("us"), !w.hasSuffix("is") else { return w }
        if w.hasSuffix("ies"), w.count > 4 { return String(w.dropLast(3)) + "y" }
        if w.hasSuffix("sses") || w.hasSuffix("xes") || w.hasSuffix("ches") || w.hasSuffix("shes") { return String(w.dropLast(2)) }
        return String(w.dropLast())
    }

    /// Near-duplicate keys: equal, equal without spaces ("off site" ~
    /// "offsite"), the same words in another order, or one typo apart
    /// (keys of 8+ characters). "q4 roadmap" ~ "roadmap q4".
    static func similar(_ a: String, _ b: String) -> Bool {
        if a == b { return true }
        if a.isEmpty || b.isEmpty { return false }
        if a.replacingOccurrences(of: " ", with: "") == b.replacingOccurrences(of: " ", with: "") { return true }
        let wa = a.split(separator: " ").sorted(), wb = b.split(separator: " ").sorted()
        if wa == wb { return true }
        guard a.count >= 8, b.count >= 8, abs(a.count - b.count) <= 1 else { return false }
        return editDistance(a, b) <= 1
    }

    /// The existing key this one should join, or itself.
    static func canonicalKey(_ key: String, existing: [String]) -> String {
        existing.first { similar($0, key) } ?? key
    }

    static func isGeneric(_ label: String) -> Bool {
        let k = key(label)
        return k.isEmpty || generic.contains(k) || generic.contains(label.lowercased())
    }

    static func editDistance(_ a: String, _ b: String) -> Int {
        let x = Array(a), y = Array(b)
        if x.isEmpty { return y.count }
        if y.isEmpty { return x.count }
        var prev = Array(0...y.count)
        var cur = [Int](repeating: 0, count: y.count + 1)
        for i in 1...x.count {
            cur[0] = i
            for j in 1...y.count {
                cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x[i - 1] == y[j - 1] ? 0 : 1))
            }
            swap(&prev, &cur)
        }
        return prev[y.count]
    }
}

/// Topics across recordings, merged by near-duplicate key, for the chip
/// row, the Topic grouping and the Chat scope. Pure and Sendable: built from
/// plain values on the main actor, used anywhere.
struct TopicIndex: Equatable, Sendable {
    struct Entry: Equatable, Sendable {
        var meetingID: UUID
        var key: String
        var label: String
        var isUser: Bool
        var startedAt: Date
    }

    struct Group: Equatable, Sendable, Identifiable {
        /// The key of the group's first member (the canonical key)
        var key: String
        var label: String
        var meetingIDs: [UUID]
        /// Most recent recording in the group
        var latest: Date
        var id: String { key }
        var count: Int { meetingIDs.count }
    }

    /// Most recordings first, then the most recent, then by label
    private(set) var groups: [Group] = []

    init(entries: [Entry]) {
        // Distinct keys by how often they appear, so the commonest spelling leads a group
        var byKey: [String: [Entry]] = [:]
        for e in entries where !e.key.isEmpty { byKey[e.key, default: []].append(e) }
        let keys = byKey.keys.sorted { (byKey[$0]!.count, $1) > (byKey[$1]!.count, $0) }
        var out: [Group] = []
        for k in keys {
            let members = byKey[k]!
            if let i = out.firstIndex(where: { Topic.similar($0.key, k) }) {
                for m in members where !out[i].meetingIDs.contains(m.meetingID) { out[i].meetingIDs.append(m.meetingID) }
                out[i].latest = max(out[i].latest, members.map(\.startedAt).max() ?? .distantPast)
                // A user's spelling wins over the AI's
                if let user = members.first(where: \.isUser)?.label, !out[i].label.isEmpty,
                   !byKey[out[i].key]!.contains(where: \.isUser) { out[i].label = user }
            } else {
                var ids: [UUID] = []
                for m in members where !ids.contains(m.meetingID) { ids.append(m.meetingID) }
                let label = members.first(where: \.isUser)?.label ?? TopicIndex.commonest(members.map(\.label))
                out.append(Group(key: k, label: label, meetingIDs: ids, latest: members.map(\.startedAt).max() ?? .distantPast))
            }
        }
        groups = out.sorted { ($0.count, $0.latest, $1.label.lowercased()) > ($1.count, $1.latest, $0.label.lowercased()) }
    }

    private static func commonest(_ labels: [String]) -> String {
        var counts: [String: Int] = [:]
        for l in labels { counts[l, default: 0] += 1 }
        return counts.max { ($0.value, $1.key) < ($1.value, $0.key) }?.key ?? labels.first ?? ""
    }

    func group(forKey key: String) -> Group? {
        groups.first { $0.key == key } ?? groups.first { Topic.similar($0.key, key) }
    }

    /// The groups a recording belongs to, biggest first
    func groups(for meetingID: UUID) -> [Group] { groups.filter { $0.meetingIDs.contains(meetingID) } }

    func meetingIDs(forKey key: String) -> Set<UUID> { Set(group(forKey: key)?.meetingIDs ?? []) }
}

extension Meeting {
    /// User topics first, then by confidence
    var orderedTopics: [MeetingTopic] {
        topics.sorted { ($0.isUser ? 1 : 0, $0.confidence, $1.createdAt) > ($1.isUser ? 1 : 0, $1.confidence, $0.createdAt) }
    }

    var removedTopicKeys: [String] {
        get {
            guard let data = removedTopicKeysJSON?.data(using: .utf8) else { return [] }
            return (try? JSONDecoder().decode([String].self, from: data)) ?? []
        }
        set {
            removedTopicKeysJSON = newValue.isEmpty ? nil
                : (try? JSONEncoder().encode(newValue)).flatMap { String(data: $0, encoding: .utf8) }
        }
    }

    @MainActor static func topicEntries(_ meetings: [Meeting]) -> [TopicIndex.Entry] {
        meetings.flatMap { m in
            m.topics.map { TopicIndex.Entry(meetingID: m.id, key: $0.key, label: $0.label, isUser: $0.isUser, startedAt: m.startedAt) }
        }
    }
}

/// Saving, editing and purging topics.
@MainActor
enum TopicStore {
    /// Replace the recording's AI topics with validated candidates. User
    /// topics stay; a candidate that matches a user topic or a key the user
    /// removed is skipped. Returns the topics now on the recording.
    @discardableResult
    static func applyAI(_ candidates: [TopicCandidate], to meeting: Meeting, context: ModelContext) throws -> [MeetingTopic] {
        for old in meeting.topics where !old.isUser { context.delete(old) }
        let userKeys = meeting.topics.filter(\.isUser).map(\.key)
        let removed = meeting.removedTopicKeys
        var kept: [String] = []
        for c in candidates where c.confidence >= Topic.minConfidence {
            guard !userKeys.contains(where: { Topic.similar($0, c.key) }),
                  !removed.contains(where: { Topic.similar($0, c.key) }),
                  !kept.contains(where: { Topic.similar($0, c.key) }) else { continue }
            guard userKeys.count + kept.count < Topic.maxPerRecording, kept.count < Topic.maxAI else { break }
            let t = MeetingTopic(label: c.label, confidence: c.confidence, source: .ai)
            context.insert(t)
            t.meeting = meeting
            kept.append(c.key)
        }
        try context.save()
        return meeting.orderedTopics
    }

    enum Failure: LocalizedError, Equatable {
        case empty, tooMany, generic
        var errorDescription: String? {
            switch self {
            case .empty: "Enter a topic."
            case .tooMany: "A recording can have at most \(Topic.maxPerRecording) topics."
            case .generic: "That word is too general to be a topic."
            }
        }
    }

    /// Add a user topic. A matching AI topic becomes the user's (its label
    /// updated); a matching user topic is left alone.
    @discardableResult
    static func add(_ label: String, to meeting: Meeting, context: ModelContext) throws -> MeetingTopic {
        guard let clean = Topic.normalizeLabel(label) else { throw Failure.empty }
        guard !Topic.isGeneric(clean) else { throw Failure.generic }
        let key = Topic.key(clean)
        if let existing = meeting.topics.first(where: { Topic.similar($0.key, key) }) {
            if !existing.isUser { rename(existing, to: clean, meeting: meeting, context: context) }
            return existing
        }
        guard meeting.topics.count < Topic.maxPerRecording else { throw Failure.tooMany }
        let t = MeetingTopic(label: clean, confidence: 1, source: .user)
        context.insert(t)
        t.meeting = meeting
        meeting.removedTopicKeys.removeAll { Topic.similar($0, key) }
        try context.save()
        return t
    }

    /// Rename: the topic becomes the user's, so a re-run keeps it.
    static func rename(_ topic: MeetingTopic, to label: String, meeting: Meeting, context: ModelContext) {
        guard let clean = Topic.normalizeLabel(label), !Topic.isGeneric(clean) else { return }
        topic.label = clean
        topic.key = Topic.key(clean)
        topic.source = MeetingTopic.Source.user.rawValue
        topic.confidence = 1
        meeting.removedTopicKeys.removeAll { Topic.similar($0, topic.key) }
        try? context.save()
    }

    /// Remove. An AI topic's key is remembered so a re-run doesn't bring it back.
    static func remove(_ topic: MeetingTopic, from meeting: Meeting, context: ModelContext) {
        if !topic.isUser, !meeting.removedTopicKeys.contains(topic.key) {
            meeting.removedTopicKeys.append(topic.key)
        }
        context.delete(topic)
        try? context.save()
    }

    /// Purge (docs/REDACTION.md): the transcript was edited, so every AI
    /// topic goes (they were named from it). User topics are the user's own
    /// words and stay. Returns how many were deleted.
    @discardableResult
    static func purgeAI(_ meeting: Meeting, context: ModelContext) -> Int {
        let ai = meeting.topics.filter { !$0.isUser }
        for t in ai { context.delete(t) }
        return ai.count
    }
}
