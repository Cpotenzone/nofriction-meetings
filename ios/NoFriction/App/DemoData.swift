#if DEBUG
import SwiftData
import UIKit

/// Launch with -NFSeedDemo to fill an empty store with sample meetings
/// (simulator screenshots / layout checks). Debug builds only.
enum DemoData {
    @MainActor
    static func seedIfRequested(_ context: ModelContext) {
        // -NFResetDemo: start from a clean store (UI tests that edit the sample meetings)
        if ProcessInfo.processInfo.arguments.contains("-NFResetDemo") {
            for m in (try? context.fetch(FetchDescriptor<Meeting>())) ?? [] { context.delete(m) }
            for p in (try? context.fetch(FetchDescriptor<Person>())) ?? [] { context.delete(p) }
            try? context.save()
        }
        guard ProcessInfo.processInfo.arguments.contains("-NFSeedDemo"),
              ((try? context.fetchCount(FetchDescriptor<Meeting>())) ?? 0) == 0 else { return }

        let now = Date()
        let people: [(String, String?, String?)] = [
            ("priya.shah@contoso.com", "Priya Shah", "https://www.linkedin.com/in/priyashah"),
            ("marcus.lee@contoso.com", "Marcus Lee", nil),
            ("dana@northwind.io", "Dana Whitfield", nil),
            ("casey@nofriction.io", "Casey", nil),
        ]

        func meeting(_ title: String, hoursAgo: Double, minutes: Double, lines: [String], attendees: [Int], organizer: Int) {
            let start = now.addingTimeInterval(-hoursAgo * 3600)
            let m = Meeting(title: title, startedAt: start)
            m.endedAt = start.addingTimeInterval(minutes * 60)
            m.scheduledStart = start
            m.scheduledEnd = m.endedAt
            m.meetingURL = "https://contoso.zoom.us/j/81234567"
            m.calendarEventID = UUID().uuidString
            context.insert(m)
            for (i, line) in lines.enumerated() {
                let s = Segment(text: line, start: start.addingTimeInterval(Double(i) * 14), duration: 12)
                context.insert(s)
                s.meeting = m
            }
            for i in attendees {
                let (email, name, li) = people[i]
                let p = MeetingLinker.findOrCreatePerson(email: email, in: context)
                p.name = name
                p.company = PersonNames.company(fromEmail: email)
                p.linkedinURL = p.linkedinURL ?? li
                p.isSelf = email.hasPrefix("casey@")
                let a = Attendance(role: i == organizer ? "organizer" : "attendee")
                context.insert(a)
                a.meeting = m
                a.person = p
            }
        }

        meeting("Kubernetes migration sync", hoursAgo: 2, minutes: 42, lines: [
            "Thanks everyone for joining. Priya, can you walk us through the Kubernetes migration timeline?",
            "Sure. We're moving the ingest services first, then the API tier in two waves.",
            "We need a decision on the Azure credits by Friday so we can size the new cluster.",
            "Marcus will own the follow-up with the security team on the network policies.",
        ], attendees: [0, 1, 3], organizer: 0)
        meeting("Northwind partnership intro", hoursAgo: 26, minutes: 30, lines: [
            "Dana, great to finally meet. Tell us a bit about how Northwind handles field inspections today.",
            "Mostly paper checklists that get typed up at the end of the week.",
        ], attendees: [2, 3], organizer: 3)
        try? context.save()
    }
}
#endif
