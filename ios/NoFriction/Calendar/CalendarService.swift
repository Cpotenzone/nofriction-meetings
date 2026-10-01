import EventKit
import Foundation

/// A calendar event reduced to what a meeting needs.
struct CalendarEventInfo: Sendable, Equatable {
    struct Participant: Sendable, Equatable {
        var email: String
        var name: String?
        var isOrganizer: Bool
        var isSelf: Bool
    }

    var id: String
    var title: String
    var start: Date
    var end: Date
    var isAllDay: Bool
    var location: String?
    var notes: String?
    var url: String?
    var participants: [Participant]

    /// Join link from the URL field, location or notes (Zoom/Meet/Teams/Webex).
    var meetingURL: String? {
        if let url, url.hasPrefix("http") { return url }
        for text in [location, notes].compactMap({ $0 }) {
            if let found = CalendarEventInfo.firstMeetingLink(in: text) { return found }
        }
        return nil
    }

    static func firstMeetingLink(in text: String) -> String? {
        let hosts = ["zoom.us", "meet.google.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com"]
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return nil }
        let range = NSRange(text.startIndex..., in: text)
        return detector.matches(in: text, range: range)
            .compactMap(\.url)
            .first { url in hosts.contains { url.host?.hasSuffix($0) == true } }?
            .absoluteString
    }
}

enum CalendarMatching {
    /// The timed event this recording belongs to: most overlap, and a
    /// meaningful one, so a recording isn't pinned to a meeting it merely
    /// brushed. Recordings often start a few minutes early. (Same rules as
    /// the Mac app's people.rs.)
    static func bestEvent(start: Date, end: Date, in events: [CalendarEventInfo]) -> CalendarEventInfo? {
        let recordingLength = max(end.timeIntervalSince(start), 60)
        return events
            .filter { !$0.isAllDay && $0.end > $0.start }
            .compactMap { event -> (TimeInterval, CalendarEventInfo)? in
                let eventStart = event.start.addingTimeInterval(-600)
                let overlap = min(end, event.end).timeIntervalSince(max(start, eventStart))
                let eventLength = max(event.end.timeIntervalSince(event.start), 60)
                let meaningful = overlap >= 300 || overlap >= 0.3 * recordingLength || overlap >= 0.5 * eventLength
                return overlap > 0 && meaningful ? (overlap, event) : nil
            }
            .max { $0.0 < $1.0 }?
            .1
    }
}

/// EventKit access. Reads only; never writes to the calendar.
@MainActor
final class CalendarService {
    static let shared = CalendarService()
    private let store = EKEventStore()

    var isAuthorized: Bool { EKEventStore.authorizationStatus(for: .event) == .fullAccess }
    var isDenied: Bool {
        let s = EKEventStore.authorizationStatus(for: .event)
        return s == .denied || s == .restricted
    }

    func requestAccess() async -> Bool {
        (try? await store.requestFullAccessToEvents()) ?? false
    }

    func events(from start: Date, to end: Date) -> [CalendarEventInfo] {
        guard isAuthorized else { return [] }
        let predicate = store.predicateForEvents(withStart: start, end: end, calendars: nil)
        return store.events(matching: predicate).map(Self.info)
    }

    /// The event happening now (or starting within 10 minutes).
    func currentEvent(at date: Date = .now) -> CalendarEventInfo? {
        let candidates = events(from: date.addingTimeInterval(-6 * 3600), to: date.addingTimeInterval(3600))
        return CalendarMatching.bestEvent(start: date, end: date.addingTimeInterval(20 * 60), in: candidates)
            ?? candidates.first { !$0.isAllDay && $0.start <= date.addingTimeInterval(600) && $0.end >= date }
    }

    private static func info(_ event: EKEvent) -> CalendarEventInfo {
        var participants: [CalendarEventInfo.Participant] = []
        func add(_ p: EKParticipant, organizer: Bool) {
            let raw = p.url.absoluteString
            guard raw.lowercased().hasPrefix("mailto:") else { return }
            let email = (String(raw.dropFirst(7)).removingPercentEncoding ?? String(raw.dropFirst(7))).lowercased()
            guard email.contains("@") else { return }
            let name = p.name.flatMap { $0.contains("@") || $0.isEmpty ? nil : $0 }
            if let i = participants.firstIndex(where: { $0.email == email }) {
                participants[i].name = participants[i].name ?? name
                participants[i].isSelf = participants[i].isSelf || p.isCurrentUser
                participants[i].isOrganizer = participants[i].isOrganizer || organizer
            } else {
                participants.append(.init(email: email, name: name, isOrganizer: organizer, isSelf: p.isCurrentUser))
            }
        }
        if let organizer = event.organizer { add(organizer, organizer: true) }
        for attendee in event.attendees ?? [] { add(attendee, organizer: false) }

        return CalendarEventInfo(
            id: event.eventIdentifier ?? UUID().uuidString,
            title: event.title ?? "Untitled",
            start: event.startDate,
            end: event.endDate,
            isAllDay: event.isAllDay,
            location: event.location,
            notes: event.notes,
            url: event.url?.absoluteString,
            participants: participants
        )
    }
}
