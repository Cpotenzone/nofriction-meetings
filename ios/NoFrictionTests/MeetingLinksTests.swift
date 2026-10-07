import SwiftData
import XCTest
@testable import noFriction

/// Links (docs/LINKS.md), iOS side. The detection cases are the Mac's file
/// (`src-tauri/src/meeting_links/detection_cases.json`, bundled into this
/// test target), so both platforms detect the same links.
final class LinkDetectorTests: XCTestCase {
    private struct Cases: Decodable {
        struct Detect: Decodable { let text: String; let spoken: Bool; let keys: [String] }
        struct Normalize: Decodable { let input: String; let key: String?; let url: String? }
        let detect: [Detect]
        let normalize: [Normalize]
    }

    private func cases() throws -> Cases {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "detection_cases", withExtension: "json"),
                                "detection_cases.json must be in the test bundle (ios/project.yml)")
        return try JSONDecoder().decode(Cases.self, from: Data(contentsOf: url))
    }

    func testSharedDetectionCases() throws {
        let all = try cases().detect
        XCTAssertGreaterThan(all.count, 40)
        for c in all {
            let got = LinkDetector.detect(c.text, spoken: c.spoken).map(\.link.key)
            XCTAssertEqual(got, c.keys, "\(c.text) (spoken \(c.spoken))")
        }
    }

    func testSharedNormalizeCases() throws {
        let all = try cases().normalize
        XCTAssertGreaterThan(all.count, 20)
        for c in all {
            let n = LinkDetector.normalize(c.input)
            XCTAssertEqual(n?.key, c.key, c.input)
            XCTAssertEqual(n?.url, c.url, c.input)
        }
    }

    func testEveryMentionInOrder() {
        let found = LinkDetector.detect("example.com then test dot org then https://example.com/", spoken: true)
        XCTAssertEqual(found.map(\.link.key), ["example.com", "test.org", "example.com"])
        XCTAssertEqual(found.map(\.pos), found.map(\.pos).sorted())
    }

    func testOnlyHttpAndHttpsOpen() {
        for ok in ["https://example.com", "http://example.org/a?b=1", "HTTPS://EXAMPLE.COM/X", "https://localhost:3000/"] {
            XCTAssertTrue(LinkDetector.isOpenable(ok), ok)
        }
        for bad in ["javascript:alert(1)", "JAVASCRIPT:alert(1)", "file:///etc/passwd", "data:text/html,<b>x</b>",
                    "vbscript:msgbox(1)", "mailto:jane@example.com", "ftp://example.com/file", "about:blank",
                    "example.com", " https://example.com", "https://example.com\n", "https://user:pw@example.com/",
                    "https://", ""] {
            XCTAssertFalse(LinkDetector.isOpenable(bad), bad)
        }
    }

    func testReferenceURL() throws {
        XCTAssertEqual(try MeetingLinks.referenceURL("example.com/syllabus"), "https://example.com/syllabus")
        XCTAssertEqual(try MeetingLinks.referenceURL("  http://example.edu/a "), "http://example.edu/a")
        XCTAssertEqual(try MeetingLinks.referenceURL("https://example.com/r?utm_source=x"), "https://example.com/r?utm_source=x",
                       "the user's own address is kept as typed")
        for bad in ["javascript:alert(1)", "file:///a.pdf", "mailto:a@b.co", "not a link", "", "ftp://x.org"] {
            XCTAssertThrowsError(try MeetingLinks.referenceURL(bad), bad)
        }
    }

    func testShortPath() {
        XCTAssertEqual(MeetingLinks.shortPath("/math"), "/math")
        XCTAssertEqual(MeetingLinks.shortPath("/courses/biology-101/modules/week-3/lecture-notes-and-slides"),
                       "/courses/…/lecture-notes-and-slides")
        XCTAssertEqual(MeetingLinks.shortPath("/" + String(repeating: "x", count: 80)).count, 36)
    }
}

@MainActor
final class MeetingLinksStoreTests: XCTestCase {
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

    private func makeMeeting(_ lines: [String]) -> Meeting {
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let m = Meeting(title: "Biology 101", startedAt: start)
        m.endedAt = start.addingTimeInterval(300)
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

    func testSaidLinksAndReferencesMerge() throws {
        let m = makeMeeting([
            "The syllabus is at example dot edu slash bio101",
            "Practice on quizlet.com and again example.edu/bio101",
        ])
        try MeetingLinks.add(to: m, url: "https://www.example.edu/bio101?utm_source=x", title: " Syllabus ", note: "Due Friday", context: context)
        try MeetingLinks.add(to: m, url: "openstax.org/books/biology-2e", title: nil, note: nil, context: context)

        let items = MeetingLinks.items(for: m)
        XCTAssertEqual(items.map(\.key), ["example.edu/bio101", "openstax.org/books/biology-2e", "quizlet.com"])
        XCTAssertEqual(items[0].sources, [.added, .said])
        XCTAssertEqual(items[0].title, "Syllabus")
        XCTAssertEqual(items[0].note, "Due Friday")
        XCTAssertEqual(items[0].saidCount, 2)
        XCTAssertEqual(items[0].firstSaid, m.startedAt)
        XCTAssertEqual(items[0].url, "https://www.example.edu/bio101?utm_source=x")
        XCTAssertEqual(items[1].sources, [.added])
        XCTAssertEqual(items[1].url, "https://openstax.org/books/biology-2e")
        XCTAssertEqual(items[2].sources, [.said])
        XCTAssertEqual(items[2].firstSaid, m.startedAt.addingTimeInterval(10))
        XCTAssertThrowsError(try MeetingLinks.add(to: m, url: "javascript:alert(1)", title: nil, note: nil, context: context))
        XCTAssertEqual(m.references.count, 2)
    }

    func testEditAndDeleteReference() throws {
        let m = makeMeeting([])
        let r = try MeetingLinks.add(to: m, url: "example.com/a", title: "A", note: nil, context: context)
        try MeetingLinks.update(r, url: "https://example.edu/reading.pdf", title: "", note: "pages 4-9", context: context)
        XCTAssertEqual(r.url, "https://example.edu/reading.pdf")
        XCTAssertNil(r.title)
        XCTAssertEqual(r.note, "pages 4-9")
        XCTAssertThrowsError(try MeetingLinks.update(r, url: "file:///etc/hosts", title: nil, note: nil, context: context))
        XCTAssertEqual(r.url, "https://example.edu/reading.pdf", "a refused edit changes nothing")
        try MeetingLinks.delete(r, context: context)
        XCTAssertTrue(MeetingLinks.items(for: m).isEmpty)
        XCTAssertEqual(try context.fetchCount(FetchDescriptor<MeetingReference>()), 0)
    }

    func testReferencesGoWithTheMeeting() throws {
        let m = makeMeeting(["see example.com"])
        let other = makeMeeting(["nothing here"])
        try MeetingLinks.add(to: m, url: "https://example.edu/syllabus", title: "Syllabus", note: "private note", context: context)
        try MeetingLinks.add(to: other, url: "https://example.org", title: nil, note: nil, context: context)
        context.delete(m)
        try context.save()
        let left = try context.fetch(FetchDescriptor<MeetingReference>())
        XCTAssertEqual(left.map(\.url), ["https://example.org"])
    }

    func testDeleteAndStrikeRemoveSaidLinks() async throws {
        let m = makeMeeting(["for practice go to quizlet dot com today", "the answer key is at secret-answers.com now"])
        try MeetingLinks.add(to: m, url: "https://example.edu/syllabus", title: "Syllabus", note: nil, context: context)
        XCTAssertEqual(MeetingLinks.items(for: m).map(\.key), ["example.edu/syllabus", "quizlet.com", "secret-answers.com"])

        // Delete "quizlet dot com" (tokens 4...6)
        let first = m.orderedSegments[0]
        _ = try RedactionEngine.delete(.words(first, 4...6), meeting: m, context: context)
        XCTAssertEqual(MeetingLinks.items(for: m).map(\.key), ["example.edu/syllabus", "secret-answers.com"])

        // Strike "secret-answers.com" (token 5): no hidden copy, no link
        let second = m.orderedSegments[1]
        try await RedactionEngine.strike(.words(second, 5...5), reason: nil, meeting: m, context: context)
        XCTAssertEqual(MeetingLinks.items(for: m).map(\.key), ["example.edu/syllabus"],
                       "references are the user's own; derived links go with the words")
    }

    func testNoLinkAcrossAStrikeMarker() async throws {
        let m = makeMeeting(["the site is example dot org for notes"])
        let seg = m.orderedSegments[0]
        try await RedactionEngine.strike(.words(seg, 4...4), reason: nil, meeting: m, context: context)
        XCTAssertTrue(seg.text.contains("⟦stricken:"))
        XCTAssertTrue(MeetingLinks.items(for: m).isEmpty)
    }
}
