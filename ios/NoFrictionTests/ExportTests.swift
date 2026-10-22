import SwiftData
import XCTest
@testable import noFriction

/// docs/EXPORT.md, iOS side: the package (same layout as the Mac), its zip,
/// and the AI tools' answer checks. The AI is never called here.
final class ExportPackageTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory.appending(path: "nf-export-test-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
    }

    private func input() throws -> ExportPackage.Input {
        let photo = dir.appending(path: "a.JPG")
        try Data("jpeg".utf8).write(to: photo)
        let screen = dir.appending(path: "b.png")
        try Data("png".utf8).write(to: screen)
        var i = ExportPackage.Input(title: "Q3 plan: <review> & next steps")
        i.when = "Tuesday, 21 October 2026 at 14:00"
        i.duration = "42 min"
        i.people = ["Ana Ruiz"]
        i.notes = "## Summary\nWe agreed to ship *beta* in May.\n## Action items\n- [Ana] Draft the plan"
        i.highlights = [.init(time: 723, label: "Beta date agreed", quote: "let's ship the beta in May")]
        i.pictures = [.init(fileURL: screen, time: 61, isPhoto: false, caption: "The roadmap"),
                      .init(fileURL: photo, time: 3725, isPhoto: true, caption: nil)]
        i.marks = [.init(time: 90, symbol: "★", label: "Important", note: "budget")]
        i.links = [.init(url: "https://example.com/plan", title: "The plan", time: 120),
                   .init(url: "javascript:alert(1)", title: nil, time: nil)]
        i.email = .init(subject: "Recap", body: "Hi all,\n\nThanks.")
        i.guideMarkdown = "# Review guide\n"
        i.flashcardsCSV = "front,back\n"
        i.transcript = [.init(time: 0, text: "Hello"), .init(time: 65, text: "[stricken from the record] ok")]
        i.hasAI = true
        return i
    }

    func testNamesMatchTheMac() {
        XCTAssertEqual(ExportPackage.fileStem("Q3 plan: review/next"), "Q3 plan review next")
        XCTAssertEqual(ExportPackage.fileStem("???"), "Recording")
        let p = ExportPackage.Picture(fileURL: URL(filePath: "/x/IMG.HEIC"), time: 3725, isPhoto: true)
        XCTAssertEqual(ExportPackage.imageName(2, p), "screens/03-photo-1-02-05.heic")
        let s = ExportPackage.Picture(fileURL: URL(filePath: "/x/noext"), time: 1, isPhoto: false)
        XCTAssertEqual(ExportPackage.imageName(0, s), "screens/01-screen-0-01.jpg")
        var c = DateComponents(); c.year = 2026; c.month = 10; c.day = 21; c.hour = 14
        let date = Calendar.current.date(from: c)!
        XCTAssertEqual(ExportPackage.folderName(date: date, title: "Weekly sync"), "2026-10-21 Weekly sync")
    }

    func testMarkdownHasEveryPartEscapedAndInOrder() throws {
        let md = ExportPackage.markdown(try input())
        XCTAssertTrue(md.hasPrefix("# Q3 plan: \\<review\\> & next steps\n"))
        XCTAssertTrue(md.contains("Meeting · Tuesday, 21 October 2026 at 14:00 · 42 min"))
        let order = ["## Highlights", "## Notes", "### Summary", "## Screens and photos", "## Marked moments", "## Links", "## Follow-up email", "## Also in this package"]
        let pos = order.map { h -> String.Index in
            guard let r = md.range(of: h) else { XCTFail("\(h) missing"); return md.startIndex }
            return r.lowerBound
        }
        XCTAssertEqual(pos, pos.sorted(), "sections out of order")
        XCTAssertTrue(md.contains("- **12:03** Beta date agreed: “let's ship the beta in May”"))
        XCTAssertTrue(md.contains("![The roadmap](screens/01-screen-1-01.png)"))
        XCTAssertTrue(md.contains("![photo at 1:02:05](screens/02-photo-1-02-05.jpg)"))
        XCTAssertTrue(md.contains("- [The plan](<https://example.com/plan>) (2:00)"))
        XCTAssertTrue(md.contains("[Transcript](transcript.txt)"))
        XCTAssertTrue(md.contains("Parts were written by AI"))
    }

    func testHTMLEscapesAndOnlyLinksWebURLs() throws {
        let h = ExportPackage.html(try input())
        XCTAssertTrue(h.contains("<title>Q3 plan: &lt;review&gt; &amp; next steps</title>"))
        XCTAssertFalse(h.contains("<review>"))
        XCTAssertTrue(h.contains("<a href=\"https://example.com/plan\">The plan</a>"))
        XCTAssertFalse(h.contains("href=\"javascript"))
        XCTAssertFalse(h.contains("<script"))
        XCTAssertTrue(h.contains("<h3>Summary</h3>"))
        XCTAssertTrue(h.contains("<li>[Ana] Draft the plan</li>"))
        let notes = ExportPackage.simpleMarkdownHTML("<img src=x onerror=alert(1)>\n**Next:**\n- **<b>**")
        XCTAssertTrue(notes.contains("&lt;img src=x onerror=alert(1)&gt;"))
        XCTAssertTrue(notes.contains("<h3>Next</h3>"))
        XCTAssertTrue(notes.contains("<li><strong>&lt;b&gt;</strong></li>"))
    }

    func testAINotesAreInertInMarkdown() {
        var i = ExportPackage.Input(title: "T")
        i.notes = "## Plan\n<img src=\"https://x.example/t.png\">\n![pixel](https://x.example/p.png)\n- see [the doc](https://example.com)"
        let md = ExportPackage.markdown(i)
        XCTAssertTrue(md.contains("\\<img src=\"https://x.example/t.png\"\\>"), md)
        XCTAssertTrue(md.contains("\\![pixel](https://x.example/p.png)"))
        XCTAssertTrue(md.contains("- see [the doc](https://example.com)"), "links stay links")
        XCTAssertTrue(md.contains("### Plan"))
    }

    func testWritesTheFolderAndZipsIt() throws {
        let i = try input()
        let names = ExportPackage.files(i).map(\.0)
        XCTAssertEqual(names, ["Q3 plan review next steps.md", "Q3 plan review next steps.html",
                               "screens/01-screen-1-01.png", "screens/02-photo-1-02-05.jpg",
                               "transcript.txt", "Review guide.md", "flashcards.csv"])
        let folder = try ExportPackage.writeFolder(i, name: "2026-10-21 Q3", in: dir)
        XCTAssertEqual(try Data(contentsOf: folder.appending(path: "screens/02-photo-1-02-05.jpg")), Data("jpeg".utf8))
        let tx = try String(contentsOf: folder.appending(path: "transcript.txt"), encoding: .utf8)
        XCTAssertTrue(tx.contains("[0:00] Hello\n"))
        XCTAssertTrue(tx.contains("[1:05] [stricken from the record] ok\n"))
        let zip = try ExportPackage.zip(folder: folder)
        XCTAssertEqual(zip.lastPathComponent, "2026-10-21 Q3.zip")
        let bytes = try Data(contentsOf: zip)
        XCTAssertGreaterThan(bytes.count, 100)
        XCTAssertEqual(Array(bytes.prefix(2)), [0x50, 0x4B], "a zip file")

        // Nothing optional: just the document and the page
        let bare = ExportPackage.Input(title: "Solo")
        XCTAssertEqual(ExportPackage.files(bare).map(\.0), ["Solo.md", "Solo.html"])
        XCTAssertTrue(ExportPackage.markdown(bare).hasSuffix("_Made with noFriction._\n"))
    }

    @MainActor
    func testWorkFolderIsClearedByAStrikeOrDeleteScrub() throws {
        try FileManager.default.createDirectory(at: ExportPackage.workFolder, withIntermediateDirectories: true)
        try Data("x".utf8).write(to: ExportPackage.workFolder.appending(path: "leftover.txt"))
        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        _ = StoreHygiene.scrub(container.mainContext)
        XCTAssertFalse(FileManager.default.fileExists(atPath: ExportPackage.workFolder.path))
    }

    // MARK: AI answers

    func testHighlightsKeepOnlyRealQuotesAndValidParts() throws {
        let parts: [ExportAI.Part] = [
            .init(time: 0, text: "Let's ship the beta in May, everyone agrees."),
            .init(time: 60, text: "The budget is [stricken from the record] for now."),
            .init(time: 120, text: "I'll draft the plan by Friday."),
        ]
        let raw = #"""
        Sure! ```json
        {"highlights":[
          {"part":3,"label":"Plan owner","quote":"I'll draft the plan by Friday"},
          {"part":1,"label":"- Beta date","quote":"“let’s ship the beta in may”"},
          {"part":1,"label":"duplicate","quote":"x"},
          {"part":2,"label":"Budget","quote":"The budget is [stricken from the record]"},
          {"part":9,"label":"out of range"},
        ]}
        ```
        """#
        let hs = try ExportAI.parseHighlights(raw, parts: parts)
        XCTAssertEqual(hs.map(\.time), [0, 60, 120])
        XCTAssertEqual(hs[0].label, "Beta date")
        XCTAssertEqual(hs[0].quote, "let’s ship the beta in may", "the model's quote marks are dropped (the package adds its own)")
        XCTAssertNil(hs[1].quote, "a stricken quote is dropped")
        XCTAssertEqual(hs[2].quote, "I'll draft the plan by Friday")
        XCTAssertThrowsError(try ExportAI.parseHighlights(#"{"highlights":[{"part":7,"label":"x"}]}"#, parts: parts))
        XCTAssertThrowsError(try ExportAI.parseHighlights("no json", parts: parts))
    }

    func testPartsCapTheirCount() {
        let long = (0..<5000).map { ExportPackage.Line(time: Double($0), text: String(repeating: "word ", count: 30)) }
        let ps = ExportAI.parts(long)
        XCTAssertLessThanOrEqual(ps.count, ExportAI.maxParts)
        XCTAssertEqual(ps.map(\.time), ps.map(\.time).sorted())
        XCTAssertTrue(ExportAI.parts([.init(time: 0, text: "  ")]).isEmpty)
        // A struck line or screen is a marker, not something anyone said
        XCTAssertTrue(ExportAI.parts([.init(time: 0, text: "[stricken from the record]"),
                                      .init(time: 1, text: "[screen stricken from the record]")]).isEmpty)
    }

    func testCaptionsAndSampling() throws {
        let raw = #"{"captions":[{"picture":2,"caption":"The roadmap"},{"picture":"1","caption":"Team photo"},{"picture":2,"caption":"dup"},{"picture":5,"caption":"nope"}]}"#
        let c = try ExportAI.parseCaptions(raw, count: 3, key: "captions")
        XCTAssertEqual(c.map(\.0), [0, 1])
        XCTAssertEqual(c.map(\.1), ["Team photo", "The roadmap"])
        XCTAssertEqual(ExportAI.sample(100, 5), [0, 24, 49, 74, 99])
        let lines = [ExportPackage.Line(time: 0, text: "too early"), .init(time: 50, text: "intro"), .init(time: 90, text: "the slide")]
        XCTAssertEqual(ExportAI.saidAround(.init(time: 80, end: 100, isPhoto: false), lines: lines, max: 500), "intro the slide")
        XCTAssertTrue(ExportAI.picksSystem(4).contains("at most 4 pictures"))
    }

    func testEmailSplitsTheSubjectLine() {
        let e = ExportSheet.splitEmail("Subject: Recap of Monday\n\nHi all,\nThanks.", fallbackSubject: "Weekly")
        XCTAssertEqual(e.subject, "Recap of Monday")
        XCTAssertEqual(e.body, "Hi all,\nThanks.")
        XCTAssertEqual(ExportSheet.splitEmail("Hi all", fallbackSubject: "Weekly").subject, "Weekly")
    }
}
