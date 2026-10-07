#if DEBUG
import Foundation
import SwiftData

/// Launch film and App Store app-preview footage (marketing/film/capture).
/// Debug builds only; driven by NoFrictionUITests/FilmFootageTests.
///
///     -NFFilm            with -NFSeedDemo: two earlier BIO 101 lectures and a
///                        stored study guide for "Lecture 7: Cellular
///                        respiration" (no AI call). The Record sheet's Start
///                        shows a demo recording (no microphone, speech engine
///                        or notification prompt) whose transcript arrives line
///                        by line. Times on screen line up with the 9:41 status bar.
///     -NFDemoLiveFeed    with -NFDemoLive: the live meeting's transcript arrives
///                        line by line (the Simulator has no speech recognition)
///     -NFDemoLiveClass   with -NFDemoLive: a Class in BIO 101 being recorded
///                        (60-minute limit, 47 minutes left) instead of the meeting
///
/// Everything here is invented, like DemoData: no real people, companies or brands.
enum FilmDemo {
    private static var args: [String] { ProcessInfo.processInfo.arguments }
    static var isOn: Bool { args.contains("-NFFilm") }
    static var liveFeed: Bool { args.contains("-NFDemoLiveFeed") }
    static var liveClass: Bool { args.contains("-NFDemoLiveClass") }

    /// Film runs: added to real times shown on screen, so a recording started
    /// now reads as 9:41 like the status bar. Zero otherwise.
    static let displayShift: TimeInterval = isOn ? DemoData.anchor.timeIntervalSinceNow : 0

    /// Now, as the film shows it: within the 9:41 minute, like the frozen
    /// status bar, however long a capture runs.
    static var now: Date {
        let shifted = Date.now.addingTimeInterval(displayShift)
        return isOn ? min(shifted, DemoData.anchor.addingTimeInterval(59)) : shifted
    }

    static let notebook = "BIO 101"

    // MARK: Scripts (invented)

    /// A lecture, for a live Class recording.
    static let lectureScript = [
        "Last time we followed glucose all the way to ATP. Today we run the process in reverse.",
        "Photosynthesis takes light, water and carbon dioxide, and builds sugar.",
        "It happens in the chloroplast, in two stages.",
        "The light reactions happen in the thylakoid membranes.",
        "They split water, release oxygen, and make ATP and NADPH.",
        "The Calvin cycle runs in the stroma and uses that energy to fix carbon dioxide.",
        "Write this down: the oxygen we breathe comes from water, not from carbon dioxide.",
        "That one shows up on the exam every year.",
        "The key enzyme is rubisco, probably the most abundant protein on Earth.",
        "Questions so far? Good. Let's look at the light reactions step by step.",
        "Light hits photosystem two first, even though it was discovered second.",
        "The energy moves an electron down a chain, a lot like the one in the mitochondria.",
    ]

    /// The rest of the live "Q4 roadmap review" meeting (DemoData.liveTitle).
    static let meetingOpening = [
        "Okay, let's get started. The goal today is to lock the Q4 roadmap and agree on owners.",
        "Priya, do you want to start with where the beta landed?",
        "Sure. Retention is up eleven percent since offline mode shipped, and support tickets are down by a third.",
        "That's great. What's behind the drop in tickets?",
    ]
    static let meetingScript = [
        "Mostly the new onboarding. People find calendar sync on their own now.",
        "Marcus, where are we on the tablet layout?",
        "Design is done for the recording screen. The open question is whether calendar matching ships in the first beta.",
        "I'd ship it. It's the thing people mention first in interviews.",
        "Agreed. Sofia, can the onboarding cover the calendar permission too?",
        "Yes, it's already in the new flow. One screen, with the reason right next to the button.",
        "We can have a beta in six weeks if we keep the scope to recording and transcripts.",
        "Then that's the plan: a beta in six weeks, and Marcus owns the checklist.",
        "I'll send the notes and the owners around after this.",
    ]

    // MARK: Seed

    /// With -NFSeedDemo -NFFilm, after the regular sample data.
    @MainActor
    static func seed(_ context: ModelContext) {
        guard isOn else { return }
        _ = displayShift   // fixed at launch
        let day: TimeInterval = 86_400
        let a = DemoData.anchor
        for (title, daysAgo, lines) in [
            ("Lecture 6: Enzymes", 5.0, [
                "Enzymes speed up reactions without being used up.",
                "Each one fits its substrate at the active site.",
                "Heat and pH change the shape, and the enzyme stops working.",
            ]),
            ("Lecture 5: Cell membranes", 7.0, [
                "The membrane is a double layer of phospholipids.",
                "Small, uncharged molecules slip through on their own.",
                "Everything else needs a channel or a pump.",
            ]),
        ] {
            let m = DemoData.meeting(title, start: a.addingTimeInterval(-daysAgo * day - 100 * 60), minutes: 50, lines: lines,
                                     spacing: 8 * 60, attendees: [], organizer: -1, calendar: false, in: context)
            m.kind = .class
            m.courseName = notebook
            m.plannedMinutes = 60
        }
        let all = (try? context.fetch(FetchDescriptor<Meeting>())) ?? []
        // Notes as the detail view renders them best (inline Markdown: bold headings, • bullets),
        // short enough that the lecture's top screen also shows Review and Marked moments
        if let lecture = all.first(where: { $0.title == "Lecture 7: Cellular respiration" }) {
            lecture.aiNotes = """
            **Summary**
            How cells turn glucose into ATP, in three stages:
            • Glycolysis: glucose into two pyruvate, no oxygen
            • Krebs cycle: in the mitochondrial matrix
            • Electron transport chain: most of the ATP
            """
            seedStudyGuide(lecture, in: context)
        }
        if let physio = all.first(where: { $0.title == "Physio check-in" }) {
            physio.aiNotes = """
            **Summary**
            Shoulder check-up: good progress since the last visit.

            **Key points**
            • Range of motion is much better than two weeks ago
            • Ice after long runs, not before

            **To-dos and reminders**
            • Band exercises: three sets of ten, morning and evening
            • Book the next visit for the end of the month
            """
        }
        try? context.save()
    }

    /// A stored study guide, as `StudyStore.save` would keep it.
    @MainActor
    private static func seedStudyGuide(_ lecture: Meeting, in context: ModelContext) {
        // Times of the lecture's lines (DemoData: one every 6 minutes, 20 s in)
        let ms = { (line: Int) in (line * 6 * 60 + 20) * 1000 }
        let summary = StudySummary(title: "Cellular respiration", sections: [
            .init(heading: "Glycolysis", bullets: [
                "Happens in the cytoplasm and needs no oxygen",
                "Splits one glucose into two pyruvate",
                "Nets two ATP",
            ]),
            .init(heading: "Krebs cycle", bullets: [
                "Runs in the mitochondrial matrix",
                "Loads electron carriers for the next stage",
            ]),
            .init(heading: "Electron transport chain (on the test)", bullets: [
                "Makes most of the cell's ATP",
                "Oxygen is the final electron acceptor; without it the chain stops",
            ]),
            .init(heading: "Deadlines", bullets: ["Problem set four is due next Thursday"]),
        ])
        let terms = StudyTerms(terms: [
            .init(term: "Glycolysis", definition: "The first stage: glucose is split into two pyruvate in the cytoplasm."),
            .init(term: "Pyruvate", definition: "The three-carbon molecule glycolysis makes from glucose."),
            .init(term: "Krebs cycle", definition: "Reactions in the mitochondrial matrix that release carbon dioxide and load electron carriers."),
            .init(term: "Electron transport chain", definition: "Proteins in the inner mitochondrial membrane that use those electrons to make most of the ATP."),
            .init(term: "Final electron acceptor", definition: "Oxygen, which takes the electrons at the end of the chain."),
            .init(term: "ATP", definition: "The molecule cells use to carry energy."),
        ])
        let cards = StudyCards(cards: [
            .init(front: "Which stage makes most of the ATP?", back: "The electron transport chain."),
            .init(front: "Where does glycolysis happen?", back: "In the cytoplasm. It doesn't need oxygen."),
            .init(front: "What does glycolysis make from one glucose?", back: "Two pyruvate, and a net gain of two ATP."),
            .init(front: "Where does the Krebs cycle run?", back: "In the mitochondrial matrix."),
            .init(front: "What is the final electron acceptor?", back: "Oxygen. Without it, the chain stops."),
        ])
        let quiz = StudyQuiz(questions: [
            .init(question: "Which stage of cellular respiration makes most of the ATP?",
                  choices: ["Glycolysis", "The Krebs cycle", "The electron transport chain", "Fermentation"], answer: 2,
                  explanation: "The electron transport chain in the inner mitochondrial membrane makes most of the ATP. Glycolysis nets only two.",
                  atMs: ms(4)),
            .init(question: "What happens to the electron transport chain without oxygen?",
                  choices: ["It speeds up", "It stops", "It makes more pyruvate", "Nothing changes"], answer: 1,
                  explanation: "Oxygen is the final electron acceptor. With nowhere for the electrons to go, the chain stops.",
                  atMs: ms(5)),
            .init(question: "Where does glycolysis happen?",
                  choices: ["The mitochondrial matrix", "The cytoplasm", "The nucleus", "The inner membrane"], answer: 1,
                  explanation: "Glycolysis runs in the cytoplasm and doesn't need oxygen.",
                  atMs: ms(1)),
        ])
        let asks = StudyAsks(questions: [
            .init(question: "Why is oxygen the final electron acceptor, and not another molecule?", atMs: ms(5)),
            .init(question: "Does fermentation make any ATP after glycolysis?", atMs: nil),
        ])
        let encoder = JSONEncoder()
        func json<T: Encodable>(_ value: T) -> String { (try? encoder.encode(value)).map { String(decoding: $0, as: UTF8.self) } ?? "{}" }
        let fingerprint = StudyInput(meeting: lecture).fingerprint
        for (kind, body) in [(StudyKind.summary, json(summary)), (.terms, json(terms)), (.flashcards, json(cards)),
                             (.quiz, json(quiz)), (.questions, json(asks))] {
            let material = StudyMaterial(kind: kind, json: body, transcriptFingerprint: fingerprint,
                                         createdAt: (lecture.endedAt ?? lecture.startedAt).addingTimeInterval(120))
            context.insert(material)
            material.meeting = lecture
        }
    }

    // MARK: Live

    /// -NFDemoLive -NFDemoLiveFeed: the meeting opens with its first lines, the rest arrive one by one.
    @MainActor
    static func showLiveMeetingFeed(session: RecordingSession, context: ModelContext) {
        let a = DemoData.anchor
        let start = a.addingTimeInterval(-25 * 60)   // 9:16
        let m = DemoData.meeting(DemoData.liveTitle, start: start, minutes: nil, lines: meetingOpening,
                                 spacing: 6 * 60, attendees: [0, 1, 4, DemoData.selfIndex], organizer: 0, in: context)
        m.scheduledStart = a.addingTimeInterval(-26 * 60)   // 9:15
        m.scheduledEnd = a.addingTimeInterval(19 * 60)      // 10:00
        try? context.save()
        session.showDemo(meeting: m, startedAt: Date.now.addingTimeInterval(-25 * 60 - 12), partial: "")
        startFeed(meetingScript, session: session, context: context)
    }

    /// -NFDemoLive -NFDemoLiveClass: a lecture 12½ minutes into a 60-minute limit.
    @MainActor
    static func showLiveClass(session: RecordingSession, context: ModelContext) {
        let a = DemoData.anchor
        let elapsed: TimeInterval = 12 * 60 + 30
        let start = a.addingTimeInterval(-elapsed)
        let m = DemoData.meeting(RecordingSession.notebookTitle(notebook, at: a), start: start, minutes: nil,
                                 lines: Array(lectureScript.prefix(4)), spacing: 3 * 60, attendees: [], organizer: -1,
                                 calendar: false, in: context)
        m.kind = .class
        m.courseName = notebook
        m.plannedMinutes = 60
        try? context.save()
        session.showDemo(meeting: m, startedAt: Date.now.addingTimeInterval(-elapsed), partial: "", limit: .minutes(60))
        startFeed(Array(lectureScript.dropFirst(4)), session: session, context: context)
    }

    /// The film's Start on the Record sheet: a new recording, named and timed
    /// like a real one (RecordingSession.start), shown without a microphone.
    @MainActor
    static func startRecording(session: RecordingSession, limit: RecordingLimit, kind: RecordingKind, notebook: String?, context: ModelContext) {
        let shown = now
        let title = notebook.map { RecordingSession.notebookTitle($0, at: shown) } ?? RecordingSession.defaultTitle(for: shown, kind: kind)
        let m = Meeting(title: title, startedAt: shown)
        m.courseName = notebook
        m.plannedMinutes = limit.minutes
        m.kind = kind
        context.insert(m)
        try? context.save()
        session.showDemo(meeting: m, startedAt: .now, partial: "", level: 0.2, limit: limit)
        startFeed(kind == .meeting ? meetingScript : lectureScript, session: session, context: context)
    }

    private static var feedTask: Task<Void, Never>?

    /// Speech, simulated: each line grows word by word as the live (volatile)
    /// text, then settles into a transcript line, about every 2 seconds.
    @MainActor
    static func startFeed(_ lines: [String], session: RecordingSession, context: ModelContext, delay: Duration = .seconds(2)) {
        guard let meeting = session.meeting else { return }
        let id = meeting.id
        feedTask?.cancel()
        feedTask = Task { @MainActor in
            @MainActor func live() -> Bool { !Task.isCancelled && session.isActive && session.meeting?.id == id }
            @MainActor func wait(_ d: Duration) async -> Bool {
                try? await Task.sleep(for: d)
                while live() && session.phase == .paused { try? await Task.sleep(for: .milliseconds(200)) }
                return live()
            }
            guard await wait(delay) else { return }
            for line in lines {
                let words = line.split(separator: " ")
                let began = now
                for n in 1...words.count {
                    session.setDemoFeed(partial: words.prefix(n).joined(separator: " "), level: Float.random(in: 0.3...0.8))
                    guard await wait(.milliseconds(115)) else { return }
                }
                let segment = Segment(text: line, start: began, duration: max(1, now.timeIntervalSince(began)))
                context.insert(segment)
                segment.meeting = meeting
                try? context.save()
                session.setDemoFeed(partial: "", level: 0.25)
                guard await wait(.milliseconds(550)) else { return }
            }
        }
    }
}
#endif
