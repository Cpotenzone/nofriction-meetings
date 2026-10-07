import SwiftData
import XCTest
@testable import noFriction

// Timed recording and classes (docs/TIMED_RECORDING_AND_CLASSES.md)

final class RecordingLimitTests: XCTestCase {
    func testChoicesAndStorage() {
        XCTAssertEqual(RecordingLimit.choices, [.minutes(15), .minutes(30), .minutes(60), .minutes(90), .noLimit])
        XCTAssertEqual(RecordingLimit(storageValue: "60"), .minutes(60))
        XCTAssertEqual(RecordingLimit(storageValue: " NONE "), .noLimit)
        XCTAssertNil(RecordingLimit(storageValue: "0"))
        XCTAssertNil(RecordingLimit(storageValue: "721"))
        XCTAssertNil(RecordingLimit(storageValue: "soon"))
        XCTAssertEqual(RecordingLimit.minutes(90).storageValue, "90")
        XCTAssertEqual(RecordingLimit.noLimit.storageValue, "none")
        XCTAssertEqual(RecordingLimit.noLimit.shortLabel, "∞")
        XCTAssertEqual(RecordingLimit.minutes(15).label, "15 min")
    }

    func testRememberedChoiceDefaultsToNoLimit() throws {
        let suite = "nf-limit-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        XCTAssertEqual(RecordingLimitStore.remembered(defaults), .noLimit, "never chosen: nothing is cut short")
        RecordingLimitStore.remember(.minutes(30), defaults)
        XCTAssertEqual(RecordingLimitStore.remembered(defaults), .minutes(30))
        RecordingLimitStore.remember(.minutes(45), defaults)
        XCTAssertEqual(RecordingLimitStore.remembered(defaults), .minutes(30), "only the five choices are remembered")
        RecordingLimitStore.remember(.noLimit, defaults)
        XCTAssertEqual(RecordingLimitStore.remembered(defaults), .noLimit)
        defaults.set("garbage", forKey: RecordingLimitStore.key)
        XCTAssertEqual(RecordingLimitStore.remembered(defaults), .noLimit)
    }
}

final class TimeLimitPlanTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)
    private func at(_ s: TimeInterval) -> Date { t0.addingTimeInterval(s) }

    func testWarnsOnceThenStopsAtTheDeadline() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(60))
        XCTAssertEqual(p.plannedMinutes, 60)
        XCTAssertEqual(p.tick(now: at(54 * 60)), .none)
        XCTAssertEqual(p.tick(now: at(55 * 60)), .warn(secondsLeft: 300), "5 minutes before the end")
        XCTAssertEqual(p.tick(now: at(55 * 60 + 1)), .none, "warned once")
        XCTAssertEqual(p.tick(now: at(60 * 60 - 1)), .none)
        XCTAssertEqual(p.tick(now: at(60 * 60)), .stop)
        XCTAssertEqual(p.tick(now: at(60 * 60 + 1)), .none, "stop is returned once")
    }

    func testFifteenMinutePlansWarnTwoMinutesAhead() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(15))
        XCTAssertEqual(p.tick(now: at(12 * 60 + 59)), .none)
        XCTAssertEqual(p.tick(now: at(13 * 60)), .warn(secondsLeft: 120))
        XCTAssertEqual(TimeLimitPlan.warnLead(plannedMinutes: 15), 120)
        XCTAssertEqual(TimeLimitPlan.warnLead(plannedMinutes: 30), 300)
    }

    func testPausingDoesNotMoveTheDeadline() {
        // Wall-clock: the plan only knows the start and the deadline
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(30))
        XCTAssertEqual(p.deadline, at(30 * 60))
        XCTAssertEqual(p.tick(now: at(30 * 60)), .stop)
    }

    func testNoLimitNeverStops() {
        var p = TimeLimitPlan(startedAt: t0, limit: .noLimit)
        XCTAssertNil(p.plannedMinutes)
        XCTAssertNil(p.remaining(at: at(10)))
        XCTAssertEqual(p.tick(now: at(10 * 3600)), .none)
        XCTAssertFalse(p.extend(now: at(10)), "nothing to extend")
    }

    func testExtendMovesTheDeadlineAndRearmsTheWarning() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(30))
        XCTAssertEqual(p.tick(now: at(25 * 60)), .warn(secondsLeft: 300))
        XCTAssertTrue(p.extend(now: at(26 * 60)))
        XCTAssertEqual(p.plannedMinutes, 45)
        XCTAssertEqual(p.remaining(at: at(26 * 60)), 19 * 60)
        XCTAssertFalse(p.warned)
        XCTAssertEqual(p.tick(now: at(30 * 60)), .none, "the old deadline no longer stops it")
        XCTAssertEqual(p.tick(now: at(40 * 60)), .warn(secondsLeft: 300))
        XCTAssertEqual(p.tick(now: at(45 * 60)), .stop)
    }

    func testExtendAfterTheDeadlineCountsFromNow() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(15))
        XCTAssertEqual(p.tick(now: at(15 * 60)), .stop)
        XCTAssertTrue(p.extend(now: at(15 * 60 + 3)))
        XCTAssertEqual(p.remaining(at: at(15 * 60 + 3)), 15 * 60)
        XCTAssertFalse(p.stopping)
        XCTAssertEqual(p.tick(now: at(16 * 60)), .none)
    }

    func testExtendIsCappedAtTwelveHours() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(RecordingLimit.maxMinutes - 5))
        XCTAssertTrue(p.extend(now: at(1)))
        XCTAssertEqual(p.plannedMinutes, RecordingLimit.maxMinutes)
        XCTAssertFalse(p.extend(now: at(2)))
    }

    func testRemoveLimitCancelsTheStop() {
        var p = TimeLimitPlan(startedAt: t0, limit: .minutes(15))
        XCTAssertEqual(p.tick(now: at(14 * 60)), .warn(secondsLeft: 60))
        p.removeLimit()
        XCTAssertNil(p.deadline)
        XCTAssertNil(p.warnAt)
        XCTAssertEqual(p.tick(now: at(3600)), .none)
    }

    func testTimerNeverActsOnAnotherMeeting() {
        let a = UUID(), b = UUID()
        var slot = TimeLimitSlot()
        slot.arm(meetingID: a, plan: TimeLimitPlan(startedAt: t0, limit: .minutes(15)))
        // A is stopped by hand; B starts with its own plan
        slot.clear()
        XCTAssertEqual(slot.tick(meetingID: a, now: at(15 * 60)), .none, "A's timer has nothing to stop")
        slot.arm(meetingID: b, plan: TimeLimitPlan(startedAt: at(60), limit: .minutes(60)))
        XCTAssertEqual(slot.tick(meetingID: a, now: at(15 * 60)), .none, "A's late tick can't stop B")
        XCTAssertEqual(slot.tick(meetingID: a, now: at(5 * 3600)), .none)
        XCTAssertFalse(slot.extend(meetingID: a, now: at(100)))
        XCTAssertFalse(slot.removeLimit(meetingID: a))
        XCTAssertEqual(slot.plan?.deadline, at(61 * 60), "B untouched")
        XCTAssertEqual(slot.tick(meetingID: b, now: at(61 * 60)), .stop)
    }

    func testSlotExtendAndRemoveForTheArmedMeeting() {
        let a = UUID()
        var slot = TimeLimitSlot()
        slot.arm(meetingID: a, plan: TimeLimitPlan(startedAt: t0, limit: .minutes(30)))
        XCTAssertTrue(slot.extend(meetingID: a, now: at(60)))
        XCTAssertEqual(slot.plan?.plannedMinutes, 45)
        XCTAssertTrue(slot.removeLimit(meetingID: a))
        XCTAssertNil(slot.plan?.deadline)
        XCTAssertEqual(slot.tick(meetingID: a, now: at(3 * 3600)), .none)
    }

    func testNotificationTitle() {
        XCTAssertEqual(TimeLimitNotifier.title(secondsLeft: 300), "5 minutes left in this recording")
        XCTAssertEqual(TimeLimitNotifier.title(secondsLeft: 120), "2 minutes left in this recording")
        XCTAssertEqual(TimeLimitNotifier.title(secondsLeft: 40), "1 minute left in this recording")
    }
}

final class NotebookTests: XCTestCase {
    func testNormalize() {
        XCTAssertEqual(Notebook.normalize("  BIO 101 —  Cell\n Biology "), "BIO 101 — Cell Biology")
        XCTAssertNil(Notebook.normalize("  \n "))
        XCTAssertNil(Notebook.normalize(nil))
        XCTAssertEqual(Notebook.normalize("CHEM\u{7}201"), "CHEM201")
        XCTAssertEqual(Notebook.normalize(String(repeating: "x", count: 200))?.count, 80)
        XCTAssertEqual(Notebook.normalize(String(repeating: "x", count: 79) + " y"), String(repeating: "x", count: 79))
    }

    func testCanonicalJoinsAnExistingNotebookIgnoringCase() {
        let existing = ["BIO 101 — Cell Biology", "HIST 200"]
        XCTAssertEqual(Notebook.canonical("bio 101 — cell biology", existing: existing), "BIO 101 — Cell Biology")
        XCTAssertEqual(Notebook.canonical("MATH 3", existing: existing), "MATH 3")
        XCTAssertNil(Notebook.canonical("  ", existing: existing))
    }

    func testRecentIsNewestFirstAndOnePerName() {
        let d = { (s: TimeInterval) in Date(timeIntervalSince1970: 1_790_000_000 + s) }
        let recents = Notebook.recent([
            ("BIO 101", d(10)), (nil, d(50)), ("hist 200", d(40)), ("bio 101", d(30)), ("  ", d(60)), ("HIST 200", d(5)),
        ])
        XCTAssertEqual(recents, ["hist 200", "bio 101"])
        XCTAssertEqual(Notebook.recent([("A", d(1)), ("B", d(2)), ("C", d(3))], limit: 2), ["C", "B"])
    }

    func testSuggestionsAndFilter() {
        let recents = ["CHEM 110", "BIO 101", "Biochem 300", "HIST 200"]
        XCTAssertEqual(Notebook.suggestions("", recents: recents, max: 2), ["CHEM 110", "BIO 101"])
        XCTAssertEqual(Notebook.suggestions("bio", recents: recents), ["BIO 101", "Biochem 300"])
        XCTAssertEqual(Notebook.suggestions("chem", recents: recents), ["CHEM 110", "Biochem 300"])
        XCTAssertTrue(Notebook.matches("BIO 101", filter: nil))
        XCTAssertTrue(Notebook.matches("bio 101", filter: "BIO 101"))
        XCTAssertFalse(Notebook.matches(nil, filter: "BIO 101"))
        XCTAssertFalse(Notebook.matches("HIST 200", filter: "BIO 101"))
    }

    /// The type picks the notes style, never whether a notebook is set.
    func testNotesPromptFollowsTheType() {
        XCTAssertEqual(MeetingAI.notesSystem(for: .meeting), MeetingAI.notesSystem)
        let lecture = MeetingAI.notesSystem(for: .class)
        XCTAssertEqual(lecture, MeetingAI.lectureNotesSystem)
        XCTAssertTrue(lecture.contains("lecture notes"))
        XCTAssertTrue(lecture.contains("## Key concepts"))
        XCTAssertTrue(lecture.contains("## Announcements and deadlines"))
        XCTAssertTrue(lecture.contains("Never write action items for attendees"))
        XCTAssertFalse(lecture.contains("## Action items"))
        let personal = MeetingAI.notesSystem(for: .personal)
        XCTAssertTrue(personal.contains("## Summary"))
        XCTAssertTrue(personal.contains("## Key points"))
        XCTAssertTrue(personal.contains("## To-dos and reminders"))
        XCTAssertFalse(personal.contains("## Action items"))
        XCTAssertFalse(personal.contains("## Decisions"))
        XCTAssertFalse(personal.lowercased().contains("lecture"))
    }

    @MainActor
    func testVocabulary() {
        XCTAssertEqual(RecordingKind.allCases.map(\.label), ["Meeting", "Class", "Personal"])
        XCTAssertEqual(RecordingKind.allCases.map(\.rawValue), ["meeting", "class", "personal"])
        XCTAssertEqual(RecordingKind.allCases.map(\.notebookPlaceholder), ["e.g. Acme project", "e.g. BIO 101", "e.g. Health"])
        XCTAssertEqual(RecordingKind.pickerTitle, "What is it?")
        XCTAssertEqual(RecordingKind.help, "Personal covers everything else: conversations, appointments, talks, ideas.")
        XCTAssertEqual(RecordingKind(stored: nil), .meeting)
        XCTAssertEqual(RecordingKind(stored: "lecture"), .meeting, "unknown values read as a meeting")
        XCTAssertEqual(RecordingKind(stored: "class"), .class)
        XCTAssertEqual(Notebook.label, "Notebook")
        XCTAssertEqual(Notebook.filterTitle, "Notebooks")
        XCTAssertTrue(RecordingSession.isDefaultTitle(RecordingSession.defaultTitle(for: .now, kind: .personal)))
        XCTAssertTrue(RecordingSession.defaultTitle(for: .now, kind: .class).hasPrefix("Class · "))
        XCTAssertFalse(RecordingSession.isDefaultTitle(RecordingSession.notebookTitle("BIO 101", at: .now)))
    }

    func testRememberedKindDefaultsToMeeting() throws {
        let suite = "nf-kind-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        XCTAssertEqual(RecordingKindStore.remembered(defaults), .meeting)
        RecordingKindStore.remember(.personal, defaults)
        XCTAssertEqual(RecordingKindStore.remembered(defaults), .personal)
        defaults.set("garbage", forKey: RecordingKindStore.key)
        XCTAssertEqual(RecordingKindStore.remembered(defaults), .meeting)
    }
}

@MainActor
final class ClassStoreTests: XCTestCase {
    func testNotebookAndTypeAreSavedShownToAIAndDeletedWithTheRecording() throws {
        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        let context = container.mainContext
        let lecture = Meeting(title: "Lecture 3")
        lecture.courseName = "BIO 101"
        lecture.plannedMinutes = 90
        lecture.kind = .class
        let standup = Meeting(title: "Standup")
        context.insert(lecture)
        context.insert(standup)
        try context.save()

        XCTAssertEqual(lecture.recordingKind, "class")
        XCTAssertEqual(standup.kind, .meeting, "no type: a meeting")
        XCTAssertTrue(MeetingAI.context(lecture).hasPrefix("Class: Lecture 3\n"))
        XCTAssertTrue(MeetingAI.context(lecture).contains("Notebook: BIO 101"))
        XCTAssertTrue(MeetingAI.context(standup).hasPrefix("Meeting: Standup\n"))
        XCTAssertFalse(MeetingAI.context(standup).contains("Notebook:"))
        XCTAssertTrue(MeetingExport.markdown(lecture).contains("Notebook: BIO 101"))
        XCTAssertTrue(MeetingExport.markdown(lecture).contains(" · Class\n"))

        let all = try context.fetch(FetchDescriptor<Meeting>())
        XCTAssertEqual(Notebook.recent(all.map { ($0.courseName, $0.startedAt) }), ["BIO 101"])

        // Delete Meeting removes the class name with the row
        context.delete(lecture)
        try context.save()
        let left = try context.fetch(FetchDescriptor<Meeting>())
        XCTAssertEqual(left.map(\.title), ["Standup"])
        XCTAssertTrue(Notebook.recent(left.map { ($0.courseName, $0.startedAt) }).isEmpty)
        XCTAssertTrue(try context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.courseName != nil })).isEmpty)
    }

    /// A store written by the build before classes (no className /
    /// plannedMinutes) opens with the current model and keeps its meetings.
    func testStoreFromBeforeClassesMigrates() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "nf-migrate-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appending(path: "old.store")
        let started = Date(timeIntervalSince1970: 1_790_000_000)

        var oldID = UUID()
        try autoreleasepool {
            let old = try ModelContainer(for: Schema(PreClassesSchema.models), configurations: ModelConfiguration(url: url))
            let ctx = ModelContext(old)
            let m = PreClassesSchema.Meeting(title: "Recorded last week", startedAt: started)
            m.aiNotes = "## Summary\nOld notes"
            ctx.insert(m)
            let s = PreClassesSchema.Segment(text: "The cell membrane is selectively permeable.", start: started, duration: 3)
            ctx.insert(s)
            s.meeting = m
            try ctx.save()
            oldID = m.id
        }

        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: url))
        let context = ModelContext(container)
        let meetings = try context.fetch(FetchDescriptor<Meeting>())
        XCTAssertEqual(meetings.count, 1)
        let m = try XCTUnwrap(meetings.first)
        XCTAssertEqual(m.id, oldID)
        XCTAssertEqual(m.title, "Recorded last week")
        XCTAssertEqual(m.aiNotes, "## Summary\nOld notes")
        XCTAssertEqual(m.segments.map(\.text), ["The cell membrane is selectively permeable."])
        XCTAssertNil(m.courseName)
        XCTAssertNil(m.plannedMinutes)

        // The new fields work on the migrated store
        m.courseName = "BIO 101"
        m.plannedMinutes = 60
        try context.save()
        let again = try ModelContext(container).fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.courseName != nil }))
        XCTAssertEqual(again.map(\.courseName), ["BIO 101"])
        XCTAssertEqual(again.first?.plannedMinutes, 60)
    }

    /// A store from before recording types (notebooks were classes then)
    /// opens with the current model; the one-time backfill makes the
    /// recordings with a notebook classes and leaves the rest meetings.
    func testStoreFromBeforeTypesMigratesAndBackfillsOnce() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "nf-migrate-kind-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appending(path: "old.store")
        let started = Date(timeIntervalSince1970: 1_790_000_000)
        let suite = "nf-kind-backfill-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        try autoreleasepool {
            let old = try ModelContainer(for: Schema(PreKindSchema.models), configurations: ModelConfiguration(url: url))
            let ctx = ModelContext(old)
            let lecture = PreKindSchema.Meeting(title: "Lecture 3", startedAt: started)
            lecture.courseName = "BIO 101"
            lecture.plannedMinutes = 90
            let sync = PreKindSchema.Meeting(title: "Weekly sync", startedAt: started.addingTimeInterval(3600))
            ctx.insert(lecture)
            ctx.insert(sync)
            let mark = PreKindSchema.MomentMarker(at: started.addingTimeInterval(30), kind: "test")
            ctx.insert(mark)
            mark.meeting = lecture
            try ctx.save()
        }

        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: url))
        let context = container.mainContext
        let all = try context.fetch(FetchDescriptor<Meeting>(sortBy: [SortDescriptor(\.startedAt)]))
        XCTAssertEqual(all.map(\.title), ["Lecture 3", "Weekly sync"])
        XCTAssertTrue(all.allSatisfy { $0.recordingKind == nil }, "the new attribute starts empty")
        XCTAssertEqual(all[0].courseName, "BIO 101")
        XCTAssertEqual(all[0].plannedMinutes, 90)
        XCTAssertEqual(all[0].markers.map(\.kind), ["test"])

        XCTAssertEqual(RecordingKindBackfill.run(context, defaults: defaults), 1)
        XCTAssertEqual(all[0].kind, .class, "a notebook from before types was a class")
        XCTAssertEqual(all[0].markers.first?.label, "On the test")
        XCTAssertNil(all[1].recordingKind, "the rest stay nil, which means meeting")
        XCTAssertEqual(all[1].kind, .meeting)
        XCTAssertEqual(all[1].courseName, nil)

        // Once only: a notebook added to a meeting later doesn't make it a class
        all[1].courseName = "Acme project"
        try context.save()
        XCTAssertEqual(RecordingKindBackfill.run(context, defaults: defaults), 0)
        XCTAssertNil(all[1].recordingKind)
        let classes = try ModelContext(container).fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.recordingKind == "class" }))
        XCTAssertEqual(classes.map(\.title), ["Lecture 3"], "saved to the store")
    }
}

/// The SwiftData model as it shipped before recording types (main @
/// 3acc780: `courseName` was the class), trimmed to the models the
/// migration test writes.
enum PreKindSchema {
    static var models: [any PersistentModel.Type] {
        [Meeting.self, Segment.self, Snapshot.self, Person.self, Attendance.self, Redaction.self, MomentMarker.self]
    }

    @Model
    final class Meeting {
        @Attribute(.unique) var id: UUID
        var title: String
        var startedAt: Date
        var endedAt: Date?
        var calendarEventID: String?
        var scheduledStart: Date?
        var scheduledEnd: Date?
        var location: String?
        var meetingURL: String?
        var inviteNotes: String?
        var audioFileName: String?
        var aiNotes: String?
        var aiNotesAt: Date?
        var aiNotesStale: Bool = false
        var source: String?
        var sourceRecordingID: String?
        var importState: String?
        var importError: String?
        var importProgress: Double?
        var audioDuration: Double?
        var sourcePausesJSON: String?
        var courseName: String?
        var plannedMinutes: Int?
        @Relationship(deleteRule: .cascade, inverse: \Segment.meeting) var segments: [Segment] = []
        @Relationship(deleteRule: .cascade, inverse: \Snapshot.meeting) var snapshots: [Snapshot] = []
        @Relationship(deleteRule: .cascade, inverse: \Attendance.meeting) var attendances: [Attendance] = []
        @Relationship(deleteRule: .cascade, inverse: \Redaction.meeting) var redactions: [Redaction] = []
        @Relationship(deleteRule: .cascade, inverse: \MomentMarker.meeting) var markers: [MomentMarker] = []

        init(title: String, startedAt: Date) {
            self.id = UUID()
            self.title = title
            self.startedAt = startedAt
        }
    }

    @Model
    final class MomentMarker {
        @Attribute(.unique) var id: UUID
        var at: Date
        var kind: String
        var note: String?
        var createdAt: Date
        var meeting: Meeting?

        init(at: Date, kind: String) {
            self.id = UUID()
            self.at = at
            self.kind = kind
            self.createdAt = .now
        }
    }

    @Model
    final class Segment {
        var text: String
        var start: Date
        var duration: TimeInterval
        var meeting: Meeting?
        var audioOffset: Double?
        var wordTimingsJSON: String?

        init(text: String, start: Date, duration: TimeInterval) {
            self.text = text
            self.start = start
            self.duration = duration
        }
    }

    @Model
    final class Redaction {
        @Attribute(.unique) var id: UUID
        var kind: String
        var action: String
        var mediaStart: Double
        var mediaEnd: Double
        var coveredFrom: Date?
        var coveredTo: Date?
        var createdAt: Date
        var reason: String?
        var meeting: Meeting?
        var pendingAudioJSON: String?
        var pendingFilesJSON: String?

        init(kind: String, action: String) {
            self.id = UUID()
            self.kind = kind
            self.action = action
            self.mediaStart = 0
            self.mediaEnd = 0
            self.createdAt = .now
        }
    }

    @Model
    final class Snapshot {
        var fileName: String
        var takenAt: Date
        var meeting: Meeting?

        init(fileName: String) {
            self.fileName = fileName
            self.takenAt = .now
        }
    }

    @Model
    final class Person {
        @Attribute(.unique) var email: String
        var name: String?
        var company: String?
        var linkedinURL: String?
        var isSelf: Bool = false
        @Relationship(deleteRule: .cascade, inverse: \Attendance.person) var attendances: [Attendance] = []

        init(email: String) { self.email = email }
    }

    @Model
    final class Attendance {
        var role: String
        var meeting: Meeting?
        var person: Person?

        init(role: String) { self.role = role }
    }
}

/// The SwiftData model as it shipped before timed recording and classes
/// (main @ 46d4af5), to write a store the migration test opens.
enum PreClassesSchema {
    static var models: [any PersistentModel.Type] {
        [Meeting.self, Segment.self, Snapshot.self, Person.self, Attendance.self, Redaction.self]
    }

    @Model
    final class Meeting {
        @Attribute(.unique) var id: UUID
        var title: String
        var startedAt: Date
        var endedAt: Date?
        var calendarEventID: String?
        var scheduledStart: Date?
        var scheduledEnd: Date?
        var location: String?
        var meetingURL: String?
        var inviteNotes: String?
        var audioFileName: String?
        var aiNotes: String?
        var aiNotesAt: Date?
        var aiNotesStale: Bool = false
        var source: String?
        var sourceRecordingID: String?
        var importState: String?
        var importError: String?
        var importProgress: Double?
        var audioDuration: Double?
        var sourcePausesJSON: String?
        @Relationship(deleteRule: .cascade, inverse: \Segment.meeting) var segments: [Segment] = []
        @Relationship(deleteRule: .cascade, inverse: \Snapshot.meeting) var snapshots: [Snapshot] = []
        @Relationship(deleteRule: .cascade, inverse: \Attendance.meeting) var attendances: [Attendance] = []
        @Relationship(deleteRule: .cascade, inverse: \Redaction.meeting) var redactions: [Redaction] = []

        init(title: String, startedAt: Date) {
            self.id = UUID()
            self.title = title
            self.startedAt = startedAt
        }
    }

    @Model
    final class Segment {
        var text: String
        var start: Date
        var duration: TimeInterval
        var meeting: Meeting?
        var audioOffset: Double?
        var wordTimingsJSON: String?

        init(text: String, start: Date, duration: TimeInterval) {
            self.text = text
            self.start = start
            self.duration = duration
        }
    }

    @Model
    final class Redaction {
        @Attribute(.unique) var id: UUID
        var kind: String
        var action: String
        var mediaStart: Double
        var mediaEnd: Double
        var coveredFrom: Date?
        var coveredTo: Date?
        var createdAt: Date
        var reason: String?
        var meeting: Meeting?
        var pendingAudioJSON: String?
        var pendingFilesJSON: String?

        init(kind: String, action: String) {
            self.id = UUID()
            self.kind = kind
            self.action = action
            self.mediaStart = 0
            self.mediaEnd = 0
            self.createdAt = .now
        }
    }

    @Model
    final class Snapshot {
        var fileName: String
        var takenAt: Date
        var meeting: Meeting?

        init(fileName: String) {
            self.fileName = fileName
            self.takenAt = .now
        }
    }

    @Model
    final class Person {
        @Attribute(.unique) var email: String
        var name: String?
        var company: String?
        var linkedinURL: String?
        var isSelf: Bool = false
        @Relationship(deleteRule: .cascade, inverse: \Attendance.person) var attendances: [Attendance] = []

        init(email: String) { self.email = email }
    }

    @Model
    final class Attendance {
        var role: String
        var meeting: Meeting?
        var person: Person?

        init(role: String) { self.role = role }
    }
}
