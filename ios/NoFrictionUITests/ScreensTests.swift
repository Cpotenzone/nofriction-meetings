import XCTest

/// Shared helpers: screenshots go to an env-var directory (if set) and are
/// attached to the test result.
extension XCTestCase {
    func saveShot(_ name: String, envVar: String = "NF_SCREENSHOT_DIR") {
        let data = XCUIScreen.main.screenshot().pngRepresentation
        if let dir = ProcessInfo.processInfo.environment[envVar] {
            let url = URL(fileURLWithPath: dir)
            try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
            try? data.write(to: url.appending(path: "\(name).png"))
        }
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.png")
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Scroll until `element` is on screen and hittable (or give up).
    func scrollTo(_ element: XCUIElement, in app: XCUIApplication, maxSwipes: Int = 8) {
        for _ in 0..<maxSwipes where !(element.exists && element.isHittable) { app.swipeUp() }
    }
}

/// Walks the main screens with sample data and saves screenshots
/// (NF_SCREENSHOT_DIR) — a layout check on iPhone and iPad.
final class ScreensTests: XCTestCase {
    func testMainScreens() {
        let device = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo"]
        app.launch()
        XCTAssertTrue(app.buttons["record-button"].firstMatch.waitForExistence(timeout: 5), "onboarding should be skipped with demo data")
        saveShot("\(device)-1-record")

        app.buttons["Meetings"].firstMatch.tap()
        sleep(1)
        saveShot("\(device)-2-meetings")
        let meeting = app.staticTexts["Brightwater pilot kickoff"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()
        sleep(1)
        saveShot("\(device)-3-detail")
        XCTAssertTrue(app.staticTexts["Dana Whitfield"].firstMatch.waitForExistence(timeout: 3))

        // AI is Pro: without a subscription, Summarize opens the paywall
        let summarize = app.buttons["ai-summarize"].firstMatch
        if summarize.waitForExistence(timeout: 3) {
            summarize.tap()
            XCTAssertTrue(app.buttons["Restore Purchases"].firstMatch.waitForExistence(timeout: 5), "paywall didn't open")
            saveShot("\(device)-3b-paywall")
            app.buttons["Close"].firstMatch.tap()
            sleep(1)
        }

        app.buttons["People"].firstMatch.tap()
        sleep(1)
        saveShot("\(device)-4-people")
        XCTAssertTrue(app.staticTexts["Marcus Lee"].firstMatch.waitForExistence(timeout: 3))

        app.buttons["Settings"].firstMatch.tap()
        sleep(1)
        saveShot("\(device)-5-settings")
        XCTAssertTrue(app.secureTextFields["api-key-field"].firstMatch.waitForExistence(timeout: 3))
    }
}

/// App Store screenshots, run by scripts/ios-screenshots.sh on each required
/// device size. Writes NN-name.png into NF_APPSTORE_DIR.
final class AppStoreScreenshots: XCTestCase {
    private func shot(_ name: String) { saveShot(name, envVar: "NF_APPSTORE_DIR") }

    func testAppStoreScreenshots() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-NFDemoLive"]
        app.launch()

        // 1. Record, mid-meeting, live transcript
        XCTAssertTrue(app.staticTexts["Q4 roadmap review"].firstMatch.waitForExistence(timeout: 5), "live demo meeting missing")
        XCTAssertTrue(app.buttons["Stop recording"].firstMatch.exists)
        sleep(2)
        shot("01-record")

        // 2. A meeting with AI notes
        app.buttons["Meetings"].firstMatch.tap()
        let meeting = app.staticTexts["Brightwater pilot kickoff"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Pilot with five inspectors")).firstMatch.waitForExistence(timeout: 5),
                      "AI notes missing")
        sleep(1)
        shot("02-meeting-notes")

        // 3. Stricken from the record
        let marker = app.descendants(matching: .any)["stricken-marker"].firstMatch
        scrollTo(marker, in: app)   // the transcript is lazy: the marker exists once scrolled near
        XCTAssertTrue(marker.waitForExistence(timeout: 5))
        // Drag the marker to just above the middle of the screen, so the
        // transcript fills the shot (not the photo grid under the nav bar)
        let target = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.42))
        marker.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: target, withVelocity: .slow, thenHoldForDuration: 0.6)
        XCTAssertTrue(marker.isHittable, "marker not on screen")
        sleep(1)
        shot("03-stricken")

        // 4. People
        app.buttons["People"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Priya Shah"].firstMatch.waitForExistence(timeout: 5))
        sleep(1)
        shot("04-people")

        // 5. Settings: paste-a-key AI setup
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.secureTextFields["api-key-field"].firstMatch.waitForExistence(timeout: 5))
        sleep(1)
        shot("05-settings-ai")

        // 6. Meetings list
        app.buttons["Meetings"].firstMatch.tap()
        if UIDevice.current.userInterfaceIdiom == .phone {
            // back to the list from the meeting pushed in step 2
            let back = app.navigationBars.buttons.element(boundBy: 0)
            if back.exists { back.tap() }
        }
        XCTAssertTrue(app.staticTexts["Design critique: onboarding"].firstMatch.waitForExistence(timeout: 5))
        sleep(1)
        shot("06-meetings")
    }
}

/// Strike from the record, end to end on a sample meeting (docs/REDACTION.md).
final class StrikeFlowTests: XCTestCase {
    func testStrikeAWordShowsTheMarker() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo"]
        app.launch()

        app.buttons["Meetings"].firstMatch.tap()
        let meeting = app.staticTexts["Weekly sync with Marcus"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()

        let line = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Quick one today")).firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 5))
        scrollTo(line, in: app, maxSwipes: 4)
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
        saveShot("strike-after-confirm")
        let marker = app.descendants(matching: .any)["stricken-marker"].firstMatch
        XCTAssertTrue(marker.waitForExistence(timeout: 10), "no marker after strike")
        XCTAssertTrue(marker.label.contains("privileged"))
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Quick one today")).firstMatch.exists)
        saveShot("strike-marker")
    }
}
