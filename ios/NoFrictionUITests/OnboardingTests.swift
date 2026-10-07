import XCTest

/// First-run welcome: every step, a permission asked in context, AI skip,
/// and reopening it from Settings.
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

        // 2. Recording consent: the same notice as before the first recording
        let consent = app.staticTexts["onboarding-consent-text"]
        XCTAssertTrue(consent.waitForExistence(timeout: 3))
        XCTAssertTrue(consent.label.hasPrefix("Recording laws differ"))
        saveShot("onboarding-2-consent")
        app.buttons["onboarding-consent-accept"].tap()

        // 3. Permissions: four rows, each with its reason; ask for one in context
        for kind in ["microphone", "speech", "calendar", "notifications"] {
            XCTAssertTrue(app.descendants(matching: .any)["permission-\(kind)"].firstMatch.waitForExistence(timeout: 3), "missing \(kind) row")
        }
        XCTAssertTrue(app.staticTexts["To record meetings, classes and everything else. Audio stays on this device."].exists)
        saveShot("onboarding-3-permissions")
        let allowNotifications = app.buttons["Allow Notifications"]
        if allowNotifications.exists {
            allowNotifications.tap()
            allowSystemAlert()
            XCTAssertTrue(app.images["Allowed"].waitForExistence(timeout: 5) || app.buttons["Notifications is off. Open Settings"].exists,
                          "notification row didn't update after the system prompt")
        }
        app.buttons["onboarding-continue"].tap()

        // 4. AI: explicit endpoint and model, optional key, or skip
        let key = app.secureTextFields["api-key-field"]
        XCTAssertTrue(key.waitForExistence(timeout: 3))
        XCTAssertTrue(app.textFields["ai-endpoint-url"].exists)
        XCTAssertTrue(app.textFields["ai-model-id"].exists)
        XCTAssertFalse(app.buttons["save-ai-endpoint"].isEnabled)
        saveShot("onboarding-4-ai")
        let skip = app.buttons["onboarding-ai-skip"]
        if skip.exists { skip.tap() } else { app.buttons["onboarding-continue"].tap() }

        // 5. Pro: explained, no hard paywall
        XCTAssertTrue(app.staticTexts["onboarding-pro"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["onboarding-see-plans"].exists)
        saveShot("onboarding-5-pro")
        app.buttons["onboarding-finish"].tap()

        // Lands on Record; the consent was recorded, so no notice sheet is pending
        XCTAssertTrue(app.buttons["record-button"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["onboarding-welcome"].exists)

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
        assertOnScreen(app.buttons["onboarding-consent-accept"], app, "I understand")
        saveShot("xxxl-onboarding-consent")
        app.buttons["onboarding-consent-accept"].tap()
        assertOnScreen(app.buttons["onboarding-continue"], app, "permissions Continue")
        saveShot("xxxl-onboarding-permissions")
        app.buttons["onboarding-continue"].tap()
        let skip = app.buttons["onboarding-ai-skip"]
        if skip.waitForExistence(timeout: 3) { assertOnScreen(skip, app, "Skip for now"); skip.tap() } else { app.buttons["onboarding-continue"].tap() }
        assertOnScreen(app.buttons["onboarding-finish"], app, "Get started")
        saveShot("xxxl-onboarding-pro")
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
