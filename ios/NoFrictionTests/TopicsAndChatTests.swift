import SwiftData
import XCTest
@testable import noFriction

// docs/TOPICS_AND_CHAT.md, iOS side: topic parsing, keys and merging, the
// store and the purge, local retrieval, citations, suggestions and the
// chat purge. The AI is always a mock; no request leaves the process.

// MARK: - Topics: parsing untrusted output

final class TopicParseTests: XCTestCase {
    func testExtractsTopicsFromFencesProseAndThinking() throws {
        let raw = "<think>{\"topics\":[]}</think>Here you go:\n```json\n{\"topics\": [{\"label\": \"Q4 roadmap\", \"confidence\": 0.9}, {\"label\": \"Hiring plan\", \"confidence\": \"70%\"}]}\n```"
        let got = try TopicParse.validate(raw)
        XCTAssertEqual(got.map(\.label), ["Q4 roadmap", "Hiring plan"])
        XCTAssertEqual(got.map(\.key), ["q4 roadmap", "hiring plan"])
        XCTAssertEqual(got[0].confidence, 0.9, accuracy: 0.001)
        XCTAssertEqual(got[1].confidence, 0.7, accuracy: 0.001)
    }

    func testAcceptsBareArraysStringsAndAlternativeNames() throws {
        XCTAssertEqual(try TopicParse.validate(#"["Mitosis", {"topic": "Cell cycle", "score": 80}]"#).map(\.label), ["Mitosis", "Cell cycle"])
        XCTAssertEqual(try TopicParse.validate(#"{"topics": [{"name": "- Lease renewal."}]}"#).map(\.label), ["Lease renewal"])
        XCTAssertEqual(try TopicParse.validate(#"{"topics": [{"label": "Budget"}]}"#)[0].confidence, 0.5, "missing confidence reads as 0.5")
    }

    func testDropsGenericLongAndDuplicateLabelsAndCapsAtFour() throws {
        let raw = #"{"topics": [{"label": "Meeting"}, {"label": "Discussion", "confidence": 1}, {"label": "Q4 roadmap", "confidence": 0.8}, {"label": "the q4 roadmaps", "confidence": 0.95}, {"label": "This is a whole sentence about nothing in particular"}, {"label": "Hiring"}, {"label": "Budget"}, {"label": "Offsite"}, {"label": "Launch"}]}"#
        let got = try TopicParse.validate(raw)
        XCTAssertEqual(got.map(\.label), ["Q4 roadmap", "Hiring", "Budget", "Offsite"])
        XCTAssertEqual(got[0].confidence, 0.95, accuracy: 0.001, "a duplicate keeps the higher confidence")
    }

    func testMalformedOutputThrowsAndNeverCrashes() {
        let cases = ["", "nope", "{", "{\"topics\": \"x\"}", "{\"topics\": []}", "{\"topics\": [{\"label\": true}]}", "[1, 2]",
                     "null", "{\"topics\": [{\"label\": \"meeting\"}]}", String(repeating: "[", count: 3000), "\u{0}{{{"]
        for c in cases { XCTAssertThrowsError(try TopicParse.validate(c), c.prefix(30).description) }
    }

    func testConfidenceForms() {
        XCTAssertEqual(TopicParse.confidence(0.25), 0.25)
        XCTAssertEqual(TopicParse.confidence("0.4"), 0.4)
        XCTAssertEqual(TopicParse.confidence("90%"), 0.9)
        XCTAssertEqual(TopicParse.confidence(85), 0.85)
        XCTAssertEqual(TopicParse.confidence(-3), 0)
        XCTAssertEqual(TopicParse.confidence(true), 0.5)
        XCTAssertEqual(TopicParse.confidence("many"), 0.5)
    }
}

// MARK: - Topics: keys and merging

final class TopicKeyTests: XCTestCase {
    func testNormalizeLabel() {
        XCTAssertEqual(Topic.normalizeLabel("  - Q4   roadmap. "), "Q4 roadmap")
        XCTAssertEqual(Topic.normalizeLabel("\u{1}Budget\u{2}"), "Budget")
        XCTAssertNil(Topic.normalizeLabel("  "))
        XCTAssertNil(Topic.normalizeLabel(nil))
        XCTAssertLessThanOrEqual(Topic.normalizeLabel(String(repeating: "x", count: 100))!.count, Topic.maxLabelLength)
    }

    func testKeyFoldsCaseDiacriticsArticlesAndPlurals() {
        XCTAssertEqual(Topic.key("The Q4 Roadmaps"), "q4 roadmap")
        XCTAssertEqual(Topic.key("q4-roadmap"), "q4 roadmap")
        XCTAssertEqual(Topic.key("Café tables"), "cafe table")
        XCTAssertEqual(Topic.key("Classes"), "class")
        XCTAssertEqual(Topic.key("Status"), "status")
        XCTAssertEqual(Topic.key("Policies"), "policy")
        XCTAssertEqual(Topic.key("the"), "the", "a label of only stopwords keeps them")
        XCTAssertEqual(Topic.key(""), "")
    }

    func testSimilarKeys() {
        XCTAssertTrue(Topic.similar("q4 roadmap", "roadmap q4"))
        XCTAssertTrue(Topic.similar("lease renewal", "lease renewel"))
        XCTAssertTrue(Topic.similar("off site", "offsite"))
        XCTAssertFalse(Topic.similar("hiring", "firing"), "short keys need an exact match")
        XCTAssertFalse(Topic.similar("", "budget"))
        XCTAssertEqual(Topic.canonicalKey("roadmap q4", existing: ["budget", "q4 roadmap"]), "q4 roadmap")
        XCTAssertEqual(Topic.canonicalKey("offsite", existing: ["budget"]), "offsite")
        XCTAssertTrue(Topic.isGeneric("Meeting"))
        XCTAssertTrue(Topic.isGeneric("General"))
        XCTAssertFalse(Topic.isGeneric("Mitosis"))
    }

    func testIndexMergesNearDuplicatesAcrossRecordings() {
        let a = UUID(), b = UUID(), c = UUID()
        let t0 = Date(timeIntervalSince1970: 1_790_000_000)
        let index = TopicIndex(entries: [
            .init(meetingID: a, key: Topic.key("Q4 roadmap"), label: "Q4 roadmap", isUser: false, startedAt: t0),
            .init(meetingID: b, key: Topic.key("the Q4 Roadmaps"), label: "the Q4 Roadmaps", isUser: false, startedAt: t0.addingTimeInterval(60)),
            .init(meetingID: c, key: Topic.key("Roadmap Q4"), label: "Roadmap Q4", isUser: false, startedAt: t0.addingTimeInterval(120)),
            .init(meetingID: b, key: Topic.key("Hiring"), label: "Hiring", isUser: true, startedAt: t0.addingTimeInterval(60)),
            .init(meetingID: c, key: Topic.key("hiring"), label: "hiring", isUser: false, startedAt: t0.addingTimeInterval(120)),
            .init(meetingID: a, key: Topic.key("Budget"), label: "Budget", isUser: false, startedAt: t0),
        ])
        XCTAssertEqual(index.groups.map(\.label), ["Q4 roadmap", "Hiring", "Budget"])
        XCTAssertEqual(index.groups.map(\.count), [3, 2, 1])
        XCTAssertEqual(index.meetingIDs(forKey: "roadmap q4"), [a, b, c], "a near-duplicate key finds the group")
        XCTAssertEqual(index.groups(for: c).map(\.label), ["Q4 roadmap", "Hiring"])
        XCTAssertEqual(index.group(forKey: "hiring")?.label, "Hiring", "the user's spelling leads")
    }

    func testMergeCandidatesAcrossChunks() {
        let lists: [[TopicCandidate]] = [
            [.init(label: "Q4 roadmap", confidence: 0.9), .init(label: "Budget", confidence: 0.6)],
            [.init(label: "the Q4 roadmaps", confidence: 0.8), .init(label: "Hiring", confidence: 0.9)],
            [.init(label: "Q4 roadmap", confidence: 1), .init(label: "Offsite", confidence: 0.4), .init(label: "Launch", confidence: 0.5)],
        ]
        let merged = MeetingAI.mergeTopicCandidates(lists, parts: 3)
        XCTAssertEqual(merged.count, Topic.maxAI)
        XCTAssertEqual(merged.first?.label, "Q4 roadmap", "named in every chunk")
        XCTAssertGreaterThan(merged[0].confidence, merged[1].confidence)
        XCTAssertFalse(merged.contains { $0.label == "the Q4 roadmaps" })
        let single = MeetingAI.mergeTopicCandidates([[.init(label: "Mitosis", confidence: 1)]], parts: 1)
        XCTAssertEqual(single[0].confidence, 1, accuracy: 0.001)
    }
}

// MARK: - Mock AI

/// Answers in order; records every request. Never touches the network.
private final class MockChat: @unchecked Sendable {
    var answers: [Result<String, Error>]
    var calls: [[ChatMessage]] = []
    let lock = NSLock()

    init(_ answers: [Result<String, Error>]) { self.answers = answers }
    convenience init(_ texts: String...) { self.init(texts.map { .success($0) }) }

    var complete: MeetingAI.Complete {
        { messages, _, _ in
            self.lock.lock(); defer { self.lock.unlock() }
            self.calls.append(messages)
            guard !self.answers.isEmpty else { throw AIError.emptyAnswer }
            return try self.answers.removeFirst().get()
        }
    }
}

// MARK: - Topics: AI run, store and purge

@MainActor
final class TopicStoreTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        Storage.prepare()
    }

    override func tearDown() async throws { container = nil; context = nil }

    private func makeMeeting(_ lines: [String], title: String = "Test", notebook: String? = nil, notes: String? = nil,
                             start: Date = Date(timeIntervalSince1970: 1_790_000_000)) -> Meeting {
        let m = Meeting(title: title, startedAt: start)
        m.endedAt = start.addingTimeInterval(Double(lines.count) * 10 + 10)
        m.courseName = notebook
        m.aiNotes = notes
        context.insert(m)
        for (i, line) in lines.enumerated() {
            let toks = RedactionText.tokens(line)
            let base = Double(i) * 10
            let timings = toks.enumerated().map { j, t in
                WordTiming(location: t.range.location, length: t.range.length, start: base + Double(j) * 0.5, end: base + Double(j) * 0.5 + 0.4)
            }
            let s = Segment(text: line, start: start.addingTimeInterval(base), duration: 8, audioOffset: base, wordTimings: timings)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    func testFindTopicsRetriesOnceAndThrowsOnAccessErrors() async throws {
        let m = makeMeeting(["We reviewed the Q4 roadmap and the hiring plan.", "Budget is tight."])
        let ai = MockChat("not json at all", #"{"topics": [{"label": "Q4 roadmap", "confidence": 0.9}, {"label": "Hiring plan", "confidence": 0.7}]}"#)
        let got = try await MeetingAI.findTopics(StudyInput(meeting: m), contextTokens: 32_000, complete: ai.complete)
        XCTAssertEqual(got.map(\.label), ["Q4 roadmap", "Hiring plan"])
        XCTAssertEqual(ai.calls.count, 2, "asked again once")
        XCTAssertTrue(ai.calls[1][1].content.contains("couldn't be used"))
        XCTAssertTrue(ai.calls[0][1].content.contains("[0:00] We reviewed the Q4 roadmap"))
        XCTAssertEqual(ai.calls[0][0].content, MeetingAI.topicsSystem)

        let denied = MockChat([.failure(AIError.consentRequired("x"))])
        do {
            _ = try await MeetingAI.findTopics(StudyInput(meeting: m), contextTokens: 32_000, complete: denied.complete)
            XCTFail("should throw")
        } catch let e as AIError {
            XCTAssertEqual(e, .consentRequired("x"))
        }
        let bad = MockChat("nope", "still nope")
        do {
            _ = try await MeetingAI.findTopics(StudyInput(meeting: m), contextTokens: 32_000, complete: bad.complete)
            XCTFail("should throw")
        } catch let e as MeetingAI.TopicFailure {
            XCTAssertEqual(e, .unusable)
        }
        let empty = makeMeeting([])
        do {
            _ = try await MeetingAI.findTopics(StudyInput(meeting: empty), contextTokens: 32_000, complete: ai.complete)
            XCTFail("should throw")
        } catch let e as MeetingAI.TopicFailure {
            XCTAssertEqual(e, .noTranscript)
        }
    }

    func testLongTranscriptIsChunkedAndMerged() async throws {
        let lines = (0..<120).map { "Line \($0) about the Q4 roadmap and the launch date for the new product, plus other things." }
        let m = makeMeeting(lines)
        var answers: [Result<String, Error>] = []
        for _ in 0..<12 { answers.append(.success(#"{"topics": [{"label": "Q4 roadmap", "confidence": 0.9}, {"label": "Launch date", "confidence": 0.6}]}"#)) }
        let ai = MockChat(answers)
        let got = try await MeetingAI.findTopics(StudyInput(meeting: m), contextTokens: 2_000, complete: ai.complete)
        XCTAssertGreaterThan(ai.calls.count, 1, "a small context needs several chunks")
        XCTAssertTrue(ai.calls[0][1].content.contains("Part 1 of \(ai.calls.count)"))
        XCTAssertEqual(got.map(\.label), ["Q4 roadmap", "Launch date"])
    }

    func testApplyKeepsUserTopicsAndRemovedKeysAndReplacesAITopics() throws {
        let m = makeMeeting(["x"])
        try TopicStore.applyAI([.init(label: "Q4 roadmap", confidence: 0.9), .init(label: "Hiring", confidence: 0.8),
                                .init(label: "Faint", confidence: 0.1)], to: m, context: context)
        XCTAssertEqual(m.orderedTopics.map(\.label), ["Q4 roadmap", "Hiring"], "below minConfidence is dropped")
        // The user renames one, removes one, adds one
        let hiring = try XCTUnwrap(m.topics.first { $0.label == "Hiring" })
        TopicStore.rename(hiring, to: "Hiring plan", meeting: m, context: context)
        XCTAssertTrue(hiring.isUser)
        let roadmap = try XCTUnwrap(m.topics.first { $0.label == "Q4 roadmap" })
        TopicStore.remove(roadmap, from: m, context: context)
        XCTAssertEqual(m.removedTopicKeys, ["q4 roadmap"])
        _ = try TopicStore.add("Budget", to: m, context: context)
        XCTAssertThrowsError(try TopicStore.add("meeting", to: m, context: context))
        XCTAssertThrowsError(try TopicStore.add("   ", to: m, context: context))
        // A re-run: the removed key stays out, user topics stay, a match with a user topic is skipped
        try TopicStore.applyAI([.init(label: "The Q4 Roadmaps", confidence: 1), .init(label: "hiring plans", confidence: 0.9),
                                .init(label: "Offsite", confidence: 0.7)], to: m, context: context)
        XCTAssertEqual(Set(m.topics.map(\.label)), ["Hiring plan", "Budget", "Offsite"])
        XCTAssertEqual(m.orderedTopics.prefix(2).map(\.label).sorted(), ["Budget", "Hiring plan"], "user topics first")
        // Adding a label that matches an AI topic makes it the user's
        let added = try TopicStore.add("Off-site", to: m, context: context)
        XCTAssertEqual(added.label, "Off-site")
        XCTAssertTrue(added.isUser)
        XCTAssertEqual(m.topics.count, 3)
        // Adding a removed key brings it back as the user's
        _ = try TopicStore.add("Q4 roadmap", to: m, context: context)
        XCTAssertEqual(m.removedTopicKeys, [])
    }

    func testAtMostMaxPerRecording() throws {
        let m = makeMeeting(["x"])
        for i in 0..<Topic.maxPerRecording { _ = try TopicStore.add("Topic \(i)", to: m, context: context) }
        XCTAssertThrowsError(try TopicStore.add("One more", to: m, context: context))
        try TopicStore.applyAI([.init(label: "Extra", confidence: 1)], to: m, context: context)
        XCTAssertEqual(m.topics.count, Topic.maxPerRecording)
    }

    func testDeleteCommitPurgesAITopicsKeepsUserOnes() async throws {
        let m = makeMeeting(["We need the Azure credits by Friday.", "Marcus owns it."])
        try TopicStore.applyAI([.init(label: "Azure credits", confidence: 0.9)], to: m, context: context)
        _ = try TopicStore.add("Vendors", to: m, context: context)
        let p = try RedactionEngine.delete(.words(m.orderedSegments[0], 3...4), meeting: m, context: context)
        XCTAssertEqual(m.topics.count, 2, "kept during the undo window")
        try await RedactionEngine.commit(p, context: context)
        XCTAssertEqual(m.topics.map(\.label), ["Vendors"])
    }

    func testStrikePurgesAITopicsAndPhotoOnlyEditsKeepThem() async throws {
        let m = makeMeeting(["We need the Azure credits by Friday.", "Marcus owns it."])
        try TopicStore.applyAI([.init(label: "Azure credits", confidence: 0.9)], to: m, context: context)
        let name = "test-\(UUID().uuidString).jpg"
        let url = Storage.snapshots.appending(path: name)
        try Data([0xFF, 0xD8, 0xFF]).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let snap = Snapshot(fileName: name, takenAt: m.startedAt.addingTimeInterval(5))
        context.insert(snap)
        snap.meeting = m
        try context.save()
        try await RedactionEngine.strike(.screens([snap]), reason: nil, meeting: m, context: context)
        XCTAssertEqual(m.topics.count, 1, "a screen-only strike keeps topics")
        try await RedactionEngine.strike(.lines([m.orderedSegments[1]]), reason: "privileged", meeting: m, context: context)
        XCTAssertEqual(m.topics.count, 0)
    }

    func testDeletingTheRecordingCascadesTopics() throws {
        let m = makeMeeting(["x"])
        try TopicStore.applyAI([.init(label: "Azure credits", confidence: 0.9)], to: m, context: context)
        _ = try TopicStore.add("Vendors", to: m, context: context)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<MeetingTopic>()), 2)
        context.delete(m)
        try context.save()
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<MeetingTopic>()), 0)
    }

    /// The list has one grouping (by day) and one filter (Notebooks); a
    /// recording never appears twice. Topics stay searchable.
    func testRecordingsGroupedByDayOnce() throws {
        let t0 = Date(timeIntervalSince1970: 1_790_000_000)
        let a = makeMeeting(["x"], title: "A", notebook: "BIO 101", start: t0)
        let b = makeMeeting(["x"], title: "B", notebook: "BIO 101", start: t0.addingTimeInterval(100))
        let c = makeMeeting(["x"], title: "C", start: t0.addingTimeInterval(3 * 86_400))
        try TopicStore.applyAI([.init(label: "Mitosis", confidence: 0.9), .init(label: "Enzymes", confidence: 0.5)], to: a, context: context)
        try TopicStore.applyAI([.init(label: "mitosis", confidence: 0.9)], to: b, context: context)
        let sections = RecordingsGrouping.byDate([c, b, a])
        XCTAssertEqual(sections.count, 2, "two days")
        XCTAssertEqual(sections[0].meetings.map(\.title), ["C"], "newest day first")
        XCTAssertEqual(sections[1].meetings.map(\.title), ["B", "A"], "the order given inside a day")
        XCTAssertEqual(sections.flatMap(\.meetings).count, 3, "each recording listed once, whatever its topics")
    }

    /// Notes of every type render through the same block-aware Markdown
    /// (F-28): headings and bullets never show as raw "##" / "- ".
    func testNotesMarkdownBlocks() {
        let lecture = """
        ## Summary
        The cell membrane.

        ## Definitions
        - **Phospholipid bilayer**: controls what enters
        - Channel proteins
          - nested
        1. first
        """
        let blocks = NotesMarkdown.blocks(lecture)
        XCTAssertEqual(blocks, [
            .heading("Summary", level: 2),
            .paragraph("The cell membrane."),
            .heading("Definitions", level: 2),
            .bullet("**Phospholipid bilayer**: controls what enters", indent: 0),
            .bullet("Channel proteins", indent: 0),
            .bullet("nested", indent: 1),
            .bullet("first", indent: 0),
        ])
        // Inline bold survives; the markers do not
        let inline = String(NotesMarkdown.inline("**term**: definition").characters)
        XCTAssertEqual(inline, "term: definition")
        // Soft-wrapped lines join into one paragraph; "#hashtag" is not a heading
        XCTAssertEqual(NotesMarkdown.blocks("one\ntwo\n\n#tag"), [.paragraph("one two"), .paragraph("#tag")])
    }

    // MARK: Chat purge and scope

    func testChatPurgeOnDeleteStrikeAndRecordingDelete() async throws {
        let m = makeMeeting(["We need the Azure credits by Friday.", "Marcus owns it."], title: "Kickoff")
        let other = makeMeeting(["Unrelated line."], title: "Other")
        let thread = ChatStore.newThread(scope: .all, context: context)
        ChatStore.append(ChatThreadMessage(role: .user, content: "Who owns the credits?", scopeLabel: "All recordings"), to: thread, context: context)
        let cite = ChatCitation(n: 1, meetingID: m.id, title: m.title, timestamp: m.startedAt, offset: 0, kind: "Transcript", excerpt: "We need the Azure credits")
        ChatStore.append(ChatThreadMessage(role: .assistant, content: "Marcus [1]", scopeLabel: "All recordings", citations: [cite]), to: thread, context: context)
        let otherCite = ChatCitation(n: 1, meetingID: other.id, title: other.title, timestamp: other.startedAt, offset: 0, kind: "Transcript", excerpt: "Unrelated")
        ChatStore.append(ChatThreadMessage(role: .assistant, content: "Nothing [1]", scopeLabel: "All recordings", citations: [otherCite]), to: thread, context: context)
        let scopedThread = ChatStore.newThread(scope: .recording(id: m.id, title: m.title), context: context)
        let untouched = ChatStore.newThread(scope: .all, context: context)
        XCTAssertEqual(thread.title, "Who owns the credits?")

        // Strike of transcript text: the answer citing it goes, the question and the other answer stay
        try await RedactionEngine.strike(.words(m.orderedSegments[0], 3...4), reason: nil, meeting: m, context: context)
        XCTAssertEqual(thread.orderedMessages.map(\.content), ["Who owns the credits?", "Nothing [1]"])
        XCTAssertTrue(thread.flagged)
        XCTAssertTrue(thread.flagNote?.contains("edited") == true)
        XCTAssertTrue(scopedThread.flagged)
        XCTAssertFalse(untouched.flagged)

        // Delete commit does the same
        ChatStore.append(ChatThreadMessage(role: .assistant, content: "Again [1]", scopeLabel: "x", citations: [cite]), to: thread, context: context)
        let p = try RedactionEngine.delete(.lines([m.orderedSegments[1]]), meeting: m, context: context)
        XCTAssertEqual(thread.messages.count, 3, "kept during the undo window")
        try await RedactionEngine.commit(p, context: context)
        XCTAssertEqual(thread.messages.count, 2)

        // Delete Recording: purge, then the row goes
        ChatStore.append(ChatThreadMessage(role: .assistant, content: "Once more [1]", scopeLabel: "x", citations: [cite]), to: thread, context: context)
        ChatStore.purge(meetingID: m.id, title: m.title, deleted: true, context: context)
        context.delete(m)
        try context.save()
        XCTAssertEqual(thread.messages.count, 2)
        XCTAssertTrue(thread.flagNote?.contains("deleted") == true)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<ChatThread>()), 3)
        ChatStore.delete(thread, context: context)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<ChatThread>()), 2)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<ChatThreadMessage>()), 0, "messages cascade with the thread")
    }

    func testMemoryKeepsTheLastEightTurns() {
        let thread = ChatStore.newThread(scope: .all, context: context)
        let t0 = Date(timeIntervalSince1970: 1_790_000_000)
        for i in 0..<12 {
            ChatStore.append(ChatThreadMessage(role: .user, content: "q\(i)", createdAt: t0.addingTimeInterval(Double(i) * 2)), to: thread, context: context)
            ChatStore.append(ChatThreadMessage(role: .assistant, content: "a\(i)", createdAt: t0.addingTimeInterval(Double(i) * 2 + 1)), to: thread, context: context)
        }
        let mem = ChatStore.memory(thread)
        XCTAssertEqual(mem.count, ChatStore.memoryTurns * 2)
        XCTAssertEqual(mem.first?.content, "q4")
        XCTAssertEqual(mem.last?.content, "a11")
        XCTAssertEqual(mem.map(\.role).prefix(2), ["user", "assistant"])
        XCTAssertEqual(ChatStore.title(from: "   \n"), "New chat")
        XCTAssertEqual(ChatStore.title(from: String(repeating: "x", count: 60)).count, 48)
    }

    func testScopeFiltersRecordings() throws {
        let t0 = Date(timeIntervalSince1970: 1_790_000_000)
        let a = makeMeeting(["x"], title: "A", notebook: "BIO 101", start: t0)
        let b = makeMeeting(["x"], title: "B", notebook: "Work", start: t0.addingTimeInterval(100))
        let c = makeMeeting(["x"], title: "C", start: t0.addingTimeInterval(200))
        try TopicStore.applyAI([.init(label: "Mitosis", confidence: 0.9)], to: a, context: context)
        try TopicStore.applyAI([.init(label: "The Mitosis", confidence: 0.9)], to: c, context: context)
        let index = TopicIndex(entries: Meeting.topicEntries([a, b, c]))
        let all = [a, b, c]
        XCTAssertEqual(ChatScope.all.filter(all, topics: index).map(\.title), ["A", "B", "C"])
        XCTAssertEqual(ChatScope.notebook("bio 101").filter(all, topics: index).map(\.title), ["A"])
        XCTAssertEqual(ChatScope.topic(key: "mitosis", label: "Mitosis").filter(all, topics: index).map(\.title), ["A", "C"])
        XCTAssertEqual(ChatScope.recording(id: b.id, title: "B").filter(all, topics: index).map(\.title), ["B"])
        // Stored and rebuilt
        let thread = ChatStore.newThread(scope: .topic(key: "mitosis", label: "Mitosis"), context: context)
        XCTAssertEqual(thread.scope, .topic(key: "mitosis", label: "Mitosis"))
        XCTAssertEqual(thread.scope.label, "Topic · Mitosis")
        XCTAssertEqual(ChatStore.newThread(scope: .recording(id: b.id, title: "B"), context: context).scope, .recording(id: b.id, title: "B"))
        XCTAssertEqual(ChatScope.stored(kind: "recording", value: "garbage", label: "x"), .all)
        XCTAssertEqual(ChatScope.notebook("BIO 101").label, "Notebook · BIO 101")
    }

    func testChatEndToEndWithMock() async throws {
        let m = makeMeeting(["We need the Azure credits by Friday.", "Marcus owns the vendor contract."], title: "Kickoff",
                            notes: "## Summary\nMarcus owns the vendor contract.\n## Decisions\n- Azure credits by Friday")
        let ai = MockChat("Marcus owns it [2]. The credits are due Friday [1][1][7].")
        let answer = try await MeetingAI.chat(question: "Who owns the vendor contract?", scopeLabel: "All recordings",
                                              sources: [ChatSource(meeting: m)], memory: [ChatMessage(role: "user", content: "hi"), ChatMessage(role: "assistant", content: "hello")],
                                              contextTokens: 32_000, complete: ai.complete)
        XCTAssertEqual(answer.citations.map(\.n), [2, 1])
        XCTAssertEqual(answer.citations.map(\.meetingID), [m.id, m.id])
        XCTAssertEqual(answer.citations[0].title, "Kickoff")
        let call = try XCTUnwrap(ai.calls.first)
        XCTAssertEqual(call.map(\.role), ["system", "user", "assistant", "user"], "memory sits between the system prompt and the question")
        XCTAssertTrue(call.last!.content.hasPrefix("SCOPE: All recordings"))
        XCTAssertTrue(call.last!.content.contains("[1] Kickoff"))
        XCTAssertTrue(call.last!.content.contains("QUESTION: Who owns the vendor contract?"))
        do {
            _ = try await MeetingAI.chat(question: "x", scopeLabel: "x", sources: [], memory: [], contextTokens: 32_000, complete: ai.complete)
            XCTFail("empty scope should throw")
        } catch let e as MeetingAI.ChatFailure {
            XCTAssertEqual(e, .nothingToSearch)
        }
    }

    /// New models and the new column open an existing on-disk store (no
    /// schema version: everything added is optional, defaulted or a new entity).
    func testOnDiskStoreRoundTripWithNewModels() throws {
        let url = FileManager.default.temporaryDirectory.appending(path: "topics-chat-\(UUID().uuidString).store")
        defer { for s in ["", "-wal", "-shm"] { try? FileManager.default.removeItem(at: URL(fileURLWithPath: url.path + s)) } }
        let id: UUID
        do {
            let c = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: url))
            let m = Meeting(title: "Old", startedAt: .now)
            c.mainContext.insert(m)
            try c.mainContext.save()
            id = m.id
        }
        let c = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: url))
        let ctx = c.mainContext
        let m = try XCTUnwrap(try ctx.fetch(FetchDescriptor<Meeting>()).first { $0.id == id })
        XCTAssertEqual(m.topics.count, 0)
        XCTAssertEqual(m.removedTopicKeys, [])
        XCTAssertEqual(try ctx.fetchCount(FetchDescriptor<ChatThread>()), 0)
        _ = try TopicStore.add("Budget", to: m, context: ctx)
        let t = ChatStore.newThread(scope: .recording(id: m.id, title: m.title), context: ctx)
        ChatStore.append(ChatThreadMessage(role: .user, content: "q"), to: t, context: ctx)
        try ctx.save()
        XCTAssertEqual(try ctx.fetchCount(FetchDescriptor<MeetingTopic>()), 1)
        XCTAssertEqual(try ctx.fetchCount(FetchDescriptor<ChatThreadMessage>()), 1)
    }
}

// MARK: - Chat: retrieval, citations, suggestions (pure)

final class ChatRetrievalTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)

    private func source(_ title: String, notebook: String? = nil, lines: [String], notes: String? = nil,
                        marks: [ChatSource.Mark] = [], daysAgo: Int = 0) -> ChatSource {
        ChatSource(meetingID: UUID(), title: title, startedAt: t0.addingTimeInterval(Double(-daysAgo) * 86_400), notebook: notebook,
                   lines: lines.enumerated().map { .init(ms: $0.offset * 10_000, text: $0.element) }, notes: notes, marks: marks)
    }

    func testPassagesWindowsNotesAndMarkers() {
        let s = source("A", lines: Array(repeating: "A line of forty characters or thereabouts.", count: 30),
                       notes: "## Summary\nShort.\n## Decisions\n- One\n- Two", marks: [.init(ms: 5_000, label: "Important", note: "check this")])
        let ps = ChatRetrieval.passages(from: [s])
        let transcript = ps.filter { $0.kind == .transcript }
        XCTAssertGreaterThan(transcript.count, 1)
        XCTAssertTrue(transcript.allSatisfy { $0.text.count <= ChatRetrieval.windowChars + 50 })
        XCTAssertEqual(transcript[1].ms, 90_000, "nine 43-character lines fill a window; the next starts at its first line's time")
        XCTAssertEqual(ps.filter { $0.kind == .notes }.map(\.text), ["## Summary\nShort.", "## Decisions\n- One\n- Two"])
        XCTAssertEqual(ps.filter { $0.kind == .marker }.first?.text, "Important: check this")
        XCTAssertEqual(ps.first?.header(n: 3).prefix(6), "[3] A ")
        // A fully stricken line isn't a passage
        XCTAssertTrue(ChatRetrieval.passages(from: [source("B", lines: [RedactionText.placeholder])]).isEmpty)
    }

    func testTermsAndRanking() {
        XCTAssertEqual(ChatRetrieval.terms("What did we decide about the Azure credits?"), ["decide", "azure", "credit"])
        let a = source("Kickoff", lines: ["We need the Azure credits by Friday.", "Marcus owns the vendor contract."])
        let b = source("Standup", lines: ["Lunch was fine.", "The vendor contract slipped a week."])
        let ranked = ChatRetrieval.rank(ChatRetrieval.passages(from: [a, b]), query: "vendor contract")
        XCTAssertEqual(ranked.count, 2)
        XCTAssertTrue(ranked.allSatisfy { $0.score > 0 })
        XCTAssertTrue(ChatRetrieval.rank(ChatRetrieval.passages(from: [a, b]), query: "the and of").isEmpty, "only stopwords: nothing matches")
        XCTAssertTrue(ChatRetrieval.rank([], query: "x").isEmpty)
    }

    func testScopingRetrievalAndBudget() {
        let bio = source("BIO 101 week 3", notebook: "BIO 101", lines: ["Mitosis has four phases.", "Prophase comes first."])
        let work = source("Kickoff", notebook: "Work", lines: ["Mitosis is not a work topic, but the word appears here."])
        // Scoping happens before retrieval: only the sources handed in are searched
        let onlyBio = ChatRetrieval.retrieve([bio], query: "mitosis phases", budgetChars: 5_000)
        XCTAssertEqual(Set(onlyBio.map(\.meetingID)), [bio.meetingID])
        let both = ChatRetrieval.retrieve([bio, work], query: "mitosis", budgetChars: 5_000)
        XCTAssertEqual(Set(both.map(\.meetingID)), [bio.meetingID, work.meetingID])
        // Budget: a tiny budget takes nothing; a budget for one passage takes the best one
        XCTAssertTrue(ChatRetrieval.retrieve([bio, work], query: "mitosis", budgetChars: 10).isEmpty)
        let one = ChatRetrieval.retrieve([bio, work], query: "mitosis phases", budgetChars: 160)
        XCTAssertEqual(one.count, 1)
        XCTAssertEqual(one[0].meetingID, bio.meetingID, "the passage with every term wins")
        // Per-recording cap and overall cap
        let many = source("Long", lines: (0..<40).map { "Budget line \($0) " + String(repeating: "filler ", count: 50) })
        let picked = ChatRetrieval.select(ChatRetrieval.rank(ChatRetrieval.passages(from: [many]), query: "budget").map(\.passage), budgetChars: 100_000)
        XCTAssertEqual(picked.count, ChatRetrieval.maxPerMeeting)
        XCTAssertEqual(picked.map(\.ms), picked.map(\.ms).sorted(), "in reading order")
        // No term match: recent recordings' notes and opening lines stand in
        let recent = source("Recent", lines: ["Hello."], notes: "## Summary\nx", daysAgo: 0)
        let old = source("Old", lines: ["Hello again."], daysAgo: 3)
        let fallback = ChatRetrieval.retrieve([old, recent], query: "summarize everything", budgetChars: 5_000)
        XCTAssertEqual(fallback.first?.title, "Old", "reading order, oldest first")
        XCTAssertEqual(Set(fallback.map(\.title)), ["Old", "Recent"])
        XCTAssertEqual(fallback.filter { $0.title == "Recent" }.first?.kind, .notes, "notes before transcript")
    }

    func testCitationMapping() {
        let a = source("Kickoff", lines: ["We need the Azure credits by Friday.", "Marcus owns it."])
        let ps = ChatRetrieval.passages(from: [a, source("Other", notebook: "Work", lines: ["Something else."], notes: "## Summary\nnotes")])
        XCTAssertEqual(ps.count, 3)
        let text = "Marcus owns it [2]. Credits by Friday [1][1]. Nothing at [9] or [0] or [x] or [ 2 ]. Notes [3]"
        let cites = MeetingAI.citations(in: text, passages: ps)
        XCTAssertEqual(cites.map(\.n), [2, 1, 3])
        XCTAssertEqual(cites[0].title, "Other")
        XCTAssertEqual(cites[1].title, "Kickoff")
        XCTAssertEqual(cites[1].timestamp, ps[0].timestamp)
        XCTAssertEqual(cites[1].offset, 0)
        XCTAssertEqual(cites[1].excerpt, "We need the Azure credits by Friday. Marcus owns it.")
        XCTAssertEqual(cites[2].kind, "Notes")
        XCTAssertTrue(MeetingAI.citations(in: "no marks", passages: ps).isEmpty)
        XCTAssertTrue(MeetingAI.citations(in: "[1]", passages: []).isEmpty)
        XCTAssertTrue(MeetingAI.excerpt(String(repeating: "word ", count: 100)).hasSuffix("…"))
        // Round trip through the stored message
        let m = ChatThreadMessage(role: .assistant, content: text, citations: cites)
        XCTAssertEqual(m.citations, cites)
        XCTAssertTrue(m.cites(a.meetingID))
        XCTAssertFalse(m.cites(UUID()))
    }

    func testSuggestedQuestionsNeedNoAI() {
        let recs = [ChatSuggestions.Recording(title: "Kickoff", kind: .meeting, startedAt: t0),
                    ChatSuggestions.Recording(title: "BIO 101 week 3", kind: .class, startedAt: t0.addingTimeInterval(60))]
        let all = ChatSuggestions.questions(scope: .all, recordings: recs, topics: ["Mitosis", "Q4 roadmap", "Budget"])
        XCTAssertEqual(all.count, ChatSuggestions.max)
        XCTAssertEqual(all[0], "What was said about Mitosis?")
        XCTAssertEqual(all[2], "What did “BIO 101 week 3” cover?")
        XCTAssertEqual(Set(all).count, all.count, "no duplicates")
        let rec = ChatSuggestions.questions(scope: .recording(id: UUID(), title: "Kickoff"), recordings: recs, topics: [])
        XCTAssertEqual(rec[0], "What was decided in “Kickoff”?")
        let cls = ChatSuggestions.questions(scope: .recording(id: UUID(), title: "BIO 101 week 3"), recordings: recs, topics: ["Mitosis"])
        XCTAssertEqual(cls[0], "What were the key concepts in “BIO 101 week 3”?")
        XCTAssertTrue(cls.contains("What was said about Mitosis?"))
        XCTAssertEqual(ChatSuggestions.questions(scope: .topic(key: "mitosis", label: "Mitosis"), recordings: [], topics: [])[0], "Summarize what was said about Mitosis.")
        XCTAssertEqual(ChatSuggestions.questions(scope: .notebook("BIO 101"), recordings: [], topics: []).first, "What are the main themes in BIO 101 so far?")
        XCTAssertFalse(ChatSuggestions.questions(scope: .all, recordings: [], topics: []).isEmpty)
    }
}
