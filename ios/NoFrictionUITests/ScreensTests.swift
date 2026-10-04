import XCTest
import StoreKitTest

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

        // 5. Settings: explicit endpoint and model setup
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

/// Captures the real paywall with local StoreKit products, never a purchased entitlement.
/// Apple reference: https://developer.apple.com/documentation/storekittest/sktestsession/init(contentsof:)
final class PaywallReviewScreenshot: XCTestCase {
    func testPaywallShowsApprovedPlans() throws {
        let configuration = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "NoFriction", withExtension: "storekit"))
        let session = try SKTestSession(contentsOf: configuration)
        session.resetToDefaultState()
        session.clearTransactions()
        session.locale = Locale(identifier: "en_US")
        session.storefront = "USA"
        defer { session.clearTransactions() }

        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["Settings"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["Settings"].firstMatch.tap()
        let upgrade = app.buttons["Upgrade to Pro"].firstMatch
        scrollTo(upgrade, in: app)
        XCTAssertTrue(upgrade.waitForExistence(timeout: 5))
        upgrade.tap()
        let monthly = app.buttons["plan-com.nofriction.meetings.pro.monthly"].firstMatch
        let yearly = app.buttons["plan-com.nofriction.meetings.pro.yearly"].firstMatch
        XCTAssertTrue(monthly.waitForExistence(timeout: 15))
        XCTAssertTrue(yearly.exists)
        XCTAssertTrue(monthly.label.contains("$0.99"), monthly.label)
        XCTAssertTrue(yearly.label.contains("$5.99"), yearly.label)
        let trial = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Free for 1 week")).firstMatch
        XCTAssertTrue(trial.waitForExistence(timeout: 5), "The StoreKit fixture must offer the approved one-week trial")
        saveShot("01-paywall", envVar: "NF_APPSTORE_DIR")
        let privacy = app.descendants(matching: .any)["Privacy Policy"].firstMatch
        scrollTo(privacy, in: app, maxSwipes: 4)
        XCTAssertTrue(privacy.exists)
        saveShot("02-paywall-terms", envVar: "NF_APPSTORE_DIR")
    }
}

/// Refreshes only the changed AI settings image, preserving the other store screenshots.
final class AIEndpointSetupScreenshots: XCTestCase {
    func testExplicitEndpointSettings() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        XCTAssertTrue(app.buttons["Settings"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["Settings"].firstMatch.tap()
        let endpoint = app.textFields["ai-endpoint-url"].firstMatch
        let model = app.textFields["ai-model-id"].firstMatch
        XCTAssertTrue(endpoint.waitForExistence(timeout: 5))
        XCTAssertTrue(model.exists)
        XCTAssertTrue(app.secureTextFields["api-key-field"].firstMatch.exists)
        XCTAssertFalse(app.buttons["save-ai-endpoint"].isEnabled)
        XCTAssertFalse(app.staticTexts["Google Gemini"].exists)
        XCTAssertFalse(app.buttons["connect-key"].exists)
        saveShot("05-settings-ai", envVar: "NF_APPSTORE_DIR")
    }
}

/// Local StoreKit integration only. Purchases and restores are initiated by the
/// shipping UI/Store object; SKTestSession controls Apple's simulated ledger.
/// Uses the existing fictional demo store; never records or contacts real accounts.
final class StoreKitLifecycleTests: XCTestCase {
    private let monthly = "com.nofriction.meetings.pro.monthly"
    private let yearly = "com.nofriction.meetings.pro.yearly"

    private func makeSession() throws -> SKTestSession {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "NoFriction", withExtension: "storekit"))
        let session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.clearTransactions()
        session.disableDialogs = true
        session.locale = Locale(identifier: "en_US")
        session.storefront = "USA"
        session.timeRate = .realTime
        return session
    }

    private func launchSettings() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo", "-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()
        let tab = app.buttons["Settings"].firstMatch
        XCTAssertTrue(tab.waitForExistence(timeout: 10))
        tab.tap()
        scrollTo(app.buttons["Restore Purchases"].firstMatch, in: app)
        return app
    }

    private func assertPro(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(app.buttons["Manage Subscription"].firstMatch.waitForExistence(timeout: 15),
                      "The app's Store did not grant Pro", file: file, line: line)
        XCTAssertFalse(app.buttons["Upgrade to Pro"].firstMatch.exists, file: file, line: line)
    }

    private func assertFree(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(app.buttons["Upgrade to Pro"].firstMatch.waitForExistence(timeout: 15),
                      "The app's Store did not remove Pro", file: file, line: line)
        XCTAssertFalse(app.buttons["Manage Subscription"].firstMatch.exists, file: file, line: line)
    }

    private func openPlan(_ product: String, app: XCUIApplication) -> XCUIElement {
        let upgrade = app.buttons["Upgrade to Pro"].firstMatch
        scrollTo(upgrade, in: app)
        XCTAssertTrue(upgrade.waitForExistence(timeout: 10))
        upgrade.tap()
        let plan = app.buttons["plan-\(product)"].firstMatch
        XCTAssertTrue(plan.waitForExistence(timeout: 15))
        return plan
    }

    private func lifecycle(_ product: String) throws {
        let session = try makeSession()
        var app = launchSettings()
        defer { app.terminate(); session.clearTransactions(); session.resetToDefaultState() }
        assertFree(app)
        openPlan(product, app: app).tap() // calls real PaywallView.buy -> Store.purchase
        assertPro(app)
        XCTAssertTrue(session.allTransactions().contains { $0.productIdentifier == product && $0.state == .purchased })

        // A new app process must recover verified currentEntitlements, without
        // relying on the in-memory flag set by purchase completion.
        app.terminate()
        app = launchSettings()
        assertPro(app)
        let restore = app.buttons["Restore Purchases"].firstMatch
        scrollTo(restore, in: app)
        restore.tap() // calls real Store.restore -> AppStore.sync -> refresh
        XCTAssertTrue(app.staticTexts["Pro restored."].firstMatch.waitForExistence(timeout: 15))
        assertPro(app)

        let oldIDs = Set(session.allTransactions().map(\.identifier))
        try session.forceRenewalOfSubscription(productIdentifier: product)
        let renewed = try XCTUnwrap(session.allTransactions().filter {
            $0.productIdentifier == product && !oldIDs.contains($0.identifier)
        }.max(by: { $0.identifier < $1.identifier }))
        assertPro(app)

        // Cancellation stops future renewal; paid/trial access stays active
        // through the current period. No account subscription is touched.
        try session.disableAutoRenewForTransaction(identifier: renewed.identifier)
        let cancelled = try XCTUnwrap(session.allTransactions().first { $0.identifier == renewed.identifier })
        XCTAssertFalse(cancelled.autoRenewingEnabled)
        assertPro(app)

        // StoreKit emits the expired transaction; the running app listener
        // must refresh its verified entitlement and remove Pro.
        try session.expireSubscription(productIdentifier: product)
        assertFree(app)
        restore.tap()
        XCTAssertTrue(app.staticTexts["No active subscription found."].firstMatch.waitForExistence(timeout: 15))
        assertFree(app)
        saveShot("local-storekit-\(product.hasSuffix("monthly") ? "monthly" : "yearly")-expired")
    }

    func testMonthlyPurchaseRestoreRenewCancelExpire() throws { try lifecycle(monthly) }
    func testYearlyPurchaseRestoreRenewCancelExpire() throws { try lifecycle(yearly) }

    @MainActor
    func testCancelledPurchaseNeverGrantsPro() async throws {
        let session = try makeSession()
        let app = launchSettings()
        defer { app.terminate(); session.clearTransactions(); session.resetToDefaultState() }
        try await session.setSimulatedError(.generic(.userCancelled), forAPI: .purchase)
        let plan = openPlan(monthly, app: app)
        plan.tap()
        // The request must finish (button re-enabled). StoreKit retains the
        // failed attempt in its ledger, but it must grant no successful transaction.
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: plan)
        await fulfillment(of: [ready], timeout: 15)
        let attempts = session.allTransactions()
        XCTAssertTrue(attempts.contains { $0.productIdentifier == monthly && $0.state == .failed })
        XCTAssertFalse(attempts.contains { $0.state == .purchased || $0.state == .restored })
        app.buttons["Close"].firstMatch.tap()
        assertFree(app)
        let restore = app.buttons["Restore Purchases"].firstMatch
        scrollTo(restore, in: app)
        restore.tap()
        XCTAssertTrue(app.staticTexts["No active subscription found."].firstMatch.waitForExistence(timeout: 15))
        assertFree(app)
    }
}
