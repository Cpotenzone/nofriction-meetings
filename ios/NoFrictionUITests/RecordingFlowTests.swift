import XCTest

/// End-to-end: record a meeting from a test audio file, check the transcript
/// appears live and lands in Meetings. Screenshots go to NF_SCREENSHOT_DIR.
final class RecordingFlowTests: XCTestCase {
    private var shotDir: URL? {
        ProcessInfo.processInfo.environment["NF_SCREENSHOT_DIR"].map { URL(fileURLWithPath: $0) }
    }

    private func shot(_ name: String) {
        let data = XCUIScreen.main.screenshot().pngRepresentation
        if let dir = shotDir { try? data.write(to: dir.appending(path: "\(name).png")) }
        let attachment = XCTAttachment(data: data, uniformTypeIdentifier: "public.png")
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Tap "Allow" on system permission alerts as they appear (mic, speech,
    /// calendar arrive one after another), for up to `seconds`.
    private func allowSystemAlerts(for seconds: TimeInterval = 15) {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let deadline = Date().addingTimeInterval(seconds)
        var quietSince = Date()
        while Date() < deadline {
            let alert = springboard.alerts.firstMatch
            if alert.waitForExistence(timeout: 1) {
                for label in ["Allow", "Allow Full Access", "OK"] where alert.buttons[label].exists {
                    alert.buttons[label].tap()
                    quietSince = Date()
                    break
                }
            } else if Date().timeIntervalSince(quietSince) > 5 {
                return
            }
        }
    }

    func testRecordsAndTranscribesMeeting() throws {
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["NF_TEST_AUDIO"], "set NF_TEST_AUDIO")
        let app = XCUIApplication()
        app.launchEnvironment["NF_TEST_AUDIO"] = audio
        app.launchArguments = ["-NFAutoRecord"]
        app.launch()
        allowSystemAlerts()

        // Live text appears while "speaking"
        let live = app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] 'Kubernetes'")).firstMatch
        XCTAssertTrue(live.waitForExistence(timeout: 40), "transcript never mentioned Kubernetes")
        shot("1-recording")

        // Let the file finish, then stop
        sleep(8)
        shot("2-recording-late")
        app.buttons["Stop recording"].tap()
        sleep(2)

        app.buttons["Recordings"].tap()
        sleep(1)
        shot("3-meetings")
        let row = app.cells.firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        sleep(1)
        shot("4-detail")
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] 'Friday'")).firstMatch.exists,
                      "saved transcript is missing later sentences")
    }
}
