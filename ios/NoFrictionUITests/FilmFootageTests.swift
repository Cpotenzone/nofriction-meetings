import StoreKitTest
import XCTest

/// Launch film and App Store app-preview footage, one test per clip, paced
/// for video (deliberate taps, a held frame on every state). Run by
/// marketing/film/capture/ios/capture.sh, which records the Simulator while
/// each test runs and cuts the clip at the times the test logs.
///
/// Skipped unless NF_FILM=1 (xcodebuild: TEST_RUNNER_NF_FILM=1), so the
/// regular UI test runs never pay for it. Output (host paths):
///   NF_FILM_DIR/<clip>.times   "start" / "still" / "end" marks, wall-clock seconds
///   NF_FILM_DIR/<clip>.png     the clip's best frame, full resolution
///   NF_FILM_STILLS_DIR/NN-name.png   App Store screenshots (testStoreStills)
///
/// Sample data only (-NFSeedDemo -NFFilm): invented people, notebooks and
/// lectures. The Pro clips (notes, Chat, Sync) buy Pro in the local StoreKit
/// test ledger (NoFriction.storekit; no account, no money) and point the
/// app's one AI endpoint at capture.sh's stand-in on this Mac
/// (NF_FILM_AI_URL, capture/ios/ai-stub.py: fixed demo text, no model).
/// Never opens People, the paywall, AI settings or a share sheet.
final class FilmFootageTests: XCTestCase {
    private var clip = ""
    private var marks: [(String, Double)] = []
    private var storeSession: SKTestSession?

    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["NF_FILM"] == "1", "Film footage only: set NF_FILM=1")
        continueAfterFailure = false
    }

    override func tearDown() {
        storeSession?.clearTransactions()
        storeSession = nil
        super.tearDown()
    }

    // MARK: Helpers

    private func launch(_ clip: String, _ extra: [String] = []) -> XCUIApplication {
        self.clip = clip
        marks = []
        let app = XCUIApplication()
        // Remembered Record-sheet choices and one-time notices pinned through the
        // argument domain, so every run looks the same.
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-NFFilm",
                               "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-AppleICUForce24HourTime", "NO",
                               "-recordingNoticeAccepted", "YES", "-classRecordingNoticeShown", "YES",
                               "-recordingKindDefault", "meeting", "-recordingDefaultLength", "none"] + extra
        app.launch()
        return app
    }

    /// noFriction Pro from the local StoreKit test ledger (the yearly plan),
    /// before the app launches. Cleared again in tearDown.
    private func buyPro() throws {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "NoFriction", withExtension: "storekit"))
        let session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.clearTransactions()
        session.disableDialogs = true
        session.locale = Locale(identifier: "en_US")
        session.storefront = "USA"
        storeSession = session
        let bought = expectation(description: "Pro bought in the StoreKit test ledger")
        Task {
            do { try await session.buyProduct(identifier: "com.nofriction.meetings.pro.yearly") } catch {
                XCTFail("StoreKit test purchase failed: \(error)")
            }
            bought.fulfill()
        }
        wait(for: [bought], timeout: 30)
    }

    /// The app's one custom AI endpoint (AISettings' stored form), pointed at
    /// capture.sh's local stand-in: http://localhost:<port>/v1, no key. A
    /// loopback endpoint needs no consent (URLPolicy).
    private func aiEndpointArgs() throws -> [String] {
        let base = try XCTUnwrap(ProcessInfo.processInfo.environment["NF_FILM_AI_URL"],
                                 "NF_FILM_AI_URL missing: run through capture.sh")
        let saved = #"{"custom":{"baseURL":"\#(base)","model":"film-demo","models":[{"id":"film-demo","contextTokens":32768}]}}"#
        let hex = Data(saved.utf8).map { String(format: "%02x", $0) }.joined()
        return ["-ai.providers", "<\(hex)>", "-ai.activeProvider", "custom"]
    }

    /// A cut point for capture.sh.
    private func mark(_ name: String) {
        marks.append((name, Date().timeIntervalSince1970))
        guard let dir = ProcessInfo.processInfo.environment["NF_FILM_DIR"] else { return }
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let text = marks.map { "\($0.0) \(String(format: "%.3f", $0.1))" }.joined(separator: "\n") + "\n"
        try? text.write(toFile: "\(dir)/\(clip).times", atomically: true, encoding: .utf8)
    }

    /// The clip's still (and the moment, for reference).
    private func still() {
        mark("still")
        saveShot(clip, envVar: "NF_FILM_DIR")
    }

    private func hold(_ seconds: Double) { Thread.sleep(forTimeInterval: seconds) }

    private func element(_ app: XCUIApplication, labelContains text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", text)).firstMatch
    }

    /// A slow, human scroll: drag up by `fraction` of the screen and let go
    /// without a fling.
    private func slowScroll(_ app: XCUIApplication, from: CGFloat = 0.78, fraction: CGFloat = 0.4, pointsPerSecond: CGFloat = 260) {
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: from))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: from - fraction))
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: XCUIGestureVelocity(pointsPerSecond), thenHoldForDuration: 0.25)
    }

    /// The Record sheet opens at half height: drag it up to full height, as a person would.
    private func expandRecordSheet(_ app: XCUIApplication, pointsPerSecond: CGFloat = 900) {
        let title = app.staticTexts["What is it?"].firstMatch
        title.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08)),
                   withVelocity: XCUIGestureVelocity(pointsPerSecond), thenHoldForDuration: 0.1)
    }

    /// Recordings tab, waiting for the sample library.
    private func openRecordings(_ app: XCUIApplication) {
        app.buttons["Recordings"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Lecture 7: Cellular respiration"].firstMatch.waitForExistence(timeout: 10), "film sample data missing")
    }

    private func notebookChip(_ app: XCUIApplication, _ name: String) -> XCUIElement {
        app.descendants(matching: .any)["notebook-filter"].firstMatch.buttons[name].firstMatch
    }

    // MARK: iPhone clips
    //
    // The iPhone app preview, in story order: Record sheet → recording, Mark,
    // Stop → Recordings: a notebook and a search that opens at the line →
    // a meeting's Notes and its follow-up email → a class's study guide →
    // Chat with a cited answer → Settings → Sync with your Mac.
    //
    // Holds are short on purpose: every XCUITest action also waits for the
    // app to settle (about 0.5–1 s), which the video shows as a held frame.
    // Inside a clip, checks use `exists` (immediate) rather than
    // waitForExistence, which adds about a second even when the element is there.

    /// Record → the Record sheet: Class, 60 min, the BIO 101 notebook, Capture
    /// screen (left off: its broadcast sheet is the system's) → Record → the
    /// recording starts and its first words arrive.
    func testClip01RecordSheet() {
        let app = launch("ios-01-record-sheet")
        let record = app.buttons["record-button"].firstMatch
        XCTAssertTrue(record.waitForExistence(timeout: 10))
        hold(1.0)
        mark("start")
        hold(0.4)
        record.tap()
        XCTAssertTrue(app.staticTexts["What is it?"].firstMatch.exists, "Record sheet didn't open")
        XCTAssertTrue(app.switches["record-capture-screen"].firstMatch.exists, "Capture screen missing from the Record sheet")
        hold(0.3)
        app.buttons["Class"].firstMatch.tap()
        hold(0.2)
        app.buttons["record-length-60"].firstMatch.tap()
        hold(0.2)
        app.buttons["BIO 101"].firstMatch.tap()
        hold(0.9)
        still()
        app.buttons["record-plan-start"].firstMatch.tap()
        XCTAssertTrue(app.buttons["mark-button"].firstMatch.exists, "recording didn't start")
        hold(4.4)
        saveShot("ios-01b-record-start", envVar: "NF_FILM_DIR")
        hold(0.4)
        mark("end")
    }

    /// A class being recorded (60-minute limit, time left), lines arriving:
    /// Mark this moment (★ Important) → On the test → Stop.
    func testClip03Mark() {
        let app = launch("ios-03-mark", ["-NFDemoLive", "-NFDemoLiveClass"])
        let markButton = app.buttons["mark-button"].firstMatch
        XCTAssertTrue(markButton.waitForExistence(timeout: 10))
        hold(0.6)
        mark("start")
        hold(2.4)
        markButton.tap()
        let test = app.buttons["On the test"].firstMatch
        XCTAssertTrue(test.exists, "mark kinds didn't show")
        hold(1.0)
        test.tap()
        hold(0.8)
        still()
        hold(1.0)
        mark("stop")
        app.buttons["record-button"].firstMatch.tap()
        XCTAssertFalse(app.buttons["mark-button"].firstMatch.exists, "the recording didn't stop")
        hold(1.2)
        mark("end")
    }

    /// Recordings → the BIO 101 notebook chip → search "exam" → the matching
    /// line under Lecture 7 → it opens at that moment, the line highlighted.
    func testClip04Library() {
        let app = launch("ios-04-library")
        openRecordings(app)
        let field = app.searchFields.firstMatch
        if !field.waitForExistence(timeout: 2) { app.swipeDown() }
        XCTAssertTrue(field.waitForExistence(timeout: 5), "no search field")
        hold(1.0)
        mark("start")
        hold(0.8)
        notebookChip(app, "BIO 101").tap()
        XCTAssertTrue(app.staticTexts["Lecture 5: Cell membranes"].firstMatch.exists)
        hold(1.0)
        field.tap()
        hold(0.3)
        // Letter by letter, as a person types (and so the list never flashes
        // "No results" between the keystrokes and the search)
        for letter in ["e", "x", "a", "m"] {
            field.typeText(letter)
            hold(0.25)
        }
        let line = app.descendants(matching: .any).matching(identifier: "search-line").firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 5), "no matching line under the result")
        hold(1.0)
        still()
        hold(0.4)
        line.tap()
        let target = app.descendants(matching: .any)["transcript-search-line"].firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 5), "the line isn't in the transcript")
        hold(2.4)
        mark("end")
    }

    /// The study guide: summary → flashcards (flip one) → practice quiz (answered, with the explanation).
    func testClip06Review() {
        let app = launch("ios-06-review")
        openRecordings(app)
        app.staticTexts["Lecture 7: Cellular respiration"].firstMatch.tap()
        let open = app.buttons["study-open"].firstMatch
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        hold(1.0)
        mark("start")
        hold(0.4)
        open.tap()
        XCTAssertTrue(app.staticTexts["Cellular respiration"].firstMatch.exists, "guide didn't open")
        hold(1.0)
        app.buttons["Flashcards"].firstMatch.tap()
        hold(0.2)
        let card = app.buttons["flashcard"].firstMatch
        card.tap()
        // The answer side, shown in full: capture.sh never shortens keep+…keep-
        XCTAssertTrue(card.label.hasPrefix("Answer"), "the flashcard didn't flip")
        mark("keep+")
        hold(1.6)
        mark("keep-")
        app.buttons["Practice quiz"].firstMatch.tap()
        hold(0.6)
        element(app, labelContains: "The electron transport chain").tap()
        XCTAssertTrue(app.staticTexts["Right."].firstMatch.exists)
        hold(0.8)
        still()
        hold(1.2)
        mark("end")
    }

    /// A meeting's Notes (summary, decisions, action items) with Make again
    /// and Follow-up email → the follow-up email, drafted (Pro).
    func testClip08Meeting() throws {
        try buyPro()
        let app = launch("ios-08-meeting", try aiEndpointArgs())
        openRecordings(app)
        let meeting = app.staticTexts["Brightwater pilot kickoff"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        hold(1.0)
        mark("start")
        hold(0.6)
        meeting.tap()
        XCTAssertTrue(element(app, labelContains: "Pilot with five inspectors").exists)
        hold(1.2)
        slowScroll(app, from: 0.7, fraction: 0.22, pointsPerSecond: 260)
        let email = app.buttons["ai-email"].firstMatch
        XCTAssertTrue(email.exists && email.isHittable, "Follow-up email isn't on screen")
        XCTAssertTrue(app.buttons["ai-summarize"].firstMatch.label.contains("Make again"), "no Make again")
        hold(0.8)
        still()
        email.tap()
        XCTAssertTrue(app.navigationBars["Follow-up email"].firstMatch.waitForExistence(timeout: 10), "the email didn't open")
        XCTAssertTrue(element(app, labelContains: "Brightwater pilot: next steps").waitForExistence(timeout: 5))
        hold(2.4)
        mark("end")
    }

    /// Chat: a suggested question across all recordings → the answer, with
    /// citation chips → a citation opens the recording at that moment (Pro).
    func testClip09Chat() throws {
        try buyPro()
        let app = launch("ios-09-chat", try aiEndpointArgs())
        XCTAssertTrue(app.buttons["Chat"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["Chat"].firstMatch.tap()
        let ask = app.buttons.matching(identifier: "chat-suggestion").matching(NSPredicate(format: "label CONTAINS %@", "deadlines")).firstMatch
        XCTAssertTrue(ask.waitForExistence(timeout: 5), "no deadlines suggestion")
        hold(1.0)
        mark("start")
        hold(0.8)
        ask.tap()
        let citation = app.buttons["chat-citation"].firstMatch
        XCTAssertTrue(citation.waitForExistence(timeout: 15), "no cited answer")
        // The answer, shown in full: capture.sh never shortens keep+…keep-
        mark("keep+")
        hold(2.4)
        mark("keep-")
        still()
        hold(0.6)
        citation.tap()
        XCTAssertTrue(app.navigationBars.buttons["Chat"].firstMatch.waitForExistence(timeout: 5), "the citation didn't open its recording")
        hold(2.0)
        mark("end")
    }

    /// Settings → Sync with your Mac: the pairing screen (Pro). The clip
    /// starts with Settings already scrolled to Sync.
    func testClip10Sync() throws {
        try buyPro()
        let app = launch("ios-10-sync")
        XCTAssertTrue(app.buttons["Settings"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].firstMatch.waitForExistence(timeout: 5))
        // Sync in the upper half of the screen, the AI section scrolled away
        // (the form builds its rows as they come on screen)
        let row = app.buttons["sync-row"].firstMatch
        for _ in 0..<16 {
            if row.exists, row.isHittable, row.frame.minY > app.frame.height * 0.2, row.frame.midY < app.frame.height * 0.6 { break }
            slowScroll(app, from: 0.75, fraction: 0.2, pointsPerSecond: 900)
        }
        XCTAssertTrue(row.exists && row.isHittable, "Sync with your Mac isn't on screen")
        hold(1.2)
        mark("start")
        hold(1.0)
        row.tap()
        XCTAssertTrue(app.buttons["sync-pair"].firstMatch.exists, "the Sync screen didn't open")
        hold(1.0)
        still()
        hold(2.0)
        mark("end")
    }

    // MARK: App Store screenshots (no video)

    /// Five 6.9" screenshots into NF_FILM_STILLS_DIR.
    func testStoreStills() {
        func shot(_ name: String) { saveShot(name, envVar: "NF_FILM_STILLS_DIR") }

        // 1. The Record sheet, Class · 60 min · BIO 101
        var app = launch("stills")
        XCTAssertTrue(app.buttons["record-button"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["record-button"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["What is it?"].firstMatch.waitForExistence(timeout: 5))
        expandRecordSheet(app)
        hold(0.8)
        app.buttons["Class"].firstMatch.tap()
        app.buttons["record-length-60"].firstMatch.tap()
        app.buttons["BIO 101"].firstMatch.tap()
        hold(1.0)
        shot("01-record-sheet")
        app.buttons["Cancel"].firstMatch.tap()
        hold(0.8)

        // 2. The lecture at the top of its page: title, notes, Review and the
        // Marks, nothing scrolled under the status bar
        openRecordings(app)
        app.staticTexts["Lecture 7: Cellular respiration"].firstMatch.tap()
        let moments = app.staticTexts["MARKS"].firstMatch
        XCTAssertTrue(app.buttons["study-open"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(moments.waitForExistence(timeout: 3) && moments.isHittable, "Marks not on the first screen")
        XCTAssertTrue(element(app, labelContains: "why oxygen").isHittable, "the last marked moment is cut off")
        hold(1.0)
        shot("02-class-marks")

        // 3. The study guide's summary; 4. an answered quiz question
        let open = app.buttons["study-open"].firstMatch
        open.tap()
        XCTAssertTrue(app.staticTexts["Cellular respiration"].firstMatch.waitForExistence(timeout: 5))
        hold(1.0)
        shot("03-review-guide")
        app.buttons["Practice quiz"].firstMatch.tap()
        let answer = element(app, labelContains: "The electron transport chain")
        XCTAssertTrue(answer.waitForExistence(timeout: 3))
        answer.tap()
        XCTAssertTrue(app.staticTexts["Right."].firstMatch.waitForExistence(timeout: 3))
        hold(1.0)
        shot("04-flashcards-quiz")
        app.terminate()

        // 5. Recordings, BIO 101 notebook selected
        app = launch("stills")
        openRecordings(app)
        notebookChip(app, "BIO 101").tap()
        XCTAssertTrue(app.staticTexts["Lecture 5: Cell membranes"].firstMatch.waitForExistence(timeout: 5))
        hold(1.0)
        shot("05-notebooks")
    }
}
