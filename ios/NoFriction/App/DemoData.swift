#if DEBUG
import SwiftData
import UIKit

/// Launch with -NFSeedDemo to fill an empty store with sample meetings
/// (App Store screenshots / layout checks); add -NFDemoLive to show a
/// meeting being recorded. Debug builds only.
///
/// Everything here is invented: people, companies (on the reserved
/// `.example` domain), links and conversations. No real personal data.
enum DemoData {
    static let liveTitle = "Q4 roadmap review"

    /// Today at 9:41, matching the status bar in App Store screenshots.
    static var anchor: Date {
        Calendar.current.date(bySettingHour: 9, minute: 41, second: 0, of: .now) ?? .now
    }

    private struct DemoPerson {
        let email: String
        let name: String
        let linkedIn: String?
    }

    private static let people: [DemoPerson] = [
        DemoPerson(email: "priya.shah@lumen-labs.example", name: "Priya Shah", linkedIn: "https://www.linkedin.com/in/nofriction-demo-priya"),
        DemoPerson(email: "marcus.lee@lumen-labs.example", name: "Marcus Lee", linkedIn: nil),
        DemoPerson(email: "dana.whitfield@brightwater.example", name: "Dana Whitfield", linkedIn: "https://www.linkedin.com/in/nofriction-demo-dana"),
        DemoPerson(email: "jonah.kim@brightwater.example", name: "Jonah Kim", linkedIn: nil),
        DemoPerson(email: "sofia.moreno@alder-studio.example", name: "Sofia Moreno", linkedIn: "https://www.linkedin.com/in/nofriction-demo-sofia"),
        DemoPerson(email: "alex.rivera@lumen-labs.example", name: "Alex Rivera", linkedIn: nil),   // "you"
    ]
    private static let selfIndex = 5

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

        let day: TimeInterval = 86_400
        let a = anchor

        // Yesterday: a customer call with AI notes, a photo and a stricken line
        let brightwater = meeting(
            "Brightwater pilot kickoff", start: a.addingTimeInterval(-day + 4 * 3600 + 19 * 60), minutes: 34,
            lines: [
                "Thanks for making the time, Dana. Can you walk us through how inspections work today?",
                "Sure. Our inspectors fill in paper checklists on site, and someone retypes them at the end of the week.",
                "So a report can be a week old before a client sees it.",
                "Exactly. And when a client asks what was said on site, we're digging through notebooks.",
                "STRIKE",
                "Okay. For the pilot, let's start with five inspectors and see how the summaries land with clients.",
                "Jonah, could you send over the three report templates you use most?",
                "Will do. I'll have them to you by Wednesday.",
                "Great. We'll set up a shared folder for feedback and check in two weeks after launch.",
                "One more thing: can inspectors add photos of the site to the summary?",
                "Yes. Snap a photo during the visit and it lands in the timeline next to what was said.",
                "Perfect. That covers most of what our clients ask for.",
                "Then let's lock October 14 for the start. I'll send the agreement by Friday.",
                "Sounds good. Thanks, everyone.",
            ],
            spacing: 75, attendees: [2, 3, selfIndex], organizer: selfIndex, in: context)
        brightwater.aiNotes = """
        **Summary**
        Brightwater's inspectors use paper checklists that are retyped weekly, so client reports lag by up to a week. They want searchable site notes and a one-page summary per visit.

        **Decisions**
        • Pilot with five inspectors, starting October 14
        • Share visit summaries as PDF

        **Action items**
        • Alex: send the pilot agreement by Friday
        • Jonah: share the three most-used report templates by Wednesday
        • Dana: choose the five pilot inspectors
        """
        brightwater.aiNotesAt = brightwater.endedAt
        if let line = brightwater.orderedSegments.first(where: { $0.text == "STRIKE" }) {
            let r = Redaction(kind: .line, action: .strike, mediaStart: 56, mediaEnd: 68,
                              coveredFrom: line.start, coveredTo: line.start.addingTimeInterval(12),
                              reason: "pricing under NDA", createdAt: a.addingTimeInterval(-day + 6 * 3600))
            context.insert(r)
            r.meeting = brightwater
            line.text = RedactionText.markerToken(r.id)
        }
        addSlide(to: brightwater, at: brightwater.startedAt.addingTimeInterval(9 * 60), in: context)

        // Earlier this week
        meeting("Design critique: onboarding", start: a.addingTimeInterval(-2 * day + 5 * 3600 + 19 * 60), minutes: 45,
                lines: [
                    "Let's start with the permission screens. Sofia, what did people say in testing?",
                    "They liked seeing why each permission is needed, right next to the button.",
                    "Can we cut the welcome down to one screen?",
                    "I'd keep the consent step. It's the one place we explain recording laws.",
                ],
                attendees: [4, 1, selfIndex], organizer: 4, in: context)
        meeting("Weekly sync with Marcus", start: a.addingTimeInterval(-3 * day + 49 * 60), minutes: 25,
                lines: [
                    "Quick one today. The Android build is green again.",
                    "Nice. Anything blocking the beta?",
                    "Just the store listing. Screenshots and the privacy answers.",
                ],
                attendees: [1, selfIndex], organizer: selfIndex, in: context)
        try? context.save()
    }

    /// -NFDemoLive: a meeting in progress with a live transcript.
    @MainActor
    static func showLiveMeeting(session: RecordingSession, context: ModelContext) {
        let title = liveTitle
        let existing = try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.title == title && $0.endedAt == nil })).first
        let start = anchor.addingTimeInterval(-25 * 60)   // 9:16
        let m = existing ?? meeting(
            liveTitle, start: start, minutes: nil,
            lines: [
                "Okay, let's get started. The goal today is to lock the Q4 roadmap and agree on owners.",
                "Priya, do you want to start with where the beta landed?",
                "Sure. Retention is up eleven percent since offline mode shipped, and support tickets are down by a third.",
                "That's great. What's behind the drop in tickets?",
                "Mostly the new onboarding. People find calendar sync on their own now.",
                "Marcus, where are we on the Android timeline?",
                "Design is done for the recording screen. The open question is whether calendar matching ships in the first beta.",
                "I'd ship it. It's the thing people mention first in interviews.",
                "Agreed. Sofia, can the onboarding cover the calendar permission too?",
                "Yes, it's already in the new flow. One screen, with the reason right next to the button.",
            ],
            spacing: 2 * 60,
            attendees: [0, 1, 4, selfIndex], organizer: 0, in: context)
        m.scheduledStart = anchor.addingTimeInterval(-26 * 60)  // 9:15
        m.scheduledEnd = anchor.addingTimeInterval(19 * 60)     // 10:00
        try? context.save()
        session.showDemo(meeting: m, startedAt: Date.now.addingTimeInterval(-25 * 60 - 12),
                         partial: "We can have a beta in six weeks if we keep the scope to recording and")
    }

    @MainActor @discardableResult
    private static func meeting(_ title: String, start: Date, minutes: Double?, lines: [String], spacing: TimeInterval = 14,
                                attendees: [Int], organizer: Int, in context: ModelContext) -> Meeting {
        let m = Meeting(title: title, startedAt: start)
        m.endedAt = minutes.map { start.addingTimeInterval($0 * 60) }
        m.scheduledStart = start
        m.scheduledEnd = m.endedAt
        m.meetingURL = "https://meet.example.com/q4-review"
        m.calendarEventID = UUID().uuidString
        context.insert(m)
        for (i, line) in lines.enumerated() {
            let s = Segment(text: line, start: start.addingTimeInterval(Double(i) * spacing + 20), duration: 12)
            context.insert(s)
            s.meeting = m
        }
        for i in attendees {
            let d = people[i]
            let p = MeetingLinker.findOrCreatePerson(email: d.email, in: context)
            p.name = d.name
            p.company = PersonNames.company(fromEmail: d.email)
            p.linkedinURL = p.linkedinURL ?? d.linkedIn
            p.isSelf = i == selfIndex
            let a = Attendance(role: i == organizer ? "organizer" : "attendee")
            context.insert(a)
            a.meeting = m
            a.person = p
        }
        return m
    }

    /// A generated "slide" photo, so the Photos section isn't empty.
    @MainActor
    private static func addSlide(to m: Meeting, at time: Date, in context: ModelContext) {
        let size = CGSize(width: 1200, height: 900)
        let image = UIGraphicsImageRenderer(size: size).image { ctx in
            UIColor(red: 0.96, green: 0.95, blue: 0.92, alpha: 1).setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
            UIColor(red: 0.98, green: 0.80, blue: 0.08, alpha: 1).setFill()
            ctx.fill(CGRect(x: 90, y: 120, width: 120, height: 14))
            let title: [NSAttributedString.Key: Any] = [.font: UIFont.systemFont(ofSize: 76, weight: .bold), .foregroundColor: UIColor(white: 0.1, alpha: 1)]
            ("Pilot plan" as NSString).draw(at: CGPoint(x: 90, y: 170), withAttributes: title)
            let body: [NSAttributedString.Key: Any] = [.font: UIFont.systemFont(ofSize: 44, weight: .regular), .foregroundColor: UIColor(white: 0.25, alpha: 1)]
            for (i, line) in ["•  5 inspectors, 6 weeks", "•  Summary per site visit", "•  Check-in after 2 weeks"].enumerated() {
                (line as NSString).draw(at: CGPoint(x: 96, y: 330 + CGFloat(i) * 92), withAttributes: body)
            }
        }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        let name = "demo-slide-\(UUID().uuidString).jpg"
        guard (try? data.write(to: Storage.snapshots.appending(path: name), options: .atomic)) != nil else { return }
        let s = Snapshot(fileName: name, takenAt: time)
        context.insert(s)
        s.meeting = m
    }
}
#endif
