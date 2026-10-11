import SwiftData
import XCTest
@testable import noFriction

/// Search on Recordings: matching lines grouped by recording, opening at the line.
@MainActor
final class RecordingSearchTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
    }

    override func tearDown() async throws {
        container = nil
        context = nil
    }

    /// A recording whose lines are 60 s apart, the first at 0:20.
    @discardableResult
    private func makeMeeting(_ title: String, _ lines: [String], startedAt: Date? = nil) -> Meeting {
        let begin = startedAt ?? start
        let m = Meeting(title: title, startedAt: begin)
        context.insert(m)
        for (i, line) in lines.enumerated() {
            let s = Segment(text: line, start: begin.addingTimeInterval(Double(i) * 60 + 20), duration: 8)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    // MARK: Matching

    func testMatchingIgnoresCaseAndDiacritics() {
        XCTAssertTrue(RecordingSearch.matches("Send the Résumé by Friday", query: "resume"))
        XCTAssertTrue(RecordingSearch.matches("send the resume", query: "RÉSUMÉ"))
        XCTAssertTrue(RecordingSearch.matches("Meeting in São Paulo", query: "sao paulo"))
        XCTAssertTrue(RecordingSearch.matches("Launch date", query: "  launch  "), "the query is trimmed")
        XCTAssertFalse(RecordingSearch.matches("Launch date", query: "lunch"))
        XCTAssertFalse(RecordingSearch.matches("anything", query: "   "), "an empty query matches nothing")
        XCTAssertNil(RecordingSearch.normalized(" \n "))
    }

    func testStrikeMarkerNeverMatches() {
        let id = UUID()
        let text = "We need the \(RedactionText.markerToken(id)) by Friday."
        XCTAssertFalse(RecordingSearch.lineMatches(text, query: "stricken"))
        XCTAssertFalse(RecordingSearch.lineMatches(text, query: String(id.uuidString.prefix(8))))
        XCTAssertFalse(RecordingSearch.lineMatches(text, query: "record"), "the placeholder isn't searchable either")
        XCTAssertTrue(RecordingSearch.lineMatches(text, query: "friday"))
        XCTAssertEqual(RecordingSearch.searchableText(text), "We need the by Friday.")
    }

    func testDisplayTextShowsPlaceholderAndExcerptsLongLines() {
        let id = UUID()
        XCTAssertEqual(RecordingSearch.displayText("We need the \(RedactionText.markerToken(id)) by Friday.", query: "friday"),
                       "We need the \(RedactionText.placeholder) by Friday.")
        let long = "Okay so before we get going let me just say that this has been a long quarter for the whole team and the launch date moved twice."
        let shown = RecordingSearch.displayText(long, query: "launch date")
        XCTAssertTrue(shown.hasPrefix("…"), shown)
        XCTAssertTrue(shown.hasSuffix("the launch date moved twice."), shown)
        XCTAssertFalse(shown.dropFirst().hasPrefix(" "), "starts at a word")
        XCTAssertEqual(RecordingSearch.displayText("Short line about the launch.", query: "launch"), "Short line about the launch.")
    }

    // MARK: Grouping

    func testGroupingKeepsThreeEarliestLinesAndCountsTheRest() {
        let a = makeMeeting("Budget review", [
            "The budget is tight.", "Unrelated.", "Budget for Q4 is set.", "We cut the budget twice.",
            "Final budget next week.", "BUDGET approved.",
        ])
        let b = makeMeeting("Standup", ["No budget talk, just the build."])
        makeMeeting("Other", ["Nothing to see here."])

        let groups = RecordingSearch.group(RecordingSearch.lines(matching: "budget", in: context))
        XCTAssertEqual(groups.count, 2)
        let ga = try! XCTUnwrap(groups[a.id])
        XCTAssertEqual(ga.lines.map(\.text), ["The budget is tight.", "Budget for Q4 is set.", "We cut the budget twice."])
        XCTAssertEqual(ga.lines.map(\.offset), [20, 140, 200], "earliest first, from the recording's start")
        XCTAssertEqual(ga.moreLines, 2)
        XCTAssertEqual(groups[b.id]?.lines.count, 1)
        XCTAssertEqual(groups[b.id]?.moreLines, 0)
    }

    func testGroupSortsByTimeEvenIfGivenOutOfOrder() {
        let m = makeMeeting("T", ["alpha one", "alpha two", "alpha three", "alpha four"])
        let lines = RecordingSearch.lines(matching: "alpha", in: context)
        let g = RecordingSearch.group(lines.reversed(), maxLines: 2)[m.id]
        XCTAssertEqual(g?.lines.map(\.text), ["alpha one", "alpha two"])
        XCTAssertEqual(g?.moreLines, 2)
    }

    // MARK: Store

    func testStoreSearchIsCaseAndDiacriticInsensitive() {
        let m = makeMeeting("Hiring", ["Send me her Résumé tomorrow.", "Nothing else."])
        let lines = RecordingSearch.lines(matching: "RESUME", in: context)
        XCTAssertEqual(lines.map(\.text), ["Send me her Résumé tomorrow."])
        XCTAssertEqual(lines.first?.meetingID, m.id)
        XCTAssertEqual(lines.first?.id, m.orderedSegments.first?.persistentModelID, "the line opens at its segment")
    }

    func testStruckAndDeletedLinesNeverAppear() throws {
        let m = makeMeeting("Pricing", ["The price is forty dollars.", "The price is secret.", "Price list attached."])
        let segs = m.orderedSegments
        // Strike the words "forty dollars" (the marker replaces them in the text)
        let r = Redaction(kind: .words, action: .strike, mediaStart: 0, mediaEnd: 1, coveredFrom: segs[0].start, coveredTo: segs[0].start)
        context.insert(r)
        r.meeting = m
        segs[0].text = "The price is \(RedactionText.markerToken(r.id))"
        // Strike a whole line
        segs[1].text = RedactionText.markerToken(r.id)
        // Delete a line
        context.delete(segs[2])
        try context.save()

        XCTAssertTrue(RecordingSearch.lines(matching: "forty", in: context).isEmpty, "struck words")
        XCTAssertTrue(RecordingSearch.lines(matching: "secret", in: context).isEmpty, "struck line")
        XCTAssertTrue(RecordingSearch.lines(matching: "list attached", in: context).isEmpty, "deleted line")
        XCTAssertTrue(RecordingSearch.lines(matching: "stricken", in: context).isEmpty, "marker tokens")
        XCTAssertTrue(RecordingSearch.lines(matching: r.id.uuidString.lowercased(), in: context).isEmpty, "marker ids")
        // The words left on a partly struck line still match, shown with the placeholder
        XCTAssertEqual(RecordingSearch.lines(matching: "price is", in: context).map(\.text),
                       ["The price is \(RedactionText.placeholder)"])
    }

    func testLineDeletedButNotYetSavedIsSkipped() {
        let m = makeMeeting("Undo window", ["Keep this line.", "Remove this line."])
        context.delete(m.orderedSegments[1])   // Delete's 5-second undo window: not saved yet
        XCTAssertEqual(RecordingSearch.lines(matching: "line", in: context).map(\.text), ["Keep this line."])
    }

    func testScreenLinesAreSearchable() {
        let m = makeMeeting("Webinar", ["From the mic."])
        let s = Segment(text: "Quarterly numbers on the slide.", start: start.addingTimeInterval(90), duration: 5)
        s.source = Snapshot.Source.screen
        context.insert(s)
        s.meeting = m
        try? context.save()
        let lines = RecordingSearch.lines(matching: "quarterly", in: context)
        XCTAssertEqual(lines.count, 1)
        XCTAssertTrue(lines[0].isFromScreen)
        XCTAssertEqual(lines[0].accessibilityLabel, "At 1:30, on screen: Quarterly numbers on the slide.")
    }

    func testRecordingInProgressIsExcluded() {
        let live = makeMeeting("Live", ["roadmap now"])
        let past = makeMeeting("Past", ["roadmap before"])
        let lines = RecordingSearch.lines(matching: "roadmap", in: context, excluding: live.id)
        XCTAssertEqual(lines.map(\.meetingID), [past.id])
    }

    func testAccessibilityLabelReadsTheTime() {
        let m = makeMeeting("T", [], startedAt: start)
        let s = Segment(text: "Ship it.", start: start.addingTimeInterval(12 * 60 + 3), duration: 2)
        context.insert(s)
        s.meeting = m
        try? context.save()
        XCTAssertEqual(RecordingSearch.lines(matching: "ship", in: context).first?.accessibilityLabel, "At 12:03: Ship it.")
    }

    func testMeetingMatchesTitleNotebookAndTopicsIgnoringDiacritics() {
        let m = makeMeeting("Café planning", [])
        m.courseName = "BIO 101"
        XCTAssertTrue(RecordingSearch.meetingMatches(m, query: "cafe"))
        XCTAssertTrue(RecordingSearch.meetingMatches(m, query: "bio"))
        XCTAssertFalse(RecordingSearch.meetingMatches(m, query: "chemistry"))
    }
}
