import SwiftData
import XCTest
@testable import noFriction

/// docs/STUDY_TOOLS.md, iOS side: parsing untrusted model output, the
/// marker model, exports and the purge. The AI is always a mock here.
final class StudyParseTests: XCTestCase {
    func testExtractsJSONFromFencesProseAndThinking() throws {
        let raw = "<think>plan {\"cards\": []}</think>Sure!\n```json\n{\"cards\": [{\"front\": \"Mitosis\", \"back\": \"Cell division\"}]}\n```\nbye {x}"
        let json = try StudyParse.validate(.flashcards, raw: raw, durationMs: 0)
        XCTAssertEqual(StudyParse.decode(StudyCards.self, json), StudyCards(cards: [.init(front: "Mitosis", back: "Cell division")]))
        // Trailing commas and brackets inside strings
        let terms = try StudyParse.validate(.terms, raw: #"{"terms": [{"term": "a {b}", "definition": "c ] d",},],}"#, durationMs: 0)
        XCTAssertEqual(StudyParse.decode(StudyTerms.self, terms)?.terms.first, .init(term: "a {b}", definition: "c ] d"))
        // A bare array for a list part, with alternative field names
        let cards = try StudyParse.validate(.flashcards, raw: #"[{"question": "Q?", "answer": "A"}]"#, durationMs: 0)
        XCTAssertEqual(StudyParse.decode(StudyCards.self, cards)?.cards, [.init(front: "Q?", back: "A")])
    }

    func testMalformedOutputThrowsAndNeverCrashes() {
        let cases = [
            "", "I can't help with that.", "{", "{\"cards\": [", "[1, 2, 3]", "{\"cards\": \"nope\"}",
            "{\"cards\": [{\"front\": true, \"back\": \"x\"}]}", "{\"cards\": [{\"front\": \"same\", \"back\": \"Same.\"}]}",
            "null", "\"text\"", "{\"sections\": [{\"heading\": \"H\", \"bullets\": []}]}", "\u{0}\u{1}{{{{[[[[",
            String(repeating: "[", count: 5000),
        ]
        for kind in StudyKind.allCases {
            for c in cases {
                XCTAssertThrowsError(try StudyParse.validate(kind, raw: c, durationMs: 60_000), "\(kind) accepted \(c.prefix(40))")
            }
        }
    }

    func testQuizAnswersAndTimesAreChecked() throws {
        let raw = #"""
        {"questions": [
          {"question": "What makes ATP?", "choices": ["Mitochondria", "Nucleus", "Ribosome", "Golgi"], "answer": 0, "explanation": "Said at 12:34.", "time": "12:34"},
          {"question": "Letter", "choices": ["a", "b", "c"], "answer": "C)", "time": "[1:02:03]"},
          {"question": "Text", "options": ["red", "green"], "correct": "Green", "t": 95},
          {"question": "Out of range", "choices": ["a", "b"], "answer": 2},
          {"question": "Bool", "choices": ["a", "b"], "answer": true},
          {"question": "Dupes", "choices": ["same", "Same"], "answer": 0},
          {"question": "Late", "choices": ["a", "b"], "answer": 1, "time": "99:00"}
        ]}
        """#
        let quiz = try XCTUnwrap(StudyParse.decode(StudyQuiz.self, try StudyParse.validate(.quiz, raw: raw, durationMs: 3_800_000)))
        XCTAssertEqual(quiz.questions.map(\.question), ["What makes ATP?", "Letter", "Text", "Late"])
        XCTAssertEqual(quiz.questions[0].atMs, 754_000)
        XCTAssertEqual(quiz.questions[1].answer, 2)
        XCTAssertEqual(quiz.questions[1].atMs, 3_723_000)
        XCTAssertEqual(quiz.questions[2].answer, 1)
        XCTAssertNil(quiz.questions[3].atMs, "after the end of the lecture")
        // Stored material validates to itself
        let stored = try StudyParse.validate(.quiz, raw: raw, durationMs: 3_800_000)
        XCTAssertEqual(try StudyParse.validate(.quiz, raw: stored, durationMs: 3_800_000), stored)
    }

    func testTextIsCleanedCappedAndKeptAsText() throws {
        let long = String(repeating: "x", count: 5000)
        let raw = #"{"cards": [{"front": "  - 1. What is\n\tosmosis? ", "back": "<img src=x onerror=alert(1)> water"}, {"front": "\#(long)", "back": "b"}, {"front": "What is osmosis?", "back": "dup"}]}"#
        let cards = try XCTUnwrap(StudyParse.decode(StudyCards.self, try StudyParse.validate(.flashcards, raw: raw, durationMs: 0))).cards
        XCTAssertEqual(cards.count, 2)
        XCTAssertEqual(cards[0].front, "What is osmosis?")
        XCTAssertEqual(cards[0].back, "<img src=x onerror=alert(1)> water", "SwiftUI Text shows it as text")
        XCTAssertLessThanOrEqual(cards[1].front.count, 400)
        XCTAssertTrue(cards[1].front.hasSuffix("…"))
        XCTAssertEqual(StudyParse.clean("**bold** stays", max: 50), "**bold** stays")
        XCTAssertEqual(StudyParse.clean("👩‍💻 dev", max: 50), "👩‍💻 dev", "emoji sequences survive")
    }

    func testClocks() {
        XCTAssertEqual(StudyParse.parseClock("12:34"), 754_000)
        XCTAssertEqual(StudyParse.parseClock("[1:02:05]"), 3_725_000)
        XCTAssertEqual(StudyParse.parseClock("95"), 95_000)
        XCTAssertNil(StudyParse.parseClock("12:75"))
        XCTAssertNil(StudyParse.parseClock("1:2:3:4"))
        XCTAssertNil(StudyParse.parseClock("-5"))
        XCTAssertEqual(StudyParse.clock(3_725_000), "1:02:05")
    }

    func testCondensedLines() throws {
        let ls = try StudyParse.condensedLines("```\n[1:00] - Mitosis has 4 phases\n\n* [bad] no time\n```", maxLines: 10)
        XCTAssertEqual(ls, ["[1:00] Mitosis has 4 phases", "[bad] no time"])
        XCTAssertThrowsError(try StudyParse.condensedLines(" \n```", maxLines: 10))
    }
}

final class StudyExportTests: XCTestCase {
    func testCSVFieldsAreQuotedAndSafe() {
        XCTAssertEqual(StudyExport.csvField("a, b"), "\"a, b\"")
        XCTAssertEqual(StudyExport.csvField("say \"hi\""), "\"say \"\"hi\"\"\"")
        XCTAssertEqual(StudyExport.csvField("one\ntwo"), "\"one two\"")
        XCTAssertEqual(StudyExport.csvField("=HYPERLINK(1)"), "\"'=HYPERLINK(1)\"")
        XCTAssertEqual(StudyExport.csvField("-5 degrees"), "\"-5 degrees\"")
        XCTAssertEqual(StudyExport.csvField("-cmd"), "\"'-cmd\"")
        XCTAssertEqual(StudyExport.flashcardsCSV([.init(front: "What, why?", back: "Because \"so\"")]),
                       "\"What, why?\",\"Because \"\"so\"\"\"\r\n")
    }

    func testMarkdownEscapesModelText() {
        let md = StudyExport.guideMarkdown(
            title: "Bio #1", kind: .class, when: "",
            summary: StudySummary(title: nil, sections: [.init(heading: "Org*an*elles", bullets: ["[click](javascript:x) <b>"])]),
            terms: nil, cards: nil,
            quiz: StudyQuiz(questions: [.init(question: "Q1?", choices: ["a", "b"], answer: 1, explanation: "because", atMs: 754_000)]),
            asks: StudyAsks(questions: [.init(question: "Why?", atMs: 60_000)]),
            marks: [.init(ms: 90_000, kind: .test, note: "phase_2"), .init(ms: 120_000, kind: .question, note: nil)])
        XCTAssertTrue(md.hasPrefix("# Study guide: Bio \\#1\n"))
        XCTAssertTrue(md.contains("\\[click\\](javascript:x) \\<b\\>"))
        XCTAssertFalse(md.contains("[click]("))
        XCTAssertTrue(md.contains("- 1:30 ✎ On the test: phase\\_2"))
        XCTAssertTrue(md.contains("- You marked 2:00 as confusing"))
        XCTAssertTrue(md.contains("1. B: because (12:34)"))
        XCTAssertEqual(StudyExport.fileStem("Bio: Cells/2"), "Bio Cells 2")
    }

    func testDeckAndQuizState() {
        var d = FlashcardDeckState(count: 3)
        d.flip(); XCTAssertTrue(d.flipped)
        d.mark(known: true)
        d.mark(known: false)
        d.mark(known: true)
        XCTAssertEqual(d.round, 2)
        XCTAssertEqual(d.order, [1])
        d.mark(known: true)
        XCTAssertTrue(d.isDone)
        XCTAssertEqual(d.known.sorted(), [0, 1, 2])

        let qs: [StudyQuiz.Item] = [
            .init(question: "Q1", choices: ["a", "b"], answer: 1, explanation: "", atMs: nil),
            .init(question: "Q2", choices: ["a", "b"], answer: 0, explanation: "", atMs: nil),
        ]
        var q = QuizRunState(count: 2)
        q.next(in: qs); XCTAssertEqual(q.pos, 0, "can't skip")
        q.pick(1, in: qs); q.pick(0, in: qs)
        XCTAssertEqual(q.picked[0], 1, "first answer stands")
        q.next(in: qs); q.pick(1, in: qs); q.next(in: qs)
        XCTAssertTrue(q.finished)
        XCTAssertEqual(q.score(in: qs).correct, 1)
    }
}

/// Answers by the part named in the system prompt; records every request.
private final class MockAI: @unchecked Sendable {
    var calls: [[ChatMessage]] = []
    var badFirst: [String: Int] = [:]
    var error: Error?
    let lock = NSLock()

    static func kind(_ system: String) -> String {
        if system.hasPrefix("You condense") { return "condense" }
        if system.contains("Write lecture notes") || system.contains("Write notes:") { return "summary" }
        if system.contains("key terms") { return "terms" }
        if system.contains("flashcards") { return "flashcards" }
        if system.contains("practice quiz") { return "quiz" }
        return "questions"
    }

    func answer(_ msgs: [ChatMessage]) throws -> String {
        lock.lock(); defer { lock.unlock() }
        calls.append(msgs)
        if let error { throw error }
        let k = Self.kind(msgs[0].content)
        if let n = badFirst[k], n > 0 { badFirst[k] = n - 1; return "Sorry: front ATP back energy" }
        switch k {
        case "condense": return "[0:10] cells have organelles\n[0:20] mitochondria make ATP"
        case "summary": return #"{"title": "Cells", "sections": [{"heading": "Organelles", "bullets": ["Mitochondria make ATP"]}]}"#
        case "terms": return #"{"terms": [{"term": "ATP", "definition": "Energy currency"}]}"#
        case "flashcards": return #"{"cards": [{"front": "What makes ATP?", "back": "Mitochondria"}]}"#
        case "quiz": return #"{"questions": [{"question": "What makes ATP?", "choices": ["Mitochondria", "Nucleus"], "answer": 0, "explanation": "0:20", "time": "0:20"}]}"#
        default: return #"{"questions": [{"question": "Why ATP?", "time": "0:20"}]}"#
        }
    }

    var complete: MeetingAI.Complete { { msgs, _, _ in try self.answer(msgs) } }
}

final class StudyGenerationTests: XCTestCase {
    private func input(lines: Int, marks: [StudyInput.Mark] = []) -> StudyInput {
        StudyInput(title: "Biology", recordingKind: .class, courseName: "BIO 101", durationMs: lines * 6000,
                   lines: (0..<lines).map { .init(ms: $0 * 6000, text: "sentence \($0) about the cell cycle and mitosis phases") },
                   marks: marks)
    }

    func testShortLectureIsOneRequestPerPart() async throws {
        let ai = MockAI()
        let out = try await MeetingAI.studyGuide(input(lines: 3, marks: [.init(ms: 6000, kind: .test, note: "phases")]),
                                                 contextTokens: 32_768, complete: ai.complete)
        XCTAssertEqual(out.count, 5)
        XCTAssertTrue(out.allSatisfy { (try? $0.1.get()) != nil })
        XCTAssertEqual(ai.calls.count, 5)
        XCTAssertTrue(ai.calls.allSatisfy { $0[1].content.contains("[0:06] ✎ On the test: phases") })
        XCTAssertTrue(ai.calls[0][1].content.hasPrefix("Lecture: Biology\nNotebook: BIO 101\n"))
    }

    func testLongLectureIsCondensedForTheOnDeviceWindow() async throws {
        let ai = MockAI()
        let out = try await MeetingAI.studyGuide(input(lines: 600), contextTokens: AppleOnDevice.contextTokens, complete: ai.complete)
        XCTAssertTrue(out.allSatisfy { (try? $0.1.get()) != nil })
        let condense = ai.calls.filter { MockAI.kind($0[0].content) == "condense" }.count
        XCTAssertGreaterThanOrEqual(condense, 2)
        for c in ai.calls {
            let chars = c.reduce(0) { $0 + $1.content.count }
            XCTAssertLessThan(chars, Int(Double(4096 - 700 - 512) * ContextFit.charsPerToken), "fits without trimming")
        }
    }

    func testBadAnswerRetriedOnceThenReportedForThatPartOnly() async throws {
        let ai = MockAI()
        ai.badFirst = ["flashcards": 1, "quiz": 2]
        let out = try await MeetingAI.studyGuide(input(lines: 3), contextTokens: 32_768, complete: ai.complete)
        let byKind = Dictionary(uniqueKeysWithValues: out.map { ($0.0, $0.1) })
        XCTAssertNotNil(try? byKind[.flashcards]?.get())
        guard case .failure(let f)? = byKind[.quiz] else { return XCTFail("quiz should fail") }
        XCTAssertTrue(f.errorDescription?.contains("asked twice") ?? false)
        XCTAssertNotNil(try? byKind[.summary]?.get())
        XCTAssertEqual(ai.calls.count, 7)
    }

    func testAccessErrorsStopTheRun() async {
        let ai = MockAI()
        ai.error = AIError.consentRequired("example.com")
        do {
            _ = try await MeetingAI.studyGuide(input(lines: 3), contextTokens: 32_768, complete: ai.complete)
            XCTFail("should throw")
        } catch {
            XCTAssertEqual(error as? AIError, .consentRequired("example.com"))
        }
        XCTAssertEqual(ai.calls.count, 1)
        // No transcript: no request at all
        let none = MockAI()
        let empty = StudyInput(title: "x", durationMs: 0, lines: [.init(ms: 0, text: RedactionText.placeholder)], marks: [])
        do { _ = try await MeetingAI.studyGuide(empty, contextTokens: 32_768, complete: none.complete); XCTFail() } catch {}
        XCTAssertTrue(none.calls.isEmpty)
    }
}

@MainActor
final class MarkerAndStudyStoreTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        Storage.prepare()
    }

    override func tearDown() async throws {
        container = nil
        context = nil
    }

    private func meeting(_ lines: [String]) -> Meeting {
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let m = Meeting(title: "Biology", startedAt: start)
        context.insert(m)
        for (i, l) in lines.enumerated() {
            let s = Segment(text: l, start: start.addingTimeInterval(Double(i) * 10), duration: 8, audioOffset: Double(i) * 10)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    func testMarkerModel() throws {
        let m = meeting(["hello"])
        let k = MomentMarker(at: m.startedAt.addingTimeInterval(90))
        context.insert(k)
        k.meeting = m
        XCTAssertEqual(k.markerKind, .important, "one tap marks Important")
        XCTAssertEqual(k.offset(in: m), 90)
        k.setKind(.test)
        k.setNote("   ask   about\nproof 2 ")
        XCTAssertEqual(k.markerKind, .test)
        XCTAssertEqual(k.note, "ask about proof 2")
        k.setNote("   ")
        XCTAssertNil(k.note)
        XCTAssertEqual(MomentMarker.clean(String(repeating: "é", count: 400))?.count, MomentMarker.maxNoteLength)
        XCTAssertEqual(MarkerKind.allCases.map(\.symbol), ["★", "?", "✎"])
        XCTAssertEqual(MarkerKind(rawValue: "test")?.label(for: .class), "On the test")
        XCTAssertEqual(MarkerKind.test.label(for: .meeting), "Follow up")
        XCTAssertEqual(MarkerKind.test.label(for: .personal), "Remember")
        XCTAssertEqual(MarkerKind.important.label(for: .personal), "Important")
        XCTAssertEqual(MarkerKind.question.label(for: .meeting), "Question")
        XCTAssertEqual(k.label, "Follow up", "a recording without a type is a meeting")
        m.kind = .class
        XCTAssertEqual(k.label, "On the test")
        XCTAssertEqual(k.kind, "test", "the stored kind never changes with the label")
        k.kind = "garbage"
        XCTAssertEqual(k.markerKind, .important, "unknown stored kinds read as Important")
        try context.save()
        XCTAssertEqual(m.orderedMarkers.count, 1)
    }

    func testMeetingDeleteRemovesMarkersAndStudyMaterials() throws {
        let m = meeting(["cells"])
        let k = MomentMarker(at: m.startedAt, kind: .question, note: "private")
        context.insert(k); k.meeting = m
        try StudyStore.save([(.terms, #"{"terms":[]}"#)], fingerprint: StudyInput(meeting: m).fingerprint, meeting: m, context: context)
        context.delete(m)
        try context.save()
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<MomentMarker>()), 0)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<StudyMaterial>()), 0)
    }

    func testInputUsesPlainTranscriptAndMarks() {
        let m = meeting(["Welcome to biology.", "the secret answer is \(RedactionText.markerToken(UUID()))"])
        m.courseName = "BIO 101"
        m.kind = .class
        let k = MomentMarker(at: m.startedAt.addingTimeInterval(12), kind: .test, note: "on the exam")
        context.insert(k); k.meeting = m
        let i = StudyInput(meeting: m)
        XCTAssertEqual(i.transcriptLines, ["[0:00] Welcome to biology.", "[0:10] the secret answer is [stricken from the record]"])
        XCTAssertEqual(i.marksBlock(), "[0:12] ✎ On the test: on the exam")
        XCTAssertEqual(i.courseName, "BIO 101")
        XCTAssertEqual(i.recordingKind, .class)
        // The same mark on a meeting reads Follow up
        m.kind = .meeting
        XCTAssertEqual(StudyInput(meeting: m).marksBlock(), "[0:12] ✎ Follow up: on the exam")
    }

    /// A Class keeps the lecture prompts word for word; Meeting and Personal
    /// get the same five parts, worded for any recording.
    func testReviewPromptsFollowTheType() {
        XCTAssertEqual(MeetingAI.studyBase, """
            You turn a lecture transcript into study material for a student. The transcript comes from speech \
            recognition: it has no speaker labels and may contain recognition errors; don't repeat obvious errors. \
            Each line starts with its time in the lecture as [m:ss]. Use only what the lecture says; never invent \
            facts, names, numbers, dates or examples. Text shown as [stricken from the record] was removed by the \
            student: never guess at or mention what it said. The student marked some moments while listening \
            (STUDENT MARKS): moments marked ✎ On the test matter most, then ★ Important; make sure they are covered. \
            Reply with only one JSON object: no Markdown, no code fence, no text before or after it.
            """)
        XCTAssertTrue(MeetingAI.condenseSystem.contains(#"the lecturer stresses ("this will be on the exam")"#))
        XCTAssertTrue(MeetingAI.studySystem(.summary).contains("Write lecture notes"))
        // Meeting and Personal never mention student, lecture, instructor or exam (same rule as the Mac)
        let schoolWords = try! NSRegularExpression(pattern: #"\b(students?|lectures?|lecturer|instructors?|exams?)\b"#, options: [.caseInsensitive])
        func mentionsSchool(_ s: String) -> Bool {
            schoolWords.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil
        }
        XCTAssertTrue(mentionsSchool(MeetingAI.studyBase), "the check itself works on the Class text")
        for kind in [RecordingKind.meeting, .personal] {
            for part in StudyKind.allCases {
                let system = MeetingAI.studySystem(part, for: kind)
                XCTAssertFalse(mentionsSchool(system), "\(kind) \(part)")
                XCTAssertTrue(system.contains("(MARKS)"))
            }
            XCTAssertFalse(mentionsSchool(MeetingAI.condenseSystem(for: kind)), "\(kind) condense")
            let input = StudyInput(title: "x", recordingKind: kind, durationMs: 1000, lines: [.init(ms: 0, text: "hi")],
                                   marks: [.init(ms: 0, kind: .test, note: nil)])
            XCTAssertFalse(mentionsSchool(MeetingAI.studyUserMessage(input, condensed: true, body: "")), "\(kind) message")
            XCTAssertFalse(mentionsSchool(MeetingAI.condenseMessage(input, chunk: "[0:00] hi", part: 1, parts: 1)), "\(kind) condense message")
        }
        XCTAssertTrue(MeetingAI.studyBase(for: .meeting).contains("✎ Follow up matter most"))
        XCTAssertTrue(MeetingAI.studyBase(for: .personal).contains("✎ Remember matter most"))
        let meeting = StudyInput(title: "Weekly sync", recordingKind: .meeting, courseName: "Acme project", durationMs: 60_000,
                                 lines: [.init(ms: 0, text: "ship on Friday")], marks: [.init(ms: 1000, kind: .test, note: nil)])
        let message = MeetingAI.studyUserMessage(meeting, condensed: true, body: "x")
        XCTAssertTrue(message.hasPrefix("Meeting: Weekly sync\nNotebook: Acme project\n"))
        XCTAssertTrue(message.contains("\nMARKS:\n[0:01] ✎ Follow up\n"))
        XCTAssertTrue(message.contains("\nNOTES (condensed from the transcript, with times):\nx"))
        XCTAssertTrue(MeetingAI.studyHeader(StudyInput(title: "Dentist", recordingKind: .personal, durationMs: 0, lines: [], marks: []))
            .hasPrefix("Recording: Dentist\n"))
    }

    func testReviewGuideExportIsTitledByType() {
        let review = StudyExport.guideMarkdown(title: "Sync", kind: .meeting, when: "", summary: nil, terms: nil, cards: nil,
                                               quiz: nil, asks: nil, marks: [.init(ms: 5000, kind: .test, note: nil)])
        XCTAssertTrue(review.hasPrefix("# Review guide: Sync\n"))
        XCTAssertTrue(review.contains("- 0:05 ✎ Follow up\n"))
        XCTAssertTrue(review.contains("from the transcript. AI can make mistakes; check against the recording."))
        XCTAssertEqual(StudyExportFile.markdown("x", title: "Sync", kind: .meeting).name, "Sync review guide.md")
        XCTAssertEqual(StudyExportFile.markdown("x", title: "Bio", kind: .class).name, "Bio study guide.md")
        XCTAssertEqual(RecordingKind.class.guideTitle, "Study guide")
        XCTAssertEqual(RecordingKind.personal.guideTitle, "Review guide")
    }

    func testStrikeDeletesTheStudyGuideAndAStaleSaveIsRefused() async throws {
        let m = meeting(["the Krebs cycle runs in the matrix", "glycolysis is in the cytoplasm"])
        let fp = StudyInput(meeting: m).fingerprint
        try StudyStore.save([(.summary, "{}"), (.flashcards, "{}")], fingerprint: fp, meeting: m, context: context)
        XCTAssertEqual(m.studyMaterials.count, 2)
        // Strike two words
        let seg = m.orderedSegments[0]
        let toks = RedactionText.tokens(seg.text)
        try await RedactionEngine.strike(.words(seg, 1...2), reason: nil, meeting: m, context: context)
        XCTAssertFalse(toks.isEmpty)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<StudyMaterial>()), 0)
        // A guide made before the edit is never saved after it
        XCTAssertThrowsError(try StudyStore.save([(.terms, "{}")], fingerprint: fp, meeting: m, context: context)) {
            XCTAssertEqual($0 as? StudyStore.Failure, .transcriptChanged)
        }
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<StudyMaterial>()), 0)
    }

    func testDeleteKeepsTheGuideDuringUndoAndDeletesItOnCommit() async throws {
        let m = meeting(["mitochondria make ATP", "ribosomes make proteins"])
        try StudyStore.save([(.quiz, "{}")], fingerprint: StudyInput(meeting: m).fingerprint, meeting: m, context: context)
        let pending = try RedactionEngine.delete(.lines([m.orderedSegments[1]]), meeting: m, context: context)
        XCTAssertEqual(m.studyMaterials.count, 1, "kept while Undo is possible")
        try await RedactionEngine.commit(pending, context: context)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<StudyMaterial>()), 0)
        // Deleting a photo alone keeps a guide (it is made from the transcript)
        try StudyStore.save([(.quiz, "{}")], fingerprint: StudyInput(meeting: m).fingerprint, meeting: m, context: context)
        let snap = Snapshot(fileName: "nope-\(UUID().uuidString).jpg")
        context.insert(snap); snap.meeting = m
        let p2 = try RedactionEngine.delete(.screens([snap]), meeting: m, context: context)
        try await RedactionEngine.commit(p2, context: context)
        XCTAssertEqual(m.studyMaterials.count, 1)
    }
}
