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
/// lectures. Never opens People, Settings, the paywall or a share sheet.
final class FilmFootageTests: XCTestCase {
    private var clip = ""
    private var marks: [(String, Double)] = []

    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["NF_FILM"] == "1", "Film footage only: set NF_FILM=1")
        continueAfterFailure = false
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
    // Holds are short on purpose: every XCUITest action also waits for the
    // app to settle (about 0.5–1 s), which the video shows as a held frame.
    // Inside a clip, checks use `exists` (immediate) rather than
    // waitForExistence, which adds about a second even when the element is there.

    /// Record → "What is it?" Class → 60 min → BIO 101 → Start → recording.
    func testClip01RecordSheet() {
        let app = launch("ios-01-record-sheet")
        let record = app.buttons["record-button"].firstMatch
        XCTAssertTrue(record.waitForExistence(timeout: 10))
        hold(1.0)
        mark("start")
        hold(0.4)
        record.tap()
        XCTAssertTrue(app.staticTexts["What is it?"].firstMatch.exists, "Record sheet didn't open")
        expandRecordSheet(app)
        hold(0.2)
        app.buttons["Class"].firstMatch.tap()
        hold(0.2)
        app.buttons["record-length-60"].firstMatch.tap()
        hold(0.2)
        app.buttons["BIO 101"].firstMatch.tap()
        hold(0.9)
        still()
        app.buttons["record-plan-start"].firstMatch.tap()
        XCTAssertTrue(app.buttons["mark-button"].firstMatch.exists, "recording didn't start")
        hold(3.4)
        saveShot("ios-01b-record-start", envVar: "NF_FILM_DIR")   // still of the clip cut from "still" to "end"
        hold(0.6)
        mark("end")
    }

    /// A meeting being recorded: the timer runs, lines arrive one by one.
    func testClip02Live() {
        let app = launch("ios-02-live", ["-NFDemoLive", "-NFDemoLiveFeed"])
        XCTAssertTrue(app.staticTexts["Q4 roadmap review"].firstMatch.waitForExistence(timeout: 10))
        hold(0.6)
        mark("start")
        hold(7.6)
        still()
        hold(2.6)
        mark("end")
    }

    /// A class being recorded (60-minute limit, time left), lines arriving.
    func testClip02bLiveClass() {
        let app = launch("ios-02b-live-class", ["-NFDemoLive", "-NFDemoLiveClass"])
        XCTAssertTrue(app.buttons["mark-button"].firstMatch.waitForExistence(timeout: 10))
        hold(0.6)
        mark("start")
        hold(7.0)
        still()
        hold(2.6)
        mark("end")
    }

    /// While recording a class: Mark this moment (★ Important), then On the test, then Question.
    func testClip03Mark() {
        let app = launch("ios-03-mark", ["-NFDemoLive", "-NFDemoLiveClass"])
        let markButton = app.buttons["mark-button"].firstMatch
        XCTAssertTrue(markButton.waitForExistence(timeout: 10))
        hold(0.6)
        mark("start")
        hold(1.8)
        markButton.tap()
        let test = app.buttons["On the test"].firstMatch
        XCTAssertTrue(test.exists, "mark kinds didn't show")
        hold(1.4)
        test.tap()
        hold(0.8)
        still()
        hold(0.6)
        app.buttons["Question"].firstMatch.tap()
        hold(1.8)
        mark("end")
    }

    /// Recordings → the BIO 101 notebook chip → the notebook's lectures.
    func testClip04Library() {
        let app = launch("ios-04-library")
        openRecordings(app)
        hold(1.0)
        mark("start")
        hold(1.4)
        notebookChip(app, "BIO 101").tap()
        XCTAssertTrue(app.staticTexts["Lecture 5: Cell membranes"].firstMatch.exists)
        hold(1.4)
        still()
        hold(1.4)
        mark("end")
    }

    /// The lecture: notes, Review, Marked moments (★ / ? / On the test) on its
    /// first screen; a slow scroll down to the marks in the transcript and
    /// back to the top, ending on the clean first screen.
    func testClip05Lecture() {
        let app = launch("ios-05-lecture")
        openRecordings(app)
        notebookChip(app, "BIO 101").tap()
        let lecture = app.staticTexts["Lecture 7: Cellular respiration"].firstMatch
        XCTAssertTrue(lecture.waitForExistence(timeout: 5))
        hold(1.0)
        mark("start")
        hold(0.5)
        lecture.tap()
        XCTAssertTrue(app.buttons["study-open"].firstMatch.exists, "stored study guide missing")
        hold(1.2)
        still()
        hold(0.4)
        slowScroll(app, from: 0.72, fraction: 0.4, pointsPerSecond: 240)
        hold(0.8)
        // Back up past the top: the bounce settles exactly at the first screen
        slowScroll(app, from: 0.3, fraction: -0.5, pointsPerSecond: 320)
        hold(1.0)
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

    /// Personal: Health notebook → Physio check-in: summary, key points,
    /// to-dos; a slow scroll to Review and the Remember mark, and back up.
    func testClip07Personal() {
        let app = launch("ios-07-personal")
        openRecordings(app)
        hold(1.0)
        mark("start")
        hold(0.6)
        notebookChip(app, "Health").tap()
        let physio = app.staticTexts["Physio check-in"].firstMatch
        XCTAssertTrue(physio.exists)
        hold(0.4)
        physio.tap()
        XCTAssertTrue(element(app, labelContains: "Band exercises").exists)
        hold(0.8)
        still()
        slowScroll(app, from: 0.72, fraction: 0.32, pointsPerSecond: 300)
        hold(0.7)
        slowScroll(app, from: 0.3, fraction: -0.45, pointsPerSecond: 420)
        hold(0.8)
        mark("end")
    }

    /// A meeting with AI notes: summary, decisions, action items. No scroll:
    /// the attendees (with their profile-link buttons) sit just below.
    func testClip08Meeting() {
        let app = launch("ios-08-meeting")
        openRecordings(app)
        let meeting = app.staticTexts["Brightwater pilot kickoff"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        hold(1.0)
        mark("start")
        hold(0.6)
        meeting.tap()
        XCTAssertTrue(element(app, labelContains: "Pilot with five inspectors").exists)
        hold(1.4)
        still()
        hold(2.6)
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
        // Marked moments, nothing scrolled under the status bar
        openRecordings(app)
        app.staticTexts["Lecture 7: Cellular respiration"].firstMatch.tap()
        let moments = app.staticTexts["MARKED MOMENTS"].firstMatch
        XCTAssertTrue(app.buttons["study-open"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(moments.waitForExistence(timeout: 3) && moments.isHittable, "Marked moments not on the first screen")
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
