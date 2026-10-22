import XCTest

/// Export… (docs/EXPORT.md): the share menu opens the Export sheet, the
/// demo recording's slide is picked, and Share… makes a package and opens
/// the share sheet. No AI is called (the AI tools need a provider).
final class ExportFlowTests: XCTestCase {
    func testExportSheetMakesAPackage() {
        let app = XCUIApplication()
        app.launchArguments = ["-NFResetDemo", "-NFSeedDemo"]
        app.launch()

        app.buttons["Recordings"].firstMatch.tap()
        let meeting = app.staticTexts["Brightwater pilot kickoff"].firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 10), "demo recording missing")
        meeting.tap()

        let menu = app.buttons["share-menu"].firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 5), "share menu missing")
        menu.tap()
        XCTAssertTrue(app.buttons["Share as Text"].waitForExistence(timeout: 5), "Share as Text missing")
        app.buttons["Export…"].tap()

        XCTAssertTrue(app.navigationBars["Export"].waitForExistence(timeout: 5), "Export sheet didn't open")
        XCTAssertTrue(app.staticTexts["What goes in"].exists || app.staticTexts["WHAT GOES IN"].exists)
        saveShot("export-1-sheet")

        // The slide is picked by default and gets a caption field
        let slide = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Photo at")).firstMatch
        scrollTo(slide, in: app)
        XCTAssertTrue(slide.waitForExistence(timeout: 5), "the demo slide isn't in the picker")
        XCTAssertTrue(slide.isSelected, "photos are picked by default")
        let caption = app.textFields["Caption (optional)"].firstMatch
        scrollTo(caption, in: app)
        XCTAssertTrue(caption.waitForExistence(timeout: 5))
        caption.tap()
        caption.typeText("The pilot plan")
        saveShot("export-2-pictures")

        app.buttons["export-share"].tap()
        // The system share sheet (its contents vary by iOS version)
        let shared = app.otherElements["ActivityListView"].waitForExistence(timeout: 10)
            || app.navigationBars["UIActivityContentView"].waitForExistence(timeout: 2)
            || app.buttons["Copy"].waitForExistence(timeout: 2)
            || app.cells["Save to Files"].waitForExistence(timeout: 2)
        XCTAssertTrue(shared, "share sheet didn't open")
        saveShot("export-3-share")
    }
}
