import XCTest

/// First-run welcome: two steps (welcome, permissions), a permission asked
/// in context, the recording notice on the first Record, and reopening the
/// welcome from Settings.
final class OnboardingTests: XCTestCase {
    private func allowSystemAlert(timeout: TimeInterval = 5) {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let alert = springboard.alerts.firstMatch
        guard alert.waitForExistence(timeout: timeout) else { return }
        for label in ["Allow", "Allow Full Access", "OK"] where alert.buttons[label].exists {
            alert.buttons[label].tap()
            return
        }
    }

    func testFirstRunFlow() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetOnboarding", "-NFResetDemo"]
        app.launch()

        // 1. Welcome
        XCTAssertTrue(app.staticTexts["onboarding-welcome"].waitForExistence(timeout: 5), "welcome didn't show on first launch")
        saveShot("onboarding-1-welcome")
        app.buttons["onboarding-continue"].tap()

        // 2. Permissions: three rows, each with its reason; ask for one in context.
        // No consent page (the notice is the sheet on the first Record), no
        // Notifications row (asked when a timer is about to end), no AI or Pro page.
        for kind in ["microphone", "speech", "calendar"] {
            XCTAssertTrue(app.descendants(matching: .any)["permission-\(kind)"].firstMatch.waitForExistence(timeout: 3), "missing \(kind) row")
        }
        XCTAssertFalse(app.descendants(matching: .any)["permission-notifications"].exists, "Notifications is not asked in onboarding")
        XCTAssertTrue(app.staticTexts["To record. Audio stays on this device."].exists)
        saveShot("onboarding-2-permissions")
        let allowCalendar = app.buttons["Allow Calendar"]
        if allowCalendar.exists {
            allowCalendar.tap()
            allowSystemAlert()
            XCTAssertTrue(app.images["Allowed"].waitForExistence(timeout: 5) || app.buttons["Calendar is off. Open Settings"].exists,
                          "calendar row didn't update after the system prompt")
        }
        XCTAssertFalse(app.secureTextFields["api-key-field"].exists, "AI setup is not part of onboarding")
        app.buttons["onboarding-finish"].tap()

        // Lands on Record. The recording notice comes once, on the first tap of Record.
        XCTAssertTrue(app.buttons["record-button"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["onboarding-welcome"].exists)
        app.buttons["record-button"].tap()
        let notice = app.buttons["recording-notice-continue"]
        XCTAssertTrue(notice.waitForExistence(timeout: 3), "recording notice didn't show on the first Record")
        saveShot("first-record-notice")
        app.buttons["Cancel"].firstMatch.tap()

        // Settings → Show welcome again → Skip closes it
        app.buttons["Settings"].firstMatch.tap()
        let again = app.buttons["show-welcome"]
        scrollTo(again, in: app, maxSwipes: 12)
        again.tap()
        XCTAssertTrue(app.staticTexts["onboarding-welcome"].waitForExistence(timeout: 5), "welcome didn't reopen from Settings")
        app.buttons["onboarding-skip"].tap()
        XCTAssertTrue(app.staticTexts["onboarding-welcome"].waitForNonExistence(timeout: 5))

        // Not shown again on the next launch
        app.terminate()
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.buttons["record-button"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["onboarding-welcome"].exists)
    }
}

/// Dynamic Type at XXXL: critical controls stay on screen and tappable.
/// Screenshots are attached to the result for a visual check.
final class DynamicTypeTests: XCTestCase {
    private let xxxl = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL"]

    private func assertOnScreen(_ e: XCUIElement, _ app: XCUIApplication, _ what: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(e.waitForExistence(timeout: 5), "\(what) missing", file: file, line: line)
        XCTAssertTrue(e.isHittable, "\(what) not hittable", file: file, line: line)
        let screen = app.windows.firstMatch.frame
        XCTAssertTrue(screen.contains(e.frame), "\(what) clipped: \(e.frame) outside \(screen)", file: file, line: line)
    }

    func testOnboardingAtXXXL() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetOnboarding"] + xxxl
        app.launch()
        assertOnScreen(app.buttons["onboarding-continue"], app, "welcome Continue")
        assertOnScreen(app.buttons["onboarding-skip"], app, "Skip")
        saveShot("xxxl-onboarding-welcome")
        app.buttons["onboarding-continue"].tap()
        assertOnScreen(app.buttons["onboarding-finish"], app, "Get started")
        saveShot("xxxl-onboarding-permissions")
        app.buttons["onboarding-finish"].tap()
    }

    func testMainControlsAtXXXL() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-NFDemoLive"] + xxxl
        app.launch()

        // Record controls
        assertOnScreen(app.buttons["Stop recording"], app, "record button")
        assertOnScreen(app.buttons["Snap"], app, "Snap")
        saveShot("xxxl-record")

        // Line editor actions (Delete / Strike) and the strike confirmation
        app.buttons["Recordings"].firstMatch.tap()
        let meeting = app.staticTexts["Weekly sync with Marcus"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 5))
        meeting.tap()
        let line = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Quick one today")).firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 5))
        scrollTo(line, in: app)
        line.tap()
        let word = app.descendants(matching: .any)["word-0"].firstMatch
        XCTAssertTrue(word.waitForExistence(timeout: 5))
        word.tap()
        assertOnScreen(app.buttons["edit-strike"], app, "Strike from the record")
        assertOnScreen(app.buttons["edit-delete"], app, "Delete")
        saveShot("xxxl-word-editor")
        app.buttons["edit-strike"].tap()
        XCTAssertTrue(app.navigationBars["Strike from the record"].waitForExistence(timeout: 5), "strike confirmation didn't open")
        saveShot("xxxl-strike-confirm-top")
        let confirm = app.buttons["strike-confirm"]
        scrollTo(confirm, in: app, maxSwipes: 12)   // the Form is lazy: it exists once scrolled near
        assertOnScreen(confirm, app, "strike confirm")
        saveShot("xxxl-strike-confirm")
    }
}
