import XCTest

/// Search on Recordings shows the lines that matched; tapping one opens the
/// recording scrolled to that line (sample data from -NFSeedDemo).
final class SearchJumpTests: XCTestCase {
    func testTappingAMatchingLineOpensTheRecordingAtIt() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo"]
        app.launch()

        app.buttons["Recordings"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Brightwater pilot kickoff"].firstMatch.waitForExistence(timeout: 5))
        let field = app.searchFields.firstMatch
        if !field.waitForExistence(timeout: 2) { app.swipeDown() }
        XCTAssertTrue(field.waitForExistence(timeout: 5), "no search field")
        field.tap()
        // Case-insensitive: the transcript says "Wednesday"
        field.typeText("wednesday")

        // Brightwater line 8 (index 7) is 7 × 75 s + 20 s = 9:05 into the recording
        let line = app.descendants(matching: .any).matching(identifier: "search-line").firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 5), "no matching line under the result")
        XCTAssertEqual(line.label, "At 9:05: Will do. I'll have them to you by Wednesday.")
        saveShot("search-lines")
        line.tap()

        // The detail opened scrolled to that line (the transcript is the last section, far below the top)
        let target = app.descendants(matching: .any)["transcript-search-line"].firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 5), "the line isn't in the transcript")
        let deadline = Date().addingTimeInterval(5)
        while !(target.isHittable) && Date() < deadline { usleep(200_000) }
        XCTAssertTrue(target.isHittable, "the transcript didn't scroll to the line")
        XCTAssertTrue(app.windows.firstMatch.frame.intersects(target.frame))
        let text = app.staticTexts["Will do. I'll have them to you by Wednesday."].firstMatch
        XCTAssertTrue(text.exists && text.isHittable, "the line's words aren't on screen")
        saveShot("search-jump-detail")
    }
}
