import XCTest

/// Walks the main screens with sample data and saves screenshots
/// (NF_SCREENSHOT_DIR) — a layout check on iPhone and iPad.
final class ScreensTests: XCTestCase {
    func testMainScreens() {
        let dir = ProcessInfo.processInfo.environment["NF_SCREENSHOT_DIR"].map { URL(fileURLWithPath: $0) }
        let device = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        func shot(_ name: String) {
            let data = XCUIScreen.main.screenshot().pngRepresentation
            if let dir { try? data.write(to: dir.appending(path: "\(device)-\(name).png")) }
        }

        let app = XCUIApplication()
        app.launchArguments = ["-NFSeedDemo"]
        app.launch()
        sleep(1)
        shot("1-record")

        app.buttons["Meetings"].firstMatch.tap()
        sleep(1)
        shot("2-meetings")
        let meeting = app.staticTexts["Kubernetes migration sync"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()
        sleep(1)
        shot("3-detail")
        XCTAssertTrue(app.staticTexts["Priya Shah"].firstMatch.exists)

        // AI is Pro: without a subscription, Summarize opens the paywall
        let summarize = app.buttons["ai-summarize"].firstMatch
        if summarize.waitForExistence(timeout: 3) {
            summarize.tap()
            XCTAssertTrue(app.buttons["Restore Purchases"].firstMatch.waitForExistence(timeout: 5), "paywall didn't open")
            shot("3b-paywall")
            app.buttons["Close"].firstMatch.tap()
            sleep(1)
        }

        app.buttons["People"].firstMatch.tap()
        sleep(1)
        shot("4-people")
        XCTAssertTrue(app.staticTexts["Marcus Lee"].firstMatch.waitForExistence(timeout: 3))

        app.buttons["Settings"].firstMatch.tap()
        sleep(1)
        shot("5-settings")
        XCTAssertTrue(app.secureTextFields["api-key-field"].firstMatch.waitForExistence(timeout: 3))
    }
}

/// Strike from the record, end to end on a sample meeting (docs/REDACTION.md).
final class StrikeFlowTests: XCTestCase {
    func testStrikeAWordShowsTheMarker() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo"]
        app.launch()

        app.buttons["Meetings"].firstMatch.tap()
        let meeting = app.staticTexts["Kubernetes migration sync"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()

        let line = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Thanks everyone")).firstMatch
        for _ in 0..<4 where !line.isHittable { app.swipeUp() }
        XCTAssertTrue(line.waitForExistence(timeout: 5))
        line.tap()

        let word = app.descendants(matching: .any)["word-0"].firstMatch
        XCTAssertTrue(word.waitForExistence(timeout: 5), "word editor didn't open")
        word.tap()
        app.buttons["edit-strike"].firstMatch.tap()

        let reason = app.textFields["strike-reason"].firstMatch
        XCTAssertTrue(reason.waitForExistence(timeout: 5), "strike confirmation didn't open")
        reason.tap()
        reason.typeText("privileged\n")
        let confirm = app.buttons["strike-confirm"].firstMatch
        for _ in 0..<4 where !(confirm.exists && confirm.isHittable) { app.swipeUp() }
        confirm.tap()

        sleep(2)
        let shots = ProcessInfo.processInfo.environment["NF_SCREENSHOT_DIR"]
        if let shots { try? XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: shots).appending(path: "strike-after-confirm.png")) }
        let marker = app.descendants(matching: .any)["stricken-marker"].firstMatch
        XCTAssertTrue(marker.waitForExistence(timeout: 10), "no marker after strike")
        XCTAssertTrue(marker.label.contains("privileged"))
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Thanks everyone")).firstMatch.exists)
        if let dir = ProcessInfo.processInfo.environment["NF_SCREENSHOT_DIR"] {
            try? XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: dir).appending(path: "strike-marker.png"))
        }
    }
}
