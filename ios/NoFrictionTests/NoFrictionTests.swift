import XCTest
@testable import noFriction

final class LinkedInTests: XCTestCase {
    func testNormalizesProfileLinks() throws {
        XCTAssertEqual(try LinkedIn.normalize("linkedin.com/in/jane-doe/"), "https://www.linkedin.com/in/jane-doe")
        XCTAssertEqual(try LinkedIn.normalize(" https://www.linkedin.com/in/jane-doe?utm_source=share "), "https://www.linkedin.com/in/jane-doe")
        XCTAssertEqual(try LinkedIn.normalize("http://m.linkedin.com/in/jd"), "https://www.linkedin.com/in/jd")
    }

    func testRejectsNonProfiles() {
        XCTAssertThrowsError(try LinkedIn.normalize("https://example.com/in/jane"))
        XCTAssertThrowsError(try LinkedIn.normalize("https://www.linkedin.com/company/acme"))
    }

    func testSearchURLUsesNameAndCompany() {
        let url = LinkedIn.searchURL(name: "Jane Doe", email: "jane@acme.com", company: "Acme")
        XCTAssertTrue(url.absoluteString.contains("keywords=Jane%20Doe%20Acme"))
    }
}

final class CalendarMatchingTests: XCTestCase {
    private let base = Date(timeIntervalSince1970: 1_790_000_000)

    private func event(_ id: String, _ startMin: Double, _ lenMin: Double, allDay: Bool = false) -> CalendarEventInfo {
        CalendarEventInfo(id: id, title: id,
                          start: base.addingTimeInterval(startMin * 60),
                          end: base.addingTimeInterval((startMin + lenMin) * 60),
                          isAllDay: allDay, location: nil, notes: nil, url: nil, participants: [])
    }

    func testPicksMostOverlap() {
        let events = [event("standup", 0, 15), event("sync", 15, 60)]
        let got = CalendarMatching.bestEvent(start: base.addingTimeInterval(12 * 60), end: base.addingTimeInterval(65 * 60), in: events)
        XCTAssertEqual(got?.id, "sync")
    }

    func testEarlyStartStillMatches() {
        let got = CalendarMatching.bestEvent(start: base.addingTimeInterval(-4 * 60), end: base.addingTimeInterval(25 * 60), in: [event("sync", 0, 30)])
        XCTAssertEqual(got?.id, "sync")
    }

    func testIgnoresTrivialOverlapAndAllDay() {
        let events = [event("earlier", -60, 61), event("offsite", -600, 1440, allDay: true)]
        XCTAssertNil(CalendarMatching.bestEvent(start: base, end: base.addingTimeInterval(3600), in: events))
    }

    func testFindsJoinLinkInNotes() {
        let link = CalendarEventInfo.firstMeetingLink(in: "Agenda below.\nJoin: https://acme.zoom.us/j/123456 thanks")
        XCTAssertEqual(link, "https://acme.zoom.us/j/123456")
    }
}

final class NamesTests: XCTestCase {
    func testGuessesNameFromEmail() {
        XCTAssertEqual(PersonNames.guess(fromEmail: "jane.doe@acme.com"), "Jane Doe")
    }

    func testCompanyFromEmail() {
        XCTAssertEqual(PersonNames.company(fromEmail: "jane@acme-corp.com"), "Acme Corp")
        XCTAssertNil(PersonNames.company(fromEmail: "jane@gmail.com"))
    }

    func testTimeFormatting() {
        XCTAssertEqual(TimeInterval(75).clock, "1:15")
        XCTAssertEqual(TimeInterval(3725).clock, "1:02:05")
    }
}
