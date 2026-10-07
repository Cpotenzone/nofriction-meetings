import AVFoundation
import Foundation
import Observation
import SwiftData
import UIKit

/// The live meeting: owns the mic, the transcription engine, the calendar
/// match and the store writes. The UI only reads its published state.
@MainActor
@Observable
final class RecordingSession {
    enum Phase: Equatable { case idle, starting, recording, paused, stopping }

    private(set) var phase: Phase = .idle
    private(set) var meeting: Meeting?
    /// Words being spoken right now (not yet final)
    private(set) var partial = ""
    /// Something the user should know (permission, model download)
    private(set) var notice: String?
    private(set) var level: Float = 0
    private(set) var startedAt: Date?
    private(set) var engineName = ""
    /// Meeting-end countdown in progress ("stopping in 30 s"), for the banner
    private(set) var endCountdown: (reason: MeetingEndDetector.Reason, deadline: Date)?
    /// Time limit of this recording ("how long?"), keyed to its meeting
    private(set) var timeLimit = TimeLimitSlot()
    /// "5 minutes left" is showing (cleared by +15, No limit, OK or stop)
    private(set) var timeWarningVisible = false

    private var context: ModelContext?
    private var capture: AudioCapture?
    private var engine: TranscriptionEngine?
    private var eventsTask: Task<Void, Never>?
    private var meterTask: Task<Void, Never>?
    private var pausedTotal: TimeInterval = 0
    private var pausedAt: Date?
    /// Meeting-end detection (nil when not recording)
    private var detector: MeetingEndDetector?
    private var detectorTask: Task<Void, Never>?
    /// Ticks the time limit every second (recording or paused)
    private var limitTask: Task<Void, Never>?
    private var levels = LevelHistory()
    /// Last kept segment, for the duplicate-filler check
    private var lastKeptText: String?
    /// Set when stopping after a detected end, so the engines' final flush
    /// is filtered as post-meeting audio
    private var endWasDetected = false
    /// Injectable for tests
    var clock: () -> Date = Date.init

    var isActive: Bool { phase == .recording || phase == .paused || phase == .starting }

    func attach(_ context: ModelContext) { self.context = context }

    /// Elapsed recording time, excluding pauses.
    func elapsed(at now: Date = .now) -> TimeInterval {
        guard let startedAt else { return 0 }
        let pausedNow = pausedAt.map { now.timeIntervalSince($0) } ?? 0
        return max(0, now.timeIntervalSince(startedAt) - pausedTotal - pausedNow)
    }

    // MARK: - Start / stop

    /// `limit` / `kind` nil: the remembered choices (starts that skip the
    /// Record sheet). The sheet passes its choices and an optional notebook.
    func start(limit: RecordingLimit? = nil, kind: RecordingKind? = nil, notebook: String? = nil) async {
        guard phase == .idle, let context else { return }
        phase = .starting
        notice = nil
        let limit = limit ?? RecordingLimitStore.remembered()
        let kind = kind ?? RecordingKindStore.remembered()
        let notebook = Notebook.normalize(notebook)

        guard await AudioCapture.requestPermission() else {
            notice = "Microphone access is off. Turn it on in Settings → noFriction."
            phase = .idle
            return
        }

        // Calendar: name the meeting and prime recognition with names
        let now = Date()
        let calendar = CalendarService.shared
        let event = calendar.isAuthorized ? calendar.currentEvent(at: now) : nil
        let title = event?.title ?? notebook.map { Self.notebookTitle($0, at: now) } ?? Self.defaultTitle(for: now, kind: kind)
        let meeting = Meeting(title: title, startedAt: now)
        meeting.courseName = notebook
        meeting.plannedMinutes = limit.minutes
        meeting.kind = kind
        context.insert(meeting)
        if let event { MeetingLinker.link(meeting, to: event, in: context) }

        let audioName = "\(meeting.id.uuidString).m4a"
        meeting.audioFileName = audioName
        try? context.save()

        let capture = AudioCapture()
        let engine = TranscriptionEngines.best()
        do {
            try capture.configureSession()
            let events = try await engine.start(format: capture.inputFormat, vocabulary: Self.vocabulary(for: event))
            capture.onBuffer = { [weak engine] buffer in engine?.append(buffer) }
            try capture.start(recordingTo: Storage.audio.appending(path: audioName))
            self.consume(events)
        } catch {
            notice = error.localizedDescription
            capture.stop()
            // Keep the meeting only if something was captured
            context.delete(meeting)
            try? context.save()
            phase = .idle
            return
        }

        self.capture = capture
        self.engine = engine
        self.meeting = meeting
        self.engineName = engine.name
        self.startedAt = now
        self.pausedTotal = 0
        self.pausedAt = nil
        UIApplication.shared.isIdleTimerDisabled = true
        phase = .recording
        startMeter()
        startEndDetection(scheduledEnd: event?.end)
        startTimeLimit(meeting: meeting, limit: limit, startedAt: now)
    }

    func stop() async {
        guard phase == .recording || phase == .paused else { return }
        phase = .stopping
        meterTask?.cancel()
        detectorTask?.cancel()
        detectorTask = nil
        endWasDetected = detector?.endDetected ?? false
        detector = nil
        endCountdown = nil
        MeetingEndNotifier.shared.clear()
        // The time limit ends with the recording, whatever stopped it
        limitTask?.cancel()
        limitTask = nil
        timeLimit.clear()
        timeWarningVisible = false
        TimeLimitNotifier.cancel()
        capture?.stop()
        await engine?.stop()
        await eventsTask?.value

        if let meeting {
            meeting.endedAt = .now
            // A recording that started before the calendar event was found
            // (e.g. access granted mid-meeting) gets matched now
            if meeting.calendarEventID == nil, CalendarService.shared.isAuthorized, let context {
                let events = CalendarService.shared.events(from: meeting.startedAt.addingTimeInterval(-3600), to: .now.addingTimeInterval(3600))
                if let event = CalendarMatching.bestEvent(start: meeting.startedAt, end: .now, in: events) {
                    MeetingLinker.link(meeting, to: event, in: context)
                }
            }
            try? context?.save()
        }

        UIApplication.shared.isIdleTimerDisabled = false
        capture = nil
        engine = nil
        eventsTask = nil
        partial = ""
        level = 0
        levels = LevelHistory()
        lastKeptText = nil
        endWasDetected = false
        meeting = nil
        startedAt = nil
        phase = .idle
    }

    func togglePause() {
        switch phase {
        case .recording:
            capture?.setPaused(true)
            pausedAt = .now
            partial = ""
            phase = .paused
            // Paused on purpose: no countdown while nothing is being recorded
            apply(detector?.cancelCountdown())
        case .paused:
            capture?.setPaused(false)
            if let pausedAt { pausedTotal += Date().timeIntervalSince(pausedAt) }
            pausedAt = nil
            phase = .recording
            apply(detector?.resetIdle())
        default:
            break
        }
    }

    // MARK: - Snapshots

    /// File a photo (slide, whiteboard, screen) into the live meeting.
    func addSnapshot(_ image: UIImage) {
        guard let meeting, let context else { return }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        let name = "\(UUID().uuidString).jpg"
        do {
            try data.write(to: Storage.snapshots.appending(path: name), options: .atomic)
            let snapshot = Snapshot(fileName: name)
            context.insert(snapshot)
            snapshot.meeting = meeting
            try context.save()
        } catch {
            notice = "Couldn't save the photo — \(error.localizedDescription)"
        }
    }

    // MARK: - Internals

    private func consume(_ events: AsyncStream<TranscriptEvent>) {
        eventsTask = Task { [weak self] in
            for await event in events {
                guard let self else { return }
                switch event {
                case .partial(let text):
                    if self.phase != .paused { self.partial = text }
                case .final(let text, let start, let duration, let audioOffset, let words):
                    self.appendFinal(text: text, start: start, duration: duration, audioOffset: audioOffset, words: words)
                case .status(let message):
                    self.notice = message
                }
            }
        }
    }

    private func appendFinal(text rawText: String, start: Date, duration: TimeInterval, audioOffset: TimeInterval?, words rawWords: [WordTiming]) {
        guard let meeting, let context else { return }
        // Hallucination / repetition filter (both engines arrive here).
        // Near silence: the meter stayed under the speech threshold while
        // this was "heard", or the meeting was already judged over.
        let threshold = detector?.config.speechLevel ?? MeetingEndDetector.Config().speechLevel
        let peak = levels.peak(from: start.addingTimeInterval(-1), to: start.addingTimeInterval(max(duration, 0) + 1))
        let nearSilence = endWasDetected || (detector?.endDetected ?? false) || (peak.map { $0 < threshold } ?? false)
        let text: String, words: [WordTiming]
        switch TranscriptFilter.clean(rawText, words: rawWords, nearSilence: nearSilence, previous: lastKeptText) {
        case .drop:
            return
        case .keep(let cleaned, let cleanedWords, let substantive):
            text = cleaned
            words = cleanedWords
            if substantive { apply(detector?.noteSpeech()) }
        }
        lastKeptText = text
        let segment = Segment(text: text, start: start, duration: duration, audioOffset: audioOffset, wordTimings: words)
        context.insert(segment)
        segment.meeting = meeting
        // Save as we go: a crash or kill mid-meeting loses at most a line
        try? context.save()
    }

    private func startMeter() {
        meterTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                guard let self else { return }
                let target = self.phase == .recording ? (self.capture?.level ?? 0) : 0
                self.level = self.level * 0.6 + target * 0.4
                if self.phase == .recording {
                    let now = self.clock()
                    self.levels.append(target, at: now)
                    self.detector?.noteLevel(target)
                }
            }
        }
    }

    // MARK: - Meeting-end detection

    private func startEndDetection(scheduledEnd: Date?) {
        detector = MeetingEndDetector(config: .load(), scheduledEnd: scheduledEnd, clock: { [weak self] in self?.clock() ?? .now })
        endCountdown = nil
        MeetingEndNotifier.shared.session = self
        if MeetingEndDetector.Config.load().enabled {
            Task { await MeetingEndNotifier.shared.requestAuthorizationIfNeeded() }
        }
        detectorTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled else { return }
                guard self.phase == .recording, self.detector != nil else { continue }
                self.detector?.config = .load()
                // Picks up a calendar match made after the recording started
                if let end = self.meeting?.scheduledEnd { self.detector?.scheduledEnd = end }
                self.apply(self.detector?.tick())
            }
        }
    }

    /// Banner / notification: "Keep recording" — cancel and snooze 10 min.
    func keepRecording() {
        apply(detector?.keepRecording())
    }

    /// Banner / notification: "Stop now".
    func stopNow() {
        apply(detector?.stopNow())
    }

    private func apply(_ action: MeetingEndDetector.Action?) {
        switch action ?? .none {
        case .beginCountdown(let reason, let deadline):
            endCountdown = (reason, deadline)
            if MeetingEndNotifier.shared.isInBackground { MeetingEndNotifier.shared.post(deadline: deadline) }
        case .cancelCountdown:
            endCountdown = nil
            MeetingEndNotifier.shared.clear()
        case .stop:
            endCountdown = nil
            MeetingEndNotifier.shared.clear()
            Task {
                await self.stop()
                self.notice = "Recording stopped — it seemed to have ended. Everything said was saved."
            }
        case .none:
            break
        }
    }

    /// The app left the screen mid-countdown: carry the prompt to a notification.
    func sceneDidChange(active: Bool) {
        guard let endCountdown else { return }
        if active {
            MeetingEndNotifier.shared.clear()
        } else {
            MeetingEndNotifier.shared.post(deadline: endCountdown.deadline)
        }
    }

    // MARK: - Time limit ("how long?")

    /// When the recording stops by itself; nil without a limit.
    var timeDeadline: Date? { timeLimit.plan?.deadline }

    /// Seconds left; nil without a limit.
    func timeRemaining(at now: Date = .now) -> TimeInterval? { timeLimit.plan?.remaining(at: now) }

    private func startTimeLimit(meeting: Meeting, limit: RecordingLimit, startedAt: Date) {
        limitTask?.cancel()
        limitTask = nil
        timeLimit.arm(meetingID: meeting.id, plan: TimeLimitPlan(startedAt: startedAt, limit: limit))
        timeWarningVisible = false
        guard limit.minutes != nil else { return }
        let id = meeting.id
        // The warning notification: ask in context (first timed recording), never at launch
        Task { [weak self] in
            await TimeLimitNotifier.requestAuthorizationIfNeeded()
            await self?.scheduleTimeWarning(for: id)
        }
        limitTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled else { return }
                self.tickTimeLimit(meetingID: id)
            }
        }
    }

    /// One tick of the timer armed for `meetingID`. Wall-clock: it runs
    /// while paused too. At the deadline it stops through `stop()`, the
    /// same path as the Stop button, and only that meeting's recording.
    func tickTimeLimit(meetingID: UUID) {
        guard phase == .recording || phase == .paused, meeting?.id == meetingID else { return }
        switch timeLimit.tick(meetingID: meetingID, now: clock()) {
        case .none:
            break
        case .warn:
            timeWarningVisible = true
        case .stop:
            let planned = timeLimit.plan?.plannedMinutes
            timeWarningVisible = false
            Task {
                guard self.meeting?.id == meetingID else { return }
                await self.stop()
                self.notice = planned.map { "Recording stopped at its \($0)-minute limit. Everything said was saved." }
                    ?? "Recording stopped at its time limit. Everything said was saved."
            }
        }
    }

    /// "+15 min" (screen or notification)
    func extendTimeLimit() {
        guard let meeting, timeLimit.extend(meetingID: meeting.id, now: clock()) else { return }
        meeting.plannedMinutes = timeLimit.plan?.plannedMinutes
        try? context?.save()
        timeWarningVisible = timeLimit.plan?.warned ?? false
        let id = meeting.id
        Task { await scheduleTimeWarning(for: id) }
    }

    /// "No limit" (screen or notification)
    func removeTimeLimit() {
        guard let meeting, timeLimit.removeLimit(meetingID: meeting.id) else { return }
        meeting.plannedMinutes = nil
        try? context?.save()
        timeWarningVisible = false
        TimeLimitNotifier.cancel()
    }

    func dismissTimeWarning() { timeWarningVisible = false }

    private func scheduleTimeWarning(for meetingID: UUID) async {
        guard timeLimit.meetingID == meetingID, let plan = timeLimit.plan, !plan.warned, let meeting else {
            TimeLimitNotifier.cancel()
            return
        }
        await TimeLimitNotifier.schedule(for: plan, title: meeting.title)
    }

    #if DEBUG
    /// Screenshots / layout checks: show `meeting` as if it were being
    /// recorded right now. No mic, no engine; Stop just ends the demo.
    func showDemo(meeting: Meeting, startedAt: Date, partial: String, level: Float = 0.4) {
        guard phase == .idle else { return }
        self.meeting = meeting
        self.startedAt = startedAt
        self.partial = partial
        self.level = level
        self.pausedTotal = 0
        self.pausedAt = nil
        self.engineName = "Demo"
        phase = .recording
    }
    #endif

    /// "Meeting — Oct 7" ("Class — Oct 7", "Personal — Oct 7"): the type
    /// and the date, when no calendar event or notebook names the recording
    /// (same as the Mac). A calendar match found later still renames it
    /// (`isDefaultTitle`).
    static func defaultTitle(for date: Date, kind: RecordingKind = .meeting) -> String {
        "\(kind.label) — " + shortDate(date)
    }

    /// Titles nobody chose: the type-and-date ones, and "Meeting · …" from
    /// builds before types.
    static func isDefaultTitle(_ title: String) -> Bool {
        title.hasPrefix("Meeting · ") || RecordingKind.allCases.contains { title.hasPrefix("\($0.label) — ") }
    }

    /// "BIO 101 — Oct 7": a recording in a notebook with no calendar event
    static func notebookTitle(_ notebook: String, at date: Date) -> String {
        notebook + " — " + shortDate(date)
    }

    private static func shortDate(_ date: Date) -> String {
        date.formatted(.dateTime.month(.abbreviated).day())
    }

    /// Names and companies from the invite help the recognizer spell them.
    static func vocabulary(for event: CalendarEventInfo?) -> [String] {
        guard let event else { return [] }
        var words: [String] = []
        for p in event.participants where !p.isSelf {
            if let name = p.name { words.append(name) }
            if let company = PersonNames.company(fromEmail: p.email) { words.append(company) }
        }
        return Array(Set(words)).sorted()
    }
}

/// Attaches calendar details and attendees to a meeting.
enum MeetingLinker {
    @MainActor
    static func link(_ meeting: Meeting, to event: CalendarEventInfo, in context: ModelContext) {
        meeting.calendarEventID = event.id
        meeting.scheduledStart = event.start
        meeting.scheduledEnd = event.end
        meeting.location = event.location.flatMap { $0.hasPrefix("http") ? nil : $0 }
        meeting.meetingURL = event.meetingURL
        meeting.inviteNotes = event.notes
        if RecordingSession.isDefaultTitle(meeting.title) { meeting.title = event.title }

        for p in event.participants {
            let person = findOrCreatePerson(email: p.email, in: context)
            if let name = p.name { person.name = name }        // invite name beats a guess
            if person.company == nil { person.company = PersonNames.company(fromEmail: p.email) }
            person.isSelf = person.isSelf || p.isSelf

            let role = p.isOrganizer ? "organizer" : "attendee"
            if let existing = meeting.attendances.first(where: { $0.person?.email == person.email }) {
                existing.role = role
            } else {
                let attendance = Attendance(role: role)
                context.insert(attendance)
                attendance.meeting = meeting
                attendance.person = person
            }
        }
    }

    @MainActor
    static func findOrCreatePerson(email: String, in context: ModelContext) -> Person {
        let key = email.lowercased()
        let descriptor = FetchDescriptor<Person>(predicate: #Predicate { $0.email == key })
        if let existing = try? context.fetch(descriptor).first { return existing }
        let person = Person(email: key)
        context.insert(person)
        return person
    }

    /// Link every unlinked meeting (e.g. right after calendar access is
    /// granted). Returns how many were linked.
    @MainActor
    static func backfill(in context: ModelContext) -> Int {
        let calendar = CalendarService.shared
        guard calendar.isAuthorized else { return 0 }
        let meetings = (try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.calendarEventID == nil }))) ?? []
        guard let lo = meetings.map(\.startedAt).min(), let hi = meetings.map({ $0.endedAt ?? $0.startedAt }).max() else { return 0 }
        let events = calendar.events(from: lo.addingTimeInterval(-86_400), to: hi.addingTimeInterval(86_400))
        var linked = 0
        for m in meetings {
            let end = m.endedAt ?? m.startedAt.addingTimeInterval(1800)
            if let event = CalendarMatching.bestEvent(start: m.startedAt, end: end, in: events) {
                link(m, to: event, in: context)
                linked += 1
            }
        }
        try? context.save()
        return linked
    }
}
